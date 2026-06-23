//! DBPX core library: deterministic lossless raster encoding for DBYTE tools.

use std::fmt;

pub const MAGIC: [u8; 8] = *b"DBPX\r\n\x1A\n";
pub const HEADER_LEN: usize = 28;
pub const MAX_PIXELS: u64 = 67_108_864;

const PXLS: [u8; 4] = *b"PXLS";
const END: [u8; 4] = *b"END!";
const CHUNK_HEAD: usize = 16;
const MAX_INDEXED_COLORS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorType(u8);

impl ColorType {
    pub const GRAY8: Self = Self(1);
    pub const RGB8: Self = Self(2);
    pub const RGBA8: Self = Self(3);

    pub fn from_id(id: u8) -> Result<Self, DbpxError> {
        match id {
            1 | 2 | 3 => Ok(Self(id)),
            _ => Err(DbpxError::new(format!("bad color type {id}"))),
        }
    }

    pub fn id(self) -> u8 {
        self.0
    }

    pub fn channels(self) -> usize {
        match self.0 {
            1 => 1,
            2 => 3,
            3 => 4,
            _ => unreachable!(),
        }
    }

    pub fn name(self) -> &'static str {
        match self.0 {
            1 => "GRAY8",
            2 => "RGB8",
            3 => "RGBA8",
            _ => "INVALID",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compression(u8);

impl Compression {
    pub const RAW: Self = Self(0);
    pub const RLE: Self = Self(1);
    pub const INDEXED: Self = Self(2);

    pub fn from_id(id: u8) -> Result<Self, DbpxError> {
        match id {
            0 | 1 | 2 => Ok(Self(id)),
            _ => Err(DbpxError::new(format!("bad compression {id}"))),
        }
    }

    pub fn id(self) -> u8 {
        self.0
    }

    pub fn name(self) -> &'static str {
        match self.0 {
            0 => "raw",
            1 => "dbpx-rle",
            2 => "dbpx-indexed",
            _ => "INVALID",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub color: ColorType,
    pub pixels: Vec<u8>,
}

impl Image {
    pub fn new(
        width: u32,
        height: u32,
        color: ColorType,
        pixels: Vec<u8>,
    ) -> Result<Self, DbpxError> {
        check_shape(width, height, color, pixels.len())?;
        Ok(Self {
            width,
            height,
            color,
            pixels,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Info {
    pub width: u32,
    pub height: u32,
    pub color: ColorType,
    pub compression: Compression,
    pub pixel_payload_len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbpxError {
    message: String,
}

impl DbpxError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for DbpxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DbpxError {}

pub fn encode(img: &Image, comp: Compression) -> Result<Vec<u8>, DbpxError> {
    check_shape(img.width, img.height, img.color, img.pixels.len())?;
    let payload = if comp == Compression::RAW {
        img.pixels.clone()
    } else if comp == Compression::RLE {
        encode_rle(img)?
    } else if comp == Compression::INDEXED {
        encode_indexed(img)?
    } else {
        return fail(format!("bad compression {}", comp.id()));
    };
    let mut out = Vec::with_capacity(HEADER_LEN + CHUNK_HEAD * 2 + payload.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&[0, 1]);
    out.extend_from_slice(&(HEADER_LEN as u16).to_le_bytes());
    out.extend_from_slice(&img.width.to_le_bytes());
    out.extend_from_slice(&img.height.to_le_bytes());
    out.extend_from_slice(&[img.color.id(), 8, comp.id(), 0]);
    out.extend_from_slice(&2u32.to_le_bytes());
    chunk(&mut out, PXLS, &payload);
    chunk(&mut out, END, &[]);
    Ok(out)
}

pub fn encode_auto(img: &Image) -> Result<Vec<u8>, DbpxError> {
    let mut best = encode(img, Compression::RAW)?;
    let rle = encode(img, Compression::RLE)?;
    if rle.len() < best.len() {
        best = rle;
    }
    if let Ok(indexed) = encode(img, Compression::INDEXED) {
        if indexed.len() < best.len() {
            best = indexed;
        }
    }
    Ok(best)
}

pub fn decode(data: &[u8]) -> Result<Image, DbpxError> {
    let (meta, payload) = parse(data)?;
    let pixels = if meta.compression == Compression::RAW {
        raw(&meta, payload)?
    } else if meta.compression == Compression::RLE {
        decode_rle(&meta, payload)?
    } else if meta.compression == Compression::INDEXED {
        decode_indexed(&meta, payload)?
    } else {
        return fail(format!("bad compression {}", meta.compression.id()));
    };
    Image::new(meta.width, meta.height, meta.color, pixels)
}

pub fn info(data: &[u8]) -> Result<Info, DbpxError> {
    Ok(parse(data)?.0)
}

pub fn rgb_bytes(img: &Image) -> Vec<u8> {
    let mut out = Vec::with_capacity(img.width as usize * img.height as usize * 3);
    if img.color == ColorType::GRAY8 {
        for &g in &img.pixels {
            out.extend_from_slice(&[g, g, g]);
        }
    } else if img.color == ColorType::RGB8 {
        out.extend_from_slice(&img.pixels);
    } else {
        for p in img.pixels.chunks_exact(4) {
            out.extend_from_slice(&p[..3]);
        }
    }
    out
}

fn parse(data: &[u8]) -> Result<(Info, &[u8]), DbpxError> {
    if data.len() < HEADER_LEN {
        return fail("truncated header");
    }
    if data[..8] != MAGIC {
        return fail("bad DBPX magic");
    }
    if data[8] != 0 {
        return fail(format!("unsupported DBPX version {}.{}", data[8], data[9]));
    }
    let header_size = u16le(data, 10)?;
    if header_size as usize != HEADER_LEN {
        return fail(format!("bad header size {header_size}"));
    }
    let width = u32le(data, 12)?;
    let height = u32le(data, 16)?;
    let color = ColorType::from_id(data[20])?;
    if data[21] != 8 {
        return fail(format!("bad bit depth {}", data[21]));
    }
    let compression = Compression::from_id(data[22])?;
    if data[23] != 0 {
        return fail(format!("reserved flags set: 0x{:02X}", data[23]));
    }
    check_dims(width, height)?;
    let expected_chunks = u32le(data, 24)?;
    let mut off = HEADER_LEN;
    let mut seen = 0u32;
    let mut payload = None;
    let mut ended = false;

    while off < data.len() {
        if off + CHUNK_HEAD > data.len() {
            return fail("truncated chunk header");
        }
        let typ = arr4(data, off)?;
        off += 4;
        let len64 = u64le(data, off)?;
        off += 8;
        let len =
            usize::try_from(len64).map_err(|_| DbpxError::new("chunk length overflows usize"))?;
        let want = u32le(data, off)?;
        off += 4;
        let end = off
            .checked_add(len)
            .ok_or_else(|| DbpxError::new("chunk length overflow"))?;
        if end > data.len() {
            return fail("truncated chunk data");
        }
        let body = &data[off..end];
        off = end;
        let got = crc_chunk(typ, body);
        if got != want {
            return fail(format!(
                "CRC mismatch for {}: expected 0x{want:08X}, got 0x{got:08X}",
                name(typ)
            ));
        }
        seen = seen
            .checked_add(1)
            .ok_or_else(|| DbpxError::new("chunk count overflow"))?;
        if typ == PXLS {
            if payload.is_some() {
                return fail("duplicate PXLS chunk");
            }
            payload = Some(body);
        } else if typ == END {
            ended = true;
            break;
        } else {
            return fail(format!("unknown chunk {}", name(typ)));
        }
    }

    if !ended {
        return fail("missing END! chunk");
    }
    let payload = payload.ok_or_else(|| DbpxError::new("missing PXLS chunk"))?;
    if seen != expected_chunks {
        return fail(format!(
            "chunk count mismatch: expected {expected_chunks}, got {seen}"
        ));
    }
    if off != data.len() {
        return fail(format!(
            "trailing data after END!: {} bytes",
            data.len() - off
        ));
    }
    Ok((
        Info {
            width,
            height,
            color,
            compression,
            pixel_payload_len: payload.len() as u64,
        },
        payload,
    ))
}

fn raw(meta: &Info, payload: &[u8]) -> Result<Vec<u8>, DbpxError> {
    let expected = expected_len(meta.width, meta.height, meta.color)?;
    if payload.len() != expected {
        return fail(format!(
            "bad pixel length: expected {expected}, got {}",
            payload.len()
        ));
    }
    Ok(payload.to_vec())
}

fn encode_rle(img: &Image) -> Result<Vec<u8>, DbpxError> {
    let total = pixels(img.width, img.height)? as usize;
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < total {
        let px = rgba(img, i);
        let mut run = 1usize;
        while run < 255 && i + run < total && rgba(img, i + run) == px {
            run += 1;
        }
        out.push(run as u8);
        out.extend_from_slice(&px);
        i += run;
    }
    Ok(out)
}

fn decode_rle(meta: &Info, payload: &[u8]) -> Result<Vec<u8>, DbpxError> {
    if payload.len() % 5 != 0 {
        return fail("bad RLE payload length");
    }
    let total = pixels(meta.width, meta.height)? as usize;
    let mut rgba_out = Vec::with_capacity(total * 4);
    let mut i = 0usize;
    while i < payload.len() {
        let run = payload[i] as usize;
        i += 1;
        if run == 0 {
            return fail("zero-length RLE run");
        }
        let px = &payload[i..i + 4];
        i += 4;
        if rgba_out.len() / 4 + run > total {
            return fail("RLE run exceeds pixel count");
        }
        for _ in 0..run {
            rgba_out.extend_from_slice(px);
        }
    }
    if rgba_out.len() / 4 != total {
        return fail("RLE decoded pixel count mismatch");
    }
    Ok(pack(&rgba_out, meta.color))
}

fn encode_indexed(img: &Image) -> Result<Vec<u8>, DbpxError> {
    let total = pixels(img.width, img.height)? as usize;
    let mut palette = Vec::<[u8; 4]>::new();
    let mut indices = Vec::<u8>::with_capacity(total);

    for i in 0..total {
        let px = rgba(img, i);
        let index = match palette.iter().position(|&seen| seen == px) {
            Some(index) => index,
            None => {
                if palette.len() == MAX_INDEXED_COLORS {
                    return fail("indexed compression supports at most 256 colors");
                }
                palette.push(px);
                palette.len() - 1
            }
        };
        indices.push(index as u8);
    }

    if palette.is_empty() {
        return fail("indexed compression requires at least one color");
    }

    let mut out = Vec::with_capacity(1 + palette.len() * 4 + indices.len());
    out.push((palette.len() - 1) as u8);
    for px in &palette {
        out.extend_from_slice(px);
    }
    out.extend_from_slice(&indices);
    Ok(out)
}

fn decode_indexed(meta: &Info, payload: &[u8]) -> Result<Vec<u8>, DbpxError> {
    if payload.is_empty() {
        return fail("truncated indexed payload");
    }
    let colors = payload[0] as usize + 1;
    let palette_bytes = colors
        .checked_mul(4)
        .ok_or_else(|| DbpxError::new("indexed palette size overflow"))?;
    let indices_start = 1usize
        .checked_add(palette_bytes)
        .ok_or_else(|| DbpxError::new("indexed payload offset overflow"))?;
    if payload.len() < indices_start {
        return fail("truncated indexed palette");
    }
    let total = pixels(meta.width, meta.height)? as usize;
    let index_bytes = payload.len() - indices_start;
    if index_bytes != total {
        return fail(format!(
            "bad indexed pixel count: expected {total}, got {index_bytes}"
        ));
    }

    let palette = &payload[1..indices_start];
    let indices = &payload[indices_start..];
    let mut rgba_out = Vec::with_capacity(total * 4);
    for &index in indices {
        if index as usize >= colors {
            return fail(format!("indexed pixel uses missing palette entry {index}"));
        }
        let at = index as usize * 4;
        rgba_out.extend_from_slice(&palette[at..at + 4]);
    }
    Ok(pack(&rgba_out, meta.color))
}

fn rgba(img: &Image, i: usize) -> [u8; 4] {
    if img.color == ColorType::GRAY8 {
        let g = img.pixels[i];
        [g, g, g, 255]
    } else if img.color == ColorType::RGB8 {
        let b = i * 3;
        [img.pixels[b], img.pixels[b + 1], img.pixels[b + 2], 255]
    } else {
        let b = i * 4;
        [
            img.pixels[b],
            img.pixels[b + 1],
            img.pixels[b + 2],
            img.pixels[b + 3],
        ]
    }
}

fn pack(rgba: &[u8], color: ColorType) -> Vec<u8> {
    if color == ColorType::GRAY8 {
        rgba.chunks_exact(4).map(|p| p[0]).collect()
    } else if color == ColorType::RGB8 {
        let mut out = Vec::with_capacity(rgba.len() / 4 * 3);
        for p in rgba.chunks_exact(4) {
            out.extend_from_slice(&p[..3]);
        }
        out
    } else {
        rgba.to_vec()
    }
}

fn chunk(out: &mut Vec<u8>, typ: [u8; 4], body: &[u8]) {
    out.extend_from_slice(&typ);
    out.extend_from_slice(&(body.len() as u64).to_le_bytes());
    out.extend_from_slice(&crc_chunk(typ, body).to_le_bytes());
    out.extend_from_slice(body);
}

fn check_shape(w: u32, h: u32, c: ColorType, got: usize) -> Result<(), DbpxError> {
    check_dims(w, h)?;
    let want = expected_len(w, h, c)?;
    if got != want {
        fail(format!("bad pixel length: expected {want}, got {got}"))
    } else {
        Ok(())
    }
}

fn check_dims(w: u32, h: u32) -> Result<(), DbpxError> {
    if w == 0 || h == 0 || w > 65_535 || h > 65_535 {
        return fail("dimensions must be non-zero and <= 65535");
    }
    let n = pixels(w, h)?;
    if n > MAX_PIXELS {
        return fail(format!("pixel count exceeds limit: {n}"));
    }
    Ok(())
}

fn pixels(w: u32, h: u32) -> Result<u64, DbpxError> {
    (w as u64)
        .checked_mul(h as u64)
        .ok_or_else(|| DbpxError::new("pixel count overflow"))
}

fn expected_len(w: u32, h: u32, c: ColorType) -> Result<usize, DbpxError> {
    usize::try_from(
        pixels(w, h)?
            .checked_mul(c.channels() as u64)
            .ok_or_else(|| DbpxError::new("pixel byte count overflow"))?,
    )
    .map_err(|_| DbpxError::new("pixel byte count overflows usize"))
}

fn arr4(data: &[u8], at: usize) -> Result<[u8; 4], DbpxError> {
    let end = at
        .checked_add(4)
        .ok_or_else(|| DbpxError::new("array4 offset overflow"))?;
    Ok(data
        .get(at..end)
        .ok_or_else(|| DbpxError::new("truncated array4"))?
        .try_into()
        .expect("length checked"))
}

fn u16le(data: &[u8], at: usize) -> Result<u16, DbpxError> {
    let end = at
        .checked_add(2)
        .ok_or_else(|| DbpxError::new("u16 offset overflow"))?;
    Ok(u16::from_le_bytes(
        data.get(at..end)
            .ok_or_else(|| DbpxError::new("truncated u16"))?
            .try_into()
            .expect("length checked"),
    ))
}

fn u32le(data: &[u8], at: usize) -> Result<u32, DbpxError> {
    let end = at
        .checked_add(4)
        .ok_or_else(|| DbpxError::new("u32 offset overflow"))?;
    Ok(u32::from_le_bytes(
        data.get(at..end)
            .ok_or_else(|| DbpxError::new("truncated u32"))?
            .try_into()
            .expect("length checked"),
    ))
}

fn u64le(data: &[u8], at: usize) -> Result<u64, DbpxError> {
    let end = at
        .checked_add(8)
        .ok_or_else(|| DbpxError::new("u64 offset overflow"))?;
    Ok(u64::from_le_bytes(
        data.get(at..end)
            .ok_or_else(|| DbpxError::new("truncated u64"))?
            .try_into()
            .expect("length checked"),
    ))
}

fn name(c: [u8; 4]) -> String {
    c.iter()
        .map(|&b| if b.is_ascii_graphic() { b as char } else { '.' })
        .collect()
}

fn crc_chunk(t: [u8; 4], b: &[u8]) -> u32 {
    let mut c = Crc32(0xFFFF_FFFF);
    c.update(&t);
    c.update(b);
    c.finish()
}

fn fail<T>(message: impl Into<String>) -> Result<T, DbpxError> {
    Err(DbpxError::new(message))
}

struct Crc32(u32);

impl Crc32 {
    fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u32;
            for _ in 0..8 {
                let m = 0u32.wrapping_sub(self.0 & 1);
                self.0 = (self.0 >> 1) ^ (0xEDB8_8320 & m);
            }
        }
    }

    fn finish(self) -> u32 {
        !self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_rle_rgba() {
        let img = Image::new(
            3,
            1,
            ColorType::RGBA8,
            vec![1, 2, 3, 255, 1, 2, 3, 255, 4, 5, 6, 7],
        )
        .unwrap();
        assert_eq!(
            decode(&encode(&img, Compression::RLE).unwrap()).unwrap(),
            img
        );
    }

    #[test]
    fn roundtrip_raw_rgb() {
        let img = Image::new(2, 1, ColorType::RGB8, vec![1, 2, 3, 4, 5, 6]).unwrap();
        assert_eq!(
            decode(&encode(&img, Compression::RAW).unwrap()).unwrap(),
            img
        );
    }

    #[test]
    fn roundtrip_indexed_rgb() {
        let img = Image::new(
            4,
            1,
            ColorType::RGB8,
            vec![1, 2, 3, 4, 5, 6, 1, 2, 3, 4, 5, 6],
        )
        .unwrap();
        assert_eq!(
            decode(&encode(&img, Compression::INDEXED).unwrap()).unwrap(),
            img
        );
    }

    #[test]
    fn encode_auto_prefers_raw_when_rle_and_indexed_are_larger() {
        let mut pixels = Vec::new();
        for i in 0..300u16 {
            pixels.extend_from_slice(&[(i & 0xFF) as u8, (i >> 8) as u8, 0]);
        }
        let img = Image::new(300, 1, ColorType::RGB8, pixels).unwrap();
        let encoded = encode_auto(&img).unwrap();
        assert_eq!(info(&encoded).unwrap().compression, Compression::RAW);
        assert_eq!(decode(&encoded).unwrap(), img);
    }

    #[test]
    fn encode_auto_prefers_rle_when_rle_is_smaller() {
        let img = Image::new(
            8,
            1,
            ColorType::RGB8,
            vec![
                9, 8, 7, 9, 8, 7, 9, 8, 7, 9, 8, 7, 9, 8, 7, 9, 8, 7, 9, 8, 7, 9, 8, 7,
            ],
        )
        .unwrap();
        let encoded = encode_auto(&img).unwrap();
        assert_eq!(info(&encoded).unwrap().compression, Compression::RLE);
        assert_eq!(decode(&encoded).unwrap(), img);
    }

    #[test]
    fn encode_auto_prefers_indexed_when_palette_is_smaller() {
        let mut pixels = Vec::new();
        for i in 0..64u8 {
            let color = if i % 2 == 0 {
                [10, 20, 30]
            } else {
                [40, 50, 60]
            };
            pixels.extend_from_slice(&color);
        }
        let img = Image::new(64, 1, ColorType::RGB8, pixels).unwrap();
        let encoded = encode_auto(&img).unwrap();
        assert_eq!(info(&encoded).unwrap().compression, Compression::INDEXED);
        assert_eq!(decode(&encoded).unwrap(), img);
    }

    #[test]
    fn indexed_rejects_too_many_colors() {
        let mut pixels = Vec::new();
        for i in 0..257u16 {
            pixels.extend_from_slice(&[(i & 0xFF) as u8, (i >> 8) as u8, 1]);
        }
        let img = Image::new(257, 1, ColorType::RGB8, pixels).unwrap();
        let err = encode(&img, Compression::INDEXED).unwrap_err().to_string();
        assert!(err.contains("at most 256 colors"));
    }

    #[test]
    fn rejects_crc_mismatch() {
        let img = Image::new(1, 1, ColorType::RGB8, vec![1, 2, 3]).unwrap();
        let mut f = encode(&img, Compression::RAW).unwrap();
        f[HEADER_LEN + CHUNK_HEAD] ^= 1;
        assert!(decode(&f).unwrap_err().to_string().contains("CRC mismatch"));
    }
}
