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

fn plane_type_index(kind: PlaneType) -> usize {
    match kind {
        PlaneType::MainLine => 0,
        PlaneType::Color => 1,
        PlaneType::Raster => 2,
        PlaneType::Selection => 3,
        PlaneType::VectorMainLine => 4,
        PlaneType::ColorTrace => 5,
        PlaneType::VectorFill => 6,
    }
}

fn validate_layer_entries(
    kind: LayerKind,
    entries: impl IntoIterator<Item = (PlaneType, PixelFormat)>,
) -> Result<(), CoreError> {
    let mut counts = [0_usize; 7];
    let mut plane_count = 0_usize;
    let mut main_line_format = None;
    let mut all_raster = true;
    let mut all_selection = true;
    let mut all_vector_compatible = true;
    for (plane_kind, format) in entries {
        validate_plane_format(plane_kind, format)?;
        counts[plane_type_index(plane_kind)] += 1;
        plane_count += 1;
        if plane_kind == PlaneType::MainLine {
            main_line_format = Some(format);
        }
        all_raster &= plane_kind == PlaneType::Raster;
        all_selection &= plane_kind == PlaneType::Selection;
        all_vector_compatible &= matches!(
            plane_kind,
            PlaneType::VectorMainLine
                | PlaneType::ColorTrace
                | PlaneType::VectorFill
                | PlaneType::Raster
        );
    }
    let count = |plane_kind| counts[plane_type_index(plane_kind)];
    let valid = match kind {
        LayerKind::BinaryColoring => {
            count(PlaneType::MainLine) == 1
                && count(PlaneType::Color) == 1
                && count(PlaneType::Selection) == 0
                && main_line_format == Some(PixelFormat::BinaryMask8)
        }
        LayerKind::GrayscaleColoring => {
            count(PlaneType::MainLine) == 1
                && count(PlaneType::Color) == 1
                && count(PlaneType::Selection) == 0
                && main_line_format.is_some_and(|format| {
                    matches!(format, PixelFormat::Grayscale8 | PixelFormat::Grayscale16)
                })
        }
        LayerKind::Raster => plane_count != 0 && all_raster,
        LayerKind::Selection => plane_count != 0 && all_selection,
        LayerKind::VectorColoring => {
            count(PlaneType::VectorMainLine) == 1
                && count(PlaneType::ColorTrace) >= 1
                && count(PlaneType::VectorFill) == 1
                && all_vector_compatible
        }
        LayerKind::Frame
        | LayerKind::VanishingPoint
        | LayerKind::Adjustment
        | LayerKind::Text
        | LayerKind::Annotation => plane_count == 0,
    };
    if valid {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(
            "layer and plane types form a disallowed combination",
        ))
    }
}

pub(crate) fn validate_layer_kind(kind: LayerKind, planes: &[PlaneNode]) -> Result<(), CoreError> {
    validate_layer_entries(
        kind,
        planes
            .iter()
            .map(|plane| (plane.kind, plane.raster.format())),
    )
}

pub(crate) fn validate_layer_kind_with_candidate(
    layer_kind: LayerKind,
    planes: &[PlaneNode],
    candidate_kind: PlaneType,
    candidate_format: PixelFormat,
) -> Result<(), CoreError> {
    validate_layer_entries(
        layer_kind,
        planes
            .iter()
            .map(|plane| (plane.kind, plane.raster.format()))
            .chain(std::iter::once((candidate_kind, candidate_format))),
    )
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
    fn new_document_has_unique_stable_ids_and_required_primary_references() {
        let document = CellDocument::new(
            DocumentIds {
                document: DocumentId::from_raw(1),
                layer: LayerId::from_raw(2),
                main_plane: PlaneId::from_raw(3),
                color_plane: PlaneId::from_raw(4),
                selection_plane: PlaneId::from_raw(5),
                light_table_set: LightTableSetId::from_raw(6),
                cell: CellId::from_raw(7),
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
        assert!(ids.insert(document.cell_id.get()));
        assert!(ids.insert(document.selection_plane_id.get()));
        for layer in &document.layers {
            assert!(ids.insert(layer.id.get()));
            for plane in &layer.planes {
                assert!(ids.insert(plane.id.get()));
            }
        }
        let (layer_id, main_plane_id, color_plane_id) = document.primary_ids();
        assert!(document.layers.iter().any(|layer| layer.id == layer_id));
        assert!(document.plane_by_id(main_plane_id).is_some());
        assert!(document.plane_by_id(color_plane_id).is_some());
    }
}
