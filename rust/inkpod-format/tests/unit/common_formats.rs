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
        assert_eq!(
            common_raster_decoded_byte_limit(format, &encoded).unwrap(),
            decoded.pixels.len() as u64
        );
        assert_eq!(
            common_raster_decode_allocation_limit(format, &encoded).unwrap(),
            decoded.pixels.len() as u64
                * if matches!(format, CommonRasterFormat::Png | CommonRasterFormat::Tga) {
                    2
                } else {
                    1
                }
        );
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
        assert_eq!(
            common_raster_decoded_byte_limit(format, &encoded).unwrap(),
            decoded.pixels.len() as u64
        );
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
    assert_eq!(
        common_raster_decoded_byte_limit(CommonRasterFormat::Png, &encoded).unwrap(),
        8
    );
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

fn rgba8_without_dpi(width: u32, height: u32, pixels: Vec<u8>) -> CommonRaster {
    CommonRaster::new(
        width,
        height,
        PixelFormat::StraightRgba8,
        None,
        None,
        pixels,
    )
    .unwrap()
}

#[test]
fn tga_decodes_all_standard_color_mapped_and_black_and_white_types() {
    let mut indexed = vec![0_u8; 18];
    indexed[1] = 1;
    indexed[2] = 1;
    indexed[3..5].copy_from_slice(&5_u16.to_le_bytes());
    indexed[5..7].copy_from_slice(&2_u16.to_le_bytes());
    indexed[7] = 15;
    indexed[12..14].copy_from_slice(&2_u16.to_le_bytes());
    indexed[14..16].copy_from_slice(&1_u16.to_le_bytes());
    indexed[16] = 8;
    indexed[17] = 0x20;
    indexed.extend_from_slice(&0x7c00_u16.to_le_bytes());
    indexed.extend_from_slice(&0x03e0_u16.to_le_bytes());
    indexed.extend_from_slice(&[5, 6]);
    assert_eq!(
        decode_common_raster(CommonRasterFormat::Tga, &indexed)
            .unwrap()
            .pixels,
        [255, 0, 0, 255, 0, 255, 0, 255]
    );

    let mut indexed_rle = vec![0_u8; 18];
    indexed_rle[1] = 1;
    indexed_rle[2] = 9;
    indexed_rle[3..5].copy_from_slice(&300_u16.to_le_bytes());
    indexed_rle[5..7].copy_from_slice(&1_u16.to_le_bytes());
    indexed_rle[7] = 32;
    indexed_rle[12..14].copy_from_slice(&2_u16.to_le_bytes());
    indexed_rle[14..16].copy_from_slice(&1_u16.to_le_bytes());
    indexed_rle[16] = 16;
    indexed_rle[17] = 0x20;
    indexed_rle.extend_from_slice(&[30, 20, 10, 128]);
    indexed_rle.push(0x81);
    indexed_rle.extend_from_slice(&300_u16.to_le_bytes());
    assert_eq!(
        decode_common_raster(CommonRasterFormat::Tga, &indexed_rle)
            .unwrap()
            .pixels,
        [10, 20, 30, 128, 10, 20, 30, 128]
    );

    for (image_type, image_data) in [(3_u8, vec![10, 20]), (11, vec![0x81, 30])] {
        let mut grayscale = vec![0_u8; 18];
        grayscale[2] = image_type;
        grayscale[12..14].copy_from_slice(&2_u16.to_le_bytes());
        grayscale[14..16].copy_from_slice(&1_u16.to_le_bytes());
        grayscale[16] = 8;
        grayscale[17] = 0x20;
        grayscale.extend_from_slice(&image_data);
        let expected = if image_type == 3 {
            vec![10, 10, 10, 255, 20, 20, 20, 255]
        } else {
            vec![30, 30, 30, 255, 30, 30, 30, 255]
        };
        assert_eq!(
            decode_common_raster(CommonRasterFormat::Tga, &grayscale)
                .unwrap()
                .pixels,
            expected
        );
    }
}

#[test]
fn tga_decodes_16_bit_true_color_with_one_attribute_bit() {
    let mut encoded = vec![0_u8; 18];
    encoded[2] = 2;
    encoded[12..14].copy_from_slice(&2_u16.to_le_bytes());
    encoded[14..16].copy_from_slice(&1_u16.to_le_bytes());
    encoded[16] = 16;
    encoded[17] = 0x21;
    encoded.extend_from_slice(&0xfc00_u16.to_le_bytes());
    encoded.extend_from_slice(&0x03e0_u16.to_le_bytes());

    assert_eq!(
        decode_common_raster(CommonRasterFormat::Tga, &encoded)
            .unwrap()
            .pixels,
        [255, 0, 0, 255, 0, 255, 0, 0]
    );
}

#[test]
fn tga_writer_covers_true_color_grayscale_and_color_mapped_storage_matrix() {
    let opaque = rgba8_without_dpi(
        3,
        1,
        vec![
            255, 0, 0, 255, // red
            0, 255, 0, 255, // green
            255, 0, 0, 255, // red
        ],
    );
    for compression in [
        TgaCompression::Uncompressed,
        TgaCompression::RunLengthEncoded,
    ] {
        for depth in [16, 24, 32] {
            let options = TgaEncodeOptions {
                image_format: TgaImageFormat::TrueColor { depth },
                compression,
                allow_color_precision_loss: depth == 16,
                ..TgaEncodeOptions::default()
            };
            let encoded = encode_tga_with_options(&opaque, &options).unwrap();
            assert_eq!(
                encoded[2],
                if compression == TgaCompression::RunLengthEncoded {
                    10
                } else {
                    2
                }
            );
            assert_eq!(encoded[16], depth);
            assert_eq!(
                decode_common_raster(CommonRasterFormat::Tga, &encoded)
                    .unwrap()
                    .pixels,
                opaque.pixels
            );
        }
    }

    let grayscale = rgba8_without_dpi(
        3,
        1,
        vec![10, 10, 10, 255, 20, 20, 20, 255, 10, 10, 10, 255],
    );
    for compression in [
        TgaCompression::Uncompressed,
        TgaCompression::RunLengthEncoded,
    ] {
        let options = TgaEncodeOptions {
            image_format: TgaImageFormat::Grayscale { depth: 8 },
            compression,
            ..TgaEncodeOptions::default()
        };
        let encoded = encode_tga_with_options(&grayscale, &options).unwrap();
        assert_eq!(
            encoded[2],
            if compression == TgaCompression::RunLengthEncoded {
                11
            } else {
                3
            }
        );
        assert_eq!(
            decode_common_raster(CommonRasterFormat::Tga, &encoded)
                .unwrap()
                .pixels,
            grayscale.pixels
        );
    }

    for (index_depth, entry_depth, first_index) in
        [(8, 15, 5), (8, 16, 7), (8, 24, 9), (16, 32, 300)]
    {
        for compression in [
            TgaCompression::Uncompressed,
            TgaCompression::RunLengthEncoded,
        ] {
            let options = TgaEncodeOptions {
                image_format: TgaImageFormat::ColorMapped {
                    index_depth,
                    entry_depth,
                    first_index,
                },
                compression,
                allow_color_precision_loss: matches!(entry_depth, 15 | 16),
                ..TgaEncodeOptions::default()
            };
            let encoded = encode_tga_with_options(&opaque, &options).unwrap();
            assert_eq!(
                encoded[2],
                if compression == TgaCompression::RunLengthEncoded {
                    9
                } else {
                    1
                }
            );
            assert_eq!(encoded[16], index_depth);
            assert_eq!(encoded[7], entry_depth);
            assert_eq!(
                decode_common_raster(CommonRasterFormat::Tga, &encoded)
                    .unwrap()
                    .pixels,
                opaque.pixels
            );
        }
    }
}

#[test]
fn tga_writer_preserves_all_four_origins_and_keeps_rle_packets_inside_rows() {
    let source = rgba8_without_dpi(
        3,
        2,
        vec![
            1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255, 13, 14, 15, 255, 16, 17, 18,
            255,
        ],
    );
    for origin in [
        TgaOrigin::BottomLeft,
        TgaOrigin::BottomRight,
        TgaOrigin::TopLeft,
        TgaOrigin::TopRight,
    ] {
        let options = TgaEncodeOptions {
            image_format: TgaImageFormat::TrueColor { depth: 24 },
            origin,
            ..TgaEncodeOptions::default()
        };
        let encoded = encode_tga_with_options(&source, &options).unwrap();
        assert_eq!(
            decode_common_raster(CommonRasterFormat::Tga, &encoded)
                .unwrap()
                .pixels,
            source.pixels,
            "{origin:?}"
        );
    }

    let repeated = rgba8_without_dpi(3, 2, [9, 8, 7, 255].repeat(6));
    let encoded = encode_tga_with_options(
        &repeated,
        &TgaEncodeOptions {
            compression: TgaCompression::RunLengthEncoded,
            ..TgaEncodeOptions::default()
        },
    )
    .unwrap();
    assert_eq!(encoded[18], 0x82);
    assert_eq!(encoded[23], 0x82);
}

#[test]
fn tga_writer_requires_explicit_lossy_conversions() {
    let source = rgba8_without_dpi(1, 1, vec![10, 20, 30, 128]);
    assert!(matches!(
        encode_tga_with_options(
            &source,
            &TgaEncodeOptions {
                image_format: TgaImageFormat::TrueColor { depth: 24 },
                ..TgaEncodeOptions::default()
            }
        ),
        Err(FormatError::Unsupported(_))
    ));
    assert!(matches!(
        encode_tga_with_options(
            &source,
            &TgaEncodeOptions {
                image_format: TgaImageFormat::TrueColor { depth: 16 },
                ..TgaEncodeOptions::default()
            }
        ),
        Err(FormatError::Unsupported(_))
    ));
    assert!(matches!(
        encode_tga_with_options(
            &source,
            &TgaEncodeOptions {
                image_format: TgaImageFormat::Grayscale { depth: 8 },
                alpha_loss: TgaAlphaLoss::Discard,
                ..TgaEncodeOptions::default()
            }
        ),
        Err(FormatError::Unsupported(_))
    ));

    let options = TgaEncodeOptions {
        image_format: TgaImageFormat::Grayscale { depth: 8 },
        alpha_loss: TgaAlphaLoss::Discard,
        grayscale_conversion: TgaGrayscaleConversion::Bt709,
        ..TgaEncodeOptions::default()
    };
    assert!(encode_tga_with_options(&source, &options).is_ok());

    let one_bit_alpha = rgba8_without_dpi(1, 1, vec![255, 0, 0, 128]);
    let mut options = TgaEncodeOptions {
        image_format: TgaImageFormat::TrueColor { depth: 16 },
        allow_color_precision_loss: true,
        ..TgaEncodeOptions::default()
    };
    assert!(matches!(
        encode_tga_with_options(&one_bit_alpha, &options),
        Err(FormatError::Unsupported(
            "TGA one-bit alpha output would lose precision"
        ))
    ));
    options.allow_alpha_precision_loss = true;
    assert!(encode_tga_with_options(&one_bit_alpha, &options).is_ok());
}

#[test]
fn tga_version_two_metadata_and_auxiliary_blocks_round_trip() {
    let source = rgba8_without_dpi(
        2,
        2,
        vec![
            10, 20, 30, 40, 10, 20, 30, 40, 50, 60, 70, 255, 80, 90, 100, 128,
        ],
    );
    let postage = rgba8_without_dpi(1, 1, vec![12, 34, 56, 78]);
    let color_correction_table = (0_u16..=255)
        .map(|value| {
            let channel = value * 257;
            [channel, channel, channel, channel]
        })
        .collect::<Vec<_>>();
    let extension = TgaExtension {
        author_name: "inkpod".into(),
        author_comments: ["one".into(), "two".into(), "three".into(), "four".into()],
        timestamp: Some(TgaTimestamp {
            month: 8,
            day: 25,
            year: 2026,
            hour: 12,
            minute: 34,
            second: 56,
        }),
        job_name: "TGA matrix".into(),
        job_duration: Some(TgaDuration {
            hours: 1,
            minutes: 2,
            seconds: 3,
        }),
        software_id: "inkpod".into(),
        software_version: 206,
        software_version_letter: Some(b'a'),
        key_color: [1, 2, 3, 4],
        pixel_aspect_ratio: Some(TgaRatio {
            numerator: 4,
            denominator: 3,
        }),
        gamma: Some(TgaRatio {
            numerator: 22,
            denominator: 10,
        }),
        color_correction_table: Some(color_correction_table),
        postage_stamp: Some(postage),
        scan_line_table: true,
        alpha_type: TgaAlphaType::Straight,
        extra: vec![9, 8, 7],
    };
    let options = TgaEncodeOptions {
        compression: TgaCompression::RunLengthEncoded,
        origin: TgaOrigin::BottomRight,
        metadata: TgaMetadata {
            image_id: b"typed-id".to_vec(),
            x_origin: 123,
            y_origin: 456,
            extension: Some(extension.clone()),
            developer_fields: vec![
                TgaDeveloperField {
                    tag: 42,
                    data: vec![1, 2, 3, 4],
                },
                TgaDeveloperField {
                    tag: 65_000,
                    data: vec![5, 6],
                },
            ],
            write_footer: true,
        },
        ..TgaEncodeOptions::default()
    };
    let encoded = encode_tga_with_options(&source, &options).unwrap();
    assert_eq!(&encoded[encoded.len() - 18..], b"TRUEVISION-XFILE.\0");

    let decoded = decode_tga_document(&encoded).unwrap();
    assert_eq!(decoded.raster.as_ref().unwrap().pixels, source.pixels);
    assert_eq!(decoded.options.image_format, options.image_format);
    assert_eq!(decoded.options.compression, options.compression);
    assert_eq!(decoded.options.origin, options.origin);
    assert_eq!(decoded.options.metadata.image_id, b"typed-id");
    assert_eq!(decoded.options.metadata.x_origin, 123);
    assert_eq!(decoded.options.metadata.y_origin, 456);
    assert_eq!(
        decoded.options.metadata.developer_fields,
        options.metadata.developer_fields
    );
    assert_eq!(decoded.options.metadata.extension, Some(extension));
    assert!(decoded.options.metadata.write_footer);
    assert_eq!(
        decode_common_raster(CommonRasterFormat::Tga, &encoded)
            .unwrap()
            .pixels,
        source.pixels
    );
}

#[test]
fn tga_type_zero_and_premultiplied_alpha_have_typed_behavior() {
    let empty = TgaDocument {
        raster: None,
        options: TgaEncodeOptions {
            image_format: TgaImageFormat::None,
            color_map: Some(TgaColorMap {
                first_index: 10,
                entry_depth: 24,
                entries: vec![[1, 2, 3, 255]],
            }),
            metadata: TgaMetadata {
                write_footer: true,
                ..TgaMetadata::default()
            },
            ..TgaEncodeOptions::default()
        },
    };
    let encoded = encode_tga_document(&empty).unwrap();
    let decoded = decode_tga_document(&encoded).unwrap();
    assert!(decoded.raster.is_none());
    assert_eq!(decoded.options.image_format, TgaImageFormat::None);
    assert_eq!(decoded.options.color_map, empty.options.color_map);
    assert!(matches!(
        decode_common_raster(CommonRasterFormat::Tga, &encoded),
        Err(FormatError::Unsupported("TGA contains no image data"))
    ));

    let source = rgba8_without_dpi(1, 1, vec![128, 64, 32, 128]);
    let extension = TgaExtension {
        alpha_type: TgaAlphaType::Premultiplied,
        ..TgaExtension::default()
    };
    let options = TgaEncodeOptions {
        metadata: TgaMetadata {
            extension: Some(extension),
            write_footer: true,
            ..TgaMetadata::default()
        },
        ..TgaEncodeOptions::default()
    };
    let encoded = encode_tga_with_options(&source, &options).unwrap();
    assert_eq!(&encoded[18..22], &[16, 32, 64, 128]);
    assert_eq!(
        decode_common_raster(CommonRasterFormat::Tga, &encoded)
            .unwrap()
            .pixels,
        source.pixels
    );
}

#[test]
fn tga_common_import_applies_color_correction_and_ignores_undefined_attributes() {
    let source = rgba8_without_dpi(1, 1, vec![10, 20, 30, 40]);
    let mut table = (0_u16..=255)
        .map(|value| {
            let channel = value * 257;
            [channel, channel, channel, channel]
        })
        .collect::<Vec<_>>();
    table[10][0] = 200 * 257;
    table[20][1] = 150 * 257;
    table[30][2] = 100 * 257;
    table[255][3] = 90 * 257;
    let extension = TgaExtension {
        color_correction_table: Some(table),
        alpha_type: TgaAlphaType::UndefinedRetain,
        ..TgaExtension::default()
    };
    let encoded = encode_tga_with_options(
        &source,
        &TgaEncodeOptions {
            metadata: TgaMetadata {
                extension: Some(extension),
                write_footer: true,
                ..TgaMetadata::default()
            },
            ..TgaEncodeOptions::default()
        },
    )
    .unwrap();

    assert_eq!(
        decode_tga_document(&encoded)
            .unwrap()
            .raster
            .unwrap()
            .pixels,
        source.pixels
    );
    assert_eq!(
        decode_common_raster(CommonRasterFormat::Tga, &encoded)
            .unwrap()
            .pixels,
        [200, 150, 100, 255]
    );
}

#[test]
fn tga_rejects_reserved_layouts_and_out_of_range_metadata_offsets() {
    let mut reserved = vec![0_u8; 18];
    reserved[2] = 2;
    reserved[12..14].copy_from_slice(&1_u16.to_le_bytes());
    reserved[14..16].copy_from_slice(&1_u16.to_le_bytes());
    reserved[16] = 24;
    reserved[17] = 0x40;
    reserved.extend_from_slice(&[0, 0, 0]);
    assert!(matches!(
        decode_tga_document(&reserved),
        Err(FormatError::Unsupported(
            "TGA descriptor reserved bits are nonzero"
        ))
    ));

    let source = rgba8_without_dpi(1, 1, vec![0, 0, 0, 255]);
    let mut encoded = encode_tga_with_options(
        &source,
        &TgaEncodeOptions {
            metadata: TgaMetadata {
                extension: Some(TgaExtension::default()),
                write_footer: true,
                ..TgaMetadata::default()
            },
            ..TgaEncodeOptions::default()
        },
    )
    .unwrap();
    let footer = encoded.len() - 26;
    encoded[footer..footer + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        decode_tga_document(&encoded),
        Err(FormatError::Invalid(_))
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
