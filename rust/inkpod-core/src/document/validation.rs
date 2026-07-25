use super::*;

pub(crate) const fn is_coloring_layer(kind: LayerKind) -> bool {
    matches!(
        kind,
        LayerKind::BinaryColoring | LayerKind::GrayscaleColoring
    )
}

pub(crate) fn validate_plane_format(kind: PlaneType, format: PixelFormat) -> Result<(), CoreError> {
    let valid = match kind {
        PlaneType::MainLine => matches!(
            format,
            PixelFormat::BinaryMask8 | PixelFormat::Grayscale8 | PixelFormat::Grayscale16
        ),
        PlaneType::Color | PlaneType::Raster => matches!(
            format,
            PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
        ),
        PlaneType::Selection => format == PixelFormat::BinaryMask8,
        PlaneType::VectorMainLine | PlaneType::ColorTrace | PlaneType::VectorFill => {
            format == PixelFormat::StraightRgba8
        }
    };
    if valid {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(
            "pixel format is not allowed for the plane type",
        ))
    }
}

pub(crate) fn validate_layer_kind(kind: LayerKind, planes: &[PlaneNode]) -> Result<(), CoreError> {
    for plane in planes {
        validate_plane_format(plane.kind, plane.raster.format())?;
    }
    let count = |kind| planes.iter().filter(|plane| plane.kind == kind).count();
    let valid = match kind {
        LayerKind::BinaryColoring => {
            count(PlaneType::MainLine) == 1
                && count(PlaneType::Color) == 1
                && count(PlaneType::Selection) == 0
                && planes
                    .iter()
                    .find(|plane| plane.kind == PlaneType::MainLine)
                    .is_some_and(|plane| plane.raster.format() == PixelFormat::BinaryMask8)
        }
        LayerKind::GrayscaleColoring => {
            count(PlaneType::MainLine) == 1
                && count(PlaneType::Color) == 1
                && count(PlaneType::Selection) == 0
                && planes
                    .iter()
                    .find(|plane| plane.kind == PlaneType::MainLine)
                    .is_some_and(|plane| {
                        matches!(
                            plane.raster.format(),
                            PixelFormat::Grayscale8 | PixelFormat::Grayscale16
                        )
                    })
        }
        LayerKind::Raster => {
            !planes.is_empty() && planes.iter().all(|plane| plane.kind == PlaneType::Raster)
        }
        LayerKind::Selection => {
            !planes.is_empty()
                && planes
                    .iter()
                    .all(|plane| plane.kind == PlaneType::Selection)
        }
        LayerKind::VectorColoring => {
            count(PlaneType::VectorMainLine) == 1
                && count(PlaneType::ColorTrace) >= 1
                && count(PlaneType::VectorFill) == 1
                && planes.iter().all(|plane| {
                    matches!(
                        plane.kind,
                        PlaneType::VectorMainLine
                            | PlaneType::ColorTrace
                            | PlaneType::VectorFill
                            | PlaneType::Raster
                    )
                })
        }
        LayerKind::Frame
        | LayerKind::VanishingPoint
        | LayerKind::Adjustment
        | LayerKind::Text
        | LayerKind::Annotation => planes.is_empty(),
    };
    if valid {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(
            "layer and plane types form a disallowed combination",
        ))
    }
}

pub(crate) fn unique_layer_name(layers: &[LayerNode], requested: &str) -> String {
    if !layers.iter().any(|layer| layer.name == requested) {
        return requested.to_owned();
    }
    for suffix in 2..=MAX_LAYERS {
        let candidate = format!("{requested} {suffix}");
        if !layers.iter().any(|layer| layer.name == candidate) {
            return candidate;
        }
    }
    format!("{requested} {}", layers.len() + 1)
}

pub(crate) fn unique_plane_name(planes: &[PlaneNode], requested: &str) -> String {
    if !planes.iter().any(|plane| plane.name == requested) {
        return requested.to_owned();
    }
    for suffix in 2..=MAX_PLANES_PER_LAYER {
        let candidate = format!("{requested} {suffix}");
        if !planes.iter().any(|plane| plane.name == candidate) {
            return candidate;
        }
    }
    format!("{requested} {}", planes.len() + 1)
}

pub(crate) fn find_plane_indices(
    document: &CellDocument,
    plane_id: u64,
) -> Result<(usize, usize), CoreError> {
    document
        .layers
        .iter()
        .enumerate()
        .find_map(|(layer_index, layer)| {
            layer
                .planes
                .iter()
                .position(|plane| plane.id == plane_id)
                .map(|plane_index| (layer_index, plane_index))
        })
        .ok_or(CoreError::InvalidArgument("plane ID does not exist"))
}

pub(crate) fn ensure_editable_plane(
    document: &CellDocument,
    plane_id: u64,
) -> Result<(), CoreError> {
    let (layer_index, plane_index) = find_plane_indices(document, plane_id)?;
    if !document.layers[layer_index].editable
        || !document.layers[layer_index].planes[plane_index].editable
    {
        Err(CoreError::InvalidState(
            "active layer or plane is not editable",
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn ensure_editable_role(
    document: &CellDocument,
    role: ActivePlane,
) -> Result<(), CoreError> {
    ensure_editable_plane(document, document.plane_for_role(role)?.id)
}

pub(crate) fn bounded_document_pixels(width: u32, height: u32) -> Result<u64, CoreError> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(CoreError::InvalidArgument("document pixel count overflows"))?;
    if pixels > MAX_FILL_PIXELS {
        Err(CoreError::InvalidArgument(
            "operation exceeds the bounded document work limit",
        ))
    } else {
        Ok(pixels)
    }
}
