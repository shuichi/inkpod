//! Native vector metadata encoding.

use super::{
    FormatError, MAX_MANIFEST_BYTES, PixelValue, Reader, push_color_value, push_i32, push_u32,
    push_u64,
};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_VECTOR_PATHS: usize = 65_536;
pub const MAX_VECTOR_SEGMENTS: usize = 262_144;
pub const MAX_VECTOR_FILLS: usize = 65_536;
pub const MAX_VECTOR_BOUNDARIES: usize = 262_144;
pub const MAX_VECTOR_CONNECTIONS: usize = MAX_VECTOR_PATHS;
const MAX_VECTOR_WIDTH_MILLI: u32 = 4_096_000;
const MAX_VECTOR_COORDINATE_MILLI: i64 = 2_000_000_000;
const VECTOR_METADATA_MAGIC: [u8; 4] = *b"VECT";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileVectorPoint {
    pub x_milli: i32,
    pub y_milli: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileVectorSegment {
    pub p0: FileVectorPoint,
    pub p1: FileVectorPoint,
    pub p2: FileVectorPoint,
    pub p3: FileVectorPoint,
    pub width_start_milli: u32,
    pub width_end_milli: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileVectorPath {
    pub id: u64,
    pub plane_id: u64,
    pub color: PixelValue,
    pub closed: bool,
    pub segments: Vec<FileVectorSegment>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileVectorFill {
    pub id: u64,
    pub plane_id: u64,
    pub color: PixelValue,
    pub boundary_path_ids: Vec<u64>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FileVectorEndpoint {
    Start,
    End,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FileVectorConnection {
    pub first_path_id: u64,
    pub first_endpoint: FileVectorEndpoint,
    pub second_path_id: u64,
    pub second_endpoint: FileVectorEndpoint,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileVectorMetadata {
    pub paths: Vec<FileVectorPath>,
    pub fills: Vec<FileVectorFill>,
    pub connections: Vec<FileVectorConnection>,
}

pub(super) fn encode_vector_metadata(
    metadata: &FileVectorMetadata,
) -> Result<Vec<u8>, FormatError> {
    validate_vector_metadata(metadata, None, None, None)?;
    let mut output = Vec::new();
    output.extend_from_slice(&VECTOR_METADATA_MAGIC);
    push_u32(&mut output, 2);
    push_u32(&mut output, metadata.paths.len() as u32);
    push_u32(&mut output, metadata.fills.len() as u32);
    push_u32(&mut output, metadata.connections.len() as u32);
    push_u32(&mut output, 0);
    for path in &metadata.paths {
        push_u64(&mut output, path.id);
        push_u64(&mut output, path.plane_id);
        push_u32(&mut output, u32::from(path.closed));
        push_u32(&mut output, path.segments.len() as u32);
        push_color_value(&mut output, path.color)?;
        for segment in &path.segments {
            for point in [segment.p0, segment.p1, segment.p2, segment.p3] {
                push_i32(&mut output, point.x_milli);
                push_i32(&mut output, point.y_milli);
            }
            push_u32(&mut output, segment.width_start_milli);
            push_u32(&mut output, segment.width_end_milli);
        }
    }
    for fill in &metadata.fills {
        push_u64(&mut output, fill.id);
        push_u64(&mut output, fill.plane_id);
        push_u32(&mut output, fill.boundary_path_ids.len() as u32);
        push_u32(&mut output, 0);
        push_color_value(&mut output, fill.color)?;
        for path_id in &fill.boundary_path_ids {
            push_u64(&mut output, *path_id);
        }
    }
    for connection in &metadata.connections {
        push_u64(&mut output, connection.first_path_id);
        push_u64(&mut output, connection.second_path_id);
        push_u32(&mut output, endpoint_value(connection.first_endpoint));
        push_u32(&mut output, endpoint_value(connection.second_endpoint));
    }
    if output.len() > MAX_MANIFEST_BYTES as usize {
        return Err(FormatError::Invalid("vector metadata exceeds its bound"));
    }
    Ok(output)
}

pub(super) fn decode_vector_metadata(bytes: &[u8]) -> Result<FileVectorMetadata, FormatError> {
    let mut reader = Reader::new(bytes);
    if reader.take(4)? != VECTOR_METADATA_MAGIC || reader.u32()? != 2 {
        return Err(FormatError::Unsupported(
            "vector metadata version is not supported",
        ));
    }
    let path_count = reader.u32()? as usize;
    let fill_count = reader.u32()? as usize;
    let connection_count = reader.u32()? as usize;
    if reader.u32()? != 0 {
        return Err(FormatError::Unsupported(
            "vector metadata reserved field is not zero",
        ));
    }
    if path_count > MAX_VECTOR_PATHS
        || fill_count > MAX_VECTOR_FILLS
        || connection_count > MAX_VECTOR_CONNECTIONS
    {
        return Err(FormatError::Invalid(
            "vector object count is outside bounds",
        ));
    }
    let mut paths = Vec::with_capacity(path_count);
    let mut segment_count = 0_usize;
    for _ in 0..path_count {
        let id = reader.u64()?;
        let plane_id = reader.u64()?;
        let flags = reader.u32()?;
        if flags & !1 != 0 {
            return Err(FormatError::Unsupported("unknown vector path flags"));
        }
        let count = reader.u32()? as usize;
        segment_count = segment_count
            .checked_add(count)
            .ok_or(FormatError::Invalid("vector segment count overflows"))?;
        if segment_count > MAX_VECTOR_SEGMENTS {
            return Err(FormatError::Invalid(
                "vector segment count exceeds its bound",
            ));
        }
        let color = reader.color_value()?;
        let mut segments = Vec::with_capacity(count);
        for _ in 0..count {
            let mut point = || -> Result<FileVectorPoint, FormatError> {
                Ok(FileVectorPoint {
                    x_milli: reader.i32()?,
                    y_milli: reader.i32()?,
                })
            };
            segments.push(FileVectorSegment {
                p0: point()?,
                p1: point()?,
                p2: point()?,
                p3: point()?,
                width_start_milli: reader.u32()?,
                width_end_milli: reader.u32()?,
            });
        }
        paths.push(FileVectorPath {
            id,
            plane_id,
            color,
            closed: flags & 1 != 0,
            segments,
        });
    }
    let mut fills = Vec::with_capacity(fill_count);
    let mut boundary_count = 0_usize;
    for _ in 0..fill_count {
        let id = reader.u64()?;
        let plane_id = reader.u64()?;
        let count = reader.u32()? as usize;
        if reader.u32()? != 0 {
            return Err(FormatError::Unsupported(
                "vector fill reserved field is not zero",
            ));
        }
        boundary_count = boundary_count
            .checked_add(count)
            .ok_or(FormatError::Invalid("vector boundary count overflows"))?;
        if boundary_count > MAX_VECTOR_BOUNDARIES {
            return Err(FormatError::Invalid(
                "vector boundary count exceeds its bound",
            ));
        }
        let color = reader.color_value()?;
        let mut boundary_path_ids = Vec::with_capacity(count);
        for _ in 0..count {
            boundary_path_ids.push(reader.u64()?);
        }
        fills.push(FileVectorFill {
            id,
            plane_id,
            color,
            boundary_path_ids,
        });
    }
    let mut connections = Vec::with_capacity(connection_count);
    for _ in 0..connection_count {
        let first_path_id = reader.u64()?;
        let second_path_id = reader.u64()?;
        let first_endpoint = decode_endpoint(reader.u32()?)?;
        let second_endpoint = decode_endpoint(reader.u32()?)?;
        connections.push(FileVectorConnection {
            first_path_id,
            first_endpoint,
            second_path_id,
            second_endpoint,
        });
    }
    if reader.position != bytes.len() {
        return Err(FormatError::Invalid("vector metadata has trailing bytes"));
    }
    let metadata = FileVectorMetadata {
        paths,
        fills,
        connections,
    };
    validate_vector_metadata(&metadata, None, None, None)?;
    Ok(metadata)
}

pub(super) fn validate_vector_metadata(
    metadata: &FileVectorMetadata,
    stroke_plane_ids: Option<&BTreeSet<u64>>,
    fill_plane_ids: Option<&BTreeSet<u64>>,
    vector_layer_for_plane: Option<&BTreeMap<u64, u64>>,
) -> Result<(), FormatError> {
    if metadata.paths.len() > MAX_VECTOR_PATHS
        || metadata.fills.len() > MAX_VECTOR_FILLS
        || metadata.connections.len() > MAX_VECTOR_CONNECTIONS
    {
        return Err(FormatError::Invalid(
            "vector object count is outside bounds",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut path_ids = BTreeSet::new();
    let mut segment_count = 0_usize;
    for path in &metadata.paths {
        segment_count = segment_count
            .checked_add(path.segments.len())
            .ok_or(FormatError::Invalid("vector segment count overflows"))?;
        if path.id == 0
            || path.plane_id == 0
            || !ids.insert(path.id)
            || !path_ids.insert(path.id)
            || path.segments.is_empty()
            || segment_count > MAX_VECTOR_SEGMENTS
            || path.color.rgba16().is_none()
            || stroke_plane_ids.is_some_and(|planes| !planes.contains(&path.plane_id))
        {
            return Err(FormatError::Invalid("vector path properties are invalid"));
        }
        for (index, segment) in path.segments.iter().enumerate() {
            let coordinates_are_valid = [segment.p0, segment.p1, segment.p2, segment.p3]
                .into_iter()
                .all(|point| {
                    i64::from(point.x_milli).abs() <= MAX_VECTOR_COORDINATE_MILLI
                        && i64::from(point.y_milli).abs() <= MAX_VECTOR_COORDINATE_MILLI
                });
            if !coordinates_are_valid {
                return Err(FormatError::Invalid(
                    "vector segment coordinate is outside bounds",
                ));
            }
            if segment.width_start_milli == 0
                || segment.width_end_milli == 0
                || segment.width_start_milli > MAX_VECTOR_WIDTH_MILLI
                || segment.width_end_milli > MAX_VECTOR_WIDTH_MILLI
                || (index > 0 && path.segments[index - 1].p3 != segment.p0)
            {
                return Err(FormatError::Invalid(
                    "vector segment properties are invalid",
                ));
            }
        }
        if path.closed
            && path
                .segments
                .last()
                .is_none_or(|segment| segment.p3 != path.segments[0].p0)
        {
            return Err(FormatError::Invalid("vector closed path is not continuous"));
        }
    }
    let mut boundary_count = 0_usize;
    for fill in &metadata.fills {
        boundary_count = boundary_count
            .checked_add(fill.boundary_path_ids.len())
            .ok_or(FormatError::Invalid("vector boundary count overflows"))?;
        if fill.id == 0
            || fill.plane_id == 0
            || !ids.insert(fill.id)
            || fill.boundary_path_ids.is_empty()
            || boundary_count > MAX_VECTOR_BOUNDARIES
            || fill.color.rgba16().is_none()
            || fill_plane_ids.is_some_and(|planes| !planes.contains(&fill.plane_id))
        {
            return Err(FormatError::Invalid("vector fill properties are invalid"));
        }
        let mut unique_boundaries = BTreeSet::new();
        let fill_layer = vector_layer_for_plane.and_then(|layers| layers.get(&fill.plane_id));
        for path_id in &fill.boundary_path_ids {
            let Some(path) = metadata.paths.iter().find(|path| path.id == *path_id) else {
                return Err(FormatError::Invalid("vector fill boundary path is missing"));
            };
            if !unique_boundaries.insert(*path_id) || !path.closed || !path_ids.contains(path_id) {
                return Err(FormatError::Invalid("vector fill boundary is invalid"));
            }
            if let Some(layers) = vector_layer_for_plane {
                if fill_layer.is_none() || layers.get(&path.plane_id) != fill_layer {
                    return Err(FormatError::Invalid(
                        "vector fill boundary crosses vector layers",
                    ));
                }
            }
        }
    }
    let paths_by_id = metadata
        .paths
        .iter()
        .map(|path| (path.id, path))
        .collect::<BTreeMap<_, _>>();
    let mut connected_endpoints = BTreeSet::new();
    let mut previous = None;
    for connection in &metadata.connections {
        let first = (connection.first_path_id, connection.first_endpoint);
        let second = (connection.second_path_id, connection.second_endpoint);
        let Some(first_path) = paths_by_id.get(&connection.first_path_id) else {
            return Err(FormatError::Invalid("vector connection path is missing"));
        };
        let Some(second_path) = paths_by_id.get(&connection.second_path_id) else {
            return Err(FormatError::Invalid("vector connection path is missing"));
        };
        if first >= second
            || first_path.closed
            || second_path.closed
            || first_path.plane_id != second_path.plane_id
            || !connected_endpoints.insert(first)
            || !connected_endpoints.insert(second)
            || previous.is_some_and(|value| value >= *connection)
        {
            return Err(FormatError::Invalid("vector connection is invalid"));
        }
        previous = Some(*connection);
    }
    Ok(())
}

fn endpoint_value(endpoint: FileVectorEndpoint) -> u32 {
    match endpoint {
        FileVectorEndpoint::Start => 0,
        FileVectorEndpoint::End => 1,
    }
}

fn decode_endpoint(value: u32) -> Result<FileVectorEndpoint, FormatError> {
    match value {
        0 => Ok(FileVectorEndpoint::Start),
        1 => Ok(FileVectorEndpoint::End),
        _ => Err(FormatError::Unsupported(
            "vector connection endpoint is not supported",
        )),
    }
}
