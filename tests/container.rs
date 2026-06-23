use dbpx::{decode, encode, encode_auto, info, ColorType, Compression, Image, HEADER_LEN};

fn one_rgb_file() -> Vec<u8> {
    let image = Image::new(1, 1, ColorType::RGB8, vec![1, 2, 3]).unwrap();
    encode(&image, Compression::RAW).unwrap()
}

fn decode_error(data: &[u8]) -> String {
    decode(data).unwrap_err().to_string()
}

#[test]
fn info_matches_encoded_header() {
    let image = Image::new(
        2,
        2,
        ColorType::RGB8,
        vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
    )
    .unwrap();

    let encoded = encode(&image, Compression::RAW).unwrap();
    let meta = info(&encoded).unwrap();

    assert_eq!(meta.width, 2);
    assert_eq!(meta.height, 2);
    assert_eq!(meta.color, ColorType::RGB8);
    assert_eq!(meta.compression, Compression::RAW);
    assert_eq!(meta.pixel_payload_len, 12);
}

#[test]
fn auto_file_decodes_to_original_pixels() {
    let image = Image::new(
        8,
        1,
        ColorType::RGB8,
        vec![
            3, 4, 5, 3, 4, 5, 3, 4, 5, 3, 4, 5, 3, 4, 5, 3, 4, 5, 3, 4, 5, 3, 4, 5,
        ],
    )
    .unwrap();

    let encoded = encode_auto(&image).unwrap();
    assert_eq!(info(&encoded).unwrap().compression, Compression::RLE);
    assert_eq!(decode(&encoded).unwrap(), image);
}

#[test]
fn rejects_trailing_data_after_end_chunk() {
    let mut encoded = one_rgb_file();
    encoded.push(0);

    let err = decode_error(&encoded);
    assert!(err.contains("trailing data"), "unexpected error: {err}");
}

#[test]
fn rejects_bad_magic() {
    let mut encoded = one_rgb_file();
    encoded[0] = b'X';

    let err = decode_error(&encoded);
    assert!(err.contains("bad DBPX magic"), "unexpected error: {err}");
}

#[test]
fn rejects_crc_mismatch() {
    let mut encoded = one_rgb_file();
    encoded[HEADER_LEN + 16] ^= 1;

    let err = decode_error(&encoded);
    assert!(err.contains("CRC mismatch"), "unexpected error: {err}");
}

#[test]
fn rejects_truncated_header() {
    let err = decode_error(b"DBPX");
    assert!(err.contains("truncated header"), "unexpected error: {err}");
}

#[test]
fn rejects_unsupported_version() {
    let mut encoded = one_rgb_file();
    encoded[8] = 1;

    let err = decode_error(&encoded);
    assert!(
        err.contains("unsupported DBPX version"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_bad_compression_id() {
    let mut encoded = one_rgb_file();
    encoded[22] = 9;

    let err = decode_error(&encoded);
    assert!(err.contains("bad compression"), "unexpected error: {err}");
}

#[test]
fn rejects_reserved_flags() {
    let mut encoded = one_rgb_file();
    encoded[23] = 1;

    let err = decode_error(&encoded);
    assert!(
        err.contains("reserved flags set"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_zero_width_header() {
    let mut encoded = one_rgb_file();
    encoded[12..16].copy_from_slice(&0u32.to_le_bytes());

    let err = decode_error(&encoded);
    assert!(err.contains("dimensions"), "unexpected error: {err}");
}

#[test]
fn rejects_missing_end_chunk() {
    let mut encoded = one_rgb_file();
    encoded.truncate(encoded.len() - 16);

    let err = decode_error(&encoded);
    assert!(err.contains("missing END"), "unexpected error: {err}");
}

#[test]
fn rejects_chunk_count_mismatch() {
    let mut encoded = one_rgb_file();
    encoded[24..28].copy_from_slice(&1u32.to_le_bytes());

    let err = decode_error(&encoded);
    assert!(
        err.contains("chunk count mismatch"),
        "unexpected error: {err}"
    );
}
