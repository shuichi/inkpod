//! Public-API audit regressions and the subsequently approved line-edit contract.
//! The 2026-09-03 audit remains in docs/line-selection-audit.md. Its five genuine
//! failures retain their assertions; old 4-neighbor/rejection/leak observations
//! are replaced by the explicitly approved new contracts, never ignored.

use inkpod_core::*;
use inkpod_format::{CommonRaster, CommonRasterFormat};
use inkpod_image::{PixelFormat, PixelValue};
use std::collections::BTreeSet;

const CLEAR: [u8; 4] = [0; 4];
const BLACK: [u8; 4] = [0, 0, 0, 255];
const WHITE: [u8; 4] = [255; 4];

fn imported(width: u32, height: u32, pixels: &[[u8; 4]], sixteen: bool) -> Core {
    assert_eq!(pixels.len(), (width * height) as usize);
    let (format, bytes) = if sixteen {
        (
            PixelFormat::StraightRgba16,
            pixels
                .iter()
                .flatten()
                .flat_map(|v| (u16::from(*v) * 257).to_le_bytes())
                .collect(),
        )
    } else {
        (
            PixelFormat::StraightRgba8,
            pixels.iter().flatten().copied().collect(),
        )
    };
    let raster = CommonRaster::new(width, height, format, None, None, bytes).unwrap();
    let mut core = Core::new();
    core.import_decoded_common_raster(CommonRasterFormat::Png, &raster, 0xa0d1)
        .unwrap();
    // The fixture must actually reach the editable source, not just the underlay.
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 0, 0)
            .unwrap()
            .rgba16()
            .unwrap(),
        pixels[0].map(|v| u16::from(v) * 257)
    );
    core
}

fn pixels(core: &Core, plane: ActivePlane) -> Vec<PixelValue> {
    let info = core.document_info().unwrap();
    (0..info.height)
        .flat_map(|y| (0..info.width).map(move |x| core.plane_pixel(plane, x, y).unwrap()))
        .collect()
}

fn mask(core: &mut Core) -> BTreeSet<(u32, u32)> {
    if core.selection_bounds().unwrap().is_none() {
        return BTreeSet::new();
    }
    // Snapshot pixels are a flattened composite, not a raw mask. Export selected
    // coordinates from an opaque probe Color plane on a public COW clone instead.
    // Neither the live source nor its revision/history/editor state is changed.
    let mut probe = core.clone();
    probe.set_active_plane(ActivePlane::Color).unwrap();
    probe
        .apply_gradient_to_plane(
            probe.document_info().unwrap().color_plane_id,
            &Gradient {
                kind: GradientKind::Linear,
                mode: GradientMode::Overwrite,
                start_x_milli: 0,
                start_y_milli: 0,
                end_x_milli: 1000,
                end_y_milli: 0,
                dither: false,
                stops: vec![
                    GradientStop {
                        position_milli: 0,
                        color: [31, 63, 127, 65535],
                    },
                    GradientStop {
                        position_milli: 500,
                        color: [31, 63, 127, 65535],
                    },
                    GradientStop {
                        position_milli: 1000,
                        color: [31, 63, 127, 65535],
                    },
                ],
            },
        )
        .unwrap();
    probe
        .copy_selection()
        .unwrap()
        .planes
        .into_iter()
        .flat_map(|plane| plane.pixels)
        .map(|pixel| (pixel.x as u32, pixel.y as u32))
        .collect()
}

fn rect(x: i32, y: i32, width: i32, height: i32) -> SelectionShape {
    SelectionShape::Rectangle(RectI32 {
        x,
        y,
        width,
        height,
    })
}

fn dust(
    core: &mut Core,
    shape: Option<&SelectionShape>,
    mode: DustMode,
    maximum: u32,
) -> Result<DispatchOutcome, CoreError> {
    core.apply_dust_removal_to_plane(
        core.document_info().unwrap().main_plane_id,
        shape,
        DustRemoval {
            background: Default::default(),
            mode,
            maximum_pixels: maximum,
        },
        |_, _| true,
    )
}

fn changed(before: &[PixelValue], after: &[PixelValue], width: u32) -> BTreeSet<(u32, u32)> {
    before
        .iter()
        .zip(after)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| (i as u32 % width, i as u32 / width))
        .collect()
}

fn persisted(core: &mut Core, name: &str) {
    core.verify_journal_replay().unwrap();
    let line = pixels(core, ActivePlane::MainLine);
    let color = pixels(core, ActivePlane::Color);
    let selection = mask(core);
    let path = std::env::temp_dir().join(format!(
        "inkpod-line-selection-audit-{name}-{}.inkpod",
        std::process::id()
    ));
    core.save(&path).unwrap();
    assert!(!core.document_info().unwrap().dirty);
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(pixels(&reopened, ActivePlane::MainLine), line);
    assert_eq!(pixels(&reopened, ActivePlane::Color), color);
    assert_eq!(mask(&mut reopened), selection);
    reopened.verify_journal_replay().unwrap();
}

#[test]
fn contract_dust_inclusive_pixel_limit_local_band_and_undo() {
    for sixteen in [false, true] {
        let mut fixture = vec![CLEAR; 32 * 32];
        for (x, y) in [
            (4, 8),
            (8, 8),
            (9, 8),
            (10, 8),
            (14, 8),
            (15, 8),
            (16, 8),
            (17, 8),
            (4, 24),
        ] {
            fixture[y * 32 + x] = BLACK;
        }
        for x in 4..24 {
            fixture[12 * 32 + x] = BLACK;
        }
        let mut core = imported(32, 32, &fixture, sixteen);
        let before = pixels(&core, ActivePlane::MainLine);
        let other = pixels(&core, ActivePlane::Color);
        let info = core.document_info().unwrap();
        let history = core.history_entries().len();
        let band = SelectionShape::Trace {
            points: vec![PointF32 { x: 3.5, y: 10.5 }, PointF32 { x: 24.5, y: 10.5 }],
            diameter: 7.0,
        };
        dust(&mut core, Some(&band), DustMode::RemoveForeground, 3).unwrap();
        let after = pixels(&core, ActivePlane::MainLine);
        assert_eq!(
            changed(&before, &after, 32),
            BTreeSet::from([(4, 8), (8, 8), (9, 8), (10, 8)])
        );
        assert_eq!(pixels(&core, ActivePlane::Color), other);
        assert_eq!(core.history_entries().len(), history + 1);
        assert_eq!(
            core.document_info().unwrap().document_revision,
            info.document_revision + 1
        );
        core.undo().unwrap();
        assert_eq!(pixels(&core, ActivePlane::MainLine), before);
        core.redo().unwrap();
        assert_eq!(pixels(&core, ActivePlane::MainLine), after);
        persisted(&mut core, if sixteen { "dust16" } else { "dust8" });
    }
}

#[test]
fn contract_dust_eight_connected_and_selection_intersection() {
    let mut fixture = vec![CLEAR; 32 * 32];
    for (x, y) in [(5, 5), (6, 6), (20, 5)] {
        fixture[y * 32 + x] = BLACK;
    }
    let mut core = imported(32, 32, &fixture, false);
    core.apply_selection(&rect(0, 0, 12, 12), SelectionOperation::New)
        .unwrap();
    let before = pixels(&core, ActivePlane::MainLine);
    let selection = mask(&mut core);
    dust(
        &mut core,
        Some(&rect(0, 0, 32, 32)),
        DustMode::RemoveForeground,
        1,
    )
    .unwrap();
    assert_eq!(
        changed(&before, &pixels(&core, ActivePlane::MainLine), 32),
        BTreeSet::new()
    );
    assert_eq!(mask(&mut core), selection);
    core.clear_selection().unwrap();
    dust(&mut core, None, DustMode::RemoveForeground, 1).unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 20, 5).unwrap(),
        PixelValue::Rgba(CLEAR)
    );
}

#[test]
fn contract_dust_hole_outlier_and_image_edge() {
    for (mode, center) in [
        (DustMode::FillTransparentHoles, CLEAR),
        (DustMode::ReplaceColorOutliers, [255, 0, 0, 255]),
    ] {
        let surrounding = [20, 40, 60, 255];
        let mut fixture = vec![surrounding; 8 * 8];
        fixture[4 * 8 + 4] = center;
        if mode == DustMode::FillTransparentHoles {
            fixture[0] = CLEAR;
        }
        let mut core = imported(8, 8, &fixture, false);
        let before = pixels(&core, ActivePlane::MainLine);
        dust(&mut core, None, mode, 1).unwrap();
        assert_eq!(
            changed(&before, &pixels(&core, ActivePlane::MainLine), 8),
            BTreeSet::from([(4, 4)])
        );
        assert_eq!(
            core.plane_pixel(ActivePlane::MainLine, 4, 4).unwrap(),
            PixelValue::Rgba(surrounding)
        );
    }
}

#[test]
fn contract_dust_region_shapes_and_no_target_noop() {
    let points = vec![
        PointF32 { x: 3.0, y: 3.0 },
        PointF32 { x: 7.0, y: 3.0 },
        PointF32 { x: 7.0, y: 7.0 },
        PointF32 { x: 3.0, y: 7.0 },
    ];
    for shape in [
        rect(3, 3, 4, 4),
        SelectionShape::Polyline(points.clone()),
        SelectionShape::Lasso(points),
        SelectionShape::Trace {
            points: vec![PointF32 { x: 5.5, y: 5.5 }],
            diameter: 3.0,
        },
    ] {
        let mut fixture = vec![CLEAR; 16 * 16];
        fixture[5 * 16 + 5] = BLACK;
        fixture[12 * 16 + 12] = BLACK;
        let mut core = imported(16, 16, &fixture, false);
        let before = pixels(&core, ActivePlane::MainLine);
        dust(&mut core, Some(&shape), DustMode::RemoveForeground, 1).unwrap();
        assert_eq!(
            changed(&before, &pixels(&core, ActivePlane::MainLine), 16),
            BTreeSet::from([(5, 5)])
        );
        let info = core.document_info().unwrap();
        let history = core.history_entries().len();
        dust(&mut core, Some(&shape), DustMode::RemoveForeground, 1).unwrap();
        assert_eq!(core.document_info().unwrap(), info);
        assert_eq!(core.history_entries().len(), history);
    }
}

#[test]
fn contract_dust_invalid_cancel_preview_and_locked_plane_are_atomic() {
    let mut fixture = vec![CLEAR; 16 * 16];
    fixture[5 * 16 + 5] = BLACK;
    let mut core = imported(16, 16, &fixture, false);
    let info = core.document_info().unwrap();
    let before = pixels(&core, ActivePlane::MainLine);
    let history = core.history_entries().len();
    for maximum in [0, 65_537] {
        assert!(dust(&mut core, None, DustMode::RemoveForeground, maximum).is_err());
    }
    assert!(
        core.apply_dust_removal_to_plane(
            u64::MAX,
            None,
            DustRemoval {
                background: Default::default(),
                mode: DustMode::RemoveForeground,
                maximum_pixels: 1
            },
            |_, _| true
        )
        .is_err()
    );
    assert!(matches!(
        core.apply_dust_removal_to_plane(
            info.main_plane_id,
            None,
            DustRemoval {
                background: Default::default(),
                mode: DustMode::RemoveForeground,
                maximum_pixels: 1
            },
            |_, _| false
        ),
        Err(CoreError::Cancelled)
    ));
    let preview = core
        .begin_dust_preview_for_view(
            0,
            CoordinateSpace::Document,
            info.main_plane_id,
            None,
            &[],
            5.0,
            DustRemoval {
                background: Default::default(),
                mode: DustMode::RemoveForeground,
                maximum_pixels: 1,
            },
            |_, _| true,
        )
        .unwrap();
    assert_ne!(preview.base_checksum, preview.preview_checksum);
    assert_eq!(pixels(&core, ActivePlane::MainLine), before);
    core.cancel_filter_preview().unwrap();
    assert_eq!(core.document_info().unwrap(), info);
    assert_eq!(core.history_entries().len(), history);
    core.begin_dust_preview_for_view(
        0,
        CoordinateSpace::Document,
        info.main_plane_id,
        None,
        &[],
        5.0,
        DustRemoval {
            background: Default::default(),
            mode: DustMode::RemoveForeground,
            maximum_pixels: 1,
        },
        |_, _| true,
    )
    .unwrap();
    core.apply_filter_preview().unwrap();
    assert_eq!(core.history_entries().len(), history + 1);
    core.undo().unwrap();
    assert_eq!(pixels(&core, ActivePlane::MainLine), before);
    core.set_plane_properties(info.main_plane_id, true, false, 1_000, "MainLine")
        .unwrap();
    let locked = core.document_info().unwrap();
    assert!(dust(&mut core, None, DustMode::RemoveForeground, 1).is_err());
    assert_eq!(core.document_info().unwrap(), locked);
}

#[test]
fn contract_dust_accepts_empty_binary_and_grayscale_mainline_as_noop() {
    for format in [
        PixelFormat::BinaryMask8,
        PixelFormat::Grayscale8,
        PixelFormat::Grayscale16,
    ] {
        let mut core = Core::new();
        let info = core.new_cell(8, 8, 96_000, 96_000).unwrap();
        core.convert_plane(info.main_plane_id, format).unwrap();
        let before = core.document_info().unwrap();
        dust(&mut core, None, DustMode::RemoveForeground, 1).unwrap();
        assert_eq!(core.document_info().unwrap(), before);
    }
}

#[test]
fn expectation_dust_does_not_cut_a_large_component_at_band_boundary() {
    let mut fixture = vec![CLEAR; 32 * 32];
    for x in 2..30 {
        fixture[16 * 32 + x] = BLACK;
    }
    let mut core = imported(32, 32, &fixture, false);
    let before = pixels(&core, ActivePlane::MainLine);
    dust(
        &mut core,
        Some(&rect(15, 15, 3, 3)),
        DustMode::RemoveForeground,
        3,
    )
    .unwrap();
    assert_eq!(
        changed(&before, &pixels(&core, ActivePlane::MainLine), 32),
        BTreeSet::new(),
        "28-pixel source line must not be misclassified as its 3-pixel selected fragment"
    );
}

#[test]
fn contract_dust_white_background_small_point_is_removed() {
    let mut fixture = vec![WHITE; 32 * 32];
    fixture[16 * 32 + 16] = BLACK;
    let mut core = imported(32, 32, &fixture, false);
    let before = pixels(&core, ActivePlane::MainLine);
    dust(&mut core, None, DustMode::RemoveForeground, 1).unwrap();
    assert_eq!(
        changed(&before, &pixels(&core, ActivePlane::MainLine), 32),
        BTreeSet::from([(16, 16)]),
        "SPEC includes background as well as transparency"
    );
}

#[test]
fn contract_dust_white_background_hole_is_filled() {
    let mut fixture = vec![BLACK; 8 * 8];
    fixture[4 * 8 + 4] = WHITE;
    let mut core = imported(8, 8, &fixture, false);
    dust(&mut core, None, DustMode::FillTransparentHoles, 1).unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 4, 4).unwrap(),
        PixelValue::Rgba(BLACK)
    );
}

#[test]
fn expectation_dust_open_background_is_not_a_hole_when_mask_clips_it() {
    let mut fixture = vec![BLACK; 8 * 8];
    for y in 0..5 {
        fixture[y * 8 + 4] = CLEAR;
    }
    let mut core = imported(8, 8, &fixture, false);
    let before = pixels(&core, ActivePlane::MainLine);
    dust(
        &mut core,
        Some(&rect(4, 4, 1, 1)),
        DustMode::FillTransparentHoles,
        1,
    )
    .unwrap();
    assert_eq!(
        pixels(&core, ActivePlane::MainLine),
        before,
        "source background connects to the image edge outside the mask"
    );
}

fn enclosure(gaps: &[(usize, usize)], sixteen: bool) -> Core {
    let mut fixture = vec![WHITE; 32 * 32];
    for y in 8..=23 {
        for x in 8..=23 {
            if x == 8 || x == 23 || y == 8 || y == 23 {
                fixture[y * 32 + x] = BLACK;
            }
        }
    }
    for &(x, y) in gaps {
        fixture[y * 32 + x] = WHITE;
    }
    imported(32, 32, &fixture, sixteen)
}

fn wand(
    core: &mut Core,
    x: u32,
    y: u32,
    tolerance: u16,
    gap_close: u8,
    operation: SelectionOperation,
) {
    core.apply_selection(
        &SelectionShape::Wand {
            x,
            y,
            tolerance,
            gap_close,
        },
        operation,
    )
    .unwrap();
}

fn interior() -> BTreeSet<(u32, u32)> {
    (9..23).flat_map(|y| (9..23).map(move |x| (x, y))).collect()
}

#[test]
fn contract_wand_closed_connected_set_source_undo_replay_save() {
    for sixteen in [false, true] {
        let mut core = enclosure(&[], sixteen);
        let before = pixels(&core, ActivePlane::MainLine);
        let other = pixels(&core, ActivePlane::Color);
        let history = core.history_entries().len();
        let revision = core.document_info().unwrap().document_revision;
        wand(&mut core, 12, 12, 0, 0, SelectionOperation::New);
        assert_eq!(mask(&mut core), interior());
        assert_eq!(pixels(&core, ActivePlane::MainLine), before);
        assert_eq!(pixels(&core, ActivePlane::Color), other);
        assert_eq!(core.history_entries().len(), history + 1);
        assert_eq!(
            core.document_info().unwrap().document_revision,
            revision + 1
        );
        let info = core.document_info().unwrap();
        wand(&mut core, 12, 12, 0, 0, SelectionOperation::New);
        assert_eq!(core.document_info().unwrap(), info);
        core.undo().unwrap();
        assert!(mask(&mut core).is_empty());
        core.redo().unwrap();
        assert_eq!(mask(&mut core), interior());
        persisted(&mut core, if sixteen { "wand16" } else { "wand8" });
    }
}

#[test]
fn contract_wand_open_control_selects_both_connected_backgrounds() {
    let mut core = enclosure(&[(15, 8)], false);
    let before = pixels(&core, ActivePlane::MainLine);
    wand(&mut core, 12, 12, 0, 0, SelectionOperation::New);
    let expected = (0..32)
        .flat_map(|y| (0..32).map(move |x| (x, y)))
        .filter(|&(x, y)| before[(y * 32 + x) as usize] == PixelValue::Rgba(WHITE))
        .collect();
    assert_eq!(mask(&mut core), expected);
    assert!(mask(&mut core).contains(&(4, 12)));
    assert_eq!(pixels(&core, ActivePlane::MainLine), before);
}

#[test]
fn expectation_wand_gap_close_prevents_escape_through_one_pixel_break() {
    let mut core = enclosure(&[(15, 8)], false);
    let before = pixels(&core, ActivePlane::MainLine);
    wand(&mut core, 12, 12, 0, 1, SelectionOperation::New);
    let selected = mask(&mut core);
    assert_eq!(pixels(&core, ActivePlane::MainLine), before);
    assert!(selected.contains(&(12, 12)));
    assert!(
        !selected.contains(&(4, 12)),
        "gap_close=1 leaked through the one-pixel break at (15,8); selected {} pixels",
        selected.len()
    );
    // The exact treatment of the virtual boundary pixel itself is unspecified.
    assert!(interior().is_subset(&selected));
}

#[test]
fn contract_wand_inclusive_gap_threshold_and_multiple_breaks() {
    for (gaps, gap) in [
        (vec![(15, 8)], 2),
        (vec![(15, 8), (16, 8)], 2),
        (vec![(15, 8), (16, 8), (17, 8)], 2),
        (vec![(15, 8), (8, 15)], 2),
    ] {
        let mut core = enclosure(&gaps, false);
        let before = pixels(&core, ActivePlane::MainLine);
        wand(&mut core, 12, 12, 0, gap, SelectionOperation::New);
        if gaps.len() == 3 {
            let expected = (0..32)
                .flat_map(|y| (0..32).map(move |x| (x, y)))
                .filter(|&(x, y)| before[(y * 32 + x) as usize] == PixelValue::Rgba(WHITE))
                .collect::<BTreeSet<_>>();
            assert_eq!(mask(&mut core), expected);
        } else {
            assert_eq!(mask(&mut core), interior());
        }
        assert_eq!(pixels(&core, ActivePlane::MainLine), before);
    }
}

#[test]
fn contract_wand_tolerance_native_depth_alpha_and_four_connectivity() {
    for sixteen in [false, true] {
        for alpha in [false, true] {
            for tolerance in [256, 257, 258] {
                let base = [40, 40, 40, 200];
                let mut near = base;
                near[if alpha { 3 } else { 0 }] += 1;
                let mut far = base;
                far[if alpha { 3 } else { 0 }] += 2;
                let mut core = imported(4, 1, &[base, near, far, base], sixteen);
                wand(&mut core, 0, 0, tolerance, 0, SelectionOperation::New);
                assert_eq!(
                    mask(&mut core),
                    if tolerance < 257 {
                        BTreeSet::from([(0, 0)])
                    } else {
                        BTreeSet::from([(0, 0), (1, 0)])
                    }
                );
            }
        }
    }
    let mut core = imported(2, 2, &[WHITE, BLACK, BLACK, WHITE], false);
    wand(&mut core, 0, 0, 0, 0, SelectionOperation::New);
    assert_eq!(mask(&mut core), BTreeSet::from([(0, 0)]));
}

#[test]
fn contract_wand_narrow_passage_tile_edge_and_final_pixel() {
    let mut fixture = vec![BLACK; 66 * 3];
    for x in 0..66 {
        fixture[66 + x] = WHITE;
    }
    let mut core = imported(66, 3, &fixture, false);
    let before = pixels(&core, ActivePlane::MainLine);
    wand(&mut core, 65, 1, 0, 0, SelectionOperation::New);
    assert_eq!(mask(&mut core), (0..66).map(|x| (x, 1)).collect());
    assert_eq!(pixels(&core, ActivePlane::MainLine), before);
}

#[test]
fn contract_wand_virtual_boundary_at_tile_edge_and_image_edge_native_planes() {
    for format in [
        PixelFormat::BinaryMask8,
        PixelFormat::Grayscale8,
        PixelFormat::Grayscale16,
        PixelFormat::StraightRgba8,
        PixelFormat::StraightRgba16,
    ] {
        for (left, top) in [(56_u32, 8_u32), (0, 0)] {
            // Scalar conversion retains coverage, so use distinct alpha as well as RGB.
            let mut source = vec![CLEAR; 80 * 32];
            for y in top..=top + 15 {
                for x in left..=left + 15 {
                    if x == left || x == left + 15 || y == top || y == top + 15 {
                        source[(y * 80 + x) as usize] = BLACK;
                    }
                }
            }
            source[(top * 80 + left + 7) as usize] = CLEAR;
            let mut core = imported(80, 32, &source, format == PixelFormat::StraightRgba16);
            let info = core.document_info().unwrap();
            core.convert_plane(info.main_plane_id, format).unwrap();
            let before = pixels(&core, ActivePlane::MainLine);
            assert_ne!(
                before[((top + 1) * 80 + left + 1) as usize],
                before[(top * 80 + left) as usize]
            );
            wand(&mut core, left + 4, top + 4, 0, 1, SelectionOperation::New);
            let expected = (top + 1..top + 15)
                .flat_map(|y| (left + 1..left + 15).map(move |x| (x, y)))
                .collect();
            assert_eq!(
                mask(&mut core),
                expected,
                "{format:?} origin=({left},{top})"
            );
            assert_eq!(pixels(&core, ActivePlane::MainLine), before);
        }
    }
}

fn trace_samples() -> Vec<SelectionSample> {
    [(8.5, 8.5), (8.5, 24.5), (24.5, 24.5)]
        .into_iter()
        .map(|(x, y)| SelectionSample {
            x,
            y,
            pressure: 1.0,
        })
        .collect()
}

fn trace(
    core: &mut Core,
    samples: Vec<SelectionSample>,
    diameter: f32,
    options: TraceBrushOptions,
    operation: SelectionOperation,
) {
    core.apply_selection_with_options(
        &SelectionShape::TraceBrush { samples, diameter },
        operation,
        RangeInterpretation::Normal,
        SelectionConstructionOptions {
            trace: options,
            ..Default::default()
        },
    )
    .unwrap();
}

fn l_band() -> BTreeSet<(u32, u32)> {
    // Independent integer oracle: two five-pixel strips plus three radius-2.5
    // disks. Pixel centers and path centers differ by integral coordinates.
    (0_i32..32)
        .flat_map(|y| (0_i32..32).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            ((6..=10).contains(&x) && (8..=24).contains(&y))
                || ((8..=24).contains(&x) && (22..=26).contains(&y))
                || [(8, 8), (8, 24), (24, 24)]
                    .iter()
                    .any(|&(cx, cy)| (x - cx) * (x - cx) + (y - cy) * (y - cy) <= 6)
        })
        .map(|(x, y)| (x as u32, y as u32))
        .collect()
}

#[test]
fn contract_trace_open_l_exact_band_preserves_source_and_history() {
    let mut fixture = vec![CLEAR; 32 * 32];
    for y in 4..28 {
        fixture[y * 32 + 8] = BLACK;
    }
    let mut core = imported(32, 32, &fixture, false);
    let before = pixels(&core, ActivePlane::MainLine);
    let other = pixels(&core, ActivePlane::Color);
    let history = core.history_entries().len();
    trace(
        &mut core,
        trace_samples(),
        5.0,
        TraceBrushOptions::default(),
        SelectionOperation::New,
    );
    let selected = mask(&mut core);
    assert_eq!(selected, l_band());
    assert!(selected.contains(&(8, 16)));
    assert!(selected.contains(&(16, 24)));
    assert!(!selected.contains(&(16, 16)));
    assert!(!selected.contains(&(28, 4)));
    assert_eq!(pixels(&core, ActivePlane::MainLine), before);
    assert_eq!(pixels(&core, ActivePlane::Color), other);
    assert_eq!(core.history_entries().len(), history + 1);
    core.undo().unwrap();
    assert!(mask(&mut core).is_empty());
    core.redo().unwrap();
    assert_eq!(mask(&mut core), selected);
    persisted(&mut core, "trace");
}

#[test]
fn contract_trace_and_wand_selection_algebra() {
    for use_wand in [false, true] {
        for op in [
            SelectionOperation::New,
            SelectionOperation::Add,
            SelectionOperation::Subtract,
            SelectionOperation::Intersect,
        ] {
            let mut core = enclosure(&[], false);
            core.apply_selection(&rect(0, 0, 16, 16), SelectionOperation::New)
                .unwrap();
            let initial: BTreeSet<_> = (0..16).flat_map(|y| (0..16).map(move |x| (x, y))).collect();
            let candidate = if use_wand { interior() } else { l_band() };
            if use_wand {
                wand(&mut core, 12, 12, 0, 0, op);
            } else {
                trace(
                    &mut core,
                    trace_samples(),
                    5.0,
                    TraceBrushOptions::default(),
                    op,
                );
            }
            let expected = match op {
                SelectionOperation::New => candidate,
                SelectionOperation::Add => initial.union(&candidate).copied().collect(),
                SelectionOperation::Subtract => initial.difference(&candidate).copied().collect(),
                SelectionOperation::Intersect => {
                    initial.intersection(&candidate).copied().collect()
                }
            };
            assert_eq!(mask(&mut core), expected);
        }
    }
}

#[test]
fn contract_trace_round_square_pressure_screen_zoom_and_repeated_samples() {
    for (shape, pressure, screen, zoom, diameter, expected_radius) in [
        (TraceBrushShape::Round, false, false, 1 << 16, 5.0, 2),
        (TraceBrushShape::Square, false, false, 4 << 16, 5.0, 2),
        (TraceBrushShape::Round, true, false, 1 << 16, 10.0, 2),
        (TraceBrushShape::Round, false, true, 2 << 16, 10.0, 2),
    ] {
        let sample = SelectionSample {
            x: 8.5,
            y: 8.5,
            pressure: 0.5,
        };
        let mut core = imported(32, 32, &[CLEAR; 32 * 32], false);
        let options = TraceBrushOptions {
            shape,
            pressure_size: pressure,
            screen_size: screen,
            view_zoom_q16: zoom,
        };
        trace(
            &mut core,
            vec![sample],
            diameter,
            options,
            SelectionOperation::New,
        );
        let expected: BTreeSet<_> = (6_i32..=10)
            .flat_map(|y| (6_i32..=10).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                shape == TraceBrushShape::Square || (x - 8) * (x - 8) + (y - 8) * (y - 8) <= 6
            })
            .map(|(x, y)| (x as u32, y as u32))
            .collect();
        assert_eq!(mask(&mut core), expected, "radius floor={expected_radius}");
        let info = core.document_info().unwrap();
        trace(
            &mut core,
            vec![sample, sample, sample],
            diameter,
            options,
            SelectionOperation::New,
        );
        assert_eq!(core.document_info().unwrap(), info);
    }
    let mut core = imported(32, 32, &[CLEAR; 32 * 32], false);
    for zoom in [1 << 16, 2 << 16, 4 << 16] {
        trace(
            &mut core,
            trace_samples(),
            5.0,
            TraceBrushOptions {
                view_zoom_q16: zoom,
                ..Default::default()
            },
            SelectionOperation::New,
        );
        assert_eq!(mask(&mut core), l_band());
    }
}

#[test]
fn contract_trace_clipped_cross_and_invalid_stale_target_atomicity() {
    let mut core = imported(32, 32, &[CLEAR; 32 * 32], false);
    let samples = [
        (-4.5, 8.5),
        (36.5, 8.5),
        (8.5, 8.5),
        (8.5, -4.5),
        (8.5, 36.5),
    ]
    .into_iter()
    .map(|(x, y)| SelectionSample {
        x,
        y,
        pressure: 1.0,
    })
    .collect();
    trace(
        &mut core,
        samples,
        1.0,
        TraceBrushOptions::default(),
        SelectionOperation::New,
    );
    let expected = (0..32)
        .map(|x| (x, 8))
        .chain((0..32).map(|y| (8, y)))
        .collect();
    assert_eq!(mask(&mut core), expected);
    let before = core.document_info().unwrap();
    let previous = mask(&mut core);
    let history = core.history_entries().len();
    for shape in [
        SelectionShape::TraceBrush {
            samples: vec![],
            diameter: 5.0,
        },
        SelectionShape::TraceBrush {
            samples: trace_samples(),
            diameter: f32::NAN,
        },
        SelectionShape::TraceBrush {
            samples: trace_samples(),
            diameter: 4097.0,
        },
        SelectionShape::Wand {
            x: 32,
            y: 0,
            tolerance: 0,
            gap_close: 0,
        },
        SelectionShape::Wand {
            x: 0,
            y: 0,
            tolerance: 0,
            gap_close: 65,
        },
    ] {
        assert!(
            core.apply_selection(&shape, SelectionOperation::New)
                .is_err()
        );
        assert_eq!(core.document_info().unwrap(), before);
        assert_eq!(mask(&mut core), previous);
        assert_eq!(core.history_entries().len(), history);
    }
    assert!(
        core.apply_selection_for_editor_target(
            &rect(0, 0, 1, 1),
            SelectionOperation::New,
            EditorTarget {
                layer_id: u64::MAX,
                plane_id: u64::MAX
            }
        )
        .is_err()
    );
    assert_eq!(core.document_info().unwrap(), before);
    assert_eq!(mask(&mut core), previous);
}

#[test]
fn audit_mask_reader_is_independent_of_source_and_does_not_mutate_live_state() {
    for background in [WHITE, CLEAR, [0, 160, 255, 64]] {
        let mut core = imported(4, 4, &[background; 16], false);
        assert!(mask(&mut core).is_empty());
        core.apply_selection(&rect(1, 2, 1, 1), SelectionOperation::New)
            .unwrap();
        let info = core.document_info().unwrap();
        let history = core.history_entries().len();
        let before = pixels(&core, ActivePlane::MainLine);
        assert_eq!(mask(&mut core), BTreeSet::from([(1, 2)]));
        assert_eq!(core.document_info().unwrap(), info);
        assert_eq!(core.history_entries().len(), history);
        assert_eq!(pixels(&core, ActivePlane::MainLine), before);
    }
}

#[test]
fn contract_wand_rgba16_low_bits_are_not_quantized_to_eight_bits() {
    for tolerance in [0, 1, 2] {
        let channels: [u16; 12] = [
            1000, 2000, 3000, 65535, 1001, 2000, 3000, 65535, 1002, 2000, 3000, 65535,
        ];
        let raster = CommonRaster::new(
            3,
            1,
            PixelFormat::StraightRgba16,
            None,
            None,
            channels.into_iter().flat_map(u16::to_le_bytes).collect(),
        )
        .unwrap();
        let mut core = Core::new();
        core.import_decoded_common_raster(CommonRasterFormat::Png, &raster, 0xa0d2)
            .unwrap();
        let before = pixels(&core, ActivePlane::MainLine);
        wand(&mut core, 0, 0, tolerance, 0, SelectionOperation::New);
        assert_eq!(
            mask(&mut core),
            (0..=u32::from(tolerance)).map(|x| (x, 0)).collect()
        );
        assert_eq!(pixels(&core, ActivePlane::MainLine), before);
    }
}

#[test]
fn contract_trace_pressure_interpolates_sparse_samples() {
    let mut core = imported(32, 32, &[CLEAR; 1024], false);
    trace(
        &mut core,
        vec![
            SelectionSample {
                x: 8.5,
                y: 16.5,
                pressure: 0.2,
            },
            SelectionSample {
                x: 24.5,
                y: 16.5,
                pressure: 1.0,
            },
        ],
        5.0,
        TraceBrushOptions {
            pressure_size: true,
            ..Default::default()
        },
        SelectionOperation::New,
    );
    let selected = mask(&mut core);
    for (x, rows) in [
        (8, vec![16]),
        (16, vec![15, 16, 17]),
        (24, vec![14, 15, 16, 17, 18]),
    ] {
        assert_eq!(
            selected
                .iter()
                .filter(|&&(px, _)| px == x)
                .map(|&(_, y)| y)
                .collect::<Vec<_>>(),
            rows
        );
    }
}

fn correction_request(
    core: &Core,
    correction: LineCorrection,
    region: Option<SelectionShape>,
) -> LineCorrectionRequest {
    LineCorrectionRequest {
        plane_id: core.document_info().unwrap().main_plane_id,
        region,
        construction: SelectionConstructionOptions::default(),
        correction,
    }
}

#[test]
fn contract_line_connection_region_preview_native_depth_history_and_replay() {
    for format in [
        PixelFormat::BinaryMask8,
        PixelFormat::Grayscale8,
        PixelFormat::Grayscale16,
        PixelFormat::StraightRgba8,
        PixelFormat::StraightRgba16,
    ] {
        for (dx, dy) in [(1, 0), (0, 1), (1, 1)] {
            for gap in [1, 2, 3] {
                let mut fixture = vec![CLEAR; 32 * 32];
                for n in 3..=24 {
                    if !(10..12).contains(&n) {
                        fixture[((3 + n * dy) * 32 + 3 + n * dx) as usize] = [20, 40, 60, 173];
                    }
                }
                // An identical remote gap is excluded by the operation band.
                if dy == 0 {
                    for x in 6..28 {
                        if !(13..15).contains(&x) {
                            fixture[27 * 32 + x] = BLACK;
                        }
                    }
                }
                let mut core = imported(32, 32, &fixture, format == PixelFormat::StraightRgba16);
                core.convert_plane(core.document_info().unwrap().main_plane_id, format)
                    .unwrap();
                let before = pixels(&core, ActivePlane::MainLine);
                let color = pixels(&core, ActivePlane::Color);
                let info = core.document_info().unwrap();
                let history = core.history_entries().len();
                let request = correction_request(
                    &core,
                    LineCorrection::Connect {
                        gap,
                        width: 1,
                        background: LineBackground::PlaneDefault,
                    },
                    Some(rect(0, 0, 27, 26)),
                );
                core.begin_line_correction_preview(&request, |_, _| true)
                    .unwrap();
                assert_eq!(pixels(&core, ActivePlane::MainLine), before);
                assert_eq!(core.document_info().unwrap(), info);
                core.cancel_filter_preview().unwrap();
                assert_eq!(core.history_entries().len(), history);
                core.begin_line_correction_preview(&request, |_, _| true)
                    .unwrap();
                core.apply_filter_preview().unwrap();
                let after = pixels(&core, ActivePlane::MainLine);
                let expected = if gap >= 2 {
                    BTreeSet::from([(3 + 10 * dx, 3 + 10 * dy), (3 + 11 * dx, 3 + 11 * dy)])
                } else {
                    BTreeSet::new()
                };
                assert_eq!(
                    changed(&before, &after, 32),
                    expected,
                    "{format:?}, {dx},{dy}, gap={gap}"
                );
                let native_ink = before[((3 + 9 * dy) * 32 + 3 + 9 * dx) as usize];
                for &(x, y) in &expected {
                    assert_eq!(after[(y * 32 + x) as usize], native_ink);
                }
                assert_eq!(pixels(&core, ActivePlane::Color), color);
                assert_eq!(
                    core.history_entries().len(),
                    history + usize::from(gap >= 2)
                );
                if gap >= 2 {
                    core.undo().unwrap();
                    assert_eq!(pixels(&core, ActivePlane::MainLine), before);
                    core.redo().unwrap();
                    assert_eq!(pixels(&core, ActivePlane::MainLine), after);
                    persisted(&mut core, &format!("connect-{format:?}-{dx}-{dy}-{gap}"));
                } else {
                    assert_eq!(core.document_info().unwrap(), info);
                }
            }
        }
    }
}

#[test]
fn contract_line_width_local_cross_sections_native_depth_and_one_undo() {
    for format in [
        PixelFormat::BinaryMask8,
        PixelFormat::Grayscale8,
        PixelFormat::Grayscale16,
        PixelFormat::StraightRgba8,
        PixelFormat::StraightRgba16,
    ] {
        for width in [1_u32, 3, 7] {
            for (mode, amount, expected_width) in [
                (LineWidthMode::Thicken, 1, width + 2),
                (LineWidthMode::Thin, 1, width.saturating_sub(2)),
                (LineWidthMode::Uniform, 3, 3),
            ] {
                let mut fixture = vec![CLEAR; 32 * 32];
                for x in 2..30 {
                    for y in 16 - width / 2..=16 + width / 2 {
                        fixture[(y * 32 + x) as usize] = [20, 40, 60, 173];
                    }
                }
                for x in 2..30 {
                    fixture[28 * 32 + x] = BLACK;
                }
                let mut core = imported(32, 32, &fixture, format == PixelFormat::StraightRgba16);
                core.convert_plane(core.document_info().unwrap().main_plane_id, format)
                    .unwrap();
                core.apply_selection(&rect(8, 0, 8, 32), SelectionOperation::New)
                    .unwrap();
                let before = pixels(&core, ActivePlane::MainLine);
                let color = pixels(&core, ActivePlane::Color);
                let selection = mask(&mut core);
                let history = core.history_entries().len();
                let request = correction_request(
                    &core,
                    LineCorrection::Width {
                        mode,
                        amount,
                        background: LineBackground::PlaneDefault,
                    },
                    Some(rect(0, 8, 24, 16)),
                );
                core.apply_line_correction(&request, |_, _| true).unwrap();
                let after = pixels(&core, ActivePlane::MainLine);
                let actual = (0..24)
                    .filter(|&y| !after[(y * 32 + 12) as usize].is_transparent())
                    .collect::<Vec<_>>();
                let expected = if expected_width == 0 {
                    vec![]
                } else {
                    (16 - expected_width / 2..=16 + expected_width / 2).collect()
                };
                assert_eq!(actual, expected, "{format:?} {mode:?} width={width}");
                for y in 0..32 {
                    for x in 0..32 {
                        if !(8..16).contains(&x) || !(8..24).contains(&y) {
                            assert_eq!(after[y * 32 + x], before[y * 32 + x]);
                        }
                    }
                }
                for y in expected {
                    assert_eq!(after[(y * 32 + 12) as usize], before[16 * 32 + 12]);
                }
                assert_eq!(pixels(&core, ActivePlane::Color), color);
                assert_eq!(mask(&mut core), selection);
                if after != before {
                    assert_eq!(core.history_entries().len(), history + 1);
                    core.undo().unwrap();
                    assert_eq!(pixels(&core, ActivePlane::MainLine), before);
                    core.redo().unwrap();
                    assert_eq!(pixels(&core, ActivePlane::MainLine), after);
                    persisted(&mut core, &format!("width-{format:?}-{mode:?}-{width}"));
                } else {
                    assert_eq!(core.history_entries().len(), history);
                }
            }
        }
    }
}

#[test]
fn contract_line_correction_invalid_cancel_locked_and_noop_are_atomic() {
    let mut fixture = vec![CLEAR; 32 * 32];
    for x in 3..27 {
        fixture[16 * 32 + x] = BLACK;
    }
    let mut core = imported(32, 32, &fixture, false);
    let before = pixels(&core, ActivePlane::MainLine);
    let info = core.document_info().unwrap();
    let history = core.history_entries().len();
    let request = correction_request(
        &core,
        LineCorrection::Width {
            mode: LineWidthMode::Thicken,
            amount: 1,
            background: LineBackground::PlaneDefault,
        },
        None,
    );
    let mut calls = 0;
    assert!(matches!(
        core.apply_line_correction(&request, |_, _| {
            calls += 1;
            calls < 18
        }),
        Err(CoreError::Cancelled)
    ));
    assert!(matches!(
        core.begin_line_correction_preview(&request, |_, _| false),
        Err(CoreError::Cancelled)
    ));
    for correction in [
        LineCorrection::Connect {
            gap: 65,
            width: 1,
            background: LineBackground::Transparent,
        },
        LineCorrection::Connect {
            gap: 1,
            width: 0,
            background: LineBackground::Transparent,
        },
        LineCorrection::Width {
            mode: LineWidthMode::Thin,
            amount: 0,
            background: LineBackground::Transparent,
        },
        LineCorrection::Width {
            mode: LineWidthMode::Uniform,
            amount: 257,
            background: LineBackground::Transparent,
        },
    ] {
        let mut invalid = request.clone();
        invalid.correction = correction;
        assert!(core.apply_line_correction(&invalid, |_, _| true).is_err());
    }
    let mut invalid = request.clone();
    invalid.plane_id = u64::MAX;
    assert!(core.apply_line_correction(&invalid, |_, _| true).is_err());
    let mut noop = request.clone();
    noop.region = Some(rect(0, 0, 3, 3));
    core.apply_line_correction(&noop, |_, _| true).unwrap();
    noop.correction = LineCorrection::Connect {
        gap: 3,
        width: 1,
        background: LineBackground::PlaneDefault,
    };
    noop.region = None;
    core.apply_line_correction(&noop, |_, _| true).unwrap();
    assert_eq!(pixels(&core, ActivePlane::MainLine), before);
    assert_eq!(core.document_info().unwrap(), info);
    assert_eq!(core.history_entries().len(), history);
    core.set_plane_properties(info.main_plane_id, true, false, 1000, "locked")
        .unwrap();
    let locked = core.document_info().unwrap();
    assert!(core.apply_line_correction(&request, |_, _| true).is_err());
    assert_eq!(core.document_info().unwrap(), locked);
}

#[test]
fn contract_line_correction_work_budget_failure_publishes_nothing() {
    let mut core = imported(256, 256, &[CLEAR; 256 * 256], false);
    let info = core.document_info().unwrap();
    let history = core.history_entries().len();
    let request = correction_request(
        &core,
        LineCorrection::Width {
            mode: LineWidthMode::Thicken,
            amount: 256,
            background: LineBackground::PlaneDefault,
        },
        None,
    );
    // Valid parameter, but 256² * 513² exceeds the fixed work budget before raster writes.
    assert!(core.apply_line_correction(&request, |_, _| true).is_err());
    assert_eq!(core.document_info().unwrap(), info);
    assert_eq!(core.history_entries().len(), history);
    assert!(
        pixels(&core, ActivePlane::MainLine)
            .into_iter()
            .all(|p| p.is_transparent())
    );
    core.verify_journal_replay().unwrap();
}

#[test]
fn contract_line_rectangle_uses_canonical_selection_geometry_at_float_boundary() {
    let mut fixture = vec![CLEAR; 32 * 32];
    for x in 3..27 {
        fixture[24 * 32 + x] = BLACK;
    }
    let mut core = imported(32, 32, &fixture, false);
    let before = pixels(&core, ActivePlane::MainLine);
    // Device 769.349976, pan 179.4 and zoom 49.1625 produce this representable f32.
    // Existing canonical selection geometry rounds to Q16 before selecting pixel centers.
    let region = core
        .line_correction_region_for_view(
            0,
            CoordinateSpace::Document,
            EffectRegionKind::Rectangle,
            &[
                StrokeSample {
                    x: 11.999_999,
                    y: 22.0,
                    pressure: 1.0,
                },
                StrokeSample {
                    x: 16.0,
                    y: 27.0,
                    pressure: 1.0,
                },
            ],
            5.0,
        )
        .unwrap();
    let request = correction_request(
        &core,
        LineCorrection::Width {
            mode: LineWidthMode::Thicken,
            amount: 1,
            background: LineBackground::PlaneDefault,
        },
        Some(region),
    );
    core.apply_line_correction(&request, |_, _| true).unwrap();
    let expected = (12..16).flat_map(|x| [(x, 23), (x, 25)]).collect();
    assert_eq!(
        changed(&before, &pixels(&core, ActivePlane::MainLine), 32),
        expected
    );
}
