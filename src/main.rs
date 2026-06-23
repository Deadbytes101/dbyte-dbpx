use dbpx::{
    decode, encode, encode_auto, info, rgb_bytes, ColorType, Compression, Image, HEADER_LEN,
};
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::hint::black_box;
use std::io::Write;
use std::path::Path;
use std::process;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const VERSION: &str = env!("CARGO_PKG_VERSION");
type AnyError = Box<dyn Error>;

#[derive(Debug)]
struct CliError(String);

#[derive(Debug)]
struct DumpChunk {
    kind: [u8; 4],
    len: u64,
    crc: u32,
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for CliError {}

fn cli_error(message: impl Into<String>) -> CliError {
    CliError(message.into())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), AnyError> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("info") => command_info(&args[2..]),
        Some("check") => command_check(&args[2..]),
        Some("dump") => command_dump(&args[2..]),
        Some("bench") => command_bench(&args[2..]),
        Some("enc-ppm") => command_encode_ppm(&args[2..]),
        Some("dec-ppm") => command_decode_ppm(&args[2..]),
        Some("dec-bmp") => command_decode_bmp(&args[2..]),
        Some("view") => command_view(&args[2..]),
        Some("make-demo") => command_make_demo(&args[2..]),
        Some("version") | Some("--version") | Some("-V") => {
            println!("dbpx {VERSION}");
            Ok(())
        }
        Some("help") | Some("--help") | Some("-h") | None => {
            usage();
            Ok(())
        }
        Some(other) => fail(format!("unknown command: {other}")),
    }
}

fn command_info(args: &[String]) -> Result<(), AnyError> {
    let input = required(args, 0, "input.dbpx")?;
    let data = fs::read(input)?;
    let meta = info(&data)?;
    println!("format: DBPX 0.1");
    println!("size: {}x{}", meta.width, meta.height);
    println!("color: {}", meta.color.name());
    println!("compression: {}", meta.compression.name());
    println!("pixel-payload-bytes: {}", meta.pixel_payload_len);
    println!("file-bytes: {}", data.len());
    Ok(())
}

fn command_check(args: &[String]) -> Result<(), AnyError> {
    let input = required(args, 0, "input.dbpx")?;
    let image = decode(&fs::read(input)?)?;
    println!(
        "ok: {}x{} {} ({} bytes decoded)",
        image.width,
        image.height,
        image.color.name(),
        image.pixels.len()
    );
    Ok(())
}

fn command_dump(args: &[String]) -> Result<(), AnyError> {
    let input = required(args, 0, "input.dbpx")?;
    let data = fs::read(input)?;
    let meta = info(&data)?;
    let chunks = dump_chunks(&data)?;

    println!("format: DBPX 0.1");
    println!("header-bytes: {HEADER_LEN}");
    println!("size: {}x{}", meta.width, meta.height);
    println!("color: {}", meta.color.name());
    println!("compression: {}", meta.compression.name());
    println!("file-bytes: {}", data.len());
    println!("chunks: {}", chunks.len());
    for (index, chunk) in chunks.iter().enumerate() {
        println!(
            "  {index}: {} len={} crc=0x{:08X}",
            chunk_name(chunk.kind),
            chunk.len,
            chunk.crc
        );
    }
    Ok(())
}

fn command_bench(args: &[String]) -> Result<(), AnyError> {
    let width = optional_u32(args.get(0), 320, "width")?;
    let height = optional_u32(args.get(1), 200, "height")?;
    let iterations = optional_usize(args.get(2), 64, "iterations")?;
    if iterations == 0 {
        return fail("iterations must be non-zero");
    }

    let image = demo(width, height)?;
    let raw_once = encode(&image, Compression::RAW)?;
    let rle_once = encode(&image, Compression::RLE)?;
    let indexed_once = encode(&image, Compression::INDEXED).ok();
    let auto_once = encode_auto(&image)?;
    let auto_meta = info(&auto_once)?;

    let mut sink = 0usize;

    let start = Instant::now();
    for _ in 0..iterations {
        let encoded = encode(black_box(&image), Compression::RAW)?;
        sink ^= black_box(encoded.len());
    }
    let encode_raw_us = start.elapsed().as_micros();

    let start = Instant::now();
    for _ in 0..iterations {
        let encoded = encode(black_box(&image), Compression::RLE)?;
        sink ^= black_box(encoded.len());
    }
    let encode_rle_us = start.elapsed().as_micros();

    let encode_indexed_us = if indexed_once.is_some() {
        let start = Instant::now();
        for _ in 0..iterations {
            let encoded = encode(black_box(&image), Compression::INDEXED)?;
            sink ^= black_box(encoded.len());
        }
        Some(start.elapsed().as_micros())
    } else {
        None
    };

    let start = Instant::now();
    for _ in 0..iterations {
        let decoded = decode(black_box(&auto_once))?;
        sink ^= black_box(decoded.pixels.len());
    }
    let decode_auto_us = start.elapsed().as_micros();

    black_box(sink);

    println!("bench: {}x{} RGB8", width, height);
    println!("iterations: {iterations}");
    println!("raw-file-bytes: {}", raw_once.len());
    println!("rle-file-bytes: {}", rle_once.len());
    match &indexed_once {
        Some(indexed) => println!("indexed-file-bytes: {}", indexed.len()),
        None => println!("indexed-file-bytes: n/a"),
    }
    println!("auto-file-bytes: {}", auto_once.len());
    println!("auto-compression: {}", auto_meta.compression.name());
    println!("encode-raw-us: {encode_raw_us}");
    println!("encode-rle-us: {encode_rle_us}");
    match encode_indexed_us {
        Some(us) => println!("encode-indexed-us: {us}"),
        None => println!("encode-indexed-us: n/a"),
    }
    println!("decode-auto-us: {decode_auto_us}");
    Ok(())
}

fn command_encode_ppm(args: &[String]) -> Result<(), AnyError> {
    let input = required(args, 0, "input.ppm")?;
    let output = required(args, 1, "output.dbpx")?;
    let image = parse_ppm(&fs::read(input)?)?;
    fs::write(output, encode_from_flags(&image, &args[2..])?)?;
    Ok(())
}

fn command_decode_ppm(args: &[String]) -> Result<(), AnyError> {
    let input = required(args, 0, "input.dbpx")?;
    let output = required(args, 1, "output.ppm")?;
    let image = decode(&fs::read(input)?)?;
    write_ppm(output, &image)?;
    Ok(())
}

fn command_decode_bmp(args: &[String]) -> Result<(), AnyError> {
    let input = required(args, 0, "input.dbpx")?;
    let output = required(args, 1, "output.bmp")?;
    let image = decode(&fs::read(input)?)?;
    write_bmp(output, &image)?;
    Ok(())
}

fn command_view(args: &[String]) -> Result<(), AnyError> {
    let input = required(args, 0, "input.dbpx")?;
    let image = decode(&fs::read(input)?)?;
    let path = temp_bmp_path()?;
    write_bmp(&path, &image)?;
    open_path(&path)?;
    println!("opened: {}", path.display());
    Ok(())
}

fn command_make_demo(args: &[String]) -> Result<(), AnyError> {
    let output = required(args, 0, "output.dbpx")?;
    let width = optional_u32(args.get(1), 320, "width")?;
    let height = optional_u32(args.get(2), 200, "height")?;
    let image = demo(width, height)?;
    let flags = if args.len() > 3 { &args[3..] } else { &[] };
    fs::write(output, encode_from_flags(&image, flags)?)?;
    Ok(())
}

fn encode_from_flags(image: &Image, flags: &[String]) -> Result<Vec<u8>, AnyError> {
    match encoder_mode(flags)? {
        Some(comp) => Ok(encode(image, comp)?),
        None => Ok(encode_auto(image)?),
    }
}

fn encoder_mode(flags: &[String]) -> Result<Option<Compression>, AnyError> {
    let mut mode = None;
    for flag in flags {
        let next = match flag.as_str() {
            "--raw" => Compression::RAW,
            "--rle" => Compression::RLE,
            "--indexed" => Compression::INDEXED,
            _ => return fail(format!("unknown encoder flag: {flag}")),
        };
        if mode.is_some() && mode != Some(next) {
            return fail("cannot use multiple encoder modes together");
        }
        mode = Some(next);
    }
    Ok(mode)
}

fn demo(width: u32, height: u32) -> Result<Image, AnyError> {
    if width == 0 || height == 0 {
        return fail("demo dimensions must be non-zero");
    }
    let total = (width as u64)
        .checked_mul(height as u64)
        .ok_or_else(|| cli_error("demo dimensions overflow"))?;
    if total > dbpx::MAX_PIXELS {
        return fail(format!("demo dimensions exceed pixel limit: {total}"));
    }
    let mut pixels = Vec::with_capacity((total * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let checker = (((x / 16) + (y / 16)) & 1) as u8;
            let r = ((x * 255) / width.max(1)) as u8;
            let g = ((y * 255) / height.max(1)) as u8;
            let b = if checker == 0 { 32 } else { 220 };
            pixels.extend_from_slice(&[r, g, b]);
        }
    }
    Ok(Image::new(width, height, ColorType::RGB8, pixels)?)
}

fn parse_ppm(data: &[u8]) -> Result<Image, AnyError> {
    let mut pos = 0usize;
    let magic = token(data, &mut pos).ok_or_else(|| cli_error("missing PPM magic"))?;
    if magic != "P6" {
        return fail("only binary P6 PPM is supported");
    }
    let width = parse_u32_token(data, &mut pos, "width")?;
    let height = parse_u32_token(data, &mut pos, "height")?;
    let max = parse_u32_token(data, &mut pos, "max value")?;
    if max != 255 {
        return fail("only PPM max value 255 is supported");
    }
    if pos >= data.len() || !data[pos].is_ascii_whitespace() {
        return fail("missing whitespace before PPM pixels");
    }
    pos += 1;
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(3))
        .ok_or_else(|| cli_error("PPM size overflow"))?;
    let actual = data.len().saturating_sub(pos);
    if actual != expected {
        return fail(format!(
            "PPM pixel length mismatch: expected {expected}, got {actual}"
        ));
    }
    Ok(Image::new(
        width,
        height,
        ColorType::RGB8,
        data[pos..].to_vec(),
    )?)
}

fn dump_chunks(data: &[u8]) -> Result<Vec<DumpChunk>, AnyError> {
    let mut off = HEADER_LEN;
    let mut chunks = Vec::new();
    while off < data.len() {
        if off + 16 > data.len() {
            return fail("truncated chunk header");
        }
        let kind = arr4(data, off)?;
        let len = u64le(data, off + 4)?;
        let crc = u32le(data, off + 12)?;
        let body_start = off + 16;
        let body_len =
            usize::try_from(len).map_err(|_| cli_error("chunk length overflows usize"))?;
        let body_end = body_start
            .checked_add(body_len)
            .ok_or_else(|| cli_error("chunk length overflow"))?;
        if body_end > data.len() {
            return fail("truncated chunk data");
        }
        chunks.push(DumpChunk { kind, len, crc });
        off = body_end;
        if kind == *b"END!" {
            break;
        }
    }
    Ok(chunks)
}

fn parse_u32_token(data: &[u8], pos: &mut usize, name: &'static str) -> Result<u32, AnyError> {
    let raw = token(data, pos).ok_or_else(|| cli_error(format!("missing PPM {name}")))?;
    Ok(raw
        .parse::<u32>()
        .map_err(|_| cli_error(format!("invalid PPM {name}: {raw}")))?)
}

fn token(data: &[u8], pos: &mut usize) -> Option<String> {
    loop {
        while *pos < data.len() && data[*pos].is_ascii_whitespace() {
            *pos += 1;
        }
        if *pos < data.len() && data[*pos] == b'#' {
            while *pos < data.len() && data[*pos] != b'\n' {
                *pos += 1;
            }
            continue;
        }
        break;
    }
    if *pos >= data.len() {
        return None;
    }
    let start = *pos;
    while *pos < data.len() && !data[*pos].is_ascii_whitespace() {
        *pos += 1;
    }
    Some(String::from_utf8_lossy(&data[start..*pos]).into_owned())
}

fn write_ppm(path: impl AsRef<Path>, image: &Image) -> Result<(), AnyError> {
    let mut file = fs::File::create(path)?;
    write!(file, "P6\n{} {}\n255\n", image.width, image.height)?;
    file.write_all(&rgb_bytes(image))?;
    Ok(())
}

fn write_bmp(path: impl AsRef<Path>, image: &Image) -> Result<(), AnyError> {
    let width = usize::try_from(image.width).map_err(|_| cli_error("BMP width overflows usize"))?;
    let height =
        usize::try_from(image.height).map_err(|_| cli_error("BMP height overflows usize"))?;
    let row_bytes = width
        .checked_mul(3)
        .ok_or_else(|| cli_error("BMP row size overflow"))?;
    let padding = (4 - (row_bytes % 4)) % 4;
    let stride = row_bytes
        .checked_add(padding)
        .ok_or_else(|| cli_error("BMP stride overflow"))?;
    let image_size = stride
        .checked_mul(height)
        .ok_or_else(|| cli_error("BMP image size overflow"))?;
    let file_size = 54usize
        .checked_add(image_size)
        .ok_or_else(|| cli_error("BMP file size overflow"))?;
    let image_size_u32 = u32::try_from(image_size).map_err(|_| cli_error("BMP too large"))?;
    let file_size_u32 = u32::try_from(file_size).map_err(|_| cli_error("BMP too large"))?;
    let width_i32 = i32::try_from(image.width).map_err(|_| cli_error("BMP width too large"))?;
    let height_i32 = i32::try_from(image.height).map_err(|_| cli_error("BMP height too large"))?;
    let rgb = rgb_bytes(image);

    let mut file = fs::File::create(path)?;
    file.write_all(b"BM")?;
    file.write_all(&file_size_u32.to_le_bytes())?;
    file.write_all(&0u16.to_le_bytes())?;
    file.write_all(&0u16.to_le_bytes())?;
    file.write_all(&54u32.to_le_bytes())?;
    file.write_all(&40u32.to_le_bytes())?;
    file.write_all(&width_i32.to_le_bytes())?;
    file.write_all(&height_i32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&24u16.to_le_bytes())?;
    file.write_all(&0u32.to_le_bytes())?;
    file.write_all(&image_size_u32.to_le_bytes())?;
    file.write_all(&2835u32.to_le_bytes())?;
    file.write_all(&2835u32.to_le_bytes())?;
    file.write_all(&0u32.to_le_bytes())?;
    file.write_all(&0u32.to_le_bytes())?;

    let pad = [0u8; 3];
    for y in (0..height).rev() {
        let start = y
            .checked_mul(row_bytes)
            .ok_or_else(|| cli_error("BMP row offset overflow"))?;
        let row = &rgb[start..start + row_bytes];
        for px in row.chunks_exact(3) {
            file.write_all(&[px[2], px[1], px[0]])?;
        }
        file.write_all(&pad[..padding])?;
    }
    Ok(())
}

fn temp_bmp_path() -> Result<std::path::PathBuf, AnyError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| cli_error("system clock before epoch"))?
        .as_nanos();
    let mut path = env::temp_dir();
    path.push(format!("dbpx-view-{}-{nanos}.bmp", process::id()));
    Ok(path)
}

fn open_path(path: &Path) -> Result<(), AnyError> {
    let status = if cfg!(target_os = "windows") {
        process::Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("")
            .arg(path)
            .status()?
    } else if cfg!(target_os = "macos") {
        process::Command::new("open").arg(path).status()?
    } else {
        process::Command::new("xdg-open").arg(path).status()?
    };
    if status.success() {
        Ok(())
    } else {
        fail(format!("viewer command failed with status {status}"))
    }
}

fn optional_u32(value: Option<&String>, default: u32, name: &'static str) -> Result<u32, AnyError> {
    match value {
        Some(raw) => Ok(raw
            .parse::<u32>()
            .map_err(|_| cli_error(format!("invalid {name}: {raw}")))?),
        None => Ok(default),
    }
}

fn optional_usize(
    value: Option<&String>,
    default: usize,
    name: &'static str,
) -> Result<usize, AnyError> {
    match value {
        Some(raw) => Ok(raw
            .parse::<usize>()
            .map_err(|_| cli_error(format!("invalid {name}: {raw}")))?),
        None => Ok(default),
    }
}

fn required<'a>(args: &'a [String], index: usize, name: &'static str) -> Result<&'a str, AnyError> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| cli_error(format!("missing argument: {name}")).into())
}

fn arr4(data: &[u8], at: usize) -> Result<[u8; 4], AnyError> {
    let end = at
        .checked_add(4)
        .ok_or_else(|| cli_error("array offset overflow"))?;
    Ok(data
        .get(at..end)
        .ok_or_else(|| cli_error("truncated array"))?
        .try_into()
        .expect("length checked"))
}

fn u32le(data: &[u8], at: usize) -> Result<u32, AnyError> {
    let end = at
        .checked_add(4)
        .ok_or_else(|| cli_error("u32 offset overflow"))?;
    Ok(u32::from_le_bytes(
        data.get(at..end)
            .ok_or_else(|| cli_error("truncated u32"))?
            .try_into()
            .expect("length checked"),
    ))
}

fn u64le(data: &[u8], at: usize) -> Result<u64, AnyError> {
    let end = at
        .checked_add(8)
        .ok_or_else(|| cli_error("u64 offset overflow"))?;
    Ok(u64::from_le_bytes(
        data.get(at..end)
            .ok_or_else(|| cli_error("truncated u64"))?
            .try_into()
            .expect("length checked"),
    ))
}

fn chunk_name(kind: [u8; 4]) -> String {
    kind.iter()
        .map(|&b| if b.is_ascii_graphic() { b as char } else { '.' })
        .collect()
}

fn fail<T>(message: impl Into<String>) -> Result<T, AnyError> {
    Err(cli_error(message).into())
}

fn usage() {
    println!(
        "DBPX tool {VERSION}\n\nUsage:\n  dbpx --version\n  dbpx info <input.dbpx>\n  dbpx check <input.dbpx>\n  dbpx dump <input.dbpx>\n  dbpx bench [width] [height] [iterations]\n  dbpx enc-ppm <input.ppm> <output.dbpx> [--raw|--rle|--indexed]\n  dbpx dec-ppm <input.dbpx> <output.ppm>\n  dbpx dec-bmp <input.dbpx> <output.bmp>\n  dbpx view <input.dbpx>\n  dbpx make-demo <output.dbpx> [width] [height] [--raw|--rle|--indexed]\n\nDefault encoder mode is auto: write the smallest available lossless mode."
    );
}
