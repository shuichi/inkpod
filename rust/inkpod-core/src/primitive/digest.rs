//! Memory-layout-independent semantic document-state serialization and hashing.

use super::*;
use crate::*;

const DOCUMENT_STATE_CONTEXT: &str = "org.inkpod.digest.document-state.v2";
const ASSET_CONTEXT: &str = "org.inkpod.digest.asset.v1";
const PROCEDURE_PAYLOAD_CONTEXT: &str = "org.inkpod.digest.procedure-payload.v1";
const DOCUMENT_STATE_SCHEMA_VERSION: u32 = 2;
const ASSET_SCHEMA_VERSION: u32 = 1;
const PROCEDURE_PAYLOAD_SCHEMA_VERSION: u32 = 1;

pub(super) fn canonical_document_state(
    document: &CellDocument,
) -> Result<(Vec<u8>, DocumentStateDigest), CoreError> {
    let paper = frame(&[
        present(document.width.to_le_bytes()),
        present(document.height.to_le_bytes()),
        present(document.dpi_x_milli.to_le_bytes()),
        present(document.dpi_y_milli.to_le_bytes()),
        present(1_u32.to_le_bytes()),
    ])?;
    let frames = frame(&[
        present(rectangle_bytes(document.frames.hundred_frame)?),
        present(rectangle_bytes(document.frames.reference_frame)?),
        present(rectangle_bytes(document.frames.drawing_frame)?),
        present(rectangle_bytes(document.frames.safe_frame)?),
        present(margins_bytes(document.frames.margins)?),
    ])?;
    let base_surface = frame(&[present(1_u32.to_le_bytes())])?;
    let layer_tree = canonical_layer_tree(document)?;
    let selection = frame(&[
        present(document.selection_plane_id.get().to_le_bytes()),
        present(canonical_raster(&document.selection)?),
    ])?;
    let palette = sequence(
        document
            .palette
            .colors()
            .iter()
            .copied()
            .map(color_bytes)
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .map(Vec::as_slice),
    )?;
    let guide_frames = document
        .guides
        .iter()
        .map(|guide| {
            let axis = match guide.axis {
                GuideAxis::Horizontal => 1_u32,
                GuideAxis::Vertical => 2_u32,
            };
            frame(&[
                present(guide.id.to_le_bytes()),
                present(axis.to_le_bytes()),
                present(integer_q16(guide.position, "guide position")?.to_le_bytes()),
            ])
        })
        .collect::<Result<Vec<_>, _>>()?;
    let guides = sequence(guide_frames.iter().map(Vec::as_slice))?;
    let grid = frame(&[
        present(integer_q16(document.grid.origin_x, "grid x origin")?.to_le_bytes()),
        present(integer_q16(document.grid.origin_y, "grid y origin")?.to_le_bytes()),
        present(unsigned_integer_q16(document.grid.spacing_x, "grid x spacing")?.to_le_bytes()),
        present(unsigned_integer_q16(document.grid.spacing_y, "grid y spacing")?.to_le_bytes()),
        present(document.grid.subdivisions.to_le_bytes()),
    ])?;
    let empty_sequence = sequence(std::iter::empty::<&[u8]>())?;
    let light_table = canonical_light_table(document)?;
    let hierarchy = frame(&[
        absent(),
        absent(),
        // The current standalone-cell model has one persistent document
        // identity. Its successor Cell identity is introduced with Genesis;
        // until then the stable document ID is the standalone cell identity.
        present(document.id.get().to_le_bytes()),
        present(empty_sequence.clone()),
        present(empty_sequence.clone()),
    ])?;
    let bytes = frame(&[
        present(document.uuid.to_be_bytes()),
        present(document.id.get().to_le_bytes()),
        present(paper),
        present(frames),
        present(base_surface),
        present(layer_tree),
        present(selection),
        present(palette),
        present(color_bytes(document.main_line_color)?),
        present(guides),
        present(grid),
        present(light_table),
        present(hierarchy),
        present(empty_sequence),
    ])?;
    let mut hasher = blake3::Hasher::new_derive_key(DOCUMENT_STATE_CONTEXT);
    hasher.update(&bytes);
    let digest = DocumentStateDigest::from_bytes(*hasher.finalize().as_bytes());
    Ok((bytes, digest))
}

pub(super) fn canonical_payload_digest(payload: &[u8]) -> Result<[u8; 32], CoreError> {
    if payload.is_empty() {
        return Ok([0; 32]);
    }
    let message = frame_with_schema(
        PROCEDURE_PAYLOAD_SCHEMA_VERSION,
        &[present(payload.to_vec())],
    )?;
    let mut hasher = blake3::Hasher::new_derive_key(PROCEDURE_PAYLOAD_CONTEXT);
    hasher.update(&message);
    Ok(*hasher.finalize().as_bytes())
}

fn canonical_layer_tree(document: &CellDocument) -> Result<Vec<u8>, CoreError> {
    let vector = document.vector.to_file(
        document
            .layers
            .iter()
            .any(|layer| layer.kind == LayerKind::VectorColoring),
    );
    let layers = document
        .layers
        .iter()
        .map(|layer| canonical_layer(document, layer, vector.as_ref()))
        .collect::<Result<Vec<_>, _>>()?;
    sequence(layers.iter().map(Vec::as_slice))
}

fn canonical_layer(
    document: &CellDocument,
    layer: &LayerNode,
    vector: Option<&inkpod_format::FileVectorMetadata>,
) -> Result<Vec<u8>, CoreError> {
    let planes = layer
        .planes
        .iter()
        .map(|plane| canonical_plane(plane, vector))
        .collect::<Result<Vec<_>, _>>()?;
    let adjustment = document
        .adjustments
        .get(&layer.id)
        .map(canonical_adjustment)
        .transpose()?;
    frame(&[
        present(layer.id.get().to_le_bytes()),
        present(layer_kind_code(layer.kind).to_le_bytes()),
        present(layer.name.as_bytes().to_vec()),
        present(boolean_bytes(layer.visible)),
        present(boolean_bytes(layer.editable)),
        present(normalized_opacity(layer.opacity_milli)?.to_le_bytes()),
        present(sequence(planes.iter().map(Vec::as_slice))?),
        adjustment,
    ])
}

fn canonical_plane(
    plane: &PlaneNode,
    vector: Option<&inkpod_format::FileVectorMetadata>,
) -> Result<Vec<u8>, CoreError> {
    let paths = vector
        .into_iter()
        .flat_map(|metadata| &metadata.paths)
        .filter(|path| path.plane_id == plane.id.get())
        .map(canonical_vector_path)
        .collect::<Result<Vec<_>, _>>()?;
    let fills = vector
        .into_iter()
        .flat_map(|metadata| &metadata.fills)
        .filter(|fill| fill.plane_id == plane.id.get())
        .map(canonical_vector_fill)
        .collect::<Result<Vec<_>, _>>()?;
    let raster = match plane.kind {
        PlaneType::VectorMainLine | PlaneType::ColorTrace | PlaneType::VectorFill => None,
        PlaneType::MainLine | PlaneType::Color | PlaneType::Raster | PlaneType::Selection => {
            Some(canonical_raster(&plane.raster)?)
        }
    };
    frame(&[
        present(plane.id.get().to_le_bytes()),
        present(plane_kind_code(plane.kind).to_le_bytes()),
        present(pixel_format_code(plane.raster.format())?.to_le_bytes()),
        present(plane.name.as_bytes().to_vec()),
        present(boolean_bytes(plane.visible)),
        present(boolean_bytes(plane.editable)),
        present(normalized_opacity(plane.opacity_milli)?.to_le_bytes()),
        raster,
        present(sequence(paths.iter().map(Vec::as_slice))?),
        present(sequence(fills.iter().map(Vec::as_slice))?),
    ])
}

fn canonical_vector_path(path: &inkpod_format::FileVectorPath) -> Result<Vec<u8>, CoreError> {
    let segments = path
        .segments
        .iter()
        .map(|segment| {
            let points = [segment.p0, segment.p1, segment.p2, segment.p3];
            frame(&[
                present(milli_q16(i64::from(points[0].x_milli), "vector p0 x")?.to_le_bytes()),
                present(milli_q16(i64::from(points[0].y_milli), "vector p0 y")?.to_le_bytes()),
                present(milli_q16(i64::from(points[1].x_milli), "vector p1 x")?.to_le_bytes()),
                present(milli_q16(i64::from(points[1].y_milli), "vector p1 y")?.to_le_bytes()),
                present(milli_q16(i64::from(points[2].x_milli), "vector p2 x")?.to_le_bytes()),
                present(milli_q16(i64::from(points[2].y_milli), "vector p2 y")?.to_le_bytes()),
                present(milli_q16(i64::from(points[3].x_milli), "vector p3 x")?.to_le_bytes()),
                present(milli_q16(i64::from(points[3].y_milli), "vector p3 y")?.to_le_bytes()),
                present(
                    milli_q16(i64::from(segment.width_start_milli), "vector start width")?
                        .to_le_bytes(),
                ),
                present(
                    milli_q16(i64::from(segment.width_end_milli), "vector end width")?
                        .to_le_bytes(),
                ),
            ])
        })
        .collect::<Result<Vec<_>, _>>()?;
    frame(&[
        present(path.id.to_le_bytes()),
        present(path.plane_id.to_le_bytes()),
        present(color_bytes(path.color)?),
        present(boolean_bytes(path.closed)),
        present(sequence(segments.iter().map(Vec::as_slice))?),
    ])
}

fn canonical_vector_fill(fill: &inkpod_format::FileVectorFill) -> Result<Vec<u8>, CoreError> {
    let boundaries = fill
        .boundary_path_ids
        .iter()
        .map(|id| id.to_le_bytes().to_vec())
        .collect::<Vec<_>>();
    frame(&[
        present(fill.id.to_le_bytes()),
        present(fill.plane_id.to_le_bytes()),
        present(color_bytes(fill.color)?),
        present(sequence(boundaries.iter().map(Vec::as_slice))?),
    ])
}

fn canonical_adjustment(adjustment: &Adjustment) -> Result<Vec<u8>, CoreError> {
    let (kind, channel, interpolation, parameters, points) = match adjustment {
        Adjustment::BrightnessContrast {
            brightness_milli,
            contrast_milli,
        } => (
            1_u32,
            0_u32,
            0_u32,
            [*brightness_milli, *contrast_milli, 0, 0, 0, 0],
            Vec::new(),
        ),
        Adjustment::ToneCurve {
            channel,
            interpolation,
            points,
        } => (
            2_u32,
            channel_code(*channel),
            interpolation_code(*interpolation),
            [0; 6],
            points
                .iter()
                .map(|point| {
                    let mut bytes = Vec::with_capacity(4);
                    bytes.extend_from_slice(&point.input.to_le_bytes());
                    bytes.extend_from_slice(&point.output.to_le_bytes());
                    bytes
                })
                .collect(),
        ),
        Adjustment::Levels(levels) => (
            3_u32,
            channel_code(levels.channel),
            0_u32,
            [
                i32::from(levels.input_shadow),
                i32::try_from(levels.input_gamma_milli).map_err(|_| {
                    CoreError::InvalidState("level gamma is not representable as i32")
                })?,
                i32::from(levels.input_highlight),
                i32::from(levels.output_shadow),
                i32::from(levels.output_highlight),
                0,
            ],
            Vec::new(),
        ),
    };
    frame(&[
        present(kind.to_le_bytes()),
        present(channel.to_le_bytes()),
        present(interpolation.to_le_bytes()),
        present(parameters[0].to_le_bytes()),
        present(parameters[1].to_le_bytes()),
        present(parameters[2].to_le_bytes()),
        present(parameters[3].to_le_bytes()),
        present(parameters[4].to_le_bytes()),
        present(parameters[5].to_le_bytes()),
        present(sequence(points.iter().map(Vec::as_slice))?),
    ])
}

fn canonical_light_table(document: &CellDocument) -> Result<Vec<u8>, CoreError> {
    let metadata = document.light_table.to_file();
    let source_planes = document
        .light_table
        .file_planes()
        .into_iter()
        .map(|plane| (plane.id, plane))
        .collect::<BTreeMap<_, _>>();
    let sets = metadata
        .sets
        .iter()
        .map(|set| {
            let items =
                set.items
                    .iter()
                    .map(|item| {
                        let source = source_planes.get(&item.source_plane_id).ok_or(
                            CoreError::InvalidState("light-table source raster is missing"),
                        )?;
                        let mode = match item.display_mode {
                            LightTableDisplayMode::Color => 1_u32,
                            LightTableDisplayMode::Monotone => 2_u32,
                            LightTableDisplayMode::Halftone => 3_u32,
                        };
                        frame(&[
                            present(item.id.to_le_bytes()),
                            present(canonical_raster_asset_id(source)?.to_vec()),
                            present(reference_origin_bytes(item.source_reference_frame)?),
                            present(item.name.as_bytes().to_vec()),
                            present(boolean_bytes(item.visible)),
                            present(normalized_opacity(item.opacity_milli)?.to_le_bytes()),
                            present(mode.to_le_bytes()),
                            present(color_bytes(item.display_color)?),
                            present(
                                milli_q16(
                                    i64::from(item.translate_x_milli),
                                    "light-table x translation",
                                )?
                                .to_le_bytes(),
                            ),
                            present(
                                milli_q16(
                                    i64::from(item.translate_y_milli),
                                    "light-table y translation",
                                )?
                                .to_le_bytes(),
                            ),
                            present(
                                milli_q16(i64::from(item.scale_x_milli), "light-table x scale")?
                                    .to_le_bytes(),
                            ),
                            present(
                                milli_q16(i64::from(item.scale_y_milli), "light-table y scale")?
                                    .to_le_bytes(),
                            ),
                            present(turn_rotation(item.rotation_milli_degrees)?.to_le_bytes()),
                        ])
                    })
                    .collect::<Result<Vec<_>, CoreError>>()?;
            frame(&[
                present(set.id.to_le_bytes()),
                present(set.name.as_bytes().to_vec()),
                present(normalized_opacity(set.global_opacity_milli)?.to_le_bytes()),
                present(sequence(items.iter().map(Vec::as_slice))?),
            ])
        })
        .collect::<Result<Vec<_>, CoreError>>()?;
    frame(&[
        (metadata.active_set_id != 0).then(|| metadata.active_set_id.to_le_bytes().to_vec()),
        present(sequence(sets.iter().map(Vec::as_slice))?),
    ])
}

fn canonical_raster_asset_id(plane: &FilePlane) -> Result<[u8; 32], CoreError> {
    let pixel_format = pixel_format_code(plane.pixel_format)?;
    let (color_space, alpha_semantics) = match plane.pixel_format {
        PixelFormat::BinaryMask8 => (None, 3_u32),
        PixelFormat::Grayscale8 | PixelFormat::Grayscale16 => (None, 1_u32),
        PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16 => (Some(1_u32), 2_u32),
        PixelFormat::PremultipliedBgra8 => {
            return Err(CoreError::InvalidState(
                "display-only pixel format cannot enter an asset digest",
            ));
        }
    };
    let bytes_per_pixel = u64::try_from(plane.pixel_format.bytes_per_pixel())
        .map_err(|_| CoreError::InvalidState("asset pixel size is not representable"))?;
    let stride = u64::from(plane.width)
        .checked_mul(bytes_per_pixel)
        .ok_or(CoreError::InvalidState("asset stride overflows"))?;
    let element_count = u64::from(plane.width)
        .checked_mul(u64::from(plane.height))
        .ok_or(CoreError::InvalidState("asset element count overflows"))?;
    let payload_length = stride
        .checked_mul(u64::from(plane.height))
        .ok_or(CoreError::InvalidState("asset payload length overflows"))?;

    let mut prefix = Vec::new();
    push_frame_prefix(&mut prefix, 11);
    push_encoded_field(&mut prefix, 1, Some(&1_u32.to_le_bytes()))?;
    push_encoded_field(&mut prefix, 2, Some(&1_u32.to_le_bytes()))?;
    push_encoded_field(&mut prefix, 3, Some(&pixel_format.to_le_bytes()))?;
    let color_space_bytes = color_space.map(u32::to_le_bytes);
    push_encoded_field(
        &mut prefix,
        4,
        color_space_bytes.as_ref().map(|bytes| bytes.as_slice()),
    )?;
    push_encoded_field(&mut prefix, 5, Some(&alpha_semantics.to_le_bytes()))?;
    push_encoded_field(&mut prefix, 6, Some(&plane.width.to_le_bytes()))?;
    push_encoded_field(&mut prefix, 7, Some(&plane.height.to_le_bytes()))?;
    push_encoded_field(&mut prefix, 8, Some(&stride.to_le_bytes()))?;
    push_encoded_field(&mut prefix, 9, Some(&element_count.to_le_bytes()))?;
    push_encoded_field(&mut prefix, 10, Some(&payload_length.to_le_bytes()))?;
    push_field_header(&mut prefix, 11, true, payload_length);

    let mut tiles = BTreeMap::new();
    for tile in &plane.tiles {
        let expected_width = plane
            .width
            .saturating_sub(tile.coord.x.saturating_mul(TILE_SIZE))
            .min(TILE_SIZE);
        let expected_height = plane
            .height
            .saturating_sub(tile.coord.y.saturating_mul(TILE_SIZE))
            .min(TILE_SIZE);
        let expected_length = u64::from(expected_width)
            .checked_mul(u64::from(expected_height))
            .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
            .ok_or(CoreError::InvalidState("asset tile length overflows"))?;
        if expected_width == 0
            || expected_height == 0
            || tile.width != expected_width
            || tile.height != expected_height
            || u64::try_from(tile.bytes.len()).ok() != Some(expected_length)
            || tiles.insert((tile.coord.y, tile.coord.x), tile).is_some()
        {
            return Err(CoreError::InvalidState(
                "light-table asset tile layout is invalid",
            ));
        }
    }

    let row_length = usize::try_from(stride)
        .map_err(|_| CoreError::InvalidState("asset row length is not representable"))?;
    let bytes_per_pixel = usize::try_from(bytes_per_pixel)
        .map_err(|_| CoreError::InvalidState("asset pixel size is not representable"))?;
    let tile_columns = plane.width.div_ceil(TILE_SIZE);
    let mut row = Vec::new();
    row.try_reserve_exact(row_length)
        .map_err(|_| CoreError::InvalidState("asset row allocation failed"))?;
    row.resize(row_length, 0);
    let mut hasher = blake3::Hasher::new_derive_key(ASSET_CONTEXT);
    hasher.update(&prefix);
    for y in 0..plane.height {
        row.fill(0);
        let tile_y = y / TILE_SIZE;
        let local_y = usize::try_from(y % TILE_SIZE)
            .map_err(|_| CoreError::InvalidState("asset row index is not representable"))?;
        for tile_x in 0..tile_columns {
            let Some(tile) = tiles.get(&(tile_y, tile_x)) else {
                continue;
            };
            if local_y >= tile.height as usize {
                continue;
            }
            let source_row_length = tile.width as usize * bytes_per_pixel;
            let source_start = local_y * source_row_length;
            let destination_start = tile_x as usize * TILE_SIZE as usize * bytes_per_pixel;
            row[destination_start..destination_start + source_row_length]
                .copy_from_slice(&tile.bytes[source_start..source_start + source_row_length]);
        }
        hasher.update(&row);
    }
    Ok(*hasher.finalize().as_bytes())
}

const fn layer_kind_code(kind: LayerKind) -> u32 {
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

const fn plane_kind_code(kind: PlaneType) -> u32 {
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

fn pixel_format_code(format: PixelFormat) -> Result<u32, CoreError> {
    match format {
        PixelFormat::BinaryMask8 => Ok(1),
        PixelFormat::Grayscale8 => Ok(2),
        PixelFormat::Grayscale16 => Ok(3),
        PixelFormat::StraightRgba8 => Ok(4),
        PixelFormat::StraightRgba16 => Ok(5),
        PixelFormat::PremultipliedBgra8 => Err(CoreError::InvalidState(
            "display-only pixel format cannot enter document-state digest",
        )),
    }
}

const fn channel_code(channel: Channel) -> u32 {
    match channel {
        Channel::Rgb => 1,
        Channel::Red => 2,
        Channel::Green => 3,
        Channel::Blue => 4,
    }
}

const fn interpolation_code(interpolation: CurveInterpolation) -> u32 {
    match interpolation {
        CurveInterpolation::Bezier => 1,
        CurveInterpolation::BSpline => 2,
    }
}

const fn boolean_bytes(value: bool) -> [u8; 1] {
    [value as u8]
}

fn normalized_opacity(opacity_milli: u32) -> Result<u16, CoreError> {
    if opacity_milli > 1_000 {
        return Err(CoreError::InvalidState(
            "document opacity is outside its canonical range",
        ));
    }
    let scaled = round_ties_even(
        i128::from(opacity_milli) * i128::from(u16::MAX),
        1_000,
        "document opacity",
    )?;
    u16::try_from(scaled)
        .map_err(|_| CoreError::InvalidState("canonical opacity is not representable"))
}

fn integer_q16(value: i32, label: &'static str) -> Result<i64, CoreError> {
    i64::from(value)
        .checked_mul(65_536)
        .ok_or(CoreError::InvalidState(label))
}

fn unsigned_integer_q16(value: u32, label: &'static str) -> Result<i64, CoreError> {
    i64::from(value)
        .checked_mul(65_536)
        .ok_or(CoreError::InvalidState(label))
}

fn milli_q16(value: i64, label: &'static str) -> Result<i64, CoreError> {
    let numerator = i128::from(value)
        .checked_mul(65_536)
        .ok_or(CoreError::InvalidState(label))?;
    let rounded = round_ties_even(numerator, 1_000, label)?;
    i64::try_from(rounded).map_err(|_| CoreError::InvalidState(label))
}

fn turn_rotation(rotation_milli_degrees: i32) -> Result<u32, CoreError> {
    let full_turn = 1_i128 << 32;
    let numerator = i128::from(rotation_milli_degrees)
        .checked_mul(full_turn)
        .ok_or(CoreError::InvalidState(
            "light-table turn rotation overflows",
        ))?;
    let rounded = round_ties_even(numerator, 360_000, "light-table turn rotation")?;
    u32::try_from(rounded.rem_euclid(full_turn))
        .map_err(|_| CoreError::InvalidState("light-table turn rotation is not representable"))
}

fn round_ties_even(
    numerator: i128,
    denominator: i128,
    label: &'static str,
) -> Result<i128, CoreError> {
    if denominator <= 0 {
        return Err(CoreError::InvalidState(label));
    }
    let quotient = numerator.div_euclid(denominator);
    let remainder = numerator.rem_euclid(denominator);
    let doubled = remainder
        .checked_mul(2)
        .ok_or(CoreError::InvalidState(label))?;
    if doubled > denominator || (doubled == denominator && quotient & 1 != 0) {
        quotient
            .checked_add(1)
            .ok_or(CoreError::InvalidState(label))
    } else {
        Ok(quotient)
    }
}

fn push_frame_prefix(bytes: &mut Vec<u8>, field_count: u32) {
    bytes.extend_from_slice(&ASSET_SCHEMA_VERSION.to_le_bytes());
    bytes.extend_from_slice(&field_count.to_le_bytes());
}

fn push_encoded_field(
    bytes: &mut Vec<u8>,
    ordinal: u32,
    value: Option<&[u8]>,
) -> Result<(), CoreError> {
    let length = value.map_or(0, <[u8]>::len);
    let length = u64::try_from(length)
        .map_err(|_| CoreError::InvalidState("canonical field length overflows"))?;
    push_field_header(bytes, ordinal, value.is_some(), length);
    if let Some(value) = value {
        bytes.extend_from_slice(value);
    }
    Ok(())
}

fn push_field_header(bytes: &mut Vec<u8>, ordinal: u32, present: bool, length: u64) {
    bytes.extend_from_slice(&ordinal.to_le_bytes());
    bytes.push(u8::from(present));
    bytes.extend_from_slice(&[0; 3]);
    bytes.extend_from_slice(&length.to_le_bytes());
}

fn canonical_raster(raster: &TileRaster) -> Result<Vec<u8>, CoreError> {
    let format = pixel_format_code(raster.format())?;
    let mut tiles = raster
        .allocated_coords()
        .filter_map(|coord| raster.tile_data(coord))
        .filter(|tile| tile.bytes.iter().any(|byte| *byte != 0))
        .collect::<Vec<_>>();
    tiles.sort_by_key(|tile| (tile.coord.y, tile.coord.x));
    let tile_frames = tiles
        .into_iter()
        .map(|tile| {
            frame(&[
                present(tile.coord.x.to_le_bytes()),
                present(tile.coord.y.to_le_bytes()),
                present(tile.width.to_le_bytes()),
                present(tile.height.to_le_bytes()),
                present(tile.bytes),
            ])
        })
        .collect::<Result<Vec<_>, _>>()?;
    frame(&[
        present(raster.width().to_le_bytes()),
        present(raster.height().to_le_bytes()),
        present(format.to_le_bytes()),
        present(TILE_SIZE.to_le_bytes()),
        present(sequence(tile_frames.iter().map(Vec::as_slice))?),
    ])
}

fn rectangle_bytes(rectangle: RectI32) -> Result<Vec<u8>, CoreError> {
    let mut bytes = Vec::with_capacity(32);
    for component in [rectangle.x, rectangle.y, rectangle.width, rectangle.height] {
        let value = i64::from(component)
            .checked_mul(65_536)
            .ok_or(CoreError::InvalidState("frame coordinate overflows Q16"))?;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    Ok(bytes)
}

fn reference_origin_bytes(reference_frame: RectI32) -> Result<Vec<u8>, CoreError> {
    let mut bytes = Vec::with_capacity(16);
    for component in [reference_frame.x, reference_frame.y] {
        let value = i64::from(component)
            .checked_mul(65_536)
            .ok_or(CoreError::InvalidState(
                "light-table reference origin overflows Q16",
            ))?;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    Ok(bytes)
}

fn margins_bytes(margins: Margins) -> Result<Vec<u8>, CoreError> {
    let mut bytes = Vec::with_capacity(32);
    for component in [margins.left, margins.top, margins.right, margins.bottom] {
        let value = i64::from(component)
            .checked_mul(65_536)
            .ok_or(CoreError::InvalidState("frame margin overflows Q16"))?;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    Ok(bytes)
}

pub(super) fn color_bytes(color: PixelValue) -> Result<Vec<u8>, CoreError> {
    let mut bytes = Vec::with_capacity(9);
    match color {
        PixelValue::Rgba(channels) => {
            bytes.push(1);
            bytes.extend_from_slice(&channels);
        }
        PixelValue::Rgba16(channels) => {
            bytes.push(2);
            for channel in channels {
                bytes.extend_from_slice(&channel.to_le_bytes());
            }
        }
        _ => {
            return Err(CoreError::InvalidArgument(
                "canonical color must be straight RGBA8 or RGBA16",
            ));
        }
    }
    Ok(bytes)
}

pub(super) fn decode_color(bytes: &[u8]) -> Result<PixelValue, CoreError> {
    match bytes {
        [1, red, green, blue, alpha] => Ok(PixelValue::Rgba([*red, *green, *blue, *alpha])),
        [2, rest @ ..] if rest.len() == 8 => {
            let mut channels = [0_u16; 4];
            for (index, chunk) in rest.chunks_exact(2).enumerate() {
                channels[index] = u16::from_le_bytes([chunk[0], chunk[1]]);
            }
            Ok(PixelValue::Rgba16(channels))
        }
        _ => Err(CoreError::InvalidArgument(
            "canonical tagged color has an invalid encoding",
        )),
    }
}

fn frame(fields: &[Option<Vec<u8>>]) -> Result<Vec<u8>, CoreError> {
    frame_with_schema(DOCUMENT_STATE_SCHEMA_VERSION, fields)
}

fn frame_with_schema(
    schema_version: u32,
    fields: &[Option<Vec<u8>>],
) -> Result<Vec<u8>, CoreError> {
    let field_count = u32::try_from(fields.len())
        .map_err(|_| CoreError::InvalidState("canonical field count overflows"))?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&schema_version.to_le_bytes());
    bytes.extend_from_slice(&field_count.to_le_bytes());
    for (index, value) in fields.iter().enumerate() {
        let ordinal = u32::try_from(index + 1)
            .map_err(|_| CoreError::InvalidState("canonical field ordinal overflows"))?;
        bytes.extend_from_slice(&ordinal.to_le_bytes());
        bytes.push(u8::from(value.is_some()));
        bytes.extend_from_slice(&[0; 3]);
        let length = value.as_ref().map_or(0, Vec::len);
        bytes.extend_from_slice(
            &u64::try_from(length)
                .map_err(|_| CoreError::InvalidState("canonical field length overflows"))?
                .to_le_bytes(),
        );
        if let Some(value) = value {
            bytes.extend_from_slice(value);
        }
    }
    Ok(bytes)
}

fn sequence<'a>(elements: impl IntoIterator<Item = &'a [u8]>) -> Result<Vec<u8>, CoreError> {
    let elements = elements.into_iter().collect::<Vec<_>>();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(
        &u64::try_from(elements.len())
            .map_err(|_| CoreError::InvalidState("canonical sequence count overflows"))?
            .to_le_bytes(),
    );
    for element in elements {
        bytes.extend_from_slice(
            &u64::try_from(element.len())
                .map_err(|_| CoreError::InvalidState("canonical element length overflows"))?
                .to_le_bytes(),
        );
        bytes.extend_from_slice(element);
    }
    Ok(bytes)
}

fn present(value: impl Into<Vec<u8>>) -> Option<Vec<u8>> {
    Some(value.into())
}

const fn absent() -> Option<Vec<u8>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initialized_document() -> CellDocument {
        let mut core = Core::new();
        core.new_cell_with_uuid(
            65,
            2,
            DEFAULT_DPI_MILLI,
            DEFAULT_DPI_MILLI,
            0x0011_2233_4455_6677_8899_aabb_ccdd_eeff,
        )
        .unwrap();
        core.document.unwrap()
    }

    fn parsed_fields(bytes: &[u8]) -> Vec<Option<&[u8]>> {
        assert!(bytes.len() >= 8);
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            DOCUMENT_STATE_SCHEMA_VERSION
        );
        let count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let mut cursor = 8_usize;
        let mut fields = Vec::with_capacity(count);
        for expected_ordinal in 1..=count {
            let header_end = cursor + 16;
            let header = &bytes[cursor..header_end];
            assert_eq!(
                u32::from_le_bytes(header[0..4].try_into().unwrap()) as usize,
                expected_ordinal
            );
            assert!(matches!(header[4], 0 | 1));
            assert_eq!(&header[5..8], &[0; 3]);
            let length =
                usize::try_from(u64::from_le_bytes(header[8..16].try_into().unwrap())).unwrap();
            cursor = header_end;
            let end = cursor + length;
            let value = &bytes[cursor..end];
            fields.push((header[4] == 1).then_some(value));
            cursor = end;
        }
        assert_eq!(cursor, bytes.len());
        fields
    }

    fn parsed_sequence(bytes: &[u8]) -> Vec<&[u8]> {
        assert!(bytes.len() >= 8);
        let count = usize::try_from(u64::from_le_bytes(bytes[0..8].try_into().unwrap())).unwrap();
        let mut cursor = 8_usize;
        let mut elements = Vec::with_capacity(count);
        for _ in 0..count {
            let length_end = cursor + 8;
            let length = usize::try_from(u64::from_le_bytes(
                bytes[cursor..length_end].try_into().unwrap(),
            ))
            .unwrap();
            cursor = length_end;
            let end = cursor + length;
            elements.push(&bytes[cursor..end]);
            cursor = end;
        }
        assert_eq!(cursor, bytes.len());
        elements
    }

    #[test]
    fn universal_frame_distinguishes_absent_and_present_empty() {
        let bytes = frame(&[absent(), present(Vec::new())]).unwrap();
        assert_eq!(&bytes[8 + 4..8 + 8], &[0, 0, 0, 0]);
        let second = 8 + 16;
        assert_eq!(&bytes[second + 4..second + 8], &[1, 0, 0, 0]);
    }

    #[test]
    fn tagged_color_round_trips_exact_depth() {
        for color in [
            PixelValue::Rgba([1, 2, 3, 4]),
            PixelValue::Rgba16([1, 257, 32_769, 65_535]),
        ] {
            assert_eq!(decode_color(&color_bytes(color).unwrap()).unwrap(), color);
        }
    }

    #[test]
    fn blank_document_uses_the_closed_fourteen_field_semantic_frame() {
        let document = initialized_document();
        let (bytes, digest) = canonical_document_state(&document).unwrap();
        let fields = parsed_fields(&bytes);

        assert_eq!(fields.len(), 14);
        assert_eq!(fields[0].unwrap(), &document.uuid.to_be_bytes());
        assert_eq!(fields[1].unwrap(), &document.id.get().to_le_bytes());
        assert_eq!(
            u64::from_le_bytes(fields[5].unwrap()[0..8].try_into().unwrap()),
            1,
            "the layer tree is an ordered sequence"
        );
        assert!(!bytes.windows(8).any(|window| window == b"INKPOD\0\0"));

        let selection = parsed_fields(fields[6].unwrap());
        assert_eq!(selection.len(), 2);
        assert_eq!(
            selection[0].unwrap(),
            &document.selection_plane_id.get().to_le_bytes()
        );
        assert_eq!(parsed_fields(selection[1].unwrap()).len(), 5);

        assert_eq!(
            digest.as_bytes(),
            &[
                76, 201, 34, 212, 199, 5, 87, 139, 175, 52, 251, 201, 208, 163, 3, 123, 159, 84,
                94, 172, 148, 184, 224, 204, 206, 198, 74, 246, 66, 28, 95, 207,
            ],
            "schema-2 digest changes require an explicit golden update"
        );
    }

    #[test]
    fn procedure_payload_digest_keeps_the_closed_v1_contract() {
        assert_eq!(
            canonical_payload_digest(b"inkpod-payload-v1").unwrap(),
            [
                67, 253, 138, 17, 55, 63, 97, 7, 100, 228, 101, 136, 215, 44, 66, 125, 220, 70,
                215, 86, 87, 108, 228, 47, 161, 151, 65, 123, 223, 139, 58, 172,
            ],
            "the DocumentStateDigest v2 transition must not alter payload digest bytes"
        );
    }

    #[test]
    fn all_zero_tile_materialization_is_not_semantic_state() {
        let document = initialized_document();
        let baseline = canonical_document_state(&document).unwrap().1;
        let mut materialized = document;
        materialized.layers[0].planes[1]
            .raster
            .set_pixel(64, 1, PixelValue::Rgba([0; 4]), 99)
            .unwrap();
        assert_eq!(canonical_document_state(&materialized).unwrap().1, baseline);
    }

    #[test]
    fn light_table_item_frame_closes_the_source_alignment_origin_field() {
        let mut core = Core::new();
        core.new_cell_with_uuid(
            65,
            2,
            DEFAULT_DPI_MILLI,
            DEFAULT_DPI_MILLI,
            0x0011_2233_4455_6677_8899_aabb_ccdd_eeff,
        )
        .unwrap();
        let source = LightTableSource::from_rgba_bytes(
            0x1234,
            1,
            RectI32 {
                x: 1,
                y: -2,
                width: 2,
                height: 2,
            },
            RgbaRasterBytes {
                width: 2,
                height: 2,
                pixel_format: PixelFormat::StraightRgba8,
                dpi_x_milli: Some(DEFAULT_DPI_MILLI),
                dpi_y_milli: Some(DEFAULT_DPI_MILLI),
                pixels: vec![255; 16],
            },
        )
        .unwrap();
        core.light_table_add_item(LightTableItemInput::new("Aligned", source))
            .unwrap();

        let (bytes, _) = canonical_document_state(core.document.as_ref().unwrap()).unwrap();
        let document_fields = parsed_fields(&bytes);
        let light_table_fields = parsed_fields(document_fields[11].unwrap());
        let sets = parsed_sequence(light_table_fields[1].unwrap());
        let set_fields = parsed_fields(sets[0]);
        let items = parsed_sequence(set_fields[3].unwrap());
        let item_fields = parsed_fields(items[0]);

        assert_eq!(item_fields.len(), 13);
        assert_eq!(item_fields[1].unwrap().len(), 32);
        assert_eq!(item_fields[2].unwrap().len(), 16);
        assert_eq!(
            i64::from_le_bytes(item_fields[2].unwrap()[0..8].try_into().unwrap()),
            65_536
        );
        assert_eq!(
            i64::from_le_bytes(item_fields[2].unwrap()[8..16].try_into().unwrap()),
            -2 * 65_536
        );
    }

    #[test]
    fn canonical_scalar_rounding_is_ties_to_even() {
        assert_eq!(normalized_opacity(0).unwrap(), 0);
        assert_eq!(normalized_opacity(1_000).unwrap(), u16::MAX);
        assert_eq!(milli_q16(500, "test").unwrap(), 32_768);
        assert_eq!(milli_q16(-500, "test").unwrap(), -32_768);
        assert_eq!(turn_rotation(0).unwrap(), 0);
        assert_eq!(turn_rotation(360_000).unwrap(), 0);
        assert_eq!(turn_rotation(-360_000).unwrap(), 0);
    }
}
