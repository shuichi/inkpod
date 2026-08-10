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

pub(super) fn validate_operation(operation: &BatchOperation) -> Result<(), CoreError> {
    if operation.version != BATCH_OPERATION_VERSION {
        return Err(CoreError::InvalidArgument(
            "batch operation version is unsupported",
        ));
    }
    let requires_target = !matches!(
        operation.kind,
        BatchOperationKind::Mirror(_)
            | BatchOperationKind::Rotate90(_)
            | BatchOperationKind::Resize(_)
    );
    if requires_target && operation.target.is_none() {
        return Err(CoreError::InvalidArgument(
            "batch operation target selector is empty",
        ));
    }
    if let Some(target) = operation.target {
        if target.layer_id.is_none() && target.layer_kind.is_none() {
            return Err(CoreError::InvalidArgument(
                "batch target layer selector is empty",
            ));
        }
        let requires_plane = !matches!(operation.kind, BatchOperationKind::Visibility { .. });
        if requires_plane && target.plane_id.is_none() && target.plane_kind.is_none() {
            return Err(CoreError::InvalidArgument(
                "batch target plane selector is empty",
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
        BatchOperationKind::ContinuousFill(seeds)
            if seeds.is_empty() || seeds.len() > MAX_BATCH_SEEDS =>
        {
            return Err(CoreError::InvalidArgument(
                "batch fill-seed count is outside bounds",
            ));
        }
        BatchOperationKind::Separation(options)
            if options.colors.is_empty() || options.colors.len() > MAX_BATCH_COLORS =>
        {
            return Err(CoreError::InvalidArgument(
                "batch separation color count is outside bounds",
            ));
        }
        BatchOperationKind::BoundaryAirbrush(effect)
            if effect.colors.len() < 2 || effect.colors.len() > MAX_BATCH_COLORS =>
        {
            return Err(CoreError::InvalidArgument(
                "batch boundary-airbrush color count is outside bounds",
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
    if let BatchOperationKind::ContinuousFill(seeds) = &operation.kind {
        for (index, seed) in seeds.iter().enumerate().filter(|(_, seed)| seed.enabled) {
            if seeds[..index]
                .iter()
                .any(|previous| previous.enabled && previous.x == seed.x && previous.y == seed.y)
            {
                return Err(CoreError::InvalidArgument(
                    "batch continuous fill contains a duplicate enabled seed",
                ));
            }
        }
    }
    if let BatchOperationKind::Separation(options) = &operation.kind {
        for (index, color) in options.colors.iter().enumerate() {
            if options.colors[..index].contains(color) {
                return Err(CoreError::InvalidArgument(
                    "batch separation contains a duplicate color",
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
            configure_each_run: false,
            target: Some(BatchTargetSelector::color_plane()),
            kind,
        })
    }

    #[test]
    fn rejects_empty_target_selector() {
        let operation = BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            configure_each_run: false,
            target: Some(BatchTargetSelector {
                layer_id: None,
                plane_id: None,
                layer_kind: None,
                plane_kind: None,
                missing_policy: BatchMissingTargetPolicy::Skip,
            }),
            kind: BatchOperationKind::ColorReplace(vec![BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([0; 4]),
                new: PixelValue::Rgba([1, 2, 3, 4]),
            }]),
        };
        assert_eq!(
            validate_operation(&operation),
            Err(CoreError::InvalidArgument(
                "batch target layer selector is empty"
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

        let seed = BatchSeed {
            enabled: false,
            x: 0,
            y: 0,
            color: PixelValue::Rgba([0; 4]),
            tolerance: 0,
            gap_close: 0,
            expected_source: None,
        };
        for count in [1, MAX_BATCH_SEEDS] {
            assert!(
                validate_kind(BatchOperationKind::ContinuousFill(vec![
                    seed.clone();
                    count
                ]))
                .is_ok()
            );
        }
        for count in [0, MAX_BATCH_SEEDS + 1] {
            assert_invalid(
                BatchOperationKind::ContinuousFill(vec![seed.clone(); count]),
                "batch fill-seed count is outside bounds",
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
            assert!(
                validate_kind(BatchOperationKind::Separation(BatchSeparation {
                    colors,
                    replacement: PixelValue::Rgba([1, 2, 3, 4]),
                    invert: false,
                    destination: BatchSeparationDestination::ReplaceSource,
                }))
                .is_ok()
            );
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
                BatchOperationKind::Separation(BatchSeparation {
                    colors,
                    replacement: PixelValue::Rgba([1, 2, 3, 4]),
                    invert: false,
                    destination: BatchSeparationDestination::ReplaceSource,
                }),
                "batch separation color count is outside bounds",
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

        let seed = |enabled| BatchSeed {
            enabled,
            x: 4,
            y: 5,
            color: PixelValue::Rgba([1, 2, 3, 4]),
            tolerance: 0,
            gap_close: 0,
            expected_source: None,
        };
        assert!(
            validate_kind(BatchOperationKind::ContinuousFill(vec![
                seed(true),
                seed(true)
            ]))
            .is_err()
        );
        assert!(
            validate_kind(BatchOperationKind::ContinuousFill(vec![
                seed(true),
                seed(false)
            ]))
            .is_ok()
        );
    }
}
