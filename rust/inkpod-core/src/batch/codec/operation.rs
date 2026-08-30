use super::super::validation::validate_operation;
use super::super::*;
use super::codes::*;
use super::payload::*;

pub(in crate::batch) fn operation_to_file(
    operation: &BatchOperation,
) -> Result<FileBatchOperation, CoreError> {
    validate_operation(operation)?;
    let (kind, payload) = encode_operation_kind(&operation.kind)?;
    let targets = std::iter::once(&operation.target)
        .chain(operation.additional_targets.iter())
        .map(|target| {
            Ok(FileBatchTarget {
                layer_id: target.layer_id.unwrap_or(0),
                plane_id: target.plane_id.unwrap_or(0),
                plane_kind: target
                    .plane_kind
                    .map(plane_kind_code)
                    .transpose()?
                    .unwrap_or(0),
                missing_policy: match target.missing_policy {
                    BatchMissingTargetPolicy::Skip => MISSING_SKIP,
                    BatchMissingTargetPolicy::Error => MISSING_ERROR,
                },
            })
        })
        .collect::<Result<Vec<_>, CoreError>>()?;
    Ok(FileBatchOperation {
        version: operation.version,
        kind,
        flags: if operation.enabled { OP_ENABLED } else { 0 },
        targets,
        payload,
    })
}

pub(in crate::batch) fn operation_from_file(
    file: FileBatchOperation,
) -> Result<BatchOperation, CoreError> {
    if file.flags & !OP_ENABLED != 0 {
        return Err(CoreError::InvalidArgument(
            "batch operation flags are invalid",
        ));
    }
    if file.targets.is_empty() {
        return Err(CoreError::InvalidArgument(
            "batch operation target is missing",
        ));
    }
    let mut targets = file
        .targets
        .into_iter()
        .map(|target| {
            Ok(BatchTargetSelector {
                layer_id: (target.layer_id != 0).then_some(target.layer_id),
                plane_id: (target.plane_id != 0).then_some(target.plane_id),
                plane_kind: (target.plane_kind != 0)
                    .then(|| parse_plane_kind(target.plane_kind))
                    .transpose()?,
                missing_policy: match target.missing_policy {
                    MISSING_SKIP => BatchMissingTargetPolicy::Skip,
                    MISSING_ERROR => BatchMissingTargetPolicy::Error,
                    _ => {
                        return Err(CoreError::InvalidArgument(
                            "batch missing-target policy is unknown",
                        ));
                    }
                },
            })
        })
        .collect::<Result<Vec<_>, CoreError>>()?;
    let target = targets.remove(0);
    let operation = BatchOperation {
        version: file.version,
        enabled: file.flags & OP_ENABLED != 0,
        target,
        additional_targets: targets,
        kind: decode_operation_kind(file.kind, &file.payload)?,
    };
    validate_operation(&operation)?;
    Ok(operation)
}

pub(super) fn encode_operation_kind(
    kind: &BatchOperationKind,
) -> Result<(u32, Vec<u8>), CoreError> {
    let mut output = PayloadWriter::default();
    let code = match kind {
        BatchOperationKind::ColorReplace(pairs) => {
            output.u32(pairs.len() as u32);
            for pair in pairs {
                output.u32(u32::from(pair.enabled));
                output.pixel(pair.old);
                output.pixel(pair.new);
            }
            OP_COLOR_REPLACE
        }
        BatchOperationKind::MoveToColorPlane(colors)
        | BatchOperationKind::Masking(colors)
        | BatchOperationKind::Erase(colors) => {
            output.u32(colors.len() as u32);
            for color in colors {
                output.pixel(*color);
            }
            match kind {
                BatchOperationKind::MoveToColorPlane(_) => OP_MOVE_TO_COLOR_PLANE,
                BatchOperationKind::Masking(_) => OP_MASKING,
                BatchOperationKind::Erase(_) => OP_ERASE,
                BatchOperationKind::ColorReplace(_) => unreachable!(),
            }
        }
    };
    Ok((code, output.bytes))
}

pub(super) fn decode_operation_kind(
    code: u32,
    payload: &[u8],
) -> Result<BatchOperationKind, CoreError> {
    let mut input = PayloadReader::new(payload);
    let kind = match code {
        OP_COLOR_REPLACE => {
            let count = input.count(MAX_BATCH_COLOR_PAIRS)?;
            let mut pairs = Vec::with_capacity(count);
            for _ in 0..count {
                pairs.push(BatchColorPair {
                    enabled: input.boolean()?,
                    old: input.pixel()?,
                    new: input.pixel()?,
                });
            }
            BatchOperationKind::ColorReplace(pairs)
        }
        OP_MOVE_TO_COLOR_PLANE | OP_MASKING | OP_ERASE => {
            let count = input.count(MAX_BATCH_COLORS)?;
            let mut colors = Vec::with_capacity(count);
            for _ in 0..count {
                colors.push(input.pixel()?);
            }
            match code {
                OP_MOVE_TO_COLOR_PLANE => BatchOperationKind::MoveToColorPlane(colors),
                OP_MASKING => BatchOperationKind::Masking(colors),
                OP_ERASE => BatchOperationKind::Erase(colors),
                _ => unreachable!(),
            }
        }
        _ => {
            return Err(CoreError::InvalidArgument(
                "batch operation kind is unknown",
            ));
        }
    };
    input.finish()?;
    Ok(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_decoder_rejects_unknown_kind() {
        assert_eq!(
            decode_operation_kind(u32::MAX, &[]),
            Err(CoreError::InvalidArgument(
                "batch operation kind is unknown"
            ))
        );
    }
}
