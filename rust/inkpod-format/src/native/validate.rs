use super::model::*;
use crate::adjustment::validate_adjustment_metadata;
use crate::light_table::validate_light_table_metadata;
use crate::vector::validate_vector_metadata;
use inkpod_image::{MAX_PALETTE_COLORS, PixelFormat, PixelValue, TileCoord};
use std::collections::BTreeSet;
pub(super) fn validate_document_metadata(
    metadata: &FileDocumentMetadata,
    file_planes: Option<&[FilePlane]>,
) -> Result<(), FormatError> {
    if metadata.layers.is_empty()
        || metadata.layers.len() > MAX_LAYERS
        || metadata.guides.len() > MAX_GUIDES
        || metadata.grid.spacing_x == 0
        || metadata.grid.spacing_y == 0
        || metadata.grid.spacing_x > 1_048_576
        || metadata.grid.spacing_y > 1_048_576
        || metadata.grid.subdivisions == 0
        || metadata.grid.subdivisions > 1_024
    {
        return Err(FormatError::Invalid(
            "document metadata values are outside bounds",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut active_layer_found = false;
    let mut active_plane_found = false;
    let mut referenced_planes = BTreeSet::new();
    for layer in &metadata.layers {
        validate_name(&layer.name)?;
        if layer.id == 0 || !ids.insert(layer.id) || layer.opacity_milli > 1_000 {
            return Err(FormatError::Invalid(
                "document layer properties are invalid",
            ));
        }
        active_layer_found |= layer.id == metadata.active_layer_id;
        for plane in &layer.planes {
            validate_name(&plane.name)?;
            if plane.id == 0
                || !ids.insert(plane.id)
                || !referenced_planes.insert(plane.id)
                || plane.opacity_milli > 1_000
            {
                return Err(FormatError::Invalid(
                    "document plane properties are invalid",
                ));
            }
            active_plane_found |= plane.id == metadata.active_plane_id;
        }
    }
    for guide in &metadata.guides {
        if guide.id == 0 || !ids.insert(guide.id) {
            return Err(FormatError::Invalid("guide ID is invalid"));
        }
    }
    if metadata.selection_plane_id == 0
        || !ids.insert(metadata.selection_plane_id)
        || !active_layer_found
        || !active_plane_found
    {
        return Err(FormatError::Invalid(
            "document active or selection ID is invalid",
        ));
    }
    if let Some(planes) = file_planes {
        let plane_ids: BTreeSet<_> = planes
            .iter()
            .filter(|plane| plane.kind != PlaneKind::LightTable)
            .map(|plane| plane.id)
            .collect();
        referenced_planes.insert(metadata.selection_plane_id);
        if referenced_planes != plane_ids {
            return Err(FormatError::Invalid(
                "document layer tree and plane payload IDs differ",
            ));
        }
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), FormatError> {
    if name.is_empty() || name.len() > MAX_NODE_NAME_BYTES || name.chars().any(char::is_control) {
        Err(FormatError::Invalid("node name is invalid"))
    } else {
        Ok(())
    }
}

pub(super) fn validate_document(document: &CellFile) -> Result<(), FormatError> {
    if document.width == 0
        || document.height == 0
        || document.width > inkpod_image::MAX_RASTER_DIMENSION
        || document.height > inkpod_image::MAX_RASTER_DIMENSION
        || document.dpi_x_milli == 0
        || document.dpi_y_milli == 0
    {
        return Err(FormatError::Invalid(
            "document dimensions or DPI are invalid",
        ));
    }
    if document.document_uuid.iter().all(|byte| *byte == 0) {
        return Err(FormatError::Invalid("document UUID must be nonzero"));
    }
    let ids = [
        document.document_id,
        document.layer_id,
        document.main_plane_id,
        document.color_plane_id,
    ];
    if ids.contains(&0) || ids.into_iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(FormatError::Invalid(
            "stable IDs must be nonzero and unique",
        ));
    }
    for frame in [
        document.frames.hundred_frame,
        document.frames.reference_frame,
        document.frames.drawing_frame,
        document.frames.safe_frame,
    ] {
        if frame.width <= 0 || frame.height <= 0 {
            return Err(FormatError::Invalid("frame dimensions must be positive"));
        }
    }
    if document
        .frames
        .margins
        .left
        .checked_add(document.frames.margins.right)
        .is_none_or(|horizontal| horizontal > document.width)
        || document
            .frames
            .margins
            .top
            .checked_add(document.frames.margins.bottom)
            .is_none_or(|vertical| vertical > document.height)
    {
        return Err(FormatError::Invalid("margins exceed document dimensions"));
    }
    if document.planes.len() < 2 || document.planes.len() > MAX_PLANES {
        return Err(FormatError::Invalid(
            "coloring cell plane count is outside bounds",
        ));
    }
    let main = document
        .planes
        .iter()
        .find(|plane| plane.kind == PlaneKind::MainLine)
        .ok_or(FormatError::Invalid("main line plane is missing"))?;
    let color = document
        .planes
        .iter()
        .find(|plane| plane.kind == PlaneKind::Color)
        .ok_or(FormatError::Invalid("color plane is missing"))?;
    if document.main_line_color.rgba16().is_none() {
        return Err(FormatError::Invalid("main-line base color must be RGBA"));
    }
    if document.palette.len() > MAX_PALETTE_COLORS
        || document
            .palette
            .iter()
            .any(|color| color.rgba16().is_none())
    {
        return Err(FormatError::Invalid(
            "palette count or color type is invalid",
        ));
    }
    if main.id != document.main_plane_id
        || !matches!(
            main.pixel_format,
            PixelFormat::BinaryMask8 | PixelFormat::Grayscale8 | PixelFormat::Grayscale16
        )
        || color.id != document.color_plane_id
        || !matches!(
            color.pixel_format,
            PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
        )
    {
        return Err(FormatError::Invalid(
            "plane ID or pixel format is inconsistent",
        ));
    }
    let mut plane_ids = BTreeSet::new();
    for plane in &document.planes {
        if plane.id == 0
            || !plane_ids.insert(plane.id)
            || plane.width == 0
            || plane.height == 0
            || plane.width > inkpod_image::MAX_RASTER_DIMENSION
            || plane.height > inkpod_image::MAX_RASTER_DIMENSION
            || (plane.kind != PlaneKind::LightTable
                && (plane.width != document.width || plane.height != document.height))
            || (plane.kind == PlaneKind::LightTable
                && !matches!(
                    plane.pixel_format,
                    PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
                ))
            || (matches!(
                plane.kind,
                PlaneKind::VectorMainLine | PlaneKind::ColorTrace | PlaneKind::VectorFill
            ) && (plane.pixel_format != PixelFormat::StraightRgba8 || !plane.tiles.is_empty()))
        {
            return Err(FormatError::Invalid("plane manifest is inconsistent"));
        }
        let mut coords = BTreeSet::new();
        for tile in &plane.tiles {
            if !coords.insert(tile.coord) {
                return Err(FormatError::Invalid("duplicate tile coordinates"));
            }
            validate_tile_shape(
                plane.width,
                plane.height,
                plane.pixel_format,
                tile.coord,
                tile.width,
                tile.height,
                tile.bytes.len() as u64,
            )?;
            if plane.pixel_format == PixelFormat::BinaryMask8
                && tile.bytes.iter().any(|value| !matches!(*value, 0 | 255))
            {
                return Err(FormatError::Invalid(
                    "binary mask contains an intermediate value",
                ));
            }
        }
    }
    if let Some(metadata) = &document.document_metadata {
        validate_document_metadata(metadata, Some(&document.planes))?;
        let width = i32::try_from(document.width)
            .map_err(|_| FormatError::Invalid("document width exceeds guide range"))?;
        let height = i32::try_from(document.height)
            .map_err(|_| FormatError::Invalid("document height exceeds guide range"))?;
        if metadata.guides.iter().any(|guide| match guide.axis {
            GuideAxis::Horizontal => !(0..=height).contains(&guide.position),
            GuideAxis::Vertical => !(0..=width).contains(&guide.position),
        }) {
            return Err(FormatError::Invalid(
                "guide position is outside the document",
            ));
        }
        let selection = document
            .planes
            .iter()
            .find(|plane| plane.id == metadata.selection_plane_id)
            .ok_or(FormatError::Invalid("selection plane is missing"))?;
        if selection.kind != PlaneKind::Selection
            || selection.pixel_format != PixelFormat::BinaryMask8
        {
            return Err(FormatError::Invalid(
                "selection plane kind or format is invalid",
            ));
        }
    }
    if let Some(metadata) = &document.light_table_metadata {
        if document.document_metadata.is_none() {
            return Err(FormatError::Invalid(
                "light-table metadata requires the document layer tree",
            ));
        }
        let source_plane_ids: BTreeSet<_> = document
            .planes
            .iter()
            .filter(|plane| plane.kind == PlaneKind::LightTable)
            .map(|plane| plane.id)
            .collect();
        validate_light_table_metadata(metadata, Some(&source_plane_ids))?;
        let mut occupied_ids = BTreeSet::from([document.document_id]);
        if let Some(document_metadata) = &document.document_metadata {
            for layer in &document_metadata.layers {
                if !occupied_ids.insert(layer.id) {
                    return Err(FormatError::Invalid(
                        "light-table state collides with an existing stable ID",
                    ));
                }
                for plane in &layer.planes {
                    if !occupied_ids.insert(plane.id) {
                        return Err(FormatError::Invalid(
                            "light-table state collides with an existing stable ID",
                        ));
                    }
                }
            }
            for id in document_metadata
                .guides
                .iter()
                .map(|guide| guide.id)
                .chain([document_metadata.selection_plane_id])
            {
                if !occupied_ids.insert(id) {
                    return Err(FormatError::Invalid(
                        "light-table state collides with an existing stable ID",
                    ));
                }
            }
        }
        for source_plane_id in &source_plane_ids {
            if !occupied_ids.insert(*source_plane_id) {
                return Err(FormatError::Invalid(
                    "light-table source plane collides with document state",
                ));
            }
        }
        for set in &metadata.sets {
            if !occupied_ids.insert(set.id) {
                return Err(FormatError::Invalid(
                    "light-table set ID collides with document state",
                ));
            }
            for item in &set.items {
                if !occupied_ids.insert(item.id) {
                    return Err(FormatError::Invalid(
                        "light-table item ID collides with document state",
                    ));
                }
                let source = document
                    .planes
                    .iter()
                    .find(|plane| plane.id == item.source_plane_id)
                    .ok_or(FormatError::Invalid("light-table source plane is missing"))?;
                if source.kind != PlaneKind::LightTable {
                    return Err(FormatError::Invalid(
                        "light-table source plane kind is invalid",
                    ));
                }
            }
        }
    } else if document
        .planes
        .iter()
        .any(|plane| plane.kind == PlaneKind::LightTable)
    {
        return Err(FormatError::Invalid(
            "light-table planes require light-table metadata",
        ));
    }

    let adjustment_layer_ids: BTreeSet<_> = document
        .document_metadata
        .iter()
        .flat_map(|metadata| metadata.layers.iter())
        .filter(|layer| layer.kind == LayerKind::Adjustment)
        .map(|layer| layer.id)
        .collect();
    if let Some(metadata) = &document.adjustment_metadata {
        if document.document_metadata.is_none() || adjustment_layer_ids.is_empty() {
            return Err(FormatError::Invalid(
                "adjustment metadata requires an adjustment layer in the document tree",
            ));
        }
        validate_adjustment_metadata(metadata, Some(&adjustment_layer_ids))?;
    } else if !adjustment_layer_ids.is_empty() {
        return Err(FormatError::Invalid(
            "adjustment layers require adjustment metadata",
        ));
    }

    let mut stroke_plane_ids = BTreeSet::new();
    let mut fill_plane_ids = BTreeSet::new();
    let mut vector_layer_for_plane = std::collections::BTreeMap::new();
    let mut has_vector_layer = false;
    if let Some(document_metadata) = &document.document_metadata {
        for layer in &document_metadata.layers {
            let payloads: Vec<_> = layer
                .planes
                .iter()
                .map(|properties| {
                    document
                        .planes
                        .iter()
                        .find(|plane| plane.id == properties.id)
                        .ok_or(FormatError::Invalid("vector plane payload is missing"))
                })
                .collect::<Result<_, _>>()?;
            if layer.kind == LayerKind::VectorColoring {
                has_vector_layer = true;
                let main_count = payloads
                    .iter()
                    .filter(|plane| plane.kind == PlaneKind::VectorMainLine)
                    .count();
                let trace_count = payloads
                    .iter()
                    .filter(|plane| plane.kind == PlaneKind::ColorTrace)
                    .count();
                let fill_count = payloads
                    .iter()
                    .filter(|plane| plane.kind == PlaneKind::VectorFill)
                    .count();
                if main_count != 1
                    || trace_count == 0
                    || fill_count != 1
                    || payloads.iter().any(|plane| {
                        !matches!(
                            plane.kind,
                            PlaneKind::VectorMainLine
                                | PlaneKind::ColorTrace
                                | PlaneKind::VectorFill
                                | PlaneKind::Raster
                        )
                    })
                {
                    return Err(FormatError::Invalid(
                        "vector layer and plane types are inconsistent",
                    ));
                }
                for plane in payloads {
                    match plane.kind {
                        PlaneKind::VectorMainLine | PlaneKind::ColorTrace => {
                            stroke_plane_ids.insert(plane.id);
                            vector_layer_for_plane.insert(plane.id, layer.id);
                        }
                        PlaneKind::VectorFill => {
                            fill_plane_ids.insert(plane.id);
                            vector_layer_for_plane.insert(plane.id, layer.id);
                        }
                        _ => {}
                    }
                }
            } else if payloads.iter().any(|plane| {
                matches!(
                    plane.kind,
                    PlaneKind::VectorMainLine | PlaneKind::ColorTrace | PlaneKind::VectorFill
                )
            }) {
                return Err(FormatError::Invalid(
                    "vector plane belongs to a non-vector layer",
                ));
            }
        }
    }
    if let Some(metadata) = &document.vector_metadata {
        if document.document_metadata.is_none() || !has_vector_layer {
            return Err(FormatError::Invalid(
                "vector metadata requires a vector layer in the document tree",
            ));
        }
        validate_vector_metadata(
            metadata,
            Some(&stroke_plane_ids),
            Some(&fill_plane_ids),
            Some(&vector_layer_for_plane),
        )?;
        let mut occupied_ids = BTreeSet::from([document.document_id]);
        if let Some(document_metadata) = &document.document_metadata {
            for layer in &document_metadata.layers {
                occupied_ids.insert(layer.id);
                for plane in &layer.planes {
                    occupied_ids.insert(plane.id);
                }
            }
            for id in document_metadata
                .guides
                .iter()
                .map(|guide| guide.id)
                .chain([document_metadata.selection_plane_id])
            {
                occupied_ids.insert(id);
            }
        }
        if let Some(light_table_metadata) = &document.light_table_metadata {
            for set in &light_table_metadata.sets {
                occupied_ids.insert(set.id);
                for item in &set.items {
                    occupied_ids.insert(item.id);
                    occupied_ids.insert(item.source_plane_id);
                }
            }
        }
        for path in &metadata.paths {
            if !occupied_ids.insert(path.id) {
                return Err(FormatError::Invalid(
                    "vector path collides with an existing stable ID",
                ));
            }
        }
        for fill in &metadata.fills {
            if !occupied_ids.insert(fill.id) {
                return Err(FormatError::Invalid(
                    "vector fill collides with an existing stable ID",
                ));
            }
            let fill_layer = vector_layer_for_plane
                .get(&fill.plane_id)
                .ok_or(FormatError::Invalid("vector fill plane is missing"))?;
            for boundary_id in &fill.boundary_path_ids {
                let boundary = metadata
                    .paths
                    .iter()
                    .find(|path| path.id == *boundary_id)
                    .ok_or(FormatError::Invalid("vector fill boundary is missing"))?;
                if vector_layer_for_plane.get(&boundary.plane_id) != Some(fill_layer) {
                    return Err(FormatError::Invalid(
                        "vector fill boundary crosses vector layers",
                    ));
                }
            }
        }
    } else if has_vector_layer
        || document.planes.iter().any(|plane| {
            matches!(
                plane.kind,
                PlaneKind::VectorMainLine | PlaneKind::ColorTrace | PlaneKind::VectorFill
            )
        })
    {
        return Err(FormatError::Invalid(
            "vector layers require vector metadata",
        ));
    }
    Ok(())
}

pub(super) fn legacy_main_line_color(document: &CellFile) -> Result<PixelValue, FormatError> {
    legacy_main_line_color_for_planes(&document.planes)
}

pub(super) fn legacy_main_line_color_for_planes(
    planes: &[FilePlane],
) -> Result<PixelValue, FormatError> {
    let color = planes
        .iter()
        .find(|plane| plane.kind == PlaneKind::Color)
        .ok_or(FormatError::Invalid("color plane is missing"))?;
    Ok(if color.pixel_format == PixelFormat::StraightRgba16 {
        PixelValue::Rgba16([0, 0, 0, u16::MAX])
    } else {
        PixelValue::Rgba([0, 0, 0, u8::MAX])
    })
}

pub(super) fn validate_tile_shape(
    raster_width: u32,
    raster_height: u32,
    format: PixelFormat,
    coord: TileCoord,
    width: u32,
    height: u32,
    length: u64,
) -> Result<(), FormatError> {
    let origin_x = coord
        .x
        .checked_mul(inkpod_image::TILE_SIZE)
        .ok_or(FormatError::Invalid("tile X origin overflows"))?;
    let origin_y = coord
        .y
        .checked_mul(inkpod_image::TILE_SIZE)
        .ok_or(FormatError::Invalid("tile Y origin overflows"))?;
    if origin_x >= raster_width || origin_y >= raster_height {
        return Err(FormatError::Invalid("tile origin is outside its plane"));
    }
    let expected_width = inkpod_image::TILE_SIZE.min(raster_width - origin_x);
    let expected_height = inkpod_image::TILE_SIZE.min(raster_height - origin_y);
    let expected_length = u64::from(expected_width)
        .checked_mul(u64::from(expected_height))
        .and_then(|pixels| pixels.checked_mul(format.bytes_per_pixel() as u64))
        .ok_or(FormatError::Invalid("tile byte length overflows"))?;
    if width != expected_width || height != expected_height || length != expected_length {
        return Err(FormatError::Invalid(
            "tile dimensions or byte length are inconsistent",
        ));
    }
    Ok(())
}
