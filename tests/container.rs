use dbpx::{decode, encode, encode_auto, info, ColorType, Compression, Image, HEADER_LEN};

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
            3, 4, 5, 3, 4, 5, 3, 4, 5, 3, 4, 5, 3, 4, 5, 3, 4, 5, 3, 4, 5, 3,
            4, 5,
        ],
    )
    .unwrap();

    let encoded = encode_auto(&image).unwrap();
    assert_eq!(info(&encoded).unwrap().compression, Compression::RLE);
    assert_eq!(decode(&encoded).unwrap(), image);
}

#[test]
fn rejects_trailing_data_after_end_chunk() {
    let image = Image::new(1, 1, ColorType::RGB8, vec![1, 2, 3]).unwrap();
    let mut encoded = encode(&image, Compression::RAW).unwrap();
    encoded.push(0);

    let err = decode(&encoded).unwrap_err().to_string();
    assert!(err.contains("trailing data"), "unexpected error: {err}");
}

#[test]
fn rejects_bad_magic() {
    let image = Image::new(1, 1, ColorType::RGB8, vec![1, 2, 3]).unwrap();
    let mut encoded = encode(&image, Compression::RAW).unwrap();
    encoded[0] = b'X';

    let err = decode(&encoded).unwrap_err().to_string();
    assert!(err.contains("bad DBPX magic"), "unexpected error: {err}");
}

#[test]
fn rejects_crc_mismatch() {
    let image = Image::new(1, 1, ColorType::RGB8, vec![1, 2, 3]).unwrap();
    let mut encoded = encode(&image, Compression::RAW).unwrap();
    encoded[HEADER_LEN + 16] ^= 1;

    let err = decode(&encoded).unwrap_err().to_string();
    assert!(err.contains("CRC mismatch"), "unexpected error: {err}");
}
