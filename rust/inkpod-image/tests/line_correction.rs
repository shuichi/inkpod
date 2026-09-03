use inkpod_image::*;

fn raster(width: u32, height: u32, format: PixelFormat) -> TileRaster {
    TileRaster::new(width, height, format).unwrap()
}
fn ink(format: PixelFormat) -> PixelValue {
    match format {
        PixelFormat::BinaryMask8 => PixelValue::Binary(255),
        PixelFormat::Grayscale8 => PixelValue::Grayscale8(173),
        PixelFormat::Grayscale16 => PixelValue::Grayscale16(44_321),
        PixelFormat::StraightRgba8 => PixelValue::Rgba([20, 40, 60, 173]),
        PixelFormat::StraightRgba16 => PixelValue::Rgba16([5111, 10222, 15333, 44321]),
        _ => unreachable!(),
    }
}
fn formats() -> [PixelFormat; 5] {
    [
        PixelFormat::BinaryMask8,
        PixelFormat::Grayscale8,
        PixelFormat::Grayscale16,
        PixelFormat::StraightRgba8,
        PixelFormat::StraightRgba16,
    ]
}
fn pixels(source: &TileRaster) -> Vec<PixelValue> {
    (0..source.height())
        .flat_map(|y| (0..source.width()).map(move |x| source.pixel(x, y).unwrap()))
        .collect()
}

#[test]
fn connection_inclusive_gap_in_all_directions_and_native_formats() {
    for format in formats() {
        for (dx, dy) in [(1, 0), (0, 1), (1, 1)] {
            for missing in [1, 2, 3] {
                let mut source = raster(32, 32, format);
                for n in 3..=24 {
                    if !(10..10 + missing).contains(&n) {
                        source
                            .set_pixel(3 + n * dx, 3 + n * dy, ink(format), 1)
                            .unwrap();
                    }
                }
                let before = pixels(&source);
                let result = apply_line_connection(
                    &source,
                    None,
                    2,
                    1,
                    LineBackground::Transparent,
                    2,
                    |_, _| true,
                )
                .unwrap();
                let mut expected = source.clone();
                if missing <= 2 {
                    for n in 10..10 + missing {
                        expected
                            .set_pixel(3 + n * dx, 3 + n * dy, ink(format), 2)
                            .unwrap();
                    }
                }
                assert_eq!(
                    pixels(&result),
                    pixels(&expected),
                    "{format:?} ({dx},{dy}) missing={missing}"
                );
                assert_eq!(pixels(&source), before);
                assert_eq!(
                    pixels(
                        &apply_line_connection(
                            &source,
                            None,
                            2,
                            1,
                            LineBackground::Transparent,
                            2,
                            |_, _| true
                        )
                        .unwrap()
                    ),
                    pixels(&result)
                );
            }
        }
    }
}

#[test]
fn thicken_thin_and_uniform_are_independent_native_depth_operations() {
    for format in formats() {
        for width in [1, 3, 7] {
            let mut source = raster(64, 32, format);
            for x in 4..60 {
                for y in 16 - width / 2..=16 + width / 2 {
                    source.set_pixel(x, y, ink(format), 1).unwrap();
                }
            }
            for (mode, amount, expected) in [
                (LineWidthMode::Thicken, 1, width + 2),
                (LineWidthMode::Thin, 1, width.saturating_sub(2)),
                (LineWidthMode::Uniform, 3, 3),
                (LineWidthMode::Uniform, 4, 4),
            ] {
                let result = apply_line_width(
                    &source,
                    None,
                    mode,
                    amount,
                    LineBackground::Transparent,
                    2,
                    |_, _| true,
                )
                .unwrap();
                let actual = (0..32)
                    .filter(|&y| !result.pixel(24, y).unwrap().is_transparent())
                    .count();
                assert_eq!(
                    actual, expected as usize,
                    "{format:?} {mode:?} source width={width}"
                );
                for y in 0..32 {
                    let pixel = result.pixel(24, y).unwrap();
                    if !pixel.is_transparent() {
                        assert_eq!(pixel, ink(format));
                    }
                }
            }
        }
    }
}

#[test]
fn uniform_reconstructs_one_continuous_line_with_three_source_widths() {
    let mut source = raster(96, 40, PixelFormat::BinaryMask8);
    for (from, to, width) in [(4, 28, 1), (28, 56, 3), (56, 92, 7)] {
        for x in from..to {
            for y in 20 - width / 2..=20 + width / 2 {
                source.set_pixel(x, y, PixelValue::Binary(255), 1).unwrap();
            }
        }
    }
    let output = apply_line_width(
        &source,
        None,
        LineWidthMode::Uniform,
        5,
        LineBackground::Transparent,
        2,
        |_, _| true,
    )
    .unwrap();
    for x in [16, 42, 74] {
        assert_eq!(
            (0..40)
                .filter(|&y| !output.pixel(x, y).unwrap().is_transparent())
                .collect::<Vec<_>>(),
            vec![18, 19, 20, 21, 22]
        );
    }
}

#[test]
fn centerline_keeps_small_components_and_closed_loops() {
    let mut source = raster(32, 32, PixelFormat::BinaryMask8);
    for y in 2..4 {
        for x in 2..4 {
            source.set_pixel(x, y, PixelValue::Binary(255), 1).unwrap();
        }
    }
    for n in 8..25 {
        for (x, y) in [(n, 8), (n, 24), (8, n), (24, n)] {
            source.set_pixel(x, y, PixelValue::Binary(255), 1).unwrap();
        }
    }
    let output = apply_line_width(
        &source,
        None,
        LineWidthMode::Uniform,
        1,
        LineBackground::Transparent,
        2,
        |_, _| true,
    )
    .unwrap();
    assert!(
        (1..5).any(|y| (1..5).any(|x| !output.pixel(x, y).unwrap().is_transparent())),
        "a 2x2 component cannot disappear during normalization"
    );
    for n in 8..25 {
        for (x, y) in [(n, 8), (n, 24), (8, n), (24, n)] {
            assert!(!output.pixel(x, y).unwrap().is_transparent());
        }
    }
    assert!(output.pixel(16, 16).unwrap().is_transparent());
}

#[test]
fn local_mask_cancel_invalid_parallel_and_ambiguous_connections() {
    let mut source = raster(32, 32, PixelFormat::BinaryMask8);
    for x in 3..=8 {
        source.set_pixel(x, 16, PixelValue::Binary(255), 1).unwrap();
    }
    for y in [14, 18] {
        for x in 12..=24 {
            source.set_pixel(x, y, PixelValue::Binary(255), 1).unwrap();
        }
    }
    assert_eq!(
        pixels(
            &apply_line_connection(
                &source,
                None,
                4,
                1,
                LineBackground::Transparent,
                2,
                |_, _| true
            )
            .unwrap()
        ),
        pixels(&source)
    );
    assert!(matches!(
        apply_line_connection(
            &source,
            None,
            4,
            1,
            LineBackground::Transparent,
            2,
            |_, _| false
        ),
        Err(RasterError::Cancelled)
    ));
    assert!(
        apply_line_connection(
            &source,
            None,
            65,
            1,
            LineBackground::Transparent,
            2,
            |_, _| true
        )
        .is_err()
    );
    let mut mask = raster(32, 32, PixelFormat::BinaryMask8);
    mask.set_pixel(6, 15, PixelValue::Binary(255), 1).unwrap();
    let result = apply_line_width(
        &source,
        Some(&mask),
        LineWidthMode::Thicken,
        1,
        LineBackground::Transparent,
        2,
        |_, _| true,
    )
    .unwrap();
    let mut expected = source.clone();
    expected
        .set_pixel(6, 15, PixelValue::Binary(255), 2)
        .unwrap();
    assert_eq!(pixels(&result), pixels(&expected));
    let empty_mask = raster(32, 32, PixelFormat::BinaryMask8);
    assert_eq!(
        pixels(
            &apply_line_width(
                &source,
                Some(&empty_mask),
                LineWidthMode::Thin,
                1,
                LineBackground::Transparent,
                2,
                |_, _| true
            )
            .unwrap()
        ),
        pixels(&source)
    );
}

#[test]
fn connection_footprint_width_parallel_crossing_and_band_exclusion() {
    let mut source = raster(32, 32, PixelFormat::BinaryMask8);
    for x in (3..=8).chain(12..=26) {
        source.set_pixel(x, 16, PixelValue::Binary(255), 1).unwrap();
    }
    let connect = |source: &TileRaster, mask: Option<&TileRaster>, width| {
        apply_line_connection(
            source,
            mask,
            3,
            width,
            LineBackground::Transparent,
            2,
            |_, _| true,
        )
        .unwrap()
    };
    let result = connect(&source, None, 3);
    let mut expected = source.clone();
    // Diameter 3 includes integer offsets -1..1 on both axes (squared distance <= 2.25).
    for y in 15..=17 {
        for x in 7..=13 {
            expected
                .set_pixel(x, y, PixelValue::Binary(255), 2)
                .unwrap();
        }
    }
    assert_eq!(pixels(&result), pixels(&expected));
    let mut mask = raster(32, 32, PixelFormat::BinaryMask8);
    for x in 7..=13 {
        mask.set_pixel(x, 16, PixelValue::Binary(255), 1).unwrap();
    }
    assert_eq!(pixels(&connect(&source, Some(&mask), 3)), pixels(&source));
    mask.set_pixel(10, 16, PixelValue::Binary(0), 1).unwrap();
    assert_eq!(pixels(&connect(&source, Some(&mask), 1)), pixels(&source));
    let mut crossing = source.clone();
    for y in 5..=26 {
        crossing
            .set_pixel(10, y, PixelValue::Binary(255), 1)
            .unwrap();
    }
    assert_eq!(pixels(&connect(&crossing, None, 1)), pixels(&crossing));
    let mut parallel = raster(32, 32, PixelFormat::BinaryMask8);
    for y in [14, 17] {
        for x in 3..=26 {
            parallel
                .set_pixel(x, y, PixelValue::Binary(255), 1)
                .unwrap();
        }
    }
    assert_eq!(pixels(&connect(&parallel, None, 1)), pixels(&parallel));
}

#[test]
fn explicit_native_background_dust_holes_outliers_and_invalid_eight_bit_color() {
    for format in [PixelFormat::StraightRgba8, PixelFormat::StraightRgba16] {
        let (background, native, foreground) = if format == PixelFormat::StraightRgba8 {
            (
                [2570, 5140, 7710, 65535],
                PixelValue::Rgba([10, 20, 30, 255]),
                PixelValue::Rgba([140, 100, 80, 255]),
            )
        } else {
            (
                [2571, 5141, 7711, 65535],
                PixelValue::Rgba16([2571, 5141, 7711, 65535]),
                PixelValue::Rgba16([35001, 25001, 20001, 65535]),
            )
        };
        let mut source = raster(7, 7, format);
        for y in 0..7 {
            for x in 0..7 {
                source.set_pixel(x, y, native, 1).unwrap();
            }
        }
        source.set_pixel(3, 3, foreground, 1).unwrap();
        let options = DustRemoval {
            mode: DustMode::RemoveForeground,
            maximum_pixels: 1,
            background: LineBackground::TransparentOrColor(background),
        };
        let result = apply_dust_removal(&source, None, options, 2, |_, _| true).unwrap();
        assert!(pixels(&result).into_iter().all(|p| p == native));
        let mut reversed = raster(7, 7, format);
        for y in 0..7 {
            for x in 0..7 {
                reversed.set_pixel(x, y, foreground, 1).unwrap();
            }
        }
        reversed.set_pixel(3, 3, native, 1).unwrap();
        for mode in [
            DustMode::FillTransparentHoles,
            DustMode::ReplaceColorOutliers,
        ] {
            let result = apply_dust_removal(
                &reversed,
                None,
                DustRemoval { mode, ..options },
                2,
                |_, _| true,
            )
            .unwrap();
            assert!(pixels(&result).into_iter().all(|p| p == foreground));
        }
        if format == PixelFormat::StraightRgba8 {
            assert!(
                apply_line_width(
                    &source,
                    None,
                    LineWidthMode::Thicken,
                    1,
                    LineBackground::TransparentOrColor([1, 2, 3, 65535]),
                    2,
                    |_, _| true
                )
                .is_err()
            );
        }
    }
}

#[test]
fn uniform_keeps_branch_endpoints_connected_and_morphology_has_circular_corners() {
    let mut source = raster(32, 32, PixelFormat::BinaryMask8);
    for y in 5..=25 {
        for x in 14..=18 {
            source.set_pixel(x, y, PixelValue::Binary(255), 1).unwrap();
        }
    }
    for x in 5..=26 {
        for y in 5..=9 {
            source.set_pixel(x, y, PixelValue::Binary(255), 1).unwrap();
        }
    }
    let output = apply_line_width(
        &source,
        None,
        LineWidthMode::Uniform,
        1,
        LineBackground::Transparent,
        2,
        |_, _| true,
    )
    .unwrap();
    let selected: std::collections::BTreeSet<_> = (0..32)
        .flat_map(|y| (0..32).map(move |x| (x, y)))
        .filter(|&(x, y)| output.pixel(x, y).unwrap() == PixelValue::Binary(255))
        .collect();
    let mut reached = std::collections::BTreeSet::new();
    let mut pending = vec![*selected.first().unwrap()];
    while let Some((x, y)) = pending.pop() {
        if !reached.insert((x, y)) {
            continue;
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                let p = ((x as i32 + dx) as u32, (y as i32 + dy) as u32);
                if selected.contains(&p) && !reached.contains(&p) {
                    pending.push(p);
                }
            }
        }
    }
    assert_eq!(reached, selected);
    assert!(selected.iter().any(|&(x, y)| x < 10 && y < 10));
    assert!(selected.iter().any(|&(x, y)| x > 22 && y < 10));
    assert!(selected.iter().any(|&(_, y)| y > 21));
    let mut point = raster(5, 5, PixelFormat::BinaryMask8);
    point.set_pixel(2, 2, PixelValue::Binary(255), 1).unwrap();
    let dilated = apply_line_width(
        &point,
        None,
        LineWidthMode::Thicken,
        1,
        LineBackground::Transparent,
        2,
        |_, _| true,
    )
    .unwrap();
    for y in 0..5 {
        for x in 0..5 {
            assert_eq!(
                dilated.pixel(x, y).unwrap() == PixelValue::Binary(255),
                (x as i32 - 2).abs() + (y as i32 - 2).abs() <= 1
            );
        }
    }
}

#[test]
fn native_low_alpha_contrast_against_custom_background_stays_foreground() {
    let background = [1000, 2000, 3000, 1];
    let foreground = PixelValue::Rgba16([1001, 2000, 3000, 1]);
    let mut source = raster(9, 9, PixelFormat::StraightRgba16);
    for y in 0..9 {
        for x in 0..9 {
            source
                .set_pixel(x, y, PixelValue::Rgba16(background), 1)
                .unwrap();
        }
    }
    for x in 2..7 {
        source.set_pixel(x, 4, foreground, 1).unwrap();
    }
    let original = pixels(&source);
    for mode in [LineWidthMode::Thicken, LineWidthMode::Thin] {
        let result = apply_line_width(
            &source,
            None,
            mode,
            1,
            LineBackground::TransparentOrColor(background),
            2,
            |_, _| true,
        )
        .unwrap();
        for y in 0..9 {
            for x in 0..9 {
                let included = mode == LineWidthMode::Thicken
                    && ((y == 4 && (1..8).contains(&x))
                        || ((y == 3 || y == 5) && (2..7).contains(&x)));
                let expected = if included {
                    foreground
                } else {
                    PixelValue::Rgba16(background)
                };
                assert_eq!(result.pixel(x, y).unwrap(), expected, "{mode:?} ({x},{y})");
            }
        }
        assert_eq!(pixels(&source), original);
    }
}
