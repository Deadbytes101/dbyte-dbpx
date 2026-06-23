# DBPX v0.1

DBPX is a small deterministic lossless raster image format.

This document describes the initial v0.1 container. The v0.1 implementation intentionally keeps the codec simple so the decoder can be audited and fuzzed before adding PNG/JPEG bridges or lossy modes.

## File identity

```text
Extension: .dbpx
Magic:     44 42 50 58 0D 0A 1A 0A
ASCII:     DBPX\r\n\x1A\n
Endian:    little-endian
Version:   0.1
```

The magic follows the PNG-style newline/control-byte pattern so accidental text transfer damage is easier to notice.

## Header

The fixed header is 28 bytes.

```text
0   u8[8]  magic
8   u8     version_major
9   u8     version_minor
10  u16    header_size
12  u32    width
16  u32    height
20  u8     color_type
21  u8     bit_depth
22  u8     compression
23  u8     flags
24  u32    chunk_count
```

Rules:

```text
version_major must be 0
header_size must be 28
width and height must be non-zero
width and height must be <= 65535
width * height must be <= 67108864
bit_depth must be 8
flags must be 0
```

## Color types

```text
1 = GRAY8
2 = RGB8
3 = RGBA8
```

RGBA8 uses straight alpha, not premultiplied alpha.

## Compression types

```text
0 = raw
1 = dbpx-rle
```

Raw stores pixels exactly in raster order.

DBPX RLE stores a sequence of 5-byte records:

```text
u8     run length, 1..255
u8[4]  RGBA pixel
```

For GRAY8 and RGB8 images, the RLE stream still stores temporary RGBA pixels. The decoder converts the decoded RGBA stream back into the declared color type after expansion.

## Chunks

Each chunk has a 16-byte header followed by payload bytes.

```text
0   u8[4]  chunk type
4   u64    payload length
12  u32    CRC-32 of chunk type + payload
16  u8[]   payload
```

Required chunk order for v0.1:

```text
PXLS
END!
```

Unknown chunks are rejected in v0.1. This is intentionally strict; extension chunks should be added only after the decoder is fuzzed and compatibility rules are frozen.

## Decoder security rules

A conforming decoder rejects:

```text
- bad magic
- unsupported version
- unsupported color type
- unsupported compression
- unsupported bit depth
- non-zero flags
- zero dimensions
- oversized dimensions
- oversized pixel count
- truncated header
- truncated chunk header
- truncated chunk body
- CRC mismatch
- duplicate PXLS chunk
- missing PXLS chunk
- missing END! chunk
- chunk_count mismatch
- trailing data after END!
- malformed RLE stream
```

All size calculations must be checked before allocation.
