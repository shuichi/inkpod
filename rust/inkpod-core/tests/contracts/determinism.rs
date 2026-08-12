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
                25, 75, 39, 150, 173, 176, 198, 214, 33, 126, 96, 44, 238, 30, 148, 26, 157, 212,
                74, 37, 189, 53, 103, 148, 60, 237, 132, 53, 241, 171, 14, 242
            ],
            [
                75, 91, 99, 169, 245, 7, 117, 88, 65, 143, 250, 200, 161, 240, 254, 245, 198, 136,
                252, 107, 55, 92, 64, 187, 28, 36, 39, 248, 90, 16, 106, 163
            ],
            [
                67, 125, 9, 250, 228, 95, 74, 108, 115, 151, 76, 57, 164, 101, 222, 61, 25, 140,
                242, 74, 55, 42, 67, 229, 2, 191, 183, 36, 87, 9, 142, 144
            ],
            [
                88, 125, 166, 197, 144, 60, 79, 73, 59, 53, 191, 10, 41, 155, 180, 253, 139, 60,
                145, 147, 64, 1, 23, 239, 138, 121, 63, 65, 24, 145, 102, 63
            ],
            [
                135, 132, 250, 65, 49, 48, 145, 45, 137, 0, 44, 197, 76, 181, 159, 250, 84, 62,
                176, 73, 125, 65, 73, 111, 82, 43, 22, 76, 98, 233, 4, 51
            ],
            [
                25, 206, 191, 214, 143, 29, 95, 151, 227, 96, 165, 83, 47, 173, 72, 221, 107, 76,
                30, 196, 238, 26, 210, 255, 171, 228, 88, 53, 5, 51, 204, 127
            ],
        ]
    );
    assert_eq!(
        composite,
        [
            198, 42, 201, 138, 2, 152, 75, 38, 135, 180, 34, 165, 92, 126, 47, 87, 141, 131, 90,
            191, 15, 85, 237, 138, 191, 220, 172, 109, 2, 233, 179, 245
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
