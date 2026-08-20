use super::model::*;
use crate::adjustment::validate_adjustment_metadata;
use crate::light_table::validate_light_table_metadata;
use inkpod_image::{MAX_PALETTE_COLORS, PixelFormat, TileCoord};
use std::collections::BTreeSet;
pub(super) fn validate_document_metadata(
    metadata: &FileDocumentMetadata,
    file_planes: Option<&[FilePlane]>,
) -> Result<(), FormatError> {
    crate::encode_color_chart(&metadata.color_chart)?;
    if metadata.layers.is_empty()
        || metadata.layers.len() > MAX_LAYERS
        || metadata.guides.len() > MAX_GUIDES
        || metadata.vanishing_points.len() > MAX_VANISHING_POINTS
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
    if let Some(frame) = metadata.shooting_frame {
        const LIMIT: u64 = 67_108_864_000;
        if frame.id == 0
            || !ids.insert(frame.id)
            || frame.center_x_milli.unsigned_abs() > LIMIT
            || frame.center_y_milli.unsigned_abs() > LIMIT
            || frame.width_milli == 0
            || frame.height_milli == 0
            || frame.width_milli > LIMIT
            || frame.height_milli > LIMIT
        {
            return Err(FormatError::Invalid(
                "shooting-frame properties are invalid",
            ));
        }
    }
    for point in &metadata.vanishing_points {
        let layer = metadata
            .layers
            .iter()
            .find(|layer| layer.id == point.layer_id)
            .ok_or(FormatError::Invalid(
                "vanishing-point owner layer does not exist",
            ))?;
        const LIMIT: u64 = 67_108_864_000;
        if point.id == 0
            || !ids.insert(point.id)
            || layer.kind != LayerKind::VanishingPoint
            || point.x_milli.unsigned_abs() > LIMIT
            || point.y_milli.unsigned_abs() > LIMIT
            || !(1_000..=180_000).contains(&point.interval_milli_degrees)
            || point.angle_milli_degrees >= 180_000
            || point.opacity_milli > 1_000
            || point.color.rgba16().is_none()
        {
            return Err(FormatError::Invalid(
                "vanishing-point properties are invalid",
            ));
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

pub(super) fn validate_document(document: &DocumentArchive) -> Result<(), FormatError> {
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
        document.cell_id,
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
        document.frames.shooting_frame,
        document.frames.maximum_close_frame,
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

    Ok(())
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
