use super::*;
use crate::{Channel, CurveInterpolation, CurvePoint, DustMode, EffectSample, PointF32};

fn seeded_core() -> (Core, u64) {
    let mut core = Core::new();
    let created = core.new_cell(4, 1, 96_000, 96_000).unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    for (x, color) in [
        [20, 40, 60, 255],
        [80, 100, 120, 128],
        [160, 180, 200, 255],
        [220, 230, 240, 255],
    ]
    .into_iter()
    .enumerate()
    {
        core.apply_stroke(&Stroke {
            tool: PaintTool::Pencil,
            plane: ActivePlane::Color,
            color,
            diameter: 1.0,
            shape: BrushShape::Round,
            smoothing: 0,
            start_color: StartColorPredicate::Any,
            auto_erase: false,
            pressure_size: false,
            coordinate_space: CoordinateSpace::Document,
            samples: vec![StrokeSample {
                x: x as f32,
                y: 0.0,
                pressure: 1.0,
            }],
        })
        .unwrap();
    }
    (core, created.color_plane_id)
}

#[test]
fn acceptance_cancel_restores_the_original_tile_checksum() {
    let (mut core, plane_id) = seeded_core();
    let original = core.document_info().unwrap().color_plane_checksum;
    let preview = core
        .begin_filter_preview(
            plane_id,
            Filter::Invert {
                channel: Channel::Rgb,
            },
        )
        .unwrap();
    assert_eq!(preview.base_checksum, original);
    assert_ne!(preview.preview_checksum, original);
    assert_ne!(
        core.build_snapshot().revision(),
        core.document_info().unwrap().document_revision
    );
    let cancelled = core.cancel_filter_preview().unwrap();
    assert_eq!(cancelled.preview_checksum, original);
    assert_eq!(core.document_info().unwrap().color_plane_checksum, original);
}

#[test]
fn acceptance_apply_is_exactly_one_undo_unit_and_last_filter_reuses_it() {
    let (mut core, plane_id) = seeded_core();
    let original = core.document_info().unwrap().color_plane_checksum;
    let history = core.history_entries().len();
    core.begin_filter_preview(
        plane_id,
        Filter::BrightnessContrast {
            brightness_milli: 100,
            contrast_milli: 200,
        },
    )
    .unwrap();
    core.apply_filter_preview().unwrap();
    assert_eq!(core.history_entries().len(), history + 1);
    let filtered = core.document_info().unwrap().color_plane_checksum;
    assert_ne!(filtered, original);
    core.undo().unwrap();
    assert_eq!(core.document_info().unwrap().color_plane_checksum, original);
    core.redo().unwrap();
    assert_eq!(core.document_info().unwrap().color_plane_checksum, filtered);
    core.apply_last_filter(plane_id).unwrap();
    assert_eq!(core.history_entries().len(), history + 2);
}

#[test]
fn filter_preview_001_parameter_updates_recompute_from_the_original_base() {
    let (mut updated, plane_id) = seeded_core();
    let original = updated.document_info().unwrap();
    let history = updated.history_entries().len();
    let first = Filter::BrightnessContrast {
        brightness_milli: 250,
        contrast_milli: 0,
    };
    let final_filter = Filter::BrightnessContrast {
        brightness_milli: -150,
        contrast_milli: 400,
    };

    let first_preview = updated
        .begin_filter_preview(plane_id, first.clone())
        .unwrap();
    let final_preview = updated
        .update_filter_preview(plane_id, final_filter.clone())
        .unwrap();
    assert_ne!(
        first_preview.preview_checksum,
        final_preview.preview_checksum
    );
    assert_eq!(updated.document_info().unwrap(), original);
    assert_eq!(updated.history_entries().len(), history);
    let published_preview = updated.build_snapshot().canonical_composite_digest();
    assert!(matches!(
        updated.update_filter_preview(
            plane_id,
            Filter::BrightnessContrast {
                brightness_milli: i32::MIN,
                contrast_milli: 0,
            },
        ),
        Err(CoreError::Raster(_))
    ));
    assert_eq!(
        updated.build_snapshot().canonical_composite_digest(),
        published_preview
    );
    assert_eq!(updated.document_info().unwrap(), original);
    assert_eq!(updated.history_entries().len(), history);

    let (mut direct, direct_plane_id) = seeded_core();
    let direct_preview = direct
        .begin_filter_preview(direct_plane_id, final_filter.clone())
        .unwrap();
    assert_eq!(
        final_preview.preview_checksum,
        direct_preview.preview_checksum
    );

    let (mut cumulative, cumulative_plane_id) = seeded_core();
    cumulative
        .begin_filter_preview(cumulative_plane_id, first)
        .unwrap();
    cumulative.apply_filter_preview().unwrap();
    let cumulative_preview = cumulative
        .begin_filter_preview(cumulative_plane_id, final_filter)
        .unwrap();
    assert_ne!(
        final_preview.preview_checksum,
        cumulative_preview.preview_checksum
    );

    let cancelled = updated.cancel_filter_preview().unwrap();
    assert_eq!(cancelled.preview_checksum, original.color_plane_checksum);
    assert_eq!(updated.document_info().unwrap(), original);
    assert_eq!(updated.history_entries().len(), history);

    updated
        .begin_filter_preview(
            plane_id,
            Filter::BrightnessContrast {
                brightness_milli: 250,
                contrast_milli: 0,
            },
        )
        .unwrap();
    updated
        .update_filter_preview(
            plane_id,
            Filter::BrightnessContrast {
                brightness_milli: -150,
                contrast_milli: 400,
            },
        )
        .unwrap();
    updated.apply_filter_preview().unwrap();
    let committed = updated.document_info().unwrap();
    assert_eq!(
        committed.color_plane_checksum,
        final_preview.preview_checksum
    );
    assert_eq!(updated.history_entries().len(), history + 1);
    updated.undo().unwrap();
    assert_eq!(
        updated.document_info().unwrap().color_plane_checksum,
        original.color_plane_checksum
    );
    updated.redo().unwrap();
    assert_eq!(
        updated.document_info().unwrap().color_plane_checksum,
        final_preview.preview_checksum
    );
}

#[test]
fn acceptance_adjustment_order_changes_composite_without_changing_source_plane() {
    let (mut core, _) = seeded_core();
    let unadjusted = core.build_snapshot().tiles()[0].pixels()[..4].to_vec();
    let original = core.document_info().unwrap().color_plane_checksum;
    let (_, brightness) = core
        .create_adjustment_layer(
            "Brightness",
            Adjustment::BrightnessContrast {
                brightness_milli: 200,
                contrast_milli: 0,
            },
        )
        .unwrap();
    let (_, curve) = core
        .create_adjustment_layer(
            "Curve",
            Adjustment::ToneCurve {
                channel: Channel::Rgb,
                interpolation: CurveInterpolation::Bezier,
                points: vec![
                    CurvePoint {
                        input: 0,
                        output: 0,
                    },
                    CurvePoint {
                        input: 32_768,
                        output: 8_000,
                    },
                    CurvePoint {
                        input: 65_535,
                        output: 65_535,
                    },
                ],
            },
        )
        .unwrap();
    let first = core.build_snapshot().tiles()[0].pixels()[..4].to_vec();
    core.reorder_layer(brightness, 0).unwrap();
    let second = core.build_snapshot().tiles()[0].pixels()[..4].to_vec();
    assert_ne!(first, second);
    assert_eq!(core.document_info().unwrap().color_plane_checksum, original);
    assert!(core.adjustment(curve).is_ok());

    core.set_layer_properties(brightness, true, true, 0, "Brightness")
        .unwrap();
    core.set_layer_properties(curve, false, true, 1_000, "Curve")
        .unwrap();
    assert_eq!(core.build_snapshot().tiles()[0].pixels()[..4], unadjusted);
    core.set_layer_properties(brightness, true, true, 1_000, "Brightness")
        .unwrap();
    core.set_layer_properties(curve, true, true, 1_000, "Curve")
        .unwrap();
    let second = core.build_snapshot().tiles()[0].pixels()[..4].to_vec();

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "inkpod-test-adjustment-{}-{nonce}.inkpod",
        std::process::id()
    ));
    core.save(&path).unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(
        reopened.adjustment(curve).unwrap(),
        core.adjustment(curve).unwrap()
    );
    assert_eq!(reopened.build_snapshot().tiles()[0].pixels()[..4], second);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn acceptance_boundary_airbrush_preserves_uniform_regions() {
    let mut core = Core::new();
    let created = core.new_cell(7, 1, 96_000, 96_000).unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    for x in 0..7 {
        core.apply_stroke(&Stroke {
            tool: PaintTool::Pencil,
            plane: ActivePlane::Color,
            color: if x < 3 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            },
            diameter: 1.0,
            shape: BrushShape::Round,
            smoothing: 0,
            start_color: StartColorPredicate::Any,
            auto_erase: false,
            pressure_size: false,
            coordinate_space: CoordinateSpace::Document,
            samples: vec![StrokeSample {
                x: x as f32,
                y: 0.0,
                pressure: 1.0,
            }],
        })
        .unwrap();
    }
    let left = core.plane_pixel(ActivePlane::Color, 0, 0).unwrap();
    let boundary = core.plane_pixel(ActivePlane::Color, 2, 0).unwrap();
    let right = core.plane_pixel(ActivePlane::Color, 6, 0).unwrap();
    core.apply_boundary_airbrush_to_plane(
        created.color_plane_id,
        &BoundaryAirbrush {
            colors: vec![[65_535, 0, 0, 65_535], [0, 0, 65_535, 65_535]],
            width: 1,
            strength_milli: 1_000,
        },
    )
    .unwrap();
    assert_eq!(core.plane_pixel(ActivePlane::Color, 0, 0).unwrap(), left);
    assert_eq!(core.plane_pixel(ActivePlane::Color, 6, 0).unwrap(), right);
    assert_ne!(
        core.plane_pixel(ActivePlane::Color, 2, 0).unwrap(),
        boundary
    );
}

#[test]
fn generic_adjustment_tree_edits_remain_saveable_and_reject_ambiguous_merge() {
    let (mut core, _) = seeded_core();
    let (_, first) = core
        .create_layer(LayerKind::Adjustment, "Generic Adjustment")
        .unwrap();
    let (_, second) = core.duplicate_layer(first).unwrap();
    assert!(core.adjustment(first).is_ok());
    assert!(core.adjustment(second).is_ok());
    let path = std::env::temp_dir().join(format!(
        "inkpod-core-generic-adjustment-{}-{}.inkpod",
        std::process::id(),
        core.document_info().unwrap().document_revision
    ));
    core.save(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(matches!(
        core.merge_layer_into_below(second),
        Err(CoreError::InvalidArgument(_))
    ));
}

#[test]
fn noop_invalid_and_adjustment_update_history_are_transactional() {
    let (mut core, plane_id) = seeded_core();
    let history = core.history_entries().len();
    core.begin_filter_preview(
        plane_id,
        Filter::BrightnessContrast {
            brightness_milli: 0,
            contrast_milli: 0,
        },
    )
    .unwrap();
    let outcome = core.apply_filter_preview().unwrap();
    assert_eq!(
        outcome.revision(),
        core.document_info().unwrap().document_revision
    );
    assert_eq!(core.history_entries().len(), history);

    assert!(matches!(
        core.begin_filter_preview(
            plane_id,
            Filter::BrightnessContrast {
                brightness_milli: i32::MIN,
                contrast_milli: 0,
            }
        ),
        Err(CoreError::Raster(_))
    ));
    assert_eq!(core.history_entries().len(), history);

    let (_, adjustment_id) = core
        .create_adjustment_layer(
            "Editable",
            Adjustment::BrightnessContrast {
                brightness_milli: 100,
                contrast_milli: 0,
            },
        )
        .unwrap();
    let before_update = core.history_entries().len();
    core.update_adjustment_layer(
        adjustment_id,
        Adjustment::BrightnessContrast {
            brightness_milli: 200,
            contrast_milli: -100,
        },
    )
    .unwrap();
    assert_eq!(core.history_entries().len(), before_update + 1);
    core.undo().unwrap();
    assert_eq!(
        core.adjustment(adjustment_id).unwrap(),
        &Adjustment::BrightnessContrast {
            brightness_milli: 100,
            contrast_milli: 0,
        }
    );
    core.redo().unwrap();
    let updated = Adjustment::BrightnessContrast {
        brightness_milli: 200,
        contrast_milli: -100,
    };
    assert_eq!(core.adjustment(adjustment_id).unwrap(), &updated);
    let after_redo = core.history_entries().len();
    let outcome = core
        .update_adjustment_layer(adjustment_id, updated)
        .unwrap();
    assert_eq!(
        outcome.revision(),
        core.document_info().unwrap().document_revision
    );
    assert_eq!(core.history_entries().len(), after_redo);
}

#[test]
fn full_effect_gestures_dust_and_alpha_are_atomic() {
    let (mut core, plane_id) = seeded_core();
    let original = core.document_info().unwrap().color_plane_checksum;
    let history = core.history_entries().len();
    core.apply_airbrush_gesture_to_plane(
        plane_id,
        &AirbrushGesture {
            samples: vec![
                EffectSample {
                    x_milli: 500,
                    y_milli: 500,
                    pressure_milli: 250,
                },
                EffectSample {
                    x_milli: 3_500,
                    y_milli: 500,
                    pressure_milli: 1_000,
                },
            ],
            radius_milli: 500,
            hardness_milli: 1_000,
            spacing_milli: 500,
            opacity_milli: 1_000,
            fade_milli: 0,
            pressure_size: true,
            pressure_opacity: true,
            continuous_dabs: 1,
            color: [0, 0, 65_535, 65_535],
        },
    )
    .unwrap();
    assert_eq!(core.history_entries().len(), history + 1);
    assert_ne!(core.document_info().unwrap().color_plane_checksum, original);
    core.undo().unwrap();
    assert_eq!(core.document_info().unwrap().color_plane_checksum, original);

    core.apply_blur_tool_to_plane(
        plane_id,
        &SelectionShape::Trace {
            points: vec![PointF32 { x: 1.0, y: 0.5 }, PointF32 { x: 2.0, y: 0.5 }],
            diameter: 2.0,
        },
        1,
        1_000,
    )
    .unwrap();
    assert_eq!(core.history_entries().len(), history + 1);
    core.undo().unwrap();

    let mut alpha_before = Vec::new();
    for x in 0..4 {
        alpha_before.push(
            core.plane_pixel(ActivePlane::Color, x, 0)
                .unwrap()
                .rgba16()
                .unwrap(),
        );
    }
    core.apply_alpha_gradient_to_plane(
        plane_id,
        &Gradient {
            kind: crate::GradientKind::Linear,
            mode: crate::GradientMode::Overwrite,
            start_x_milli: 500,
            start_y_milli: 500,
            end_x_milli: 3_500,
            end_y_milli: 500,
            dither: false,
            stops: vec![
                crate::GradientStop {
                    position_milli: 0,
                    color: [0, 0, 0, 0],
                },
                crate::GradientStop {
                    position_milli: 500,
                    color: [0, 0, 0, 32_768],
                },
                crate::GradientStop {
                    position_milli: 1_000,
                    color: [0, 0, 0, 65_535],
                },
            ],
        },
    )
    .unwrap();
    for (x, before) in alpha_before.into_iter().enumerate() {
        let after = core
            .plane_pixel(ActivePlane::Color, x as u32, 0)
            .unwrap()
            .rgba16()
            .unwrap();
        assert_eq!(&after[..3], &before[..3]);
    }
    assert!(core.journal_state().unwrap().is_complete());
    core.verify_journal_replay().unwrap();
}

#[test]
fn worker_cancel_and_dust_never_commit_partial_results() {
    let (mut core, plane_id) = seeded_core();
    let revision = core.document_info().unwrap().document_revision;
    let history = core.history_entries().len();
    assert!(matches!(
        core.begin_filter_preview_with_progress(plane_id, Filter::AutoContrast, |_, _| false),
        Err(CoreError::Cancelled)
    ));
    assert!(matches!(
        core.cancel_filter_preview(),
        Err(CoreError::InvalidState("there is no active filter preview"))
    ));
    assert_eq!(core.document_info().unwrap().document_revision, revision);
    assert_eq!(core.history_entries().len(), history);

    let checksum = core.document_info().unwrap().color_plane_checksum;
    let mut polls = 0;
    assert!(matches!(
        core.apply_dust_removal_to_plane(
            plane_id,
            Some(&SelectionShape::Rectangle(crate::RectI32 {
                x: 0,
                y: 0,
                width: 4,
                height: 1
            })),
            DustRemoval {
                mode: DustMode::ReplaceColorOutliers,
                maximum_pixels: 1
            },
            |_, _| {
                polls += 1;
                polls < 2
            },
        ),
        Err(CoreError::Cancelled)
    ));
    assert_eq!(core.document_info().unwrap().document_revision, revision);
    assert_eq!(core.history_entries().len(), history);
    assert_eq!(core.document_info().unwrap().color_plane_checksum, checksum);
}
