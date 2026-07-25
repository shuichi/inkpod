//! Native adjustment-layer metadata encoding.

use super::{FormatError, MAX_MANIFEST_BYTES, Reader, push_i32, push_u32, push_u64};
use inkpod_image::{
    Adjustment, Channel, CurveInterpolation, CurvePoint, Levels, MAX_CURVE_POINTS, PixelValue,
    apply_adjustment,
};
use std::collections::BTreeSet;

pub const MAX_ADJUSTMENT_LAYERS: usize = 4_096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileAdjustmentLayer {
    pub layer_id: u64,
    pub adjustment: Adjustment,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileAdjustmentMetadata {
    pub adjustments: Vec<FileAdjustmentLayer>,
}

pub(super) fn encode_adjustment_metadata(
    metadata: &FileAdjustmentMetadata,
) -> Result<Vec<u8>, FormatError> {
    validate_adjustment_metadata(metadata, None)?;
    let mut output = Vec::new();
    output.extend_from_slice(b"M6AD");
    push_u32(&mut output, 1);
    push_u32(&mut output, metadata.adjustments.len() as u32);
    push_u32(&mut output, 0);
    for layer in &metadata.adjustments {
        push_u64(&mut output, layer.layer_id);
        let (kind, channel, interpolation, points, parameters) = match &layer.adjustment {
            Adjustment::BrightnessContrast {
                brightness_milli,
                contrast_milli,
            } => (
                1,
                0,
                0,
                &[][..],
                [*brightness_milli, *contrast_milli, 0, 0, 0, 0],
            ),
            Adjustment::ToneCurve {
                channel,
                interpolation,
                points,
            } => (
                2,
                channel_code(*channel),
                interpolation_code(*interpolation),
                points.as_slice(),
                [0; 6],
            ),
            Adjustment::Levels(levels) => (
                3,
                channel_code(levels.channel),
                0,
                &[][..],
                [
                    i32::from(levels.input_shadow),
                    levels.input_gamma_milli as i32,
                    i32::from(levels.input_highlight),
                    i32::from(levels.output_shadow),
                    i32::from(levels.output_highlight),
                    0,
                ],
            ),
        };
        push_u32(&mut output, kind);
        push_u32(&mut output, channel);
        push_u32(&mut output, interpolation);
        push_u32(&mut output, points.len() as u32);
        for parameter in parameters {
            push_i32(&mut output, parameter);
        }
        for point in points {
            push_u32(&mut output, u32::from(point.input));
            push_u32(&mut output, u32::from(point.output));
        }
    }
    if output.len() > MAX_MANIFEST_BYTES as usize {
        return Err(FormatError::Invalid(
            "adjustment metadata exceeds its bound",
        ));
    }
    Ok(output)
}

pub(super) fn decode_adjustment_metadata(
    bytes: &[u8],
) -> Result<FileAdjustmentMetadata, FormatError> {
    let mut reader = Reader::new(bytes);
    if reader.take(4)? != b"M6AD" || reader.u32()? != 1 {
        return Err(FormatError::Unsupported(
            "adjustment metadata version is not supported",
        ));
    }
    let count = reader.u32()? as usize;
    if reader.u32()? != 0 {
        return Err(FormatError::Unsupported(
            "adjustment metadata reserved field is not zero",
        ));
    }
    if count > MAX_ADJUSTMENT_LAYERS {
        return Err(FormatError::Invalid("adjustment count exceeds its bound"));
    }
    let mut adjustments = Vec::with_capacity(count);
    for _ in 0..count {
        let layer_id = reader.u64()?;
        let kind = reader.u32()?;
        let channel = reader.u32()?;
        let interpolation = reader.u32()?;
        let point_count = reader.u32()? as usize;
        if point_count > MAX_CURVE_POINTS {
            return Err(FormatError::Invalid(
                "adjustment curve point count exceeds its bound",
            ));
        }
        let parameters = [
            reader.i32()?,
            reader.i32()?,
            reader.i32()?,
            reader.i32()?,
            reader.i32()?,
            reader.i32()?,
        ];
        let mut points = Vec::with_capacity(point_count);
        for _ in 0..point_count {
            let input = reader.u32()?;
            let output = reader.u32()?;
            points.push(CurvePoint {
                input: input
                    .try_into()
                    .map_err(|_| FormatError::Invalid("adjustment curve input exceeds 16-bit"))?,
                output: output
                    .try_into()
                    .map_err(|_| FormatError::Invalid("adjustment curve output exceeds 16-bit"))?,
            });
        }
        let adjustment = match kind {
            1 if channel == 0
                && interpolation == 0
                && points.is_empty()
                && parameters[2..] == [0; 4] =>
            {
                Adjustment::BrightnessContrast {
                    brightness_milli: parameters[0],
                    contrast_milli: parameters[1],
                }
            }
            2 if parameters == [0; 6] => Adjustment::ToneCurve {
                channel: parse_channel(channel)?,
                interpolation: parse_interpolation(interpolation)?,
                points,
            },
            3 if interpolation == 0 && points.is_empty() && parameters[5] == 0 => {
                Adjustment::Levels(Levels {
                    channel: parse_channel(channel)?,
                    input_shadow: bounded_u16(parameters[0])?,
                    input_gamma_milli: parameters[1]
                        .try_into()
                        .map_err(|_| FormatError::Invalid("adjustment gamma is negative"))?,
                    input_highlight: bounded_u16(parameters[2])?,
                    output_shadow: bounded_u16(parameters[3])?,
                    output_highlight: bounded_u16(parameters[4])?,
                })
            }
            _ => return Err(FormatError::Unsupported("adjustment record is unknown")),
        };
        adjustments.push(FileAdjustmentLayer {
            layer_id,
            adjustment,
        });
    }
    if reader.position != bytes.len() {
        return Err(FormatError::Invalid(
            "adjustment metadata has trailing bytes",
        ));
    }
    let metadata = FileAdjustmentMetadata { adjustments };
    validate_adjustment_metadata(&metadata, None)?;
    Ok(metadata)
}

pub(super) fn validate_adjustment_metadata(
    metadata: &FileAdjustmentMetadata,
    adjustment_layer_ids: Option<&BTreeSet<u64>>,
) -> Result<(), FormatError> {
    if metadata.adjustments.is_empty() || metadata.adjustments.len() > MAX_ADJUSTMENT_LAYERS {
        return Err(FormatError::Invalid("adjustment count is outside bounds"));
    }
    let mut ids = BTreeSet::new();
    for layer in &metadata.adjustments {
        if layer.layer_id == 0
            || !ids.insert(layer.layer_id)
            || adjustment_layer_ids.is_some_and(|expected| !expected.contains(&layer.layer_id))
            || apply_adjustment(PixelValue::Rgba([0; 4]), &layer.adjustment).is_err()
        {
            return Err(FormatError::Invalid("adjustment properties are invalid"));
        }
    }
    if adjustment_layer_ids.is_some_and(|expected| expected != &ids) {
        return Err(FormatError::Invalid("adjustment layers are incomplete"));
    }
    Ok(())
}

const fn channel_code(channel: Channel) -> u32 {
    match channel {
        Channel::Rgb => 1,
        Channel::Red => 2,
        Channel::Green => 3,
        Channel::Blue => 4,
    }
}

fn parse_channel(value: u32) -> Result<Channel, FormatError> {
    match value {
        1 => Ok(Channel::Rgb),
        2 => Ok(Channel::Red),
        3 => Ok(Channel::Green),
        4 => Ok(Channel::Blue),
        _ => Err(FormatError::Unsupported("adjustment channel is unknown")),
    }
}

const fn interpolation_code(value: CurveInterpolation) -> u32 {
    match value {
        CurveInterpolation::Bezier => 1,
        CurveInterpolation::BSpline => 2,
    }
}

fn parse_interpolation(value: u32) -> Result<CurveInterpolation, FormatError> {
    match value {
        1 => Ok(CurveInterpolation::Bezier),
        2 => Ok(CurveInterpolation::BSpline),
        _ => Err(FormatError::Unsupported(
            "adjustment curve interpolation is unknown",
        )),
    }
}

fn bounded_u16(value: i32) -> Result<u16, FormatError> {
    value
        .try_into()
        .map_err(|_| FormatError::Invalid("adjustment level value exceeds 16-bit"))
}
