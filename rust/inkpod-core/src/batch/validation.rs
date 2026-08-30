use super::*;

pub(super) fn within_range(name: &str, input: &BatchInputSelector) -> bool {
    parse_cell_number(name).is_none_or(|number| within_cell_range(number, input))
}

pub(super) fn within_cell_range(number: u32, input: &BatchInputSelector) -> bool {
    (input.first_cell == 0 || number >= input.first_cell)
        && (input.last_cell == 0 || number <= input.last_cell)
}

pub(super) fn path_label(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("cell.inkpod")
}

pub(super) fn validate_component(value: &str, required: bool) -> Result<(), CoreError> {
    if (required && value.is_empty())
        || value.len() > MAX_BATCH_NAME_BYTES
        || value.as_bytes().contains(&0)
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(CoreError::InvalidArgument(
            "batch name or basename is invalid",
        ));
    }
    Ok(())
}

pub(super) fn validate_path(value: &str) -> Result<(), CoreError> {
    if value.len() > MAX_BATCH_PATH_BYTES || value.as_bytes().contains(&0) {
        return Err(CoreError::InvalidArgument("batch path is invalid"));
    }
    Ok(())
}

pub(super) fn validate_naming_template(template: &str) -> Result<(), CoreError> {
    if template.is_empty() || template.len() > MAX_BATCH_NAME_BYTES || template.contains('.') {
        return Err(CoreError::InvalidArgument(
            "batch naming template is invalid",
        ));
    }
    let mut remaining = template;
    while let Some(open) = remaining.find('{') {
        remaining = &remaining[open..];
        if let Some(rest) = remaining.strip_prefix("{stem}") {
            remaining = rest;
            continue;
        }
        let Some(rest) = remaining.strip_prefix("{index:") else {
            return Err(CoreError::InvalidArgument(
                "batch naming template contains an unknown token",
            ));
        };
        let Some(close) = rest.find('}') else {
            return Err(CoreError::InvalidArgument(
                "batch naming template token is unterminated",
            ));
        };
        let width = rest[..close]
            .parse::<usize>()
            .map_err(|_| CoreError::InvalidArgument("batch index width is invalid"))?;
        if !(1..=12).contains(&width) {
            return Err(CoreError::InvalidArgument(
                "batch index width is outside bounds",
            ));
        }
        remaining = &rest[close + 1..];
    }
    if remaining.contains('}') {
        return Err(CoreError::InvalidArgument(
            "batch naming template contains an unmatched brace",
        ));
    }
    validate_component(template, true)
}

pub(crate) fn validate_operation(operation: &BatchOperation) -> Result<(), CoreError> {
    if operation.version != BATCH_OPERATION_VERSION {
        return Err(CoreError::InvalidArgument(
            "batch operation version is unsupported",
        ));
    }
    if operation.additional_targets.len() >= MAX_BATCH_TARGETS {
        return Err(CoreError::InvalidArgument(
            "batch target count is outside bounds",
        ));
    }
    if !matches!(operation.kind, BatchOperationKind::ColorReplace(_))
        && !operation.additional_targets.is_empty()
    {
        return Err(CoreError::InvalidArgument(
            "batch operation kind does not support multiple targets",
        ));
    }
    for (index, target) in std::iter::once(&operation.target)
        .chain(operation.additional_targets.iter())
        .enumerate()
    {
        if target.plane_id.is_none() && target.plane_kind.is_none() {
            return Err(CoreError::InvalidArgument(
                "batch target plane selector is empty",
            ));
        }
        if target.plane_kind == Some(PlaneType::MainLine) {
            return Err(CoreError::InvalidArgument(
                "batch target plane kind must be Color or Raster",
            ));
        }
        if std::iter::once(&operation.target)
            .chain(operation.additional_targets.iter())
            .take(index)
            .any(|previous| previous == target)
        {
            return Err(CoreError::InvalidArgument(
                "batch operation contains a duplicate target selector",
            ));
        }
    }
    match &operation.kind {
        BatchOperationKind::ColorReplace(pairs)
            if pairs.is_empty() || pairs.len() > MAX_BATCH_COLOR_PAIRS =>
        {
            return Err(CoreError::InvalidArgument(
                "batch color-pair count is outside bounds",
            ));
        }
        BatchOperationKind::MoveToColorPlane(colors)
        | BatchOperationKind::Masking(colors)
        | BatchOperationKind::Erase(colors)
            if colors.is_empty() || colors.len() > MAX_BATCH_COLORS =>
        {
            return Err(CoreError::InvalidArgument(
                "batch operation color count is outside bounds",
            ));
        }
        _ => {}
    }
    if let BatchOperationKind::ColorReplace(pairs) = &operation.kind {
        for (index, pair) in pairs.iter().enumerate().filter(|(_, pair)| pair.enabled) {
            if pairs[..index]
                .iter()
                .any(|previous| previous.enabled && previous.old == pair.old)
            {
                return Err(CoreError::InvalidArgument(
                    "batch color replacement contains a duplicate enabled old color",
                ));
            }
        }
    }
    if let BatchOperationKind::MoveToColorPlane(colors)
    | BatchOperationKind::Masking(colors)
    | BatchOperationKind::Erase(colors) = &operation.kind
    {
        for (index, color) in colors.iter().enumerate() {
            if colors[..index].contains(color) {
                return Err(CoreError::InvalidArgument(
                    "batch operation contains a duplicate color",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn ensure_pixel_matches_format(
    value: PixelValue,
    format: PixelFormat,
) -> Result<(), CoreError> {
    if matches!(
        (value, format),
        (PixelValue::Binary(_), PixelFormat::BinaryMask8)
            | (PixelValue::Grayscale8(_), PixelFormat::Grayscale8)
            | (PixelValue::Grayscale16(_), PixelFormat::Grayscale16)
            | (PixelValue::Rgba(_), PixelFormat::StraightRgba8)
            | (PixelValue::Rgba16(_), PixelFormat::StraightRgba16)
    ) {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(
            "batch color depth does not match the target plane",
        ))
    }
}

pub(super) const fn empty_pixel(format: PixelFormat) -> PixelValue {
    match format {
        PixelFormat::BinaryMask8 => PixelValue::Binary(0),
        PixelFormat::Grayscale8 => PixelValue::Grayscale8(0),
        PixelFormat::Grayscale16 => PixelValue::Grayscale16(0),
        PixelFormat::StraightRgba8 => PixelValue::Rgba([0; 4]),
        PixelFormat::StraightRgba16 => PixelValue::Rgba16([0; 4]),
        PixelFormat::PremultipliedBgra8 => PixelValue::Rgba([0; 4]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate_kind(kind: BatchOperationKind) -> Result<(), CoreError> {
        validate_operation(&BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            target: BatchTargetSelector::color_plane(),
            additional_targets: Vec::new(),
            kind,
        })
    }

    #[test]
    fn rejects_empty_target_selector() {
        let operation = BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            target: BatchTargetSelector {
                layer_id: None,
                plane_id: None,
                plane_kind: None,
                missing_policy: BatchMissingTargetPolicy::Skip,
            },
            additional_targets: Vec::new(),
            kind: BatchOperationKind::ColorReplace(vec![BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([0; 4]),
                new: PixelValue::Rgba([1, 2, 3, 4]),
            }]),
        };
        assert_eq!(
            validate_operation(&operation),
            Err(CoreError::InvalidArgument(
                "batch target plane selector is empty"
            ))
        );
    }

    #[test]
    fn rejects_main_line_target_role() {
        let operation = BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            target: BatchTargetSelector {
                layer_id: None,
                plane_id: None,
                plane_kind: Some(PlaneType::MainLine),
                missing_policy: BatchMissingTargetPolicy::Error,
            },
            additional_targets: Vec::new(),
            kind: BatchOperationKind::Erase(vec![PixelValue::Binary(0)]),
        };
        assert_eq!(
            validate_operation(&operation),
            Err(CoreError::InvalidArgument(
                "batch target plane kind must be Color or Raster"
            ))
        );
    }

    #[test]
    fn operation_item_counts_enforce_closed_bounds() {
        let assert_invalid = |kind, message| {
            assert_eq!(
                validate_kind(kind),
                Err(CoreError::InvalidArgument(message))
            );
        };

        let pair = BatchColorPair {
            enabled: false,
            old: PixelValue::Rgba([0; 4]),
            new: PixelValue::Rgba([1, 2, 3, 4]),
        };
        for count in [1, MAX_BATCH_COLOR_PAIRS] {
            assert!(
                validate_kind(BatchOperationKind::ColorReplace(vec![pair.clone(); count])).is_ok()
            );
        }
        for count in [0, MAX_BATCH_COLOR_PAIRS + 1] {
            assert_invalid(
                BatchOperationKind::ColorReplace(vec![pair.clone(); count]),
                "batch color-pair count is outside bounds",
            );
        }

        for count in [1, MAX_BATCH_COLORS] {
            let colors = (0..count)
                .map(|index| {
                    PixelValue::Rgba([
                        (index >> 8) as u8,
                        index as u8,
                        (index >> 16) as u8,
                        u8::MAX,
                    ])
                })
                .collect();
            assert!(validate_kind(BatchOperationKind::Masking(colors)).is_ok());
        }
        for count in [0, MAX_BATCH_COLORS + 1] {
            let colors = (0..count)
                .map(|index| {
                    PixelValue::Rgba([
                        (index >> 8) as u8,
                        index as u8,
                        (index >> 16) as u8,
                        u8::MAX,
                    ])
                })
                .collect();
            assert_invalid(
                BatchOperationKind::Masking(colors),
                "batch operation color count is outside bounds",
            );
        }
    }

    #[test]
    fn enabled_duplicate_rows_are_rejected_without_forbidding_disabled_alternatives() {
        let pair = |enabled| BatchColorPair {
            enabled,
            old: PixelValue::Rgba([1, 2, 3, 4]),
            new: PixelValue::Rgba([4, 3, 2, 1]),
        };
        assert!(
            validate_kind(BatchOperationKind::ColorReplace(vec![
                pair(true),
                pair(true)
            ]))
            .is_err()
        );
        assert!(
            validate_kind(BatchOperationKind::ColorReplace(vec![
                pair(true),
                pair(false)
            ]))
            .is_ok()
        );
        assert!(
            validate_kind(BatchOperationKind::Erase(vec![
                PixelValue::Rgba([1, 2, 3, 4]),
                PixelValue::Rgba([1, 2, 3, 4]),
            ]))
            .is_err()
        );
    }
}
