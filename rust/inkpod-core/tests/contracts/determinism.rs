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
                236, 6, 143, 195, 35, 91, 225, 71, 79, 137, 250, 222, 201, 205, 250, 255, 252, 150,
                78, 179, 161, 61, 194, 55, 203, 165, 180, 67, 179, 239, 112, 36
            ],
            [
                12, 135, 42, 42, 130, 44, 175, 94, 13, 44, 246, 100, 222, 17, 184, 97, 141, 12, 22,
                30, 233, 77, 249, 249, 81, 209, 7, 99, 221, 43, 240, 72
            ],
            [
                88, 26, 254, 248, 88, 76, 252, 58, 80, 212, 182, 181, 55, 80, 4, 49, 155, 241, 245,
                92, 42, 191, 233, 10, 130, 103, 65, 125, 240, 6, 240, 60
            ],
            [
                109, 169, 227, 182, 210, 72, 166, 171, 60, 237, 177, 219, 4, 132, 34, 127, 87, 166,
                27, 222, 104, 185, 217, 79, 121, 207, 51, 211, 189, 16, 249, 155
            ],
            [
                93, 11, 106, 33, 252, 151, 189, 254, 167, 235, 195, 1, 244, 5, 129, 35, 113, 6, 77,
                65, 215, 191, 122, 71, 48, 21, 85, 10, 22, 141, 84, 66
            ],
            [
                10, 208, 72, 186, 142, 81, 33, 0, 55, 82, 153, 74, 166, 66, 200, 204, 113, 42, 24,
                169, 148, 158, 137, 85, 173, 227, 37, 117, 77, 224, 79, 200
            ],
        ]
    );
    assert_eq!(
        composite,
        [
            233, 81, 112, 164, 88, 214, 33, 245, 6, 232, 222, 5, 209, 109, 152, 109, 106, 198, 83,
            67, 3, 135, 147, 117, 192, 108, 219, 189, 103, 167, 68, 74
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
