# dbyte-dbpx

DBPX is a small deterministic lossless raster image format for screenshots, pixel art, debug frames, firmware display assets, and DBYTE tooling.

It is not trying to beat JPEG on photographs in v0.1. The first target is narrower: a format that is easy to inspect, safe to decode, fast to round-trip, and small enough that one person can understand the full implementation.

## Format

```text
Extension:   .dbpx
Name:        DBPX - DBYTE Pixel Exchange
Magic:       DBPX\r\n\x1A\n
Version:     0.1
Endian:      little-endian
Raster:      GRAY8, RGB8, RGBA8
Alpha:       straight / non-premultiplied
Compression: raw or dbpx-rle
Integrity:   CRC-32 per chunk
```

## Commands

```text
dbpx --version
dbpx info <input.dbpx>
dbpx check <input.dbpx>
dbpx enc-ppm <input.ppm> <output.dbpx> [--raw|--rle]
dbpx dec-ppm <input.dbpx> <output.ppm>
dbpx make-demo <output.dbpx> [width] [height] [--raw|--rle]
```

The default encoder mode is auto. It writes raw or dbpx-rle, whichever produces the smaller DBPX file. Use `--raw` or `--rle` only when you want to force a specific mode.

The CLI starts with PPM instead of PNG/JPEG so the v0.1 toolchain stays dependency-free. PNG/JPEG bridges should be added as optional tooling, not as mandatory core dependencies.

## Build and verify

```sh
cargo fmt --check
cargo check
cargo test
```

Windows:

```powershell
.\scripts\verify.ps1
```

Unix:

```sh
./scripts/verify.sh
```

## Minimal run

```sh
cargo run -- --version
cargo run -- make-demo demo.dbpx 320 200
cargo run -- info demo.dbpx
cargo run -- check demo.dbpx
cargo run -- dec-ppm demo.dbpx demo.ppm
```

For a forced RLE encode:

```sh
cargo run -- make-demo demo-rle.dbpx 320 200 --rle
```

## v0.1 scope

Included:

```text
- DBPX binary container
- strict header validation
- bounded dimensions and pixel count
- raw pixel payload mode
- simple DBPX RLE lossless mode
- auto encoder selection
- CRC-32 per chunk
- dependency-free Rust core and CLI
- PPM bridge
- CLI integration tests
```

Not included yet:

```text
- PNG/JPEG import/export
- lossy photo compression
- progressive decoding
- palettes
- metadata chunks
- thumbnails
- WASM/browser decoder
```

DBPX is meant to be boring in the decoder and weird in the soul: a small image format for deterministic graphics work, DBYTE cartridge assets, VM framebuffer dumps, Pico/TFT diagnostics, and low-level visual tools.
