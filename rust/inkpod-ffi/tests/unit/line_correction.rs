use super::*;

fn png(complete: bool) -> Vec<u8> {
    let mut bytes = vec![0_u8; 16 * 16 * 4];
    for x in 2..14 {
        if x == 7 && !complete {
            continue;
        }
        bytes[(8 * 16 + x) * 4..(8 * 16 + x + 1) * 4].copy_from_slice(&[0, 0, 0, 255]);
    }
    let raster =
        inkpod_format::CommonRaster::new(16, 16, PixelFormat::StraightRgba8, None, None, bytes)
            .unwrap();
    inkpod_format::encode_common_raster(CommonRasterFormat::Png, &raster, false).unwrap()
}

#[test]
fn line_ffi_exact_pixels_preview_stale_invalid_cancel_and_ownership() {
    let config = InkpodCoreConfig {
        struct_size: size_of::<InkpodCoreConfig>() as u32,
        abi_version: INKPOD_ABI_VERSION,
        feature_flags: 0,
    };
    let mut core = ptr::null_mut();
    let mut reference = ptr::null_mut();
    let mut document = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..Default::default()
    };
    let mut expected = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..Default::default()
    };
    let before_bytes = png(false);
    let after_bytes = png(true);
    // SAFETY: All records and backing buffers are live, aligned, disjoint, and
    // used on this test's owner thread; owned handles are released below.
    unsafe {
        assert_eq!(inkpod_core_create(&config, &mut core), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_create(&config, &mut reference),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_import_common_raster(
                core,
                INKPOD_COMMON_RASTER_PNG,
                before_bytes.as_ptr(),
                before_bytes.len() as u64,
                7,
                91,
                &mut document
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_import_common_raster(
                reference,
                INKPOD_COMMON_RASTER_PNG,
                after_bytes.as_ptr(),
                after_bytes.len() as u64,
                7,
                92,
                &mut expected
            ),
            INKPOD_STATUS_OK
        );
        let mut before = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_get_document_info(core, &mut before),
            INKPOD_STATUS_OK
        );
        let input = InkpodLineCorrectionInput {
            struct_size: size_of::<InkpodLineCorrectionInput>() as u32,
            mode: INKPOD_LINE_CONNECT,
            plane_id: document.main_plane_id,
            expected_document_revision: document.document_revision,
            coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
            gap: 1,
            line_width: 1,
            brush_shape: INKPOD_TRACE_ROUND,
            view_zoom_q16: 65536,
            ..Default::default()
        };
        let mut output = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 17,
            revision: 123,
            accepted_command_count: 456,
        };
        for variant in 0..10 {
            let mut bad = input;
            match variant {
                0 => bad.struct_size -= 1,
                1 => bad.mode = 999,
                2 => bad.feature_flags = 1,
                3 => bad.expected_document_revision -= 1,
                4 => bad.plane_id = u64::MAX,
                5 => bad.brush_shape = 999,
                6 => bad.gap = 65,
                7 => {
                    bad.use_region = 1;
                    bad.shape = INKPOD_SELECTION_TRACE;
                    bad.sample_count = u64::MAX;
                }
                8 => bad.background_mode = 999,
                _ => {}
            }
            let mut task = ptr::null_mut();
            assert_eq!(inkpod_task_create(&mut task), INKPOD_STATUS_OK);
            if variant == 9 {
                assert_eq!(inkpod_task_cancel(task), INKPOD_STATUS_OK);
            }
            let status = inkpod_core_line_correct(core, &bad, task, &mut output);
            let wanted = match variant {
                0 => INKPOD_STATUS_INCOMPATIBLE_ABI,
                2 => INKPOD_STATUS_UNSUPPORTED,
                3 => INKPOD_STATUS_INVALID_STATE,
                9 => INKPOD_STATUS_CANCELLED,
                _ => INKPOD_STATUS_INVALID_ARGUMENT,
            };
            assert_eq!(status, wanted, "case {variant}");
            assert_eq!(
                (
                    output.reserved,
                    output.revision,
                    output.accepted_command_count
                ),
                (17, 123, 456)
            );
            assert_eq!(
                inkpod_core_get_document_info(core, &mut document),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                (
                    document.document_revision,
                    document.main_plane_checksum,
                    document.color_plane_checksum
                ),
                (
                    before.document_revision,
                    before.main_plane_checksum,
                    before.color_plane_checksum
                )
            );
            assert_eq!(inkpod_task_release(&mut task), INKPOD_STATUS_OK);
            assert_eq!(inkpod_task_release(&mut task), INKPOD_STATUS_OK);
        }
        let mut task = ptr::null_mut();
        assert_eq!(inkpod_task_create(&mut task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_line_correct(core, ptr::null(), task, &mut output),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(inkpod_task_release(&mut task), INKPOD_STATUS_OK);
        let mut preview = InkpodFilterPreviewInfo {
            struct_size: size_of::<InkpodFilterPreviewInfo>() as u32,
            reserved: 0,
            plane_id: 0,
            base_checksum: 0,
            preview_checksum: 0,
            preview_revision: 0,
        };
        for commit in [false, true] {
            assert_eq!(inkpod_task_create(&mut task), INKPOD_STATUS_OK);
            assert_eq!(
                inkpod_core_line_preview_begin(core, &input, task, &mut preview),
                INKPOD_STATUS_OK
            );
            assert_eq!(preview.base_checksum, before.main_plane_checksum);
            assert_eq!(preview.preview_checksum, expected.main_plane_checksum);
            assert_eq!(
                inkpod_core_get_document_info(core, &mut document),
                INKPOD_STATUS_OK
            );
            assert_eq!(document.document_revision, before.document_revision);
            if commit {
                assert_eq!(
                    inkpod_core_filter_preview_apply(core, &mut output),
                    INKPOD_STATUS_OK
                );
            } else {
                assert_eq!(
                    inkpod_core_filter_preview_cancel(core, &mut preview),
                    INKPOD_STATUS_OK
                );
            }
            assert_eq!(inkpod_task_release(&mut task), INKPOD_STATUS_OK);
        }
        assert_eq!(
            inkpod_core_get_document_info(core, &mut document),
            INKPOD_STATUS_OK
        );
        assert_eq!(document.document_revision, before.document_revision + 1);
        assert_eq!(document.main_plane_checksum, expected.main_plane_checksum);
        assert_eq!(document.color_plane_checksum, before.color_plane_checksum);
        assert_eq!(inkpod_core_destroy(&mut reference), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}
