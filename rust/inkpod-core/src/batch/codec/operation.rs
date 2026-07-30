use super::super::validation::validate_operation;
use super::super::*;
use super::codes::*;
use super::filter::*;
use super::payload::*;

pub(in crate::batch) fn operation_to_file(
    operation: &BatchOperation,
) -> Result<FileBatchOperation, CoreError> {
    validate_operation(operation)?;
    let (kind, payload) = encode_operation_kind(&operation.kind)?;
    let target = operation
        .target
        .map_or(FileBatchTarget::default(), |target| FileBatchTarget {
            layer_id: target.layer_id.unwrap_or(0),
            plane_id: target.plane_id.unwrap_or(0),
            layer_kind: target.layer_kind.map_or(0, layer_kind_code),
            plane_kind: target.plane_kind.map_or(0, plane_kind_code),
            missing_policy: match target.missing_policy {
                BatchMissingTargetPolicy::Skip => MISSING_SKIP,
                BatchMissingTargetPolicy::Error => MISSING_ERROR,
            },
        });
    Ok(FileBatchOperation {
        version: operation.version,
        kind,
        flags: (if operation.enabled { OP_ENABLED } else { 0 })
            | (if operation.configure_each_run {
                OP_CONFIGURE_EACH_RUN
            } else {
                0
            }),
        target,
        payload,
    })
}

pub(in crate::batch) fn operation_from_file(
    file: FileBatchOperation,
) -> Result<BatchOperation, CoreError> {
    if file.flags & !(OP_ENABLED | OP_CONFIGURE_EACH_RUN) != 0 {
        return Err(CoreError::InvalidArgument(
            "batch operation flags are invalid",
        ));
    }
    let target = if file.target == FileBatchTarget::default() {
        None
    } else {
        Some(BatchTargetSelector {
            layer_id: (file.target.layer_id != 0).then_some(file.target.layer_id),
            plane_id: (file.target.plane_id != 0).then_some(file.target.plane_id),
            layer_kind: (file.target.layer_kind != 0)
                .then(|| parse_layer_kind(file.target.layer_kind))
                .transpose()?,
            plane_kind: (file.target.plane_kind != 0)
                .then(|| parse_plane_kind(file.target.plane_kind))
                .transpose()?,
            missing_policy: match file.target.missing_policy {
                MISSING_SKIP => BatchMissingTargetPolicy::Skip,
                MISSING_ERROR => BatchMissingTargetPolicy::Error,
                _ => {
                    return Err(CoreError::InvalidArgument(
                        "batch missing-target policy is unknown",
                    ));
                }
            },
        })
    };
    let operation = BatchOperation {
        version: file.version,
        enabled: file.flags & OP_ENABLED != 0,
        configure_each_run: file.flags & OP_CONFIGURE_EACH_RUN != 0,
        target,
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
        BatchOperationKind::ContinuousFill(seeds) => {
            output.u32(seeds.len() as u32);
            for seed in seeds {
                output.u32(seed.x);
                output.u32(seed.y);
                output.pixel(seed.color);
                output.u32(u32::from(seed.tolerance));
                output.u32(u32::from(seed.gap_close));
                output.u32(u32::from(seed.expected_source.is_some()));
                output.pixel(seed.expected_source.unwrap_or(PixelValue::Rgba([0; 4])));
            }
            OP_CONTINUOUS_FILL
        }
        BatchOperationKind::Separation(options) => {
            output.u32(options.colors.len() as u32);
            for color in &options.colors {
                output.pixel(*color);
            }
            output.pixel(options.replacement);
            output.u32(u32::from(options.invert));
            OP_SEPARATION
        }
        BatchOperationKind::Visibility { visible } => {
            output.u32(u32::from(*visible));
            OP_VISIBILITY
        }
        BatchOperationKind::LineWidth(mode) => {
            let (mode, value) = match mode {
                VectorWidthMode::Add(value) => (1, *value),
                VectorWidthMode::Subtract(value) => (2, *value),
                VectorWidthMode::Scale(value) => (3, *value),
                VectorWidthMode::Constant(value) => (4, *value),
            };
            output.u32(mode);
            output.u32(value.to_bits());
            OP_LINE_WIDTH
        }
        BatchOperationKind::Filter(filter) => {
            encode_filter(&mut output, filter)?;
            OP_FILTER
        }
        BatchOperationKind::BoundaryAirbrush(effect) => {
            output.u32(effect.colors.len() as u32);
            for color in &effect.colors {
                for component in color {
                    output.u32(u32::from(*component));
                }
            }
            output.u32(effect.width);
            output.u32(effect.strength_milli);
            OP_BOUNDARY_AIRBRUSH
        }
        BatchOperationKind::DustRemoval(options) => {
            output.u32(match options.mode {
                DustMode::RemoveForeground => 1,
                DustMode::FillTransparentHoles => 2,
                DustMode::ReplaceColorOutliers => 3,
            });
            output.u32(options.maximum_pixels);
            OP_DUST_REMOVAL
        }
        BatchOperationKind::Mirror(axis) => {
            output.u32(match axis {
                MirrorAxis::Horizontal => 1,
                MirrorAxis::Vertical => 2,
            });
            OP_MIRROR
        }
        BatchOperationKind::Rotate90(direction) => {
            output.u32(match direction {
                RotateDirection::Left90 => 1,
                RotateDirection::Right90 => 2,
            });
            OP_ROTATE_90
        }
        BatchOperationKind::Resize(resize) => {
            output.u32(resize.width);
            output.u32(resize.height);
            output.u32(resize.dpi_x_milli);
            output.u32(resize.dpi_y_milli);
            output.u32(u32::from(resize.resample));
            output.u32(resize_anchor_code(resize.anchor));
            OP_RESIZE
        }
        BatchOperationKind::ConvertPlane {
            destination_kind,
            destination_format,
        } => {
            output.u32(plane_kind_code(*destination_kind));
            output.u32(pixel_format_code(*destination_format));
            OP_CONVERT_PLANE
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
        OP_CONTINUOUS_FILL => {
            let count = input.count(MAX_BATCH_SEEDS)?;
            let mut seeds = Vec::with_capacity(count);
            for _ in 0..count {
                let x = input.u32()?;
                let y = input.u32()?;
                let color = input.pixel()?;
                let tolerance = u16::try_from(input.u32()?)
                    .map_err(|_| CoreError::InvalidArgument("batch fill tolerance is invalid"))?;
                let gap_close = u8::try_from(input.u32()?)
                    .map_err(|_| CoreError::InvalidArgument("batch gap-close value is invalid"))?;
                let has_expected = input.boolean()?;
                let expected = input.pixel()?;
                seeds.push(BatchSeed {
                    x,
                    y,
                    color,
                    tolerance,
                    gap_close,
                    expected_source: has_expected.then_some(expected),
                });
            }
            BatchOperationKind::ContinuousFill(seeds)
        }
        OP_SEPARATION => {
            let count = input.count(MAX_BATCH_COLORS)?;
            let mut colors = Vec::with_capacity(count);
            for _ in 0..count {
                colors.push(input.pixel()?);
            }
            BatchOperationKind::Separation(BatchSeparation {
                colors,
                replacement: input.pixel()?,
                invert: input.boolean()?,
            })
        }
        OP_VISIBILITY => BatchOperationKind::Visibility {
            visible: input.boolean()?,
        },
        OP_LINE_WIDTH => {
            let mode = input.u32()?;
            let value = f32::from_bits(input.u32()?);
            BatchOperationKind::LineWidth(match mode {
                1 => VectorWidthMode::Add(value),
                2 => VectorWidthMode::Subtract(value),
                3 => VectorWidthMode::Scale(value),
                4 => VectorWidthMode::Constant(value),
                _ => {
                    return Err(CoreError::InvalidArgument(
                        "batch line-width mode is unknown",
                    ));
                }
            })
        }
        OP_FILTER => BatchOperationKind::Filter(decode_filter(&mut input)?),
        OP_BOUNDARY_AIRBRUSH => {
            let count = input.count(MAX_BATCH_COLORS)?;
            let mut colors = Vec::with_capacity(count);
            for _ in 0..count {
                let mut color = [0_u16; 4];
                for component in &mut color {
                    *component = u16::try_from(input.u32()?).map_err(|_| {
                        CoreError::InvalidArgument("batch boundary color is invalid")
                    })?;
                }
                colors.push(color);
            }
            BatchOperationKind::BoundaryAirbrush(BoundaryAirbrush {
                colors,
                width: input.u32()?,
                strength_milli: input.u32()?,
            })
        }
        OP_DUST_REMOVAL => BatchOperationKind::DustRemoval(DustRemoval {
            mode: match input.u32()? {
                1 => DustMode::RemoveForeground,
                2 => DustMode::FillTransparentHoles,
                3 => DustMode::ReplaceColorOutliers,
                _ => return Err(CoreError::InvalidArgument("batch dust mode is unknown")),
            },
            maximum_pixels: input.u32()?,
        }),
        OP_MIRROR => BatchOperationKind::Mirror(match input.u32()? {
            1 => MirrorAxis::Horizontal,
            2 => MirrorAxis::Vertical,
            _ => return Err(CoreError::InvalidArgument("batch mirror axis is unknown")),
        }),
        OP_ROTATE_90 => BatchOperationKind::Rotate90(match input.u32()? {
            1 => RotateDirection::Left90,
            2 => RotateDirection::Right90,
            _ => {
                return Err(CoreError::InvalidArgument(
                    "batch rotation direction is unknown",
                ));
            }
        }),
        OP_RESIZE => BatchOperationKind::Resize(DocumentResize {
            width: input.u32()?,
            height: input.u32()?,
            dpi_x_milli: input.u32()?,
            dpi_y_milli: input.u32()?,
            resample: input.boolean()?,
            anchor: parse_resize_anchor(input.u32()?)?,
        }),
        OP_CONVERT_PLANE => BatchOperationKind::ConvertPlane {
            destination_kind: parse_plane_kind(input.u32()?)?,
            destination_format: parse_pixel_format(input.u32()?)?,
        },
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
