use super::super::*;

pub(in crate::batch) fn input_kind_code(kind: BatchInputKind) -> u32 {
    match kind {
        BatchInputKind::File => INPUT_FILE,
        BatchInputKind::Folder => INPUT_FOLDER,
        BatchInputKind::ActiveDocument => INPUT_ACTIVE_DOCUMENT,
    }
}

pub(in crate::batch) fn parse_input_kind(value: u32) -> Result<BatchInputKind, CoreError> {
    match value {
        INPUT_FILE => Ok(BatchInputKind::File),
        INPUT_FOLDER => Ok(BatchInputKind::Folder),
        INPUT_ACTIVE_DOCUMENT => Ok(BatchInputKind::ActiveDocument),
        _ => Err(CoreError::InvalidArgument("batch input kind is unknown")),
    }
}

pub(in crate::batch) fn output_policy_code(policy: BatchOutputDestination) -> u32 {
    match policy {
        BatchOutputDestination::Folder => OUTPUT_FOLDER,
        BatchOutputDestination::ActiveDocument => OUTPUT_ACTIVE_DOCUMENT,
        BatchOutputDestination::NewTabs => OUTPUT_NEW_TABS,
    }
}

pub(in crate::batch) fn parse_output_policy(
    value: u32,
) -> Result<BatchOutputDestination, CoreError> {
    match value {
        OUTPUT_FOLDER => Ok(BatchOutputDestination::Folder),
        OUTPUT_ACTIVE_DOCUMENT => Ok(BatchOutputDestination::ActiveDocument),
        OUTPUT_NEW_TABS => Ok(BatchOutputDestination::NewTabs),
        _ => Err(CoreError::InvalidArgument("batch output policy is unknown")),
    }
}

pub(in crate::batch) const fn output_format_code(format: BatchOutputFormat) -> u32 {
    match format {
        BatchOutputFormat::Inkpod => OUTPUT_NATIVE_INKPOD,
        BatchOutputFormat::Png => OUTPUT_PNG,
        BatchOutputFormat::Tiff => OUTPUT_TIFF,
        BatchOutputFormat::Tga => OUTPUT_TGA,
        BatchOutputFormat::Bmp => OUTPUT_BMP,
    }
}

pub(in crate::batch) fn parse_output_format(value: u32) -> Result<BatchOutputFormat, CoreError> {
    match value {
        OUTPUT_NATIVE_INKPOD => Ok(BatchOutputFormat::Inkpod),
        OUTPUT_PNG => Ok(BatchOutputFormat::Png),
        OUTPUT_TIFF => Ok(BatchOutputFormat::Tiff),
        OUTPUT_TGA => Ok(BatchOutputFormat::Tga),
        OUTPUT_BMP => Ok(BatchOutputFormat::Bmp),
        _ => Err(CoreError::InvalidArgument("batch output format is unknown")),
    }
}

pub(in crate::batch) fn failure_policy_code(policy: BatchFailurePolicy) -> u32 {
    match policy {
        BatchFailurePolicy::Continue => FAILURE_CONTINUE,
        BatchFailurePolicy::Stop => FAILURE_STOP,
    }
}

pub(in crate::batch) fn parse_failure_policy(value: u32) -> Result<BatchFailurePolicy, CoreError> {
    match value {
        FAILURE_CONTINUE => Ok(BatchFailurePolicy::Continue),
        FAILURE_STOP => Ok(BatchFailurePolicy::Stop),
        _ => Err(CoreError::InvalidArgument(
            "batch failure policy is unknown",
        )),
    }
}
pub(super) fn layer_kind_code(kind: LayerKind) -> u32 {
    match kind {
        LayerKind::BinaryColoring => 1,
        LayerKind::GrayscaleColoring => 2,
        LayerKind::Raster => 3,
        LayerKind::Selection => 4,
        LayerKind::Frame => 5,
        LayerKind::VanishingPoint => 6,
        LayerKind::Adjustment => 7,
    }
}

pub(super) fn parse_layer_kind(value: u32) -> Result<LayerKind, CoreError> {
    match value {
        1 => Ok(LayerKind::BinaryColoring),
        2 => Ok(LayerKind::GrayscaleColoring),
        3 => Ok(LayerKind::Raster),
        4 => Ok(LayerKind::Selection),
        5 => Ok(LayerKind::Frame),
        6 => Ok(LayerKind::VanishingPoint),
        7 => Ok(LayerKind::Adjustment),
        _ => Err(CoreError::InvalidArgument("batch layer kind is unknown")),
    }
}

pub(super) fn plane_kind_code(kind: PlaneType) -> u32 {
    match kind {
        PlaneType::MainLine => 1,
        PlaneType::Color => 2,
        PlaneType::Raster => 3,
        PlaneType::Selection => 4,
    }
}

pub(super) fn parse_plane_kind(value: u32) -> Result<PlaneType, CoreError> {
    match value {
        1 => Ok(PlaneType::MainLine),
        2 => Ok(PlaneType::Color),
        3 => Ok(PlaneType::Raster),
        4 => Ok(PlaneType::Selection),
        _ => Err(CoreError::InvalidArgument("batch plane kind is unknown")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_codes_reject_unknown_values() {
        assert!(parse_input_kind(u32::MAX).is_err());
        assert!(parse_output_policy(u32::MAX).is_err());
        assert!(parse_failure_policy(u32::MAX).is_err());
        assert!(parse_layer_kind(u32::MAX).is_err());
        assert!(parse_plane_kind(u32::MAX).is_err());
        assert!(parse_output_format(u32::MAX).is_err());
    }
}
