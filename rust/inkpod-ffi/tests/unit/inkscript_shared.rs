use super::*;

struct Fixture {
    core: *mut InkpodCore,
    input: *mut InkpodCore,
    source: *mut InkpodInkScriptSource,
    program: *mut InkpodInkScriptProgram,
    io: *mut InkpodInkScriptIo,
    plan_task: *mut InkpodInkScriptPlanTask,
    run: *mut InkpodInkScriptRunTask,
}

impl Fixture {
    fn new(mode: u32, active: bool, capacity: u64, maximum_bytes: u64) -> Self {
        Self::with_options(mode, active, capacity, maximum_bytes, false, false)
    }
    fn with_options(
        mode: u32,
        active: bool,
        capacity: u64,
        maximum_bytes: u64,
        seeded: bool,
        sequence: bool,
    ) -> Self {
        // SAFETY: This fixture owns every handle/record and uses the public C boundary only.
        unsafe {
            let core = new_core();
            let input = new_core();
            let options = InkpodCellCreateOptions {
                struct_size: size_of::<InkpodCellCreateOptions>() as u32,
                reserved: 0,
                feature_flags: 0,
                document_uuid_high: 0,
                document_uuid_low: 0x5501,
                width: 8,
                height: 8,
                dpi_x_milli: 72_000,
                dpi_y_milli: 72_000,
            };
            let mut document = InkpodDocumentInfo {
                struct_size: size_of::<InkpodDocumentInfo>() as u32,
                ..Default::default()
            };
            assert_eq!(
                inkpod_core_new_cell(input, &options, &mut document),
                INKPOD_STATUS_OK
            );
            if seeded {
                let fill = InkpodFillInput {
                    struct_size: size_of::<InkpodFillInput>() as u32,
                    operation: INKPOD_FILL_SEED,
                    flags: 0,
                    seed_x: 0,
                    seed_y: 0,
                    color: InkpodColorValue {
                        struct_size: size_of::<InkpodColorValue>() as u32,
                        depth: INKPOD_COLOR_DEPTH_8,
                        red: 255,
                        green: 0,
                        blue: 0,
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
                let mut result = InkpodFillResult {
                    struct_size: size_of::<InkpodFillResult>() as u32,
                    ..Default::default()
                };
                assert_eq!(
                    inkpod_core_apply_fill_for_editor_target(
                        input,
                        document.layer_id,
                        document.color_plane_id,
                        &fill,
                        &mut result
                    ),
                    INKPOD_STATUS_OK
                );
                assert_eq!(result.changed_pixel_count, 64);
            }
            let policy = if active {
                "active_document"
            } else {
                "new_tabs"
            };
            let inputs = if sequence {
                "profile = canonical; current_sequence;"
            } else {
                "profile = batch; current_document;"
            };
            let text = format!(
                r#"inkscript 3;
requires {{ procedure_catalog = 8; replay_epoch = 29; }}
inputs {{ {inputs} }}
program {{ step "erase" {{ enabled = true; invoke apply_batch_operations {{ operations = [
{{ kind = erase; enabled = true; target = {{ kind = role; plane_kind = color; missing = error; }}; colors = [rgba8(255,0,0,255)]; }}
]; }}; }} }}
output {{ policy = {policy}; }}
execution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}"#
            );
            let mut source = ptr::null_mut();
            assert_eq!(
                inkpod_inkscript_source_parse(&source_input(text.as_bytes()), &mut source),
                INKPOD_STATUS_OK
            );
            let mut program = ptr::null_mut();
            assert_eq!(
                inkpod_core_inkscript_compile(core, source, &compile_request(), &mut program),
                INKPOD_STATUS_OK
            );
            let mut io = ptr::null_mut();
            let request = InkpodInkScriptIoRequest {
                struct_size: size_of::<InkpodInkScriptIoRequest>() as u32,
                version: INKPOD_INKSCRIPT_RECORD_VERSION,
                new_tab_capacity: capacity,
                ..Default::default()
            };
            assert_eq!(
                inkpod_core_inkscript_io_create(core, ptr::null_mut(), &request, &mut io),
                INKPOD_STATUS_OK
            );
            let session = InkpodInkScriptIoSession {
                struct_size: size_of::<InkpodInkScriptIoSession>() as u32,
                version: INKPOD_INKSCRIPT_RECORD_VERSION,
                session_id: 51,
                session_generation: 2,
                source_generation: 3,
                session_core: input,
                label: InkpodInkScriptUtf8Span {
                    bytes: b"A001.inkpod".as_ptr(),
                    byte_count: 11,
                },
                display_number: 1,
                ..Default::default()
            };
            assert_eq!(
                inkpod_core_inkscript_io_capture_session(core, io, &session),
                INKPOD_STATUS_OK
            );
            if sequence {
                let member = InkpodInkScriptIoSequenceMember {
                    struct_size: size_of::<InkpodInkScriptIoSequenceMember>() as u32,
                    version: INKPOD_INKSCRIPT_RECORD_VERSION,
                    kind: INKPOD_INKSCRIPT_SESSION_MEMBER,
                    session_id: 51,
                    ..Default::default()
                };
                let request = InkpodInkScriptIoSequenceRequest {
                    struct_size: size_of::<InkpodInkScriptIoSequenceRequest>() as u32,
                    version: INKPOD_INKSCRIPT_RECORD_VERSION,
                    sequence_id: 8,
                    generation: 3,
                    members: &member,
                    member_count: 1,
                    member_stride_bytes: size_of::<InkpodInkScriptIoSequenceMember>() as u64,
                    ..Default::default()
                };
                assert_eq!(
                    inkpod_core_inkscript_io_capture_sequence(core, io, &request),
                    INKPOD_STATUS_OK
                );
            }
            let plan_request = InkpodInkScriptSharedPlanRequest {
                struct_size: size_of::<InkpodInkScriptSharedPlanRequest>() as u32,
                version: INKPOD_INKSCRIPT_RECORD_VERSION,
                controller_id: 41,
                session_generation: 7,
                current_session_id: 51,
                ..Default::default()
            };
            let mut plan_task = ptr::null_mut();
            assert_eq!(
                inkpod_core_inkscript_shared_plan_task_create(
                    core,
                    program,
                    io,
                    &plan_request,
                    &mut plan_task
                ),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_inkscript_plan_task_advance(core, plan_task),
                INKPOD_STATUS_OK
            );
            let mut plan = ptr::null_mut();
            assert_eq!(
                inkpod_core_inkscript_plan_task_take_plan(core, plan_task, &mut plan),
                INKPOD_STATUS_OK
            );
            let confirmation_request = InkpodInkScriptConfirmationRequest {
                struct_size: size_of::<InkpodInkScriptConfirmationRequest>() as u32,
                version: INKPOD_INKSCRIPT_RECORD_VERSION,
                scope: INKPOD_INKSCRIPT_SCOPE_ALL,
                reserved: 0,
                feature_flags: 0,
                document_uuid_low: 0,
                document_uuid_high: 0,
                file_alias: [0; 32],
            };
            let mut confirmation = ptr::null_mut();
            assert_eq!(
                inkpod_core_inkscript_confirmation_create(
                    core,
                    plan,
                    &confirmation_request,
                    &mut confirmation
                ),
                INKPOD_STATUS_OK
            );
            let request = InkpodInkScriptRunRequest {
                struct_size: size_of::<InkpodInkScriptRunRequest>() as u32,
                version: INKPOD_INKSCRIPT_RECORD_VERSION,
                mode,
                reserved: 0,
                feature_flags: 0,
                controller_id: 41,
                session_generation: 7,
                maximum_output_bytes: maximum_bytes,
                host: InkpodInkScriptHostAdapter {
                    struct_size: 0,
                    version: 0,
                    feature_flags: 0,
                    context: ptr::null_mut(),
                    call: None,
                },
            };
            let mut run = ptr::null_mut();
            assert_eq!(
                inkpod_core_inkscript_run_task_create(
                    core,
                    program,
                    &mut plan,
                    &mut confirmation,
                    &request,
                    &mut run
                ),
                INKPOD_STATUS_OK
            );
            assert!(plan.is_null() && confirmation.is_null());
            Self {
                core,
                input,
                source,
                program,
                io,
                plan_task,
                run,
            }
        }
    }
    fn finish(&self) -> u32 {
        // SAFETY: Fixture owns the task and initialized DTO on the creating thread.
        unsafe {
            loop {
                let status = inkpod_core_inkscript_run_task_advance(self.core, self.run);
                let mut event = InkpodInkScriptTaskEvent {
                    struct_size: size_of::<InkpodInkScriptTaskEvent>() as u32,
                    version: INKPOD_INKSCRIPT_RECORD_VERSION,
                    ..Default::default()
                };
                assert_eq!(
                    inkpod_core_inkscript_run_task_advance(self.core, self.run),
                    INKPOD_STATUS_QUEUE_FULL
                );
                assert_eq!(
                    inkpod_core_inkscript_run_task_event_take(self.core, self.run, &mut event),
                    INKPOD_STATUS_OK
                );
                if event.kind == INKPOD_INKSCRIPT_EVENT_RUN_COMPLETE {
                    return status;
                }
                assert_eq!(status, INKPOD_STATUS_OK);
            }
        }
    }
    fn take(
        &self,
    ) -> (
        *mut InkpodInkScriptStagedResult,
        InkpodInkScriptStagedInfo,
        u32,
    ) {
        let mut result = ptr::null_mut();
        let mut info = InkpodInkScriptStagedInfo {
            struct_size: size_of::<InkpodInkScriptStagedInfo>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            ..Default::default()
        };
        // SAFETY: Live task and initialized distinct output owners stay valid for the call.
        let status = unsafe {
            inkpod_core_inkscript_run_task_take_staged_result(
                self.core,
                self.run,
                0,
                &mut result,
                &mut info,
            )
        };
        (result, info, status)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // SAFETY: Owned child handles are released on their owner thread before either Core.
        unsafe {
            assert_eq!(
                inkpod_core_inkscript_run_task_release(self.core, &mut self.run),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_inkscript_plan_task_release(self.core, &mut self.plan_task),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_inkscript_program_release(self.core, &mut self.program),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_inkscript_io_release(self.core, &mut self.io),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_inkscript_source_release(&mut self.source),
                INKPOD_STATUS_OK
            );
            assert_eq!(inkpod_core_destroy(&mut self.input), INKPOD_STATUS_OK);
            assert_eq!(inkpod_core_destroy(&mut self.core), INKPOD_STATUS_OK);
        }
    }
}

#[test]
fn shared_new_tab_and_preview_transfer_once_with_distinct_identity_and_origin() {
    for mode in [
        INKPOD_INKSCRIPT_RUN_INSTALL,
        INKPOD_INKSCRIPT_RUN_IMAGE_PREVIEW,
    ] {
        let fixture = Fixture::new(mode, false, 1, 0);
        assert_eq!(fixture.finish(), INKPOD_STATUS_OK);
        // SAFETY: Fixture and output DTOs remain live, with unique result/Core owners.
        unsafe {
            let mut count = 0;
            assert_eq!(
                inkpod_core_inkscript_run_task_result_count(fixture.core, fixture.run, &mut count),
                INKPOD_STATUS_OK
            );
            assert_eq!(count, 1);
            let (mut result, info, status) = fixture.take();
            assert_eq!(status, INKPOD_STATUS_OK);
            assert_eq!(
                (
                    info.session_id,
                    info.session_generation,
                    info.source_generation
                ),
                (51, 2, 3)
            );
            assert_eq!(
                info.kind,
                if mode == INKPOD_INKSCRIPT_RUN_INSTALL {
                    INKPOD_INKSCRIPT_STAGED_NEW_TAB
                } else {
                    INKPOD_INKSCRIPT_STAGED_IMAGE_PREVIEW
                }
            );
            let mut adopted = ptr::null_mut();
            assert_eq!(
                inkpod_core_inkscript_staged_result_take_core(
                    fixture.core,
                    &mut result,
                    &mut adopted
                ),
                INKPOD_STATUS_OK
            );
            assert!(result.is_null());
            let mut document = InkpodDocumentInfo {
                struct_size: size_of::<InkpodDocumentInfo>() as u32,
                ..Default::default()
            };
            assert_eq!(
                inkpod_core_get_document_info(adopted, &mut document),
                INKPOD_STATUS_OK
            );
            assert_ne!(
                (document.document_uuid_high, document.document_uuid_low),
                (0, 0x5501)
            );
            assert_eq!(fixture.take().2, INKPOD_STATUS_INVALID_STATE);
            assert_eq!(
                inkpod_core_inkscript_staged_result_release(fixture.core, &mut result),
                INKPOD_STATUS_OK
            );
            assert_eq!(inkpod_core_destroy(&mut adopted), INKPOD_STATUS_OK);
        }
    }
}

#[test]
fn shared_stale_and_terminal_cancel_do_not_publish_active_or_preview() {
    for (mode, active) in [
        (INKPOD_INKSCRIPT_RUN_INSTALL, true),
        (INKPOD_INKSCRIPT_RUN_IMAGE_PREVIEW, false),
    ] {
        for stale in [false, true] {
            let fixture = Fixture::new(mode, active, 1, 0);
            assert_eq!(fixture.finish(), INKPOD_STATUS_OK);
            // SAFETY: Atomics and shared invalidation are called with live synchronized owners.
            unsafe {
                if stale {
                    assert_eq!(
                        inkpod_core_inkscript_io_invalidate(fixture.core, fixture.io, 51),
                        INKPOD_STATUS_OK
                    );
                } else {
                    assert_eq!(
                        inkpod_inkscript_run_task_cancel(fixture.run),
                        INKPOD_STATUS_OK
                    );
                }
            }
            let (result, _, status) = fixture.take();
            assert!(result.is_null());
            assert_eq!(
                status,
                if stale {
                    INKPOD_STATUS_INVALID_STATE
                } else {
                    INKPOD_STATUS_CANCELLED
                }
            );
        }
    }
}

#[test]
fn shared_active_noop_retains_revision_and_cancelled_newtab_keeps_successful_item() {
    let fixture = Fixture::new(INKPOD_INKSCRIPT_RUN_INSTALL, true, 1, 0);
    assert_eq!(fixture.finish(), INKPOD_STATUS_OK);
    let (mut result, _, status) = fixture.take();
    assert_eq!(status, INKPOD_STATUS_OK);
    // SAFETY: Result and Core are exclusively owned and output DTO initialized.
    unsafe {
        let mut before = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_get_document_info(fixture.input, &mut before),
            INKPOD_STATUS_OK
        );
        let mut invalid = ptr::null_mut();
        assert_eq!(
            inkpod_core_inkscript_staged_result_take_core(fixture.core, &mut result, &mut invalid),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(!result.is_null() && invalid.is_null());
        assert_eq!(
            inkpod_core_inkscript_staged_result_apply_active(
                fixture.core,
                &mut result,
                fixture.input,
                51,
                2,
                3,
                0
            ),
            INKPOD_STATUS_OK
        );
        assert!(result.is_null());
        let mut after = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_get_document_info(fixture.input, &mut after),
            INKPOD_STATUS_OK
        );
        assert_eq!(before.document_revision, after.document_revision);
        assert_eq!(before.flags, after.flags);
    }
    let fixture = Fixture::new(INKPOD_INKSCRIPT_RUN_INSTALL, false, 1, 0);
    assert_eq!(fixture.finish(), INKPOD_STATUS_OK);
    // SAFETY: The already completed successful item remains owned until explicitly released.
    unsafe {
        assert_eq!(
            inkpod_inkscript_run_task_cancel(fixture.run),
            INKPOD_STATUS_OK
        );
        let (mut result, _, status) = fixture.take();
        assert_eq!(status, INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_inkscript_staged_result_release(fixture.core, &mut result),
            INKPOD_STATUS_OK
        );
    }
}

#[test]
fn shared_preview_resource_limit_and_pre_run_cancel_publish_no_result() {
    let fixture = Fixture::new(INKPOD_INKSCRIPT_RUN_IMAGE_PREVIEW, false, 1, 1);
    assert_eq!(fixture.finish(), INKPOD_STATUS_INVALID_ARGUMENT);
    assert!(fixture.take().0.is_null());
    let fixture = Fixture::new(INKPOD_INKSCRIPT_RUN_IMAGE_PREVIEW, false, 1, 0);
    // SAFETY: Task cancellation is atomic while the fixture keeps the task alive.
    unsafe {
        assert_eq!(
            inkpod_inkscript_run_task_cancel(fixture.run),
            INKPOD_STATUS_OK
        );
    }
    assert_eq!(fixture.finish(), INKPOD_STATUS_CANCELLED);
    assert!(fixture.take().0.is_null());
}

#[test]
fn shared_io_rejects_noncurrent_invalid_and_wrong_thread_without_ownership_loss() {
    // SAFETY: Every handle and advertised record is live, and cross-thread access is joined.
    unsafe {
        let mut core = new_core();
        let mut io = ptr::null_mut();
        let mut request = InkpodInkScriptIoRequest {
            struct_size: size_of::<InkpodInkScriptIoRequest>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            new_tab_capacity: 1,
            ..Default::default()
        };
        request.version += 1;
        assert_eq!(
            inkpod_core_inkscript_io_create(core, ptr::null_mut(), &request, &mut io),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert!(io.is_null());
        request.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        request.path_count = u64::MAX;
        assert_eq!(
            inkpod_core_inkscript_io_create(core, ptr::null_mut(), &request, &mut io),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(io.is_null());
        request.path_count = 0;
        assert_eq!(
            inkpod_core_inkscript_io_create(core, ptr::null_mut(), &request, &mut io),
            INKPOD_STATUS_OK
        );
        let address = core as usize;
        let io_address = io as usize;
        assert_eq!(
            std::thread::spawn(move || inkpod_core_inkscript_io_invalidate(
                address as *mut InkpodCore,
                io_address as *mut InkpodInkScriptIo,
                0
            ))
            .join()
            .unwrap(),
            INKPOD_STATUS_WRONG_THREAD
        );
        assert_eq!(
            inkpod_core_inkscript_io_release(core, &mut io),
            INKPOD_STATUS_OK
        );
        assert!(io.is_null());
        assert_eq!(
            inkpod_core_inkscript_io_release(core, &mut io),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn shared_active_change_is_one_undo_and_redo_and_late_cancel_preserves_state() {
    for late_cancel in [false, true] {
        let fixture = Fixture::with_options(INKPOD_INKSCRIPT_RUN_INSTALL, true, 1, 0, true, false);
        assert_eq!(fixture.finish(), INKPOD_STATUS_OK);
        let (mut result, _, status) = fixture.take();
        assert_eq!(status, INKPOD_STATUS_OK);
        // SAFETY: Every handle is fixture-owned. Query DTOs are initialized and disjoint.
        unsafe {
            let mut before = InkpodDocumentInfo {
                struct_size: size_of::<InkpodDocumentInfo>() as u32,
                ..Default::default()
            };
            assert_eq!(
                inkpod_core_get_document_info(fixture.input, &mut before),
                INKPOD_STATUS_OK
            );
            if late_cancel {
                let address = fixture.run as usize;
                assert_eq!(
                    std::thread::spawn(move || inkpod_inkscript_run_task_cancel(
                        address as *mut InkpodInkScriptRunTask
                    ))
                    .join()
                    .unwrap(),
                    INKPOD_STATUS_OK
                );
            }
            assert_eq!(
                inkpod_core_inkscript_staged_result_apply_active(
                    fixture.core,
                    &mut result,
                    fixture.input,
                    51,
                    2,
                    3,
                    0
                ),
                if late_cancel {
                    INKPOD_STATUS_CANCELLED
                } else {
                    INKPOD_STATUS_OK
                }
            );
            assert!(result.is_null());
            let mut after = InkpodDocumentInfo {
                struct_size: size_of::<InkpodDocumentInfo>() as u32,
                ..Default::default()
            };
            assert_eq!(
                inkpod_core_get_document_info(fixture.input, &mut after),
                INKPOD_STATUS_OK
            );
            if late_cancel {
                assert_eq!(after.document_revision, before.document_revision);
                assert_eq!(after.color_plane_checksum, before.color_plane_checksum);
                continue;
            }
            assert_eq!(after.document_revision, before.document_revision + 1);
            assert_ne!(after.color_plane_checksum, before.color_plane_checksum);
            let mut dispatch = InkpodDispatchResult {
                struct_size: size_of::<InkpodDispatchResult>() as u32,
                reserved: 0,
                revision: 0,
                accepted_command_count: 0,
            };
            assert_eq!(
                inkpod_core_undo(fixture.input, &mut dispatch),
                INKPOD_STATUS_OK
            );
            let mut undone = InkpodDocumentInfo {
                struct_size: size_of::<InkpodDocumentInfo>() as u32,
                ..Default::default()
            };
            assert_eq!(
                inkpod_core_get_document_info(fixture.input, &mut undone),
                INKPOD_STATUS_OK
            );
            assert_eq!(undone.color_plane_checksum, before.color_plane_checksum);
            assert_eq!(
                inkpod_core_redo(fixture.input, &mut dispatch),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_get_document_info(fixture.input, &mut undone),
                INKPOD_STATUS_OK
            );
            assert_eq!(undone.color_plane_checksum, after.color_plane_checksum);
        }
    }
}

#[test]
fn shared_sequence_uses_existing_canonical_plan_and_retains_staged_ownership() {
    let fixture = Fixture::with_options(INKPOD_INKSCRIPT_RUN_INSTALL, false, 1, 0, true, true);
    assert_eq!(fixture.finish(), INKPOD_STATUS_OK);
    let (mut result, info, status) = fixture.take();
    assert_eq!(status, INKPOD_STATUS_OK);
    assert_eq!(info.kind, INKPOD_INKSCRIPT_STAGED_NEW_TAB);
    // SAFETY: Result is still owned by this fixture's thread and parent Core.
    unsafe {
        assert_eq!(
            inkpod_core_inkscript_staged_result_release(fixture.core, &mut result),
            INKPOD_STATUS_OK
        );
    }
}

#[test]
fn shared_preview_after_take_observes_original_task_cancel_without_adoption() {
    let fixture = Fixture::new(INKPOD_INKSCRIPT_RUN_IMAGE_PREVIEW, false, 1, 0);
    assert_eq!(fixture.finish(), INKPOD_STATUS_OK);
    let (mut result, _, status) = fixture.take();
    assert_eq!(status, INKPOD_STATUS_OK);
    // SAFETY: Task is retained during result publication so its atomic cancellation is callable.
    unsafe {
        assert_eq!(
            inkpod_inkscript_run_task_cancel(fixture.run),
            INKPOD_STATUS_OK
        );
        let mut output = ptr::null_mut();
        assert_eq!(
            inkpod_core_inkscript_staged_result_take_core(fixture.core, &mut result, &mut output),
            INKPOD_STATUS_CANCELLED
        );
        assert!(output.is_null() && !result.is_null());
        assert_eq!(
            inkpod_core_inkscript_staged_result_release(fixture.core, &mut result),
            INKPOD_STATUS_OK
        );
    }
}

#[test]
fn shared_backing_validation_ignores_live_edits_but_revokes_changed_save_path() {
    let fixture = Fixture::with_options(INKPOD_INKSCRIPT_RUN_INSTALL, false, 1, 0, true, false);
    // SAFETY: All objects remain live on this owner thread and public output records are initialized.
    unsafe {
        let mut dispatch = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 0,
            revision: 0,
            accepted_command_count: 0,
        };
        assert_eq!(
            inkpod_core_undo(fixture.input, &mut dispatch),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_io_validate_session(fixture.core, fixture.io, 51, fixture.input),
            INKPOD_STATUS_OK
        );
        assert_eq!(fixture.finish(), INKPOD_STATUS_OK);
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "inkpod-ffi-shared-backing-{}-{unique}.inkpod",
            std::process::id()
        ));
        let utf8 = path.to_str().unwrap().as_bytes();
        let mut document = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_save(
                fixture.input,
                utf8.as_ptr(),
                utf8.len() as u64,
                &mut document
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_io_validate_session(fixture.core, fixture.io, 51, fixture.input),
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(fixture.take().2, INKPOD_STATUS_INVALID_STATE);
        std::fs::remove_file(path).unwrap();
    }
}
