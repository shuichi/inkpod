use super::model::*;
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
        || metadata.saved_selections.len() > MAX_SAVED_SELECTION_MASKS
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
        if layer.id == 0
            || !ids.insert(layer.id)
            || layer.opacity_milli > 1_000
            || layer.planes.len() < 2
            || layer.planes.len() > MAX_PLANES
        {
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
    let mut saved_selection_names = BTreeSet::new();
    for saved_selection in &metadata.saved_selections {
        validate_name(&saved_selection.name)?;
        if saved_selection.id == 0
            || !ids.insert(saved_selection.id)
            || !referenced_planes.insert(saved_selection.id)
            || !saved_selection_names.insert(saved_selection.name.as_str())
        {
            return Err(FormatError::Invalid(
                "saved-selection properties are invalid",
            ));
        }
    }
    if metadata.selection_plane_id == 0
        || !ids.insert(metadata.selection_plane_id)
        || metadata.fill_protection_plane_id == 0
        || !ids.insert(metadata.fill_protection_plane_id)
        || !active_layer_found
        || !active_plane_found
    {
        return Err(FormatError::Invalid(
            "document active, selection, or fill-protection ID is invalid",
        ));
    }
    if let Some(planes) = file_planes {
        for layer in &metadata.layers {
            let mut main_line_count = 0_usize;
            let mut color_count = 0_usize;
            for properties in &layer.planes {
                let plane = planes
                    .iter()
                    .find(|plane| plane.id == properties.id)
                    .ok_or(FormatError::Invalid("document plane payload is missing"))?;
                match plane.kind {
                    PlaneKind::MainLine => main_line_count += 1,
                    PlaneKind::Color => color_count += 1,
                    PlaneKind::Raster => {}
                    PlaneKind::CurrentSelection
                    | PlaneKind::SavedSelection
                    | PlaneKind::FillProtection
                    | PlaneKind::LightTable => {
                        return Err(FormatError::Invalid(
                            "document layer contains a non-image plane",
                        ));
                    }
                }
            }
            if main_line_count != 1 || color_count != 1 {
                return Err(FormatError::Invalid(
                    "document layer must contain one main-line and one color plane",
                ));
            }
        }
        let plane_ids: BTreeSet<_> = planes
            .iter()
            .filter(|plane| plane.kind != PlaneKind::LightTable)
            .map(|plane| plane.id)
            .collect();
        referenced_planes.insert(metadata.selection_plane_id);
        referenced_planes.insert(metadata.fill_protection_plane_id);
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
        .find(|plane| plane.id == document.main_plane_id)
        .ok_or(FormatError::Invalid("main line plane is missing"))?;
    let color = document
        .planes
        .iter()
        .find(|plane| plane.id == document.color_plane_id)
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
    if main.kind != PlaneKind::MainLine
        || !matches!(
            main.pixel_format,
            PixelFormat::BinaryMask8
                | PixelFormat::Grayscale8
                | PixelFormat::Grayscale16
                | PixelFormat::StraightRgba8
                | PixelFormat::StraightRgba16
        )
        || color.kind != PlaneKind::Color
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
        let format_is_valid = match plane.kind {
            PlaneKind::MainLine => matches!(
                plane.pixel_format,
                PixelFormat::BinaryMask8
                    | PixelFormat::Grayscale8
                    | PixelFormat::Grayscale16
                    | PixelFormat::StraightRgba8
                    | PixelFormat::StraightRgba16
            ),
            PlaneKind::Color | PlaneKind::Raster | PlaneKind::LightTable => matches!(
                plane.pixel_format,
                PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
            ),
            PlaneKind::CurrentSelection | PlaneKind::SavedSelection | PlaneKind::FillProtection => {
                plane.pixel_format == PixelFormat::BinaryMask8
            }
        };
        if plane.id == 0
            || !plane_ids.insert(plane.id)
            || plane.width == 0
            || plane.height == 0
            || plane.width > inkpod_image::MAX_RASTER_DIMENSION
            || plane.height > inkpod_image::MAX_RASTER_DIMENSION
            || (plane.kind != PlaneKind::LightTable
                && (plane.width != document.width || plane.height != document.height))
            || !format_is_valid
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
    let document_metadata = document
        .document_metadata
        .as_ref()
        .ok_or(FormatError::Invalid(
            "current document metadata is required",
        ))?;
    {
        let metadata = document_metadata;
        validate_document_metadata(metadata, Some(&document.planes))?;
        let mut object_ids = BTreeSet::from([document.document_id, document.cell_id]);
        for id in document
            .planes
            .iter()
            .filter(|plane| plane.kind != PlaneKind::LightTable)
            .map(|plane| plane.id)
            .chain(metadata.layers.iter().map(|layer| layer.id))
            .chain(metadata.guides.iter().map(|guide| guide.id))
            .chain(metadata.shooting_frame.iter().map(|frame| frame.id))
        {
            if !object_ids.insert(id) {
                return Err(FormatError::Invalid(
                    "document stable object IDs are not globally unique",
                ));
            }
        }
        let primary_layer = metadata
            .layers
            .iter()
            .find(|layer| layer.id == document.layer_id)
            .ok_or(FormatError::Invalid("primary layer is missing"))?;
        if !primary_layer
            .planes
            .iter()
            .any(|plane| plane.id == document.main_plane_id)
            || !primary_layer
                .planes
                .iter()
                .any(|plane| plane.id == document.color_plane_id)
        {
            return Err(FormatError::Invalid(
                "primary layer does not own the primary planes",
            ));
        }
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
        if selection.kind != PlaneKind::CurrentSelection
            || selection.pixel_format != PixelFormat::BinaryMask8
        {
            return Err(FormatError::Invalid(
                "selection plane kind or format is invalid",
            ));
        }
        let fill_protection = document
            .planes
            .iter()
            .find(|plane| plane.id == metadata.fill_protection_plane_id)
            .ok_or(FormatError::Invalid("fill-protection plane is missing"))?;
        if fill_protection.kind != PlaneKind::FillProtection
            || fill_protection.pixel_format != PixelFormat::BinaryMask8
        {
            return Err(FormatError::Invalid(
                "fill-protection plane kind or format is invalid",
            ));
        }
        for saved_selection in &metadata.saved_selections {
            let plane = document
                .planes
                .iter()
                .find(|plane| plane.id == saved_selection.id)
                .ok_or(FormatError::Invalid("saved-selection plane is missing"))?;
            if plane.kind != PlaneKind::SavedSelection
                || plane.pixel_format != PixelFormat::BinaryMask8
            {
                return Err(FormatError::Invalid(
                    "saved-selection plane kind or format is invalid",
                ));
            }
        }
    }
    if let Some(metadata) = &document.light_table_metadata {
        let source_plane_ids: BTreeSet<_> = document
            .planes
            .iter()
            .filter(|plane| plane.kind == PlaneKind::LightTable)
            .map(|plane| plane.id)
            .collect();
        validate_light_table_metadata(metadata, Some(&source_plane_ids))?;
        let mut occupied_ids = BTreeSet::from([document.document_id, document.cell_id]);
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
            .chain([
                document_metadata.selection_plane_id,
                document_metadata.fill_protection_plane_id,
            ])
            .chain(
                document_metadata
                    .saved_selections
                    .iter()
                    .map(|selection| selection.id),
            )
            .chain(
                document_metadata
                    .shooting_frame
                    .iter()
                    .map(|frame| frame.id),
            )
        {
            if !occupied_ids.insert(id) {
                return Err(FormatError::Invalid(
                    "light-table state collides with an existing stable ID",
                ));
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
