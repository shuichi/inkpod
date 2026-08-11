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
                147, 201, 144, 158, 5, 75, 243, 31, 72, 123, 13, 186, 99, 168, 52, 136, 40, 149,
                57, 213, 185, 204, 183, 113, 56, 168, 104, 241, 110, 168, 211, 118
            ],
            [
                23, 143, 200, 159, 200, 122, 135, 60, 15, 29, 112, 170, 190, 207, 195, 188, 58,
                165, 181, 133, 197, 53, 209, 124, 238, 232, 37, 33, 215, 252, 21, 81
            ],
            [
                169, 18, 191, 192, 149, 45, 10, 48, 96, 32, 198, 219, 150, 163, 24, 121, 134, 165,
                93, 56, 151, 124, 241, 169, 140, 242, 170, 255, 200, 147, 66, 194
            ],
            [
                150, 246, 162, 231, 221, 184, 95, 44, 233, 80, 175, 255, 209, 184, 139, 104, 218,
                197, 111, 235, 199, 94, 69, 151, 140, 254, 209, 170, 188, 14, 237, 207
            ],
            [
                53, 178, 26, 215, 1, 224, 67, 95, 162, 13, 22, 68, 228, 195, 216, 226, 168, 223,
                17, 232, 72, 135, 182, 7, 235, 63, 179, 106, 237, 160, 141, 99
            ],
            [
                102, 184, 12, 107, 153, 136, 182, 243, 61, 108, 207, 81, 85, 47, 28, 203, 29, 109,
                44, 193, 180, 44, 18, 43, 137, 131, 185, 4, 231, 64, 199, 27
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
