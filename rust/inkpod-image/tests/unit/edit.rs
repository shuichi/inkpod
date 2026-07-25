use super::*;

fn rgba8(width: u32, height: u32) -> TileRaster {
    TileRaster::new(width, height, PixelFormat::StraightRgba8).unwrap()
}

#[test]
fn acceptance_eight_sixteen_bit_alpha_and_selection_edges_are_golden_fixed() {
    for format in [PixelFormat::StraightRgba8, PixelFormat::StraightRgba16] {
        let mut source = TileRaster::new(3, 1, format).unwrap();
        let left = from_rgba16(format, [10_000, 20_000, 30_000, 0]);
        let middle = from_rgba16(format, [20_000, 30_000, 40_000, 32_768]);
        let right = from_rgba16(format, [30_000, 40_000, 50_000, 65_535]);
        source.set_pixel(0, 0, left, 1).unwrap();
        source.set_pixel(1, 0, middle, 1).unwrap();
        source.set_pixel(2, 0, right, 1).unwrap();
        let mut selection = TileRaster::new(3, 1, PixelFormat::BinaryMask8).unwrap();
        selection
            .set_pixel(1, 0, PixelValue::Binary(255), 1)
            .unwrap();
        let output = apply_filter(
            &source,
            Some(&selection),
            &Filter::Invert {
                channel: Channel::Rgb,
            },
            2,
        )
        .unwrap();
        assert_eq!(output.pixel(0, 0).unwrap(), left);
        assert_eq!(output.pixel(2, 0).unwrap(), right);
        let expected = match format {
            PixelFormat::StraightRgba8 => PixelValue::Rgba([177, 138, 99, 128]),
            PixelFormat::StraightRgba16 => PixelValue::Rgba16([45_535, 35_535, 25_535, 32_768]),
            _ => unreachable!("test uses straight RGBA formats"),
        };
        assert_eq!(output.pixel(1, 0).unwrap(), expected);
    }
}

#[test]
fn invalid_extremes_and_oversized_work_are_rejected_without_panicking() {
    let source = rgba8(2, 2);
    for filter in [
        Filter::BrightnessContrast {
            brightness_milli: i32::MIN,
            contrast_milli: 0,
        },
        Filter::Hsv(HsvAdjustment {
            hue_degrees_milli: i32::MIN,
            saturation_milli: 0,
            value_milli: 0,
        }),
        Filter::ColorBalance(ColorBalance {
            red_milli: i32::MIN,
            green_milli: 0,
            blue_milli: 0,
        }),
    ] {
        assert!(apply_filter(&source, None, &filter, 2).is_err());
    }

    let extreme_gradient = Gradient {
        kind: GradientKind::Linear,
        mode: GradientMode::Overwrite,
        start_x_milli: i64::MIN,
        start_y_milli: i64::MIN,
        end_x_milli: i64::MAX,
        end_y_milli: i64::MAX,
        dither: false,
        stops: vec![
            GradientStop {
                position_milli: 0,
                color: [0; 4],
            },
            GradientStop {
                position_milli: 500,
                color: [32_768; 4],
            },
            GradientStop {
                position_milli: 1_000,
                color: [65_535; 4],
            },
        ],
    };
    assert!(apply_gradient(&source, None, &extreme_gradient, 2).is_ok());
    assert!(
        apply_airbrush(
            &source,
            None,
            AirbrushStroke {
                center_x_milli: i64::MIN,
                center_y_milli: i64::MAX,
                radius_milli: 1_000,
                hardness_milli: 0,
                opacity_milli: 1_000,
                color: [65_535; 4],
            },
            2,
        )
        .is_ok()
    );
    assert_eq!(
        apply_stamp(
            &source,
            None,
            Stamp {
                source_x: i32::MIN,
                source_y: i32::MIN,
                destination_x: i32::MAX,
                destination_y: i32::MAX,
                width: u32::MAX,
                height: u32::MAX,
                opacity_milli: 1_000,
            },
            2,
        )
        .unwrap(),
        source
    );

    let oversized = TileRaster::new(8_193, 8_193, PixelFormat::StraightRgba8).unwrap();
    assert!(apply_filter(&oversized, None, &Filter::AutoContrast, 2).is_err());
    let expensive = TileRaster::new(1_024, 1_024, PixelFormat::StraightRgba8).unwrap();
    assert!(
        apply_filter(
            &expensive,
            None,
            &Filter::GaussianBlur {
                radius: MAX_FILTER_RADIUS,
                strength_milli: 1_000,
            },
            2,
        )
        .is_err()
    );
}

#[test]
fn full_effect_gestures_are_deterministic_and_pressure_aware() {
    let source = rgba8(8, 4);
    let gesture = AirbrushGesture {
        samples: vec![
            EffectSample {
                x_milli: 1_500,
                y_milli: 1_500,
                pressure_milli: 250,
            },
            EffectSample {
                x_milli: 6_500,
                y_milli: 1_500,
                pressure_milli: 1_000,
            },
        ],
        radius_milli: 1_500,
        hardness_milli: 500,
        spacing_milli: 500,
        opacity_milli: 1_000,
        fade_milli: 250,
        pressure_size: true,
        pressure_opacity: true,
        continuous_dabs: 2,
        color: [65_535, 0, 0, 65_535],
    };
    let first = apply_airbrush_gesture(&source, None, &gesture, 2).unwrap();
    let second = apply_airbrush_gesture(&source, None, &gesture, 2).unwrap();
    assert_eq!(first, second);
    assert_ne!(first, source);
    assert!(
        first.pixel(6, 1).unwrap().rgba16().unwrap()[3]
            > first.pixel(1, 1).unwrap().rgba16().unwrap()[3]
    );

    let mut stamp_source = rgba8(8, 4);
    stamp_source
        .set_pixel(1, 1, PixelValue::Rgba([0, 255, 0, 255]), 1)
        .unwrap();
    stamp_source
        .set_pixel(3, 1, PixelValue::Rgba([0, 255, 0, 255]), 1)
        .unwrap();
    let stamped = apply_stamp_gesture(
        &stamp_source,
        None,
        &StampGesture {
            source_x_milli: 1_500,
            source_y_milli: 1_500,
            samples: vec![
                EffectSample {
                    x_milli: 4_500,
                    y_milli: 1_500,
                    pressure_milli: 1_000,
                },
                EffectSample {
                    x_milli: 6_500,
                    y_milli: 1_500,
                    pressure_milli: 500,
                },
            ],
            radius_milli: 600,
            hardness_milli: 1_000,
            spacing_milli: 1_000,
            opacity_milli: 1_000,
            shape: StampShape::Round,
            pressure_size: true,
            pressure_opacity: true,
        },
        2,
    )
    .unwrap();
    assert_eq!(
        stamped.pixel(4, 1).unwrap(),
        PixelValue::Rgba([0, 255, 0, 255])
    );
    assert_ne!(
        stamped.pixel(6, 1).unwrap(),
        stamp_source.pixel(6, 1).unwrap()
    );
}

#[test]
fn paint_003_dust_modes_preview_bounds_and_cancel_are_atomic() {
    let mut point = rgba8(5, 5);
    point
        .set_pixel(2, 2, PixelValue::Rgba([255, 0, 0, 255]), 1)
        .unwrap();
    let removed = apply_dust_removal(
        &point,
        None,
        DustRemoval {
            mode: DustMode::RemoveForeground,
            maximum_pixels: 1,
        },
        2,
        |_, _| true,
    )
    .unwrap();
    assert_eq!(removed.pixel(2, 2).unwrap(), PixelValue::Rgba([0; 4]));

    let mut hole = rgba8(5, 5);
    for y in 1..4 {
        for x in 1..4 {
            hole.set_pixel(x, y, PixelValue::Rgba([20, 40, 60, 255]), 1)
                .unwrap();
        }
    }
    hole.set_pixel(2, 2, PixelValue::Rgba([0; 4]), 1).unwrap();
    let filled = apply_dust_removal(
        &hole,
        None,
        DustRemoval {
            mode: DustMode::FillTransparentHoles,
            maximum_pixels: 1,
        },
        2,
        |_, _| true,
    )
    .unwrap();
    assert_eq!(
        filled.pixel(2, 2).unwrap(),
        PixelValue::Rgba([20, 40, 60, 255])
    );

    let mut outlier = hole.clone();
    outlier
        .set_pixel(2, 2, PixelValue::Rgba([0, 0, 255, 255]), 1)
        .unwrap();
    let replaced = apply_dust_removal(
        &outlier,
        None,
        DustRemoval {
            mode: DustMode::ReplaceColorOutliers,
            maximum_pixels: 1,
        },
        2,
        |_, _| true,
    )
    .unwrap();
    assert_eq!(
        replaced.pixel(2, 2).unwrap(),
        PixelValue::Rgba([20, 40, 60, 255])
    );

    let mut polls = 0;
    assert_eq!(
        apply_dust_removal(
            &outlier,
            None,
            DustRemoval {
                mode: DustMode::ReplaceColorOutliers,
                maximum_pixels: 8
            },
            2,
            |_, _| {
                polls += 1;
                polls < 2
            },
        ),
        Err(RasterError::Cancelled)
    );
    assert_eq!(
        apply_filter_with_progress(&outlier, None, &Filter::AutoContrast, 2, |_, _| false,),
        Err(RasterError::Cancelled)
    );
}

#[test]
fn adjust_001_alpha_gradient_never_changes_rgb() {
    let mut source = rgba8(3, 1);
    for x in 0..3 {
        source
            .set_pixel(x, 0, PixelValue::Rgba([10, 20, 30, 200]), 1)
            .unwrap();
    }
    let output = apply_alpha_gradient(
        &source,
        None,
        &Gradient {
            kind: GradientKind::Linear,
            mode: GradientMode::Overwrite,
            start_x_milli: 500,
            start_y_milli: 500,
            end_x_milli: 2_500,
            end_y_milli: 500,
            dither: false,
            stops: vec![
                GradientStop {
                    position_milli: 0,
                    color: [0, 0, 0, 0],
                },
                GradientStop {
                    position_milli: 500,
                    color: [0, 0, 0, 32_768],
                },
                GradientStop {
                    position_milli: 1_000,
                    color: [0, 0, 0, 65_535],
                },
            ],
        },
        2,
    )
    .unwrap();
    for x in 0..3 {
        assert_eq!(
            &output.pixel(x, 0).unwrap().rgba16().unwrap()[..3],
            &[2_570, 5_140, 7_710]
        );
    }
    assert_eq!(output.pixel(0, 0).unwrap().rgba16().unwrap()[3], 0);
    assert_eq!(output.pixel(2, 0).unwrap().rgba16().unwrap()[3], 65_535);
}

#[test]
fn boundary_effect_never_changes_a_uniform_region() {
    let mut source = rgba8(7, 3);
    for y in 0..3 {
        for x in 0..7 {
            let color = if x < 3 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            source.set_pixel(x, y, PixelValue::Rgba(color), 1).unwrap();
        }
    }
    let output = apply_boundary_airbrush(
        &source,
        None,
        &BoundaryAirbrush {
            colors: vec![[65_535, 0, 0, 65_535], [0, 0, 65_535, 65_535]],
            width: 1,
            strength_milli: 1_000,
        },
        2,
    )
    .unwrap();
    assert_eq!(output.pixel(0, 1).unwrap(), source.pixel(0, 1).unwrap());
    assert_eq!(output.pixel(6, 1).unwrap(), source.pixel(6, 1).unwrap());
    assert_ne!(output.pixel(2, 1).unwrap(), source.pixel(2, 1).unwrap());
    assert_ne!(output.pixel(3, 1).unwrap(), source.pixel(3, 1).unwrap());
}

#[test]
fn filter_catalog_executes_with_bounded_parameters() {
    let mut source = TileRaster::new(3, 2, PixelFormat::StraightRgba16).unwrap();
    for y in 0..2 {
        for x in 0..3 {
            source
                .set_pixel(
                    x,
                    y,
                    PixelValue::Rgba16([
                        (x * 10_000 + y * 2_000) as u16,
                        (x * 5_000 + 10_000) as u16,
                        (y * 20_000 + 5_000) as u16,
                        (20_000 + x * 10_000) as u16,
                    ]),
                    1,
                )
                .unwrap();
        }
    }
    let curve = vec![
        CurvePoint {
            input: 0,
            output: 0,
        },
        CurvePoint {
            input: 32_768,
            output: 40_000,
        },
        CurvePoint {
            input: 65_535,
            output: 65_535,
        },
    ];
    let filters = vec![
        Filter::SharpenWeak,
        Filter::SharpenStrong,
        Filter::BlurWeak,
        Filter::BlurStrong,
        Filter::GaussianBlur {
            radius: 1,
            strength_milli: 500,
        },
        Filter::UnsharpMask {
            radius: 1,
            amount_milli: 1_250,
            threshold: 64,
        },
        Filter::Invert {
            channel: Channel::Green,
        },
        Filter::AutoContrast,
        Filter::BrightnessContrast {
            brightness_milli: 100,
            contrast_milli: -200,
        },
        Filter::ToneCurve {
            channel: Channel::Rgb,
            interpolation: CurveInterpolation::Bezier,
            points: curve.clone(),
        },
        Filter::ToneCurve {
            channel: Channel::Blue,
            interpolation: CurveInterpolation::BSpline,
            points: curve,
        },
        Filter::Levels(Levels {
            channel: Channel::Red,
            input_shadow: 1_000,
            input_gamma_milli: 1_200,
            input_highlight: 64_000,
            output_shadow: 500,
            output_highlight: 65_000,
        }),
        Filter::Hsv(HsvAdjustment {
            hue_degrees_milli: 30_000,
            saturation_milli: 100,
            value_milli: -100,
        }),
        Filter::ColorBalance(ColorBalance {
            red_milli: 50,
            green_milli: -50,
            blue_milli: 100,
        }),
    ];
    for (revision, filter) in filters.iter().enumerate() {
        let output = apply_filter(&source, None, filter, revision as u64 + 2).unwrap();
        assert_eq!(output.format(), PixelFormat::StraightRgba16);
        assert_eq!((output.width(), output.height()), (3, 2));
    }
}

#[test]
fn gradient_airbrush_stamp_and_alpha_edit_are_typed_and_deterministic() {
    let source = rgba8(5, 5);
    let gradient = apply_gradient(
        &source,
        None,
        &Gradient {
            kind: GradientKind::Linear,
            mode: GradientMode::Overwrite,
            start_x_milli: 500,
            start_y_milli: 500,
            end_x_milli: 4_500,
            end_y_milli: 500,
            dither: false,
            stops: vec![
                GradientStop {
                    position_milli: 0,
                    color: [65_535, 0, 0, 65_535],
                },
                GradientStop {
                    position_milli: 500,
                    color: [0, 65_535, 0, 32_768],
                },
                GradientStop {
                    position_milli: 1_000,
                    color: [0, 0, 65_535, 65_535],
                },
            ],
        },
        2,
    )
    .unwrap();
    let sprayed = apply_airbrush(
        &gradient,
        None,
        AirbrushStroke {
            center_x_milli: 2_500,
            center_y_milli: 2_500,
            radius_milli: 2_000,
            hardness_milli: 500,
            opacity_milli: 500,
            color: [65_535; 4],
        },
        3,
    )
    .unwrap();
    let stamped = apply_stamp(
        &sprayed,
        None,
        Stamp {
            source_x: 0,
            source_y: 0,
            destination_x: 3,
            destination_y: 3,
            width: 2,
            height: 2,
            opacity_milli: 1_000,
        },
        4,
    )
    .unwrap();
    assert_ne!(stamped.checksum(), gradient.checksum());

    let mut alpha = TileRaster::new(5, 5, PixelFormat::Grayscale16).unwrap();
    alpha
        .set_pixel(2, 2, PixelValue::Grayscale16(12_345), 1)
        .unwrap();
    let before = stamped.pixel(2, 2).unwrap().rgba16().unwrap();
    let edited = edit_alpha(&stamped, None, &alpha, 5).unwrap();
    let after = edited.pixel(2, 2).unwrap().rgba16().unwrap();
    assert_eq!(&after[..3], &before[..3]);
    assert_eq!(after[3], ((12_345_u32 + 128) / 257 * 257) as u16);
}
