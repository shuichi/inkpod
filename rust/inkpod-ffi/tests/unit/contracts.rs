use super::*;
use std::collections::BTreeSet;
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[test]
fn shooting_frame_abi_validates_numeric_input_and_borrows_snapshot_records() {
    let (mut core, initial) = create_core(32, 24, 0x5f20);
    unsafe {
        let mut input = InkpodShootingFrameInput {
            struct_size: size_of::<InkpodShootingFrameInput>() as u32,
            anchor: INKPOD_SHOOTING_FRAME_ANCHOR_CENTER,
            feature_flags: INKPOD_FEATURE_NONE,
            center_x: 10.0,
            center_y: 8.0,
            width: 30.0,
            height: 18.0,
            rotation_degrees: 33.75,
            visible: 1,
            include_in_instruction_export: 1,
        };
        let mut revision = 0;
        let mut frame_id = 0;
        assert_eq!(
            inkpod_core_shooting_frame_edit(
                core,
                initial.document_revision,
                INKPOD_SHOOTING_FRAME_EDIT_CREATE,
                0,
                &input,
                &mut revision,
                &mut frame_id,
            ),
            INKPOD_STATUS_OK
        );
        assert_ne!(frame_id, 0);

        let mut present = 0;
        let mut frame = InkpodShootingFrameInfo {
            struct_size: size_of::<InkpodShootingFrameInfo>() as u32,
            ..InkpodShootingFrameInfo::default()
        };
        assert_eq!(
            inkpod_core_shooting_frame_get(core, &mut present, &mut frame),
            INKPOD_STATUS_OK
        );
        assert_eq!(present, 1);
        assert_eq!(frame.frame_id, frame_id);
        assert_eq!(frame.center_x_milli, 10_000);
        assert_eq!(frame.corners.len(), 4);

        let stable = queried_document_info(core);
        input.rotation_degrees = f64::NAN;
        assert_eq!(
            inkpod_core_shooting_frame_edit(
                core,
                stable.document_revision,
                INKPOD_SHOOTING_FRAME_EDIT_UPDATE,
                frame_id,
                &input,
                &mut revision,
                &mut frame_id,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        input.rotation_degrees = 0.0;
        input.anchor = u32::MAX;
        assert_eq!(
            inkpod_core_shooting_frame_edit(
                core,
                stable.document_revision,
                INKPOD_SHOOTING_FRAME_EDIT_UPDATE,
                frame_id,
                &input,
                &mut revision,
                &mut frame_id,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            document_observation(&queried_document_info(core)),
            document_observation(&stable)
        );
        input.anchor = INKPOD_SHOOTING_FRAME_ANCHOR_BOTTOM_RIGHT;

        assert_eq!(
            inkpod_core_shooting_frame_preview_begin(
                core,
                stable.document_revision,
                INKPOD_SHOOTING_FRAME_EDIT_UPDATE,
                frame_id,
                &input,
            ),
            INKPOD_STATUS_OK
        );
        input.center_x = 12.0;
        assert_eq!(
            inkpod_core_shooting_frame_preview_update(core, &input),
            INKPOD_STATUS_OK
        );
        let options = InkpodSnapshotOptions {
            struct_size: size_of::<InkpodSnapshotOptions>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
        };
        let mut snapshot = ptr::null_mut();
        assert_eq!(
            inkpod_core_build_snapshot(core, &options, &mut snapshot),
            INKPOD_STATUS_OK
        );
        let mut view = InkpodSnapshotShootingFrameView {
            struct_size: size_of::<InkpodSnapshotShootingFrameView>() as u32,
            ..InkpodSnapshotShootingFrameView::default()
        };
        assert_eq!(
            inkpod_snapshot_get_shooting_frames(snapshot, &mut view),
            INKPOD_STATUS_OK
        );
        assert_eq!(view.frame_count, 1);
        assert!(!view.frames.is_null());
        assert_eq!(
            view.frame_stride_bytes,
            size_of::<InkpodShootingFrameInfo>() as u64
        );
        assert_eq!(inkpod_snapshot_release(&mut snapshot), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_shooting_frame_preview_apply(core, &mut revision, &mut frame_id),
            INKPOD_STATUS_OK
        );
        assert!(revision > stable.document_revision);

        let mut instruction_buffer = ptr::null_mut();
        assert_eq!(
            inkpod_core_export_instruction_common_raster(
                core,
                INKPOD_COMMON_RASTER_PNG,
                0,
                &mut instruction_buffer,
            ),
            INKPOD_STATUS_OK
        );
        assert!(!instruction_buffer.is_null());
        assert_eq!(
            inkpod_byte_buffer_release(&mut instruction_buffer),
            INKPOD_STATUS_OK
        );

        let committed = queried_document_info(core);
        input.center_x = 14.0;
        assert_eq!(
            inkpod_core_shooting_frame_preview_begin(
                core,
                committed.document_revision,
                INKPOD_SHOOTING_FRAME_EDIT_UPDATE,
                frame_id,
                &input,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_shooting_frame_preview_cancel(core),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            document_observation(&queried_document_info(core)),
            document_observation(&committed)
        );
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn vanishing_point_abi_owns_query_records_and_borrows_snapshot_spans() {
    let (mut core, initial) = create_core(64, 48, 0x5f21);
    unsafe {
        let (_, layer_id) = (*core)
            .core
            .create_layer(LayerKind::VanishingPoint, "Perspective")
            .unwrap();
        (*core)
            .core
            .apply_view(ViewCommand::ViewportResized {
                viewport_width: 64.0,
                viewport_height: 48.0,
            })
            .unwrap();
        let base = queried_document_info(core);
        assert!(base.document_revision > initial.document_revision);
        let color = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            depth: INKPOD_COLOR_DEPTH_16,
            red: 1_000,
            green: 20_000,
            blue: 50_000,
            alpha: u16::MAX,
        };
        let mut input = InkpodVanishingPointInput {
            struct_size: size_of::<InkpodVanishingPointInput>() as u32,
            visible: 1,
            feature_flags: INKPOD_FEATURE_NONE,
            layer_id,
            x_milli: -20_000,
            y_milli: 24_000,
            interval_milli_degrees: 15_000,
            angle_milli_degrees: 195_000,
            opacity_milli: 750,
            reserved: 0,
            color,
        };
        let mut revision = 0;
        let mut point_id = 0;
        assert_eq!(
            inkpod_core_vanishing_point_edit(
                core,
                base.document_revision,
                INKPOD_VANISHING_POINT_EDIT_CREATE,
                0,
                &input,
                &mut revision,
                &mut point_id,
            ),
            INKPOD_STATUS_OK
        );
        assert_ne!(point_id, 0);

        let mut count = 0;
        assert_eq!(
            inkpod_core_vanishing_points_copy(core, ptr::null_mut(), 0, 0, &mut count),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        assert_eq!(count, 1);
        let mut point = InkpodVanishingPointInfo::default();
        assert_eq!(
            inkpod_core_vanishing_points_copy(
                core,
                &mut point,
                1,
                size_of::<InkpodVanishingPointInfo>() as u64,
                &mut count,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(point.point_id, point_id);
        assert_eq!(point.color.depth, INKPOD_COLOR_DEPTH_16);
        assert_eq!(point.angle_milli_degrees, 15_000);

        let stable = queried_document_info(core);
        input.struct_size -= 1;
        assert_eq!(
            inkpod_core_vanishing_point_edit(
                core,
                stable.document_revision,
                INKPOD_VANISHING_POINT_EDIT_UPDATE,
                point_id,
                &input,
                &mut revision,
                &mut point_id,
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        input.struct_size = size_of::<InkpodVanishingPointInput>() as u32;
        assert_eq!(
            queried_document_info(core).document_revision,
            stable.document_revision
        );

        input.x_milli = 96_000;
        assert_eq!(
            inkpod_core_vanishing_point_preview_begin(
                core,
                stable.document_revision,
                INKPOD_VANISHING_POINT_EDIT_UPDATE,
                point_id,
                &input,
            ),
            INKPOD_STATUS_OK
        );
        input.y_milli = 30_000;
        assert_eq!(
            inkpod_core_vanishing_point_preview_update(core, &input),
            INKPOD_STATUS_OK
        );
        let options = InkpodSnapshotOptions {
            struct_size: size_of::<InkpodSnapshotOptions>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
        };
        let mut snapshot = ptr::null_mut();
        assert_eq!(
            inkpod_core_build_snapshot(core, &options, &mut snapshot),
            INKPOD_STATUS_OK
        );
        let mut view = InkpodSnapshotVanishingPointView {
            struct_size: size_of::<InkpodSnapshotVanishingPointView>() as u32,
            ..InkpodSnapshotVanishingPointView::default()
        };
        assert_eq!(
            inkpod_snapshot_get_vanishing_points(snapshot, &mut view),
            INKPOD_STATUS_OK
        );
        assert_eq!(view.point_count, 1);
        assert!(view.radial_guide_count > 0);
        assert!(!view.points.is_null());
        assert!(!view.radial_guides.is_null());
        assert_eq!(inkpod_snapshot_release(&mut snapshot), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_vanishing_point_preview_cancel(core),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            queried_document_info(core).document_revision,
            stable.document_revision
        );
        assert_eq!(
            inkpod_core_vanishing_point_preview_begin(
                core,
                stable.document_revision,
                INKPOD_VANISHING_POINT_EDIT_UPDATE,
                point_id,
                &input,
            ),
            INKPOD_STATUS_OK
        );
        let mut applied_revision = 0;
        let mut applied_id = 0;
        assert_eq!(
            inkpod_core_vanishing_point_preview_apply(core, &mut applied_revision, &mut applied_id,),
            INKPOD_STATUS_OK
        );
        assert!(applied_revision > stable.document_revision);
        assert_eq!(applied_id, point_id);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

fn config() -> InkpodCoreConfig {
    InkpodCoreConfig {
        struct_size: size_of::<InkpodCoreConfig>() as u32,
        abi_version: INKPOD_ABI_VERSION,
        feature_flags: INKPOD_FEATURE_NONE,
    }
}

fn dispatch() -> InkpodDispatchResult {
    InkpodDispatchResult {
        struct_size: size_of::<InkpodDispatchResult>() as u32,
        reserved: 0,
        revision: 0,
        accepted_command_count: 0,
    }
}

fn document_info() -> InkpodDocumentInfo {
    InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..InkpodDocumentInfo::default()
    }
}

fn queried_document_info(core: *mut InkpodCore) -> InkpodDocumentInfo {
    let mut info = document_info();
    // SAFETY: The caller supplies a live owner-thread Core for this test helper.
    unsafe {
        assert_eq!(
            inkpod_core_get_document_info(core, &mut info),
            INKPOD_STATUS_OK
        );
    }
    info
}

fn document_observation(info: &InkpodDocumentInfo) -> (u64, u64, u64, u64, u64, u64) {
    (
        info.document_revision,
        u64::from(info.flags),
        info.document_id,
        info.layer_id,
        info.main_plane_checksum,
        info.color_plane_checksum,
    )
}

fn queried_history_info(core: *mut InkpodCore) -> InkpodHistoryInfo {
    let mut history = InkpodHistoryInfo {
        struct_size: size_of::<InkpodHistoryInfo>() as u32,
        reserved: 0,
        cursor: 0,
        item_count: 0,
    };
    // SAFETY: The caller supplies a live owner-thread Core for this test helper.
    unsafe {
        assert_eq!(
            inkpod_core_history_info(core, &mut history),
            INKPOD_STATUS_OK
        );
    }
    history
}

#[test]
fn output_color_guard_abi_validates_profile_task_stale_and_owned_result_records() {
    let (mut core, initial) = create_core(2, 2, 0x6f01);
    unsafe {
        let mut request = InkpodOutputColorGuardRequest {
            struct_size: size_of::<InkpodOutputColorGuardRequest>() as u32,
            profile: INKPOD_OUTPUT_COLOR_GUARD_BT709_CONSERVATIVE_YCBCR,
            operation: INKPOD_SELECTION_NEW,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            base_document_revision: initial.document_revision,
        };
        let mut result = InkpodOutputColorGuardResult {
            struct_size: size_of::<InkpodOutputColorGuardResult>() as u32,
            ..InkpodOutputColorGuardResult::default()
        };

        let mut task = ptr::null_mut();
        assert_eq!(inkpod_task_create(&mut task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_select_output_color_guard(core, &request, task, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(result.revision, initial.document_revision);
        assert_eq!(result.accepted_command_count, 1);
        assert_eq!(result.scanned_pixel_count, 0);
        assert_eq!(result.selected_pixel_count, 0);
        assert_eq!(result.transparent_pixel_count, 4);
        assert_eq!(inkpod_task_release(&mut task), INKPOD_STATUS_OK);

        request.profile = u32::MAX;
        let mut invalid_task = ptr::null_mut();
        assert_eq!(inkpod_task_create(&mut invalid_task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_select_output_color_guard(core, &request, invalid_task, &mut result),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(inkpod_task_release(&mut invalid_task), INKPOD_STATUS_OK);
        request.profile = INKPOD_OUTPUT_COLOR_GUARD_BT709_CONSERVATIVE_YCBCR;

        let original_size = request.struct_size;
        request.struct_size -= 1;
        let mut short_task = ptr::null_mut();
        assert_eq!(inkpod_task_create(&mut short_task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_select_output_color_guard(core, &request, short_task, &mut result),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(inkpod_task_release(&mut short_task), INKPOD_STATUS_OK);
        request.struct_size = original_size;

        let mut cancelled_task = ptr::null_mut();
        assert_eq!(inkpod_task_create(&mut cancelled_task), INKPOD_STATUS_OK);
        assert_eq!(inkpod_task_cancel(cancelled_task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_select_output_color_guard(core, &request, cancelled_task, &mut result),
            INKPOD_STATUS_CANCELLED
        );
        assert_eq!(inkpod_task_release(&mut cancelled_task), INKPOD_STATUS_OK);

        let selection = InkpodSelectionInput {
            struct_size: size_of::<InkpodSelectionInput>() as u32,
            shape: INKPOD_SELECTION_RECTANGLE,
            operation: INKPOD_SELECTION_NEW,
            bounds: InkpodFrameRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            interpretation: INKPOD_RANGE_NORMAL,
            trace_shape: INKPOD_TRACE_ROUND,
            view_zoom_q16: 1 << 16,
            ..InkpodSelectionInput::default()
        };
        let mut dispatch_result = dispatch();
        assert_eq!(
            inkpod_core_apply_selection(core, &selection, &mut dispatch_result),
            INKPOD_STATUS_OK
        );
        let before_stale = queried_document_info(core);
        let mut stale_task = ptr::null_mut();
        assert_eq!(inkpod_task_create(&mut stale_task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_select_output_color_guard(core, &request, stale_task, &mut result),
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(
            queried_document_info(core).document_revision,
            before_stale.document_revision
        );
        assert_eq!(inkpod_task_release(&mut stale_task), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

fn editor_state_info() -> InkpodEditorStateInfo {
    InkpodEditorStateInfo {
        struct_size: size_of::<InkpodEditorStateInfo>() as u32,
        ..InkpodEditorStateInfo::default()
    }
}

fn editor_state_update(kind: u32, revision: u64) -> InkpodEditorStateUpdate {
    InkpodEditorStateUpdate {
        struct_size: size_of::<InkpodEditorStateUpdate>() as u32,
        kind,
        expected_editor_revision: revision,
        ..InkpodEditorStateUpdate::default()
    }
}

fn color(red: u16, green: u16, blue: u16, alpha: u16) -> InkpodColorValue {
    InkpodColorValue {
        struct_size: size_of::<InkpodColorValue>() as u32,
        depth: INKPOD_COLOR_DEPTH_8,
        red,
        green,
        blue,
        alpha,
    }
}

fn create_core(width: u32, height: u32, uuid_low: u64) -> (*mut InkpodCore, InkpodDocumentInfo) {
    let mut core = ptr::null_mut();
    let options = InkpodCellCreateOptions {
        struct_size: size_of::<InkpodCellCreateOptions>() as u32,
        reserved: 0,
        feature_flags: 0,
        document_uuid_high: 0x494e_4b50_4f44_4646,
        document_uuid_low: uuid_low,
        width,
        height,
        dpi_x_milli: 96_000,
        dpi_y_milli: 96_000,
    };
    let mut info = document_info();
    // SAFETY: All records are complete, aligned, non-overlapping, and live for each call.
    unsafe {
        assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_new_cell(core, &options, &mut info),
            INKPOD_STATUS_OK
        );
    }
    (core, info)
}

fn temporary_inkpod_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "inkpod-ffi-contract-{label}-{}-{}.inkpod",
        std::process::id(),
        TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn application_palette_and_chart_codecs_are_bounded_current_version_ffi_contracts() {
    let palette_path = temporary_inkpod_path("palette-codec").with_extension("inkpalette");
    let chart_path = temporary_inkpod_path("chart-codec").with_extension("inkchart");
    let colors = [color(1, 2, 3, 255), color(21, 34, 55, 144)];
    let palette = InkpodColorArray {
        struct_size: size_of::<InkpodColorArray>() as u32,
        reserved: 0,
        feature_flags: INKPOD_FEATURE_NONE,
        colors: colors.as_ptr(),
        color_count: colors.len() as u64,
        color_stride_bytes: size_of::<InkpodColorValue>() as u64,
    };
    let palette_bytes = palette_path.to_string_lossy().into_owned().into_bytes();
    unsafe {
        assert_eq!(
            inkpod_palette_file_save(palette_bytes.as_ptr(), palette_bytes.len() as u64, &palette,),
            INKPOD_STATUS_OK
        );
    }
    let mut output = InkpodColorBuffer {
        struct_size: size_of::<InkpodColorBuffer>() as u32,
        reserved: 0,
        feature_flags: INKPOD_FEATURE_NONE,
        colors: ptr::null_mut(),
        color_capacity: 0,
        color_stride_bytes: 0,
        color_count: 0,
    };
    unsafe {
        assert_eq!(
            inkpod_palette_file_load(
                palette_bytes.as_ptr(),
                palette_bytes.len() as u64,
                &mut output,
            ),
            INKPOD_STATUS_OK
        );
    }
    assert_eq!(output.color_count, 2);
    let mut decoded = [InkpodColorValue::default(); 2];
    output.colors = decoded.as_mut_ptr();
    output.color_capacity = decoded.len() as u64;
    output.color_stride_bytes = size_of::<InkpodColorValue>() as u64;
    unsafe {
        assert_eq!(
            inkpod_palette_file_load(
                palette_bytes.as_ptr(),
                palette_bytes.len() as u64,
                &mut output,
            ),
            INKPOD_STATUS_OK
        );
    }
    assert_eq!(decoded[1].green, 34);

    let names = [b"Blue".as_slice(), "濃青".as_bytes()];
    let entries = [
        InkpodColorChartEntry {
            struct_size: size_of::<InkpodColorChartEntry>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            color: colors[0],
            name_utf8: names[0].as_ptr(),
            name_bytes: names[0].len() as u64,
        },
        InkpodColorChartEntry {
            struct_size: size_of::<InkpodColorChartEntry>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            color: colors[1],
            name_utf8: names[1].as_ptr(),
            name_bytes: names[1].len() as u64,
        },
    ];
    let chart_bytes = chart_path.to_string_lossy().into_owned().into_bytes();
    unsafe {
        assert_eq!(
            inkpod_color_chart_file_save(
                chart_bytes.as_ptr(),
                chart_bytes.len() as u64,
                entries.as_ptr(),
                entries.len() as u64,
                size_of::<InkpodColorChartEntry>() as u64,
            ),
            INKPOD_STATUS_OK
        );
    }
    let mut chart = ptr::null_mut();
    unsafe {
        assert_eq!(
            inkpod_color_chart_file_load(
                chart_bytes.as_ptr(),
                chart_bytes.len() as u64,
                &mut chart,
            ),
            INKPOD_STATUS_OK
        );
    }
    let mut count = 0;
    unsafe {
        assert_eq!(
            inkpod_color_chart_file_count(chart, &mut count),
            INKPOD_STATUS_OK
        );
    }
    assert_eq!(count, 2);
    let mut chart_color = color(0, 0, 0, 0);
    let mut required = 0;
    unsafe {
        assert_eq!(
            inkpod_color_chart_file_get(
                chart,
                1,
                &mut chart_color,
                ptr::null_mut(),
                0,
                &mut required,
            ),
            INKPOD_STATUS_OK
        );
    }
    let mut name = vec![0; required as usize];
    unsafe {
        assert_eq!(
            inkpod_color_chart_file_get(
                chart,
                1,
                &mut chart_color,
                name.as_mut_ptr(),
                name.len() as u64,
                &mut required,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_color_chart_file_release(&mut chart),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_color_chart_file_release(&mut chart),
            INKPOD_STATUS_INVALID_STATE
        );
    }
    assert_eq!(name, names[1]);
    assert_eq!(chart_color.blue, 55);

    let malformed_path = temporary_inkpod_path("palette-old").with_extension("inkpalette");
    std::fs::write(&malformed_path, b"INKPAL0\0\0\0\0\0").unwrap();
    let malformed_bytes = malformed_path.to_string_lossy().into_owned().into_bytes();
    let mut malformed_output = InkpodColorBuffer {
        struct_size: size_of::<InkpodColorBuffer>() as u32,
        reserved: 0,
        feature_flags: INKPOD_FEATURE_NONE,
        colors: ptr::null_mut(),
        color_capacity: 0,
        color_stride_bytes: 0,
        color_count: 0,
    };
    unsafe {
        assert_ne!(
            inkpod_palette_file_load(
                malformed_bytes.as_ptr(),
                malformed_bytes.len() as u64,
                &mut malformed_output,
            ),
            INKPOD_STATUS_OK
        );
    }
    std::fs::remove_file(palette_path).unwrap();
    std::fs::remove_file(chart_path).unwrap();
    std::fs::remove_file(malformed_path).unwrap();
}

#[test]
fn cell_creation_carries_the_typed_initial_layer_in_genesis() {
    let mut core = ptr::null_mut();
    let options = InkpodCellCreateOptions {
        struct_size: size_of::<InkpodCellCreateOptions>() as u32,
        reserved: INKPOD_LAYER_RASTER,
        feature_flags: INKPOD_CELL_CREATE_INITIAL_LAYER_KIND,
        document_uuid_high: 0x4d36_4745_4e45_5349,
        document_uuid_low: TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        width: 8,
        height: 8,
        dpi_x_milli: 96_000,
        dpi_y_milli: 96_000,
    };
    let mut info = document_info();
    let mut node = InkpodNodeInfo {
        struct_size: size_of::<InkpodNodeInfo>() as u32,
        ..InkpodNodeInfo::default()
    };
    unsafe {
        assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_new_cell(core, &options, &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_node_get(core, 0, u32::MAX, &mut node),
            INKPOD_STATUS_OK
        );
        assert_eq!(node.kind, INKPOD_LAYER_RASTER);
        assert_eq!(queried_history_info(core).item_count, 0);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

fn save_document(core: *mut InkpodCore, path: &Path) -> InkpodDocumentInfo {
    let path_bytes = path.to_string_lossy().into_owned().into_bytes();
    let mut info = document_info();
    // SAFETY: Core is live on this thread and the UTF-8 path span and output live for the call.
    unsafe {
        assert_eq!(
            inkpod_core_save(
                core,
                path_bytes.as_ptr(),
                path_bytes.len() as u64,
                &mut info,
            ),
            INKPOD_STATUS_OK
        );
    }
    info
}

#[test]
fn editor_defaults_and_state_ffi_are_caller_owned_exact_depth_and_side_effect_free() {
    let mut core = ptr::null_mut();
    // SAFETY: Every live pointer below is aligned, complete, and uniquely writable for its call.
    unsafe {
        assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);

        assert_eq!(
            inkpod_core_get_editor_defaults(ptr::null_mut(), ptr::null_mut()),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_get_editor_defaults(core, ptr::null_mut()),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut short_defaults = InkpodEditorDefaults {
            struct_size: size_of::<InkpodEditorDefaults>() as u32 - 1,
            width: u32::MAX,
            ..InkpodEditorDefaults::default()
        };
        assert_eq!(
            inkpod_core_get_editor_defaults(core, &mut short_defaults),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(short_defaults.width, u32::MAX);

        let mut defaults = InkpodEditorDefaults {
            struct_size: size_of::<InkpodEditorDefaults>() as u32,
            ..InkpodEditorDefaults::default()
        };
        assert_eq!(
            inkpod_core_get_editor_defaults(core, &mut defaults),
            INKPOD_STATUS_OK
        );
        assert_eq!((defaults.width, defaults.height), (1_920, 1_080));
        assert_eq!(
            (defaults.dpi_x_milli, defaults.dpi_y_milli),
            (96_000, 96_000)
        );
        assert_eq!(defaults.state.active_tool, INKPOD_EDITOR_TOOL_PENCIL);
        assert_eq!(defaults.state.current_color.depth, INKPOD_COLOR_DEPTH_8);
        assert_eq!(
            (
                defaults.state.current_color.red,
                defaults.state.current_color.green,
                defaults.state.current_color.blue,
                defaults.state.current_color.alpha,
            ),
            (0, 0, 0, 255)
        );
        assert_eq!(defaults.state.current_diameter_q16, 1_i64 << 16);
        assert_eq!(defaults.state.flags & INKPOD_EDITOR_STATE_HAS_TARGET, 0);
        assert_eq!(
            defaults.state.fill.struct_size,
            size_of::<InkpodEditorFillOptions>() as u32
        );
        assert_eq!(defaults.state.fill.extension_distance, 1);

        let options = InkpodCellCreateOptions {
            struct_size: size_of::<InkpodCellCreateOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
            document_uuid_high: 0x494e_4b50_4f44_4646,
            document_uuid_low: 0x4544_4954,
            width: 32,
            height: 24,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
        };
        let mut document = document_info();
        assert_eq!(
            inkpod_core_new_cell(core, &options, &mut document),
            INKPOD_STATUS_OK
        );
        let initial_document_revision = document.document_revision;

        assert_eq!(
            inkpod_core_get_editor_state(core, ptr::null_mut()),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut short_state = InkpodEditorStateInfo {
            struct_size: size_of::<InkpodEditorStateInfo>() as u32 - 1,
            editor_revision: u64::MAX,
            ..InkpodEditorStateInfo::default()
        };
        assert_eq!(
            inkpod_core_get_editor_state(core, &mut short_state),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(short_state.editor_revision, u64::MAX);

        let mut state = editor_state_info();
        assert_eq!(
            inkpod_core_get_editor_state(core, &mut state),
            INKPOD_STATUS_OK
        );
        assert_eq!(state.editor_revision, 1);
        assert_ne!(state.flags & INKPOD_EDITOR_STATE_HAS_TARGET, 0);
        assert_eq!(state.active_layer_id, document.layer_id);
        assert_eq!(state.active_plane_id, document.main_plane_id);
        let initial_digest = state.editor_digest;

        let mut queried_document = document_info();
        assert_eq!(
            inkpod_core_get_document_info(core, &mut queried_document),
            INKPOD_STATUS_OK
        );
        let mut history = InkpodHistoryInfo {
            struct_size: size_of::<InkpodHistoryInfo>() as u32,
            reserved: 0,
            cursor: 0,
            item_count: 0,
        };
        assert_eq!(
            inkpod_core_history_info(core, &mut history),
            INKPOD_STATUS_OK
        );
        assert_eq!((history.cursor, history.item_count), (0, 0));

        // Mutating a returned inline copy proves it does not alias Core-owned storage.
        state.active_tool = u32::MAX;
        state.editor_digest.fill(0);
        let mut queried_again = editor_state_info();
        assert_eq!(
            inkpod_core_get_editor_state(core, &mut queried_again),
            INKPOD_STATUS_OK
        );
        assert_eq!(queried_again.active_tool, INKPOD_EDITOR_TOOL_PENCIL);
        assert_eq!(queried_again.editor_digest, initial_digest);
        assert_eq!(queried_again.editor_revision, 1);

        let mut color_update = editor_state_update(INKPOD_EDITOR_UPDATE_TOOL_COLOR, 1);
        color_update.tool = INKPOD_EDITOR_TOOL_BRUSH;
        color_update.color = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            depth: INKPOD_COLOR_DEPTH_16,
            red: 1,
            green: 257,
            blue: 32_769,
            alpha: 65_534,
        };
        let mut changed = editor_state_info();
        assert_eq!(
            inkpod_core_update_editor_state(core, &color_update, &mut changed),
            INKPOD_STATUS_OK
        );
        assert_eq!(changed.editor_revision, 2);
        assert_ne!(changed.editor_digest, initial_digest);
        assert_ne!(changed.flags & INKPOD_EDITOR_STATE_DIRTY, 0);

        let mut tool_update = editor_state_update(INKPOD_EDITOR_UPDATE_ACTIVE_TOOL, 2);
        tool_update.tool = INKPOD_EDITOR_TOOL_BRUSH;
        assert_eq!(
            inkpod_core_update_editor_state(core, &tool_update, &mut changed),
            INKPOD_STATUS_OK
        );
        assert_eq!(changed.editor_revision, 3);
        assert_eq!(changed.current_color.depth, INKPOD_COLOR_DEPTH_16);
        assert_eq!(
            (
                changed.current_color.red,
                changed.current_color.green,
                changed.current_color.blue,
                changed.current_color.alpha,
            ),
            (1, 257, 32_769, 65_534)
        );
        let changed_digest = changed.editor_digest;

        // The same update is a semantic no-op and retains revision/digest.
        tool_update.expected_editor_revision = changed.editor_revision;
        let mut no_op = editor_state_info();
        assert_eq!(
            inkpod_core_update_editor_state(core, &tool_update, &mut no_op),
            INKPOD_STATUS_OK
        );
        assert_eq!(no_op.editor_revision, changed.editor_revision);
        assert_eq!(no_op.editor_digest, changed_digest);

        let mut sample = InkpodStrokeSample {
            struct_size: size_of::<InkpodStrokeSample>() as u32,
            flags: 0,
            x: 2.0,
            y: 2.0,
            pressure: 1.0,
            reserved: 0,
        };
        let editor_stroke = InkpodEditorStrokeInput {
            struct_size: size_of::<InkpodEditorStrokeInput>() as u32,
            coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
            tool: 0,
            reserved: 0,
            flags: 0,
            samples: &sample,
            sample_count: 1,
            sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
        };
        assert_eq!(
            inkpod_core_editor_stroke_begin(core, &editor_stroke),
            INKPOD_STATUS_OK
        );
        sample.x = 15.0;
        assert_eq!(sample.x, 15.0);
        assert_eq!(inkpod_core_stroke_cancel(core), INKPOD_STATUS_OK);
        let pencil_stroke = InkpodEditorStrokeInput {
            tool: INKPOD_EDITOR_TOOL_PENCIL,
            ..editor_stroke
        };
        assert_eq!(
            inkpod_core_editor_stroke_begin(core, &pencil_stroke),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_stroke_cancel(core), INKPOD_STATUS_OK);
        let mut secondary_view_id = 0;
        assert_eq!(
            inkpod_core_view_create(core, &mut secondary_view_id),
            INKPOD_STATUS_OK
        );
        assert_ne!(secondary_view_id, 0);
        assert_eq!(
            inkpod_core_editor_stroke_begin_for_view(core, secondary_view_id, &pencil_stroke,),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_stroke_cancel(core), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_editor_stroke_begin_for_view(core, u64::MAX, &pencil_stroke),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_view_close(core, secondary_view_id),
            INKPOD_STATUS_OK
        );

        let mut after_document = document_info();
        assert_eq!(
            inkpod_core_get_document_info(core, &mut after_document),
            INKPOD_STATUS_OK
        );
        assert_eq!(after_document.document_revision, initial_document_revision);
        assert_ne!(after_document.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
        assert_eq!(
            inkpod_core_history_info(core, &mut history),
            INKPOD_STATUS_OK
        );
        assert_eq!((history.cursor, history.item_count), (0, 0));

        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        assert!(core.is_null());
    }
}

#[test]
fn editor_state_ffi_rejects_short_unknown_stale_and_invalid_updates_atomically() {
    let (mut core, _) = create_core(16, 16, 0x4544_4955);
    // SAFETY: Complete records and the live owner-thread Core remain valid for every call.
    unsafe {
        let mut before = editor_state_info();
        assert_eq!(
            inkpod_core_get_editor_state(core, &mut before),
            INKPOD_STATUS_OK
        );

        assert_eq!(
            inkpod_core_update_editor_state(core, ptr::null(), ptr::null_mut()),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_editor_stroke_begin(ptr::null_mut(), ptr::null()),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_editor_stroke_begin_for_view(ptr::null_mut(), 0, ptr::null(),),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let short_stroke = InkpodEditorStrokeInput {
            struct_size: size_of::<InkpodEditorStrokeInput>() as u32 - 1,
            ..InkpodEditorStrokeInput::default()
        };
        assert_eq!(
            inkpod_core_editor_stroke_begin(core, &short_stroke),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        let unknown_stroke = InkpodEditorStrokeInput {
            struct_size: size_of::<InkpodEditorStrokeInput>() as u32,
            coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
            tool: u32::MAX,
            reserved: 0,
            flags: 0,
            samples: ptr::null(),
            sample_count: 0,
            sample_stride_bytes: 0,
        };
        assert_eq!(
            inkpod_core_editor_stroke_begin(core, &unknown_stroke),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut short =
            editor_state_update(INKPOD_EDITOR_UPDATE_ACTIVE_TOOL, before.editor_revision);
        short.struct_size -= 1;
        let mut untouched = editor_state_info();
        untouched.editor_revision = u64::MAX;
        assert_eq!(
            inkpod_core_update_editor_state(core, &short, &mut untouched),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(untouched.editor_revision, u64::MAX);

        let mut unknown =
            editor_state_update(INKPOD_EDITOR_UPDATE_ACTIVE_TOOL, before.editor_revision);
        unknown.tool = u32::MAX;
        assert_eq!(
            inkpod_core_update_editor_state(core, &unknown, &mut untouched),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut unknown_kind = editor_state_update(u32::MAX, before.editor_revision);
        unknown_kind.tool = INKPOD_EDITOR_TOOL_BRUSH;
        assert_eq!(
            inkpod_core_update_editor_state(core, &unknown_kind, &mut untouched),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut nonzero_reserved =
            editor_state_update(INKPOD_EDITOR_UPDATE_ACTIVE_TOOL, before.editor_revision);
        nonzero_reserved.tool = INKPOD_EDITOR_TOOL_BRUSH;
        nonzero_reserved.reserved = 1;
        assert_eq!(
            inkpod_core_update_editor_state(core, &nonzero_reserved, &mut untouched),
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        let mut stale = editor_state_update(INKPOD_EDITOR_UPDATE_ACTIVE_TOOL, 0);
        stale.tool = INKPOD_EDITOR_TOOL_BRUSH;
        assert_eq!(
            inkpod_core_update_editor_state(core, &stale, &mut untouched),
            INKPOD_STATUS_INVALID_STATE
        );
        let mut invalid_target =
            editor_state_update(INKPOD_EDITOR_UPDATE_ACTIVE_TARGET, before.editor_revision);
        invalid_target.active_layer_id = u64::MAX - 1;
        invalid_target.active_plane_id = u64::MAX;
        assert_eq!(
            inkpod_core_update_editor_state(core, &invalid_target, &mut untouched),
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        let mut after = editor_state_info();
        assert_eq!(
            inkpod_core_get_editor_state(core, &mut after),
            INKPOD_STATUS_OK
        );
        assert_eq!(after.editor_revision, before.editor_revision);
        assert_eq!(after.editor_digest, before.editor_digest);
        assert_eq!(after.flags, before.flags);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn editor_state_ffi_accepts_raster_geometry_tools_and_rejects_removed_codes() {
    let (mut core, _) = create_core(16, 16, 0x4745_4f4d);
    // SAFETY: Complete records and the live owner-thread Core remain valid for every call.
    unsafe {
        let mut state = editor_state_info();
        assert_eq!(
            inkpod_core_get_editor_state(core, &mut state),
            INKPOD_STATUS_OK
        );
        for tool in [
            INKPOD_EDITOR_TOOL_GEOMETRY_LINE,
            INKPOD_EDITOR_TOOL_GEOMETRY_CURVE,
            INKPOD_EDITOR_TOOL_GEOMETRY_RECTANGLE,
            INKPOD_EDITOR_TOOL_GEOMETRY_ELLIPSE,
            INKPOD_EDITOR_TOOL_GEOMETRY_POLYGON,
            INKPOD_EDITOR_TOOL_GEOMETRY_POLYLINE,
        ] {
            let mut update =
                editor_state_update(INKPOD_EDITOR_UPDATE_ACTIVE_TOOL, state.editor_revision);
            update.tool = tool;
            let previous_revision = state.editor_revision;
            assert_eq!(
                inkpod_core_update_editor_state(core, &update, &mut state),
                INKPOD_STATUS_OK
            );
            assert_eq!(state.active_tool, tool);
            assert_eq!(state.editor_revision, previous_revision + 1);
            assert_ne!(state.flags & INKPOD_EDITOR_STATE_HAS_CURRENT_COLOR, 0);
        }

        for removed_tool in [1_201, 1_207] {
            let before = state;
            let mut update =
                editor_state_update(INKPOD_EDITOR_UPDATE_ACTIVE_TOOL, state.editor_revision);
            update.tool = removed_tool;
            assert_eq!(
                inkpod_core_update_editor_state(core, &update, &mut state),
                INKPOD_STATUS_INVALID_ARGUMENT
            );
            let mut after = editor_state_info();
            assert_eq!(
                inkpod_core_get_editor_state(core, &mut after),
                INKPOD_STATUS_OK
            );
            assert_eq!(after.editor_revision, before.editor_revision);
            assert_eq!(after.editor_digest, before.editor_digest);
            state = after;
        }
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn editor_brush_options_are_inline_owned_and_validate_noop_stale_and_negative_inputs() {
    let (mut core, _) = create_core(16, 16, 0x4252_5553);
    // SAFETY: Every record is complete, aligned, uniquely writable, and live for its call.
    unsafe {
        let mut before = editor_state_info();
        assert_eq!(
            inkpod_core_get_editor_state(core, &mut before),
            INKPOD_STATUS_OK
        );
        assert_eq!(before.brush.shape, INKPOD_BRUSH_ROUND);
        assert_eq!(before.brush.smoothing, 0);
        assert_eq!(before.brush.start_color, INKPOD_START_COLOR_ANY);

        let mut update =
            editor_state_update(INKPOD_EDITOR_UPDATE_BRUSH_OPTIONS, before.editor_revision);
        update.brush = InkpodEditorBrushOptions {
            struct_size: size_of::<InkpodEditorBrushOptions>() as u32,
            shape: INKPOD_BRUSH_SQUARE,
            smoothing: 777,
            reserved: 0,
            start_color: INKPOD_START_COLOR_EXACT_NATIVE,
            reserved2: 0,
        };
        let mut changed = editor_state_info();
        assert_eq!(
            inkpod_core_update_editor_state(core, &update, &mut changed),
            INKPOD_STATUS_OK
        );
        assert_eq!(changed.editor_revision, before.editor_revision + 1);
        assert_eq!(changed.brush.shape, INKPOD_BRUSH_SQUARE);
        assert_eq!(changed.brush.smoothing, 777);
        assert_eq!(changed.brush.start_color, INKPOD_START_COLOR_EXACT_NATIVE);

        // The returned nested record is an inline caller-owned copy.
        changed.brush.shape = u32::MAX;
        assert_eq!(changed.brush.shape, u32::MAX);
        let mut owned_again = editor_state_info();
        assert_eq!(
            inkpod_core_get_editor_state(core, &mut owned_again),
            INKPOD_STATUS_OK
        );
        assert_eq!(owned_again.brush.shape, INKPOD_BRUSH_SQUARE);

        update.expected_editor_revision = owned_again.editor_revision;
        let mut no_op = editor_state_info();
        assert_eq!(
            inkpod_core_update_editor_state(core, &update, &mut no_op),
            INKPOD_STATUS_OK
        );
        assert_eq!(no_op.editor_revision, owned_again.editor_revision);
        assert_eq!(no_op.editor_digest, owned_again.editor_digest);

        let mut stale = update;
        stale.expected_editor_revision = 0;
        assert_eq!(
            inkpod_core_update_editor_state(core, &stale, &mut no_op),
            INKPOD_STATUS_INVALID_STATE
        );

        for (invalid_brush, expected_status) in [
            (
                InkpodEditorBrushOptions {
                    struct_size: size_of::<InkpodEditorBrushOptions>() as u32 - 1,
                    ..update.brush
                },
                INKPOD_STATUS_INVALID_ARGUMENT,
            ),
            (
                InkpodEditorBrushOptions {
                    shape: u32::MAX,
                    ..update.brush
                },
                INKPOD_STATUS_INVALID_ARGUMENT,
            ),
            (
                InkpodEditorBrushOptions {
                    smoothing: 1_001,
                    ..update.brush
                },
                INKPOD_STATUS_INVALID_ARGUMENT,
            ),
            (
                InkpodEditorBrushOptions {
                    reserved: 1,
                    ..update.brush
                },
                INKPOD_STATUS_INVALID_ARGUMENT,
            ),
            (
                InkpodEditorBrushOptions {
                    start_color: u32::MAX,
                    ..update.brush
                },
                INKPOD_STATUS_INVALID_ARGUMENT,
            ),
            (
                InkpodEditorBrushOptions {
                    reserved2: 1,
                    ..update.brush
                },
                INKPOD_STATUS_INVALID_ARGUMENT,
            ),
        ] {
            let mut invalid = update;
            invalid.brush = invalid_brush;
            assert_eq!(
                inkpod_core_update_editor_state(core, &invalid, &mut no_op),
                expected_status
            );
        }
        let mut after = editor_state_info();
        assert_eq!(
            inkpod_core_get_editor_state(core, &mut after),
            INKPOD_STATUS_OK
        );
        assert_eq!(after.editor_revision, owned_again.editor_revision);
        assert_eq!(after.editor_digest, owned_again.editor_digest);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn grouped_edit_target_ffi_owns_normalized_spans_and_rejects_stale_or_short_records() {
    let (mut core, document) = create_core(8, 8, 0x4544_5447);
    // SAFETY: Complete records and the live owner-thread Core remain valid for every call.
    unsafe {
        let mut state = editor_state_info();
        assert_eq!(
            inkpod_core_get_editor_state(core, &mut state),
            INKPOD_STATUS_OK
        );
        let targets = [
            InkpodEditTarget {
                struct_size: size_of::<InkpodEditTarget>() as u32,
                kind: INKPOD_EDIT_TARGET_PLANE,
                layer_id: document.layer_id,
                plane_id: document.color_plane_id,
                reserved: 0,
            },
            InkpodEditTarget {
                struct_size: size_of::<InkpodEditTarget>() as u32,
                kind: INKPOD_EDIT_TARGET_PLANE,
                layer_id: document.layer_id,
                plane_id: document.main_plane_id,
                reserved: 0,
            },
        ];
        let mut changed = editor_state_info();
        assert_eq!(
            inkpod_core_set_edit_targets(
                core,
                state.editor_revision,
                targets.as_ptr(),
                targets.len() as u64,
                size_of::<InkpodEditTarget>() as u64,
                &mut changed,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(changed.editor_revision, state.editor_revision + 1);

        let mut required = 0;
        assert_eq!(
            inkpod_core_get_edit_targets(core, ptr::null_mut(), 0, 0, &mut required),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        assert_eq!(required, 2);
        let mut copied = [InkpodEditTarget::default(); 2];
        assert_eq!(
            inkpod_core_get_edit_targets(
                core,
                copied.as_mut_ptr(),
                copied.len() as u64,
                size_of::<InkpodEditTarget>() as u64,
                &mut required,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(copied[0].plane_id, document.main_plane_id);
        assert_eq!(copied[1].plane_id, document.color_plane_id);
        copied[0].plane_id = u64::MAX;
        assert_eq!(copied[0].plane_id, u64::MAX);
        let mut recopy = [InkpodEditTarget::default(); 2];
        assert_eq!(
            inkpod_core_get_edit_targets(
                core,
                recopy.as_mut_ptr(),
                2,
                size_of::<InkpodEditTarget>() as u64,
                &mut required,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(recopy[0].plane_id, document.main_plane_id);
        let mut capabilities = InkpodEditTargetCapabilities {
            struct_size: size_of::<InkpodEditTargetCapabilities>() as u32,
            ..InkpodEditTargetCapabilities::default()
        };
        assert_eq!(
            inkpod_core_get_edit_target_capabilities(core, &mut capabilities),
            INKPOD_STATUS_OK
        );
        assert_eq!(capabilities.can_set_visibility, 1);
        assert_eq!(capabilities.can_set_editability, 1);
        assert_eq!(capabilities.can_merge, 0);
        capabilities.struct_size -= 1;
        assert_eq!(
            inkpod_core_get_edit_target_capabilities(core, &mut capabilities),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );

        assert_eq!(
            inkpod_core_set_edit_targets(
                core,
                state.editor_revision,
                targets.as_ptr(),
                2,
                size_of::<InkpodEditTarget>() as u64,
                &mut state,
            ),
            INKPOD_STATUS_INVALID_STATE
        );
        let mut short = targets;
        short[0].struct_size -= 1;
        assert_eq!(
            inkpod_core_set_edit_targets(
                core,
                changed.editor_revision,
                short.as_ptr(),
                2,
                size_of::<InkpodEditTarget>() as u64,
                &mut state,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        let duplicate_targets = [targets[1], targets[1]];
        assert_eq!(
            inkpod_core_set_edit_targets(
                core,
                changed.editor_revision,
                duplicate_targets.as_ptr(),
                duplicate_targets.len() as u64,
                size_of::<InkpodEditTarget>() as u64,
                &mut changed,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_get_edit_targets(core, ptr::null_mut(), 0, 0, &mut required),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        assert_eq!(required, 1);
        let foreign = InkpodEditTarget {
            layer_id: u64::MAX,
            plane_id: u64::MAX - 1,
            ..targets[0]
        };
        let before_foreign = changed;
        assert_eq!(
            inkpod_core_set_edit_targets(
                core,
                changed.editor_revision,
                &foreign,
                1,
                size_of::<InkpodEditTarget>() as u64,
                &mut changed,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(changed.editor_revision, before_foreign.editor_revision);
        assert_eq!(
            inkpod_core_set_edit_targets(
                core,
                changed.editor_revision,
                targets.as_ptr(),
                u64::from(INKPOD_MAX_EDIT_TARGETS) + 1,
                size_of::<InkpodEditTarget>() as u64,
                &mut changed,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(changed.editor_revision, before_foreign.editor_revision);

        let layer = InkpodEditTarget {
            struct_size: size_of::<InkpodEditTarget>() as u32,
            kind: INKPOD_EDIT_TARGET_LAYER,
            layer_id: document.layer_id,
            plane_id: 0,
            reserved: 0,
        };
        assert_eq!(
            inkpod_core_set_edit_targets(
                core,
                changed.editor_revision,
                &layer,
                1,
                size_of::<InkpodEditTarget>() as u64,
                &mut changed,
            ),
            INKPOD_STATUS_OK
        );
        let command = InkpodEditTargetCommand {
            struct_size: size_of::<InkpodEditTargetCommand>() as u32,
            operation: INKPOD_EDIT_TARGET_DUPLICATE,
            flags: 0,
            kind: 0,
            pixel_format: 0,
            reserved: 0,
        };
        let mut dispatch = dispatch();
        let mut output_count = 0;
        assert_eq!(
            inkpod_core_apply_edit_target_command(
                core,
                &command,
                &mut dispatch,
                ptr::null_mut(),
                0,
                0,
                &mut output_count,
            ),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        assert_eq!(output_count, 1);
        let history_before = queried_history_info(core);
        assert_eq!(history_before.item_count, 0);
        let mut output = InkpodEditTarget::default();
        assert_eq!(
            inkpod_core_apply_edit_target_command(
                core,
                &command,
                &mut dispatch,
                &mut output,
                1,
                size_of::<InkpodEditTarget>() as u64,
                &mut output_count,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(dispatch.accepted_command_count, 1);
        assert_eq!(output.kind, INKPOD_EDIT_TARGET_LAYER);
        assert_ne!(output.layer_id, document.layer_id);
        assert_eq!(queried_history_info(core).item_count, 1);

        let deleted_target = output;
        let delete_command = InkpodEditTargetCommand {
            operation: INKPOD_EDIT_TARGET_DELETE,
            ..command
        };
        output_count = u64::MAX;
        assert_eq!(
            inkpod_core_apply_edit_target_command(
                core,
                &delete_command,
                &mut dispatch,
                ptr::null_mut(),
                0,
                0,
                &mut output_count,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(output_count, 0);
        assert_eq!(dispatch.accepted_command_count, 1);
        assert_eq!(
            inkpod_core_get_editor_state(core, &mut changed),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_set_edit_targets(
                core,
                changed.editor_revision,
                &deleted_target,
                1,
                size_of::<InkpodEditTarget>() as u64,
                &mut changed,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn captured_editor_target_ffi_routes_fill_selection_and_color_without_live_retargeting() {
    let (mut core, document) = create_core(4, 4, 0x4544_4954);
    let fill = InkpodFillInput {
        struct_size: size_of::<InkpodFillInput>() as u32,
        operation: INKPOD_FILL_SEED,
        flags: 0,
        seed_x: 1,
        seed_y: 1,
        color: InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            depth: INKPOD_COLOR_DEPTH_8,
            red: 12,
            green: 34,
            blue: 56,
            alpha: 255,
        },
        tolerance: 0,
        gap_close: 0,
        inclusion_mode: INKPOD_INCLUSION_NONE,
        selection: InkpodFrameRect::default(),
        inclusion_colors: ptr::null(),
        inclusion_color_count: 0,
        inclusion_color_stride_bytes: 0,
        extension_distance: 0,
        reserved: 0,
    };
    let mut fill_result = InkpodFillResult {
        struct_size: size_of::<InkpodFillResult>() as u32,
        ..InkpodFillResult::default()
    };
    let selected_color = InkpodColorValue {
        struct_size: size_of::<InkpodColorValue>() as u32,
        depth: INKPOD_COLOR_DEPTH_8,
        red: 12,
        green: 34,
        blue: 56,
        alpha: 255,
    };
    let selection = InkpodSelectionInput {
        struct_size: size_of::<InkpodSelectionInput>() as u32,
        shape: INKPOD_SELECTION_RECTANGLE,
        operation: INKPOD_SELECTION_NEW,
        reserved: 0,
        bounds: InkpodFrameRect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        },
        points: ptr::null(),
        point_count: 0,
        point_stride_bytes: 0,
        diameter: 0.0,
        tolerance: 0,
        gap_close: 0,
        seed_x: 0,
        seed_y: 0,
        interpretation: INKPOD_RANGE_NORMAL,
        trace_shape: INKPOD_TRACE_ROUND,
        view_zoom_q16: 1 << 16,
        ..InkpodSelectionInput::default()
    };
    let mut selection_result = dispatch();

    // SAFETY: All records are complete and the Core is live on this owner thread.
    unsafe {
        assert_eq!(
            inkpod_core_apply_fill_for_editor_target(
                ptr::null_mut(),
                0,
                0,
                ptr::null(),
                ptr::null_mut(),
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_apply_fill_for_editor_target(
                core,
                document.layer_id,
                document.main_plane_id,
                &fill,
                &mut fill_result,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(fill_result.changed_pixel_count, 16);

        let mut editor = editor_state_info();
        assert_eq!(
            inkpod_core_get_editor_state(core, &mut editor),
            INKPOD_STATUS_OK
        );
        assert_eq!(editor.active_layer_id, document.layer_id);
        assert_eq!(editor.active_plane_id, document.color_plane_id);

        let mut live_target =
            editor_state_update(INKPOD_EDITOR_UPDATE_ACTIVE_TARGET, editor.editor_revision);
        live_target.active_layer_id = document.layer_id;
        live_target.active_plane_id = document.main_plane_id;
        let mut live_editor = editor_state_info();
        assert_eq!(
            inkpod_core_update_editor_state(core, &live_target, &mut live_editor),
            INKPOD_STATUS_OK
        );
        assert_eq!(live_editor.active_plane_id, document.main_plane_id);

        assert_eq!(
            inkpod_core_apply_selection_for_editor_target(
                core,
                document.layer_id,
                document.main_plane_id,
                &selection,
                &mut selection_result,
            ),
            INKPOD_STATUS_OK
        );
        let selection_revision = selection_result.revision;

        let mut untouched = dispatch();
        untouched.revision = u64::MAX;
        assert_eq!(
            inkpod_core_select_color_for_editor_target(
                ptr::null_mut(),
                document.layer_id,
                document.color_plane_id,
                &selected_color,
                0,
                0,
                INKPOD_SELECTION_NEW,
                &mut untouched,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(untouched.revision, u64::MAX);
        assert_eq!(
            inkpod_core_select_color_for_editor_target(
                core,
                document.layer_id,
                document.color_plane_id,
                ptr::null(),
                0,
                0,
                INKPOD_SELECTION_NEW,
                &mut untouched,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let short_color = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32 - 1,
            ..selected_color
        };
        assert_eq!(
            inkpod_core_select_color_for_editor_target(
                core,
                document.layer_id,
                document.color_plane_id,
                &short_color,
                0,
                0,
                INKPOD_SELECTION_NEW,
                &mut untouched,
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        let mut short_result = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32 - 1,
            reserved: 0,
            revision: u64::MAX,
            accepted_command_count: u64::MAX,
        };
        assert_eq!(
            inkpod_core_select_color_for_editor_target(
                core,
                document.layer_id,
                document.color_plane_id,
                &selected_color,
                0,
                0,
                INKPOD_SELECTION_NEW,
                &mut short_result,
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );

        assert_eq!(
            inkpod_core_select_color_for_editor_target(
                core,
                document.layer_id,
                document.color_plane_id,
                &selected_color,
                0,
                0,
                INKPOD_SELECTION_NEW,
                &mut selection_result,
            ),
            INKPOD_STATUS_OK
        );
        assert!(selection_result.revision > selection_revision);
        let mut after_color = editor_state_info();
        assert_eq!(
            inkpod_core_get_editor_state(core, &mut after_color),
            INKPOD_STATUS_OK
        );
        assert_eq!(after_color.active_layer_id, document.layer_id);
        assert_eq!(after_color.active_plane_id, document.main_plane_id);

        let mut before_invalid = document_info();
        assert_eq!(
            inkpod_core_get_document_info(core, &mut before_invalid),
            INKPOD_STATUS_OK
        );
        let mut history_before = InkpodHistoryInfo {
            struct_size: size_of::<InkpodHistoryInfo>() as u32,
            reserved: 0,
            cursor: 0,
            item_count: 0,
        };
        assert_eq!(
            inkpod_core_history_info(core, &mut history_before),
            INKPOD_STATUS_OK
        );
        untouched.revision = u64::MAX;
        assert_eq!(
            inkpod_core_select_color_for_editor_target(
                core,
                document.layer_id,
                u64::MAX,
                &selected_color,
                0,
                0,
                INKPOD_SELECTION_NEW,
                &mut untouched,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(untouched.revision, u64::MAX);
        let mut history_after = InkpodHistoryInfo {
            struct_size: size_of::<InkpodHistoryInfo>() as u32,
            reserved: 0,
            cursor: 0,
            item_count: 0,
        };
        assert_eq!(
            inkpod_core_history_info(core, &mut history_after),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (history_after.cursor, history_after.item_count),
            (history_before.cursor, history_before.item_count)
        );
        let mut after_invalid_color = document_info();
        assert_eq!(
            inkpod_core_get_document_info(core, &mut after_invalid_color),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (
                after_invalid_color.document_revision,
                after_invalid_color.flags,
                after_invalid_color.main_plane_checksum,
                after_invalid_color.color_plane_checksum,
            ),
            (
                before_invalid.document_revision,
                before_invalid.flags,
                before_invalid.main_plane_checksum,
                before_invalid.color_plane_checksum,
            )
        );
        let mut editor_after_invalid_color = editor_state_info();
        assert_eq!(
            inkpod_core_get_editor_state(core, &mut editor_after_invalid_color),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            editor_after_invalid_color.editor_revision,
            after_color.editor_revision
        );
        assert_eq!(
            editor_after_invalid_color.editor_digest,
            after_color.editor_digest
        );

        let mut before_invalid = document_info();
        assert_eq!(
            inkpod_core_get_document_info(core, &mut before_invalid),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_apply_selection_for_editor_target(
                core,
                document.layer_id,
                u64::MAX,
                &selection,
                &mut selection_result,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut after_invalid = document_info();
        assert_eq!(
            inkpod_core_get_document_info(core, &mut after_invalid),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (
                after_invalid.document_revision,
                after_invalid.flags,
                after_invalid.main_plane_checksum,
                after_invalid.color_plane_checksum,
            ),
            (
                before_invalid.document_revision,
                before_invalid.flags,
                before_invalid.main_plane_checksum,
                before_invalid.color_plane_checksum,
            )
        );
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

fn export_png(core: *mut InkpodCore) -> Vec<u8> {
    export_png_with_composite(core, 0)
}

fn export_png_with_composite(core: *mut InkpodCore, composite_white: u32) -> Vec<u8> {
    let mut buffer = ptr::null_mut();
    let mut bytes = ptr::null();
    let mut byte_count = 0;
    // SAFETY: The core is live; the returned buffer is viewed before its unique release.
    unsafe {
        assert_eq!(
            inkpod_core_export_common_raster(
                core,
                INKPOD_COMMON_RASTER_PNG,
                composite_white,
                &mut buffer,
            ),
            INKPOD_STATUS_OK
        );
        assert!(!buffer.is_null());
        assert_eq!(
            inkpod_byte_buffer_view(buffer, &mut bytes, &mut byte_count),
            INKPOD_STATUS_OK
        );
        assert!(!bytes.is_null());
        assert!(byte_count > 0);
        let owned = std::slice::from_raw_parts(bytes, byte_count as usize).to_vec();
        assert_eq!(inkpod_byte_buffer_release(&mut buffer), INKPOD_STATUS_OK);
        assert!(buffer.is_null());
        assert_eq!(inkpod_byte_buffer_release(&mut buffer), INKPOD_STATUS_OK);
        owned
    }
}

fn rectangle_selection(core: *mut InkpodCore, bounds: InkpodFrameRect) -> InkpodDispatchResult {
    let input = InkpodSelectionInput {
        struct_size: size_of::<InkpodSelectionInput>() as u32,
        shape: INKPOD_SELECTION_RECTANGLE,
        operation: INKPOD_SELECTION_NEW,
        reserved: 0,
        bounds,
        points: ptr::null(),
        point_count: 0,
        point_stride_bytes: 0,
        diameter: 0.0,
        tolerance: 0,
        gap_close: 0,
        seed_x: 0,
        seed_y: 0,
        interpretation: INKPOD_RANGE_NORMAL,
        trace_shape: INKPOD_TRACE_ROUND,
        view_zoom_q16: 1 << 16,
        ..InkpodSelectionInput::default()
    };
    let mut result = dispatch();
    // SAFETY: Core and complete, non-overlapping input/output records live for the call.
    unsafe {
        assert_eq!(
            inkpod_core_apply_selection(core, &input, &mut result),
            INKPOD_STATUS_OK
        );
    }
    result
}

#[test]
fn clear_selected_content_journal_saves_and_reopens_through_the_abi() {
    let (mut core, _) = create_core(8, 8, 0x434c_4541_5253_4156);
    let normal_path = temporary_inkpod_path("clear-selected-normal");
    let recovery_path = temporary_inkpod_path("clear-selected-recovery");
    let normal_bytes = normal_path.to_string_lossy().into_owned().into_bytes();
    let recovery_bytes = recovery_path.to_string_lossy().into_owned().into_bytes();
    let sample = InkpodStrokeSample {
        struct_size: size_of::<InkpodStrokeSample>() as u32,
        flags: 0,
        x: 2.0,
        y: 3.0,
        pressure: 1.0,
        reserved: 0,
    };
    let stroke = InkpodStrokeInput {
        struct_size: size_of::<InkpodStrokeInput>() as u32,
        tool: INKPOD_TOOL_PENCIL,
        plane: INKPOD_PLANE_COLOR,
        coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
        flags: 0,
        color_rgba: 0x0c22_38ff,
        diameter: 1.0,
        samples: &sample,
        sample_count: 1,
        sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
        shape: INKPOD_BRUSH_ROUND,
        smoothing: 0,
        reserved_2: 0,
        start_color: INKPOD_START_COLOR_ANY,
        reserved_3: 0,
    };
    let mut result = dispatch();
    let mut info = document_info();

    unsafe {
        assert_eq!(
            inkpod_core_set_active_plane(core, INKPOD_PLANE_COLOR),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_apply_stroke(core, &stroke, &mut result),
            INKPOD_STATUS_OK
        );
    }
    rectangle_selection(
        core,
        InkpodFrameRect {
            x: 2,
            y: 3,
            width: 1,
            height: 1,
        },
    );
    unsafe {
        assert_eq!(
            inkpod_core_clear_selected_content(core, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_autosave(
                core,
                recovery_bytes.as_ptr(),
                recovery_bytes.len() as u64,
                &mut info,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_save(
                core,
                normal_bytes.as_ptr(),
                normal_bytes.len() as u64,
                &mut info,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);

        assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_open(
                core,
                normal_bytes.as_ptr(),
                normal_bytes.len() as u64,
                &mut info,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_undo(core, &mut result), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_redo(core, &mut result), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }

    std::fs::remove_file(normal_path).unwrap();
    std::fs::remove_file(recovery_path).unwrap();
}

#[test]
fn ffi_contract_document_history_selection_clipboard_and_raster_round_trip() {
    let (mut core, mut info) = create_core(6, 4, 1);
    let save_path = temporary_inkpod_path("document");
    let base_layer_id = info.layer_id;
    let base_main_plane_id = info.main_plane_id;
    let base_color_plane_id = info.color_plane_id;
    let mut result = dispatch();

    let frames = InkpodPaperFramesInput {
        struct_size: size_of::<InkpodPaperFramesInput>() as u32,
        reserved: 0,
        feature_flags: 0,
        hundred_frame: info.hundred_frame,
        reference_frame: info.reference_frame,
        drawing_frame: info.drawing_frame,
        safe_frame: info.safe_frame,
        shooting_frame: info.shooting_frame,
        maximum_close_frame: info.maximum_close_frame,
        margin_left: 1,
        margin_top: 1,
        margin_right: 1,
        margin_bottom: 1,
    };
    let convert = InkpodTreeEdit {
        struct_size: size_of::<InkpodTreeEdit>() as u32,
        operation: INKPOD_TREE_CONVERT_LAYER,
        flags: 0,
        object_id: base_layer_id,
        parent_id: 0,
        destination_index: 0,
        kind: INKPOD_LAYER_GRAYSCALE_COLORING,
        pixel_format: 0,
        opacity_milli: 0,
        name_utf8: ptr::null(),
        name_bytes: 0,
    };
    let mut converted_layer_id = 0;

    // SAFETY: Every pointer refers to a complete live record and the core stays on its owner thread.
    unsafe {
        assert_eq!(
            inkpod_core_update_paper_frames(core, &frames, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_tree_edit(core, &convert, &mut result, &mut converted_layer_id),
            INKPOD_STATUS_OK
        );
        assert_eq!(converted_layer_id, 0);
        assert_eq!(
            inkpod_core_set_active_node(core, base_layer_id, base_main_plane_id),
            INKPOD_STATUS_OK
        );

        let main_line = color(12, 34, 56, 255);
        assert_eq!(
            inkpod_core_set_main_line_color(core, &main_line, &mut result),
            INKPOD_STATUS_OK
        );
        let mut copied_main_line = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            ..InkpodColorValue::default()
        };
        assert_eq!(
            inkpod_core_get_main_line_color(core, &mut copied_main_line),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (
                copied_main_line.red,
                copied_main_line.green,
                copied_main_line.blue,
                copied_main_line.alpha,
            ),
            (12, 34, 56, 255)
        );
        assert_eq!(
            inkpod_core_palette_generate(core, 8, 5, &mut result),
            INKPOD_STATUS_OK
        );

        let mut history = InkpodHistoryInfo {
            struct_size: size_of::<InkpodHistoryInfo>() as u32,
            reserved: 0,
            cursor: 0,
            item_count: 0,
        };
        assert_eq!(
            inkpod_core_history_info(core, &mut history),
            INKPOD_STATUS_OK
        );
        assert!(history.item_count >= 3);
        assert_eq!(history.cursor, history.item_count);

        let mut history_item = InkpodHistoryItem {
            struct_size: size_of::<InkpodHistoryItem>() as u32,
            flags: 0,
            index: 0,
            entry_kind: 0,
            reserved: 0,
        };
        assert_eq!(
            inkpod_core_history_item(core, 0, &mut history_item),
            INKPOD_STATUS_OK
        );
        assert!(matches!(
            history_item.entry_kind,
            INKPOD_HISTORY_ENTRY_RASTER
                | INKPOD_HISTORY_ENTRY_PALETTE
                | INKPOD_HISTORY_ENTRY_COLOR_CHART
                | INKPOD_HISTORY_ENTRY_MAIN_LINE_COLOR
                | INKPOD_HISTORY_ENTRY_DOCUMENT
        ));
        assert_eq!(history_item.reserved, 0);

        assert_eq!(
            inkpod_core_history_jump(core, 0, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_history_jump(core, history.item_count, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_set_active_node(core, base_layer_id, base_color_plane_id),
            INKPOD_STATUS_OK
        );
    }

    rectangle_selection(
        core,
        InkpodFrameRect {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        },
    );
    let selection_name = b"Contract selection";
    let mut selection_layer_id = 0;
    // SAFETY: The name span and all output records remain live and non-overlapping.
    unsafe {
        assert_eq!(
            inkpod_core_selection_adjust(core, INKPOD_SELECTION_ADJUST_EXPAND, 1, &mut result,),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_selection_to_layer(
                core,
                selection_name.as_ptr(),
                selection_name.len() as u64,
                &mut result,
                &mut selection_layer_id,
            ),
            INKPOD_STATUS_OK
        );
        assert_ne!(selection_layer_id, 0);
        assert_eq!(
            inkpod_core_selection_clear(core, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_selection_from_layer(
                core,
                selection_layer_id,
                INKPOD_SELECTION_LAYER_REPLACE,
                &mut result,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_set_active_node(core, base_layer_id, base_color_plane_id),
            INKPOD_STATUS_OK
        );
    }
    save_document(core, &save_path);

    let expected_source_pixels = [
        255_u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
    ];
    let mut source_pixels = expected_source_pixels;
    let clipboard_input = InkpodClipboardRgbaInput {
        struct_size: size_of::<InkpodClipboardRgbaInput>() as u32,
        reserved: 0,
        origin_x: 1,
        origin_y: 1,
        width: 2,
        height: 2,
        pixels_rgba8: source_pixels.as_ptr(),
        pixel_bytes: source_pixels.len() as u64,
        row_stride_bytes: 8,
    };
    let mut clipboard = ptr::null_mut();
    // SAFETY: Input pixels live through creation; the clipboard is uniquely released after use.
    unsafe {
        assert_eq!(
            inkpod_clipboard_create_rgba8(&clipboard_input, &mut clipboard),
            INKPOD_STATUS_OK
        );
        source_pixels.fill(0);
        let mut raster = InkpodClipboardRasterBuffer {
            struct_size: size_of::<InkpodClipboardRasterBuffer>() as u32,
            reserved: 0,
            origin_x: 0,
            origin_y: 0,
            width: 0,
            height: 0,
            pixels_rgba8: ptr::null_mut(),
            pixel_capacity: 0,
            required_bytes: 0,
            row_stride_bytes: 0,
        };
        assert_eq!(
            inkpod_clipboard_render_rgba8(clipboard, &mut raster),
            INKPOD_STATUS_OK
        );
        assert_eq!(raster.required_bytes, expected_source_pixels.len() as u64);
        let mut too_small = vec![0_u8; raster.required_bytes as usize - 1];
        raster.pixels_rgba8 = too_small.as_mut_ptr();
        raster.pixel_capacity = too_small.len() as u64;
        assert_eq!(
            inkpod_clipboard_render_rgba8(clipboard, &mut raster),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        let mut rendered = vec![0_u8; raster.required_bytes as usize];
        raster.pixels_rgba8 = rendered.as_mut_ptr();
        raster.pixel_capacity = rendered.len() as u64;
        assert_eq!(
            inkpod_clipboard_render_rgba8(clipboard, &mut raster),
            INKPOD_STATUS_OK
        );
        assert_eq!(rendered, expected_source_pixels);

        let new_plane_name = b"Atomic Paste Plane";
        let new_plane = InkpodTreeEdit {
            struct_size: size_of::<InkpodTreeEdit>() as u32,
            operation: INKPOD_TREE_CREATE_PLANE,
            flags: INKPOD_NODE_VISIBLE | INKPOD_NODE_EDITABLE,
            object_id: 0,
            parent_id: base_layer_id,
            destination_index: 0,
            kind: INKPOD_TYPED_PLANE_RASTER,
            pixel_format: INKPOD_STORAGE_RGBA8,
            opacity_milli: 900,
            name_utf8: new_plane_name.as_ptr(),
            name_bytes: new_plane_name.len() as u64,
        };
        let history_before_new_plane = queried_history_info(core);
        let revision_before_new_plane = queried_document_info(core).document_revision;
        assert_eq!(
            inkpod_core_paste_begin_new_plane(core, clipboard, &new_plane),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            queried_document_info(core).document_revision,
            revision_before_new_plane
        );
        assert_eq!(
            queried_history_info(core).item_count,
            history_before_new_plane.item_count
        );
        assert_eq!(inkpod_core_floating_cancel(core), INKPOD_STATUS_OK);
        assert_eq!(
            queried_history_info(core).item_count,
            history_before_new_plane.item_count
        );
        assert_eq!(
            inkpod_core_paste_begin_new_plane(core, clipboard, &new_plane),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_floating_commit(core, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            queried_history_info(core).item_count,
            history_before_new_plane.item_count + 1
        );
        assert_eq!(inkpod_core_undo(core, &mut result), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_redo(core, &mut result), INKPOD_STATUS_OK);

        assert_eq!(
            inkpod_core_paste_begin_mode(core, clipboard, 2),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_floating_cancel(core), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_paste_begin_mode(core, clipboard, 2),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_floating_commit(core, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_revert_active_selection(core, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_clear_selected_content(core, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_selection_clear(core, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_clipboard_release(&mut clipboard), INKPOD_STATUS_OK);

        assert_eq!(
            inkpod_core_rotate_document(core, 1, &mut result),
            INKPOD_STATUS_OK
        );
        let resize = InkpodDocumentResizeInput {
            struct_size: size_of::<InkpodDocumentResizeInput>() as u32,
            anchor: 3,
            flags: 0,
            width: 7,
            height: 5,
            dpi_x_milli: 120_000,
            dpi_y_milli: 120_000,
        };
        assert_eq!(
            inkpod_core_resize_document(core, &resize, &mut result),
            INKPOD_STATUS_OK
        );

        let mut png = export_png(core);
        let before_failed_import = queried_document_info(core);
        let before_failed_import_history = queried_history_info(core);
        let malformed_raster = b"not a valid PNG";
        let mut rejected_info = document_info();
        rejected_info.flags = u32::MAX;
        rejected_info.document_revision = u64::MAX;
        rejected_info.width = u32::MAX;
        rejected_info.color_plane_checksum = u64::MAX;
        assert_eq!(
            inkpod_core_import_common_raster(
                core,
                INKPOD_COMMON_RASTER_PNG,
                malformed_raster.as_ptr(),
                malformed_raster.len() as u64,
                0x494e_4b50_4f44_494d,
                2,
                &mut rejected_info,
            ),
            INKPOD_STATUS_IO_ERROR
        );
        assert_eq!(
            (
                rejected_info.flags,
                rejected_info.document_revision,
                rejected_info.width,
                rejected_info.color_plane_checksum,
            ),
            (u32::MAX, u64::MAX, u32::MAX, u64::MAX)
        );
        let after_failed_import = queried_document_info(core);
        let after_failed_import_history = queried_history_info(core);
        assert_eq!(
            (
                after_failed_import.flags,
                after_failed_import.document_revision,
                after_failed_import.view_revision,
                after_failed_import.document_id,
                after_failed_import.layer_id,
                after_failed_import.main_plane_id,
                after_failed_import.color_plane_id,
            ),
            (
                before_failed_import.flags,
                before_failed_import.document_revision,
                before_failed_import.view_revision,
                before_failed_import.document_id,
                before_failed_import.layer_id,
                before_failed_import.main_plane_id,
                before_failed_import.color_plane_id,
            )
        );
        assert_eq!(
            (
                after_failed_import.document_uuid_high,
                after_failed_import.document_uuid_low,
                after_failed_import.width,
                after_failed_import.height,
                after_failed_import.dpi_x_milli,
                after_failed_import.dpi_y_milli,
                after_failed_import.main_plane_checksum,
                after_failed_import.color_plane_checksum,
            ),
            (
                before_failed_import.document_uuid_high,
                before_failed_import.document_uuid_low,
                before_failed_import.width,
                before_failed_import.height,
                before_failed_import.dpi_x_milli,
                before_failed_import.dpi_y_milli,
                before_failed_import.main_plane_checksum,
                before_failed_import.color_plane_checksum,
            )
        );
        assert_eq!(
            (
                after_failed_import_history.cursor,
                after_failed_import_history.item_count,
            ),
            (
                before_failed_import_history.cursor,
                before_failed_import_history.item_count,
            )
        );
        assert_eq!(export_png(core), png);

        info = document_info();
        assert_eq!(
            inkpod_core_import_common_raster(
                core,
                INKPOD_COMMON_RASTER_PNG,
                png.as_ptr(),
                png.len() as u64,
                0x494e_4b50_4f44_494d,
                1,
                &mut info,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!((info.width, info.height), (7, 5));
        let imported_png = export_png(core);
        assert_eq!(imported_png, png);
        png.fill(0);
        drop(png);
        assert_eq!(export_png(core), imported_png);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
    std::fs::remove_file(save_path).unwrap();
}

#[test]
fn ffi_import_accepts_rle_black_and_white_tga_through_the_shared_format_id() {
    let (mut core, _) = create_core(1, 1, 0x494e_4b50_4f44_5447);
    let mut tga = vec![0_u8; 18];
    tga[2] = 11;
    tga[12..14].copy_from_slice(&2_u16.to_le_bytes());
    tga[14..16].copy_from_slice(&1_u16.to_le_bytes());
    tga[16] = 8;
    tga[17] = 0x20;
    tga.extend_from_slice(&[0x81, 77]);
    let mut info = document_info();

    unsafe {
        assert_eq!(
            inkpod_core_import_common_raster(
                core,
                INKPOD_COMMON_RASTER_TGA,
                tga.as_ptr(),
                tga.len() as u64,
                0x494e_4b50_4f44_5447,
                1,
                &mut info,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!((info.width, info.height), (2, 1));
        let exported = export_png(core);
        let raster =
            inkpod_format::decode_common_raster(inkpod_format::CommonRasterFormat::Png, &exported)
                .unwrap();
        assert_eq!(raster.pixels, [77, 77, 77, 255, 77, 77, 77, 255]);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn ffi_color_chart_preview_owns_copies_applies_and_rejects_double_release() {
    let (mut core, _) = create_core(2, 1, 1);
    let red_name = b"Red";
    let red = InkpodColorChartEntry {
        struct_size: size_of::<InkpodColorChartEntry>() as u32,
        reserved: 0,
        feature_flags: INKPOD_FEATURE_NONE,
        color: color(255, 0, 0, 255),
        name_utf8: red_name.as_ptr(),
        name_bytes: red_name.len() as u64,
    };
    let mut result = dispatch();
    // SAFETY: All records and borrowed name bytes remain live for each call.
    unsafe {
        assert_eq!(
            inkpod_core_color_chart_set(
                core,
                &red,
                1,
                size_of::<InkpodColorChartEntry>() as u64,
                0,
                &mut result,
            ),
            INKPOD_STATUS_OK
        );
    }
    let before_preview = queried_document_info(core);
    let history_before = queried_history_info(core);
    let mut cancelled_task = ptr::null_mut();
    let mut cancelled_preview = ptr::null_mut();
    let mut cancelled_summary = InkpodColorChartPreviewSummary {
        struct_size: size_of::<InkpodColorChartPreviewSummary>() as u32,
        ..InkpodColorChartPreviewSummary::default()
    };
    // SAFETY: The task owner starts null, remains live through the worker call,
    // and is released only after the call has stopped borrowing it.
    unsafe {
        assert_eq!(inkpod_task_create(&mut cancelled_task), INKPOD_STATUS_OK);
        assert_eq!(inkpod_task_cancel(cancelled_task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_color_chart_preview_create_task(
                core,
                8,
                0,
                cancelled_task,
                &mut cancelled_summary,
                &mut cancelled_preview,
            ),
            INKPOD_STATUS_CANCELLED
        );
        assert!(cancelled_preview.is_null());
        assert_eq!(inkpod_task_release(&mut cancelled_task), INKPOD_STATUS_OK);
    }
    assert_eq!(
        document_observation(&queried_document_info(core)),
        document_observation(&before_preview)
    );
    assert_eq!(queried_history_info(core).cursor, history_before.cursor);

    let mut preview = ptr::null_mut();
    let mut summary = InkpodColorChartPreviewSummary {
        struct_size: size_of::<InkpodColorChartPreviewSummary>() as u32,
        ..InkpodColorChartPreviewSummary::default()
    };
    // SAFETY: Owner starts null; summary is complete and writable.
    unsafe {
        assert_eq!(
            inkpod_core_color_chart_preview_create(core, 8, 0, &mut summary, &mut preview),
            INKPOD_STATUS_OK
        );
    }
    assert!(!preview.is_null());
    assert_eq!(
        summary.base_document_revision,
        before_preview.document_revision
    );
    assert_eq!(summary.entry_count, 1);
    assert_eq!(summary.retained_color_count, 0);
    assert_eq!(summary.added_color_count, 1);
    assert_eq!(
        document_observation(&queried_document_info(core)),
        document_observation(&before_preview)
    );
    let history_after_preview = queried_history_info(core);
    assert_eq!(history_after_preview.cursor, history_before.cursor);
    assert_eq!(history_after_preview.item_count, history_before.item_count);

    let mut copied = InkpodColorValue {
        struct_size: size_of::<InkpodColorValue>() as u32,
        ..InkpodColorValue::default()
    };
    let mut name_bytes = 0_u64;
    let mut frequency = 0_u64;
    // SAFETY: Preview is live and output records are writable.
    unsafe {
        assert_eq!(
            inkpod_color_chart_preview_get(
                preview,
                0,
                &mut copied,
                ptr::null_mut(),
                0,
                &mut name_bytes,
                &mut frequency,
            ),
            INKPOD_STATUS_OK
        );
    }
    assert_eq!(
        (copied.red, copied.green, copied.blue, copied.alpha),
        (255, 255, 255, 255)
    );
    assert_eq!(frequency, 2);
    let mut short = vec![0_u8; name_bytes.saturating_sub(1) as usize];
    // SAFETY: Deliberately undersized writable buffer exercises the negative contract.
    unsafe {
        assert_eq!(
            inkpod_color_chart_preview_get(
                preview,
                0,
                &mut copied,
                short.as_mut_ptr(),
                short.len() as u64,
                &mut name_bytes,
                &mut frequency,
            ),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
    }
    let mut name = vec![0_u8; name_bytes as usize];
    let (mut other_core, _) = create_core(2, 1, 2);
    let mut other_result = dispatch();
    // SAFETY: The second Core has a distinct document UUID but the same
    // revision, proving that the preview token is not revision-only.
    unsafe {
        assert_eq!(
            inkpod_core_color_chart_set(
                other_core,
                &red,
                1,
                size_of::<InkpodColorChartEntry>() as u64,
                0,
                &mut other_result,
            ),
            INKPOD_STATUS_OK
        );
        let other_before = queried_document_info(other_core);
        assert_eq!(
            inkpod_core_color_chart_preview_apply(other_core, preview, &mut other_result),
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(
            document_observation(&queried_document_info(other_core)),
            document_observation(&other_before)
        );
        assert_eq!(inkpod_core_destroy(&mut other_core), INKPOD_STATUS_OK);
    }
    // SAFETY: Exact-capacity name output and live preview satisfy the contract.
    unsafe {
        assert_eq!(
            inkpod_color_chart_preview_get(
                preview,
                0,
                &mut copied,
                name.as_mut_ptr(),
                name.len() as u64,
                &mut name_bytes,
                &mut frequency,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_color_chart_preview_apply(core, preview, &mut result),
            INKPOD_STATUS_OK
        );
    }
    assert_eq!(std::str::from_utf8(&name).unwrap(), "Color 1");
    assert_eq!(
        queried_history_info(core).item_count,
        history_before.item_count + 1
    );

    let mut info = InkpodColorChartInfo {
        struct_size: size_of::<InkpodColorChartInfo>() as u32,
        ..InkpodColorChartInfo::default()
    };
    let mut committed_color = InkpodColorValue {
        struct_size: size_of::<InkpodColorValue>() as u32,
        ..InkpodColorValue::default()
    };
    let mut committed_name_bytes = 0_u64;
    // SAFETY: Chart query outputs are complete and writable; null storage with
    // zero capacity is the documented name-size query.
    unsafe {
        assert_eq!(
            inkpod_core_color_chart_info(core, &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(info.entry_count, 1);
        assert_eq!(
            inkpod_core_color_chart_get(
                core,
                0,
                &mut committed_color,
                ptr::null_mut(),
                0,
                &mut committed_name_bytes,
            ),
            INKPOD_STATUS_OK
        );
    }
    let mut committed_name = vec![0_u8; committed_name_bytes as usize];
    // SAFETY: Exact-capacity name storage and unique release owners satisfy the contract.
    unsafe {
        assert_eq!(
            inkpod_core_color_chart_get(
                core,
                0,
                &mut committed_color,
                committed_name.as_mut_ptr(),
                committed_name.len() as u64,
                &mut committed_name_bytes,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(std::str::from_utf8(&committed_name).unwrap(), "Color 1");
        assert_eq!(
            inkpod_color_chart_preview_release(&mut preview),
            INKPOD_STATUS_OK
        );
        assert!(preview.is_null());
        assert_eq!(
            inkpod_color_chart_preview_release(&mut preview),
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn ffi_contract_light_table_sequence_and_owned_buffers() {
    let (mut source_core, _) = create_core(3, 2, 2);
    let png = export_png_with_composite(source_core, 1);
    // SAFETY: The source core is live, uniquely owned, and destroyed on its owner thread.
    unsafe {
        assert_eq!(inkpod_core_destroy(&mut source_core), INKPOD_STATUS_OK);
    }

    let (mut light_core, light_info) = create_core(3, 2, 3);
    let sample_x = u32::try_from(light_info.reference_frame.x).unwrap();
    let sample_y = u32::try_from(light_info.reference_frame.y).unwrap();
    let expected_item_name = b"encoded reference".to_vec();
    let mut item_name = expected_item_name.clone();
    let mut result = dispatch();
    let mut item_id = 0;
    let mut add_png = png.clone();
    // SAFETY: Encoded bytes/name and all output records remain live for each call.
    unsafe {
        assert_eq!(
            inkpod_core_light_table_add_common_raster(
                light_core,
                INKPOD_COMMON_RASTER_PNG,
                add_png.as_ptr(),
                add_png.len() as u64,
                item_name.as_ptr(),
                item_name.len() as u64,
                7,
                8,
                9,
                &mut result,
                &mut item_id,
            ),
            INKPOD_STATUS_OK
        );
        assert_ne!(item_id, 0);
        item_name.fill(0);
        drop(item_name);
        add_png.fill(0);
        drop(add_png);

        let mut retained_sample = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            ..InkpodColorValue::default()
        };
        assert_eq!(
            inkpod_core_light_table_sample(light_core, sample_x, sample_y, &mut retained_sample),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (
                retained_sample.red,
                retained_sample.green,
                retained_sample.blue,
                retained_sample.alpha,
            ),
            (255, 255, 255, 255)
        );

        let before_failed_add = queried_document_info(light_core);
        let before_failed_add_history = queried_history_info(light_core);
        let mut before_failed_add_set = InkpodLightTableSetInfo {
            struct_size: size_of::<InkpodLightTableSetInfo>() as u32,
            ..InkpodLightTableSetInfo::default()
        };
        assert_eq!(
            inkpod_core_light_table_set_get(light_core, 0, &mut before_failed_add_set),
            INKPOD_STATUS_OK
        );
        let malformed_raster = b"not a valid PNG";
        let rejected_name = b"rejected reference";
        let mut rejected_add_result = dispatch();
        rejected_add_result.revision = u64::MAX - 1;
        rejected_add_result.accepted_command_count = u64::MAX - 2;
        let mut rejected_item_id = u64::MAX - 3;
        assert_eq!(
            inkpod_core_light_table_add_common_raster(
                light_core,
                INKPOD_COMMON_RASTER_PNG,
                malformed_raster.as_ptr(),
                malformed_raster.len() as u64,
                rejected_name.as_ptr(),
                rejected_name.len() as u64,
                7,
                8,
                10,
                &mut rejected_add_result,
                &mut rejected_item_id,
            ),
            INKPOD_STATUS_IO_ERROR
        );
        assert_eq!(rejected_add_result.revision, u64::MAX - 1);
        assert_eq!(rejected_add_result.accepted_command_count, u64::MAX - 2);
        assert_eq!(rejected_item_id, u64::MAX - 3);
        let after_failed_add = queried_document_info(light_core);
        let after_failed_add_history = queried_history_info(light_core);
        let mut after_failed_add_set = InkpodLightTableSetInfo {
            struct_size: size_of::<InkpodLightTableSetInfo>() as u32,
            ..InkpodLightTableSetInfo::default()
        };
        assert_eq!(
            inkpod_core_light_table_set_get(light_core, 0, &mut after_failed_add_set),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (
                after_failed_add.document_revision,
                after_failed_add.flags,
                after_failed_add.main_plane_checksum,
                after_failed_add.color_plane_checksum,
            ),
            (
                before_failed_add.document_revision,
                before_failed_add.flags,
                before_failed_add.main_plane_checksum,
                before_failed_add.color_plane_checksum,
            )
        );
        assert_eq!(
            (
                after_failed_add_history.cursor,
                after_failed_add_history.item_count,
            ),
            (
                before_failed_add_history.cursor,
                before_failed_add_history.item_count,
            )
        );
        assert_eq!(
            after_failed_add_set.item_count,
            before_failed_add_set.item_count
        );
        assert_eq!(
            inkpod_core_light_table_sample(light_core, sample_x, sample_y, &mut retained_sample),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (
                retained_sample.red,
                retained_sample.green,
                retained_sample.blue,
                retained_sample.alpha,
            ),
            (255, 255, 255, 255)
        );

        assert_eq!(inkpod_core_undo(light_core, &mut result), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_light_table_sample(light_core, sample_x, sample_y, &mut retained_sample),
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(inkpod_core_redo(light_core, &mut result), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_light_table_sample(light_core, sample_x, sample_y, &mut retained_sample),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (
                retained_sample.red,
                retained_sample.green,
                retained_sample.blue,
                retained_sample.alpha,
            ),
            (255, 255, 255, 255)
        );

        let mut set_info = InkpodLightTableSetInfo {
            struct_size: size_of::<InkpodLightTableSetInfo>() as u32,
            ..InkpodLightTableSetInfo::default()
        };
        assert_eq!(
            inkpod_core_light_table_set_get(light_core, 0, &mut set_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(set_info.item_count, 1);
        let mut set_name = vec![0_u8; set_info.name_bytes as usize];
        set_info.name_utf8 = set_name.as_mut_ptr();
        set_info.name_capacity = set_name.len() as u64;
        assert_eq!(
            inkpod_core_light_table_set_get(light_core, 0, &mut set_info),
            INKPOD_STATUS_OK
        );

        let mut item_info = InkpodLightTableItemInfo {
            struct_size: size_of::<InkpodLightTableItemInfo>() as u32,
            display_color: color(0, 0, 0, 0),
            ..InkpodLightTableItemInfo::default()
        };
        assert_eq!(
            inkpod_core_light_table_item_get(light_core, 0, &mut item_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(item_info.id, item_id);
        let mut copied_item_name = vec![0_u8; item_info.name_bytes as usize];
        item_info.name_utf8 = copied_item_name.as_mut_ptr();
        item_info.name_capacity = copied_item_name.len() as u64;
        assert_eq!(
            inkpod_core_light_table_item_get(light_core, 0, &mut item_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(copied_item_name, expected_item_name);

        let edit = InkpodLightTableEdit {
            struct_size: size_of::<InkpodLightTableEdit>() as u32,
            operation: INKPOD_LIGHT_TABLE_UPDATE_ITEM,
            object_id: item_id,
            destination_index: 0,
            flags: INKPOD_LIGHT_TABLE_ITEM_VISIBLE,
            opacity_milli: 600,
            display_mode: INKPOD_LIGHT_TABLE_MONOTONE,
            display_color: color(32, 64, 96, 255),
            translate_x_milli: 1_000,
            translate_y_milli: -1_000,
            scale_x_milli: 1_000,
            scale_y_milli: 1_000,
            rotation_milli_degrees: 0,
            reserved: 0,
            name_utf8: ptr::null(),
            name_bytes: 0,
        };
        let mut edited_id = 0;
        assert_eq!(
            inkpod_core_light_table_edit(light_core, &edit, &mut result, &mut edited_id),
            INKPOD_STATUS_OK
        );
        assert_eq!(edited_id, item_id);
        let mut reload_png = png.clone();
        assert_eq!(
            inkpod_core_light_table_reload_common_raster(
                light_core,
                item_id,
                INKPOD_COMMON_RASTER_PNG,
                reload_png.as_ptr(),
                reload_png.len() as u64,
                7,
                8,
                42,
                &mut result,
            ),
            INKPOD_STATUS_OK
        );
        reload_png.fill(0);
        drop(reload_png);
        item_info.name_utf8 = ptr::null_mut();
        item_info.name_capacity = 0;
        assert_eq!(
            inkpod_core_light_table_item_get(light_core, 0, &mut item_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(item_info.source_revision, 42);
        assert_eq!(item_info.opacity_milli, 600);

        let before_failed_reload = queried_document_info(light_core);
        let before_failed_reload_history = queried_history_info(light_core);
        let before_failed_reload_item = (
            item_info.id,
            item_info.source_document_uuid_high,
            item_info.source_document_uuid_low,
            item_info.source_revision,
            item_info.opacity_milli,
            item_info.display_mode,
            item_info.translate_x_milli,
            item_info.translate_y_milli,
            item_info.name_bytes,
        );
        let mut rejected_reload_result = dispatch();
        rejected_reload_result.revision = u64::MAX - 4;
        rejected_reload_result.accepted_command_count = u64::MAX - 5;
        assert_eq!(
            inkpod_core_light_table_reload_common_raster(
                light_core,
                item_id,
                INKPOD_COMMON_RASTER_PNG,
                malformed_raster.as_ptr(),
                malformed_raster.len() as u64,
                7,
                8,
                43,
                &mut rejected_reload_result,
            ),
            INKPOD_STATUS_IO_ERROR
        );
        assert_eq!(rejected_reload_result.revision, u64::MAX - 4);
        assert_eq!(rejected_reload_result.accepted_command_count, u64::MAX - 5);
        let after_failed_reload = queried_document_info(light_core);
        let after_failed_reload_history = queried_history_info(light_core);
        item_info.name_utf8 = ptr::null_mut();
        item_info.name_capacity = 0;
        assert_eq!(
            inkpod_core_light_table_item_get(light_core, 0, &mut item_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (
                after_failed_reload.document_revision,
                after_failed_reload.flags,
                after_failed_reload.main_plane_checksum,
                after_failed_reload.color_plane_checksum,
            ),
            (
                before_failed_reload.document_revision,
                before_failed_reload.flags,
                before_failed_reload.main_plane_checksum,
                before_failed_reload.color_plane_checksum,
            )
        );
        assert_eq!(
            (
                after_failed_reload_history.cursor,
                after_failed_reload_history.item_count,
            ),
            (
                before_failed_reload_history.cursor,
                before_failed_reload_history.item_count,
            )
        );
        assert_eq!(
            (
                item_info.id,
                item_info.source_document_uuid_high,
                item_info.source_document_uuid_low,
                item_info.source_revision,
                item_info.opacity_milli,
                item_info.display_mode,
                item_info.translate_x_milli,
                item_info.translate_y_milli,
                item_info.name_bytes,
            ),
            before_failed_reload_item
        );
        assert_eq!(
            inkpod_core_light_table_sample(
                light_core,
                sample_x + 1,
                sample_y,
                &mut retained_sample,
            ),
            INKPOD_STATUS_OK
        );

        assert_eq!(inkpod_core_undo(light_core, &mut result), INKPOD_STATUS_OK);
        item_info.name_utf8 = ptr::null_mut();
        item_info.name_capacity = 0;
        assert_eq!(
            inkpod_core_light_table_item_get(light_core, 0, &mut item_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(item_info.source_revision, 9);
        assert_eq!(inkpod_core_redo(light_core, &mut result), INKPOD_STATUS_OK);
        item_info.name_utf8 = ptr::null_mut();
        item_info.name_capacity = 0;
        assert_eq!(
            inkpod_core_light_table_item_get(light_core, 0, &mut item_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(item_info.source_revision, 42);
        assert_eq!(
            inkpod_core_light_table_sample(
                light_core,
                sample_x + 1,
                sample_y,
                &mut retained_sample,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_destroy(&mut light_core), INKPOD_STATUS_OK);
    }

    let (mut sequence_core, _) = create_core(1, 1, 4);
    let clean_path = temporary_inkpod_path("sequence");
    let source_recovery_path = temporary_inkpod_path("sequence-source-recovery");
    let target_recovery_path = temporary_inkpod_path("sequence-target-recovery");
    save_document(sequence_core, &clean_path);
    let clean_file_bytes = std::fs::read(&clean_path).unwrap();
    let names = [b"cell10.png".as_slice(), b"cell2.png".as_slice()];
    let mut files = [
        InkpodNamedBytesInput {
            struct_size: size_of::<InkpodNamedBytesInput>() as u32,
            reserved: 0,
            name_utf8: names[0].as_ptr(),
            name_bytes: names[0].len() as u64,
            bytes: png.as_ptr(),
            byte_count: png.len() as u64,
        },
        InkpodNamedBytesInput {
            struct_size: size_of::<InkpodNamedBytesInput>() as u32,
            reserved: 0,
            name_utf8: names[1].as_ptr(),
            name_bytes: names[1].len() as u64,
            bytes: png.as_ptr(),
            byte_count: png.len() as u64,
        },
    ];
    let (mut binding_core, _) = create_core(1, 1, 5);
    // SAFETY: Every handle, record and nested encoded/name span is live on the owner thread.
    unsafe {
        let mut imported = document_info();
        assert_eq!(
            inkpod_core_import_common_raster(
                binding_core,
                INKPOD_COMMON_RASTER_PNG,
                png.as_ptr(),
                png.len() as u64,
                0x494e_4b50_4f44_494d,
                5,
                &mut imported,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_sequence_import_encoded(
                binding_core,
                INKPOD_COMMON_RASTER_PNG,
                files.as_ptr(),
                files.len() as u64,
                size_of::<InkpodNamedBytesInput>() as u64,
            ),
            INKPOD_STATUS_OK
        );
        let before_binding = document_observation(&queried_document_info(binding_core));
        let mut binding_plan = InkpodSequenceActivationPlan {
            struct_size: size_of::<InkpodSequenceActivationPlan>() as u32,
            ..InkpodSequenceActivationPlan::default()
        };
        assert_eq!(
            inkpod_core_sequence_activation_resolve(binding_core, 0, &mut binding_plan),
            INKPOD_STATUS_OK
        );
        assert_eq!(binding_plan.result_class, INKPOD_SEQUENCE_ACTIVATION_BIND);
        assert_eq!(binding_plan.source_index, INKPOD_SEQUENCE_INDEX_NONE);
        assert_eq!(binding_plan.source_generation, 0);
        assert_eq!(
            inkpod_core_sequence_activation_commit(binding_core, &binding_plan, &mut imported),
            INKPOD_STATUS_OK
        );
        assert_eq!(document_observation(&imported), before_binding);
        assert_eq!(
            inkpod_core_sequence_activation_commit(binding_core, &binding_plan, &mut imported),
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(
            document_observation(&queried_document_info(binding_core)),
            before_binding
        );
        assert_eq!(inkpod_core_destroy(&mut binding_core), INKPOD_STATUS_OK);
    }
    // SAFETY: The strided records and all nested spans remain live for each owner-thread call.
    unsafe {
        files[1].struct_size = size_of::<u32>() as u32;
        assert_eq!(
            inkpod_core_sequence_import_encoded(
                sequence_core,
                INKPOD_COMMON_RASTER_PNG,
                files.as_ptr(),
                files.len() as u64,
                size_of::<InkpodNamedBytesInput>() as u64,
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        files[1].struct_size = size_of::<InkpodNamedBytesInput>() as u32;
        assert_eq!(
            inkpod_core_sequence_import_encoded(
                sequence_core,
                INKPOD_COMMON_RASTER_PNG,
                files.as_ptr(),
                files.len() as u64,
                size_of::<InkpodNamedBytesInput>() as u64,
            ),
            INKPOD_STATUS_OK
        );

        let mut mixed_files = [
            InkpodNamedRasterInput {
                struct_size: size_of::<InkpodNamedRasterInput>() as u32,
                reserved: 0,
                format: INKPOD_COMMON_RASTER_PNG,
                reserved2: 0,
                name_utf8: names[0].as_ptr(),
                name_bytes: names[0].len() as u64,
                bytes: png.as_ptr(),
                byte_count: png.len() as u64,
            },
            InkpodNamedRasterInput {
                struct_size: size_of::<InkpodNamedRasterInput>() as u32,
                reserved: 0,
                format: INKPOD_COMMON_RASTER_PNG,
                reserved2: 1,
                name_utf8: names[1].as_ptr(),
                name_bytes: names[1].len() as u64,
                bytes: png.as_ptr(),
                byte_count: png.len() as u64,
            },
        ];
        assert_eq!(
            inkpod_core_sequence_import_mixed_encoded(
                sequence_core,
                mixed_files.as_ptr(),
                mixed_files.len() as u64,
                size_of::<InkpodNamedRasterInput>() as u64,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        mixed_files[1].reserved2 = 0;
        assert_eq!(
            inkpod_core_sequence_import_mixed_encoded(
                sequence_core,
                mixed_files.as_ptr(),
                mixed_files.len() as u64,
                size_of::<InkpodNamedRasterInput>() as u64,
            ),
            INKPOD_STATUS_OK
        );

        let mut cell = InkpodSequenceCellInfo {
            struct_size: size_of::<InkpodSequenceCellInfo>() as u32,
            ..InkpodSequenceCellInfo::default()
        };
        assert_eq!(
            inkpod_core_sequence_cell_get(sequence_core, 0, &mut cell),
            INKPOD_STATUS_OK
        );
        assert_eq!(cell.cell_number, 2);
        let mut cell_name = vec![0_u8; cell.name_bytes as usize];
        cell.name_utf8 = cell_name.as_mut_ptr();
        cell.name_capacity = cell_name.len() as u64;
        assert_eq!(
            inkpod_core_sequence_cell_get(sequence_core, 0, &mut cell),
            INKPOD_STATUS_OK
        );
        assert_eq!(cell_name, b"cell2.png");

        let mut thumbnail = InkpodSequenceThumbnailBuffer {
            struct_size: size_of::<InkpodSequenceThumbnailBuffer>() as u32,
            ..InkpodSequenceThumbnailBuffer::default()
        };
        assert_eq!(
            inkpod_core_sequence_thumbnail_get(sequence_core, 0, &mut thumbnail),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            thumbnail.required_bytes,
            u64::from(thumbnail.height) * u64::from(thumbnail.stride_bytes)
        );
        assert_ne!(thumbnail.checksum, 0);
        let mut short_pixels = vec![0_u8; thumbnail.required_bytes as usize - 1];
        thumbnail.pixels_rgba8 = short_pixels.as_mut_ptr();
        thumbnail.pixel_capacity = short_pixels.len() as u64;
        assert_eq!(
            inkpod_core_sequence_thumbnail_get(sequence_core, 0, &mut thumbnail),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        let mut pixels = vec![0_u8; thumbnail.required_bytes as usize];
        thumbnail.pixels_rgba8 = pixels.as_mut_ptr();
        thumbnail.pixel_capacity = pixels.len() as u64;
        assert_eq!(
            inkpod_core_sequence_thumbnail_get(sequence_core, 0, &mut thumbnail),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_sequence_thumbnail_get(sequence_core, 2, &mut thumbnail),
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        assert_eq!(
            inkpod_core_subpalette_set(sequence_core, 0),
            INKPOD_STATUS_OK
        );
        let mut sampled = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            ..InkpodColorValue::default()
        };
        assert_eq!(
            inkpod_core_subpalette_sample(sequence_core, 0, 0, &mut sampled),
            INKPOD_STATUS_OK
        );
        let mut subpalette_view_id = 0_u64;
        assert_eq!(
            inkpod_core_view_create(sequence_core, &mut subpalette_view_id),
            INKPOD_STATUS_OK
        );
        let reference_fit = InkpodViewInput {
            struct_size: size_of::<InkpodViewInput>() as u32,
            kind: INKPOD_VIEW_FIT,
            flags: 0,
            value1: 240.0,
            value2: 120.0,
            value3: 0.0,
            value4: 0.0,
        };
        assert_eq!(
            inkpod_core_subpalette_view_apply(sequence_core, subpalette_view_id, &reference_fit,),
            INKPOD_STATUS_OK
        );
        let mut reference_sample = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            ..InkpodColorValue::default()
        };
        assert_eq!(
            inkpod_core_subpalette_view_sample(
                sequence_core,
                subpalette_view_id,
                120.0,
                60.0,
                &mut reference_sample,
            ),
            INKPOD_STATUS_OK
        );
        let snapshot_options = InkpodSnapshotOptions {
            struct_size: size_of::<InkpodSnapshotOptions>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
        };
        let mut reference_snapshot = ptr::null_mut();
        assert_eq!(
            inkpod_core_subpalette_build_snapshot(
                sequence_core,
                subpalette_view_id,
                &snapshot_options,
                &mut reference_snapshot,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_snapshot_release(&mut reference_snapshot),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_view_close(sequence_core, subpalette_view_id),
            INKPOD_STATUS_OK
        );

        let before_activation = queried_document_info(sequence_core);
        let mut activation = InkpodSequenceActivationPlan {
            struct_size: size_of::<u32>() as u32,
            ..InkpodSequenceActivationPlan::default()
        };
        assert_eq!(
            inkpod_core_sequence_activation_resolve(sequence_core, 0, ptr::null_mut()),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_sequence_activation_resolve(sequence_core, 0, &mut activation),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        activation.struct_size = size_of::<InkpodSequenceActivationPlan>() as u32;
        assert_eq!(
            inkpod_core_sequence_activation_resolve(ptr::null_mut(), 0, &mut activation),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_sequence_activation_resolve(sequence_core, u32::MAX, &mut activation),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_sequence_activation_resolve(sequence_core, 0, &mut activation),
            INKPOD_STATUS_OK
        );
        assert_eq!(activation.result_class, INKPOD_SEQUENCE_ACTIVATION_REPLACE);
        assert_eq!(activation.source_index, INKPOD_SEQUENCE_INDEX_NONE);
        assert_eq!(activation.source_generation, 0);
        let mut active = document_info();
        assert_eq!(
            inkpod_core_sequence_activation_commit(sequence_core, ptr::null(), &mut active),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_sequence_activation_commit(sequence_core, &activation, ptr::null_mut()),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut malformed_activation = activation;
        malformed_activation.struct_size = size_of::<u32>() as u32;
        assert_eq!(
            inkpod_core_sequence_activation_commit(
                sequence_core,
                &malformed_activation,
                &mut active,
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        malformed_activation = activation;
        malformed_activation.feature_flags = 1;
        assert_eq!(
            inkpod_core_sequence_activation_commit(
                sequence_core,
                &malformed_activation,
                &mut active,
            ),
            INKPOD_STATUS_UNSUPPORTED
        );
        malformed_activation = activation;
        malformed_activation.result_class = u32::MAX;
        assert_eq!(
            inkpod_core_sequence_activation_commit(
                sequence_core,
                &malformed_activation,
                &mut active,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        malformed_activation = activation;
        malformed_activation.source_generation = 1;
        assert_eq!(
            inkpod_core_sequence_activation_commit(
                sequence_core,
                &malformed_activation,
                &mut active,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut stale_activation = activation;
        stale_activation.sequence_revision += 1;
        assert_eq!(
            inkpod_core_sequence_activation_commit(sequence_core, &stale_activation, &mut active),
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(
            document_observation(&queried_document_info(sequence_core)),
            document_observation(&before_activation)
        );
        assert_eq!(
            inkpod_core_sequence_activation_commit(sequence_core, &activation, &mut active),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_sequence_activate(sequence_core, 0, &mut active),
            INKPOD_STATUS_OK
        );
        assert_eq!(active.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
        let mut active_editor = editor_state_info();
        assert_eq!(
            inkpod_core_get_editor_state(sequence_core, &mut active_editor),
            INKPOD_STATUS_OK
        );
        assert_eq!(active_editor.flags & INKPOD_EDITOR_STATE_DIRTY, 0);
        assert_eq!(active_editor.active_layer_id, active.layer_id);
        assert_eq!(active_editor.active_plane_id, active.main_plane_id);
        assert_eq!(
            inkpod_core_sequence_activation_resolve(sequence_core, 0, &mut activation),
            INKPOD_STATUS_OK
        );
        assert_eq!(activation.result_class, INKPOD_SEQUENCE_ACTIVATION_NOOP);
        assert_eq!(activation.source_index, 0);
        let before_no_op = document_observation(&active);
        for source_generation in [true, false] {
            stale_activation = activation;
            if source_generation {
                stale_activation.source_generation += 1;
            } else {
                stale_activation.target_source_generation += 1;
            }
            assert_eq!(
                inkpod_core_sequence_activation_commit(
                    sequence_core,
                    &stale_activation,
                    &mut active,
                ),
                INKPOD_STATUS_INVALID_STATE
            );
        }
        assert_eq!(
            inkpod_core_sequence_activation_commit(sequence_core, &activation, &mut active),
            INKPOD_STATUS_OK
        );
        assert_eq!(document_observation(&active), before_no_op);

        let before_stopped_step = queried_document_info(sequence_core);
        let mut step_plan = InkpodSequenceStepPlan {
            struct_size: size_of::<u32>() as u32,
            ..InkpodSequenceStepPlan::default()
        };
        assert_eq!(
            inkpod_core_sequence_step_resolve(
                sequence_core,
                INKPOD_SEQUENCE_PREVIOUS,
                INKPOD_SEQUENCE_ENDPOINT_STOP,
                ptr::null_mut(),
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_sequence_step_resolve(
                sequence_core,
                INKPOD_SEQUENCE_PREVIOUS,
                INKPOD_SEQUENCE_ENDPOINT_STOP,
                &mut step_plan,
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        step_plan.struct_size = size_of::<InkpodSequenceStepPlan>() as u32;
        assert_eq!(
            inkpod_core_sequence_step_resolve(
                sequence_core,
                INKPOD_SEQUENCE_PREVIOUS,
                u32::MAX,
                &mut step_plan,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_sequence_step_resolve(
                sequence_core,
                INKPOD_SEQUENCE_PREVIOUS,
                INKPOD_SEQUENCE_ENDPOINT_STOP,
                &mut step_plan,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(step_plan.result_class, INKPOD_SEQUENCE_STEP_STOPPED);
        assert_eq!(step_plan.source_index, 0);
        assert_eq!(step_plan.target_index, 0);
        assert_eq!(
            inkpod_core_sequence_step_commit(sequence_core, ptr::null(), &mut active),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut malformed_step = step_plan;
        malformed_step.feature_flags = 1;
        assert_eq!(
            inkpod_core_sequence_step_commit(sequence_core, &malformed_step, &mut active),
            INKPOD_STATUS_UNSUPPORTED
        );
        malformed_step = step_plan;
        malformed_step.result_class = u32::MAX;
        assert_eq!(
            inkpod_core_sequence_step_commit(sequence_core, &malformed_step, &mut active),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut stale_step = step_plan;
        stale_step.sequence_revision += 1;
        assert_eq!(
            inkpod_core_sequence_step_commit(sequence_core, &stale_step, &mut active),
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(
            inkpod_core_sequence_step_commit(sequence_core, &step_plan, &mut active),
            INKPOD_STATUS_OK
        );
        let after_stopped_step = queried_document_info(sequence_core);
        assert_eq!(
            after_stopped_step.document_revision,
            before_stopped_step.document_revision
        );
        assert_eq!(
            after_stopped_step.main_plane_checksum,
            before_stopped_step.main_plane_checksum
        );
        assert_eq!(after_stopped_step.flags, before_stopped_step.flags);
        assert_eq!(
            inkpod_core_sequence_step_resolve(
                sequence_core,
                INKPOD_SEQUENCE_PREVIOUS,
                INKPOD_SEQUENCE_ENDPOINT_WRAP,
                &mut step_plan,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(step_plan.result_class, INKPOD_SEQUENCE_STEP_WRAPPED);
        assert_eq!(step_plan.target_index, 1);

        let mut switch_request = InkpodSequenceSwitchRequest {
            struct_size: size_of::<u32>() as u32,
            ..InkpodSequenceSwitchRequest::default()
        };
        assert_eq!(
            inkpod_core_sequence_switch_request(
                sequence_core,
                1,
                INKPOD_SEQUENCE_SWITCH_AUTOSAVE,
                &mut switch_request,
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        switch_request.struct_size = size_of::<InkpodSequenceSwitchRequest>() as u32;
        assert_eq!(
            inkpod_core_sequence_switch_request(sequence_core, 1, u32::MAX, &mut switch_request,),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_sequence_switch_request(
                sequence_core,
                1,
                INKPOD_SEQUENCE_SWITCH_AUTOSAVE,
                &mut switch_request,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(switch_request.flags, INKPOD_SEQUENCE_SWITCH_REQUIRED);
        assert_eq!(
            inkpod_core_sequence_commit_autosaved_switch(sequence_core, ptr::null(), &mut active,),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut malformed_request = switch_request;
        malformed_request.flags = 0;
        assert_eq!(
            inkpod_core_sequence_commit_autosaved_switch(
                sequence_core,
                &malformed_request,
                &mut active,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut stale_request = switch_request;
        stale_request.source_document_revision += 1;
        assert_eq!(
            inkpod_core_sequence_commit_autosaved_switch(
                sequence_core,
                &stale_request,
                &mut active,
            ),
            INKPOD_STATUS_INVALID_STATE
        );

        let source_recovery_bytes = source_recovery_path
            .to_string_lossy()
            .into_owned()
            .into_bytes();
        assert_eq!(
            inkpod_core_autosave(
                sequence_core,
                source_recovery_bytes.as_ptr(),
                source_recovery_bytes.len() as u64,
                &mut active,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_sequence_commit_autosaved_switch(
                sequence_core,
                &switch_request,
                &mut active,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            active.document_uuid_high,
            switch_request.target_document_uuid_high
        );
        assert_eq!(
            active.document_uuid_low,
            switch_request.target_document_uuid_low
        );
        assert_eq!(active.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
        assert_eq!(
            inkpod_core_get_editor_state(sequence_core, &mut active_editor),
            INKPOD_STATUS_OK
        );
        assert_eq!(active_editor.flags & INKPOD_EDITOR_STATE_DIRTY, 0);

        let mut return_request = InkpodSequenceSwitchRequest {
            struct_size: size_of::<InkpodSequenceSwitchRequest>() as u32,
            ..InkpodSequenceSwitchRequest::default()
        };
        assert_eq!(
            inkpod_core_sequence_switch_request(
                sequence_core,
                0,
                INKPOD_SEQUENCE_SWITCH_AUTOSAVE,
                &mut return_request,
            ),
            INKPOD_STATUS_OK
        );
        let target_recovery_bytes = target_recovery_path
            .to_string_lossy()
            .into_owned()
            .into_bytes();
        assert_eq!(
            inkpod_core_autosave(
                sequence_core,
                target_recovery_bytes.as_ptr(),
                target_recovery_bytes.len() as u64,
                &mut active,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_sequence_restore_autosaved_switch(
                sequence_core,
                &return_request,
                target_recovery_bytes.as_ptr(),
                target_recovery_bytes.len() as u64,
                &mut active,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            queried_document_info(sequence_core).document_uuid_low,
            return_request.source_document_uuid_low
        );
        let missing_path = b"Z:/inkpod-missing/sequence-recovery.inkpod";
        assert_eq!(
            inkpod_core_sequence_restore_autosaved_switch(
                sequence_core,
                &return_request,
                missing_path.as_ptr(),
                missing_path.len() as u64,
                &mut active,
            ),
            INKPOD_STATUS_IO_ERROR
        );
        let after_missing = queried_document_info(sequence_core);
        assert_eq!(
            after_missing.document_uuid_low,
            return_request.source_document_uuid_low
        );
        assert_eq!(
            inkpod_core_sequence_restore_autosaved_switch(
                sequence_core,
                &return_request,
                source_recovery_bytes.as_ptr(),
                source_recovery_bytes.len() as u64,
                &mut active,
            ),
            INKPOD_STATUS_OK
        );
        assert_ne!(active.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
        assert_ne!(active.flags & INKPOD_DOCUMENT_FLAG_RECOVERED, 0);
        assert_eq!(
            active.document_uuid_high,
            return_request.target_document_uuid_high
        );
        assert_eq!(
            active.document_uuid_low,
            return_request.target_document_uuid_low
        );
        assert_eq!(std::fs::read(&clean_path).unwrap(), clean_file_bytes);

        let mut encoded = ptr::null_mut();
        assert_eq!(
            inkpod_core_sequence_export_encoded(
                sequence_core,
                INKPOD_COMMON_RASTER_PNG,
                0,
                &mut encoded,
            ),
            INKPOD_STATUS_OK
        );
        let mut encoded_count = 0;
        assert_eq!(
            inkpod_encoded_sequence_count(encoded, &mut encoded_count),
            INKPOD_STATUS_OK
        );
        assert_eq!(encoded_count, 2);
        let mut encoded_name = ptr::null();
        let mut encoded_name_bytes = 0;
        let mut encoded_bytes = ptr::null();
        let mut encoded_byte_count = 0;
        assert_eq!(
            inkpod_encoded_sequence_get(
                encoded,
                0,
                &mut encoded_name,
                &mut encoded_name_bytes,
                &mut encoded_bytes,
                &mut encoded_byte_count,
            ),
            INKPOD_STATUS_OK
        );
        assert!(!encoded_name.is_null());
        assert!(!encoded_bytes.is_null());
        assert!(encoded_name_bytes > 0 && encoded_byte_count > 0);
        assert_eq!(
            inkpod_encoded_sequence_release(&mut encoded),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_encoded_sequence_release(&mut encoded),
            INKPOD_STATUS_OK
        );

        let motion = InkpodMotionCheckInput {
            struct_size: size_of::<InkpodMotionCheckInput>() as u32,
            fps: 24,
            flags: INKPOD_MOTION_FLAG_LOOP,
        };
        let mut frame = InkpodMotionFrame {
            struct_size: size_of::<InkpodMotionFrame>() as u32,
            ..InkpodMotionFrame::default()
        };
        assert_eq!(
            inkpod_core_motion_check_start(sequence_core, &motion, &mut frame),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_motion_check_toggle_pause(sequence_core, &mut frame),
            INKPOD_STATUS_OK
        );
        assert_ne!(frame.flags & INKPOD_MOTION_FRAME_PAUSED, 0);
        assert_eq!(
            inkpod_core_motion_check_stop(sequence_core),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_destroy(&mut sequence_core), INKPOD_STATUS_OK);
    }
    std::fs::remove_file(clean_path).unwrap();
    std::fs::remove_file(source_recovery_path).unwrap();
    std::fs::remove_file(target_recovery_path).unwrap();
}

#[test]
fn snap_001_ffi_geometry_point_resolution_is_bounded_view_targeted_and_non_mutating() {
    let (mut core, initial) = create_core(32, 24, 29);
    // SAFETY: The Core handle and every record/span/output remain complete,
    // aligned, live, and non-overlapping for each call.
    unsafe {
        (*core)
            .core
            .set_grid(GridConfig {
                origin_x: 0,
                origin_y: 0,
                spacing_x: 8,
                spacing_y: 8,
                subdivisions: 2,
            })
            .unwrap();
        (*core).core.add_guide(GuideAxis::Vertical, 5).unwrap();
        (*core)
            .core
            .apply_view(ViewCommand::SetSnapEnabled(true))
            .unwrap();
        (*core)
            .core
            .apply_view(ViewCommand::OneToOne {
                viewport_width: 32.0,
                viewport_height: 24.0,
            })
            .unwrap();
        (*core)
            .core
            .apply_view(ViewCommand::PanBy {
                device_dx: 3.0,
                device_dy: -2.0,
            })
            .unwrap();
        let view_revision = (*core).core.view_state().revision();
        let samples = [
            InkpodStrokeSample {
                struct_size: size_of::<InkpodStrokeSample>() as u32,
                flags: 0,
                x: 8.2,
                y: 5.8,
                pressure: 1.0,
                reserved: 0,
            },
            InkpodStrokeSample {
                struct_size: size_of::<InkpodStrokeSample>() as u32,
                flags: 0,
                x: 34.999,
                y: 21.999,
                pressure: 1.0,
                reserved: 0,
            },
        ];
        let input = InkpodGeometryPointResolveInput {
            struct_size: size_of::<InkpodGeometryPointResolveInput>() as u32,
            coordinate_space: INKPOD_COORDINATE_SPACE_DEVICE,
            feature_flags: INKPOD_GEOMETRY_RESOLVE_USE_VIEW_SNAP,
            view_id: 0,
            expected_view_revision: view_revision,
            samples: samples.as_ptr(),
            sample_count: samples.len() as u64,
            sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
        };
        let mut result = InkpodGeometryPointResolveResult {
            struct_size: size_of::<InkpodGeometryPointResolveResult>() as u32,
            reserved: 0,
            view_revision: 0,
            point_count: 0,
        };
        let mut points = [InkpodGeometryPoint {
            struct_size: size_of::<InkpodGeometryPoint>() as u32,
            reserved: 0,
            x: -1.0,
            y: -1.0,
        }; 2];
        let before = queried_document_info(core);
        let history_before = queried_history_info(core);
        assert_eq!(
            inkpod_core_geometry_points_resolve(
                core,
                &input,
                &mut result,
                points.as_mut_ptr(),
                points.len() as u64,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(result.view_revision, view_revision);
        assert_eq!(result.point_count, 2);
        assert_eq!((points[0].x, points[0].y), (5.0, 8.0));
        assert_eq!((points[1].x, points[1].y), (32.0, 24.0));
        let after = queried_document_info(core);
        let history_after = queried_history_info(core);
        assert_eq!(after.document_revision, before.document_revision);
        assert_eq!(after.flags, before.flags);
        assert_eq!(history_after.cursor, history_before.cursor);
        assert_eq!(history_after.item_count, history_before.item_count);

        let mut bypass = input;
        bypass.feature_flags = INKPOD_GEOMETRY_RESOLVE_BYPASS_SNAP;
        assert_eq!(
            inkpod_core_geometry_points_resolve(
                core,
                &bypass,
                &mut result,
                points.as_mut_ptr(),
                points.len() as u64,
            ),
            INKPOD_STATUS_OK
        );
        assert!((points[0].x - 5.2).abs() < 0.000_1);
        assert!((points[0].y - 7.8).abs() < 0.000_1);

        let mut short = input;
        short.struct_size -= 1;
        assert_eq!(
            inkpod_core_geometry_points_resolve(
                core,
                &short,
                &mut result,
                points.as_mut_ptr(),
                points.len() as u64,
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        let mut unknown = input;
        unknown.feature_flags = 1_u64 << 63;
        assert_eq!(
            inkpod_core_geometry_points_resolve(
                core,
                &unknown,
                &mut result,
                points.as_mut_ptr(),
                points.len() as u64,
            ),
            INKPOD_STATUS_UNSUPPORTED
        );
        let mut invalid_space = input;
        invalid_space.coordinate_space = u32::MAX;
        assert_eq!(
            inkpod_core_geometry_points_resolve(
                core,
                &invalid_space,
                &mut result,
                points.as_mut_ptr(),
                points.len() as u64,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut overflow = input;
        overflow.sample_count = inkpod_core::MAX_GEOMETRY_POINTS as u64 + 1;
        assert_eq!(
            inkpod_core_geometry_points_resolve(
                core,
                &overflow,
                &mut result,
                points.as_mut_ptr(),
                points.len() as u64,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut invalid_samples = samples;
        invalid_samples[0].x = f32::NAN;
        let mut invalid_value = input;
        invalid_value.samples = invalid_samples.as_ptr();
        assert_eq!(
            inkpod_core_geometry_points_resolve(
                core,
                &invalid_value,
                &mut result,
                points.as_mut_ptr(),
                points.len() as u64,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut stale = input;
        stale.expected_view_revision -= 1;
        assert_eq!(
            inkpod_core_geometry_points_resolve(
                core,
                &stale,
                &mut result,
                points.as_mut_ptr(),
                points.len() as u64,
            ),
            INKPOD_STATUS_INVALID_STATE
        );
        points[0].x = -1.0;
        points[1].x = -1.0;
        assert_eq!(
            inkpod_core_geometry_points_resolve(core, &input, &mut result, points.as_mut_ptr(), 1,),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        assert_eq!(result.point_count, 2);
        assert_eq!((points[0].x, points[1].x), (-1.0, -1.0));
        let mut short_output = points;
        short_output[0].struct_size -= 1;
        assert_eq!(
            inkpod_core_geometry_points_resolve(
                core,
                &input,
                &mut result,
                short_output.as_mut_ptr(),
                short_output.len() as u64,
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(
            inkpod_core_geometry_points_resolve(
                core,
                &input,
                &mut result,
                ptr::null_mut(),
                points.len() as u64,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_geometry_points_resolve(
                ptr::null_mut(),
                &input,
                &mut result,
                points.as_mut_ptr(),
                points.len() as u64,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        let secondary = (*core).core.create_view().unwrap();
        let secondary_revision = (*core)
            .core
            .apply_view_for(
                secondary,
                ViewCommand::Flip {
                    axis: MirrorAxis::Horizontal,
                },
            )
            .unwrap()
            .revision();
        let mut secondary_input = input;
        secondary_input.view_id = secondary;
        secondary_input.expected_view_revision = secondary_revision;
        (*core).core.close_view(secondary).unwrap();
        assert_eq!(
            inkpod_core_geometry_points_resolve(
                core,
                &secondary_input,
                &mut result,
                points.as_mut_ptr(),
                points.len() as u64,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(queried_document_info(core).document_revision > initial.document_revision);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn ffi_contract_geometry_preview_copies_bounded_points_and_rejects_invalid_records() {
    let (mut core, initial) = create_core(32, 32, 29);
    let mut result = dispatch();
    // SAFETY: Every record and borrowed span is complete and live for its call.
    unsafe {
        let plane_id = initial.color_plane_id;
        let base = initial.document_revision;
        let points = [
            InkpodGeometryPoint {
                struct_size: size_of::<InkpodGeometryPoint>() as u32,
                reserved: 0,
                x: 4.0,
                y: 5.0,
            },
            InkpodGeometryPoint {
                struct_size: size_of::<InkpodGeometryPoint>() as u32,
                reserved: 0,
                x: 20.0,
                y: 18.0,
            },
        ];
        let input = InkpodGeometryInput {
            struct_size: size_of::<InkpodGeometryInput>() as u32,
            primitive: INKPOD_GEOMETRY_RECTANGLE,
            feature_flags: INKPOD_GEOMETRY_OUTLINE
                | INKPOD_GEOMETRY_FILL
                | INKPOD_GEOMETRY_SQUARE_CROSS_SECTION,
            plane_id,
            base_revision: base,
            outline_color: color(1, 2, 3, 255),
            fill_color: color(80, 90, 100, 255),
            outline_width: 2.0,
            aspect_ratio_q16: 0,
            polygon_sides: 5,
            rotation_turns: 0,
            points: points.as_ptr(),
            point_count: points.len() as u64,
            point_stride_bytes: size_of::<InkpodGeometryPoint>() as u64,
        };
        let mut preview = InkpodGeometryPreviewInfo {
            struct_size: size_of::<InkpodGeometryPreviewInfo>() as u32,
            reserved: 0,
            plane_id: 0,
            base_revision: 0,
            preview_revision: 0,
        };

        let mut stale = input;
        stale.base_revision -= 1;
        assert_eq!(
            inkpod_core_geometry_preview_begin(core, &stale, &mut preview),
            INKPOD_STATUS_INVALID_STATE
        );
        let mut invalid_points = points;
        invalid_points[0].struct_size -= 4;
        let mut short_point = input;
        short_point.points = invalid_points.as_ptr();
        assert_eq!(
            inkpod_core_geometry_preview_begin(core, &short_point, &mut preview),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        let mut invalid = input;
        invalid.feature_flags |= 1_u64 << 63;
        assert_eq!(
            inkpod_core_geometry_preview_begin(core, &invalid, &mut preview),
            INKPOD_STATUS_UNSUPPORTED
        );
        invalid = input;
        invalid.point_stride_bytes -= 1;
        assert_eq!(
            inkpod_core_geometry_preview_begin(core, &invalid, &mut preview),
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        assert_eq!(
            inkpod_core_geometry_preview_begin(core, &input, &mut preview),
            INKPOD_STATUS_OK
        );
        assert_eq!(preview.base_revision, base);
        assert!(preview.preview_revision >= 1_u64 << 63);
        assert_eq!(queried_document_info(core).document_revision, base);
        invalid = input;
        invalid.primitive = INKPOD_GEOMETRY_ELLIPSE;
        assert_eq!(
            inkpod_core_geometry_preview_update(core, &invalid, &mut preview),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_geometry_preview_update(core, &input, &mut preview),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_geometry_preview_cancel(core), INKPOD_STATUS_OK);
        assert_eq!(queried_document_info(core).document_revision, base);

        assert_eq!(
            inkpod_core_geometry_preview_begin(core, &input, &mut preview),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_geometry_preview_commit(core, &mut result),
            INKPOD_STATUS_OK
        );
        let committed = queried_document_info(core);
        assert_eq!(committed.document_revision, base + 1);
        assert_ne!(committed.color_plane_checksum, initial.color_plane_checksum);
        assert_eq!(
            inkpod_core_geometry_preview_cancel(core),
            INKPOD_STATUS_INVALID_STATE
        );

        let mut one_shot = input;
        one_shot.base_revision = base + 1;
        assert_eq!(
            inkpod_core_geometry_apply(core, &one_shot, &mut result),
            INKPOD_STATUS_UNSUPPORTED
        );
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

fn names_followed_by_parenthesis(source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let bytes = source.as_bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        let Some(relative) = source[offset..].find("inkpod_") else {
            break;
        };
        let start = offset + relative;
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        let mut next = end;
        while next < bytes.len() && bytes[next].is_ascii_whitespace() {
            next += 1;
        }
        if next < bytes.len() && bytes[next] == b'(' {
            names.insert(source[start..end].to_owned());
        }
        offset = end.max(start + 1);
    }
    names
}

#[test]
fn replay_contract_and_snapshot_digest_are_bounded_side_effect_free_queries() {
    let (mut core, before) = create_core(8, 8, 0x4d37);
    let mut contract = InkpodReplayContract {
        struct_size: size_of::<InkpodReplayContract>() as u32,
        replay_epoch: 0,
        procedure_format_version: 0,
        canonical_numeric_version: 0,
        primitive_count: 0,
        reserved: u32::MAX,
        feature_flags: u64::MAX,
        primitive_catalog_digest: [0; 32],
    };
    let options = InkpodSnapshotOptions {
        struct_size: size_of::<InkpodSnapshotOptions>() as u32,
        reserved: 0,
        feature_flags: INKPOD_FEATURE_NONE,
    };
    let mut snapshot = ptr::null_mut();
    let mut digest = InkpodCanonicalDigest {
        struct_size: size_of::<InkpodCanonicalDigest>() as u32,
        algorithm: 0,
        bytes: [0; 32],
    };
    let mut render_plan = InkpodSnapshotRenderPlan {
        struct_size: size_of::<InkpodSnapshotRenderPlan>() as u32,
        abi_version: 0,
        feature_flags: u64::MAX,
        passes: ptr::null(),
        pass_count: u64::MAX,
        pass_stride_bytes: 0,
        adjustment_luts_rgb8: ptr::null(),
        adjustment_lut_count: u64::MAX,
        adjustment_lut_stride_bytes: 0,
    };
    // SAFETY: All handles and complete non-overlapping outputs are live for the calls.
    unsafe {
        assert_eq!(
            inkpod_core_get_replay_contract(core, &mut contract),
            INKPOD_STATUS_OK
        );
        assert_eq!(contract.replay_epoch, 25);
        assert_eq!(contract.procedure_format_version, 29);
        assert_eq!(contract.canonical_numeric_version, 1);
        assert!(contract.primitive_count > 0);
        assert_ne!(contract.primitive_catalog_digest, [0; 32]);
        assert_eq!(
            inkpod_core_build_snapshot(core, &options, &mut snapshot),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_snapshot_get_canonical_digest(snapshot, &mut digest),
            INKPOD_STATUS_OK
        );
        assert_eq!(digest.algorithm, INKPOD_DIGEST_BLAKE3_256);
        assert_ne!(digest.bytes, [0; 32]);
        let mut short_plan = render_plan;
        short_plan.struct_size -= 1;
        assert_eq!(
            inkpod_snapshot_get_render_plan(snapshot, &mut short_plan),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(
            inkpod_snapshot_get_render_plan(ptr::null(), &mut render_plan),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_snapshot_get_render_plan(snapshot, &mut render_plan),
            INKPOD_STATUS_OK
        );
        assert_eq!(render_plan.abi_version, INKPOD_ABI_VERSION);
        assert_eq!(render_plan.feature_flags, INKPOD_FEATURE_NONE);
        assert_eq!(
            render_plan.pass_stride_bytes,
            size_of::<InkpodSnapshotRenderPass>() as u64
        );
        assert_eq!(render_plan.adjustment_lut_stride_bytes, 3 * 256);
        assert_eq!(render_plan.pass_count, 0);
        assert!(render_plan.passes.is_null());
        assert_eq!(render_plan.adjustment_lut_count, 0);
        assert!(render_plan.adjustment_luts_rgb8.is_null());
        let after = queried_document_info(core);
        assert_eq!(after.document_revision, before.document_revision);
        assert_eq!(after.view_revision, before.view_revision);
        assert_eq!(after.main_plane_checksum, before.main_plane_checksum);
        assert_eq!(after.color_plane_checksum, before.color_plane_checksum);
        assert_eq!(inkpod_snapshot_release(&mut snapshot), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

fn exported_function_names(source: &str) -> BTreeSet<String> {
    source
        .lines()
        .filter_map(|line| {
            let function = line.find("fn inkpod_")?;
            let start = function + 3;
            let tail = &line[start..];
            let end = tail.find('(')?;
            Some(tail[..end].trim().to_owned())
        })
        .collect()
}

fn rust_sources(directory: &Path) -> Vec<String> {
    let mut sources = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in std::fs::read_dir(&current)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", current.display()))
        {
            let path = entry
                .expect("source directory entry must be readable")
                .path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                sources.push(
                    std::fs::read_to_string(&path).unwrap_or_else(|error| {
                        panic!("failed to read {}: {error}", path.display())
                    }),
                );
            }
        }
    }
    sources
}

#[test]
fn light_table_bulk_ffi_previews_commits_and_rejects_short_unknown_stale_inputs() {
    let (mut core, initial) = create_core(1, 1, 0x8123);
    let names = [
        b"cell1.png".as_slice(),
        b"cell2.png".as_slice(),
        b"cell3.png".as_slice(),
        b"cell4.png".as_slice(),
        b"cell5.png".as_slice(),
    ];
    let pixels = [
        [1_u8, 0, 0, 255],
        [2_u8, 0, 0, 255],
        [3_u8, 0, 0, 255],
        [4_u8, 0, 0, 255],
        [5_u8, 0, 0, 255],
    ];
    let uuids = [
        (0_u64, 0x8101_u64),
        (0, 0x8102),
        (initial.document_uuid_high, initial.document_uuid_low),
        (0, 0x8104),
        (0, 0x8105),
    ];
    let sources = std::array::from_fn::<_, 5, _>(|index| InkpodRasterSourceInput {
        struct_size: size_of::<InkpodRasterSourceInput>() as u32,
        pixel_format: INKPOD_STORAGE_RGBA8,
        flags: 0,
        document_uuid_high: uuids[index].0,
        document_uuid_low: uuids[index].1,
        source_revision: (index + 1) as u64,
        width: 1,
        height: 1,
        dpi_x_milli: 96_000,
        dpi_y_milli: 96_000,
        reference_frame: InkpodFrameRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        },
        pixels: pixels[index].as_ptr(),
        pixel_bytes: 4,
        row_stride_bytes: 4,
    });
    let cells = std::array::from_fn::<_, 5, _>(|index| InkpodSequenceCellInput {
        struct_size: size_of::<InkpodSequenceCellInput>() as u32,
        reserved: 0,
        name_utf8: names[index].as_ptr(),
        name_bytes: names[index].len() as u64,
        source: sources[index],
    });
    let sequence = InkpodSequenceInput {
        struct_size: size_of::<InkpodSequenceInput>() as u32,
        reserved: 0,
        feature_flags: 0,
        cells: cells.as_ptr(),
        cell_count: cells.len() as u64,
        cell_stride_bytes: size_of::<InkpodSequenceCellInput>() as u64,
    };

    // SAFETY: Every caller-owned record and nested span stays live and aligned.
    unsafe {
        assert_eq!(inkpod_core_sequence_set(core, &sequence), INKPOD_STATUS_OK);
        let mut set = InkpodLightTableSetInfo {
            struct_size: size_of::<InkpodLightTableSetInfo>() as u32,
            ..InkpodLightTableSetInfo::default()
        };
        assert_eq!(
            inkpod_core_light_table_set_get(core, 0, &mut set),
            INKPOD_STATUS_OK
        );

        let mut request = InkpodLightTableBulkRequest {
            struct_size: size_of::<u32>() as u32,
            ..InkpodLightTableBulkRequest::default()
        };
        assert_eq!(
            inkpod_core_light_table_bulk_request(
                core,
                set.id,
                INKPOD_LIGHT_TABLE_BULK_BOTH,
                2,
                800,
                200,
                &mut request,
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        request.struct_size = size_of::<InkpodLightTableBulkRequest>() as u32;
        assert_eq!(
            inkpod_core_light_table_bulk_request(core, set.id, u32::MAX, 2, 800, 200, &mut request,),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_light_table_bulk_request(
                core,
                set.id,
                INKPOD_LIGHT_TABLE_BULK_BOTH,
                2,
                800,
                200,
                &mut request,
            ),
            INKPOD_STATUS_OK
        );

        let mut preview = InkpodLightTableBulkPreviewInfo {
            struct_size: size_of::<InkpodLightTableBulkPreviewInfo>() as u32,
            ..InkpodLightTableBulkPreviewInfo::default()
        };
        assert_eq!(
            inkpod_core_light_table_bulk_preview(
                core,
                &request,
                ptr::null_mut(),
                0,
                0,
                &mut preview,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (preview.entry_count, preview.add_count, preview.skip_count),
            (4, 4, 0)
        );

        let mut entries = std::array::from_fn::<_, 4, _>(|_| InkpodLightTableBulkPreviewEntry {
            struct_size: size_of::<InkpodLightTableBulkPreviewEntry>() as u32,
            ..InkpodLightTableBulkPreviewEntry::default()
        });
        assert_eq!(
            inkpod_core_light_table_bulk_preview(
                core,
                &request,
                entries.as_mut_ptr(),
                3,
                size_of::<InkpodLightTableBulkPreviewEntry>() as u64,
                &mut preview,
            ),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        entries[2].struct_size -= 1;
        assert_eq!(
            inkpod_core_light_table_bulk_preview(
                core,
                &request,
                entries.as_mut_ptr(),
                entries.len() as u64,
                size_of::<InkpodLightTableBulkPreviewEntry>() as u64,
                &mut preview,
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        entries[2].struct_size = size_of::<InkpodLightTableBulkPreviewEntry>() as u32;
        assert_eq!(
            inkpod_core_light_table_bulk_preview(
                core,
                &request,
                entries.as_mut_ptr(),
                entries.len() as u64,
                size_of::<InkpodLightTableBulkPreviewEntry>() as u64,
                &mut preview,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            entries
                .iter()
                .map(|entry| (
                    entry.cell_number,
                    entry.distance,
                    entry.opacity_milli,
                    entry.action
                ))
                .collect::<Vec<_>>(),
            vec![
                (5, 2, 600, INKPOD_LIGHT_TABLE_BULK_ADD),
                (4, 1, 800, INKPOD_LIGHT_TABLE_BULK_ADD),
                (2, 1, 800, INKPOD_LIGHT_TABLE_BULK_ADD),
                (1, 2, 600, INKPOD_LIGHT_TABLE_BULK_ADD),
            ]
        );

        let before_apply = queried_document_info(core);
        let mut result = dispatch();
        let mut summary = InkpodLightTableBulkSummary {
            struct_size: size_of::<InkpodLightTableBulkSummary>() as u32,
            ..InkpodLightTableBulkSummary::default()
        };
        let mut short_ids = [0_u64; 3];
        assert_eq!(
            inkpod_core_light_table_bulk_register(
                core,
                &request,
                &mut result,
                &mut summary,
                short_ids.as_mut_ptr(),
                short_ids.len() as u64,
            ),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        assert_eq!(
            document_observation(&queried_document_info(core)),
            document_observation(&before_apply)
        );
        let mut ids = [0_u64; 4];
        assert_eq!(
            inkpod_core_light_table_bulk_register(
                core,
                &request,
                &mut result,
                &mut summary,
                ids.as_mut_ptr(),
                ids.len() as u64,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(result.revision, before_apply.document_revision + 1);
        assert_eq!(
            (summary.add_count, summary.skip_count, summary.item_id_count),
            (4, 0, 4)
        );
        assert!(ids.iter().all(|id| *id != 0));

        for (index, expected) in [(0, 0x8105_u64), (1, 0x8104), (2, 0x8102), (3, 0x8101)] {
            let mut item = InkpodLightTableItemInfo {
                struct_size: size_of::<InkpodLightTableItemInfo>() as u32,
                display_color: color(0, 0, 0, 0),
                ..InkpodLightTableItemInfo::default()
            };
            assert_eq!(
                inkpod_core_light_table_item_get(core, index, &mut item),
                INKPOD_STATUS_OK
            );
            assert_eq!(item.source_document_uuid_low, expected);
        }

        let mut duplicate_request = InkpodLightTableBulkRequest {
            struct_size: size_of::<InkpodLightTableBulkRequest>() as u32,
            ..InkpodLightTableBulkRequest::default()
        };
        assert_eq!(
            inkpod_core_light_table_bulk_request(
                core,
                set.id,
                INKPOD_LIGHT_TABLE_BULK_BOTH,
                2,
                800,
                200,
                &mut duplicate_request,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_light_table_bulk_preview(
                core,
                &duplicate_request,
                entries.as_mut_ptr(),
                entries.len() as u64,
                size_of::<InkpodLightTableBulkPreviewEntry>() as u64,
                &mut preview,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!((preview.add_count, preview.skip_count), (0, 4));
        assert!(
            entries
                .iter()
                .all(|entry| entry.action == INKPOD_LIGHT_TABLE_BULK_SKIP_EXISTING)
        );
        let before_noop = queried_document_info(core);
        assert_eq!(
            inkpod_core_light_table_bulk_register(
                core,
                &duplicate_request,
                &mut result,
                &mut summary,
                ptr::null_mut(),
                0,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            document_observation(&queried_document_info(core)),
            document_observation(&before_noop)
        );
        assert_eq!(
            (summary.add_count, summary.skip_count, summary.item_id_count),
            (0, 4, 0)
        );

        let stale_request = duplicate_request;
        assert_eq!(
            inkpod_core_light_table_set_global_opacity(core, 999, &mut result),
            INKPOD_STATUS_OK
        );
        let before_stale = queried_document_info(core);
        summary.add_count = u32::MAX;
        assert_eq!(
            inkpod_core_light_table_bulk_register(
                core,
                &stale_request,
                &mut result,
                &mut summary,
                ptr::null_mut(),
                0,
            ),
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(
            document_observation(&queried_document_info(core)),
            document_observation(&before_stale)
        );
        assert_eq!(summary.add_count, u32::MAX);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn history_visualization_abi_owns_commit_rows_and_supports_bounded_size_queries() {
    let (mut core, _) = create_core(32, 24, 0x4856_4953);
    unsafe {
        let mut result = dispatch();
        let main_line = color(12, 34, 56, 255);
        assert_eq!(
            inkpod_core_set_main_line_color(core, &main_line, &mut result),
            INKPOD_STATUS_OK
        );
        let document_before = queried_document_info(core);
        let history_before = queried_history_info(core);

        let mut visualization = ptr::null_mut();
        let mut cancelled_task = ptr::null_mut();
        assert_eq!(inkpod_task_create(&mut cancelled_task), INKPOD_STATUS_OK);
        assert_eq!(inkpod_task_cancel(cancelled_task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_history_visualization_create_with_task(
                core,
                cancelled_task,
                &mut visualization,
            ),
            INKPOD_STATUS_CANCELLED
        );
        assert!(visualization.is_null());
        assert_eq!(inkpod_task_release(&mut cancelled_task), INKPOD_STATUS_OK);

        let mut builder_task = ptr::null_mut();
        let mut builder = ptr::null_mut();
        assert_eq!(inkpod_task_create(&mut builder_task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_history_visualization_builder_begin(core, builder_task, &mut builder),
            INKPOD_STATUS_OK
        );
        assert!(!builder.is_null());
        let mut task_info = InkpodTaskInfo {
            struct_size: size_of::<InkpodTaskInfo>() as u32,
            state: 0,
            completed_work: 0,
            total_work: 0,
            reserved: 0,
        };
        assert_eq!(
            inkpod_task_query(builder_task, &mut task_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(task_info.state, INKPOD_TASK_RUNNING);
        assert_eq!((task_info.completed_work, task_info.total_work), (0, 1));

        let mut progress = InkpodHistoryVisualizationProgress {
            struct_size: size_of::<InkpodHistoryVisualizationProgress>() as u32,
            ..InkpodHistoryVisualizationProgress::default()
        };
        let full_progress_size = progress.struct_size;
        progress.struct_size -= 1;
        assert_eq!(
            inkpod_history_visualization_builder_step(
                builder,
                builder_task,
                1,
                &mut progress,
                &mut visualization,
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        progress.struct_size = full_progress_size;
        let mut wrong_task = ptr::null_mut();
        assert_eq!(inkpod_task_create(&mut wrong_task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_history_visualization_builder_step(
                builder,
                wrong_task,
                1,
                &mut progress,
                &mut visualization,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(inkpod_task_release(&mut wrong_task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_history_visualization_builder_step(
                builder,
                builder_task,
                1,
                &mut progress,
                &mut visualization,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(progress.done, 1);
        assert_eq!((progress.completed_events, progress.total_events), (1, 1));
        assert_eq!((progress.completed_rows, progress.total_rows), (1, 1));
        assert!(!visualization.is_null());
        assert_eq!(
            inkpod_history_visualization_builder_release(&mut builder, builder_task),
            INKPOD_STATUS_OK
        );
        assert!(builder.is_null());
        assert_eq!(
            inkpod_task_query(builder_task, &mut task_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(task_info.state, INKPOD_TASK_COMPLETED);
        assert_eq!(inkpod_task_release(&mut builder_task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_history_visualization_release(&mut visualization),
            INKPOD_STATUS_OK
        );

        assert_eq!(
            inkpod_core_history_visualization_create(core, &mut visualization),
            INKPOD_STATUS_OK
        );
        assert!(!visualization.is_null());
        assert_eq!(
            document_observation(&queried_document_info(core)),
            document_observation(&document_before)
        );
        assert_eq!(queried_history_info(core).cursor, history_before.cursor);

        let mut row_count = 0;
        assert_eq!(
            inkpod_history_visualization_row_count(visualization, &mut row_count),
            INKPOD_STATUS_OK
        );
        assert_eq!(row_count, 1);

        let mut row = InkpodHistoryVisualizationRowBuffer {
            struct_size: size_of::<InkpodHistoryVisualizationRowBuffer>() as u32,
            ..InkpodHistoryVisualizationRowBuffer::default()
        };
        assert_eq!(
            inkpod_history_visualization_row_get(visualization, 0, &mut row),
            INKPOD_STATUS_OK
        );
        assert_eq!(row.journal_event_id, 1);
        assert_eq!(row.procedure_id, 1);
        assert_eq!(row.committed_state_id, 2);
        assert_eq!(row.branch_id, 1);
        assert_eq!(row.primitive_id, INKPOD_PRIMITIVE_SET_MAIN_LINE_COLOR);
        assert_eq!((row.thumbnail_width, row.thumbnail_height), (32, 24));
        assert_eq!(row.thumbnail_stride_bytes, 32 * 4);
        assert_eq!(row.thumbnail_bytes, 32 * 24 * 4);
        assert!(row.primitive_name_bytes > 0);
        assert!(row.arguments_bytes > 0);

        let mut name = vec![0_u8; row.primitive_name_bytes as usize];
        let mut arguments = vec![0_u8; row.arguments_bytes as usize];
        let mut thumbnail = vec![0_u8; row.thumbnail_bytes as usize - 1];
        row.primitive_name_utf8 = name.as_mut_ptr();
        row.primitive_name_capacity = name.len() as u64;
        row.arguments_utf8 = arguments.as_mut_ptr();
        row.arguments_capacity = arguments.len() as u64;
        row.thumbnail_rgba8 = thumbnail.as_mut_ptr();
        row.thumbnail_capacity = thumbnail.len() as u64;
        assert_eq!(
            inkpod_history_visualization_row_get(visualization, 0, &mut row),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );

        thumbnail.resize(row.thumbnail_bytes as usize, 0);
        row.thumbnail_rgba8 = thumbnail.as_mut_ptr();
        row.thumbnail_capacity = thumbnail.len() as u64;
        assert_eq!(
            inkpod_history_visualization_row_get(visualization, 0, &mut row),
            INKPOD_STATUS_OK
        );
        assert_eq!(std::str::from_utf8(&name).unwrap(), "SetMainLineColor");
        assert_eq!(
            std::str::from_utf8(&arguments).unwrap(),
            "color=Rgba([12, 34, 56, 255])"
        );

        let full_size = row.struct_size;
        row.struct_size -= 1;
        assert_eq!(
            inkpod_history_visualization_row_get(visualization, 0, &mut row),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        row.struct_size = full_size;
        assert_eq!(
            inkpod_history_visualization_row_get(visualization, 1, &mut row),
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        assert_eq!(
            inkpod_history_visualization_release(&mut visualization),
            INKPOD_STATUS_OK
        );
        assert!(visualization.is_null());
        assert_eq!(
            inkpod_history_visualization_release(&mut visualization),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn external_subpalette_abi_owns_catalog_decode_view_sample_and_snapshot() {
    unsafe {
        let mut subpalette = ptr::null_mut();
        assert_eq!(inkpod_subpalette_create(&mut subpalette), INKPOD_STATUS_OK);
        assert!(!subpalette.is_null());

        let names = [
            b"cell10.png".as_slice(),
            b"palette.png".as_slice(),
            b"cell2.png".as_slice(),
        ];
        let mut sources = Vec::new();
        for (index, name) in names.iter().enumerate() {
            sources.push(InkpodSubpaletteSourceInput {
                struct_size: size_of::<InkpodSubpaletteSourceInput>() as u32,
                reserved: 0,
                source_token: index as u64 + 1,
                name_utf8: name.as_ptr(),
                name_bytes: name.len() as u64,
            });
        }
        let mut info = InkpodSubpaletteInfo {
            struct_size: size_of::<InkpodSubpaletteInfo>() as u32,
            ..InkpodSubpaletteInfo::default()
        };
        assert_eq!(
            inkpod_subpalette_replace_sources(
                subpalette,
                sources.as_ptr(),
                sources.len() as u64,
                size_of::<InkpodSubpaletteSourceInput>() as u64,
                &mut info,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(info.item_count, 3);
        assert_eq!(info.active_index, INKPOD_SUBPALETTE_INDEX_NONE);
        assert_eq!(info.flags, 0);

        let mut item = InkpodSubpaletteItemInfo {
            struct_size: size_of::<InkpodSubpaletteItemInfo>() as u32,
            ..InkpodSubpaletteItemInfo::default()
        };
        assert_eq!(
            inkpod_subpalette_item_get(subpalette, 0, &mut item),
            INKPOD_STATUS_OK
        );
        assert_eq!(item.cell_number, 2);
        assert_ne!(item.flags & INKPOD_SUBPALETTE_ITEM_HAS_CELL_NUMBER, 0);
        let first_item_id = item.item_id;

        let mut required = 0;
        assert_eq!(
            inkpod_subpalette_item_name_copy(subpalette, 0, ptr::null_mut(), 0, &mut required,),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        let mut name = vec![0_u8; required as usize];
        assert_eq!(
            inkpod_subpalette_item_name_copy(
                subpalette,
                0,
                name.as_mut_ptr(),
                name.len() as u64,
                &mut required,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(name, b"cell2.png");

        let mut adjacent = 0;
        assert_eq!(
            inkpod_subpalette_adjacent_item(subpalette, INKPOD_SEQUENCE_NEXT, &mut adjacent,),
            INKPOD_STATUS_OK
        );
        assert_eq!(adjacent, first_item_id);

        let (mut source_core, _) = create_core(2, 2, 0x5355_4250);
        let png = export_png(source_core);
        assert_eq!(inkpod_core_destroy(&mut source_core), INKPOD_STATUS_OK);
        let mut item_ids = Vec::new();
        for index in 0..info.item_count {
            let mut cached_item = InkpodSubpaletteItemInfo {
                struct_size: size_of::<InkpodSubpaletteItemInfo>() as u32,
                ..InkpodSubpaletteItemInfo::default()
            };
            assert_eq!(
                inkpod_subpalette_item_get(subpalette, index, &mut cached_item),
                INKPOD_STATUS_OK
            );
            item_ids.push(cached_item.item_id);
        }
        let mut cached_rasters = item_ids
            .iter()
            .map(|item_id| InkpodSubpaletteRasterInput {
                struct_size: size_of::<InkpodSubpaletteRasterInput>() as u32,
                format: INKPOD_COMMON_RASTER_PNG,
                item_id: *item_id,
                bytes: png.as_ptr(),
                byte_count: png.len() as u64,
            })
            .collect::<Vec<_>>();
        let unchanged_output = info;
        cached_rasters[0].struct_size -= 1;
        assert_eq!(
            inkpod_subpalette_load_cached_rasters(
                subpalette,
                cached_rasters.as_ptr(),
                cached_rasters.len() as u64,
                size_of::<InkpodSubpaletteRasterInput>() as u64,
                first_item_id,
                &mut info,
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(info.catalog_revision, unchanged_output.catalog_revision);
        assert_eq!(info.active_index, unchanged_output.active_index);
        cached_rasters[0].struct_size = size_of::<InkpodSubpaletteRasterInput>() as u32;
        let cache_status = inkpod_subpalette_load_cached_rasters(
            subpalette,
            cached_rasters.as_ptr(),
            cached_rasters.len() as u64,
            size_of::<InkpodSubpaletteRasterInput>() as u64,
            first_item_id,
            &mut info,
        );
        let mut diagnostic_size = 0_u64;
        let _ = inkpod_error_message_size(&mut diagnostic_size);
        let mut diagnostic = vec![0_u8; diagnostic_size as usize];
        let _ = inkpod_error_message_copy(
            diagnostic.as_mut_ptr(),
            diagnostic.len() as u64,
            &mut diagnostic_size,
        );
        assert_eq!(
            cache_status,
            INKPOD_STATUS_OK,
            "{}",
            String::from_utf8_lossy(&diagnostic)
        );
        assert_eq!(info.active_index, 0);
        assert_ne!(info.flags & INKPOD_SUBPALETTE_INFO_IMAGE_LOADED, 0);
        assert_ne!(info.flags & INKPOD_SUBPALETTE_INFO_CACHE_COMPLETE, 0);
        assert_eq!(
            inkpod_subpalette_select_cached_raster(subpalette, item_ids[1], &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(info.active_index, 1);
        assert_eq!(
            inkpod_subpalette_select_cached_raster(subpalette, first_item_id, &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(info.active_index, 0);

        let complete = info;
        cached_rasters[1].item_id = first_item_id;
        assert_eq!(
            inkpod_subpalette_load_cached_rasters(
                subpalette,
                cached_rasters.as_ptr(),
                cached_rasters.len() as u64,
                size_of::<InkpodSubpaletteRasterInput>() as u64,
                first_item_id,
                &mut info,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(info.active_index, complete.active_index);
        assert_eq!(info.flags, complete.flags);
        cached_rasters[1].item_id = item_ids[1];

        let view = InkpodViewInput {
            struct_size: size_of::<InkpodViewInput>() as u32,
            kind: INKPOD_VIEW_ONE_TO_ONE,
            flags: INKPOD_FEATURE_NONE,
            value1: 2.0,
            value2: 2.0,
            value3: 0.0,
            value4: 0.0,
        };
        assert_eq!(
            inkpod_subpalette_view_apply(subpalette, &view),
            INKPOD_STATUS_OK
        );
        let mut color = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            ..InkpodColorValue::default()
        };
        assert_eq!(
            inkpod_subpalette_sample(subpalette, 0.5, 0.5, &mut color),
            INKPOD_STATUS_OK
        );

        let options = InkpodSnapshotOptions {
            struct_size: size_of::<InkpodSnapshotOptions>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
        };
        let mut snapshot = ptr::null_mut();
        assert_eq!(
            inkpod_subpalette_build_snapshot(subpalette, &options, &mut snapshot),
            INKPOD_STATUS_OK
        );
        assert!(!snapshot.is_null());
        assert_eq!(inkpod_snapshot_release(&mut snapshot), INKPOD_STATUS_OK);

        let stable = info;
        assert_eq!(
            inkpod_subpalette_load_common_raster(
                subpalette,
                first_item_id,
                INKPOD_COMMON_RASTER_PNG,
                b"bad".as_ptr(),
                3,
                &mut info,
            ),
            INKPOD_STATUS_IO_ERROR
        );
        let mut observed = InkpodSubpaletteInfo {
            struct_size: size_of::<InkpodSubpaletteInfo>() as u32,
            ..InkpodSubpaletteInfo::default()
        };
        assert_eq!(
            inkpod_subpalette_get_info(subpalette, &mut observed),
            INKPOD_STATUS_OK
        );
        assert_eq!(observed.catalog_revision, stable.catalog_revision);
        assert_eq!(observed.active_index, stable.active_index);

        assert_eq!(
            inkpod_subpalette_clear(subpalette, &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(info.item_count, 0);
        assert_eq!(inkpod_subpalette_release(&mut subpalette), INKPOD_STATUS_OK);
        assert!(subpalette.is_null());
        assert_eq!(inkpod_subpalette_release(&mut subpalette), INKPOD_STATUS_OK);
    }
}

#[test]
fn ffi_contract_public_surface_matches_header_and_every_function_has_a_test_reference() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("FFI crate must be below the repository root");
    let read = |path: &Path| {
        std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
    };
    let header = read(&repository.join("include/inkpod/core_ffi.h"));
    let contract_tests = read(&repository.join("rust/inkpod-ffi/tests/unit/contracts.rs"));
    let ffi_tests = read(&repository.join("rust/inkpod-ffi/tests/unit/ffi.rs"));
    let v3_tests = read(&repository.join("rust/inkpod-ffi/tests/unit/v3.rs"));
    let batch_tests = read(&repository.join("rust/inkpod-ffi/tests/unit/batch.rs"));
    let file_io_tests = read(&repository.join("rust/inkpod-ffi/tests/unit/file_io.rs"));
    let cut_tests = read(&repository.join("rust/inkpod-ffi/tests/unit/cut.rs"));
    let inkscript_tests = read(&repository.join("rust/inkpod-ffi/tests/unit/inkscript.rs"));
    let cpp_tests = read(&repository.join("tests/abi_smoke.cpp"));

    let header_names = names_followed_by_parenthesis(&header);
    let implementation_names = rust_sources(&repository.join("rust/inkpod-ffi/src"))
        .iter()
        .flat_map(|source| exported_function_names(source))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        implementation_names, header_names,
        "public header declarations and no_mangle Rust exports drifted"
    );

    let mut referenced = names_followed_by_parenthesis(&ffi_tests);
    referenced.extend(names_followed_by_parenthesis(&v3_tests));
    referenced.extend(names_followed_by_parenthesis(&batch_tests));
    referenced.extend(names_followed_by_parenthesis(&file_io_tests));
    referenced.extend(names_followed_by_parenthesis(&cut_tests));
    referenced.extend(names_followed_by_parenthesis(&inkscript_tests));
    referenced.extend(names_followed_by_parenthesis(&contract_tests));
    referenced.extend(names_followed_by_parenthesis(&cpp_tests));
    let missing = header_names
        .difference(&referenced)
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "public FFI functions without a direct contract-test reference: {missing:?}"
    );
}

#[test]
fn sequence_catalog_and_snapshot_source_abi_preserve_immutable_provenance() {
    let (mut core, _) = create_core(1, 1, 0x9100);
    let names = [b"cell1.tga".as_slice(), b"cell2.tga".as_slice()];
    let pixels = [[1_u8, 2, 3, 255], [7_u8, 8, 9, 255]];
    let cells = std::array::from_fn::<_, 2, _>(|index| InkpodSequenceCellInput {
        struct_size: size_of::<InkpodSequenceCellInput>() as u32,
        reserved: 0,
        name_utf8: names[index].as_ptr(),
        name_bytes: names[index].len() as u64,
        source: InkpodRasterSourceInput {
            struct_size: size_of::<InkpodRasterSourceInput>() as u32,
            pixel_format: INKPOD_STORAGE_RGBA8,
            flags: 0,
            document_uuid_high: 0,
            document_uuid_low: 0x9101 + index as u64,
            source_revision: 3 + index as u64,
            width: 1,
            height: 1,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
            reference_frame: InkpodFrameRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            pixels: pixels[index].as_ptr(),
            pixel_bytes: 4,
            row_stride_bytes: 4,
        },
    });
    let sequence = InkpodSequenceInput {
        struct_size: size_of::<InkpodSequenceInput>() as u32,
        reserved: 0,
        feature_flags: 0,
        cells: cells.as_ptr(),
        cell_count: cells.len() as u64,
        cell_stride_bytes: size_of::<InkpodSequenceCellInput>() as u64,
    };
    let options = InkpodSnapshotOptions {
        struct_size: size_of::<InkpodSnapshotOptions>() as u32,
        reserved: 0,
        feature_flags: 0,
    };
    let mut catalog = InkpodSequenceCatalogInfo {
        struct_size: size_of::<InkpodSequenceCatalogInfo>() as u32,
        ..InkpodSequenceCatalogInfo::default()
    };
    let mut identity = InkpodSnapshotSourceIdentity {
        struct_size: size_of::<InkpodSnapshotSourceIdentity>() as u32,
        flags: u32::MAX,
        ..InkpodSnapshotSourceIdentity::default()
    };
    let mut info = document_info();
    let mut ordinary = ptr::null_mut();
    let mut first = ptr::null_mut();
    // SAFETY: All size-prefixed objects and nested spans are complete, live,
    // aligned and non-overlapping. Core operations stay on its owner thread.
    unsafe {
        assert_eq!(
            inkpod_core_sequence_catalog_get(ptr::null_mut(), &mut catalog),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut short = InkpodSequenceCatalogInfo {
            struct_size: 4,
            ..catalog
        };
        assert_eq!(
            inkpod_core_sequence_catalog_get(core, &mut short),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(
            inkpod_core_sequence_catalog_get(core, &mut catalog),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (
                catalog.sequence_revision,
                catalog.owner_generation,
                catalog.cell_count
            ),
            (0, 0, 0)
        );
        assert_eq!(catalog.active_index, INKPOD_SEQUENCE_INDEX_NONE);
        assert_eq!(
            inkpod_core_build_snapshot(core, &options, &mut ordinary),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_snapshot_get_source_identity(ordinary, &mut identity),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (
                identity.flags,
                identity.document_uuid_high,
                identity.document_uuid_low,
                identity.source_generation,
                identity.owner_generation
            ),
            (0, 0, 0, 0, 0)
        );
        assert_eq!(
            inkpod_snapshot_get_source_identity(ptr::null(), &mut identity),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_snapshot_get_source_identity(ordinary, ptr::null_mut()),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut short_identity = InkpodSnapshotSourceIdentity {
            struct_size: 4,
            ..identity
        };
        assert_eq!(
            inkpod_snapshot_get_source_identity(ordinary, &mut short_identity),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(inkpod_core_sequence_set(core, &sequence), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_sequence_activate(core, 0, &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_sequence_catalog_get(core, &mut catalog),
            INKPOD_STATUS_OK
        );
        assert_eq!((catalog.cell_count, catalog.active_index), (2, 0));
        assert_ne!(catalog.owner_generation, 0);
        assert_eq!(
            inkpod_core_build_snapshot(core, &options, &mut first),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_snapshot_get_source_identity(first, &mut identity),
            INKPOD_STATUS_OK
        );
        assert_eq!(identity.flags, INKPOD_SNAPSHOT_SOURCE_SEQUENCE_PRISTINE);
        assert_eq!(
            (
                identity.document_uuid_high,
                identity.document_uuid_low,
                identity.source_generation
            ),
            (0, 0x9101, 3)
        );
        assert_eq!(identity.owner_generation, catalog.owner_generation);
        assert_eq!(
            inkpod_core_sequence_activate(core, 1, &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
    // A renderer thread may still query the old snapshot after Core switches or
    // is destroyed; its provenance must never follow the active UI selection.
    let address = first as usize;
    let observed = std::thread::spawn(move || {
        let mut output = InkpodSnapshotSourceIdentity {
            struct_size: size_of::<InkpodSnapshotSourceIdentity>() as u32,
            ..InkpodSnapshotSourceIdentity::default()
        };
        // SAFETY: Main thread retains the live immutable snapshot until join.
        assert_eq!(
            unsafe {
                inkpod_snapshot_get_source_identity(address as *const InkpodSnapshot, &mut output)
            },
            INKPOD_STATUS_OK
        );
        (
            output.document_uuid_low,
            output.source_generation,
            output.owner_generation,
        )
    })
    .join()
    .unwrap();
    assert_eq!(observed, (0x9101, 3, catalog.owner_generation));
    // SAFETY: Both owner variables still contain their live uniquely released handles.
    unsafe {
        assert_eq!(inkpod_snapshot_release(&mut first), INKPOD_STATUS_OK);
        assert_eq!(inkpod_snapshot_release(&mut first), INKPOD_STATUS_OK);
        assert_eq!(inkpod_snapshot_release(&mut ordinary), INKPOD_STATUS_OK);
    }
}
