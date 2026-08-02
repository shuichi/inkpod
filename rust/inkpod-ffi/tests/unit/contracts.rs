use super::batch::{inkpod_batch_task_create, inkpod_batch_task_query, inkpod_batch_task_release};
use super::*;
use std::collections::BTreeSet;
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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

fn export_png(core: *mut InkpodCore) -> Vec<u8> {
    let mut buffer = ptr::null_mut();
    let mut bytes = ptr::null();
    let mut byte_count = 0;
    // SAFETY: The core is live; the returned buffer is viewed before its unique release.
    unsafe {
        assert_eq!(
            inkpod_core_export_common_raster(core, INKPOD_COMMON_RASTER_PNG, 0, &mut buffer),
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
            name_utf8: ptr::null_mut(),
            name_capacity: 0,
            name_bytes: 0,
        };
        assert_eq!(
            inkpod_core_history_item(core, 0, &mut history_item),
            INKPOD_STATUS_OK
        );
        assert!(history_item.name_bytes > 0);
        let mut history_name = vec![0_u8; history_item.name_bytes as usize];
        history_item.name_utf8 = history_name.as_mut_ptr();
        history_item.name_capacity = history_name.len() as u64;
        assert_eq!(
            inkpod_core_history_item(core, 0, &mut history_item),
            INKPOD_STATUS_OK
        );
        assert!(std::str::from_utf8(&history_name).is_ok());

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

    let source_pixels = [
        255_u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
    ];
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
        assert_eq!(raster.required_bytes, source_pixels.len() as u64);
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
        assert_eq!(rendered, source_pixels);

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

        let png = export_png(core);
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
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
    std::fs::remove_file(save_path).unwrap();
}

#[test]
fn ffi_contract_light_table_sequence_and_owned_buffers() {
    let (mut source_core, _) = create_core(3, 2, 2);
    let png = export_png(source_core);
    // SAFETY: The source core is live, uniquely owned, and destroyed on its owner thread.
    unsafe {
        assert_eq!(inkpod_core_destroy(&mut source_core), INKPOD_STATUS_OK);
    }

    let (mut light_core, _) = create_core(3, 2, 3);
    let item_name = b"encoded reference";
    let mut result = dispatch();
    let mut item_id = 0;
    // SAFETY: Encoded bytes/name and all output records remain live for each call.
    unsafe {
        assert_eq!(
            inkpod_core_light_table_add_common_raster(
                light_core,
                INKPOD_COMMON_RASTER_PNG,
                png.as_ptr(),
                png.len() as u64,
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
        assert_eq!(copied_item_name, item_name);

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
        assert_eq!(
            inkpod_core_light_table_reload_common_raster(
                light_core,
                item_id,
                INKPOD_COMMON_RASTER_PNG,
                png.as_ptr(),
                png.len() as u64,
                7,
                8,
                42,
                &mut result,
            ),
            INKPOD_STATUS_OK
        );
        item_info.name_utf8 = ptr::null_mut();
        item_info.name_capacity = 0;
        assert_eq!(
            inkpod_core_light_table_item_get(light_core, 0, &mut item_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(item_info.source_revision, 42);
        assert_eq!(item_info.opacity_milli, 600);
        assert_eq!(inkpod_core_destroy(&mut light_core), INKPOD_STATUS_OK);
    }

    let (mut sequence_core, _) = create_core(1, 1, 4);
    let clean_path = temporary_inkpod_path("sequence");
    save_document(sequence_core, &clean_path);
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

        let mut active = document_info();
        assert_eq!(
            inkpod_core_sequence_activate(sequence_core, 0, &mut active),
            INKPOD_STATUS_OK
        );
        assert_eq!(active.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);

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
}

fn vector_segment(start: (f32, f32), end: (f32, f32)) -> InkpodVectorCubicSegment {
    let delta_x = (end.0 - start.0) / 3.0;
    let delta_y = (end.1 - start.1) / 3.0;
    InkpodVectorCubicSegment {
        struct_size: size_of::<InkpodVectorCubicSegment>() as u32,
        reserved: 0,
        p0: InkpodVectorPoint {
            x: start.0,
            y: start.1,
        },
        p1: InkpodVectorPoint {
            x: start.0 + delta_x,
            y: start.1 + delta_y,
        },
        p2: InkpodVectorPoint {
            x: start.0 + delta_x * 2.0,
            y: start.1 + delta_y * 2.0,
        },
        p3: InkpodVectorPoint { x: end.0, y: end.1 },
        width_start: 1.0,
        width_end: 1.0,
    }
}

#[test]
fn ffi_contract_vector_filter_and_task_state_machines() {
    let (mut core, info) = create_core(8, 8, 5);
    let vector_name = b"Contract vector";
    let create_vector = InkpodTreeEdit {
        struct_size: size_of::<InkpodTreeEdit>() as u32,
        operation: INKPOD_TREE_CREATE_LAYER,
        flags: 0,
        object_id: 0,
        parent_id: 0,
        destination_index: 0,
        kind: INKPOD_LAYER_VECTOR_COLORING,
        pixel_format: 0,
        opacity_milli: 0,
        name_utf8: vector_name.as_ptr(),
        name_bytes: vector_name.len() as u64,
    };
    let mut result = dispatch();
    let mut vector_layer_id = 0;

    // SAFETY: All borrowed spans and complete records remain live for their calls.
    unsafe {
        assert_eq!(
            inkpod_core_tree_edit(core, &create_vector, &mut result, &mut vector_layer_id),
            INKPOD_STATUS_OK
        );
        let mut vector_layer_index = None;
        let mut node = InkpodNodeInfo {
            struct_size: size_of::<InkpodNodeInfo>() as u32,
            ..InkpodNodeInfo::default()
        };
        for index in 0..2 {
            if inkpod_core_node_get(core, index, u32::MAX, &mut node) == INKPOD_STATUS_OK
                && node.id == vector_layer_id
            {
                vector_layer_index = Some(index);
                break;
            }
        }
        let vector_layer_index = vector_layer_index.expect("new vector layer must be queryable");
        assert_eq!(
            inkpod_core_node_get(core, vector_layer_index, 0, &mut node),
            INKPOD_STATUS_OK
        );
        let vector_plane_id = node.id;

        let left_segment = vector_segment((0.0, 1.0), (1.0, 1.0));
        let right_segment = vector_segment((2.0, 1.0), (3.0, 1.0));
        let path = |segment: &InkpodVectorCubicSegment| InkpodVectorPathInput {
            struct_size: size_of::<InkpodVectorPathInput>() as u32,
            reserved: 0,
            flags: 0,
            plane_id: vector_plane_id,
            color: color(0, 0, 0, 255),
            segments: segment,
            segment_count: 1,
            segment_stride_bytes: size_of::<InkpodVectorCubicSegment>() as u64,
        };
        let mut left_id = 0;
        let mut right_id = 0;
        assert_eq!(
            inkpod_core_vector_add_path(core, &path(&left_segment), &mut result, &mut left_id),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_vector_add_path(core, &path(&right_segment), &mut result, &mut right_id),
            INKPOD_STATUS_OK
        );
        let mut connector_id = 0;
        assert_eq!(
            inkpod_core_vector_connect(core, vector_plane_id, 1.5, &mut result, &mut connector_id,),
            INKPOD_STATUS_OK
        );
        assert_ne!(connector_id, 0);
        let path_ids = [left_id, right_id, connector_id];
        let width = InkpodVectorWidthInput {
            struct_size: size_of::<InkpodVectorWidthInput>() as u32,
            mode: INKPOD_VECTOR_WIDTH_ADD,
            feature_flags: 0,
            path_ids: path_ids.as_ptr(),
            path_count: path_ids.len() as u64,
            parameter: 1.0,
            reserved: 0,
        };
        assert_eq!(
            inkpod_core_vector_correct_width(core, &width, &mut result),
            INKPOD_STATUS_OK
        );
        let mut thumbnail = InkpodLayerThumbnailBuffer {
            struct_size: size_of::<InkpodLayerThumbnailBuffer>() as u32,
            layer_id: vector_layer_id,
            maximum_width: 4,
            maximum_height: 4,
            ..InkpodLayerThumbnailBuffer::default()
        };
        assert_eq!(
            inkpod_core_layer_thumbnail(core, &mut thumbnail),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (thumbnail.width, thumbnail.height, thumbnail.stride_bytes),
            (4, 4, 16)
        );
        assert_eq!(thumbnail.required_bytes, 64);
        let mut short_thumbnail_pixels = vec![0_u8; thumbnail.required_bytes as usize - 1];
        thumbnail.pixels_rgba8 = short_thumbnail_pixels.as_mut_ptr();
        thumbnail.pixel_capacity = short_thumbnail_pixels.len() as u64;
        assert_eq!(
            inkpod_core_layer_thumbnail(core, &mut thumbnail),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        let mut thumbnail_pixels = vec![0_u8; thumbnail.required_bytes as usize];
        thumbnail.pixels_rgba8 = thumbnail_pixels.as_mut_ptr();
        thumbnail.pixel_capacity = thumbnail_pixels.len() as u64;
        assert_eq!(
            inkpod_core_layer_thumbnail(core, &mut thumbnail),
            INKPOD_STATUS_OK
        );
        assert!(thumbnail_pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
        thumbnail.maximum_width = 0;
        assert_eq!(
            inkpod_core_layer_thumbnail(core, &mut thumbnail),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let erase = InkpodVectorEraseInput {
            struct_size: size_of::<InkpodVectorEraseInput>() as u32,
            mode: INKPOD_VECTOR_ERASE_WHOLE_PATH,
            plane_id: vector_plane_id,
            x: 0.5,
            y: 1.0,
            radius: 0.25,
            reserved: 0,
        };
        assert_eq!(
            inkpod_core_vector_erase(core, &erase, &mut result),
            INKPOD_STATUS_OK
        );

        assert_eq!(
            inkpod_core_set_active_node(core, info.layer_id, info.color_plane_id),
            INKPOD_STATUS_OK
        );
        let mut filter = InkpodFilterInput {
            struct_size: size_of::<InkpodFilterInput>() as u32,
            kind: INKPOD_FILTER_INVERT,
            feature_flags: 0,
            plane_id: info.color_plane_id,
            channel: INKPOD_FILTER_CHANNEL_RGB,
            interpolation: INKPOD_CURVE_BEZIER,
            parameter_0: 0,
            parameter_1: 0,
            parameter_2: 0,
            parameter_3: 0,
            parameter_4: 0,
            point_stride_bytes: 0,
            points: ptr::null(),
            point_count: 0,
        };
        let mut preview = InkpodFilterPreviewInfo {
            struct_size: size_of::<InkpodFilterPreviewInfo>() as u32,
            reserved: 0,
            plane_id: 0,
            base_checksum: 0,
            preview_checksum: 0,
            preview_revision: 0,
        };
        assert_eq!(
            inkpod_core_filter_preview_begin(core, &filter, &mut preview),
            INKPOD_STATUS_OK
        );
        filter.kind = INKPOD_FILTER_BLUR_WEAK;
        assert_eq!(
            inkpod_core_filter_preview_update(core, &filter, &mut preview),
            INKPOD_STATUS_OK
        );
        let mut task = ptr::null_mut();
        assert_eq!(inkpod_task_create(&mut task), INKPOD_STATUS_OK);
        filter.kind = INKPOD_FILTER_SHARPEN_WEAK;
        assert_eq!(
            inkpod_core_filter_preview_update_task(core, &filter, task, &mut preview),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_filter_preview_apply(core, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_task_release(&mut task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_filter_apply_last(core, info.color_plane_id, &mut result),
            INKPOD_STATUS_OK
        );

        let mut batch_task = ptr::null_mut();
        assert_eq!(inkpod_batch_task_create(&mut batch_task), INKPOD_STATUS_OK);
        let mut task_info = InkpodTaskInfo {
            struct_size: size_of::<InkpodTaskInfo>() as u32,
            state: u32::MAX,
            completed_work: u64::MAX,
            total_work: u64::MAX,
            reserved: u64::MAX,
        };
        assert_eq!(
            inkpod_batch_task_query(batch_task, &mut task_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(task_info.state, INKPOD_TASK_READY);
        assert_eq!(task_info.reserved, 0);
        assert_eq!(inkpod_batch_task_release(&mut batch_task), INKPOD_STATUS_OK);
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
    let batch_tests = read(&repository.join("rust/inkpod-ffi/tests/unit/batch.rs"));
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
    referenced.extend(names_followed_by_parenthesis(&batch_tests));
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
