use dbpx::{decode, encode, encode_auto, info, rgb_bytes, ColorType, Compression, Image};
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process;

const VERSION: &str = env!("CARGO_PKG_VERSION");
type AnyError = Box<dyn Error>;

#[derive(Debug)]
struct CliError(String);

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
        Some("enc-ppm") => command_encode_ppm(&args[2..]),
        Some("dec-ppm") => command_decode_ppm(&args[2..]),
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

fn command_encode_ppm(args: &[String]) -> Result<(), AnyError> {
    let input = required(args, 0, "input.ppm")?;
    let output = required(args, 1, "output.dbpx")?;
    let image = parse_ppm(&fs::read(input)?)?;
    fs::write(output, encode_from_flags(&image, args)?)?;
    Ok(())
}

fn command_decode_ppm(args: &[String]) -> Result<(), AnyError> {
    let input = required(args, 0, "input.dbpx")?;
    let output = required(args, 1, "output.ppm")?;
    let image = decode(&fs::read(input)?)?;
    write_ppm(output, &image)?;
    Ok(())
}

fn command_make_demo(args: &[String]) -> Result<(), AnyError> {
    let output = required(args, 0, "output.dbpx")?;
    let width = optional_u32(args.get(1), 320, "width")?;
    let height = optional_u32(args.get(2), 200, "height")?;
    let image = demo(width, height)?;
    fs::write(output, encode_from_flags(&image, args)?)?;
    Ok(())
}

fn encode_from_flags(image: &Image, args: &[String]) -> Result<Vec<u8>, dbpx::DbpxError> {
    if args.iter().any(|arg| arg == "--raw") {
        encode(image, Compression::RAW)
    } else if args.iter().any(|arg| arg == "--rle") {
        encode(image, Compression::RLE)
    } else {
        encode_auto(image)
    }
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

fn optional_u32(value: Option<&String>, default: u32, name: &'static str) -> Result<u32, AnyError> {
    match value {
        Some(raw) => Ok(raw
            .parse::<u32>()
            .map_err(|_| cli_error(format!("invalid {name}: {raw}")))?),
        None => Ok(default),
    }
}

fn required<'a>(args: &'a [String], index: usize, name: &'static str) -> Result<&'a str, AnyError> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| cli_error(format!("missing argument: {name}")).into())
}

fn fail<T>(message: impl Into<String>) -> Result<T, AnyError> {
    Err(cli_error(message).into())
}

fn usage() {
    println!(
        "DBPX tool {VERSION}\n\nUsage:\n  dbpx --version\n  dbpx info <input.dbpx>\n  dbpx check <input.dbpx>\n  dbpx enc-ppm <input.ppm> <output.dbpx> [--raw|--rle]\n  dbpx dec-ppm <input.dbpx> <output.ppm>\n  dbpx make-demo <output.dbpx> [width] [height] [--raw|--rle]\n\nDefault encoder mode is auto: write raw or dbpx-rle, whichever is smaller."
    );
}
