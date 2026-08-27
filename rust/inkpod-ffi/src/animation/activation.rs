use super::*;
use inkpod_core::{SequenceActivationKind, SequenceActivationPlan};

/// Resolves an explicit sequence selection without changing the live Core.
///
/// # Safety
/// Core and output must be nonoverlapping, complete live owner-thread records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_activation_resolve(
    core: *mut InkpodCore,
    target_index: u32,
    out_plan: *mut InkpodSequenceActivationPlan,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(out_plan.cast_const(), "InkpodSequenceActivationPlan") }
        {
            return status;
        }
        // SAFETY: Complete live owner-thread storage is required by the caller.
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.resolve_sequence_activation(target_index as usize) {
            Ok(plan) => {
                // SAFETY: The complete writable output was validated above.
                write_activation_plan(unsafe { &mut *out_plan }, plan);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Revalidates and commits an explicit sequence selection on its owner thread.
///
/// # Safety
/// Core, input and output must be complete live, nonoverlapping records. The
/// input is borrowed only for this call; output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_activation_commit(
    core: *mut InkpodCore,
    plan: *const InkpodSequenceActivationPlan,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(plan, "InkpodSequenceActivationPlan") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: The complete borrowed input record was validated above.
        let plan = match parse_activation_plan(unsafe { &*plan }) {
            Ok(plan) => plan,
            Err(status) => return status,
        };
        // SAFETY: Caller owns this live Core and disjoint output record.
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.commit_sequence_activation(plan) {
            Ok(info) => {
                // SAFETY: The complete writable output was validated above.
                write_document_info(unsafe { &mut *out_info }, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

fn parse_activation_plan(
    input: &InkpodSequenceActivationPlan,
) -> Result<SequenceActivationPlan, u32> {
    if input.feature_flags != INKPOD_FEATURE_NONE {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "unknown activation plan features",
        ));
    }
    let kind = match input.result_class {
        INKPOD_SEQUENCE_ACTIVATION_NOOP => SequenceActivationKind::NoOp,
        INKPOD_SEQUENCE_ACTIVATION_BIND => SequenceActivationKind::Bind,
        INKPOD_SEQUENCE_ACTIVATION_REPLACE => SequenceActivationKind::Replace,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence activation kind is not defined",
            ));
        }
    };
    let source_uuid = (u128::from(input.source_document_uuid_high) << 64)
        | u128::from(input.source_document_uuid_low);
    let target_uuid = (u128::from(input.target_document_uuid_high) << 64)
        | u128::from(input.target_document_uuid_low);
    let unbound = input.source_index == INKPOD_SEQUENCE_INDEX_NONE;
    if input.sequence_revision == 0
        || source_uuid == 0
        || target_uuid == 0
        || input.source_document_revision == 0
        || input.source_editor_revision == 0
        || input.target_source_generation == 0
        || input.target_index == INKPOD_SEQUENCE_INDEX_NONE
        || unbound != (input.source_generation == 0)
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "sequence activation identity contains invalid values",
        ));
    }
    Ok(SequenceActivationPlan {
        kind,
        sequence_revision: input.sequence_revision,
        source_document_uuid: source_uuid,
        source_document_revision: input.source_document_revision,
        source_editor_revision: input.source_editor_revision,
        source_index: (!unbound).then_some(input.source_index),
        source_generation: (!unbound).then_some(input.source_generation),
        target_index: input.target_index,
        target_document_uuid: target_uuid,
        target_source_generation: input.target_source_generation,
    })
}

fn write_activation_plan(output: &mut InkpodSequenceActivationPlan, plan: SequenceActivationPlan) {
    output.result_class = match plan.kind {
        SequenceActivationKind::NoOp => INKPOD_SEQUENCE_ACTIVATION_NOOP,
        SequenceActivationKind::Bind => INKPOD_SEQUENCE_ACTIVATION_BIND,
        SequenceActivationKind::Replace => INKPOD_SEQUENCE_ACTIVATION_REPLACE,
    };
    output.feature_flags = INKPOD_FEATURE_NONE;
    output.sequence_revision = plan.sequence_revision;
    output.source_document_uuid_high = (plan.source_document_uuid >> 64) as u64;
    output.source_document_uuid_low = plan.source_document_uuid as u64;
    output.source_generation = plan.source_generation.unwrap_or(0);
    output.source_document_revision = plan.source_document_revision;
    output.source_editor_revision = plan.source_editor_revision;
    output.target_document_uuid_high = (plan.target_document_uuid >> 64) as u64;
    output.target_document_uuid_low = plan.target_document_uuid as u64;
    output.target_source_generation = plan.target_source_generation;
    output.source_index = plan.source_index.unwrap_or(INKPOD_SEQUENCE_INDEX_NONE);
    output.target_index = plan.target_index;
}
