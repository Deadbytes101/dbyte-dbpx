use dbpx::{decode, encode, encode_auto, info, ColorType, Compression, Image};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let cases = [
        ("gradient", gradient(64, 64)?),
        ("solid", solid(64, 64)?),
        ("flat2", flat2(64, 64)?),
        ("stripes", stripes(64, 64)?),
    ];

    println!("case,width,height,raw,rle,indexed,auto,winner,decoded");
    for (name, image) in cases {
        let raw = encode(&image, Compression::RAW)?;
        let rle = encode(&image, Compression::RLE)?;
        let indexed = encode(&image, Compression::INDEXED).ok();
        let auto = encode_auto(&image)?;
        let meta = info(&auto)?;
        let decoded = decode(&auto)?;

        let indexed_text = indexed
            .as_ref()
            .map(|data| data.len().to_string())
            .unwrap_or_else(|| "n/a".to_string());

        println!(
            "{name},{},{},{},{},{},{},{},{}",
            image.width,
            image.height,
            raw.len(),
            rle.len(),
            indexed_text,
            auto.len(),
            meta.compression.name(),
            decoded.pixels.len()
        );
    }

    Ok(())
}

fn gradient(width: u32, height: u32) -> Result<Image, Box<dyn Error>> {
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            pixels.push(((x * 255) / width.max(1)) as u8);
            pixels.push(((y * 255) / height.max(1)) as u8);
            pixels.push(((x ^ y) & 0xFF) as u8);
        }
    }
    Ok(Image::new(width, height, ColorType::RGB8, pixels)?)
}

fn solid(width: u32, height: u32) -> Result<Image, Box<dyn Error>> {
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    for _ in 0..(width * height) {
        pixels.extend_from_slice(&[10, 20, 30]);
    }
    Ok(Image::new(width, height, ColorType::RGB8, pixels)?)
}

fn flat2(width: u32, height: u32) -> Result<Image, Box<dyn Error>> {
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    for i in 0..(width * height) {
        if i % 2 == 0 {
            pixels.extend_from_slice(&[10, 20, 30]);
        } else {
            pixels.extend_from_slice(&[40, 50, 60]);
        }
    }
    Ok(Image::new(width, height, ColorType::RGB8, pixels)?)
}

fn stripes(width: u32, height: u32) -> Result<Image, Box<dyn Error>> {
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        let color = if y % 2 == 0 {
            [220, 220, 220]
        } else {
            [20, 20, 20]
        };
        for _ in 0..width {
            pixels.extend_from_slice(&color);
        }
    }
    Ok(Image::new(width, height, ColorType::RGB8, pixels)?)
}
