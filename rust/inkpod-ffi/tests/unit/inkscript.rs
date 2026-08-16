use super::*;

fn source_text() -> &'static [u8] {
    br#"inkscript 2;
requires { procedure_catalog = 2; replay_epoch = 23; }
inputs { current_document; }
program {
    step "Set grid" {
        enabled = true;
        invoke set_grid {
            grid = { origin_x = 1; origin_y = 2; spacing_x = 8; spacing_y = 9; subdivisions = 2; };
        };
    }
}
output { policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = "ffi"; start_number = 1; direction = ascending; }
execution { failure = stop; wait_ms = 0; preview_before_save = false; }
"#
}

fn parameter_source_text() -> &'static [u8] {
    br#"inkscript 2;
requires { procedure_catalog = 2; replay_epoch = 23; }
inputs { current_document; }
parameters {
    param spacing: u32 = 8 { ask = each_run; };
}
program {
    step "Set grid parameter" {
        enabled = true;
        invoke set_grid {
            grid = { origin_x = 1; origin_y = 2; spacing_x = $spacing; spacing_y = 9; subdivisions = 2; };
        };
    }
}
output { policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = "ffi-param"; start_number = 1; direction = ascending; }
execution { failure = stop; wait_ms = 0; preview_before_save = false; }
"#
}

fn source_input(bytes: &[u8]) -> InkpodInkScriptSourceInput {
    InkpodInkScriptSourceInput {
        struct_size: size_of::<InkpodInkScriptSourceInput>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        feature_flags: INKPOD_FEATURE_NONE,
        controller_id: 41,
        session_generation: 7,
        source_id: 99,
        source_utf8: bytes.as_ptr(),
        source_bytes: bytes.len() as u64,
    }
}

fn compile_request() -> InkpodInkScriptCompileRequest {
    InkpodInkScriptCompileRequest {
        struct_size: size_of::<InkpodInkScriptCompileRequest>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        feature_flags: INKPOD_FEATURE_NONE,
        controller_id: 41,
        session_generation: 7,
        flags: 0,
        reserved: 0,
        parameter_choices: ptr::null(),
        parameter_choice_count: 0,
        parameter_choice_stride_bytes: 0,
        max_invocations: 0,
    }
}

fn new_core() -> *mut InkpodCore {
    let config = InkpodCoreConfig {
        struct_size: size_of::<InkpodCoreConfig>() as u32,
        abi_version: INKPOD_ABI_VERSION,
        feature_flags: INKPOD_FEATURE_NONE,
    };
    let mut core = ptr::null_mut();
    // SAFETY: The initialized config and owner output remain live for the call.
    assert_eq!(
        unsafe { inkpod_core_create(&config, &mut core) },
        INKPOD_STATUS_OK
    );
    core
}

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn inkscript_source_parse_copies_diagnostics_and_text_in_batches() {
    assert_eq!(INKPOD_ABI_VERSION, 15);
    assert_send_sync::<InkpodInkScriptSource>();
    assert_send_sync::<InkpodInkScriptProgram>();
    assert_send_sync::<InkpodInkScriptFragment>();
    let malformed = b"inkscript 1;\nprogram { @ }\n";
    let input = source_input(malformed);
    let mut source = ptr::null_mut();
    // SAFETY: Input bytes and unique owner storage remain live and do not overlap.
    assert_eq!(
        unsafe { inkpod_inkscript_source_parse(&input, &mut source) },
        INKPOD_STATUS_OK
    );
    assert!(!source.is_null());

    let mut summary = InkpodInkScriptSourceSummary {
        struct_size: size_of::<InkpodInkScriptSourceSummary>() as u32,
        ..Default::default()
    };
    // SAFETY: The source is live and summary is initialized writable storage.
    assert_eq!(
        unsafe { inkpod_inkscript_source_summary(source, &mut summary) },
        INKPOD_STATUS_OK
    );
    assert_eq!(summary.version, INKPOD_INKSCRIPT_RECORD_VERSION);
    assert_eq!(summary.controller_id, 41);
    assert_eq!(summary.session_generation, 7);
    assert_eq!(summary.source_id, 99);
    assert_eq!(summary.source_bytes, malformed.len() as u64);
    assert_eq!(summary.flags & INKPOD_INKSCRIPT_SOURCE_VALID, 0);
    assert_ne!(summary.flags & INKPOD_INKSCRIPT_SOURCE_COMPLETE, 0);
    assert!(summary.diagnostic_count >= 2);

    let mut text_query = InkpodInkScriptUtf8Buffer {
        struct_size: size_of::<InkpodInkScriptUtf8Buffer>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        ..Default::default()
    };
    // SAFETY: A null/zero buffer is the documented size-query form.
    assert_eq!(
        unsafe { inkpod_inkscript_source_text_copy(source, &mut text_query) },
        INKPOD_STATUS_BUFFER_TOO_SMALL
    );
    assert_eq!(text_query.required_bytes, malformed.len() as u64);
    let mut text = vec![0_u8; text_query.required_bytes as usize];
    text_query.bytes = text.as_mut_ptr();
    text_query.capacity_bytes = text.len() as u64;
    // SAFETY: The buffer has the advertised writable capacity.
    assert_eq!(
        unsafe { inkpod_inkscript_source_text_copy(source, &mut text_query) },
        INKPOD_STATUS_OK
    );
    assert_eq!(text_query.written_bytes, malformed.len() as u64);
    assert_eq!(text, malformed);

    let mut query = InkpodInkScriptDiagnosticBuffer {
        struct_size: size_of::<InkpodInkScriptDiagnosticBuffer>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        feature_flags: INKPOD_FEATURE_NONE,
        first_diagnostic: 0,
        records: ptr::null_mut(),
        record_capacity: 0,
        record_stride_bytes: 0,
        utf8: ptr::null_mut(),
        utf8_capacity_bytes: 0,
        records_written: 0,
        required_records: 0,
        utf8_written_bytes: 0,
        required_utf8_bytes: 0,
    };
    // SAFETY: Null/zero record and UTF-8 buffers request the required sizes.
    assert_eq!(
        unsafe { inkpod_inkscript_source_diagnostics_copy(source, &mut query) },
        INKPOD_STATUS_BUFFER_TOO_SMALL
    );
    assert_eq!(query.required_records, summary.diagnostic_count);
    assert!(query.required_utf8_bytes > 0);

    let mut short_query = query;
    short_query.struct_size -= 1;
    // SAFETY: The readable prefix intentionally advertises a short buffer record.
    assert_eq!(
        unsafe { inkpod_inkscript_source_diagnostics_copy(source, &mut short_query) },
        INKPOD_STATUS_INCOMPATIBLE_ABI
    );

    let mut records = vec![
        InkpodInkScriptDiagnostic {
            struct_size: size_of::<InkpodInkScriptDiagnostic>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            ..Default::default()
        };
        query.required_records as usize
    ];
    let mut utf8 = vec![0_u8; query.required_utf8_bytes as usize];
    query.records = records.as_mut_ptr();
    query.record_capacity = records.len() as u64;
    query.record_stride_bytes = size_of::<InkpodInkScriptDiagnostic>() as u64;
    query.utf8 = utf8.as_mut_ptr();
    query.utf8_capacity_bytes = utf8.len() as u64;
    // SAFETY: Every strided record and the packed UTF-8 buffer are initialized and writable.
    assert_eq!(
        unsafe { inkpod_inkscript_source_diagnostics_copy(source, &mut query) },
        INKPOD_STATUS_OK
    );
    assert_eq!(query.records_written, records.len() as u64);
    assert_eq!(query.utf8_written_bytes, utf8.len() as u64);
    assert!(records.iter().all(|record| {
        record.severity == INKPOD_INKSCRIPT_DIAGNOSTIC_ERROR
            && record.byte_start <= record.byte_end
            && record.start_line != 0
            && record.start_column != 0
    }));
    let code = &records[0];
    let code_start = code.code_offset as usize;
    let code_end = code_start + code.code_bytes as usize;
    assert!(
        std::str::from_utf8(&utf8[code_start..code_end])
            .unwrap()
            .starts_with("INKS-")
    );

    // SAFETY: The unique owner is consumed once; null owner storage is a no-op.
    assert_eq!(
        unsafe { inkpod_inkscript_source_release(&mut source) },
        INKPOD_STATUS_OK
    );
    assert!(source.is_null());
    assert_eq!(
        unsafe { inkpod_inkscript_source_release(&mut source) },
        INKPOD_STATUS_OK
    );
}

#[test]
fn inkscript_compile_and_export_are_generation_bound_and_failure_atomic() {
    let input = source_input(source_text());
    let mut source = ptr::null_mut();
    // SAFETY: Input bytes and owner storage are valid.
    assert_eq!(
        unsafe { inkpod_inkscript_source_parse(&input, &mut source) },
        INKPOD_STATUS_OK
    );
    let mut core = new_core();
    // SAFETY: The test owns the live Core on its creating thread.
    unsafe {
        (*core)
            .core
            .new_cell_with_uuid(8, 8, 72_000, 72_000, 0x2500)
            .unwrap();
        (*core).core.add_guide(GuideAxis::Vertical, 2).unwrap();
    }
    let request = compile_request();
    let mut program = ptr::null_mut();
    // SAFETY: Core/source/request and unique output owner are live and non-overlapping.
    assert_eq!(
        unsafe { inkpod_core_inkscript_compile(core, source, &request, &mut program) },
        INKPOD_STATUS_OK
    );
    assert!(!program.is_null());

    let mut program_summary = InkpodInkScriptProgramSummary {
        struct_size: size_of::<InkpodInkScriptProgramSummary>() as u32,
        ..Default::default()
    };
    // SAFETY: Program and parent Core are live on the owner thread.
    assert_eq!(
        unsafe { inkpod_core_inkscript_program_summary(core, program, &mut program_summary) },
        INKPOD_STATUS_OK
    );
    assert_eq!(program_summary.version, INKPOD_INKSCRIPT_RECORD_VERSION);
    assert_eq!(program_summary.controller_id, 41);
    assert_eq!(program_summary.session_generation, 7);
    assert_eq!(program_summary.max_invocations, 1);
    assert_ne!(program_summary.static_compile_digest, [0; 32]);
    assert_ne!(program_summary.core_generation, 0);

    let parameter_input = source_input(parameter_source_text());
    let mut parameter_source = ptr::null_mut();
    // SAFETY: Parameter source bytes and unique owner storage are valid.
    assert_eq!(
        unsafe { inkpod_inkscript_source_parse(&parameter_input, &mut parameter_source) },
        INKPOD_STATUS_OK
    );
    let parameter_name = b"spacing";
    let accepted = InkpodInkScriptParameterChoice {
        struct_size: size_of::<InkpodInkScriptParameterChoice>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        kind: INKPOD_INKSCRIPT_PARAMETER_ACCEPT_DEFAULT,
        reserved: 0,
        feature_flags: 0,
        name_utf8: parameter_name.as_ptr(),
        name_bytes: parameter_name.len() as u64,
        value_utf8: ptr::null(),
        value_bytes: 0,
    };
    let mut default_request = compile_request();
    default_request.parameter_choices = &accepted;
    default_request.parameter_choice_count = 1;
    default_request.parameter_choice_stride_bytes =
        size_of::<InkpodInkScriptParameterChoice>() as u64;
    let mut default_program = ptr::null_mut();
    // SAFETY: The complete default-accept record is borrowed only for this call.
    assert_eq!(
        unsafe {
            inkpod_core_inkscript_compile(
                core,
                parameter_source,
                &default_request,
                &mut default_program,
            )
        },
        INKPOD_STATUS_OK
    );
    let override_text = b"19";
    let override_choice = InkpodInkScriptParameterChoice {
        kind: INKPOD_INKSCRIPT_PARAMETER_OVERRIDE,
        value_utf8: override_text.as_ptr(),
        value_bytes: override_text.len() as u64,
        ..accepted
    };
    let mut override_request = default_request;
    override_request.parameter_choices = &override_choice;
    let mut override_program = ptr::null_mut();
    // SAFETY: The bounded standalone value is borrowed and copied during static compilation.
    assert_eq!(
        unsafe {
            inkpod_core_inkscript_compile(
                core,
                parameter_source,
                &override_request,
                &mut override_program,
            )
        },
        INKPOD_STATUS_OK
    );
    let mut default_summary = InkpodInkScriptProgramSummary {
        struct_size: size_of::<InkpodInkScriptProgramSummary>() as u32,
        ..Default::default()
    };
    let mut override_summary = default_summary;
    // SAFETY: Both generation-bound programs and summaries are live on the Core owner thread.
    assert_eq!(
        unsafe {
            inkpod_core_inkscript_program_summary(core, default_program, &mut default_summary)
        },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe {
            inkpod_core_inkscript_program_summary(core, override_program, &mut override_summary)
        },
        INKPOD_STATUS_OK
    );
    assert_ne!(
        default_summary.static_compile_digest,
        override_summary.static_compile_digest
    );
    let invalid_text = b"[1,,2]";
    let invalid_choice = InkpodInkScriptParameterChoice {
        value_utf8: invalid_text.as_ptr(),
        value_bytes: invalid_text.len() as u64,
        ..override_choice
    };
    let mut invalid_request = override_request;
    invalid_request.parameter_choices = &invalid_choice;
    let mut invalid_program = ptr::null_mut();
    // SAFETY: Invalid standalone value text is rejected before publication.
    assert_eq!(
        unsafe {
            inkpod_core_inkscript_compile(
                core,
                parameter_source,
                &invalid_request,
                &mut invalid_program,
            )
        },
        INKPOD_STATUS_INVALID_ARGUMENT
    );
    assert!(invalid_program.is_null());
    let wrong_type = b"true";
    let wrong_type_choice = InkpodInkScriptParameterChoice {
        value_utf8: wrong_type.as_ptr(),
        value_bytes: wrong_type.len() as u64,
        ..override_choice
    };
    let mut wrong_type_request = override_request;
    wrong_type_request.parameter_choices = &wrong_type_choice;
    // SAFETY: The value is syntactically valid but fails the source parameter's closed u32 type.
    assert_eq!(
        unsafe {
            inkpod_core_inkscript_compile(
                core,
                parameter_source,
                &wrong_type_request,
                &mut invalid_program,
            )
        },
        INKPOD_STATUS_INVALID_ARGUMENT
    );
    assert!(invalid_program.is_null());

    let core_address = core as usize;
    let program_address = program as usize;
    let wrong_thread = std::thread::spawn(move || {
        let mut summary = InkpodInkScriptProgramSummary {
            struct_size: size_of::<InkpodInkScriptProgramSummary>() as u32,
            ..Default::default()
        };
        // SAFETY: This deliberately exercises the wrong-thread rejection without concurrent use.
        unsafe {
            inkpod_core_inkscript_program_summary(
                core_address as *mut InkpodCore,
                program_address as *const InkpodInkScriptProgram,
                &mut summary,
            )
        }
    });
    assert_eq!(wrong_thread.join().unwrap(), INKPOD_STATUS_WRONG_THREAD);

    // SAFETY: The test temporarily substitutes only the empty ABI object registry, then restores
    // it before any further Core operation to exercise generation invalidation without destroying
    // the parent Core.
    let original_objects = unsafe {
        std::mem::replace(
            &mut (*core).objects,
            crate::v3::ObjectRegistry::new().unwrap(),
        )
    };
    assert_eq!(
        unsafe { inkpod_core_inkscript_program_summary(core, program, &mut program_summary) },
        INKPOD_STATUS_INVALID_STATE
    );
    unsafe { (*core).objects = original_objects };

    let event_id = unsafe {
        (*core)
            .core
            .journal_entries()
            .iter()
            .find_map(|entry| match entry {
                inkpod_core::JournalEntry::Commit(commit) => Some(commit.event_id().get()),
                inkpod_core::JournalEntry::HistoryMove(_)
                | inkpod_core::JournalEntry::BranchCut(_) => None,
            })
            .unwrap()
    };
    let event = InkpodInkScriptJournalEvent {
        struct_size: size_of::<InkpodInkScriptJournalEvent>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        event_id,
        reserved: 0,
    };
    let export_request = InkpodInkScriptExportRequest {
        struct_size: size_of::<InkpodInkScriptExportRequest>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        feature_flags: INKPOD_FEATURE_NONE,
        controller_id: 41,
        session_generation: 7,
        flags: 0,
        reserved: 0,
        events: &event,
        event_count: 1,
        event_stride_bytes: size_of::<InkpodInkScriptJournalEvent>() as u64,
        max_commits: 0,
        max_source_bytes: 0,
        max_inline_asset_bytes: 0,
    };
    let mut fragment = ptr::null_mut();
    // SAFETY: The request span and unique fragment owner are live on the Core owner thread.
    assert_eq!(
        unsafe { inkpod_core_inkscript_fragment_export(core, &export_request, &mut fragment) },
        INKPOD_STATUS_OK
    );
    assert!(!fragment.is_null());
    let mut fragment_summary = InkpodInkScriptFragmentSummary {
        struct_size: size_of::<InkpodInkScriptFragmentSummary>() as u32,
        ..Default::default()
    };
    // SAFETY: Fragment and its parent Core generation are live.
    assert_eq!(
        unsafe { inkpod_core_inkscript_fragment_summary(core, fragment, &mut fragment_summary) },
        INKPOD_STATUS_OK
    );
    assert_eq!(fragment_summary.commit_count, 1);
    assert_eq!(fragment_summary.controller_id, 41);
    assert_eq!(fragment_summary.session_generation, 7);
    assert!(fragment_summary.text_bytes > 0);

    let mut text_query = InkpodInkScriptUtf8Buffer {
        struct_size: size_of::<InkpodInkScriptUtf8Buffer>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        ..Default::default()
    };
    // SAFETY: Null/zero is the size-query form and Core/fragment are live.
    assert_eq!(
        unsafe { inkpod_core_inkscript_fragment_text_copy(core, fragment, &mut text_query) },
        INKPOD_STATUS_BUFFER_TOO_SMALL
    );
    let mut text = vec![0_u8; text_query.required_bytes as usize];
    text_query.bytes = text.as_mut_ptr();
    text_query.capacity_bytes = text.len() as u64;
    // SAFETY: The exact required writable span is supplied.
    assert_eq!(
        unsafe { inkpod_core_inkscript_fragment_text_copy(core, fragment, &mut text_query) },
        INKPOD_STATUS_OK
    );
    let text = std::str::from_utf8(&text).unwrap();
    assert!(text.starts_with("inkscript_fragment 2;\n"));
    assert!(text.contains("invoke add_guide"));

    let before = unsafe { (*core).core.document_state_digest().unwrap() };
    let mut cancelled = export_request;
    cancelled.flags = INKPOD_INKSCRIPT_EXPORT_CANCEL;
    let mut no_fragment = ptr::null_mut();
    // SAFETY: Request is valid and asks for pre-publication cancellation.
    assert_eq!(
        unsafe { inkpod_core_inkscript_fragment_export(core, &cancelled, &mut no_fragment) },
        INKPOD_STATUS_CANCELLED
    );
    assert!(no_fragment.is_null());
    assert_eq!(
        unsafe { (*core).core.document_state_digest().unwrap() },
        before
    );

    let mut limited = export_request;
    limited.max_source_bytes = 1;
    // SAFETY: The lowered limit fails before publishing an owned fragment.
    assert_eq!(
        unsafe { inkpod_core_inkscript_fragment_export(core, &limited, &mut no_fragment) },
        INKPOD_STATUS_INVALID_ARGUMENT
    );
    assert!(no_fragment.is_null());

    // SAFETY: Each unique owner is released once before its required parent Core.
    assert_eq!(
        unsafe { inkpod_core_inkscript_fragment_release(core, &mut fragment) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_core_inkscript_fragment_release(core, &mut fragment) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_core_inkscript_program_release(core, &mut program) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_core_inkscript_program_release(core, &mut default_program) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_core_inkscript_program_release(core, &mut override_program) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_inkscript_source_release(&mut parameter_source) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_inkscript_source_release(&mut source) },
        INKPOD_STATUS_OK
    );
    assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
}

#[test]
fn inkscript_abi_rejects_old_versions_invalid_records_and_stale_routes() {
    let old_config = InkpodCoreConfig {
        struct_size: size_of::<InkpodCoreConfig>() as u32,
        abi_version: 14,
        feature_flags: INKPOD_FEATURE_NONE,
    };
    let mut old_core = ptr::null_mut();
    // SAFETY: The complete old-version config is readable and output is unique.
    assert_eq!(
        unsafe { inkpod_core_create(&old_config, &mut old_core) },
        INKPOD_STATUS_INCOMPATIBLE_ABI
    );
    assert!(old_core.is_null());

    let mut input = source_input(source_text());
    let mut source = ptr::null_mut();
    // SAFETY: NULL and a deliberately misaligned record pointer must fail before dereference.
    assert_eq!(
        unsafe { inkpod_inkscript_source_parse(ptr::null(), &mut source) },
        INKPOD_STATUS_INVALID_ARGUMENT
    );
    let aligned_storage = std::mem::MaybeUninit::<InkpodInkScriptSourceInput>::uninit();
    // SAFETY: Adding one byte stays within the live record-sized storage and makes the typed
    // pointer deliberately misaligned; the ABI must reject it before reading the prefix.
    let misaligned_input = unsafe { aligned_storage.as_ptr().cast::<u8>().add(1) }
        .cast::<InkpodInkScriptSourceInput>();
    assert_eq!(
        unsafe { inkpod_inkscript_source_parse(misaligned_input, &mut source) },
        INKPOD_STATUS_INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe { inkpod_inkscript_source_parse(&input, ptr::null_mut()) },
        INKPOD_STATUS_INVALID_ARGUMENT
    );
    input.struct_size -= 1;
    // SAFETY: The readable prefix intentionally advertises an incompatible short record.
    assert_eq!(
        unsafe { inkpod_inkscript_source_parse(&input, &mut source) },
        INKPOD_STATUS_INCOMPATIBLE_ABI
    );
    assert!(source.is_null());
    input.struct_size = size_of::<InkpodInkScriptSourceInput>() as u32;
    input.feature_flags = 1;
    // SAFETY: Unknown feature flags are rejected before source bytes are retained.
    assert_eq!(
        unsafe { inkpod_inkscript_source_parse(&input, &mut source) },
        INKPOD_STATUS_UNSUPPORTED
    );
    input.feature_flags = 0;
    input.version = 0;
    // SAFETY: Unknown record versions are rejected exactly.
    assert_eq!(
        unsafe { inkpod_inkscript_source_parse(&input, &mut source) },
        INKPOD_STATUS_INCOMPATIBLE_ABI
    );
    input.version = INKPOD_INKSCRIPT_RECORD_VERSION;
    input.source_utf8 = std::ptr::dangling();
    input.source_bytes = (128_u64 * 1024 * 1024) + 1;
    // SAFETY: Oversize is rejected before the deliberately invalid data pointer is dereferenced.
    assert_eq!(
        unsafe { inkpod_inkscript_source_parse(&input, &mut source) },
        INKPOD_STATUS_INVALID_ARGUMENT
    );

    input = source_input(source_text());
    // SAFETY: The restored source input is valid.
    assert_eq!(
        unsafe { inkpod_inkscript_source_parse(&input, &mut source) },
        INKPOD_STATUS_OK
    );
    let mut core = new_core();
    let mut stale = compile_request();
    stale.session_generation += 1;
    let mut program = ptr::null_mut();
    // SAFETY: The mismatched immutable routing token is rejected without publication.
    assert_eq!(
        unsafe { inkpod_core_inkscript_compile(core, source, &stale, &mut program) },
        INKPOD_STATUS_INVALID_STATE
    );
    assert!(program.is_null());
    let mut unknown = compile_request();
    unknown.flags = u64::MAX;
    // SAFETY: Unknown flags are rejected without publishing a program.
    assert_eq!(
        unsafe { inkpod_core_inkscript_compile(core, source, &unknown, &mut program) },
        INKPOD_STATUS_UNSUPPORTED
    );
    let parameter_name = b"unused";
    let unknown_choice = InkpodInkScriptParameterChoice {
        struct_size: size_of::<InkpodInkScriptParameterChoice>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        kind: u32::MAX,
        reserved: 0,
        feature_flags: 0,
        name_utf8: parameter_name.as_ptr(),
        name_bytes: parameter_name.len() as u64,
        value_utf8: ptr::null(),
        value_bytes: 0,
    };
    let mut unknown_enum = compile_request();
    unknown_enum.parameter_choices = &unknown_choice;
    unknown_enum.parameter_choice_count = 1;
    unknown_enum.parameter_choice_stride_bytes = size_of::<InkpodInkScriptParameterChoice>() as u64;
    // SAFETY: The complete unknown-enum record is rejected before compilation.
    assert_eq!(
        unsafe { inkpod_core_inkscript_compile(core, source, &unknown_enum, &mut program) },
        INKPOD_STATUS_INVALID_ARGUMENT
    );
    let mut cancelled = compile_request();
    cancelled.flags = INKPOD_INKSCRIPT_COMPILE_CANCEL;
    // SAFETY: Explicit cancel publishes no program.
    assert_eq!(
        unsafe { inkpod_core_inkscript_compile(core, source, &cancelled, &mut program) },
        INKPOD_STATUS_CANCELLED
    );
    assert!(program.is_null());

    // SAFETY: Owner variables are live and releases occur in dependency order.
    assert_eq!(
        unsafe { inkpod_inkscript_source_release(&mut source) },
        INKPOD_STATUS_OK
    );
    assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
}
