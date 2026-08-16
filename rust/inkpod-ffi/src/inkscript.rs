use super::*;
use inkpod_core::inkscript::{
    InkScriptExportError, InkScriptExportLimits, InkScriptExportPortability,
    InkScriptFragmentExport, InkScriptRunParameterChoice, InkScriptRunParameterDecision,
    ScriptCompileError, ScriptCompileLimits, StaticScriptProgram, compile_inkscript_with_limits,
    export_inkscript_fragment_with_limits,
};
use inkpod_core::{JournalEntry, JournalEventId};
use inkpod_format::{
    InkScriptDiagnostic, InkScriptDiagnosticSeverity, InkScriptDocumentKind, InkScriptSource,
    InkScriptSourceId, MAX_INKSCRIPT_DIAGNOSTICS, MAX_INKSCRIPT_IDENTIFIER_BYTES,
    MAX_INKSCRIPT_PARAMETERS, MAX_INKSCRIPT_PROGRAM_STATEMENTS, MAX_INKSCRIPT_SOURCE_BYTES,
    parse_inkscript, parse_inkscript_value,
};

struct OwnedDiagnostic {
    severity: u32,
    source_id: u64,
    byte_start: u64,
    byte_end: u64,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    code: Box<[u8]>,
    message: Box<[u8]>,
    path: Box<[u8]>,
    hint: Box<[u8]>,
}

enum OwnedSource {
    Valid(InkScriptSource),
    Invalid(Box<[u8]>),
}

impl OwnedSource {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Valid(source) => source.bytes(),
            Self::Invalid(bytes) => bytes,
        }
    }

    const fn valid(&self) -> Option<&InkScriptSource> {
        match self {
            Self::Valid(source) => Some(source),
            Self::Invalid(_) => None,
        }
    }
}

pub struct InkpodInkScriptSource {
    source: OwnedSource,
    diagnostics: Box<[OwnedDiagnostic]>,
    controller_id: u64,
    session_generation: u64,
    source_id: u64,
    document_kind: u32,
    complete: bool,
    valid: bool,
    has_utf8_bom: bool,
}

pub struct InkpodInkScriptProgram {
    program: StaticScriptProgram,
    owner_thread: ThreadId,
    core_generation: u64,
    controller_id: u64,
    session_generation: u64,
}

pub struct InkpodInkScriptFragment {
    fragment: InkScriptFragmentExport,
    owner_thread: ThreadId,
    core_generation: u64,
    controller_id: u64,
    session_generation: u64,
}

fn owned_diagnostic(value: &InkScriptDiagnostic) -> OwnedDiagnostic {
    let range = value.range();
    let severity = match value.severity() {
        InkScriptDiagnosticSeverity::Error => INKPOD_INKSCRIPT_DIAGNOSTIC_ERROR,
    };
    OwnedDiagnostic {
        severity,
        source_id: value.source_id().get(),
        byte_start: range.span().start(),
        byte_end: range.span().end(),
        start_line: range.start().line(),
        start_column: range.start().column(),
        end_line: range.end().line(),
        end_column: range.end().column(),
        code: value.code().as_str().as_bytes().into(),
        message: value.message().as_bytes().into(),
        path: value.path().join(".").into_bytes().into_boxed_slice(),
        hint: value.hint().unwrap_or("").as_bytes().into(),
    }
}

fn fail_version(name: &str) -> u32 {
    fail(
        INKPOD_STATUS_INCOMPATIBLE_ABI,
        &format!("{name}.version is not exact-current"),
    )
}

fn validate_source(
    source: *const InkpodInkScriptSource,
) -> Result<&'static InkpodInkScriptSource, u32> {
    if source.is_null() || !is_aligned(source) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript source handle is null or misaligned",
        ));
    }
    // SAFETY: Exported contracts require one live handle, externally synchronized against release.
    Ok(unsafe { &*source })
}

fn validate_core(core: *mut InkpodCore) -> Result<&'static mut InkpodCore, u32> {
    if core.is_null() || !is_aligned(core) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript Core handle is null or misaligned",
        ));
    }
    // SAFETY: Exported contracts require a live uniquely owned Core for this call.
    let core = unsafe { &mut *core };
    let status = validate_core_thread(core);
    if status != INKPOD_STATUS_OK {
        return Err(status);
    }
    // SAFETY: No reference escapes the exported call.
    Ok(unsafe { &mut *(core as *mut InkpodCore) })
}

fn validate_program<'a>(
    core: &InkpodCore,
    program: *const InkpodInkScriptProgram,
) -> Result<&'a InkpodInkScriptProgram, u32> {
    if program.is_null() || !is_aligned(program) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript program handle is null or misaligned",
        ));
    }
    // SAFETY: Exported contracts require one live handle synchronized against release.
    let program = unsafe { &*program };
    if program.owner_thread != thread::current().id() {
        return Err(fail(
            INKPOD_STATUS_WRONG_THREAD,
            "InkScript program must be used on its compile thread",
        ));
    }
    if program.core_generation != core.objects.generation() {
        return Err(fail(
            INKPOD_STATUS_INVALID_STATE,
            "InkScript program belongs to a stale Core generation",
        ));
    }
    Ok(program)
}

fn validate_fragment<'a>(
    core: &InkpodCore,
    fragment: *const InkpodInkScriptFragment,
) -> Result<&'a InkpodInkScriptFragment, u32> {
    if fragment.is_null() || !is_aligned(fragment) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript fragment handle is null or misaligned",
        ));
    }
    // SAFETY: Exported contracts require one live handle synchronized against release.
    let fragment = unsafe { &*fragment };
    if fragment.owner_thread != thread::current().id() {
        return Err(fail(
            INKPOD_STATUS_WRONG_THREAD,
            "InkScript fragment must be used on its export thread",
        ));
    }
    if fragment.core_generation != core.objects.generation() {
        return Err(fail(
            INKPOD_STATUS_INVALID_STATE,
            "InkScript fragment belongs to a stale Core generation",
        ));
    }
    Ok(fragment)
}

fn validate_utf8_buffer(
    output: *mut InkpodInkScriptUtf8Buffer,
) -> Result<&'static mut InkpodInkScriptUtf8Buffer, u32> {
    // SAFETY: Exported contracts require a readable size prefix.
    unsafe { validate_struct(output.cast_const(), "InkpodInkScriptUtf8Buffer")? };
    // SAFETY: The complete record is caller-owned writable storage for this call.
    let output = unsafe { &mut *output };
    if output.version != INKPOD_INKSCRIPT_RECORD_VERSION {
        return Err(fail_version("InkpodInkScriptUtf8Buffer"));
    }
    if output.feature_flags != INKPOD_FEATURE_NONE {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "InkpodInkScriptUtf8Buffer has unsupported feature flags",
        ));
    }
    Ok(output)
}

fn copy_utf8(value: &[u8], output: *mut InkpodInkScriptUtf8Buffer) -> u32 {
    let output = match validate_utf8_buffer(output) {
        Ok(output) => output,
        Err(status) => return status,
    };
    let required = value.len() as u64;
    output.written_bytes = 0;
    output.required_bytes = required;
    if output.capacity_bytes < required || (required != 0 && output.bytes.is_null()) {
        return INKPOD_STATUS_BUFFER_TOO_SMALL;
    }
    let Ok(capacity) = usize::try_from(output.capacity_bytes) else {
        return fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript UTF-8 buffer capacity is not representable",
        );
    };
    if capacity != 0 && output.bytes.is_null() {
        return fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript UTF-8 buffer is null with nonzero capacity",
        );
    }
    if required != 0 {
        // SAFETY: The caller advertises a non-overlapping writable span at least `required` long.
        unsafe { ptr::copy_nonoverlapping(value.as_ptr(), output.bytes, value.len()) };
    }
    output.written_bytes = required;
    INKPOD_STATUS_OK
}

fn event_id(entry: &JournalEntry) -> JournalEventId {
    match entry {
        JournalEntry::Commit(commit) => commit.event_id(),
        JournalEntry::HistoryMove(movement) => movement.event_id(),
        JournalEntry::BranchCut(cut) => cut.event_id(),
    }
}

fn map_compile_error(error: ScriptCompileError) -> u32 {
    match error {
        ScriptCompileError::ParameterCancelled => fail(
            INKPOD_STATUS_CANCELLED,
            "InkScript parameter resolution was cancelled",
        ),
        ScriptCompileError::ResourceLimit => fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript compile resource limit was exceeded",
        ),
        ScriptCompileError::Syntax => fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript source contains syntax diagnostics",
        ),
        ScriptCompileError::Semantic(code) => fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("InkScript semantic diagnostic: {}", code.as_str()),
        ),
        ScriptCompileError::Envelope(code) => fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("InkScript envelope diagnostic: {}", code.as_str()),
        ),
        ScriptCompileError::Type(value) => fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("InkScript type diagnostic: {}", value.code().as_str()),
        ),
        ScriptCompileError::Freeze(code) => fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("InkScript freeze diagnostic: {}", code.as_str()),
        ),
        ScriptCompileError::InvalidPathIntent => fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript path intent is invalid",
        ),
        ScriptCompileError::Asset(_) => fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript asset declaration is invalid",
        ),
        ScriptCompileError::Catalog(_) => fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript catalog validation failed",
        ),
    }
}

fn map_export_error(error: InkScriptExportError) -> u32 {
    match error {
        InkScriptExportError::Cancelled => fail(
            INKPOD_STATUS_CANCELLED,
            "InkScript fragment export was cancelled",
        ),
        InkScriptExportError::EmptySelection => fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript fragment selection is empty",
        ),
        InkScriptExportError::NotACommit(_) => fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript fragment selection includes a non-Commit event",
        ),
        InkScriptExportError::NonLinearSelection => fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript fragment selection is not one linear ancestor chain",
        ),
        InkScriptExportError::MissingRuntimeInvocation => fail(
            INKPOD_STATUS_INVALID_STATE,
            "InkScript fragment source lacks a typed runtime invocation",
        ),
        InkScriptExportError::UnsupportedPrimitive(_) => fail(
            INKPOD_STATUS_UNSUPPORTED,
            "InkScript fragment source contains an unsupported primitive",
        ),
        InkScriptExportError::InvalidSource => fail(
            INKPOD_STATUS_INVALID_STATE,
            "InkScript fragment source journal is inconsistent",
        ),
        InkScriptExportError::ResourceLimit => fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript fragment export resource limit was exceeded",
        ),
    }
}

/// Parses and owns one bounded source without retaining caller memory.
///
/// # Safety
/// `input` is a complete readable record and its byte span is readable for the call. `out_source`
/// is unique aligned owner storage containing null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_inkscript_source_parse(
    input: *const InkpodInkScriptSourceInput,
    out_source: *mut *mut InkpodInkScriptSource,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_source.is_null() || !is_aligned(out_source) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript source owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller supplies readable unique owner storage.
        if !unsafe { out_source.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript source output already owns a handle",
            );
        }
        // SAFETY: Public records expose a readable size prefix.
        if let Err(status) = unsafe { validate_struct(input, "InkpodInkScriptSourceInput") } {
            return status;
        }
        // SAFETY: A full readable record was validated above.
        let input = unsafe { &*input };
        if input.version != INKPOD_INKSCRIPT_RECORD_VERSION {
            return fail_version("InkpodInkScriptSourceInput");
        }
        if input.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "InkpodInkScriptSourceInput has unsupported feature flags",
            );
        }
        if input.controller_id == 0 || input.session_generation == 0 || input.source_id == 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript source routing identities must be nonzero",
            );
        }
        if input.source_bytes > MAX_INKSCRIPT_SOURCE_BYTES as u64 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript source exceeds the exact-current byte limit",
            );
        }
        let Ok(source_bytes) = usize::try_from(input.source_bytes) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript source byte count is not representable",
            );
        };
        if source_bytes != 0 && input.source_utf8.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript source bytes are null with nonzero length",
            );
        }
        // SAFETY: The caller promises the byte span is readable for this call; zero permits null.
        let bytes = if source_bytes == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(input.source_utf8, source_bytes) }
        };
        let source_id = InkScriptSourceId::new(input.source_id);
        let (source, diagnostics, document_kind, complete, valid, has_utf8_bom) =
            match InkScriptSource::new(source_id, bytes) {
                Ok(source) => {
                    let parsed = parse_inkscript(&source);
                    let diagnostics = parsed
                        .diagnostics()
                        .iter()
                        .map(owned_diagnostic)
                        .collect::<Vec<_>>()
                        .into_boxed_slice();
                    let document_kind = match parsed.cst().document_kind() {
                        InkScriptDocumentKind::Unknown => INKPOD_INKSCRIPT_DOCUMENT_UNKNOWN,
                        InkScriptDocumentKind::File => INKPOD_INKSCRIPT_DOCUMENT_FILE,
                        InkScriptDocumentKind::Fragment => INKPOD_INKSCRIPT_DOCUMENT_FRAGMENT,
                    };
                    let complete = parsed.is_complete();
                    let valid = parsed.is_valid();
                    let has_utf8_bom = source.has_utf8_bom();
                    (
                        OwnedSource::Valid(source),
                        diagnostics,
                        document_kind,
                        complete,
                        valid,
                        has_utf8_bom,
                    )
                }
                Err(diagnostic) => (
                    OwnedSource::Invalid(bytes.to_vec().into_boxed_slice()),
                    vec![owned_diagnostic(&diagnostic)].into_boxed_slice(),
                    INKPOD_INKSCRIPT_DOCUMENT_UNKNOWN,
                    false,
                    false,
                    bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
                ),
            };
        if diagnostics.len() > MAX_INKSCRIPT_DIAGNOSTICS {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript parser published too many diagnostics",
            );
        }
        let handle = Box::new(InkpodInkScriptSource {
            source,
            diagnostics,
            controller_id: input.controller_id,
            session_generation: input.session_generation,
            source_id: input.source_id,
            document_kind,
            complete,
            valid,
            has_utf8_bom,
        });
        // SAFETY: A unique Rust owner is published only after complete construction.
        unsafe { out_source.write(Box::into_raw(handle)) };
        INKPOD_STATUS_OK
    })
}

/// Copies fixed-width source metadata without exposing parser nodes.
///
/// # Safety
/// `source` is live and synchronized against release. `out_summary` is a complete writable record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_inkscript_source_summary(
    source: *const InkpodInkScriptSource,
    out_summary: *mut InkpodInkScriptSourceSummary,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let source = match validate_source(source) {
            Ok(source) => source,
            Err(status) => return status,
        };
        // SAFETY: Output records expose a readable size prefix before write.
        let struct_size = match unsafe {
            validate_struct(out_summary.cast_const(), "InkpodInkScriptSourceSummary")
        } {
            Ok(size) => size,
            Err(status) => return status,
        };
        let mut flags = 0;
        if source.complete {
            flags |= INKPOD_INKSCRIPT_SOURCE_COMPLETE;
        }
        if source.valid {
            flags |= INKPOD_INKSCRIPT_SOURCE_VALID;
        }
        if source.has_utf8_bom {
            flags |= INKPOD_INKSCRIPT_SOURCE_UTF8_BOM;
        }
        let output = InkpodInkScriptSourceSummary {
            struct_size,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            feature_flags: INKPOD_FEATURE_NONE,
            controller_id: source.controller_id,
            session_generation: source.session_generation,
            source_id: source.source_id,
            source_bytes: source.source.bytes().len() as u64,
            diagnostic_count: source.diagnostics.len() as u64,
            document_kind: source.document_kind,
            reserved: 0,
            flags,
        };
        // SAFETY: The full output record is writable and non-overlapping by contract.
        unsafe { out_summary.write(output) };
        INKPOD_STATUS_OK
    })
}

/// Copies the original source bytes with a two-stage caller buffer contract.
///
/// # Safety
/// `source` is live and synchronized against release; `output` and any advertised byte span are
/// writable for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_inkscript_source_text_copy(
    source: *const InkpodInkScriptSource,
    output: *mut InkpodInkScriptUtf8Buffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let source = match validate_source(source) {
            Ok(source) => source,
            Err(status) => return status,
        };
        copy_utf8(source.source.bytes(), output)
    })
}

fn packed_field(bytes: &[u8], packed: &mut Vec<u8>) -> Result<(u64, u64), u32> {
    let offset = packed.len() as u64;
    let next = packed.len().checked_add(bytes.len()).ok_or_else(|| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript diagnostic UTF-8 byte count overflows",
        )
    })?;
    if next > MAX_INKSCRIPT_SOURCE_BYTES {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript diagnostic UTF-8 byte limit was exceeded",
        ));
    }
    packed.extend_from_slice(bytes);
    Ok((offset, bytes.len() as u64))
}

/// Copies a diagnostic range and all variable UTF-8 fields in two batched spans.
///
/// # Safety
/// Handles are live. `output` is complete writable storage. On the copy call every strided record
/// is initialized with the exact record version and every advertised output span is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_inkscript_source_diagnostics_copy(
    source: *const InkpodInkScriptSource,
    output: *mut InkpodInkScriptDiagnosticBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let source = match validate_source(source) {
            Ok(source) => source,
            Err(status) => return status,
        };
        // SAFETY: The public buffer exposes a readable size prefix.
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodInkScriptDiagnosticBuffer") }
        {
            return status;
        }
        // SAFETY: Complete caller-owned buffer record validated above.
        let output = unsafe { &mut *output };
        if output.version != INKPOD_INKSCRIPT_RECORD_VERSION {
            return fail_version("InkpodInkScriptDiagnosticBuffer");
        }
        if output.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "InkpodInkScriptDiagnosticBuffer has unsupported feature flags",
            );
        }
        let Ok(first) = usize::try_from(output.first_diagnostic) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript first diagnostic index is not representable",
            );
        };
        if first > source.diagnostics.len() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript first diagnostic index is outside bounds",
            );
        }
        let selected = &source.diagnostics[first..];
        let mut packed = Vec::new();
        let mut records = Vec::with_capacity(selected.len());
        for diagnostic in selected {
            let (code_offset, code_bytes) = match packed_field(&diagnostic.code, &mut packed) {
                Ok(value) => value,
                Err(status) => return status,
            };
            let (message_offset, message_bytes) =
                match packed_field(&diagnostic.message, &mut packed) {
                    Ok(value) => value,
                    Err(status) => return status,
                };
            let (path_offset, path_bytes) = match packed_field(&diagnostic.path, &mut packed) {
                Ok(value) => value,
                Err(status) => return status,
            };
            let (hint_offset, hint_bytes) = match packed_field(&diagnostic.hint, &mut packed) {
                Ok(value) => value,
                Err(status) => return status,
            };
            records.push(InkpodInkScriptDiagnostic {
                struct_size: size_of::<InkpodInkScriptDiagnostic>() as u32,
                version: INKPOD_INKSCRIPT_RECORD_VERSION,
                severity: diagnostic.severity,
                reserved: 0,
                feature_flags: INKPOD_FEATURE_NONE,
                source_id: diagnostic.source_id,
                byte_start: diagnostic.byte_start,
                byte_end: diagnostic.byte_end,
                start_line: diagnostic.start_line,
                start_column: diagnostic.start_column,
                end_line: diagnostic.end_line,
                end_column: diagnostic.end_column,
                code_offset,
                code_bytes,
                message_offset,
                message_bytes,
                path_offset,
                path_bytes,
                hint_offset,
                hint_bytes,
            });
        }
        output.records_written = 0;
        output.utf8_written_bytes = 0;
        output.required_records = records.len() as u64;
        output.required_utf8_bytes = packed.len() as u64;
        let enough = output.record_capacity >= records.len() as u64
            && output.utf8_capacity_bytes >= packed.len() as u64
            && (records.is_empty() || !output.records.is_null())
            && (packed.is_empty() || !output.utf8.is_null());
        if !enough {
            return INKPOD_STATUS_BUFFER_TOO_SMALL;
        }
        if records.is_empty() {
            if output.record_capacity != 0 && output.records.is_null() {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "InkScript diagnostic record buffer is null with nonzero capacity",
                );
            }
        } else if output.record_stride_bytes < size_of::<InkpodInkScriptDiagnostic>() as u64 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript diagnostic record stride is too small",
            );
        }
        let Ok(stride) = usize::try_from(output.record_stride_bytes) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript diagnostic record stride is not representable",
            );
        };
        for index in 0..records.len() {
            let Some(offset) = index.checked_mul(stride) else {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "InkScript diagnostic record span overflows",
                );
            };
            // SAFETY: The caller advertises a writable strided array of `record_capacity` entries.
            let destination = unsafe { output.records.cast::<u8>().add(offset) }
                .cast::<InkpodInkScriptDiagnostic>();
            if !is_aligned(destination) {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "InkScript diagnostic record is misaligned",
                );
            }
            // SAFETY: Every public record must expose a readable size prefix.
            if let Err(status) =
                unsafe { validate_struct(destination.cast_const(), "InkpodInkScriptDiagnostic") }
            {
                return status;
            }
            // SAFETY: A complete readable output record was validated above.
            let initialized = unsafe { &*destination };
            if initialized.version != INKPOD_INKSCRIPT_RECORD_VERSION {
                return fail_version("InkpodInkScriptDiagnostic");
            }
            if initialized.feature_flags != INKPOD_FEATURE_NONE {
                return fail(
                    INKPOD_STATUS_UNSUPPORTED,
                    "InkpodInkScriptDiagnostic has unsupported feature flags",
                );
            }
        }
        if !packed.is_empty() {
            // SAFETY: Capacity validation above proves the packed byte output span is writable.
            unsafe { ptr::copy_nonoverlapping(packed.as_ptr(), output.utf8, packed.len()) };
        }
        for (index, record) in records.into_iter().enumerate() {
            let offset = index * stride;
            // SAFETY: Each destination was fully validated before any output was changed.
            let destination = unsafe { output.records.cast::<u8>().add(offset) }
                .cast::<InkpodInkScriptDiagnostic>();
            // SAFETY: The complete non-overlapping output record is writable.
            unsafe { destination.write(record) };
        }
        output.records_written = selected.len() as u64;
        output.utf8_written_bytes = packed.len() as u64;
        INKPOD_STATUS_OK
    })
}

/// Releases an immutable source and nulls unique caller owner storage.
///
/// # Safety
/// `source` contains null or one unique live handle and is synchronized against all access.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_inkscript_source_release(
    source: *mut *mut InkpodInkScriptSource,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if source.is_null() || !is_aligned(source) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript source owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller provides readable/writable unique owner storage.
        let handle = unsafe { source.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript source handle is misaligned",
            );
        }
        // SAFETY: Null the owner before consuming the unique Rust allocation exactly once.
        unsafe { source.write(ptr::null_mut()) };
        // SAFETY: The caller transfers the unique live owner to this release call.
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

unsafe fn parameter_choices(
    request: &InkpodInkScriptCompileRequest,
    source_id: u64,
) -> Result<Vec<InkScriptRunParameterChoice>, u32> {
    if request.parameter_choice_count > MAX_INKSCRIPT_PARAMETERS as u64 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript parameter choice count exceeds its bound",
        ));
    }
    if request.parameter_choice_count == 0 {
        if !request.parameter_choices.is_null() || request.parameter_choice_stride_bytes != 0 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "empty InkScript parameter choice span must be null with zero stride",
            ));
        }
        return Ok(Vec::new());
    }
    if request.parameter_choices.is_null()
        || !is_aligned(request.parameter_choices)
        || request.parameter_choice_stride_bytes
            < size_of::<InkpodInkScriptParameterChoice>() as u64
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript parameter choice span is null, misaligned, or has a short stride",
        ));
    }
    let count = usize::try_from(request.parameter_choice_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript parameter choice count is not representable",
        )
    })?;
    let stride = usize::try_from(request.parameter_choice_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript parameter choice stride is not representable",
        )
    })?;
    let mut choices = Vec::with_capacity(count);
    let mut total_value_bytes = 0_usize;
    for index in 0..count {
        let offset = index.checked_mul(stride).ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript parameter choice span overflows",
            )
        })?;
        // SAFETY: Caller advertises a readable strided record span for the duration of the call.
        let choice = unsafe { request.parameter_choices.cast::<u8>().add(offset) }
            .cast::<InkpodInkScriptParameterChoice>();
        // SAFETY: Each record exposes a readable size prefix.
        unsafe { validate_struct(choice, "InkpodInkScriptParameterChoice")? };
        // SAFETY: The complete record is readable after size validation.
        let choice = unsafe { &*choice };
        if choice.version != INKPOD_INKSCRIPT_RECORD_VERSION {
            return Err(fail_version("InkpodInkScriptParameterChoice"));
        }
        if choice.feature_flags != 0 || choice.reserved != 0 {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "InkpodInkScriptParameterChoice has unsupported flags or reserved values",
            ));
        }
        if choice.name_bytes == 0 || choice.name_bytes > MAX_INKSCRIPT_IDENTIFIER_BYTES as u64 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript parameter name length is outside bounds",
            ));
        }
        let name_bytes = usize::try_from(choice.name_bytes).map_err(|_| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript parameter name length is not representable",
            )
        })?;
        if choice.name_utf8.is_null() {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript parameter name is null",
            ));
        }
        // SAFETY: Caller advertises a readable name byte span for this call.
        let name = unsafe { slice::from_raw_parts(choice.name_utf8, name_bytes) };
        let name = std::str::from_utf8(name).map_err(|_| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript parameter name is not UTF-8",
            )
        })?;
        match choice.kind {
            INKPOD_INKSCRIPT_PARAMETER_ACCEPT_DEFAULT => {
                if !choice.value_utf8.is_null() || choice.value_bytes != 0 {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "accepted-default InkScript parameter has an unexpected value span",
                    ));
                }
                choices.push(InkScriptRunParameterChoice::AcceptDefault {
                    name: name.to_owned(),
                });
            }
            INKPOD_INKSCRIPT_PARAMETER_OVERRIDE => {
                if choice.value_utf8.is_null()
                    || choice.value_bytes == 0
                    || choice.value_bytes > MAX_INKSCRIPT_SOURCE_BYTES as u64
                {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "InkScript parameter override value span is null or outside bounds",
                    ));
                }
                let value_bytes = usize::try_from(choice.value_bytes).map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "InkScript parameter override value length is not representable",
                    )
                })?;
                total_value_bytes =
                    total_value_bytes.checked_add(value_bytes).ok_or_else(|| {
                        fail(
                            INKPOD_STATUS_INVALID_ARGUMENT,
                            "InkScript parameter override byte total overflows",
                        )
                    })?;
                if total_value_bytes > MAX_INKSCRIPT_SOURCE_BYTES {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "InkScript parameter override byte total exceeds its bound",
                    ));
                }
                // SAFETY: Caller advertises a readable value byte span for this call.
                let bytes = unsafe { slice::from_raw_parts(choice.value_utf8, value_bytes) };
                let value_source = InkScriptSource::new(InkScriptSourceId::new(source_id), bytes)
                    .map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "InkScript parameter override is not valid UTF-8",
                    )
                })?;
                let value = parse_inkscript_value(&value_source).map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "InkScript parameter override is not one exact value",
                    )
                })?;
                choices.push(InkScriptRunParameterChoice::Override {
                    name: name.to_owned(),
                    value,
                });
            }
            _ => {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "InkpodInkScriptParameterChoice.kind is unknown",
                ));
            }
        }
    }
    Ok(choices)
}

/// Compiles one parsed source into an immutable generation-bound program.
///
/// # Safety
/// Core/source/request are live and externally synchronized. All request spans are readable for
/// the call and `out_program` is unique owner storage containing null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_compile(
    core: *mut InkpodCore,
    source: *const InkpodInkScriptSource,
    request: *const InkpodInkScriptCompileRequest,
    out_program: *mut *mut InkpodInkScriptProgram,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let source = match validate_source(source) {
            Ok(source) => source,
            Err(status) => return status,
        };
        if out_program.is_null() || !is_aligned(out_program) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript program owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller supplies readable unique owner storage.
        if !unsafe { out_program.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript program output already owns a handle",
            );
        }
        // SAFETY: Public request exposes a readable size prefix.
        if let Err(status) = unsafe { validate_struct(request, "InkpodInkScriptCompileRequest") } {
            return status;
        }
        // SAFETY: Complete request was validated above.
        let request = unsafe { &*request };
        if request.version != INKPOD_INKSCRIPT_RECORD_VERSION {
            return fail_version("InkpodInkScriptCompileRequest");
        }
        if request.feature_flags != 0
            || request.flags & !INKPOD_INKSCRIPT_COMPILE_CANCEL != 0
            || request.reserved != 0
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "InkpodInkScriptCompileRequest has unsupported flags or reserved values",
            );
        }
        if request.controller_id != source.controller_id
            || request.session_generation != source.session_generation
        {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript source belongs to a stale controller generation",
            );
        }
        if request.flags & INKPOD_INKSCRIPT_COMPILE_CANCEL != 0 {
            return fail(
                INKPOD_STATUS_CANCELLED,
                "InkScript compile was cancelled before publication",
            );
        }
        let source_value = match source.source.valid() {
            Some(value) if source.valid => value,
            Some(_) | None => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "InkScript source is not structurally valid",
                );
            }
        };
        // SAFETY: Request parameter records and names are borrowed for this call only.
        let choices = match unsafe { parameter_choices(request, source.source_id) } {
            Ok(choices) => choices,
            Err(status) => return status,
        };
        let mut limits = ScriptCompileLimits::exact_current();
        if request.max_invocations != 0 {
            limits = limits.with_invocations(request.max_invocations);
        }
        let program = match compile_inkscript_with_limits(
            source_value,
            InkScriptRunParameterDecision::Resolve(choices),
            limits,
        ) {
            Ok(program) => program,
            Err(error) => return map_compile_error(error),
        };
        let handle = Box::new(InkpodInkScriptProgram {
            program,
            owner_thread: thread::current().id(),
            core_generation: core.objects.generation(),
            controller_id: request.controller_id,
            session_generation: request.session_generation,
        });
        // SAFETY: A unique Rust owner is published only after successful compilation.
        unsafe { out_program.write(Box::into_raw(handle)) };
        INKPOD_STATUS_OK
    })
}

/// Copies immutable compile digests and checked aggregate budgets.
///
/// # Safety
/// Core/program are live on their owner thread and `out_summary` is complete writable storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_program_summary(
    core: *mut InkpodCore,
    program: *const InkpodInkScriptProgram,
    out_summary: *mut InkpodInkScriptProgramSummary,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let program = match validate_program(core, program) {
            Ok(program) => program,
            Err(status) => return status,
        };
        // SAFETY: Output records expose a readable size prefix.
        let struct_size = match unsafe {
            validate_struct(out_summary.cast_const(), "InkpodInkScriptProgramSummary")
        } {
            Ok(value) => value,
            Err(status) => return status,
        };
        let budget = program.program.budget();
        let output = InkpodInkScriptProgramSummary {
            struct_size,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            feature_flags: INKPOD_FEATURE_NONE,
            controller_id: program.controller_id,
            session_generation: program.session_generation,
            core_generation: program.core_generation,
            static_compile_digest: *program.program.static_compile_digest(),
            path_intent_digest: *program.program.path_intent_digest(),
            max_invocations: budget.max_invocations(),
            max_output_ids: budget.max_output_ids(),
            max_asset_bytes: budget.max_asset_bytes(),
            max_work_units: budget.max_work_units(),
            max_output_growth: budget.max_output_growth(),
            path_intent_count: program.program.path_intents().len() as u64,
        };
        // SAFETY: The full non-overlapping output record is writable.
        unsafe { out_summary.write(output) };
        INKPOD_STATUS_OK
    })
}

/// Releases a program before its parent Core and nulls unique owner storage.
///
/// # Safety
/// Core is the live generation used to compile the program; `program` contains null or one unique
/// live handle. Both are used on the compile thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_program_release(
    core: *mut InkpodCore,
    program: *mut *mut InkpodInkScriptProgram,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if program.is_null() || !is_aligned(program) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript program owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller supplies readable/writable unique owner storage.
        let handle = unsafe { program.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if let Err(status) = validate_program(core, handle) {
            return status;
        }
        // SAFETY: Null before consuming the unique Rust allocation once.
        unsafe { program.write(ptr::null_mut()) };
        // SAFETY: Caller transfers the unique live owner.
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

unsafe fn selected_events(
    core: &InkpodCore,
    request: &InkpodInkScriptExportRequest,
) -> Result<Vec<JournalEventId>, u32> {
    if request.event_count == 0 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript fragment event selection is empty",
        ));
    }
    if request.event_count > MAX_INKSCRIPT_PROGRAM_STATEMENTS as u64 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript fragment event count exceeds its bound",
        ));
    }
    if request.events.is_null()
        || !is_aligned(request.events)
        || request.event_stride_bytes < size_of::<InkpodInkScriptJournalEvent>() as u64
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript fragment event span is null, misaligned, or has a short stride",
        ));
    }
    let count = usize::try_from(request.event_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript fragment event count is not representable",
        )
    })?;
    let stride = usize::try_from(request.event_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript fragment event stride is not representable",
        )
    })?;
    let journal = core.core.journal_entries();
    let mut selected = Vec::with_capacity(count);
    for index in 0..count {
        let offset = index.checked_mul(stride).ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript fragment event span overflows",
            )
        })?;
        // SAFETY: Caller advertises a readable strided event span for this call.
        let record = unsafe { request.events.cast::<u8>().add(offset) }
            .cast::<InkpodInkScriptJournalEvent>();
        // SAFETY: Every record exposes a readable size prefix.
        unsafe { validate_struct(record, "InkpodInkScriptJournalEvent")? };
        // SAFETY: Full record is readable after validation.
        let record = unsafe { &*record };
        if record.version != INKPOD_INKSCRIPT_RECORD_VERSION {
            return Err(fail_version("InkpodInkScriptJournalEvent"));
        }
        if record.reserved != 0 || record.event_id == 0 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkpodInkScriptJournalEvent has invalid reserved or zero values",
            ));
        }
        let Some(found) = journal
            .iter()
            .map(event_id)
            .find(|value| value.get() == record.event_id)
        else {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript fragment event ID is not present in this Core journal",
            ));
        };
        selected.push(found);
    }
    Ok(selected)
}

/// Exports selected canonical Commit events into one immutable exact-current fragment.
///
/// # Safety
/// Core/request/event records are live on the Core owner thread and `out_fragment` is unique null
/// owner storage. Event spans are borrowed only for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_fragment_export(
    core: *mut InkpodCore,
    request: *const InkpodInkScriptExportRequest,
    out_fragment: *mut *mut InkpodInkScriptFragment,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if out_fragment.is_null() || !is_aligned(out_fragment) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript fragment owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller supplies readable unique owner storage.
        if !unsafe { out_fragment.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript fragment output already owns a handle",
            );
        }
        // SAFETY: Public request exposes a readable size prefix.
        if let Err(status) = unsafe { validate_struct(request, "InkpodInkScriptExportRequest") } {
            return status;
        }
        // SAFETY: Complete request is readable after validation.
        let request = unsafe { &*request };
        if request.version != INKPOD_INKSCRIPT_RECORD_VERSION {
            return fail_version("InkpodInkScriptExportRequest");
        }
        if request.feature_flags != 0
            || request.flags & !INKPOD_INKSCRIPT_EXPORT_CANCEL != 0
            || request.reserved != 0
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "InkpodInkScriptExportRequest has unsupported flags or reserved values",
            );
        }
        if request.controller_id == 0 || request.session_generation == 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript fragment routing identities must be nonzero",
            );
        }
        if request.flags & INKPOD_INKSCRIPT_EXPORT_CANCEL != 0 {
            return fail(
                INKPOD_STATUS_CANCELLED,
                "InkScript fragment export was cancelled before publication",
            );
        }
        // SAFETY: Request event records are borrowed for this call only.
        let selected = match unsafe { selected_events(core, request) } {
            Ok(selected) => selected,
            Err(status) => return status,
        };
        let mut limits = InkScriptExportLimits::exact_current();
        if request.max_commits != 0 {
            let Ok(value) = usize::try_from(request.max_commits) else {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "InkScript fragment Commit limit is not representable",
                );
            };
            limits = limits.with_commits(value);
        }
        if request.max_source_bytes != 0 {
            let Ok(value) = usize::try_from(request.max_source_bytes) else {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "InkScript fragment source limit is not representable",
                );
            };
            limits = limits.with_source_bytes(value);
        }
        if request.max_inline_asset_bytes != 0 {
            limits = limits.with_asset_bytes(request.max_inline_asset_bytes);
        }
        let mut never_cancel = || false;
        let fragment = match export_inkscript_fragment_with_limits(
            &core.core,
            &selected,
            limits,
            &mut never_cancel,
        ) {
            Ok(fragment) => fragment,
            Err(error) => return map_export_error(error),
        };
        let handle = Box::new(InkpodInkScriptFragment {
            fragment,
            owner_thread: thread::current().id(),
            core_generation: core.objects.generation(),
            controller_id: request.controller_id,
            session_generation: request.session_generation,
        });
        // SAFETY: A unique Rust owner is published after complete successful export.
        unsafe { out_fragment.write(Box::into_raw(handle)) };
        INKPOD_STATUS_OK
    })
}

/// Copies immutable fragment linkage, portability, and resource metadata.
///
/// # Safety
/// Core/fragment are live on the export thread and `out_summary` is complete writable storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_fragment_summary(
    core: *mut InkpodCore,
    fragment: *const InkpodInkScriptFragment,
    out_summary: *mut InkpodInkScriptFragmentSummary,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let fragment = match validate_fragment(core, fragment) {
            Ok(fragment) => fragment,
            Err(status) => return status,
        };
        // SAFETY: Output exposes a readable size prefix.
        let struct_size = match unsafe {
            validate_struct(out_summary.cast_const(), "InkpodInkScriptFragmentSummary")
        } {
            Ok(value) => value,
            Err(status) => return status,
        };
        let portability = match fragment.fragment.portability() {
            InkScriptExportPortability::Portable => INKPOD_INKSCRIPT_PORTABLE,
            InkScriptExportPortability::RequiresBinding => INKPOD_INKSCRIPT_REQUIRES_BINDING,
            InkScriptExportPortability::StrictSourceOnly => INKPOD_INKSCRIPT_STRICT_SOURCE_ONLY,
        };
        let output = InkpodInkScriptFragmentSummary {
            struct_size,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            feature_flags: INKPOD_FEATURE_NONE,
            controller_id: fragment.controller_id,
            session_generation: fragment.session_generation,
            core_generation: fragment.core_generation,
            base_state_id: fragment.fragment.base_state_id().get(),
            final_state_id: fragment.fragment.final_state_id().get(),
            commit_count: fragment.fragment.commit_count() as u64,
            portability,
            reserved: 0,
            required_precondition_count: fragment.fragment.required_preconditions().len() as u64,
            text_bytes: fragment.fragment.text().len() as u64,
        };
        // SAFETY: Complete non-overlapping output record is writable.
        unsafe { out_summary.write(output) };
        INKPOD_STATUS_OK
    })
}

/// Copies canonical BOM-free fragment text using a two-stage caller buffer.
///
/// # Safety
/// Core/fragment are live on the export thread; `output` and its advertised span are writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_fragment_text_copy(
    core: *mut InkpodCore,
    fragment: *const InkpodInkScriptFragment,
    output: *mut InkpodInkScriptUtf8Buffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let fragment = match validate_fragment(core, fragment) {
            Ok(fragment) => fragment,
            Err(status) => return status,
        };
        copy_utf8(fragment.fragment.text().as_bytes(), output)
    })
}

/// Releases a fragment before its parent Core and nulls unique owner storage.
///
/// # Safety
/// Core is the live generation used for export; `fragment` contains null or one unique live
/// handle. Both are used on the export thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_fragment_release(
    core: *mut InkpodCore,
    fragment: *mut *mut InkpodInkScriptFragment,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if fragment.is_null() || !is_aligned(fragment) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript fragment owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller supplies readable/writable unique owner storage.
        let handle = unsafe { fragment.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if let Err(status) = validate_fragment(core, handle) {
            return status;
        }
        // SAFETY: Null before consuming the unique Rust allocation once.
        unsafe { fragment.write(ptr::null_mut()) };
        // SAFETY: Caller transfers the unique live owner.
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}
