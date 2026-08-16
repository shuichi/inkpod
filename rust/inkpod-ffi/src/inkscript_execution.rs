use super::*;
use core::ffi::c_void;
use inkpod_core::inkscript::abi_bridge::*;
use inkpod_core::inkscript::{ScriptPathIntentSubject, StaticScriptProgram};
use inkpod_format::{InkScriptPathIntentAccess, MAX_INKSCRIPT_INPUTS, MAX_INKSCRIPT_STRING_BYTES};
use std::cell::UnsafeCell;
use std::mem::size_of;

const MAX_HOST_RECORDS: u64 = MAX_INKSCRIPT_INPUTS as u64;

#[derive(Clone, Copy)]
struct HostBridge {
    context: *mut c_void,
    call: unsafe extern "C" fn(
        *mut c_void,
        *const InkpodInkScriptHostRequest,
        *mut InkpodInkScriptHostResponse,
    ) -> u32,
}

// SAFETY: The ABI contract requires the callback context to remain live and externally
// synchronized until every task using it has been released. Only the owner thread invokes it.
unsafe impl Send for HostBridge {}

impl HostBridge {
    fn from_record(record: &InkpodInkScriptHostAdapter) -> Result<Self, u32> {
        if record.struct_size < size_of::<InkpodInkScriptHostAdapter>() as u32 {
            return Err(fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodInkScriptHostAdapter.struct_size is too small",
            ));
        }
        if record.version != INKPOD_INKSCRIPT_RECORD_VERSION {
            return Err(fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodInkScriptHostAdapter.version is not exact-current",
            ));
        }
        if record.feature_flags != INKPOD_FEATURE_NONE {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "InkpodInkScriptHostAdapter has unsupported feature flags",
            ));
        }
        let Some(call) = record.call else {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkpodInkScriptHostAdapter.call is null",
            ));
        };
        Ok(Self {
            context: record.context,
            call,
        })
    }

    fn invoke(
        self,
        operation: u32,
        populate: impl FnOnce(&mut InkpodInkScriptHostRequest),
    ) -> Result<InkpodInkScriptHostResponse, u32> {
        let mut request = host_request(operation);
        populate(&mut request);
        let mut response = host_response();
        // SAFETY: The callback and context lifetime are guaranteed by the task's host contract.
        // Request and response storage remain live and non-overlapping for the call.
        let status = unsafe { (self.call)(self.context, &request, &mut response) };
        if status != INKPOD_STATUS_OK {
            return Err(status);
        }
        if response.struct_size < size_of::<InkpodInkScriptHostResponse>() as u32
            || response.version != INKPOD_INKSCRIPT_RECORD_VERSION
        {
            return Err(fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkScript host returned a noncurrent response record",
            ));
        }
        if response.feature_flags != INKPOD_FEATURE_NONE {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "InkScript host returned unsupported feature flags",
            ));
        }
        Ok(response)
    }
}

fn host_request(operation: u32) -> InkpodInkScriptHostRequest {
    InkpodInkScriptHostRequest {
        struct_size: size_of::<InkpodInkScriptHostRequest>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        operation,
        flags: 0,
        feature_flags: INKPOD_FEATURE_NONE,
        intent_id: 0,
        session_id: 0,
        session_generation: 0,
        source_generation: 0,
        byte_offset: 0,
        byte_capacity: 0,
        asset_symbol: InkpodInkScriptUtf8Span::default(),
        identity: ptr::null(),
        fingerprint: ptr::null(),
        relative_components: ptr::null(),
        relative_component_count: 0,
        known_directories: ptr::null(),
        known_directory_count: 0,
        temporary: InkpodInkScriptTemporaryIdentity::default(),
        overwrite_guard: [0; 32],
        bytes: ptr::null(),
        byte_count: 0,
    }
}

fn host_response() -> InkpodInkScriptHostResponse {
    InkpodInkScriptHostResponse {
        struct_size: size_of::<InkpodInkScriptHostResponse>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        flags: 0,
        reserved: 0,
        feature_flags: INKPOD_FEATURE_NONE,
        generation: 0,
        secondary_generation: 0,
        sequence_id: 0,
        records: ptr::null(),
        record_count: 0,
        record_stride_bytes: 0,
        observed_entries: 0,
        normalized_name_bytes: 0,
        work_units: 0,
        maximum_depth: 0,
        result_kind: 0,
        identity: ptr::null(),
        fingerprint: ptr::null(),
        fingerprint_after: ptr::null(),
        session: ptr::null(),
        bytes: ptr::null(),
        byte_count: 0,
        temporary: InkpodInkScriptTemporaryIdentity::default(),
        overwrite_guard: [0; 32],
    }
}

fn checked_utf8(span: InkpodInkScriptUtf8Span, name: &str) -> Result<String, u32> {
    if span.byte_count > MAX_INKSCRIPT_STRING_BYTES as u64 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{name} exceeds the InkScript string bound"),
        ));
    }
    if span.byte_count == 0 {
        return Ok(String::new());
    }
    if span.bytes.is_null() {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{name} is null with a nonzero length"),
        ));
    }
    let length = usize::try_from(span.byte_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{name} length is not representable"),
        )
    })?;
    // SAFETY: The host/caller contract keeps the advertised byte range readable for this call.
    let bytes = unsafe { slice::from_raw_parts(span.bytes, length) };
    std::str::from_utf8(bytes).map(str::to_owned).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{name} is not UTF-8"),
        )
    })
}

unsafe fn checked_record<'a, T>(pointer: *const T, name: &str) -> Result<&'a T, u32> {
    // SAFETY: The caller promises a readable size prefix for every non-null record pointer.
    unsafe { validate_struct(pointer, name)? };
    // SAFETY: Full record size and alignment were validated above.
    Ok(unsafe { &*pointer })
}

unsafe fn read_strided<T: Copy>(
    pointer: *const T,
    count: u64,
    stride: u64,
    name: &str,
) -> Result<Vec<T>, u32> {
    if count > MAX_HOST_RECORDS {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{name} count exceeds the execution bound"),
        ));
    }
    if count == 0 {
        if !pointer.is_null() || stride != 0 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                &format!("{name} empty span is not canonical"),
            ));
        }
        return Ok(Vec::new());
    }
    if pointer.is_null() || stride < size_of::<T>() as u64 || stride % align_of::<T>() as u64 != 0 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{name} span is null, short, or misaligned"),
        ));
    }
    let stride = usize::try_from(stride).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{name} stride overflow"),
        )
    })?;
    let count = usize::try_from(count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{name} count overflow"),
        )
    })?;
    let _ = count.checked_mul(stride).ok_or_else(|| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{name} byte range overflow"),
        )
    })?;
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{name} allocation exceeds resources"),
        )
    })?;
    for index in 0..count {
        // SAFETY: The checked strided range contains this record and the record is Copy.
        let record = unsafe { pointer.cast::<u8>().add(index * stride).cast::<T>().read() };
        values.push(record);
    }
    Ok(values)
}

unsafe fn validate_output_records<T>(
    pointer: *mut T,
    count: usize,
    stride: usize,
    name: &str,
    validate_fields: impl Fn(&T, usize) -> Result<(), u32>,
) -> Result<(), u32> {
    for index in 0..count {
        let offset = index.checked_mul(stride).ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                &format!("{name} output span overflows"),
            )
        })?;
        // SAFETY: The caller advertises this writable strided output record. Validation reads only
        // the required size prefix before the full record is accessed.
        let record = unsafe { pointer.cast::<u8>().add(offset).cast::<T>() };
        // SAFETY: Every public output record begins with a readable size prefix.
        unsafe { validate_struct(record.cast_const(), name)? };
        // SAFETY: The complete record and its alignment were validated above.
        validate_fields(unsafe { &*record }, stride)?;
    }
    Ok(())
}

fn path_record(value: &ValidatedPathIdentity) -> (Vec<u8>, InkpodInkScriptPathIdentity) {
    let key = value.canonical_key().as_bytes().to_vec();
    let record = InkpodInkScriptPathIdentity {
        struct_size: size_of::<InkpodInkScriptPathIdentity>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        feature_flags: 0,
        canonical_key: InkpodInkScriptUtf8Span {
            bytes: key.as_ptr(),
            byte_count: key.len() as u64,
        },
        volume_id: value.volume_id(),
        object_id: value.object_id().unwrap_or([0; 32]),
        object_generation: value.object_generation().unwrap_or(0),
        alias_key: value.alias_key(),
        parent_object_id: value.parent_object_id(),
        parent_generation: value.parent_generation(),
        parent_alias_key: value.parent_alias_key(),
        flags: if value.is_expected_absent() {
            INKPOD_INKSCRIPT_PATH_EXPECTED_ABSENT
        } else {
            0
        },
    };
    (key, record)
}

unsafe fn path_from_record(
    pointer: *const InkpodInkScriptPathIdentity,
) -> Result<ValidatedPathIdentity, u32> {
    // SAFETY: Callback/caller advertises a complete path record.
    let value = unsafe { checked_record(pointer, "InkpodInkScriptPathIdentity")? };
    if value.version != INKPOD_INKSCRIPT_RECORD_VERSION
        || value.feature_flags != 0
        || value.flags & !INKPOD_INKSCRIPT_PATH_EXPECTED_ABSENT != 0
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript path identity has invalid version or flags",
        ));
    }
    let key = checked_utf8(value.canonical_key, "InkScript canonical path key")?;
    let result = if value.flags & INKPOD_INKSCRIPT_PATH_EXPECTED_ABSENT != 0 {
        if value.object_id != [0; 32] || value.object_generation != 0 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "expected-absent InkScript path has an object identity",
            ));
        }
        ValidatedPathIdentity::expected_absent(
            key,
            value.volume_id,
            value.parent_object_id,
            value.alias_key,
            value.parent_alias_key,
        )
    } else {
        if value.object_generation == 0 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "existing InkScript path has no object generation",
            ));
        }
        ValidatedPathIdentity::existing(
            key,
            value.volume_id,
            value.object_id,
            value.alias_key,
            value.parent_object_id,
            value.parent_alias_key,
        )
        .and_then(|identity| {
            identity.with_generations(Some(value.object_generation), value.parent_generation)
        })
    };
    result.map_err(map_plan_error)
}

unsafe fn fingerprint_from_record(
    pointer: *const InkpodInkScriptNativeFingerprint,
) -> Result<NativeInputFingerprint, u32> {
    // SAFETY: Callback/caller advertises a complete fingerprint record.
    let value = unsafe { checked_record(pointer, "InkpodInkScriptNativeFingerprint")? };
    let valid_flags = INKPOD_INKSCRIPT_FINGERPRINT_HAS_CHANGE_TOKEN
        | INKPOD_INKSCRIPT_FINGERPRINT_ATOMIC_OVERWRITE;
    if value.version != INKPOD_INKSCRIPT_RECORD_VERSION
        || value.feature_flags != 0
        || value.flags & !valid_flags != 0
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript native fingerprint has invalid version or flags",
        ));
    }
    // SAFETY: Nested callback record remains live for this conversion.
    let path = unsafe { path_from_record(value.path)? };
    let label = checked_utf8(value.display_label, "InkScript input label")?;
    let uuid = u128::from(value.document_uuid_low) | (u128::from(value.document_uuid_high) << 64);
    NativeInputFingerprint::new(
        path,
        label,
        value.display_number,
        uuid,
        value.logical_length,
        value.content_digest,
        (value.flags & INKPOD_INKSCRIPT_FINGERPRINT_HAS_CHANGE_TOKEN != 0)
            .then_some(value.change_token),
        value.flags & INKPOD_INKSCRIPT_FINGERPRINT_ATOMIC_OVERWRITE != 0,
    )
    .map_err(map_plan_error)
}

fn map_plan_error(error: ScriptPlanError) -> u32 {
    match error {
        ScriptPlanError::Cancelled => fail(INKPOD_STATUS_CANCELLED, "InkScript planning cancelled"),
        ScriptPlanError::ResourceLimit | ScriptPlanError::NumberOverflow => fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript planning exceeded a checked resource or numeric bound",
        ),
        ScriptPlanError::UnsupportedAtomicOverwrite => fail(
            INKPOD_STATUS_UNSUPPORTED,
            "InkScript host cannot provide atomic overwrite",
        ),
        ScriptPlanError::StaleAuthority
        | ScriptPlanError::StaleInput
        | ScriptPlanError::StaleConfirmation
        | ScriptPlanError::ConfirmationConsumed => fail(
            INKPOD_STATUS_INVALID_STATE,
            "InkScript plan, authority, input, or confirmation is stale",
        ),
        _ => fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript execution planning rejected the supplied DTOs",
        ),
    }
}

fn map_plan_adapter_status(status: u32) -> ScriptPlanAdapterError {
    match status {
        INKPOD_STATUS_INVALID_ARGUMENT | INKPOD_STATUS_INCOMPATIBLE_ABI => {
            ScriptPlanAdapterError::InvalidData
        }
        INKPOD_STATUS_UNSUPPORTED | INKPOD_STATUS_NO_DOCUMENT => {
            ScriptPlanAdapterError::Unavailable
        }
        _ => ScriptPlanAdapterError::Failure,
    }
}

fn map_run_adapter_status(status: u32) -> ScriptRunAdapterError {
    match status {
        INKPOD_STATUS_CANCELLED => ScriptRunAdapterError::Cancelled,
        INKPOD_STATUS_IO_ERROR => ScriptRunAdapterError::Io,
        INKPOD_STATUS_UNSUPPORTED => ScriptRunAdapterError::UnsupportedAtomicInstall,
        INKPOD_STATUS_INVALID_ARGUMENT | INKPOD_STATUS_INCOMPATIBLE_ABI => {
            ScriptRunAdapterError::InvalidData
        }
        _ => ScriptRunAdapterError::Unavailable,
    }
}

fn access_from_abi(value: u32) -> Result<InkScriptPathIntentAccess, u32> {
    match value {
        INKPOD_INKSCRIPT_PATH_READ => Ok(InkScriptPathIntentAccess::Read),
        INKPOD_INKSCRIPT_PATH_ENUMERATE => Ok(InkScriptPathIntentAccess::Enumerate),
        INKPOD_INKSCRIPT_PATH_CREATE => Ok(InkScriptPathIntentAccess::Create),
        INKPOD_INKSCRIPT_PATH_REPLACE => Ok(InkScriptPathIntentAccess::Replace),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript path access is unknown",
        )),
    }
}

fn access_to_abi(value: InkScriptPathIntentAccess) -> u32 {
    match value {
        InkScriptPathIntentAccess::Read => INKPOD_INKSCRIPT_PATH_READ,
        InkScriptPathIntentAccess::Enumerate => INKPOD_INKSCRIPT_PATH_ENUMERATE,
        InkScriptPathIntentAccess::Create => INKPOD_INKSCRIPT_PATH_CREATE,
        InkScriptPathIntentAccess::Replace => INKPOD_INKSCRIPT_PATH_REPLACE,
    }
}

unsafe fn session_from_record(
    pointer: *const InkpodInkScriptSessionInput,
) -> Result<ScriptSessionSnapshot, u32> {
    // SAFETY: Callback advertises a complete session record and nested Core/path for this call.
    let value = unsafe { checked_record(pointer, "InkpodInkScriptSessionInput")? };
    if value.version != INKPOD_INKSCRIPT_RECORD_VERSION
        || value.feature_flags != 0
        || value.reserved != 0
        || value.core.is_null()
        || !is_aligned(value.core)
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript session snapshot record is invalid",
        ));
    }
    // SAFETY: The host returns a live borrowed Core and no reference escapes this conversion.
    let core = unsafe { &*value.core };
    let status = validate_core_thread(core);
    if status != INKPOD_STATUS_OK {
        return Err(status);
    }
    let label = checked_utf8(value.display_label, "InkScript session label")?;
    let backing = if value.backing_path.is_null() {
        None
    } else {
        // SAFETY: Optional nested path remains live for the callback conversion.
        Some(unsafe { path_from_record(value.backing_path)? })
    };
    ScriptSessionSnapshot::capture(
        value.session_id,
        value.session_generation,
        value.source_generation,
        label,
        value.display_number,
        backing,
        &core.core,
    )
    .map_err(map_plan_error)
}

unsafe fn sequence_from_response(
    response: &InkpodInkScriptHostResponse,
) -> Result<ScriptSequenceSnapshot, u32> {
    // SAFETY: The callback response advertises a bounded strided member span.
    let members = unsafe {
        read_strided::<InkpodInkScriptSequenceMember>(
            response.records.cast(),
            response.record_count,
            response.record_stride_bytes,
            "InkScript sequence members",
        )?
    };
    let mut output = Vec::new();
    output.try_reserve_exact(members.len()).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript sequence member allocation failed",
        )
    })?;
    for member in members {
        if member.struct_size < size_of::<InkpodInkScriptSequenceMember>() as u32
            || member.version != INKPOD_INKSCRIPT_RECORD_VERSION
            || member.feature_flags != 0
            || member.reserved != 0
        {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript sequence member record is invalid",
            ));
        }
        match member.kind {
            INKPOD_INKSCRIPT_SESSION_MEMBER => {
                if !member.fingerprint.is_null() {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "InkScript session member has a file fingerprint",
                    ));
                }
                // SAFETY: Nested session remains live for this callback conversion.
                output.push(ScriptSequenceMemberSnapshot::Session(unsafe {
                    session_from_record(member.session)?
                }));
            }
            INKPOD_INKSCRIPT_FILE_MEMBER => {
                if !member.session.is_null() || member.source_generation == 0 {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "InkScript file member has invalid source metadata",
                    ));
                }
                // SAFETY: Nested fingerprint remains live for this callback conversion.
                output.push(ScriptSequenceMemberSnapshot::File {
                    source_generation: member.source_generation,
                    fingerprint: unsafe { fingerprint_from_record(member.fingerprint)? },
                });
            }
            _ => {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "InkScript sequence member kind is unknown",
                ));
            }
        }
    }
    ScriptSequenceSnapshot::new(response.sequence_id, response.generation, output)
        .map_err(map_plan_error)
}

fn temporary_to_record(value: ScriptTemporaryIdentity) -> InkpodInkScriptTemporaryIdentity {
    InkpodInkScriptTemporaryIdentity {
        volume_id: value.volume_id(),
        parent_object_id: value.parent_object_id(),
        parent_generation: value.parent_generation(),
        object_id: value.object_id(),
        object_generation: value.object_generation(),
    }
}

fn temporary_from_record(
    value: InkpodInkScriptTemporaryIdentity,
) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError> {
    ScriptTemporaryIdentity::new(
        value.volume_id,
        value.parent_object_id,
        value.parent_generation,
        value.object_id,
        value.object_generation,
    )
}

fn with_fingerprint_record<R>(
    value: &NativeInputFingerprint,
    operation: impl FnOnce(&InkpodInkScriptNativeFingerprint) -> R,
) -> R {
    let (key, path) = path_record(value.path());
    let label = value.display_label().as_bytes();
    let uuid = value.document_uuid();
    let flags = if value.change_token().is_some() {
        INKPOD_INKSCRIPT_FINGERPRINT_HAS_CHANGE_TOKEN
    } else {
        0
    } | if value.supports_atomic_overwrite() {
        INKPOD_INKSCRIPT_FINGERPRINT_ATOMIC_OVERWRITE
    } else {
        0
    };
    let record = InkpodInkScriptNativeFingerprint {
        struct_size: size_of::<InkpodInkScriptNativeFingerprint>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        feature_flags: 0,
        path: &path,
        display_label: InkpodInkScriptUtf8Span {
            bytes: label.as_ptr(),
            byte_count: label.len() as u64,
        },
        display_number: value.display_number(),
        flags,
        document_uuid_low: uuid as u64,
        document_uuid_high: (uuid >> 64) as u64,
        logical_length: value.logical_length(),
        content_digest: value.content_digest(),
        change_token: value.change_token().unwrap_or([0; 32]),
    };
    let result = operation(&record);
    drop(key);
    result
}

struct HostPlanAdapter {
    host: HostBridge,
}

impl HostPlanAdapter {
    fn session(&mut self, operation: u32) -> Result<ScriptSessionSnapshot, ScriptPlanAdapterError> {
        let response = self
            .host
            .invoke(operation, |_| {})
            .map_err(map_plan_adapter_status)?;
        if response.flags & INKPOD_INKSCRIPT_HOST_RESPONSE_PRESENT == 0 {
            return Err(ScriptPlanAdapterError::Unavailable);
        }
        // SAFETY: Host response pointers are borrowed for this conversion only.
        unsafe { session_from_record(response.session) }
            .map_err(|_| ScriptPlanAdapterError::InvalidData)
    }
}

impl ScriptPlanAdapter for HostPlanAdapter {
    fn authority_generation(&mut self) -> Result<u64, ScriptPlanAdapterError> {
        self.host
            .invoke(INKPOD_INKSCRIPT_HOST_AUTHORITY_GENERATION, |_| {})
            .map(|response| response.generation)
            .map_err(map_plan_adapter_status)
    }

    fn open_session_set(&mut self) -> Result<OpenSessionSetSnapshot, ScriptPlanAdapterError> {
        let response = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_OPEN_SESSIONS, |_| {})
            .map_err(map_plan_adapter_status)?;
        // SAFETY: Host response advertises a bounded strided open-session span.
        let records = unsafe {
            read_strided::<InkpodInkScriptOpenSession>(
                response.records.cast(),
                response.record_count,
                response.record_stride_bytes,
                "InkScript open sessions",
            )
        }
        .map_err(|_| ScriptPlanAdapterError::InvalidData)?;
        let mut sessions = Vec::new();
        for record in records {
            if record.struct_size < size_of::<InkpodInkScriptOpenSession>() as u32
                || record.version != INKPOD_INKSCRIPT_RECORD_VERSION
                || record.feature_flags != 0
            {
                return Err(ScriptPlanAdapterError::InvalidData);
            }
            // SAFETY: Nested path remains live for this callback conversion.
            let path = unsafe { path_from_record(record.backing_path) }
                .map_err(|_| ScriptPlanAdapterError::InvalidData)?;
            let uuid = u128::from(record.document_uuid_low)
                | (u128::from(record.document_uuid_high) << 64);
            sessions.push(
                OpenSessionRecord::new(record.session_id, record.session_generation, uuid, path)
                    .map_err(|_| ScriptPlanAdapterError::InvalidData)?,
            );
        }
        OpenSessionSetSnapshot::new(response.generation, sessions)
            .map_err(|_| ScriptPlanAdapterError::InvalidData)
    }

    fn resolve_file(
        &mut self,
        intent_id: u64,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<NativeInputFingerprint, ScriptPlanAdapterError> {
        let response = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_RESOLVE_FILE, |request| {
                request.intent_id = intent_id;
            })
            .map_err(map_plan_adapter_status)?;
        // SAFETY: Host response fingerprint is borrowed for this conversion.
        unsafe { fingerprint_from_record(response.fingerprint) }
            .map_err(|_| ScriptPlanAdapterError::InvalidData)
    }

    fn enumerate_folder(
        &mut self,
        intent_id: u64,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<FolderScan, ScriptPlanAdapterError> {
        let response = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_ENUMERATE_FOLDER, |request| {
                request.intent_id = intent_id;
            })
            .map_err(map_plan_adapter_status)?;
        // SAFETY: Host response advertises a bounded strided fingerprint span.
        let records = unsafe {
            read_strided::<InkpodInkScriptNativeFingerprint>(
                response.records.cast(),
                response.record_count,
                response.record_stride_bytes,
                "InkScript folder fingerprints",
            )
        }
        .map_err(|_| ScriptPlanAdapterError::InvalidData)?;
        let files = records
            .iter()
            .map(|record| {
                // SAFETY: Each copied record's nested pointers remain host-owned for this call.
                unsafe { fingerprint_from_record(record) }
                    .map_err(|_| ScriptPlanAdapterError::InvalidData)
            })
            .collect::<Result<Vec<_>, _>>()?;
        FolderScan::new(
            response.observed_entries,
            response.normalized_name_bytes,
            response.work_units,
            response.maximum_depth,
            files,
        )
        .map_err(|_| ScriptPlanAdapterError::InvalidData)
    }

    fn capture_current_document(
        &mut self,
        _expected: &ScriptSessionExpectation,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptSessionSnapshot, ScriptPlanAdapterError> {
        self.session(INKPOD_INKSCRIPT_HOST_CURRENT_DOCUMENT)
    }

    fn capture_current_sequence(
        &mut self,
        _expected: &ScriptSequenceExpectation,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptSequenceSnapshot, ScriptPlanAdapterError> {
        let response = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_CURRENT_SEQUENCE, |_| {})
            .map_err(map_plan_adapter_status)?;
        if response.flags & INKPOD_INKSCRIPT_HOST_RESPONSE_PRESENT == 0 {
            return Err(ScriptPlanAdapterError::Unavailable);
        }
        // SAFETY: Host response member span is borrowed for this conversion.
        unsafe { sequence_from_response(&response) }
            .map_err(|_| ScriptPlanAdapterError::InvalidData)
    }

    fn capture_open_session(
        &mut self,
        session: &OpenSessionRecord,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptSessionSnapshot, ScriptPlanAdapterError> {
        let response = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_CAPTURE_OPEN_SESSION, |request| {
                request.session_id = session.session_id();
                request.session_generation = session.session_generation();
            })
            .map_err(map_plan_adapter_status)?;
        // SAFETY: Host response session is borrowed for this conversion.
        unsafe { session_from_record(response.session) }
            .map_err(|_| ScriptPlanAdapterError::InvalidData)
    }

    fn resolve_destination(
        &mut self,
        request_value: &ScriptDestinationRequest,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ValidatedPathIdentity, ScriptPlanAdapterError> {
        let (key, base_record, intent_id) = match request_value.base() {
            ScriptDestinationBase::AuthorizedRoot { intent_id, root } => {
                let (key, record) = path_record(root);
                (key, record, *intent_id)
            }
            ScriptDestinationBase::InputParent { input_path } => {
                let (key, record) = path_record(input_path);
                (key, record, 0)
            }
        };
        let bytes = request_value
            .relative_components()
            .iter()
            .map(|value| value.as_bytes())
            .collect::<Vec<_>>();
        let components = bytes
            .iter()
            .map(|value| InkpodInkScriptUtf8Span {
                bytes: value.as_ptr(),
                byte_count: value.len() as u64,
            })
            .collect::<Vec<_>>();
        let response = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_RESOLVE_DESTINATION, |request| {
                request.intent_id = intent_id;
                request.identity = &base_record;
                request.relative_components = components.as_ptr();
                request.relative_component_count = components.len() as u64;
            })
            .map_err(map_plan_adapter_status)?;
        drop(key);
        // SAFETY: Host response identity is borrowed for this conversion.
        unsafe { path_from_record(response.identity) }
            .map_err(|_| ScriptPlanAdapterError::InvalidData)
    }
}

struct HostAssetReader {
    host: HostBridge,
    symbol: Box<str>,
    offset: u64,
}

impl AuthorizedAssetReader for HostAssetReader {
    fn observe_identity(&mut self) -> Result<AuthorizedAssetIdentity, AuthorizedAssetReadError> {
        let bytes = self.symbol.as_bytes();
        let response = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_ASSET_IDENTITY, |request| {
                request.asset_symbol = InkpodInkScriptUtf8Span {
                    bytes: bytes.as_ptr(),
                    byte_count: bytes.len() as u64,
                };
            })
            .map_err(|_| AuthorizedAssetReadError::IdentityUnavailable)?;
        if response.overwrite_guard == [0; 32] || response.generation == 0 {
            return Err(AuthorizedAssetReadError::IdentityUnavailable);
        }
        Ok(AuthorizedAssetIdentity::new(
            response.overwrite_guard,
            response.generation,
            response.byte_count,
        ))
    }

    fn read_chunk(&mut self, target: &mut [u8]) -> Result<usize, AuthorizedAssetReadError> {
        let symbol = self.symbol.as_bytes();
        let response = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_ASSET_READ, |request| {
                request.asset_symbol = InkpodInkScriptUtf8Span {
                    bytes: symbol.as_ptr(),
                    byte_count: symbol.len() as u64,
                };
                request.byte_offset = self.offset;
                request.byte_capacity = target.len() as u64;
            })
            .map_err(|_| AuthorizedAssetReadError::ReadFailed)?;
        if response.byte_count > target.len() as u64
            || (response.byte_count != 0 && response.bytes.is_null())
        {
            return Err(AuthorizedAssetReadError::ReadFailed);
        }
        let count = usize::try_from(response.byte_count)
            .map_err(|_| AuthorizedAssetReadError::ReadFailed)?;
        if count != 0 {
            // SAFETY: The host promises a readable response span for this callback return.
            let bytes = unsafe { slice::from_raw_parts(response.bytes, count) };
            target[..count].copy_from_slice(bytes);
        }
        self.offset = self
            .offset
            .checked_add(response.byte_count)
            .ok_or(AuthorizedAssetReadError::ReadFailed)?;
        Ok(count)
    }
}

struct HostRunAdapter {
    host: HostBridge,
}

impl ScriptRunAdapter for HostRunAdapter {
    fn authority_generation(&mut self) -> Result<u64, ScriptRunAdapterError> {
        self.host
            .invoke(INKPOD_INKSCRIPT_HOST_AUTHORITY_GENERATION, |_| {})
            .map(|response| response.generation)
            .map_err(map_run_adapter_status)
    }

    fn open_session_set_generation(&mut self) -> Result<u64, ScriptRunAdapterError> {
        self.host
            .invoke(INKPOD_INKSCRIPT_HOST_OPEN_SESSION_GENERATION, |_| {})
            .map(|response| response.generation)
            .map_err(map_run_adapter_status)
    }

    fn session_is_current(
        &mut self,
        session_id: u64,
        session_generation: u64,
        source_generation: u64,
    ) -> Result<bool, ScriptRunAdapterError> {
        self.host
            .invoke(INKPOD_INKSCRIPT_HOST_SESSION_IS_CURRENT, |request| {
                request.session_id = session_id;
                request.session_generation = session_generation;
                request.source_generation = source_generation;
            })
            .map(|response| response.flags & INKPOD_INKSCRIPT_HOST_RESPONSE_PRESENT != 0)
            .map_err(map_run_adapter_status)
    }

    fn read_native(
        &mut self,
        expected: &NativeInputFingerprint,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptNativeRead, ScriptRunAdapterError> {
        with_fingerprint_record(expected, |record| {
            let response = self
                .host
                .invoke(INKPOD_INKSCRIPT_HOST_READ_NATIVE, |request| {
                    request.fingerprint = record;
                })
                .map_err(map_run_adapter_status)?;
            if response.byte_count != 0 && response.bytes.is_null() {
                return Err(ScriptRunAdapterError::InvalidData);
            }
            let count = usize::try_from(response.byte_count)
                .map_err(|_| ScriptRunAdapterError::InvalidData)?;
            // SAFETY: Host response bytes remain readable for this callback conversion.
            let bytes = if count == 0 {
                Vec::new()
            } else {
                unsafe { slice::from_raw_parts(response.bytes, count) }.to_vec()
            };
            // SAFETY: Both nested fingerprint records remain live for this conversion.
            let before = unsafe { fingerprint_from_record(response.fingerprint) }
                .map_err(|_| ScriptRunAdapterError::InvalidData)?;
            let after = unsafe { fingerprint_from_record(response.fingerprint_after) }
                .map_err(|_| ScriptRunAdapterError::InvalidData)?;
            Ok(ScriptNativeRead::new(bytes, before, after))
        })
    }

    fn fingerprint_native(
        &mut self,
        expected: &NativeInputFingerprint,
    ) -> Result<NativeInputFingerprint, ScriptRunAdapterError> {
        with_fingerprint_record(expected, |record| {
            let response = self
                .host
                .invoke(INKPOD_INKSCRIPT_HOST_FINGERPRINT_NATIVE, |request| {
                    request.fingerprint = record;
                })
                .map_err(map_run_adapter_status)?;
            // SAFETY: Nested host response is borrowed for this conversion.
            unsafe { fingerprint_from_record(response.fingerprint) }
                .map_err(|_| ScriptRunAdapterError::InvalidData)
        })
    }

    fn atomic_capabilities(
        &mut self,
        destination: &ValidatedPathIdentity,
    ) -> Result<ScriptAtomicCapabilities, ScriptRunAdapterError> {
        let (key, record) = path_record(destination);
        let response = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_ATOMIC_CAPABILITIES, |request| {
                request.identity = &record;
            })
            .map_err(map_run_adapter_status)?;
        drop(key);
        Ok(ScriptAtomicCapabilities {
            install: response.flags & INKPOD_INKSCRIPT_HOST_CAPABILITY_INSTALL != 0,
            overwrite: response.flags & INKPOD_INKSCRIPT_HOST_CAPABILITY_OVERWRITE != 0,
        })
    }

    fn prepare_destination(
        &mut self,
        destination: &ValidatedPathIdentity,
        known_job_directories: &[ValidatedPathIdentity],
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptPreparedDestination, ScriptRunAdapterError> {
        let (key, destination_record) = path_record(destination);
        let owned = known_job_directories
            .iter()
            .map(path_record)
            .collect::<Vec<_>>();
        let records = owned.iter().map(|value| value.1).collect::<Vec<_>>();
        let response = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_PREPARE_DESTINATION, |request| {
                request.identity = &destination_record;
                request.known_directories = records.as_ptr();
                request.known_directory_count = records.len() as u64;
            })
            .map_err(map_run_adapter_status)?;
        drop(key);
        // SAFETY: Host response identity and created-directory span are borrowed for this call.
        let observed = unsafe { path_from_record(response.identity) }
            .map_err(|_| ScriptRunAdapterError::InvalidData)?;
        let created_records = unsafe {
            read_strided::<InkpodInkScriptPathIdentity>(
                response.records.cast(),
                response.record_count,
                response.record_stride_bytes,
                "InkScript created directories",
            )
        }
        .map_err(|_| ScriptRunAdapterError::InvalidData)?;
        let created = created_records
            .iter()
            .map(|record| {
                // SAFETY: Nested UTF-8 path data remains live for this host response.
                unsafe { path_from_record(record) }.map_err(|_| ScriptRunAdapterError::InvalidData)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ScriptPreparedDestination::new(observed, created))
    }

    fn revalidate_destination(
        &mut self,
        destination: &ValidatedPathIdentity,
    ) -> Result<ValidatedPathIdentity, ScriptRunAdapterError> {
        let (key, record) = path_record(destination);
        let response = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_REVALIDATE_DESTINATION, |request| {
                request.identity = &record;
            })
            .map_err(map_run_adapter_status)?;
        drop(key);
        // SAFETY: Host response identity is borrowed for this conversion.
        unsafe { path_from_record(response.identity) }
            .map_err(|_| ScriptRunAdapterError::InvalidData)
    }

    fn create_same_volume_temporary(
        &mut self,
        destination: &ValidatedPathIdentity,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError> {
        let (key, record) = path_record(destination);
        let response = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_CREATE_TEMPORARY, |request| {
                request.identity = &record;
            })
            .map_err(map_run_adapter_status)?;
        drop(key);
        temporary_from_record(response.temporary)
    }

    fn write_flush_close_temporary(
        &mut self,
        temporary: ScriptTemporaryIdentity,
        bytes: &[u8],
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError> {
        let response = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_WRITE_CLOSE_TEMPORARY, |request| {
                request.temporary = temporary_to_record(temporary);
                request.bytes = bytes.as_ptr();
                request.byte_count = bytes.len() as u64;
            })
            .map_err(map_run_adapter_status)?;
        temporary_from_record(response.temporary)
    }

    fn revalidate_closed_temporary(
        &mut self,
        temporary: ScriptTemporaryIdentity,
    ) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError> {
        let response = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_REVALIDATE_TEMPORARY, |request| {
                request.temporary = temporary_to_record(temporary);
            })
            .map_err(map_run_adapter_status)?;
        temporary_from_record(response.temporary)
    }

    fn acquire_overwrite_guard(
        &mut self,
        source: &NativeInputFingerprint,
    ) -> Result<ScriptOverwriteGuard, ScriptRunAdapterError> {
        with_fingerprint_record(source, |record| {
            let response = self
                .host
                .invoke(INKPOD_INKSCRIPT_HOST_ACQUIRE_OVERWRITE_GUARD, |request| {
                    request.fingerprint = record;
                })
                .map_err(map_run_adapter_status)?;
            ScriptOverwriteGuard::new(response.overwrite_guard)
        })
    }

    fn fingerprint_under_guard(
        &mut self,
        guard: ScriptOverwriteGuard,
        source: &NativeInputFingerprint,
    ) -> Result<NativeInputFingerprint, ScriptRunAdapterError> {
        with_fingerprint_record(source, |record| {
            let response = self
                .host
                .invoke(INKPOD_INKSCRIPT_HOST_FINGERPRINT_UNDER_GUARD, |request| {
                    request.fingerprint = record;
                    request.overwrite_guard = guard.id();
                })
                .map_err(map_run_adapter_status)?;
            // SAFETY: Host response fingerprint is borrowed for this conversion.
            unsafe { fingerprint_from_record(response.fingerprint) }
                .map_err(|_| ScriptRunAdapterError::InvalidData)
        })
    }

    fn release_overwrite_guard(&mut self, guard: ScriptOverwriteGuard) {
        let _ = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_RELEASE_OVERWRITE_GUARD, |request| {
                request.overwrite_guard = guard.id();
            });
    }

    fn atomic_install(
        &mut self,
        temporary: ScriptTemporaryIdentity,
        destination: &ValidatedPathIdentity,
        overwrite_guard: Option<ScriptOverwriteGuard>,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptAtomicInstallResult, ScriptRunAdapterError> {
        let (key, record) = path_record(destination);
        let response = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_ATOMIC_INSTALL, |request| {
                request.temporary = temporary_to_record(temporary);
                request.identity = &record;
                if let Some(guard) = overwrite_guard {
                    request.flags |= INKPOD_INKSCRIPT_HOST_HAS_OVERWRITE_GUARD;
                    request.overwrite_guard = guard.id();
                }
            })
            .map_err(map_run_adapter_status)?;
        drop(key);
        match response.result_kind {
            1 => Ok(ScriptAtomicInstallResult::Installed),
            2 => Ok(ScriptAtomicInstallResult::InstalledAfterCancellation),
            3 => Ok(ScriptAtomicInstallResult::CancelledBeforeLinearization),
            _ => Err(ScriptRunAdapterError::InvalidData),
        }
    }

    fn cleanup_closed_temporary(&mut self, temporary: ScriptTemporaryIdentity) {
        let _ = self
            .host
            .invoke(INKPOD_INKSCRIPT_HOST_CLEANUP_TEMPORARY, |request| {
                request.temporary = temporary_to_record(temporary);
            });
    }
}

#[derive(Clone, Copy)]
struct TaskEventData {
    kind: u32,
    task_state: u32,
    ordinal: u64,
    completed: u64,
    total: u64,
    wait_milliseconds: u32,
    outcome: u32,
    failure: u32,
}

pub struct InkpodInkScriptPlanTask {
    owner_thread: ThreadId,
    core_generation: u64,
    controller_id: u64,
    session_generation: u64,
    authority_generation: u64,
    open_session_set_generation: u64,
    program: StaticScriptProgram,
    grants: Vec<AuthorityGrant>,
    script_path: Option<ValidatedPathIdentity>,
    maximum_folder_entries: u64,
    host: HostBridge,
    state: AtomicU32,
    cancelled: AtomicBool,
    completed_work: AtomicU64,
    total_work: AtomicU64,
    owner_data: UnsafeCell<PlanTaskOwnerData>,
}

struct PlanTaskOwnerData {
    terminal_status: u32,
    pending_event: Option<TaskEventData>,
    plan: Option<ScriptExecutionPlan>,
}

// SAFETY: Immutable route data and progress/cancellation atomics may be read concurrently.
// `owner_data` is accessed only by the recorded owner thread, and release is externally
// synchronized against every task operation by the C ABI contract.
unsafe impl Sync for InkpodInkScriptPlanTask {}

pub struct InkpodInkScriptPlan {
    owner_thread: ThreadId,
    core_generation: u64,
    controller_id: u64,
    session_generation: u64,
    plan: ScriptExecutionPlan,
}

pub struct InkpodInkScriptConfirmation {
    owner_thread: ThreadId,
    core_generation: u64,
    controller_id: u64,
    session_generation: u64,
    plan_digest: [u8; 32],
    token: ScriptConfirmationToken,
}

pub struct InkpodInkScriptRunTask {
    owner_thread: ThreadId,
    core_generation: u64,
    state: AtomicU32,
    cancelled: AtomicBool,
    completed_work: AtomicU64,
    total_work: AtomicU64,
    owner_data: UnsafeCell<RunTaskOwnerData>,
}

struct RunTaskOwnerData {
    terminal_status: u32,
    pending_event: Option<TaskEventData>,
    task: ScriptRunTask,
    adapter: HostRunAdapter,
    report: Option<ScriptRunReport>,
}

// SAFETY: Immutable route data and progress/cancellation atomics may be read concurrently.
// `owner_data` is accessed only by the recorded owner thread, and release is externally
// synchronized against every task operation by the C ABI contract.
unsafe impl Sync for InkpodInkScriptRunTask {}

pub struct InkpodInkScriptReport {
    report: ScriptRunReport,
}

fn validate_execution_core(core: *mut InkpodCore) -> Result<&'static mut InkpodCore, u32> {
    if core.is_null() || !is_aligned(core) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript execution Core is null or misaligned",
        ));
    }
    // SAFETY: Exported contracts require a live uniquely routed Core on its owner thread.
    let core = unsafe { &mut *core };
    let status = validate_core_thread(core);
    if status != INKPOD_STATUS_OK {
        return Err(status);
    }
    Ok(core)
}

fn validate_route(
    owner_thread: ThreadId,
    core_generation: u64,
    core: &InkpodCore,
) -> Result<(), u32> {
    if owner_thread != thread::current().id() {
        return Err(fail(
            INKPOD_STATUS_WRONG_THREAD,
            "InkScript execution handle must be used on its owner thread",
        ));
    }
    if core_generation != core.objects.generation() {
        return Err(fail(
            INKPOD_STATUS_INVALID_STATE,
            "InkScript execution handle belongs to a stale Core generation",
        ));
    }
    Ok(())
}

unsafe fn authority_grants(
    request: &InkpodInkScriptPlanTaskRequest,
) -> Result<Vec<AuthorityGrant>, u32> {
    // SAFETY: The caller advertises a readable bounded strided authority span.
    let records = unsafe {
        read_strided::<InkpodInkScriptAuthorityGrant>(
            request.grants,
            request.grant_count,
            request.grant_stride_bytes,
            "InkScript authority grants",
        )?
    };
    records
        .into_iter()
        .map(|record| {
            if record.struct_size < size_of::<InkpodInkScriptAuthorityGrant>() as u32
                || record.version != INKPOD_INKSCRIPT_RECORD_VERSION
                || record.feature_flags != 0
                || record.reserved != 0
            {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "InkScript authority grant record is invalid",
                ));
            }
            let access = access_from_abi(record.access)?;
            // SAFETY: Nested path remains readable during request ingestion.
            let path = unsafe { path_from_record(record.resolved)? };
            AuthorityGrant::new(
                record.intent_id,
                access,
                record.authority_id,
                record.authority_generation,
                path,
            )
            .map_err(map_plan_error)
        })
        .collect()
}

fn optional_session(
    host: HostBridge,
    operation: u32,
) -> Result<Option<ScriptSessionSnapshot>, u32> {
    let response = match host.invoke(operation, |_| {}) {
        Ok(response) => response,
        Err(INKPOD_STATUS_NO_DOCUMENT | INKPOD_STATUS_UNSUPPORTED) => return Ok(None),
        Err(status) => return Err(status),
    };
    if response.flags & INKPOD_INKSCRIPT_HOST_RESPONSE_PRESENT == 0 {
        return Ok(None);
    }
    // SAFETY: Host response session remains borrowed for this conversion.
    unsafe { session_from_record(response.session) }.map(Some)
}

fn optional_sequence(host: HostBridge) -> Result<Option<ScriptSequenceSnapshot>, u32> {
    let response = match host.invoke(INKPOD_INKSCRIPT_HOST_CURRENT_SEQUENCE, |_| {}) {
        Ok(response) => response,
        Err(INKPOD_STATUS_NO_DOCUMENT | INKPOD_STATUS_UNSUPPORTED) => return Ok(None),
        Err(status) => return Err(status),
    };
    if response.flags & INKPOD_INKSCRIPT_HOST_RESPONSE_PRESENT == 0 {
        return Ok(None);
    }
    // SAFETY: Host response member span remains borrowed for this conversion.
    unsafe { sequence_from_response(&response) }.map(Some)
}

fn plan_once(task: &InkpodInkScriptPlanTask) -> Result<ScriptExecutionPlan, u32> {
    if task.cancelled.load(Ordering::Acquire) {
        return Err(INKPOD_STATUS_CANCELLED);
    }
    let current_document = optional_session(task.host, INKPOD_INKSCRIPT_HOST_CURRENT_DOCUMENT)?;
    let current_sequence = optional_sequence(task.host)?;
    let command_context = ScriptCommandContext::new(
        current_document
            .as_ref()
            .map(ScriptSessionExpectation::from_snapshot)
            .transpose()
            .map_err(map_plan_error)?,
        current_sequence
            .as_ref()
            .map(ScriptSequenceExpectation::from_snapshot)
            .transpose()
            .map_err(map_plan_error)?,
    );
    let authority = AuthoritySnapshot::new(
        *task.program.static_compile_digest(),
        *task.program.path_intent_digest(),
        task.authority_generation,
        task.grants.clone(),
        command_context,
        task.open_session_set_generation,
        task.script_path.clone(),
    )
    .map_err(map_plan_error)?;

    let symbols = task
        .program
        .path_intents()
        .iter()
        .filter_map(|intent| match intent.subject() {
            ScriptPathIntentSubject::Asset(symbol) => Some(symbol.clone()),
            ScriptPathIntentSubject::Input(_) | ScriptPathIntentSubject::OutputRoot => None,
        })
        .collect::<Vec<_>>();
    let mut readers = symbols
        .iter()
        .map(|symbol| HostAssetReader {
            host: task.host,
            symbol: symbol.clone().into_boxed_str(),
            offset: 0,
        })
        .collect::<Vec<_>>();
    let identities = readers
        .iter_mut()
        .map(AuthorizedAssetReader::observe_identity)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript external asset authority could not be observed",
            )
        })?;
    let mut streams = symbols
        .iter()
        .zip(identities)
        .zip(readers.iter_mut())
        .map(|((symbol, identity), reader)| {
            AuthorizedAssetStream::new(symbol.as_str(), identity, reader)
        })
        .collect::<Vec<_>>();
    let mut adapter = HostPlanAdapter { host: task.host };
    let mut cancelled = || task.cancelled.load(Ordering::Acquire);
    let limits = if task.maximum_folder_entries == 0 {
        ScriptPlanLimits::exact_current()
    } else {
        ScriptPlanLimits::exact_current().with_folder_entries(task.maximum_folder_entries)
    };
    plan_inkscript(
        &task.program,
        &authority,
        &mut adapter,
        &mut streams,
        limits,
        &mut cancelled,
    )
    .map_err(map_plan_error)
}

fn task_info(
    state: &AtomicU32,
    completed: &AtomicU64,
    total: &AtomicU64,
    output: *mut InkpodTaskInfo,
) -> u32 {
    // SAFETY: Public task info exposes a readable size prefix.
    if let Err(status) = unsafe { validate_struct(output.cast_const(), "InkpodTaskInfo") } {
        return status;
    }
    // SAFETY: Full caller-owned writable record was validated above.
    let output = unsafe { &mut *output };
    *output = InkpodTaskInfo {
        struct_size: size_of::<InkpodTaskInfo>() as u32,
        state: state.load(Ordering::Acquire),
        completed_work: completed.load(Ordering::Acquire),
        total_work: total.load(Ordering::Acquire),
        reserved: 0,
    };
    INKPOD_STATUS_OK
}

fn write_event(event: TaskEventData, output: *mut InkpodInkScriptTaskEvent) -> u32 {
    // SAFETY: Public event exposes a readable size prefix.
    if let Err(status) = unsafe { validate_struct(output.cast_const(), "InkpodInkScriptTaskEvent") }
    {
        return status;
    }
    // SAFETY: Full caller-owned writable record was validated above.
    let output = unsafe { &mut *output };
    if output.version != 0 && output.version != INKPOD_INKSCRIPT_RECORD_VERSION {
        return fail(
            INKPOD_STATUS_INCOMPATIBLE_ABI,
            "InkpodInkScriptTaskEvent.version is not exact-current",
        );
    }
    if output.feature_flags != 0 {
        return fail(
            INKPOD_STATUS_UNSUPPORTED,
            "InkpodInkScriptTaskEvent has unsupported feature flags",
        );
    }
    *output = InkpodInkScriptTaskEvent {
        struct_size: size_of::<InkpodInkScriptTaskEvent>() as u32,
        version: INKPOD_INKSCRIPT_RECORD_VERSION,
        kind: event.kind,
        task_state: event.task_state,
        feature_flags: 0,
        ordinal: event.ordinal,
        completed_items: event.completed,
        total_items: event.total,
        wait_milliseconds: event.wait_milliseconds,
        outcome: event.outcome,
        failure: event.failure,
        reserved: 0,
    };
    INKPOD_STATUS_OK
}

/// # Safety
/// `core` and `program` must be live, owner-thread handles from the same Core generation. `output`
/// and any advertised record/text storage must be valid, aligned, initialized, and writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_program_path_intents_copy(
    core: *mut InkpodCore,
    program: *const InkpodInkScriptProgram,
    output: *mut InkpodInkScriptPathIntentBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if program.is_null() || !is_aligned(program) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript program is null or misaligned",
            );
        }
        // SAFETY: Program is a live opaque handle synchronized against release.
        let program = unsafe { &*program };
        if let Err(status) = validate_route(program.owner_thread, program.core_generation, core) {
            return status;
        }
        // SAFETY: Public buffer exposes a readable size prefix.
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodInkScriptPathIntentBuffer") }
        {
            return status;
        }
        // SAFETY: Complete caller-owned output was validated above.
        let output = unsafe { &mut *output };
        if output.version != INKPOD_INKSCRIPT_RECORD_VERSION {
            return fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodInkScriptPathIntentBuffer.version is not exact-current",
            );
        }
        if output.feature_flags != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "InkpodInkScriptPathIntentBuffer has unsupported feature flags",
            );
        }
        let intents = program.program.path_intents();
        let required_utf8 = intents.iter().fold(0_u64, |total, intent| {
            total
                .saturating_add(intent.text().len() as u64)
                .saturating_add(match intent.subject() {
                    ScriptPathIntentSubject::Asset(symbol) => symbol.len() as u64,
                    ScriptPathIntentSubject::Input(_) | ScriptPathIntentSubject::OutputRoot => 0,
                })
        });
        output.records_written = 0;
        output.utf8_written_bytes = 0;
        output.required_records = intents.len() as u64;
        output.required_utf8_bytes = required_utf8;
        if output.record_capacity < intents.len() as u64
            || output.utf8_capacity_bytes < required_utf8
            || (!intents.is_empty() && output.records.is_null())
            || (required_utf8 != 0 && output.utf8.is_null())
            || (!intents.is_empty()
                && (output.record_stride_bytes < size_of::<InkpodInkScriptPathIntent>() as u64
                    || output.record_stride_bytes % align_of::<InkpodInkScriptPathIntent>() as u64
                        != 0))
        {
            return INKPOD_STATUS_BUFFER_TOO_SMALL;
        }
        let stride = match usize::try_from(output.record_stride_bytes) {
            Ok(value) => value,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "InkScript intent stride overflow",
                );
            }
        };
        // SAFETY: Capacity, pointer, stride, and full record size were checked above. Validate the
        // complete caller-initialized span before changing either output buffer.
        if let Err(status) = unsafe {
            validate_output_records(
                output.records,
                intents.len(),
                stride,
                "InkpodInkScriptPathIntent",
                |record, stride| {
                    if record.struct_size as usize > stride {
                        return Err(fail(
                            INKPOD_STATUS_INCOMPATIBLE_ABI,
                            "InkpodInkScriptPathIntent.struct_size exceeds its stride",
                        ));
                    }
                    if record.version != INKPOD_INKSCRIPT_RECORD_VERSION {
                        return Err(fail(
                            INKPOD_STATUS_INCOMPATIBLE_ABI,
                            "InkpodInkScriptPathIntent.version is not exact-current",
                        ));
                    }
                    if record.feature_flags != 0 {
                        return Err(fail(
                            INKPOD_STATUS_UNSUPPORTED,
                            "InkpodInkScriptPathIntent has unsupported feature flags",
                        ));
                    }
                    Ok(())
                },
            )
        } {
            return status;
        }
        let mut packed = Vec::new();
        if packed.try_reserve_exact(required_utf8 as usize).is_err() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript intent buffer allocation failed",
            );
        }
        let mut records = Vec::with_capacity(intents.len());
        for intent in intents {
            let text_offset = packed.len() as u64;
            packed.extend_from_slice(intent.text().as_bytes());
            let (subject_kind, subject_index, subject_offset, subject_bytes) =
                match intent.subject() {
                    ScriptPathIntentSubject::Input(index) => {
                        (INKPOD_INKSCRIPT_INTENT_INPUT, *index as u64, 0, 0)
                    }
                    ScriptPathIntentSubject::Asset(symbol) => {
                        let offset = packed.len() as u64;
                        packed.extend_from_slice(symbol.as_bytes());
                        (
                            INKPOD_INKSCRIPT_INTENT_ASSET,
                            0,
                            offset,
                            symbol.len() as u64,
                        )
                    }
                    ScriptPathIntentSubject::OutputRoot => {
                        (INKPOD_INKSCRIPT_INTENT_OUTPUT_ROOT, 0, 0, 0)
                    }
                };
            records.push(InkpodInkScriptPathIntent {
                struct_size: size_of::<InkpodInkScriptPathIntent>() as u32,
                version: INKPOD_INKSCRIPT_RECORD_VERSION,
                access: access_to_abi(intent.access()),
                subject_kind,
                feature_flags: 0,
                intent_id: intent.id(),
                subject_index,
                text_offset,
                text_bytes: intent.text().len() as u64,
                subject_offset,
                subject_bytes,
            });
        }
        for (index, record) in records.iter().enumerate() {
            // SAFETY: Caller advertises a writable validated strided record span.
            unsafe {
                output
                    .records
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodInkScriptPathIntent>()
                    .write(*record);
            }
        }
        if !packed.is_empty() {
            // SAFETY: Caller advertises a writable packed UTF-8 buffer of sufficient capacity.
            unsafe { ptr::copy_nonoverlapping(packed.as_ptr(), output.utf8, packed.len()) };
        }
        output.records_written = records.len() as u64;
        output.utf8_written_bytes = packed.len() as u64;
        INKPOD_STATUS_OK
    })
}

/// # Safety
/// `core` and `program` must be live owner-thread handles. `request` and its nested spans must be
/// readable for the call, `out_task` uniquely writable, and the host context must outlive the task.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_plan_task_create(
    core: *mut InkpodCore,
    program: *const InkpodInkScriptProgram,
    request: *const InkpodInkScriptPlanTaskRequest,
    out_task: *mut *mut InkpodInkScriptPlanTask,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_task.is_null() || !is_aligned(out_task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript plan-task owner is null or misaligned",
            );
        }
        // SAFETY: Caller supplies readable unique owner storage.
        if !unsafe { out_task.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript plan-task output already owns a handle",
            );
        }
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if program.is_null() || !is_aligned(program) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript program is null or misaligned",
            );
        }
        // SAFETY: Program is a live opaque handle synchronized against release.
        let program = unsafe { &*program };
        if let Err(status) = validate_route(program.owner_thread, program.core_generation, core) {
            return status;
        }
        // SAFETY: Public request exposes a readable size prefix.
        if let Err(status) = unsafe { validate_struct(request, "InkpodInkScriptPlanTaskRequest") } {
            return status;
        }
        // SAFETY: Full request was validated above and is borrowed only for this call.
        let request = unsafe { &*request };
        if request.version != INKPOD_INKSCRIPT_RECORD_VERSION {
            return fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodInkScriptPlanTaskRequest.version is not exact-current",
            );
        }
        if request.feature_flags != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "InkpodInkScriptPlanTaskRequest has unsupported feature flags",
            );
        }
        if request.controller_id == 0
            || request.session_generation == 0
            || request.authority_generation == 0
            || request.open_session_set_generation == 0
            || request.controller_id != program.controller_id
            || request.session_generation != program.session_generation
        {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript plan-task route or generation is stale",
            );
        }
        let host = match HostBridge::from_record(&request.host) {
            Ok(host) => host,
            Err(status) => return status,
        };
        // SAFETY: The bounded grant span and nested paths are copied before return.
        let grants = match unsafe { authority_grants(request) } {
            Ok(grants) => grants,
            Err(status) => return status,
        };
        let script_path = if request.script_path.is_null() {
            None
        } else {
            // SAFETY: Optional nested path is copied during this call.
            match unsafe { path_from_record(request.script_path) } {
                Ok(path) => Some(path),
                Err(status) => return status,
            }
        };
        let task = Box::new(InkpodInkScriptPlanTask {
            owner_thread: thread::current().id(),
            core_generation: core.objects.generation(),
            controller_id: request.controller_id,
            session_generation: request.session_generation,
            authority_generation: request.authority_generation,
            open_session_set_generation: request.open_session_set_generation,
            program: program.program.clone(),
            grants,
            script_path,
            maximum_folder_entries: request.maximum_folder_entries,
            host,
            state: AtomicU32::new(INKPOD_TASK_READY),
            cancelled: AtomicBool::new(false),
            completed_work: AtomicU64::new(0),
            total_work: AtomicU64::new(1),
            owner_data: UnsafeCell::new(PlanTaskOwnerData {
                terminal_status: INKPOD_STATUS_INVALID_STATE,
                pending_event: None,
                plan: None,
            }),
        });
        // SAFETY: Output storage is unique and currently null.
        unsafe { out_task.write(Box::into_raw(task)) };
        INKPOD_STATUS_OK
    })
}

/// # Safety
/// `task` must remain live and synchronized against release; `output` must be initialized, aligned,
/// and uniquely writable for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_inkscript_plan_task_query(
    task: *const InkpodInkScriptPlanTask,
    output: *mut InkpodTaskInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript plan task is null or misaligned",
            );
        }
        // SAFETY: Query reads atomics only; caller synchronizes against release.
        let task = unsafe { &*task };
        task_info(&task.state, &task.completed_work, &task.total_work, output)
    })
}

/// # Safety
/// `task` must remain live and synchronized against concurrent release.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_inkscript_plan_task_cancel(
    task: *const InkpodInkScriptPlanTask,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript plan task is null or misaligned",
            );
        }
        // SAFETY: Cancellation touches only the task's atomic flag.
        unsafe { &*task }.cancelled.store(true, Ordering::Release);
        INKPOD_STATUS_OK
    })
}

/// # Safety
/// `core` and `task` must be live, matching-generation handles used exclusively on their owner
/// thread. The task's borrowed host callback context must remain valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_plan_task_advance(
    core: *mut InkpodCore,
    task: *mut InkpodInkScriptPlanTask,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript plan task is null or misaligned",
            );
        }
        // SAFETY: The live allocation remains synchronized against release. Mutable owner data is
        // accessed only after validating the recorded owner thread.
        let task = unsafe { &*task };
        if let Err(status) = validate_route(task.owner_thread, task.core_generation, core) {
            return status;
        }
        // SAFETY: Route validation above proves this is the only thread allowed to access
        // `owner_data`; query/cancel touch only disjoint atomics.
        let owner_data = unsafe { &mut *task.owner_data.get() };
        if owner_data.pending_event.is_some() {
            return fail(
                INKPOD_STATUS_QUEUE_FULL,
                "InkScript plan-task event queue is full",
            );
        }
        if task.state.load(Ordering::Acquire) != INKPOD_TASK_READY {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript plan task is not ready",
            );
        }
        task.state.store(INKPOD_TASK_RUNNING, Ordering::Release);
        let result = plan_once(task);
        let status = match result {
            Ok(plan) => {
                owner_data.plan = Some(plan);
                task.completed_work.store(1, Ordering::Release);
                task.state.store(INKPOD_TASK_COMPLETED, Ordering::Release);
                INKPOD_STATUS_OK
            }
            Err(INKPOD_STATUS_CANCELLED) => {
                task.state.store(INKPOD_TASK_CANCELLED, Ordering::Release);
                INKPOD_STATUS_CANCELLED
            }
            Err(status) => {
                task.state.store(INKPOD_TASK_FAILED, Ordering::Release);
                status
            }
        };
        owner_data.terminal_status = status;
        owner_data.pending_event = Some(TaskEventData {
            kind: INKPOD_INKSCRIPT_EVENT_PLAN_COMPLETE,
            task_state: task.state.load(Ordering::Acquire),
            ordinal: 0,
            completed: task.completed_work.load(Ordering::Acquire),
            total: 1,
            wait_milliseconds: 0,
            outcome: 0,
            failure: 0,
        });
        status
    })
}

/// # Safety
/// `core` and `task` must be live owner-thread handles from the same generation. `output` must be an
/// initialized, aligned, uniquely writable event record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_plan_task_event_take(
    core: *mut InkpodCore,
    task: *mut InkpodInkScriptPlanTask,
    output: *mut InkpodInkScriptTaskEvent,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript plan task is null or misaligned",
            );
        }
        // SAFETY: The live allocation remains synchronized against release. Mutable owner data is
        // accessed only after validating the recorded owner thread.
        let task = unsafe { &*task };
        if let Err(status) = validate_route(task.owner_thread, task.core_generation, core) {
            return status;
        }
        // SAFETY: Route validation proves exclusive owner-thread access to this cell.
        let owner_data = unsafe { &mut *task.owner_data.get() };
        let Some(event) = owner_data.pending_event else {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript plan-task event queue is empty",
            );
        };
        let status = write_event(event, output);
        if status == INKPOD_STATUS_OK {
            owner_data.pending_event = None;
        }
        status
    })
}

/// # Safety
/// `core` and `task` must be live owner-thread handles from the same generation, and `out_plan`
/// must be uniquely writable owner storage containing no live plan.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_plan_task_take_plan(
    core: *mut InkpodCore,
    task: *mut InkpodInkScriptPlanTask,
    out_plan: *mut *mut InkpodInkScriptPlan,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_plan.is_null() || !is_aligned(out_plan) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript plan owner is null or misaligned",
            );
        }
        // SAFETY: Caller supplies unique owner storage.
        if !unsafe { out_plan.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript plan output already owns a handle",
            );
        }
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript plan task is null or misaligned",
            );
        }
        // SAFETY: The live allocation remains synchronized against release. Mutable owner data is
        // accessed only after validating the recorded owner thread.
        let task = unsafe { &*task };
        if let Err(status) = validate_route(task.owner_thread, task.core_generation, core) {
            return status;
        }
        // SAFETY: Route validation proves exclusive owner-thread access to this cell.
        let owner_data = unsafe { &mut *task.owner_data.get() };
        if owner_data.terminal_status != INKPOD_STATUS_OK {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript plan task did not complete successfully",
            );
        }
        let Some(plan) = owner_data.plan.take() else {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript plan was already taken",
            );
        };
        let plan = Box::new(InkpodInkScriptPlan {
            owner_thread: task.owner_thread,
            core_generation: task.core_generation,
            controller_id: task.controller_id,
            session_generation: task.session_generation,
            plan,
        });
        // SAFETY: Output owner is unique and null.
        unsafe { out_plan.write(Box::into_raw(plan)) };
        INKPOD_STATUS_OK
    })
}

/// # Safety
/// `core` must be the live parent of `*owner`; `owner` must be unique, aligned owner storage used on
/// the Core owner thread and synchronized against query/cancel calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_plan_task_release(
    core: *mut InkpodCore,
    owner: *mut *mut InkpodInkScriptPlanTask,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if owner.is_null() || !is_aligned(owner) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript plan-task owner is null or misaligned",
            );
        }
        // SAFETY: Caller supplies unique owner storage synchronized against task use.
        let pointer = unsafe { owner.read() };
        if pointer.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(pointer) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript plan-task handle is misaligned",
            );
        }
        // SAFETY: Live owner handle is inspected before exactly-once release.
        let task = unsafe { &*pointer };
        if let Err(status) = validate_route(task.owner_thread, task.core_generation, core) {
            return status;
        }
        // SAFETY: Unique Rust owner is consumed and caller storage is cleared.
        unsafe {
            drop(Box::from_raw(pointer));
            owner.write(ptr::null_mut());
        }
        INKPOD_STATUS_OK
    })
}

fn validate_plan<'a>(
    core: &InkpodCore,
    pointer: *const InkpodInkScriptPlan,
) -> Result<&'a InkpodInkScriptPlan, u32> {
    if pointer.is_null() || !is_aligned(pointer) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript plan is null or misaligned",
        ));
    }
    // SAFETY: Caller owns a live plan synchronized against release.
    let plan = unsafe { &*pointer };
    validate_route(plan.owner_thread, plan.core_generation, core)?;
    Ok(plan)
}

/// # Safety
/// `core` and `plan` must be live matching-generation handles on their owner thread, and `output`
/// must be initialized, aligned, and uniquely writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_plan_summary(
    core: *mut InkpodCore,
    plan: *const InkpodInkScriptPlan,
    output: *mut InkpodInkScriptPlanSummary,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let plan = match validate_plan(core, plan) {
            Ok(plan) => plan,
            Err(status) => return status,
        };
        // SAFETY: Public output exposes a readable size prefix.
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodInkScriptPlanSummary") }
        {
            return status;
        }
        // SAFETY: Complete caller-owned output was validated above.
        unsafe {
            output.write(InkpodInkScriptPlanSummary {
                struct_size: size_of::<InkpodInkScriptPlanSummary>() as u32,
                version: INKPOD_INKSCRIPT_RECORD_VERSION,
                feature_flags: 0,
                controller_id: plan.controller_id,
                session_generation: plan.session_generation,
                core_generation: plan.core_generation,
                plan_digest: plan.plan.plan_digest(),
                item_count: plan.plan.input_count() as u64,
            });
        }
        INKPOD_STATUS_OK
    })
}

fn validate_preview_buffer(
    output: *mut InkpodInkScriptPreviewBuffer,
) -> Result<&'static mut InkpodInkScriptPreviewBuffer, u32> {
    // SAFETY: Public buffer exposes a readable size prefix.
    unsafe { validate_struct(output.cast_const(), "InkpodInkScriptPreviewBuffer")? };
    // SAFETY: Complete caller-owned output was validated above.
    let output = unsafe { &mut *output };
    if output.version != INKPOD_INKSCRIPT_RECORD_VERSION {
        return Err(fail(
            INKPOD_STATUS_INCOMPATIBLE_ABI,
            "InkpodInkScriptPreviewBuffer.version is not exact-current",
        ));
    }
    if output.feature_flags != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "InkpodInkScriptPreviewBuffer has unsupported feature flags",
        ));
    }
    Ok(output)
}

/// # Safety
/// `core` and `plan` must be live matching-generation handles on their owner thread. `output` and
/// its advertised initialized record/text storage must be valid, aligned, and writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_plan_preview_copy(
    core: *mut InkpodCore,
    plan: *const InkpodInkScriptPlan,
    output: *mut InkpodInkScriptPreviewBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let plan = match validate_plan(core, plan) {
            Ok(plan) => plan,
            Err(status) => return status,
        };
        let output = match validate_preview_buffer(output) {
            Ok(output) => output,
            Err(status) => return status,
        };
        let Ok(first) = usize::try_from(output.first_item) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript preview offset overflow",
            );
        };
        if first > plan.plan.preview_items().len() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript preview offset is out of range",
            );
        }
        let items = &plan.plan.preview_items()[first..];
        let required_utf8 = items.iter().fold(0_u64, |total, item| {
            total
                .saturating_add(item.display_label().len() as u64)
                .saturating_add(item.output_name().len() as u64)
                .saturating_add(item.destination_key().len() as u64)
        });
        output.records_written = 0;
        output.utf8_written_bytes = 0;
        output.required_records = items.len() as u64;
        output.required_utf8_bytes = required_utf8;
        if output.record_capacity < items.len() as u64
            || output.utf8_capacity_bytes < required_utf8
            || (!items.is_empty() && output.records.is_null())
            || (required_utf8 != 0 && output.utf8.is_null())
            || (!items.is_empty()
                && (output.record_stride_bytes < size_of::<InkpodInkScriptPreviewItem>() as u64
                    || output.record_stride_bytes
                        % align_of::<InkpodInkScriptPreviewItem>() as u64
                        != 0))
        {
            return INKPOD_STATUS_BUFFER_TOO_SMALL;
        }
        let stride = match usize::try_from(output.record_stride_bytes) {
            Ok(value) => value,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "InkScript preview stride overflow",
                );
            }
        };
        // SAFETY: Capacity, pointer, stride, and full record size were checked above. Validate the
        // complete caller-initialized span before changing either output buffer.
        if let Err(status) = unsafe {
            validate_output_records(
                output.records,
                items.len(),
                stride,
                "InkpodInkScriptPreviewItem",
                |record, stride| {
                    if record.struct_size as usize > stride {
                        return Err(fail(
                            INKPOD_STATUS_INCOMPATIBLE_ABI,
                            "InkpodInkScriptPreviewItem.struct_size exceeds its stride",
                        ));
                    }
                    if record.version != INKPOD_INKSCRIPT_RECORD_VERSION {
                        return Err(fail(
                            INKPOD_STATUS_INCOMPATIBLE_ABI,
                            "InkpodInkScriptPreviewItem.version is not exact-current",
                        ));
                    }
                    if record.feature_flags != 0 {
                        return Err(fail(
                            INKPOD_STATUS_UNSUPPORTED,
                            "InkpodInkScriptPreviewItem has unsupported feature flags",
                        ));
                    }
                    Ok(())
                },
            )
        } {
            return status;
        }
        let mut packed = Vec::new();
        if packed.try_reserve_exact(required_utf8 as usize).is_err() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript preview buffer allocation failed",
            );
        }
        let mut records = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let input_offset = packed.len() as u64;
            packed.extend_from_slice(item.display_label().as_bytes());
            let output_offset = packed.len() as u64;
            packed.extend_from_slice(item.output_name().as_bytes());
            let destination_offset = packed.len() as u64;
            packed.extend_from_slice(item.destination_key().as_bytes());
            records.push(InkpodInkScriptPreviewItem {
                struct_size: size_of::<InkpodInkScriptPreviewItem>() as u32,
                version: INKPOD_INKSCRIPT_RECORD_VERSION,
                feature_flags: 0,
                ordinal: (first + index) as u64,
                input_offset,
                input_bytes: item.display_label().len() as u64,
                output_offset,
                output_bytes: item.output_name().len() as u64,
                destination_offset,
                destination_bytes: item.destination_key().len() as u64,
            });
        }
        for (index, record) in records.iter().enumerate() {
            // SAFETY: Caller advertises a writable validated strided record span.
            unsafe {
                output
                    .records
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodInkScriptPreviewItem>()
                    .write(*record);
            }
        }
        if !packed.is_empty() {
            // SAFETY: Caller advertises a writable packed UTF-8 buffer of sufficient capacity.
            unsafe { ptr::copy_nonoverlapping(packed.as_ptr(), output.utf8, packed.len()) };
        }
        output.records_written = records.len() as u64;
        output.utf8_written_bytes = packed.len() as u64;
        INKPOD_STATUS_OK
    })
}

/// # Safety
/// `core` must be the live parent of `*owner`; `owner` must be unique, aligned owner storage used on
/// the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_plan_release(
    core: *mut InkpodCore,
    owner: *mut *mut InkpodInkScriptPlan,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if owner.is_null() || !is_aligned(owner) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript plan owner is null or misaligned",
            );
        }
        // SAFETY: Caller supplies unique owner storage.
        let pointer = unsafe { owner.read() };
        if pointer.is_null() {
            return INKPOD_STATUS_OK;
        }
        let plan = match validate_plan(core, pointer) {
            Ok(plan) => plan,
            Err(status) => return status,
        };
        let _ = plan;
        // SAFETY: Unique Rust owner is consumed and caller storage is cleared.
        unsafe {
            drop(Box::from_raw(pointer));
            owner.write(ptr::null_mut());
        }
        INKPOD_STATUS_OK
    })
}

/// # Safety
/// `core` and `plan` must be live matching-generation handles on their owner thread. `request` must
/// be readable and `out_confirmation` uniquely writable owner storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_confirmation_create(
    core: *mut InkpodCore,
    plan: *const InkpodInkScriptPlan,
    request: *const InkpodInkScriptConfirmationRequest,
    out_confirmation: *mut *mut InkpodInkScriptConfirmation,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_confirmation.is_null() || !is_aligned(out_confirmation) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript confirmation owner is null or misaligned",
            );
        }
        // SAFETY: Caller supplies unique owner storage.
        if !unsafe { out_confirmation.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript confirmation output already owns a handle",
            );
        }
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let plan = match validate_plan(core, plan) {
            Ok(plan) => plan,
            Err(status) => return status,
        };
        // SAFETY: Public request exposes a readable size prefix.
        if let Err(status) =
            unsafe { validate_struct(request, "InkpodInkScriptConfirmationRequest") }
        {
            return status;
        }
        // SAFETY: Complete request was validated above.
        let request = unsafe { &*request };
        if request.version != INKPOD_INKSCRIPT_RECORD_VERSION || request.reserved != 0 {
            return fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodInkScriptConfirmationRequest is not exact-current",
            );
        }
        if request.feature_flags != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "InkpodInkScriptConfirmationRequest has unsupported feature flags",
            );
        }
        let scope = match request.scope {
            INKPOD_INKSCRIPT_SCOPE_ALL => ScriptRunScope::All,
            INKPOD_INKSCRIPT_SCOPE_CURRENT_DOCUMENT => {
                let uuid = u128::from(request.document_uuid_low)
                    | (u128::from(request.document_uuid_high) << 64);
                if uuid == 0 {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "InkScript current-document scope has a zero UUID",
                    );
                }
                ScriptRunScope::CurrentDocument(uuid)
            }
            INKPOD_INKSCRIPT_SCOPE_CURRENT_FILE => {
                if request.file_alias == [0; 32] {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "InkScript current-file scope has a zero alias",
                    );
                }
                ScriptRunScope::CurrentFile(request.file_alias)
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "InkScript run scope is unknown",
                );
            }
        };
        let token = match issue_confirmation_token(&plan.plan, scope) {
            Ok(token) => token,
            Err(error) => return map_plan_error(error),
        };
        let confirmation = Box::new(InkpodInkScriptConfirmation {
            owner_thread: plan.owner_thread,
            core_generation: plan.core_generation,
            controller_id: plan.controller_id,
            session_generation: plan.session_generation,
            plan_digest: plan.plan.plan_digest(),
            token,
        });
        // SAFETY: Output owner is unique and null.
        unsafe { out_confirmation.write(Box::into_raw(confirmation)) };
        INKPOD_STATUS_OK
    })
}

fn validate_confirmation<'a>(
    core: &InkpodCore,
    pointer: *const InkpodInkScriptConfirmation,
) -> Result<&'a InkpodInkScriptConfirmation, u32> {
    if pointer.is_null() || !is_aligned(pointer) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript confirmation is null or misaligned",
        ));
    }
    // SAFETY: Caller owns a live confirmation synchronized against release.
    let value = unsafe { &*pointer };
    validate_route(value.owner_thread, value.core_generation, core)?;
    Ok(value)
}

/// # Safety
/// `core` must be the live parent of `*owner`; `owner` must be unique, aligned owner storage used on
/// the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_confirmation_release(
    core: *mut InkpodCore,
    owner: *mut *mut InkpodInkScriptConfirmation,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if owner.is_null() || !is_aligned(owner) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript confirmation owner is null or misaligned",
            );
        }
        // SAFETY: Caller supplies unique owner storage.
        let pointer = unsafe { owner.read() };
        if pointer.is_null() {
            return INKPOD_STATUS_OK;
        }
        if let Err(status) = validate_confirmation(core, pointer) {
            return status;
        }
        // SAFETY: Unique Rust owner is consumed and caller storage is cleared.
        unsafe {
            drop(Box::from_raw(pointer));
            owner.write(ptr::null_mut());
        }
        INKPOD_STATUS_OK
    })
}

fn outcome_to_abi(value: &ScriptItemOutcome) -> (u32, u32) {
    match value {
        ScriptItemOutcome::NotStarted => (INKPOD_INKSCRIPT_OUTCOME_NOT_STARTED, 0),
        ScriptItemOutcome::Installed => (INKPOD_INKSCRIPT_OUTCOME_INSTALLED, 0),
        ScriptItemOutcome::DryRun => (INKPOD_INKSCRIPT_OUTCOME_DRY_RUN, 0),
        ScriptItemOutcome::Cancelled => (INKPOD_INKSCRIPT_OUTCOME_CANCELLED, 0),
        ScriptItemOutcome::Failed(failure) => {
            let failure = match failure {
                ScriptItemFailure::StaleAuthority => INKPOD_INKSCRIPT_FAILURE_STALE_AUTHORITY,
                ScriptItemFailure::StaleSession => INKPOD_INKSCRIPT_FAILURE_STALE_SESSION,
                ScriptItemFailure::StaleInput => INKPOD_INKSCRIPT_FAILURE_STALE_INPUT,
                ScriptItemFailure::StaleDestination => INKPOD_INKSCRIPT_FAILURE_STALE_DESTINATION,
                ScriptItemFailure::UnsupportedAtomicInstall => {
                    INKPOD_INKSCRIPT_FAILURE_UNSUPPORTED_INSTALL
                }
                ScriptItemFailure::UnsupportedAtomicOverwrite => {
                    INKPOD_INKSCRIPT_FAILURE_UNSUPPORTED_OVERWRITE
                }
                ScriptItemFailure::Decode => INKPOD_INKSCRIPT_FAILURE_DECODE,
                ScriptItemFailure::Execute => INKPOD_INKSCRIPT_FAILURE_EXECUTE,
                ScriptItemFailure::Encode => INKPOD_INKSCRIPT_FAILURE_ENCODE,
                ScriptItemFailure::Save => INKPOD_INKSCRIPT_FAILURE_SAVE,
                ScriptItemFailure::ResourceLimit => INKPOD_INKSCRIPT_FAILURE_RESOURCE,
                ScriptItemFailure::Adapter => INKPOD_INKSCRIPT_FAILURE_ADAPTER,
            };
            (INKPOD_INKSCRIPT_OUTCOME_FAILED, failure)
        }
    }
}

/// # Safety
/// All handles and owner slots must be live, aligned, same-generation values on the Core owner
/// thread. `request` must be readable, `out_task` uniquely writable, and its host context must
/// outlive the task. Plan and confirmation owner slots are consumed only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_run_task_create(
    core: *mut InkpodCore,
    program: *const InkpodInkScriptProgram,
    plan_owner: *mut *mut InkpodInkScriptPlan,
    confirmation_owner: *mut *mut InkpodInkScriptConfirmation,
    request: *const InkpodInkScriptRunRequest,
    out_task: *mut *mut InkpodInkScriptRunTask,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_task.is_null() || !is_aligned(out_task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript run-task owner is null or misaligned",
            );
        }
        // SAFETY: Caller supplies unique owner storage.
        if !unsafe { out_task.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript run-task output already owns a handle",
            );
        }
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if program.is_null() || !is_aligned(program) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript program is null or misaligned",
            );
        }
        // SAFETY: Program is a live opaque handle synchronized against release.
        let program = unsafe { &*program };
        if let Err(status) = validate_route(program.owner_thread, program.core_generation, core) {
            return status;
        }
        if plan_owner.is_null()
            || !is_aligned(plan_owner)
            || confirmation_owner.is_null()
            || !is_aligned(confirmation_owner)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript run ownership inputs are null or misaligned",
            );
        }
        // SAFETY: Caller supplies live unique owner variables.
        let plan_pointer = unsafe { plan_owner.read() };
        // SAFETY: Caller supplies live unique owner variables.
        let confirmation_pointer = unsafe { confirmation_owner.read() };
        let plan = match validate_plan(core, plan_pointer) {
            Ok(plan) => plan,
            Err(status) => return status,
        };
        let confirmation = match validate_confirmation(core, confirmation_pointer) {
            Ok(value) => value,
            Err(status) => return status,
        };
        // SAFETY: Public request exposes a readable size prefix.
        if let Err(status) = unsafe { validate_struct(request, "InkpodInkScriptRunRequest") } {
            return status;
        }
        // SAFETY: Complete request was validated above.
        let request = unsafe { &*request };
        if request.version != INKPOD_INKSCRIPT_RECORD_VERSION || request.reserved != 0 {
            return fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodInkScriptRunRequest is not exact-current",
            );
        }
        if request.feature_flags != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "InkpodInkScriptRunRequest has unsupported feature flags",
            );
        }
        if request.controller_id != plan.controller_id
            || request.session_generation != plan.session_generation
            || confirmation.controller_id != plan.controller_id
            || confirmation.session_generation != plan.session_generation
            || confirmation.plan_digest != plan.plan.plan_digest()
            || program.controller_id != plan.controller_id
            || program.session_generation != plan.session_generation
        {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript run plan, token, program, or route is stale",
            );
        }
        let mode = match request.mode {
            INKPOD_INKSCRIPT_RUN_DRY => ScriptRunMode::DryRun,
            INKPOD_INKSCRIPT_RUN_INSTALL => ScriptRunMode::Install,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "InkScript run mode is unknown",
                );
            }
        };
        let host = match HostBridge::from_record(&request.host) {
            Ok(host) => host,
            Err(status) => return status,
        };
        let mut token = confirmation.token.clone();
        let limits = if request.maximum_output_bytes == 0 {
            ScriptRunLimits::exact_current()
        } else {
            ScriptRunLimits::exact_current().with_output_bytes(request.maximum_output_bytes)
        };
        let inner = match start_inkscript_run(
            &program.program,
            plan.plan.clone(),
            &mut token,
            mode,
            limits,
        ) {
            Ok(task) => task,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_STATE,
                    "InkScript run rejected a stale or consumed plan confirmation",
                );
            }
        };
        let total = inner.total_items() as u64;
        let task = Box::new(InkpodInkScriptRunTask {
            owner_thread: plan.owner_thread,
            core_generation: plan.core_generation,
            state: AtomicU32::new(INKPOD_TASK_READY),
            cancelled: AtomicBool::new(false),
            completed_work: AtomicU64::new(0),
            total_work: AtomicU64::new(total),
            owner_data: UnsafeCell::new(RunTaskOwnerData {
                terminal_status: INKPOD_STATUS_INVALID_STATE,
                pending_event: None,
                task: inner,
                adapter: HostRunAdapter { host },
                report: None,
            }),
        });
        // SAFETY: Validation and construction succeeded; consume both unique owners atomically.
        unsafe {
            drop(Box::from_raw(plan_pointer));
            drop(Box::from_raw(confirmation_pointer));
            plan_owner.write(ptr::null_mut());
            confirmation_owner.write(ptr::null_mut());
            out_task.write(Box::into_raw(task));
        }
        INKPOD_STATUS_OK
    })
}

/// # Safety
/// `task` must remain live and synchronized against release; `output` must be initialized, aligned,
/// and uniquely writable for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_inkscript_run_task_query(
    task: *const InkpodInkScriptRunTask,
    output: *mut InkpodTaskInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript run task is null or misaligned",
            );
        }
        // SAFETY: Query reads atomics only; caller synchronizes against release.
        let task = unsafe { &*task };
        task_info(&task.state, &task.completed_work, &task.total_work, output)
    })
}

/// # Safety
/// `task` must remain live and synchronized against concurrent release.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_inkscript_run_task_cancel(
    task: *const InkpodInkScriptRunTask,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript run task is null or misaligned",
            );
        }
        // SAFETY: Cancellation touches only the task's atomic flag.
        unsafe { &*task }.cancelled.store(true, Ordering::Release);
        INKPOD_STATUS_OK
    })
}

/// # Safety
/// `core` and `task` must be live, matching-generation handles used exclusively on their owner
/// thread. The task's borrowed host callback context and nested response data must remain valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_run_task_advance(
    core: *mut InkpodCore,
    task: *mut InkpodInkScriptRunTask,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript run task is null or misaligned",
            );
        }
        // SAFETY: The live allocation remains synchronized against release. Mutable owner data is
        // accessed only after validating the recorded owner thread.
        let task = unsafe { &*task };
        if let Err(status) = validate_route(task.owner_thread, task.core_generation, core) {
            return status;
        }
        // SAFETY: Route validation above proves this is the only thread allowed to access
        // `owner_data`; query/cancel touch only disjoint atomics.
        let owner_data = unsafe { &mut *task.owner_data.get() };
        if owner_data.pending_event.is_some() {
            return fail(
                INKPOD_STATUS_QUEUE_FULL,
                "InkScript run-task event queue is full",
            );
        }
        let state = task.state.load(Ordering::Acquire);
        if matches!(
            state,
            INKPOD_TASK_COMPLETED | INKPOD_TASK_CANCELLED | INKPOD_TASK_FAILED
        ) {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript run task is already terminal",
            );
        }
        task.state.store(INKPOD_TASK_RUNNING, Ordering::Release);
        let cancelled = &task.cancelled;
        let mut poll = || cancelled.load(Ordering::Acquire);
        match owner_data.task.advance(&mut owner_data.adapter, &mut poll) {
            ScriptRunAdvance::ItemCompleted {
                ordinal,
                completed,
                total,
                outcome,
            } => {
                task.completed_work
                    .store(completed as u64, Ordering::Release);
                let (outcome, failure) = outcome_to_abi(&outcome);
                owner_data.pending_event = Some(TaskEventData {
                    kind: INKPOD_INKSCRIPT_EVENT_ITEM_COMPLETE,
                    task_state: INKPOD_TASK_RUNNING,
                    ordinal: ordinal as u64,
                    completed: completed as u64,
                    total: total as u64,
                    wait_milliseconds: 0,
                    outcome,
                    failure,
                });
                INKPOD_STATUS_OK
            }
            ScriptRunAdvance::WaitRequested { milliseconds } => {
                owner_data.pending_event = Some(TaskEventData {
                    kind: INKPOD_INKSCRIPT_EVENT_WAIT_REQUESTED,
                    task_state: INKPOD_TASK_RUNNING,
                    ordinal: 0,
                    completed: task.completed_work.load(Ordering::Acquire),
                    total: task.total_work.load(Ordering::Acquire),
                    wait_milliseconds: milliseconds,
                    outcome: 0,
                    failure: 0,
                });
                INKPOD_STATUS_OK
            }
            ScriptRunAdvance::Complete => {
                let report = match owner_data.task.finish() {
                    Ok(report) => report,
                    Err(_) => {
                        task.state.store(INKPOD_TASK_FAILED, Ordering::Release);
                        owner_data.terminal_status = INKPOD_STATUS_INVALID_STATE;
                        return fail(
                            INKPOD_STATUS_INVALID_STATE,
                            "InkScript run task completed without a report",
                        );
                    }
                };
                let status = if report.cancelled {
                    INKPOD_STATUS_CANCELLED
                } else {
                    INKPOD_STATUS_OK
                };
                let terminal_state = if report.cancelled {
                    INKPOD_TASK_CANCELLED
                } else {
                    INKPOD_TASK_COMPLETED
                };
                task.completed_work.store(
                    report
                        .items
                        .iter()
                        .filter(|item| !matches!(item.outcome, ScriptItemOutcome::NotStarted))
                        .count() as u64,
                    Ordering::Release,
                );
                owner_data.report = Some(report);
                owner_data.terminal_status = status;
                task.state.store(terminal_state, Ordering::Release);
                owner_data.pending_event = Some(TaskEventData {
                    kind: INKPOD_INKSCRIPT_EVENT_RUN_COMPLETE,
                    task_state: terminal_state,
                    ordinal: 0,
                    completed: task.completed_work.load(Ordering::Acquire),
                    total: task.total_work.load(Ordering::Acquire),
                    wait_milliseconds: 0,
                    outcome: 0,
                    failure: 0,
                });
                status
            }
        }
    })
}

/// # Safety
/// `core` and `task` must be live owner-thread handles from the same generation. `output` must be an
/// initialized, aligned, uniquely writable event record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_run_task_event_take(
    core: *mut InkpodCore,
    task: *mut InkpodInkScriptRunTask,
    output: *mut InkpodInkScriptTaskEvent,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript run task is null or misaligned",
            );
        }
        // SAFETY: The live allocation remains synchronized against release. Mutable owner data is
        // accessed only after validating the recorded owner thread.
        let task = unsafe { &*task };
        if let Err(status) = validate_route(task.owner_thread, task.core_generation, core) {
            return status;
        }
        // SAFETY: Route validation proves exclusive owner-thread access to this cell.
        let owner_data = unsafe { &mut *task.owner_data.get() };
        let Some(event) = owner_data.pending_event else {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript run-task event queue is empty",
            );
        };
        let status = write_event(event, output);
        if status == INKPOD_STATUS_OK {
            owner_data.pending_event = None;
        }
        status
    })
}

/// # Safety
/// `core` and `task` must be live owner-thread handles from the same generation, and `out_report`
/// must be uniquely writable owner storage containing no live report.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_run_task_take_report(
    core: *mut InkpodCore,
    task: *mut InkpodInkScriptRunTask,
    out_report: *mut *mut InkpodInkScriptReport,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_report.is_null() || !is_aligned(out_report) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript report owner is null or misaligned",
            );
        }
        // SAFETY: Caller supplies unique owner storage.
        if !unsafe { out_report.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript report output already owns a handle",
            );
        }
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript run task is null or misaligned",
            );
        }
        // SAFETY: The live allocation remains synchronized against release. Mutable owner data is
        // accessed only after validating the recorded owner thread.
        let task = unsafe { &*task };
        if let Err(status) = validate_route(task.owner_thread, task.core_generation, core) {
            return status;
        }
        // SAFETY: Route validation proves exclusive owner-thread access to this cell.
        let owner_data = unsafe { &mut *task.owner_data.get() };
        let Some(report) = owner_data.report.take() else {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript run report is unavailable or was already taken",
            );
        };
        // SAFETY: Output owner is unique and null.
        unsafe {
            out_report.write(Box::into_raw(Box::new(InkpodInkScriptReport { report })));
        }
        INKPOD_STATUS_OK
    })
}

/// # Safety
/// `core` must be the live parent of `*owner`; `owner` must be unique, aligned owner storage used on
/// the Core owner thread and synchronized against query/cancel calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_run_task_release(
    core: *mut InkpodCore,
    owner: *mut *mut InkpodInkScriptRunTask,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_execution_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if owner.is_null() || !is_aligned(owner) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript run-task owner is null or misaligned",
            );
        }
        // SAFETY: Caller supplies unique owner storage.
        let pointer = unsafe { owner.read() };
        if pointer.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(pointer) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript run-task handle is misaligned",
            );
        }
        // SAFETY: Live owner handle is inspected before exactly-once release.
        let task = unsafe { &*pointer };
        if let Err(status) = validate_route(task.owner_thread, task.core_generation, core) {
            return status;
        }
        // SAFETY: Unique Rust owner is consumed and caller storage is cleared.
        unsafe {
            drop(Box::from_raw(pointer));
            owner.write(ptr::null_mut());
        }
        INKPOD_STATUS_OK
    })
}

fn validate_report(
    pointer: *const InkpodInkScriptReport,
) -> Result<&'static InkpodInkScriptReport, u32> {
    if pointer.is_null() || !is_aligned(pointer) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript report is null or misaligned",
        ));
    }
    // SAFETY: Caller owns a live immutable report synchronized against release.
    Ok(unsafe { &*pointer })
}

/// # Safety
/// `report` must remain live and externally synchronized against release; `output` must be
/// initialized, aligned, and uniquely writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_inkscript_report_summary(
    report: *const InkpodInkScriptReport,
    output: *mut InkpodInkScriptReportSummary,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let report = match validate_report(report) {
            Ok(report) => report,
            Err(status) => return status,
        };
        // SAFETY: Public output exposes a readable size prefix.
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodInkScriptReportSummary") }
        {
            return status;
        }
        let flags = if report.report.dry_run {
            INKPOD_INKSCRIPT_REPORT_DRY_RUN
        } else {
            0
        } | if report.report.cancelled {
            INKPOD_INKSCRIPT_REPORT_CANCELLED
        } else {
            0
        };
        // SAFETY: Complete caller-owned output was validated above.
        unsafe {
            output.write(InkpodInkScriptReportSummary {
                struct_size: size_of::<InkpodInkScriptReportSummary>() as u32,
                version: INKPOD_INKSCRIPT_RECORD_VERSION,
                feature_flags: 0,
                flags,
                item_count: report.report.items.len() as u64,
                created_directory_count: report.report.created_directories.len() as u64,
            });
        }
        INKPOD_STATUS_OK
    })
}

/// # Safety
/// `report` must remain live and synchronized against release. `output` and its advertised
/// initialized record/text storage must be valid, aligned, and writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_inkscript_report_items_copy(
    report: *const InkpodInkScriptReport,
    output: *mut InkpodInkScriptReportBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let report = match validate_report(report) {
            Ok(report) => report,
            Err(status) => return status,
        };
        // SAFETY: Public buffer exposes a readable size prefix.
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodInkScriptReportBuffer") }
        {
            return status;
        }
        // SAFETY: Complete caller-owned output was validated above.
        let output = unsafe { &mut *output };
        if output.version != INKPOD_INKSCRIPT_RECORD_VERSION {
            return fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodInkScriptReportBuffer.version is not exact-current",
            );
        }
        if output.feature_flags != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "InkpodInkScriptReportBuffer has unsupported feature flags",
            );
        }
        let Ok(first) = usize::try_from(output.first_item) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript report offset overflow",
            );
        };
        if first > report.report.items.len() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript report offset is out of range",
            );
        }
        let items = &report.report.items[first..];
        let required_utf8 = items.iter().fold(0_u64, |total, item| {
            total
                .saturating_add(item.input_label.len() as u64)
                .saturating_add(item.destination_key.len() as u64)
        });
        output.records_written = 0;
        output.utf8_written_bytes = 0;
        output.required_records = items.len() as u64;
        output.required_utf8_bytes = required_utf8;
        if output.record_capacity < items.len() as u64
            || output.utf8_capacity_bytes < required_utf8
            || (!items.is_empty() && output.records.is_null())
            || (required_utf8 != 0 && output.utf8.is_null())
            || (!items.is_empty()
                && (output.record_stride_bytes < size_of::<InkpodInkScriptReportItem>() as u64
                    || output.record_stride_bytes % align_of::<InkpodInkScriptReportItem>() as u64
                        != 0))
        {
            return INKPOD_STATUS_BUFFER_TOO_SMALL;
        }
        let stride = match usize::try_from(output.record_stride_bytes) {
            Ok(value) => value,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "InkScript report stride overflow",
                );
            }
        };
        // SAFETY: Capacity, pointer, stride, and full record size were checked above. Validate the
        // complete caller-initialized span before changing either output buffer.
        if let Err(status) = unsafe {
            validate_output_records(
                output.records,
                items.len(),
                stride,
                "InkpodInkScriptReportItem",
                |record, stride| {
                    if record.struct_size as usize > stride {
                        return Err(fail(
                            INKPOD_STATUS_INCOMPATIBLE_ABI,
                            "InkpodInkScriptReportItem.struct_size exceeds its stride",
                        ));
                    }
                    if record.version != INKPOD_INKSCRIPT_RECORD_VERSION {
                        return Err(fail(
                            INKPOD_STATUS_INCOMPATIBLE_ABI,
                            "InkpodInkScriptReportItem.version is not exact-current",
                        ));
                    }
                    if record.feature_flags != 0 {
                        return Err(fail(
                            INKPOD_STATUS_UNSUPPORTED,
                            "InkpodInkScriptReportItem has unsupported feature flags",
                        ));
                    }
                    Ok(())
                },
            )
        } {
            return status;
        }
        let mut packed = Vec::new();
        if packed.try_reserve_exact(required_utf8 as usize).is_err() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript report buffer allocation failed",
            );
        }
        let mut records = Vec::with_capacity(items.len());
        for item in items {
            let input_offset = packed.len() as u64;
            packed.extend_from_slice(item.input_label.as_bytes());
            let destination_offset = packed.len() as u64;
            packed.extend_from_slice(item.destination_key.as_bytes());
            let (outcome, failure) = outcome_to_abi(&item.outcome);
            let (commit_count, final_revision, next_stable_id, final_state_digest) = item
                .execution
                .as_ref()
                .map(|execution| {
                    (
                        execution.commit_count(),
                        execution.final_revision(),
                        execution.next_stable_id(),
                        *execution.final_state_digest().as_bytes(),
                    )
                })
                .unwrap_or((0, 0, 0, [0; 32]));
            records.push(InkpodInkScriptReportItem {
                struct_size: size_of::<InkpodInkScriptReportItem>() as u32,
                version: INKPOD_INKSCRIPT_RECORD_VERSION,
                outcome,
                failure,
                feature_flags: 0,
                ordinal: item.ordinal as u64,
                input_offset,
                input_bytes: item.input_label.len() as u64,
                destination_offset,
                destination_bytes: item.destination_key.len() as u64,
                commit_count,
                final_revision,
                next_stable_id,
                final_state_digest,
            });
        }
        for (index, record) in records.iter().enumerate() {
            // SAFETY: Caller advertises a writable validated strided record span.
            unsafe {
                output
                    .records
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodInkScriptReportItem>()
                    .write(*record);
            }
        }
        if !packed.is_empty() {
            // SAFETY: Caller advertises a writable packed UTF-8 buffer of sufficient capacity.
            unsafe { ptr::copy_nonoverlapping(packed.as_ptr(), output.utf8, packed.len()) };
        }
        output.records_written = records.len() as u64;
        output.utf8_written_bytes = packed.len() as u64;
        INKPOD_STATUS_OK
    })
}

/// # Safety
/// `owner` must be unique, aligned owner storage synchronized against all report queries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_inkscript_report_release(
    owner: *mut *mut InkpodInkScriptReport,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if owner.is_null() || !is_aligned(owner) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript report owner is null or misaligned",
            );
        }
        // SAFETY: Caller supplies unique owner storage synchronized against report reads.
        let pointer = unsafe { owner.read() };
        if pointer.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(pointer) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "InkScript report handle is misaligned",
            );
        }
        // SAFETY: Unique Rust owner is consumed and caller storage is cleared.
        unsafe {
            drop(Box::from_raw(pointer));
            owner.write(ptr::null_mut());
        }
        INKPOD_STATUS_OK
    })
}
