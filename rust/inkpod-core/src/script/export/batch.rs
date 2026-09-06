//! Exact journal lifting for the expanded, single-transaction Batch invocation.

use std::collections::BTreeMap;

use super::{
    InkScriptExportError, StrictBindings, pixel_literal, poll, resolve_reference, uuid_literal,
};
use crate::{
    BATCH_OPERATION_VERSION, BatchMissingTargetPolicy, BatchOperation, BatchOperationKind,
    BatchTargetSelector, PlaneType,
};

pub(super) fn arguments(
    operations: &[BatchOperation],
    source_uuid: u128,
    produced: &BTreeMap<u64, String>,
    strict: &mut StrictBindings,
    maximum_bytes: usize,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<String, InkScriptExportError> {
    if operations.is_empty() {
        return Err(InkScriptExportError::InvalidSource);
    }
    let mut output = String::new();
    append(&mut output, "operations = [", maximum_bytes)?;
    for (index, operation) in operations.iter().enumerate() {
        poll(cancelled)?;
        // Canonical Batch payloads already contain only enabled, individually resolved targets.
        // UI grouping and disabled operations are deliberately not reconstructed from history.
        if operation.version != BATCH_OPERATION_VERSION
            || !operation.enabled
            || !operation.additional_targets.is_empty()
        {
            return Err(InkScriptExportError::InvalidSource);
        }
        if index != 0 {
            append(&mut output, ", ", maximum_bytes)?;
        }
        let target = target_literal(&operation.target, source_uuid, produced, strict)?;
        match &operation.kind {
            BatchOperationKind::ColorReplace(pairs) => {
                append(
                    &mut output,
                    &format!(
                        "{{ kind = color_replace; enabled = true; targets = [{target}]; pairs = ["
                    ),
                    maximum_bytes,
                )?;
                for (index, pair) in pairs.iter().enumerate() {
                    poll(cancelled)?;
                    if index != 0 {
                        append(&mut output, ", ", maximum_bytes)?;
                    }
                    append(
                        &mut output,
                        &format!(
                            "{{ enabled = {}; old = {}; new = {}; }}",
                            pair.enabled,
                            pixel_literal(pair.old),
                            pixel_literal(pair.new),
                        ),
                        maximum_bytes,
                    )?;
                }
            }
            BatchOperationKind::MoveToColorPlane(colors)
            | BatchOperationKind::Masking(colors)
            | BatchOperationKind::Erase(colors) => {
                let name = match &operation.kind {
                    BatchOperationKind::MoveToColorPlane(_) => "move_to_color_plane",
                    BatchOperationKind::Masking(_) => "masking",
                    BatchOperationKind::Erase(_) => "erase",
                    BatchOperationKind::ColorReplace(_) => unreachable!(),
                };
                append(
                    &mut output,
                    &format!("{{ kind = {name}; enabled = true; target = {target}; colors = ["),
                    maximum_bytes,
                )?;
                for (index, color) in colors.iter().enumerate() {
                    poll(cancelled)?;
                    if index != 0 {
                        append(&mut output, ", ", maximum_bytes)?;
                    }
                    append(&mut output, &pixel_literal(*color), maximum_bytes)?;
                }
            }
        }
        append(&mut output, "]; }", maximum_bytes)?;
    }
    append(&mut output, "];", maximum_bytes)?;
    Ok(output)
}

fn target_literal(
    target: &BatchTargetSelector,
    source_uuid: u128,
    produced: &BTreeMap<u64, String>,
    strict: &mut StrictBindings,
) -> Result<String, InkScriptExportError> {
    let layer_id = target
        .layer_id
        .filter(|id| *id != 0)
        .ok_or(InkScriptExportError::InvalidSource)?;
    let plane_id = target
        .plane_id
        .filter(|id| *id != 0)
        .ok_or(InkScriptExportError::InvalidSource)?;
    let plane_kind = match target.plane_kind {
        Some(PlaneType::Color) => "color",
        Some(PlaneType::Raster) => "raster",
        _ => return Err(InkScriptExportError::InvalidSource),
    };
    if target.missing_policy != BatchMissingTargetPolicy::Error {
        return Err(InkScriptExportError::InvalidSource);
    }
    if produced.contains_key(&plane_id) {
        return Ok(format!(
            "{{ kind = references; layer = {}; plane = {}; }}",
            resolve_reference("layer", layer_id, produced, strict),
            resolve_reference("plane", plane_id, produced, strict),
        ));
    }
    Ok(format!(
        "{{ kind = strict; source_document_uuid = uuid\"{}\"; persistent_layer_id = {layer_id}; persistent_plane_id = {plane_id}; plane_kind = {plane_kind}; missing = error; }}",
        uuid_literal(source_uuid),
    ))
}

fn append(
    output: &mut String,
    text: &str,
    maximum_bytes: usize,
) -> Result<(), InkScriptExportError> {
    if output
        .len()
        .checked_add(text.len())
        .is_none_or(|length| length > maximum_bytes)
    {
        return Err(InkScriptExportError::ResourceLimit);
    }
    output
        .try_reserve(text.len())
        .map_err(|_| InkScriptExportError::ResourceLimit)?;
    output.push_str(text);
    Ok(())
}
