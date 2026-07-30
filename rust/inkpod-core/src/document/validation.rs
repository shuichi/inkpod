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
    plane_id: PlaneId,
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
    plane_id: PlaneId,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(id: u64, kind: PlaneType, format: PixelFormat) -> PlaneNode {
        PlaneNode {
            id: PlaneId::from_raw(id),
            kind,
            name: format!("Plane {id}"),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            raster: TileRaster::new(4, 4, format).unwrap(),
        }
    }

    #[test]
    fn layer_topology_rejects_missing_duplicate_and_incompatible_required_planes() {
        let main = plane(1, PlaneType::MainLine, PixelFormat::BinaryMask8);
        let color = plane(2, PlaneType::Color, PixelFormat::StraightRgba8);
        assert!(validate_layer_kind(LayerKind::BinaryColoring, &[main.clone(), color]).is_ok());
        assert!(
            validate_layer_kind(LayerKind::BinaryColoring, std::slice::from_ref(&main)).is_err()
        );
        assert!(validate_layer_kind(LayerKind::BinaryColoring, &[main.clone(), main]).is_err());
        assert!(validate_plane_format(PlaneType::MainLine, PixelFormat::StraightRgba8).is_err());
    }

    #[test]
    fn new_document_has_unique_stable_ids_and_valid_active_references() {
        let document = CellDocument::new(
            DocumentIds {
                document: DocumentId::from_raw(1),
                layer: LayerId::from_raw(2),
                main_plane: PlaneId::from_raw(3),
                color_plane: PlaneId::from_raw(4),
                selection_plane: PlaneId::from_raw(5),
                light_table_set: LightTableSetId::from_raw(6),
            },
            7,
            PaperSpec {
                width: 4,
                height: 4,
                dpi_x_milli: DEFAULT_DPI_MILLI,
                dpi_y_milli: DEFAULT_DPI_MILLI,
            },
        )
        .unwrap();
        let mut ids = BTreeSet::new();
        assert!(ids.insert(document.id.get()));
        assert!(ids.insert(document.selection_plane_id.get()));
        for layer in &document.layers {
            assert!(ids.insert(layer.id.get()));
            for plane in &layer.planes {
                assert!(ids.insert(plane.id.get()));
            }
        }
        assert!(
            document
                .layers
                .iter()
                .any(|layer| layer.id == document.active_layer_id)
        );
        assert!(document.plane_by_id(document.active_plane_id).is_some());
    }
}
