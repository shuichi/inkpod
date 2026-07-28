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
