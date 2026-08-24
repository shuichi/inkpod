use super::*;

fn rgba8() -> CommonRaster {
    CommonRaster::new(
        3,
        2,
        PixelFormat::StraightRgba8,
        Some(96_000),
        Some(120_000),
        vec![
            1, 2, 3, 4, 5, 6, 7, 128, 8, 9, 10, 255, 11, 12, 13, 0, 14, 15, 16, 200, 17, 18, 19,
            255,
        ],
    )
    .unwrap()
}

fn rgba16() -> CommonRaster {
    let channels = [
        0_u16, 1, 257, 65_535, 2, 3, 4, 32_768, 5, 6, 7, 0, 8, 9, 10, 100,
    ];
    let pixels = channels
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    CommonRaster::new(
        2,
        2,
        PixelFormat::StraightRgba16,
        Some(72_000),
        Some(144_000),
        pixels,
    )
    .unwrap()
}

fn assert_dpi_close(actual: Option<u32>, expected: Option<u32>) {
    match (actual, expected) {
        (Some(actual), Some(expected)) => assert!(actual.abs_diff(expected) <= 20),
        (actual, expected) => assert_eq!(actual, expected),
    }
}

#[test]
fn common_formats_round_trip_depth_alpha_dimensions_and_dpi() {
    for format in [
        CommonRasterFormat::Png,
        CommonRasterFormat::Tiff,
        CommonRasterFormat::Tga,
        CommonRasterFormat::Bmp,
    ] {
        let source = rgba8();
        let encoded = encode_common_raster(format, &source, false).unwrap();
        let decoded = decode_common_raster(format, &encoded).unwrap();
        assert_eq!(decoded.info.width, source.info.width, "{format:?}");
        assert_eq!(decoded.info.height, source.info.height, "{format:?}");
        assert_eq!(
            decoded.info.pixel_format, source.info.pixel_format,
            "{format:?}"
        );
        assert_eq!(decoded.pixels, source.pixels, "{format:?}");
        if format.supports_dpi() {
            assert_dpi_close(decoded.info.dpi_x_milli, source.info.dpi_x_milli);
            assert_dpi_close(decoded.info.dpi_y_milli, source.info.dpi_y_milli);
        } else {
            assert_eq!(decoded.info.dpi_x_milli, None);
            assert_eq!(decoded.info.dpi_y_milli, None);
        }
    }
    for format in [CommonRasterFormat::Png, CommonRasterFormat::Tiff] {
        let source = rgba16();
        let encoded = encode_common_raster(format, &source, false).unwrap();
        let decoded = decode_common_raster(format, &encoded).unwrap();
        assert_eq!(decoded.info.width, source.info.width, "{format:?}");
        assert_eq!(decoded.info.height, source.info.height, "{format:?}");
        assert_eq!(
            decoded.info.pixel_format, source.info.pixel_format,
            "{format:?}"
        );
        assert_dpi_close(decoded.info.dpi_x_milli, source.info.dpi_x_milli);
        assert_dpi_close(decoded.info.dpi_y_milli, source.info.dpi_y_milli);
        assert_eq!(decoded.pixels, source.pixels, "{format:?}");
    }
    assert!(matches!(
        encode_common_raster(CommonRasterFormat::Tga, &rgba16(), false),
        Err(FormatError::Unsupported(_))
    ));
}

#[test]
fn white_background_export_is_explicit_and_alpha_safe() {
    let source = rgba8();
    let encoded = encode_common_raster(CommonRasterFormat::Png, &source, true).unwrap();
    let decoded = decode_common_raster(CommonRasterFormat::Png, &encoded).unwrap();
    assert!(decoded.pixels.chunks_exact(4).all(|pixel| pixel[3] == 255));
    assert_eq!(&decoded.pixels[12..16], &[255, 255, 255, 255]);
    assert_eq!(source.pixels[15], 0);
}

#[test]
fn tiff_declares_straight_alpha_and_rejects_associated_alpha() {
    let mut encoded = encode_common_raster(CommonRasterFormat::Tiff, &rgba8(), false).unwrap();
    let entry = encoded
        .windows(2)
        .enumerate()
        .find_map(|(index, bytes)| (bytes == 338_u16.to_le_bytes()).then_some(index))
        .unwrap();
    assert_eq!(
        u32::from_le_bytes(encoded[entry + 8..entry + 12].try_into().unwrap()),
        2
    );
    encoded[entry + 8..entry + 12].copy_from_slice(&1_u32.to_le_bytes());
    assert!(matches!(
        decode_common_raster(CommonRasterFormat::Tiff, &encoded),
        Err(FormatError::Unsupported(_))
    ));
}

#[test]
fn common_raster_revalidates_public_metadata_before_allocation() {
    let mut mutated = rgba8();
    mutated.info.width = MAX_RASTER_DIMENSION + 1;
    assert!(matches!(
        encode_common_raster(CommonRasterFormat::Png, &mutated, false),
        Err(FormatError::Invalid(
            "common raster dimensions are outside bounds"
        ))
    ));

    let mut bmp = vec![0_u8; 54];
    bmp[..2].copy_from_slice(b"BM");
    bmp[10..14].copy_from_slice(&54_u32.to_le_bytes());
    bmp[14..18].copy_from_slice(&40_u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&(MAX_RASTER_DIMENSION + 1).to_le_bytes());
    bmp[22..26].copy_from_slice(&1_i32.to_le_bytes());
    bmp[26..28].copy_from_slice(&1_u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&32_u16.to_le_bytes());
    assert!(matches!(
        decode_common_raster(CommonRasterFormat::Bmp, &bmp),
        Err(FormatError::Invalid(
            "common raster dimensions are outside bounds"
        ))
    ));
}

#[test]
fn png_expands_indexed_palette_and_transparency() {
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, 2, 1);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(vec![10, 20, 30, 40, 50, 60]);
        encoder.set_trns(vec![0, 128]);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[0, 1]).unwrap();
    }

    let decoded = decode_common_raster(CommonRasterFormat::Png, &encoded).unwrap();
    assert_eq!(decoded.info.pixel_format, PixelFormat::StraightRgba8);
    assert_eq!(decoded.pixels, [10, 20, 30, 0, 40, 50, 60, 128]);
}

#[test]
fn tga_honors_right_origin_and_alpha_attribute_bits() {
    let mut encoded = vec![0_u8; 18];
    encoded[2] = 2;
    encoded[12..14].copy_from_slice(&2_u16.to_le_bytes());
    encoded[14..16].copy_from_slice(&1_u16.to_le_bytes());
    encoded[16] = 32;
    encoded[17] = 0x38; // top-right origin, eight alpha bits
    encoded.extend_from_slice(&[3, 2, 1, 4, 7, 6, 5, 8]);

    let decoded = decode_common_raster(CommonRasterFormat::Tga, &encoded).unwrap();
    assert_eq!(decoded.pixels, [5, 6, 7, 8, 1, 2, 3, 4]);

    encoded[17] = 0x30; // top-right origin, no declared alpha channel
    let decoded = decode_common_raster(CommonRasterFormat::Tga, &encoded).unwrap();
    assert_eq!(decoded.pixels, [5, 6, 7, 255, 1, 2, 3, 255]);
}

#[test]
fn tga_decodes_run_length_encoded_true_color_packets_across_rows() {
    let mut encoded = vec![0_u8; 18];
    encoded[2] = 10;
    encoded[12..14].copy_from_slice(&3_u16.to_le_bytes());
    encoded[14..16].copy_from_slice(&2_u16.to_le_bytes());
    encoded[16] = 24;

    encoded.extend_from_slice(&[
        0x00, 3, 2, 1, // one raw pixel
        0x82, 6, 5, 4, // three repeated pixels, crossing the row boundary
        0x01, 9, 8, 7, 12, 11, 10, // two raw pixels
    ]);

    let decoded = decode_common_raster(CommonRasterFormat::Tga, &encoded).unwrap();
    assert_eq!(decoded.info.width, 3);
    assert_eq!(decoded.info.height, 2);
    assert_eq!(
        decoded.pixels,
        [
            4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255, // top row
            1, 2, 3, 255, 4, 5, 6, 255, 4, 5, 6, 255, // bottom row
        ]
    );
}

#[test]
fn tga_rejects_malformed_run_length_packets() {
    let mut encoded = vec![0_u8; 18];
    encoded[2] = 10;
    encoded[12..14].copy_from_slice(&2_u16.to_le_bytes());
    encoded[14..16].copy_from_slice(&1_u16.to_le_bytes());
    encoded[16] = 24;

    encoded.push(0x81);
    assert!(matches!(
        decode_common_raster(CommonRasterFormat::Tga, &encoded),
        Err(FormatError::Invalid("TGA RLE pixel data is truncated"))
    ));

    encoded.extend_from_slice(&[3, 2, 1]);
    encoded[18] = 0x82;
    assert!(matches!(
        decode_common_raster(CommonRasterFormat::Tga, &encoded),
        Err(FormatError::Invalid("TGA RLE packet exceeds image bounds"))
    ));
}

#[test]
fn bmp_accepts_padded_24_bit_rows_and_validates_bitfield_masks() {
    let mut encoded = vec![0_u8; 54];
    encoded[..2].copy_from_slice(b"BM");
    encoded[2..6].copy_from_slice(&62_u32.to_le_bytes());
    encoded[10..14].copy_from_slice(&54_u32.to_le_bytes());
    encoded[14..18].copy_from_slice(&40_u32.to_le_bytes());
    encoded[18..22].copy_from_slice(&2_i32.to_le_bytes());
    encoded[22..26].copy_from_slice(&(-1_i32).to_le_bytes());
    encoded[26..28].copy_from_slice(&1_u16.to_le_bytes());
    encoded[28..30].copy_from_slice(&24_u16.to_le_bytes());
    encoded[34..38].copy_from_slice(&8_u32.to_le_bytes());
    encoded.extend_from_slice(&[3, 2, 1, 6, 5, 4, 0, 0]);

    let decoded = decode_common_raster(CommonRasterFormat::Bmp, &encoded).unwrap();
    assert_eq!(decoded.pixels, [1, 2, 3, 255, 4, 5, 6, 255]);

    let mut invalid_masks = encode_common_raster(CommonRasterFormat::Bmp, &rgba8(), false).unwrap();
    invalid_masks[54..58].copy_from_slice(&0x0000_00ff_u32.to_le_bytes());
    assert!(matches!(
        decode_common_raster(CommonRasterFormat::Bmp, &invalid_masks),
        Err(FormatError::Unsupported(
            "BMP bitfield masks are unsupported"
        ))
    ));
}
