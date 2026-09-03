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
        // DocumentStateDigest schema 13/domain 11; pixel-composite golden is unchanged.
        vec![
            [
                9, 128, 247, 165, 183, 44, 150, 108, 210, 113, 120, 66, 68, 215, 30, 209, 65, 76,
                112, 5, 136, 247, 103, 120, 29, 178, 241, 103, 48, 147, 45, 130
            ],
            [
                56, 151, 17, 50, 203, 222, 92, 170, 1, 53, 131, 70, 175, 211, 55, 118, 118, 18, 81,
                18, 143, 24, 155, 84, 212, 71, 43, 178, 205, 158, 171, 243
            ],
            [
                102, 253, 226, 74, 180, 87, 67, 82, 30, 64, 218, 228, 176, 49, 104, 62, 39, 21,
                224, 15, 255, 240, 34, 253, 198, 229, 24, 67, 47, 235, 238, 19
            ],
            [
                252, 114, 107, 224, 43, 157, 155, 194, 142, 26, 16, 97, 99, 61, 50, 156, 240, 192,
                122, 82, 33, 45, 174, 102, 143, 6, 214, 57, 80, 124, 215, 120
            ],
            [
                50, 241, 245, 133, 226, 193, 116, 255, 167, 160, 135, 6, 170, 189, 130, 14, 79, 38,
                107, 0, 180, 213, 186, 122, 5, 45, 83, 226, 124, 134, 106, 76
            ],
            [
                243, 85, 242, 88, 111, 179, 31, 178, 83, 95, 135, 249, 236, 230, 14, 240, 102, 181,
                144, 130, 246, 193, 27, 44, 198, 95, 247, 138, 110, 201, 15, 108
            ]
        ]
    );
    assert_eq!(
        composite,
        [
            255, 124, 168, 146, 112, 90, 246, 52, 97, 172, 230, 138, 196, 78, 63, 231, 147, 221,
            33, 64, 153, 107, 55, 228, 83, 189, 129, 245, 173, 98, 57, 87
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
