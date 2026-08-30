use super::*;

pub(crate) const MAX_SAVED_SELECTION_MASKS: usize = 4_096;

pub(crate) fn validate_plane_format(kind: PlaneType, format: PixelFormat) -> Result<(), CoreError> {
    let valid = match kind {
        PlaneType::MainLine => matches!(
            format,
            PixelFormat::BinaryMask8
                | PixelFormat::Grayscale8
                | PixelFormat::Grayscale16
                | PixelFormat::StraightRgba8
                | PixelFormat::StraightRgba16
        ),
        PlaneType::Color | PlaneType::Raster => matches!(
            format,
            PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
        ),
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
    }
}

fn validate_layer_entries(
    entries: impl IntoIterator<Item = (PlaneType, PixelFormat)>,
) -> Result<(), CoreError> {
    let mut counts = [0_usize; 3];
    for (plane_kind, format) in entries {
        validate_plane_format(plane_kind, format)?;
        counts[plane_type_index(plane_kind)] += 1;
    }
    let count = |plane_kind| counts[plane_type_index(plane_kind)];
    let valid = count(PlaneType::MainLine) == 1 && count(PlaneType::Color) == 1;
    if valid {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(
            "a layer must contain exactly one main-line and one color plane",
        ))
    }
}

pub(crate) fn validate_layer(planes: &[PlaneNode]) -> Result<(), CoreError> {
    validate_layer_entries(
        planes
            .iter()
            .map(|plane| (plane.kind, plane.raster.format())),
    )
}

pub(crate) fn unique_saved_selection_name(
    saved_selections: &[SavedSelectionMask],
    requested: &str,
) -> String {
    if !saved_selections
        .iter()
        .any(|selection| selection.name == requested)
    {
        return requested.to_owned();
    }
    for suffix in 2..=MAX_SAVED_SELECTION_MASKS {
        let candidate = suffixed_node_name(requested, suffix);
        if !saved_selections
            .iter()
            .any(|selection| selection.name == candidate)
        {
            return candidate;
        }
    }
    suffixed_node_name(requested, saved_selections.len() + 1)
}

fn suffixed_node_name(requested: &str, suffix: usize) -> String {
    let suffix = format!(" {suffix}");
    let maximum_base_bytes = 1_024_usize.saturating_sub(suffix.len());
    let mut end = requested.len().min(maximum_base_bytes);
    while !requested.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{suffix}", &requested[..end])
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
        assert!(validate_layer(&[main.clone(), color]).is_ok());
        assert!(validate_layer(std::slice::from_ref(&main)).is_err());
        assert!(validate_layer(&[main.clone(), main]).is_err());
        assert!(validate_plane_format(PlaneType::MainLine, PixelFormat::StraightRgba8).is_ok());
        let rgba_main = plane(3, PlaneType::MainLine, PixelFormat::StraightRgba16);
        let rgba_color = plane(4, PlaneType::Color, PixelFormat::StraightRgba16);
        assert!(validate_layer(&[rgba_main, rgba_color]).is_ok());
        assert!(
            validate_plane_format(PlaneType::MainLine, PixelFormat::PremultipliedBgra8).is_err()
        );
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
                fill_protection_plane: PlaneId::from_raw(6),
                light_table_set: LightTableSetId::from_raw(7),
                cell: CellId::from_raw(8),
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
        assert!(ids.insert(document.fill_protection_plane_id.get()));
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
