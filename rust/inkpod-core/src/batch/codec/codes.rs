use super::super::*;

pub(in crate::batch) fn input_kind_code(kind: BatchInputKind) -> u32 {
    match kind {
        BatchInputKind::File => INPUT_FILE,
        BatchInputKind::Folder => INPUT_FOLDER,
        BatchInputKind::CurrentSequence => INPUT_CURRENT_SEQUENCE,
    }
}

pub(in crate::batch) fn parse_input_kind(value: u32) -> Result<BatchInputKind, CoreError> {
    match value {
        INPUT_FILE => Ok(BatchInputKind::File),
        INPUT_FOLDER => Ok(BatchInputKind::Folder),
        INPUT_CURRENT_SEQUENCE => Ok(BatchInputKind::CurrentSequence),
        _ => Err(CoreError::InvalidArgument("batch input kind is unknown")),
    }
}

pub(in crate::batch) fn output_policy_code(policy: BatchOutputPolicy) -> u32 {
    match policy {
        BatchOutputPolicy::Duplicate => OUTPUT_DUPLICATE,
        BatchOutputPolicy::NewSave => OUTPUT_NEW_SAVE,
        BatchOutputPolicy::ExplicitOverwrite => OUTPUT_OVERWRITE,
    }
}

pub(in crate::batch) fn parse_output_policy(value: u32) -> Result<BatchOutputPolicy, CoreError> {
    match value {
        OUTPUT_DUPLICATE => Ok(BatchOutputPolicy::Duplicate),
        OUTPUT_NEW_SAVE => Ok(BatchOutputPolicy::NewSave),
        OUTPUT_OVERWRITE => Ok(BatchOutputPolicy::ExplicitOverwrite),
        _ => Err(CoreError::InvalidArgument("batch output policy is unknown")),
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
pub(super) fn channel_code(channel: Channel) -> u32 {
    match channel {
        Channel::Rgb => 1,
        Channel::Red => 2,
        Channel::Green => 3,
        Channel::Blue => 4,
    }
}

pub(super) fn parse_channel(value: u32) -> Result<Channel, CoreError> {
    match value {
        1 => Ok(Channel::Rgb),
        2 => Ok(Channel::Red),
        3 => Ok(Channel::Green),
        4 => Ok(Channel::Blue),
        _ => Err(CoreError::InvalidArgument(
            "batch filter channel is unknown",
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
        LayerKind::Text => 8,
        LayerKind::Annotation => 9,
        LayerKind::VectorColoring => 10,
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
        8 => Ok(LayerKind::Text),
        9 => Ok(LayerKind::Annotation),
        10 => Ok(LayerKind::VectorColoring),
        _ => Err(CoreError::InvalidArgument("batch layer kind is unknown")),
    }
}

pub(super) fn plane_kind_code(kind: PlaneType) -> u32 {
    match kind {
        PlaneType::MainLine => 1,
        PlaneType::Color => 2,
        PlaneType::Raster => 3,
        PlaneType::Selection => 4,
        PlaneType::VectorMainLine => 5,
        PlaneType::ColorTrace => 6,
        PlaneType::VectorFill => 7,
    }
}

pub(super) fn parse_plane_kind(value: u32) -> Result<PlaneType, CoreError> {
    match value {
        1 => Ok(PlaneType::MainLine),
        2 => Ok(PlaneType::Color),
        3 => Ok(PlaneType::Raster),
        4 => Ok(PlaneType::Selection),
        5 => Ok(PlaneType::VectorMainLine),
        6 => Ok(PlaneType::ColorTrace),
        7 => Ok(PlaneType::VectorFill),
        _ => Err(CoreError::InvalidArgument("batch plane kind is unknown")),
    }
}

pub(super) fn pixel_format_code(format: PixelFormat) -> u32 {
    match format {
        PixelFormat::BinaryMask8 => 1,
        PixelFormat::Grayscale8 => 2,
        PixelFormat::Grayscale16 => 3,
        PixelFormat::StraightRgba8 => 4,
        PixelFormat::StraightRgba16 => 5,
        PixelFormat::PremultipliedBgra8 => 6,
    }
}

pub(super) fn parse_pixel_format(value: u32) -> Result<PixelFormat, CoreError> {
    match value {
        1 => Ok(PixelFormat::BinaryMask8),
        2 => Ok(PixelFormat::Grayscale8),
        3 => Ok(PixelFormat::Grayscale16),
        4 => Ok(PixelFormat::StraightRgba8),
        5 => Ok(PixelFormat::StraightRgba16),
        6 => Ok(PixelFormat::PremultipliedBgra8),
        _ => Err(CoreError::InvalidArgument(
            "batch destination pixel format is unknown",
        )),
    }
}

pub(super) fn resize_anchor_code(anchor: ResizeAnchor) -> u32 {
    match anchor {
        ResizeAnchor::TopLeft => 1,
        ResizeAnchor::TopRight => 2,
        ResizeAnchor::Center => 3,
        ResizeAnchor::BottomLeft => 4,
        ResizeAnchor::BottomRight => 5,
    }
}

pub(super) fn parse_resize_anchor(value: u32) -> Result<ResizeAnchor, CoreError> {
    match value {
        1 => Ok(ResizeAnchor::TopLeft),
        2 => Ok(ResizeAnchor::TopRight),
        3 => Ok(ResizeAnchor::Center),
        4 => Ok(ResizeAnchor::BottomLeft),
        5 => Ok(ResizeAnchor::BottomRight),
        _ => Err(CoreError::InvalidArgument("batch resize anchor is unknown")),
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
        assert!(parse_channel(u32::MAX).is_err());
        assert!(parse_layer_kind(u32::MAX).is_err());
        assert!(parse_plane_kind(u32::MAX).is_err());
        assert!(parse_pixel_format(u32::MAX).is_err());
        assert!(parse_resize_anchor(u32::MAX).is_err());
    }
}
