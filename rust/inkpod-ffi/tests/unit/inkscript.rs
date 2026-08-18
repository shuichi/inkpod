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
    assert_eq!(INKPOD_ABI_VERSION, 16);
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
        abi_version: 15,
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

struct ExecutionHostPath {
    _key: Box<[u8]>,
    record: Box<InkpodInkScriptPathIdentity>,
}

impl ExecutionHostPath {
    fn existing(key: &str, object: u8, parent: u8) -> Self {
        Self::new(key, object, parent, false)
    }

    fn absent(key: &str, parent: u8) -> Self {
        Self::new(key, 0, parent, true)
    }

    fn new(key: &str, object: u8, parent: u8, absent: bool) -> Self {
        let key = key.as_bytes().to_vec().into_boxed_slice();
        let record = Box::new(InkpodInkScriptPathIdentity {
            struct_size: size_of::<InkpodInkScriptPathIdentity>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            feature_flags: 0,
            canonical_key: InkpodInkScriptUtf8Span {
                bytes: key.as_ptr(),
                byte_count: key.len() as u64,
            },
            volume_id: [1; 16],
            object_id: if absent { [0; 32] } else { [object; 32] },
            object_generation: if absent { 0 } else { 1 },
            alias_key: [if absent { 10 } else { object }; 32],
            parent_object_id: [parent; 32],
            parent_generation: if absent { 7 } else { 1 },
            parent_alias_key: [parent.saturating_add(1); 32],
            flags: if absent {
                INKPOD_INKSCRIPT_PATH_EXPECTED_ABSENT
            } else {
                0
            },
        });
        Self { _key: key, record }
    }
}

struct ExecutionHost {
    _label: Box<[u8]>,
    session: Box<InkpodInkScriptSessionInput>,
    root: ExecutionHostPath,
    destination: ExecutionHostPath,
    authority_generation: u64,
    open_generation: u64,
    temporary: InkpodInkScriptTemporaryIdentity,
    temporary_bytes: Vec<u8>,
    installed_bytes: Vec<u8>,
    fail_write: bool,
}

impl ExecutionHost {
    fn new(input_core: *mut InkpodCore) -> Self {
        let label = b"current.inkpod".to_vec().into_boxed_slice();
        let session = Box::new(InkpodInkScriptSessionInput {
            struct_size: size_of::<InkpodInkScriptSessionInput>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            feature_flags: 0,
            core: input_core,
            session_id: 71,
            session_generation: 3,
            source_generation: 5,
            display_label: InkpodInkScriptUtf8Span {
                bytes: label.as_ptr(),
                byte_count: label.len() as u64,
            },
            display_number: 1,
            reserved: 0,
            backing_path: ptr::null(),
        });
        Self {
            _label: label,
            session,
            root: ExecutionHostPath::existing("root:/out", 60, 70),
            destination: ExecutionHostPath::absent("root:/out/ffi_0001.inkpod", 60),
            authority_generation: 9,
            open_generation: 4,
            temporary: InkpodInkScriptTemporaryIdentity {
                volume_id: [1; 16],
                parent_object_id: [60; 32],
                parent_generation: 7,
                object_id: [88; 32],
                object_generation: 1,
            },
            temporary_bytes: Vec::new(),
            installed_bytes: Vec::new(),
            fail_write: false,
        }
    }
}

unsafe extern "C" fn execution_host_call(
    context: *mut core::ffi::c_void,
    request: *const InkpodInkScriptHostRequest,
    response: *mut InkpodInkScriptHostResponse,
) -> u32 {
    if context.is_null() || request.is_null() || response.is_null() {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: The test owns the context and callback records for the task lifetime.
    let host = unsafe { &mut *context.cast::<ExecutionHost>() };
    // SAFETY: The ABI passes a complete request record.
    let request = unsafe { &*request };
    // SAFETY: The ABI passes unique writable response storage.
    let response = unsafe { &mut *response };
    response.struct_size = size_of::<InkpodInkScriptHostResponse>() as u32;
    response.version = INKPOD_INKSCRIPT_RECORD_VERSION;
    response.feature_flags = 0;
    match request.operation {
        INKPOD_INKSCRIPT_HOST_AUTHORITY_GENERATION => {
            response.generation = host.authority_generation;
        }
        INKPOD_INKSCRIPT_HOST_OPEN_SESSIONS => {
            response.generation = host.open_generation;
            response.records = ptr::null();
            response.record_count = 0;
            response.record_stride_bytes = 0;
        }
        INKPOD_INKSCRIPT_HOST_CURRENT_DOCUMENT => {
            response.flags = INKPOD_INKSCRIPT_HOST_RESPONSE_PRESENT;
            response.session = host.session.as_ref();
        }
        INKPOD_INKSCRIPT_HOST_CURRENT_SEQUENCE => return INKPOD_STATUS_NO_DOCUMENT,
        INKPOD_INKSCRIPT_HOST_RESOLVE_DESTINATION => {
            response.identity = host.destination.record.as_ref();
        }
        INKPOD_INKSCRIPT_HOST_OPEN_SESSION_GENERATION => {
            response.generation = host.open_generation;
        }
        INKPOD_INKSCRIPT_HOST_SESSION_IS_CURRENT => {
            response.flags = if request.session_id == 71
                && request.session_generation == 3
                && request.source_generation == 5
            {
                INKPOD_INKSCRIPT_HOST_RESPONSE_PRESENT
            } else {
                0
            };
        }
        INKPOD_INKSCRIPT_HOST_ATOMIC_CAPABILITIES => {
            response.flags = INKPOD_INKSCRIPT_HOST_CAPABILITY_INSTALL
                | INKPOD_INKSCRIPT_HOST_CAPABILITY_OVERWRITE;
        }
        INKPOD_INKSCRIPT_HOST_PREPARE_DESTINATION
        | INKPOD_INKSCRIPT_HOST_REVALIDATE_DESTINATION => {
            response.identity = host.destination.record.as_ref();
            response.records = ptr::null();
            response.record_count = 0;
            response.record_stride_bytes = 0;
        }
        INKPOD_INKSCRIPT_HOST_CREATE_TEMPORARY | INKPOD_INKSCRIPT_HOST_REVALIDATE_TEMPORARY => {
            response.temporary = host.temporary;
        }
        INKPOD_INKSCRIPT_HOST_WRITE_CLOSE_TEMPORARY => {
            if host.fail_write {
                return INKPOD_STATUS_IO_ERROR;
            }
            if request.byte_count != 0 && request.bytes.is_null() {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            // SAFETY: The run adapter lends the encoded native bytes for this callback.
            host.temporary_bytes =
                unsafe { slice::from_raw_parts(request.bytes, request.byte_count as usize) }
                    .to_vec();
            response.temporary = host.temporary;
        }
        INKPOD_INKSCRIPT_HOST_ATOMIC_INSTALL => {
            host.installed_bytes = std::mem::take(&mut host.temporary_bytes);
            response.result_kind = 1;
        }
        INKPOD_INKSCRIPT_HOST_CLEANUP_TEMPORARY => host.temporary_bytes.clear(),
        _ => return INKPOD_STATUS_UNSUPPORTED,
    }
    INKPOD_STATUS_OK
}

fn execution_host_record(host: &mut ExecutionHost) -> InkpodInkScriptHostAdapter {
    InkpodInkScriptHostAdapter {
        struct_size: size_of::<InkpodInkScriptHostAdapter>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        feature_flags: 0,
        context: (host as *mut ExecutionHost).cast(),
        call: Some(execution_host_call),
    }
}

unsafe fn compiled_execution_fixture() -> (
    *mut InkpodCore,
    *mut InkpodCore,
    *mut InkpodInkScriptSource,
    *mut InkpodInkScriptProgram,
) {
    let owner = new_core();
    let input_core = new_core();
    // SAFETY: Test owns the input Core on the current thread.
    unsafe {
        (*input_core)
            .core
            .new_cell_with_uuid(8, 8, 72_000, 72_000, 0x2600)
            .unwrap();
    }
    let input = source_input(source_text());
    let mut source = ptr::null_mut();
    // SAFETY: Source bytes and owner storage are live.
    assert_eq!(
        unsafe { inkpod_inkscript_source_parse(&input, &mut source) },
        INKPOD_STATUS_OK
    );
    let mut program = ptr::null_mut();
    let request = compile_request();
    // SAFETY: Core/source/request and unique owner storage are live.
    assert_eq!(
        unsafe { inkpod_core_inkscript_compile(owner, source, &request, &mut program) },
        INKPOD_STATUS_OK
    );
    (owner, input_core, source, program)
}

unsafe fn make_plan(
    owner: *mut InkpodCore,
    program: *mut InkpodInkScriptProgram,
    host: &mut ExecutionHost,
) -> (*mut InkpodInkScriptPlanTask, *mut InkpodInkScriptPlan) {
    let grant = InkpodInkScriptAuthorityGrant {
        struct_size: size_of::<InkpodInkScriptAuthorityGrant>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        access: INKPOD_INKSCRIPT_PATH_CREATE,
        reserved: 0,
        feature_flags: 0,
        intent_id: 1,
        authority_id: [1; 32],
        authority_generation: 9,
        resolved: host.root.record.as_ref(),
    };
    let request = InkpodInkScriptPlanTaskRequest {
        struct_size: size_of::<InkpodInkScriptPlanTaskRequest>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        feature_flags: 0,
        controller_id: 41,
        session_generation: 7,
        authority_generation: 9,
        open_session_set_generation: 4,
        grants: &grant,
        grant_count: 1,
        grant_stride_bytes: size_of::<InkpodInkScriptAuthorityGrant>() as u64,
        script_path: ptr::null(),
        maximum_folder_entries: 0,
        host: execution_host_record(host),
    };
    let mut task = ptr::null_mut();
    // SAFETY: Every request and owner pointer remains live for handle creation.
    assert_eq!(
        unsafe { inkpod_core_inkscript_plan_task_create(owner, program, &request, &mut task) },
        INKPOD_STATUS_OK
    );
    let task_address = task as usize;
    let (query_status, mut task_info) = std::thread::spawn(move || {
        let mut task_info = InkpodTaskInfo {
            struct_size: size_of::<InkpodTaskInfo>() as u32,
            state: 0,
            completed_work: 0,
            total_work: 0,
            reserved: 0,
        };
        // SAFETY: Query is read-only, the task remains live until join, and release is excluded.
        let status = unsafe {
            inkpod_inkscript_plan_task_query(
                task_address as *const InkpodInkScriptPlanTask,
                &mut task_info,
            )
        };
        (status, task_info)
    })
    .join()
    .expect("plan-task query thread must not panic");
    assert_eq!(query_status, INKPOD_STATUS_OK);
    assert_eq!(task_info.state, INKPOD_TASK_READY);
    // SAFETY: The task and parent Core are live on the owner thread.
    assert_eq!(
        unsafe { inkpod_core_inkscript_plan_task_advance(owner, task) },
        INKPOD_STATUS_OK
    );
    // The bounded one-event queue must be drained before another advance.
    assert_eq!(
        unsafe { inkpod_core_inkscript_plan_task_advance(owner, task) },
        INKPOD_STATUS_QUEUE_FULL
    );
    let mut event = InkpodInkScriptTaskEvent {
        struct_size: size_of::<InkpodInkScriptTaskEvent>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        ..Default::default()
    };
    // SAFETY: Event output and task are live.
    assert_eq!(
        unsafe { inkpod_core_inkscript_plan_task_event_take(owner, task, &mut event) },
        INKPOD_STATUS_OK
    );
    assert_eq!(event.kind, INKPOD_INKSCRIPT_EVENT_PLAN_COMPLETE);
    assert_eq!(
        unsafe { inkpod_inkscript_plan_task_query(task, &mut task_info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(task_info.state, INKPOD_TASK_COMPLETED);
    let mut plan = ptr::null_mut();
    // SAFETY: Successful task transfers one plan into unique owner storage.
    assert_eq!(
        unsafe { inkpod_core_inkscript_plan_task_take_plan(owner, task, &mut plan) },
        INKPOD_STATUS_OK
    );
    (task, plan)
}

#[test]
fn inkscript_execution_abi_plans_runs_installs_and_batches_reports() {
    // SAFETY: The fixture owns every handle and callback context on this thread.
    unsafe {
        let (mut owner, mut input_core, mut source, mut program) = compiled_execution_fixture();
        let input_before = (*input_core).core.document_state_digest().unwrap();
        let input_info = (*input_core).core.document_info().unwrap();
        let input_editor = (*input_core).core.editor_state().unwrap();
        let input_history = (*input_core).core.history_entries();
        let mut host = ExecutionHost::new(input_core);

        let mut intent_query = InkpodInkScriptPathIntentBuffer {
            struct_size: size_of::<InkpodInkScriptPathIntentBuffer>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_inkscript_program_path_intents_copy(owner, program, &mut intent_query),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        assert_eq!(intent_query.required_records, 1);
        let mut intent_records = vec![InkpodInkScriptPathIntent {
            struct_size: size_of::<InkpodInkScriptPathIntent>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            ..Default::default()
        }];
        let mut intent_utf8 = vec![0; intent_query.required_utf8_bytes as usize];
        intent_query.records = intent_records.as_mut_ptr();
        intent_query.record_capacity = intent_records.len() as u64;
        intent_query.record_stride_bytes = size_of::<InkpodInkScriptPathIntent>() as u64;
        intent_query.utf8 = intent_utf8.as_mut_ptr();
        intent_query.utf8_capacity_bytes = intent_utf8.len() as u64;
        assert_eq!(
            inkpod_core_inkscript_program_path_intents_copy(owner, program, &mut intent_query),
            INKPOD_STATUS_OK
        );
        assert_eq!(intent_records[0].access, INKPOD_INKSCRIPT_PATH_CREATE);
        assert_eq!(
            intent_records[0].subject_kind,
            INKPOD_INKSCRIPT_INTENT_OUTPUT_ROOT
        );
        assert_eq!(
            &intent_utf8[intent_records[0].text_offset as usize
                ..(intent_records[0].text_offset + intent_records[0].text_bytes) as usize],
            b"out"
        );

        let (mut plan_task, mut plan) = make_plan(owner, program, &mut host);
        let mut plan_summary = InkpodInkScriptPlanSummary {
            struct_size: size_of::<InkpodInkScriptPlanSummary>() as u32,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_inkscript_plan_summary(owner, plan, &mut plan_summary),
            INKPOD_STATUS_OK
        );
        assert_eq!(plan_summary.item_count, 1);
        assert_ne!(plan_summary.plan_digest, [0; 32]);

        let mut preview = InkpodInkScriptPreviewBuffer {
            struct_size: size_of::<InkpodInkScriptPreviewBuffer>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_inkscript_plan_preview_copy(owner, plan, &mut preview),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        let mut preview_records = vec![InkpodInkScriptPreviewItem {
            struct_size: size_of::<InkpodInkScriptPreviewItem>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            ..Default::default()
        }];
        let mut preview_utf8 = vec![0; preview.required_utf8_bytes as usize];
        preview.records = preview_records.as_mut_ptr();
        preview.record_capacity = 1;
        preview.record_stride_bytes = size_of::<InkpodInkScriptPreviewItem>() as u64;
        preview.utf8 = preview_utf8.as_mut_ptr();
        preview.utf8_capacity_bytes = preview_utf8.len() as u64;
        assert_eq!(
            inkpod_core_inkscript_plan_preview_copy(owner, plan, &mut preview),
            INKPOD_STATUS_OK
        );
        assert_eq!(preview_records[0].ordinal, 0);
        assert_eq!(
            &preview_utf8[preview_records[0].output_offset as usize
                ..(preview_records[0].output_offset + preview_records[0].output_bytes) as usize],
            b"ffi_0001.inkpod"
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
                owner,
                plan,
                &confirmation_request,
                &mut confirmation,
            ),
            INKPOD_STATUS_OK
        );
        let run_request = InkpodInkScriptRunRequest {
            struct_size: size_of::<InkpodInkScriptRunRequest>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            mode: INKPOD_INKSCRIPT_RUN_INSTALL,
            reserved: 0,
            feature_flags: 0,
            controller_id: 41,
            session_generation: 7,
            maximum_output_bytes: 0,
            host: execution_host_record(&mut host),
        };
        let mut run_task = ptr::null_mut();
        assert_eq!(
            inkpod_core_inkscript_run_task_create(
                owner,
                program,
                &mut plan,
                &mut confirmation,
                &run_request,
                &mut run_task,
            ),
            INKPOD_STATUS_OK
        );
        assert!(plan.is_null());
        assert!(confirmation.is_null());
        let mut run_info = InkpodTaskInfo {
            struct_size: size_of::<InkpodTaskInfo>() as u32,
            state: 0,
            completed_work: 0,
            total_work: 0,
            reserved: 0,
        };
        assert_eq!(
            inkpod_inkscript_run_task_query(run_task, &mut run_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(run_info.state, INKPOD_TASK_READY);
        assert_eq!(
            inkpod_core_inkscript_run_task_advance(owner, run_task),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_run_task_advance(owner, run_task),
            INKPOD_STATUS_QUEUE_FULL
        );
        let mut event = InkpodInkScriptTaskEvent {
            struct_size: size_of::<InkpodInkScriptTaskEvent>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_inkscript_run_task_event_take(owner, run_task, &mut event),
            INKPOD_STATUS_OK
        );
        assert_eq!(event.kind, INKPOD_INKSCRIPT_EVENT_ITEM_COMPLETE);
        assert_eq!(event.outcome, INKPOD_INKSCRIPT_OUTCOME_INSTALLED);
        assert_eq!(
            inkpod_core_inkscript_run_task_advance(owner, run_task),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_run_task_event_take(owner, run_task, &mut event),
            INKPOD_STATUS_OK
        );
        assert_eq!(event.kind, INKPOD_INKSCRIPT_EVENT_RUN_COMPLETE);
        assert_eq!(
            inkpod_inkscript_run_task_query(run_task, &mut run_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(run_info.state, INKPOD_TASK_COMPLETED);

        let mut report = ptr::null_mut();
        assert_eq!(
            inkpod_core_inkscript_run_task_take_report(owner, run_task, &mut report),
            INKPOD_STATUS_OK
        );
        let mut report_summary = InkpodInkScriptReportSummary {
            struct_size: size_of::<InkpodInkScriptReportSummary>() as u32,
            ..Default::default()
        };
        assert_eq!(
            inkpod_inkscript_report_summary(report, &mut report_summary),
            INKPOD_STATUS_OK
        );
        assert_eq!(report_summary.item_count, 1);
        assert_eq!(report_summary.flags, 0);
        let mut report_buffer = InkpodInkScriptReportBuffer {
            struct_size: size_of::<InkpodInkScriptReportBuffer>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            ..Default::default()
        };
        assert_eq!(
            inkpod_inkscript_report_items_copy(report, &mut report_buffer),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        let mut report_records = vec![InkpodInkScriptReportItem {
            struct_size: size_of::<InkpodInkScriptReportItem>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            ..Default::default()
        }];
        let mut report_utf8 = vec![0; report_buffer.required_utf8_bytes as usize];
        report_buffer.records = report_records.as_mut_ptr();
        report_buffer.record_capacity = 1;
        report_buffer.record_stride_bytes = size_of::<InkpodInkScriptReportItem>() as u64;
        report_buffer.utf8 = report_utf8.as_mut_ptr();
        report_buffer.utf8_capacity_bytes = report_utf8.len() as u64;
        assert_eq!(
            inkpod_inkscript_report_items_copy(report, &mut report_buffer),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            report_records[0].outcome,
            INKPOD_INKSCRIPT_OUTCOME_INSTALLED
        );
        assert_eq!(report_records[0].commit_count, 1);
        assert!(!host.installed_bytes.is_empty());

        inkpod_format::decode_procedure_file(&host.installed_bytes).unwrap();
        let reopen_path =
            std::env::temp_dir().join(format!("inkpod-m26-{}-reopen.inkpod", std::process::id()));
        std::fs::write(&reopen_path, &host.installed_bytes).unwrap();
        let mut reopened = inkpod_core::Core::new();
        reopened.open(&reopen_path).unwrap();
        std::fs::remove_file(&reopen_path).unwrap();
        assert_eq!(
            reopened.persistence_info().unwrap().open_strategy,
            inkpod_core::NativeOpenStrategy::FullReplay
        );
        assert!(!reopened.document_info().unwrap().dirty);
        assert!(!reopened.editor_state().unwrap().dirty);
        assert_eq!(reopened.history_entries().len(), input_history.len() + 1);
        assert!(report_records[0].next_stable_id > input_info.color_plane_id);
        let mut moved = reopened.clone();
        moved.undo().unwrap();
        assert_eq!(moved.document_state_digest().unwrap(), input_before);
        moved.redo().unwrap();
        assert_eq!(
            moved.document_state_digest().unwrap(),
            reopened.document_state_digest().unwrap()
        );

        assert_eq!(
            (*input_core).core.document_state_digest().unwrap(),
            input_before
        );
        assert_eq!(
            (*input_core)
                .core
                .document_info()
                .unwrap()
                .document_revision,
            input_info.document_revision
        );
        assert_eq!((*input_core).core.editor_state().unwrap(), input_editor);
        assert!(!(*input_core).core.document_info().unwrap().dirty);

        assert_eq!(
            inkpod_inkscript_report_release(&mut report),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_inkscript_report_release(&mut report),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_run_task_release(owner, &mut run_task),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_plan_task_release(owner, &mut plan_task),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_program_release(owner, &mut program),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_inkscript_source_release(&mut source),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_destroy(&mut input_core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut owner), INKPOD_STATUS_OK);
    }
}

#[test]
fn inkscript_execution_abi_cancel_stale_and_save_failure_are_atomic() {
    // SAFETY: Each subcase owns every handle and callback context on this thread.
    unsafe {
        let (mut owner, mut input_core, mut source, mut program) = compiled_execution_fixture();
        let input_digest = (*input_core).core.document_state_digest().unwrap();
        let input_history = (*input_core).core.history_entries();
        let mut host = ExecutionHost::new(input_core);

        let mut null_task = ptr::null_mut();
        assert_eq!(
            inkpod_core_inkscript_plan_task_create(owner, program, ptr::null(), &mut null_task),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(null_task.is_null());

        let mut intent_buffer = InkpodInkScriptPathIntentBuffer {
            struct_size: size_of::<InkpodInkScriptPathIntentBuffer>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_inkscript_program_path_intents_copy(owner, program, &mut intent_buffer),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        let mut short_intent = [InkpodInkScriptPathIntent {
            struct_size: size_of::<InkpodInkScriptPathIntent>() as u32 - 1,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            ..Default::default()
        }];
        let mut intent_utf8 = vec![0_u8; intent_buffer.required_utf8_bytes as usize];
        intent_buffer.records = short_intent.as_mut_ptr();
        intent_buffer.record_capacity = 1;
        intent_buffer.record_stride_bytes = size_of::<InkpodInkScriptPathIntent>() as u64;
        intent_buffer.utf8 = intent_utf8.as_mut_ptr();
        intent_buffer.utf8_capacity_bytes = intent_utf8.len() as u64;
        assert_eq!(
            inkpod_core_inkscript_program_path_intents_copy(owner, program, &mut intent_buffer),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(intent_buffer.records_written, 0);
        assert_eq!(intent_buffer.utf8_written_bytes, 0);

        let grant = InkpodInkScriptAuthorityGrant {
            struct_size: size_of::<InkpodInkScriptAuthorityGrant>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            access: INKPOD_INKSCRIPT_PATH_CREATE,
            reserved: 0,
            feature_flags: 0,
            intent_id: 1,
            authority_id: [1; 32],
            authority_generation: 9,
            resolved: host.root.record.as_ref(),
        };
        let mut plan_request = InkpodInkScriptPlanTaskRequest {
            struct_size: size_of::<InkpodInkScriptPlanTaskRequest>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            feature_flags: 0,
            controller_id: 41,
            session_generation: 7,
            authority_generation: 9,
            open_session_set_generation: 4,
            grants: &grant,
            grant_count: 1,
            grant_stride_bytes: size_of::<InkpodInkScriptAuthorityGrant>() as u64,
            script_path: ptr::null(),
            maximum_folder_entries: 0,
            host: execution_host_record(&mut host),
        };
        let mut cancelled_task = ptr::null_mut();
        assert_eq!(
            inkpod_core_inkscript_plan_task_create(
                owner,
                program,
                &plan_request,
                &mut cancelled_task,
            ),
            INKPOD_STATUS_OK
        );
        let cancelled_task_address = cancelled_task as usize;
        assert_eq!(
            std::thread::spawn(move || {
                inkpod_inkscript_plan_task_cancel(
                    cancelled_task_address as *const InkpodInkScriptPlanTask,
                )
            })
            .join()
            .expect("plan-task cancellation thread must not panic"),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_plan_task_advance(owner, cancelled_task),
            INKPOD_STATUS_CANCELLED
        );
        let mut no_plan = ptr::null_mut();
        assert_eq!(
            inkpod_core_inkscript_plan_task_take_plan(owner, cancelled_task, &mut no_plan),
            INKPOD_STATUS_INVALID_STATE
        );
        assert!(no_plan.is_null());
        assert_eq!(
            inkpod_core_inkscript_plan_task_release(owner, &mut cancelled_task),
            INKPOD_STATUS_OK
        );

        plan_request.feature_flags = 1;
        let mut invalid_task = ptr::null_mut();
        assert_eq!(
            inkpod_core_inkscript_plan_task_create(
                owner,
                program,
                &plan_request,
                &mut invalid_task,
            ),
            INKPOD_STATUS_UNSUPPORTED
        );
        assert!(invalid_task.is_null());
        plan_request.feature_flags = 0;
        plan_request.struct_size -= 1;
        assert_eq!(
            inkpod_core_inkscript_plan_task_create(
                owner,
                program,
                &plan_request,
                &mut invalid_task,
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
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

        let (mut released_plan_task, mut released_plan) = make_plan(owner, program, &mut host);
        let mut released_confirmation = ptr::null_mut();
        assert_eq!(
            inkpod_core_inkscript_confirmation_create(
                owner,
                released_plan,
                &confirmation_request,
                &mut released_confirmation,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_confirmation_release(owner, &mut released_confirmation),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_confirmation_release(owner, &mut released_confirmation),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_plan_release(owner, &mut released_plan),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_plan_release(owner, &mut released_plan),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_plan_task_release(owner, &mut released_plan_task),
            INKPOD_STATUS_OK
        );

        let (mut cancelled_plan_task, mut cancelled_plan) = make_plan(owner, program, &mut host);
        let mut cancelled_confirmation = ptr::null_mut();
        assert_eq!(
            inkpod_core_inkscript_confirmation_create(
                owner,
                cancelled_plan,
                &confirmation_request,
                &mut cancelled_confirmation,
            ),
            INKPOD_STATUS_OK
        );
        let cancelled_run_request = InkpodInkScriptRunRequest {
            struct_size: size_of::<InkpodInkScriptRunRequest>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            mode: INKPOD_INKSCRIPT_RUN_INSTALL,
            reserved: 0,
            feature_flags: 0,
            controller_id: 41,
            session_generation: 7,
            maximum_output_bytes: 0,
            host: execution_host_record(&mut host),
        };
        let mut cancelled_run_task = ptr::null_mut();
        assert_eq!(
            inkpod_core_inkscript_run_task_create(
                owner,
                program,
                &mut cancelled_plan,
                &mut cancelled_confirmation,
                &cancelled_run_request,
                &mut cancelled_run_task,
            ),
            INKPOD_STATUS_OK
        );
        let cancelled_run_task_address = cancelled_run_task as usize;
        assert_eq!(
            std::thread::spawn(move || {
                inkpod_inkscript_run_task_cancel(
                    cancelled_run_task_address as *const InkpodInkScriptRunTask,
                )
            })
            .join()
            .expect("run-task cancellation thread must not panic"),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_run_task_advance(owner, cancelled_run_task),
            INKPOD_STATUS_OK
        );
        let mut cancelled_event = InkpodInkScriptTaskEvent {
            struct_size: size_of::<InkpodInkScriptTaskEvent>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_inkscript_run_task_event_take(
                owner,
                cancelled_run_task,
                &mut cancelled_event,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(cancelled_event.kind, INKPOD_INKSCRIPT_EVENT_ITEM_COMPLETE);
        assert_eq!(cancelled_event.outcome, INKPOD_INKSCRIPT_OUTCOME_CANCELLED);
        assert_eq!(
            inkpod_core_inkscript_run_task_advance(owner, cancelled_run_task),
            INKPOD_STATUS_CANCELLED
        );
        let mut cancelled_run_info = InkpodTaskInfo {
            struct_size: size_of::<InkpodTaskInfo>() as u32,
            state: 0,
            completed_work: 0,
            total_work: 0,
            reserved: 0,
        };
        assert_eq!(
            inkpod_inkscript_run_task_query(cancelled_run_task, &mut cancelled_run_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(cancelled_run_info.state, INKPOD_TASK_CANCELLED);
        assert_eq!(
            inkpod_core_inkscript_run_task_event_take(
                owner,
                cancelled_run_task,
                &mut cancelled_event,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(cancelled_event.kind, INKPOD_INKSCRIPT_EVENT_RUN_COMPLETE);
        assert_eq!(cancelled_event.task_state, INKPOD_TASK_CANCELLED);
        assert_eq!(
            inkpod_core_inkscript_run_task_release(owner, &mut cancelled_run_task),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_plan_task_release(owner, &mut cancelled_plan_task),
            INKPOD_STATUS_OK
        );

        let (mut plan_task, mut plan) = make_plan(owner, program, &mut host);
        let mut confirmation = ptr::null_mut();
        assert_eq!(
            inkpod_core_inkscript_confirmation_create(
                owner,
                plan,
                &confirmation_request,
                &mut confirmation,
            ),
            INKPOD_STATUS_OK
        );
        host.fail_write = true;
        let run_request = InkpodInkScriptRunRequest {
            struct_size: size_of::<InkpodInkScriptRunRequest>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            mode: INKPOD_INKSCRIPT_RUN_INSTALL,
            reserved: 0,
            feature_flags: 0,
            controller_id: 41,
            session_generation: 7,
            maximum_output_bytes: 0,
            host: execution_host_record(&mut host),
        };
        let mut run_task = ptr::null_mut();
        assert_eq!(
            inkpod_core_inkscript_run_task_create(
                owner,
                program,
                &mut plan,
                &mut confirmation,
                &run_request,
                &mut run_task,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_run_task_advance(owner, run_task),
            INKPOD_STATUS_OK
        );
        let mut event = InkpodInkScriptTaskEvent {
            struct_size: size_of::<InkpodInkScriptTaskEvent>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_inkscript_run_task_event_take(owner, run_task, &mut event),
            INKPOD_STATUS_OK
        );
        assert_eq!(event.outcome, INKPOD_INKSCRIPT_OUTCOME_FAILED);
        assert_eq!(event.failure, INKPOD_INKSCRIPT_FAILURE_SAVE);
        assert!(host.installed_bytes.is_empty());
        assert!(host.temporary_bytes.is_empty());
        assert_eq!(
            (*input_core).core.document_state_digest().unwrap(),
            input_digest
        );
        assert_eq!((*input_core).core.history_entries(), input_history);

        assert_eq!(
            inkpod_core_inkscript_run_task_release(owner, &mut run_task),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_plan_task_release(owner, &mut plan_task),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_program_release(owner, &mut program),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_inkscript_source_release(&mut source),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_destroy(&mut input_core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut owner), INKPOD_STATUS_OK);
    }
}

#[test]
fn inkscript_execution_abi_rejects_stale_confirmation_authority() {
    // SAFETY: The fixture owns all opaque handles and the callback context on this thread.
    unsafe {
        let (mut owner, mut input_core, mut source, mut program) = compiled_execution_fixture();
        let input_digest = (*input_core).core.document_state_digest().unwrap();
        let mut host = ExecutionHost::new(input_core);
        let (mut plan_task, mut plan) = make_plan(owner, program, &mut host);
        let request = InkpodInkScriptConfirmationRequest {
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
            inkpod_core_inkscript_confirmation_create(owner, plan, &request, &mut confirmation),
            INKPOD_STATUS_OK
        );
        let run_request = InkpodInkScriptRunRequest {
            struct_size: size_of::<InkpodInkScriptRunRequest>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            mode: INKPOD_INKSCRIPT_RUN_INSTALL,
            reserved: 0,
            feature_flags: 0,
            controller_id: 41,
            session_generation: 7,
            maximum_output_bytes: 0,
            host: execution_host_record(&mut host),
        };
        let mut run_task = ptr::null_mut();
        assert_eq!(
            inkpod_core_inkscript_run_task_create(
                owner,
                program,
                &mut plan,
                &mut confirmation,
                &run_request,
                &mut run_task,
            ),
            INKPOD_STATUS_OK
        );
        host.authority_generation += 1;
        assert_eq!(
            inkpod_core_inkscript_run_task_advance(owner, run_task),
            INKPOD_STATUS_OK
        );
        let mut event = InkpodInkScriptTaskEvent {
            struct_size: size_of::<InkpodInkScriptTaskEvent>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_inkscript_run_task_event_take(owner, run_task, &mut event),
            INKPOD_STATUS_OK
        );
        assert_eq!(event.outcome, INKPOD_INKSCRIPT_OUTCOME_FAILED);
        assert_eq!(event.failure, INKPOD_INKSCRIPT_FAILURE_STALE_AUTHORITY);
        assert!(host.installed_bytes.is_empty());
        assert_eq!(
            (*input_core).core.document_state_digest().unwrap(),
            input_digest
        );

        assert_eq!(
            inkpod_core_inkscript_run_task_release(owner, &mut run_task),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_plan_task_release(owner, &mut plan_task),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_inkscript_program_release(owner, &mut program),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_inkscript_source_release(&mut source),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_destroy(&mut input_core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut owner), INKPOD_STATUS_OK);
    }
}
