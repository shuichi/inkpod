use super::*;
use std::path::Path;

const DETERMINISM_FIXTURE_UUID: u128 = 0x494e_4b50_4f44_2d4d_372d_474f_4c44_454e;

fn record_boundary(core: &Core, boundaries: &mut Vec<[u8; 32]>) {
    let replay = core
        .verify_journal_replay()
        .expect("determinism fixture must replay");
    let live = core
        .document_state_digest()
        .expect("determinism fixture has a document");
    assert_eq!(replay.document_state_digest(), live);
    boundaries.push(*live.as_bytes());
}

#[test]
fn fixed_math_procedure_boundaries_and_composite_are_cross_arch_golden() {
    let mut core = Core::new();
    let created = core
        .new_cell_with_uuid(
            16,
            16,
            DEFAULT_DPI_MILLI,
            DEFAULT_DPI_MILLI,
            DETERMINISM_FIXTURE_UUID,
        )
        .unwrap();
    let mut boundaries = Vec::new();
    record_boundary(&core, &mut boundaries);

    core.apply_stroke(&Stroke {
        tool: PaintTool::Brush,
        plane: ActivePlane::Color,
        color: [17, 43, 91, 219],
        diameter: 3.375,
        shape: BrushShape::Round,
        smoothing: 0,
        start_color: StartColorPredicate::Any,
        auto_erase: false,
        pressure_size: true,
        coordinate_space: CoordinateSpace::Document,
        samples: vec![
            StrokeSample {
                x: 1.125,
                y: 2.875,
                pressure: 0.375,
            },
            StrokeSample {
                x: 13.625,
                y: 5.25,
                pressure: 0.9375,
            },
        ],
    })
    .unwrap();
    record_boundary(&core, &mut boundaries);

    core.apply_gradient_to_plane(
        created.color_plane_id,
        &Gradient {
            kind: GradientKind::Radial,
            mode: GradientMode::Composite,
            start_x_milli: 3_250,
            start_y_milli: 4_750,
            end_x_milli: 12_125,
            end_y_milli: 9_375,
            dither: true,
            stops: vec![
                GradientStop {
                    position_milli: 0,
                    color: [1_000, 2_000, 3_000, 50_000],
                },
                GradientStop {
                    position_milli: 425,
                    color: [12_345, 23_456, 34_567, 40_000],
                },
                GradientStop {
                    position_milli: 1_000,
                    color: [60_000, 40_000, 20_000, 30_000],
                },
            ],
        },
    )
    .unwrap();
    record_boundary(&core, &mut boundaries);

    core.apply_blur_to_plane(created.color_plane_id, 2, 725)
        .unwrap();
    record_boundary(&core, &mut boundaries);

    core.begin_filter_preview(
        created.color_plane_id,
        Filter::Levels(Levels {
            channel: Channel::Rgb,
            input_shadow: 321,
            input_gamma_milli: 1_375,
            input_highlight: 64_123,
            output_shadow: 777,
            output_highlight: 63_999,
        }),
    )
    .unwrap();
    core.apply_filter_preview().unwrap();
    record_boundary(&core, &mut boundaries);

    core.apply_airbrush_to_plane(
        created.color_plane_id,
        AirbrushStroke {
            center_x_milli: 7_625,
            center_y_milli: 10_375,
            radius_milli: 3_125,
            hardness_milli: 375,
            opacity_milli: 625,
            color: [50_000, 5_000, 42_000, 55_000],
        },
    )
    .unwrap();
    record_boundary(&core, &mut boundaries);

    let composite = core
        .build_snapshot()
        .canonical_composite_digest()
        .unwrap()
        .as_bytes();
    assert_eq!(
        boundaries,
        vec![
            [
                62, 123, 140, 170, 23, 54, 4, 174, 184, 50, 252, 168, 180, 139, 11, 66, 242, 218,
                242, 217, 60, 160, 47, 46, 220, 48, 182, 71, 24, 105, 43, 37
            ],
            [
                49, 88, 89, 21, 58, 180, 65, 85, 220, 214, 83, 81, 163, 45, 151, 163, 6, 186, 93,
                206, 50, 243, 100, 10, 250, 139, 78, 81, 131, 159, 128, 182
            ],
            [
                18, 64, 55, 76, 119, 202, 69, 85, 47, 217, 199, 71, 224, 33, 0, 37, 232, 114, 86,
                72, 50, 40, 134, 137, 173, 189, 119, 127, 200, 255, 167, 143
            ],
            [
                61, 78, 1, 233, 81, 171, 51, 227, 173, 131, 86, 228, 212, 83, 27, 69, 16, 22, 156,
                98, 209, 14, 23, 54, 18, 222, 203, 0, 34, 101, 191, 217
            ],
            [
                99, 107, 197, 9, 59, 4, 127, 86, 231, 178, 14, 70, 19, 5, 254, 48, 142, 9, 174,
                170, 144, 127, 21, 106, 191, 88, 226, 93, 6, 174, 250, 204
            ],
            [
                25, 50, 213, 253, 100, 147, 44, 135, 55, 212, 86, 19, 53, 233, 50, 132, 10, 217,
                212, 170, 79, 61, 195, 210, 124, 193, 228, 202, 222, 4, 45, 146
            ],
        ]
    );
    assert_eq!(
        composite,
        [
            126, 136, 50, 103, 91, 70, 211, 112, 171, 146, 89, 210, 114, 237, 112, 173, 223, 95,
            79, 151, 174, 21, 134, 2, 11, 100, 189, 49, 140, 76, 5, 41
        ]
    );
}

#[test]
fn production_sources_have_no_platform_transcendental_image_path() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("Core crate is below the repository root");
    let prohibited = [
        ".exp(",
        ".powf(",
        ".sin(",
        ".cos(",
        ".sin_cos(",
        ".hypot(",
        ".sqrt(",
        ".atan2(",
        ".to_radians(",
        "std::f32::consts",
        "std::f64::consts",
    ];
    for root in [
        "rust/inkpod-core/src",
        "rust/inkpod-image/src",
        "rust/inkpod-ffi/src",
    ] {
        let mut pending = vec![repository.join(root)];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(&directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|extension| extension == "rs") {
                    let source = std::fs::read_to_string(&path).unwrap();
                    for needle in prohibited {
                        assert!(
                            !source.contains(needle),
                            "platform transcendental guard found {needle:?} in {}",
                            path.display()
                        );
                    }
                }
            }
        }
    }
}
