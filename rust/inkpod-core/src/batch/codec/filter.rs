use super::super::*;
use super::codes::*;
use super::payload::*;

pub(super) fn encode_filter(output: &mut PayloadWriter, filter: &Filter) -> Result<(), CoreError> {
    match filter {
        Filter::SharpenWeak => output.u32(1),
        Filter::SharpenStrong => output.u32(2),
        Filter::BlurWeak => output.u32(3),
        Filter::BlurStrong => output.u32(4),
        Filter::GaussianBlur {
            radius,
            strength_milli,
        } => {
            output.u32(5);
            output.u32(*radius);
            output.u32(*strength_milli);
        }
        Filter::UnsharpMask {
            radius,
            amount_milli,
            threshold,
        } => {
            output.u32(6);
            output.u32(*radius);
            output.u32(*amount_milli);
            output.u32(u32::from(*threshold));
        }
        Filter::Invert { channel } => {
            output.u32(7);
            output.u32(channel_code(*channel));
        }
        Filter::AutoContrast => output.u32(8),
        Filter::BrightnessContrast {
            brightness_milli,
            contrast_milli,
        } => {
            output.u32(9);
            output.i32(*brightness_milli);
            output.i32(*contrast_milli);
        }
        Filter::ToneCurve {
            channel,
            interpolation,
            points,
        } => {
            output.u32(10);
            output.u32(channel_code(*channel));
            output.u32(match interpolation {
                CurveInterpolation::Bezier => 1,
                CurveInterpolation::BSpline => 2,
            });
            output.u32(points.len() as u32);
            for point in points {
                output.u32(u32::from(point.input));
                output.u32(u32::from(point.output));
            }
        }
        Filter::Levels(levels) => {
            output.u32(11);
            output.u32(channel_code(levels.channel));
            output.u32(u32::from(levels.input_shadow));
            output.u32(levels.input_gamma_milli);
            output.u32(u32::from(levels.input_highlight));
            output.u32(u32::from(levels.output_shadow));
            output.u32(u32::from(levels.output_highlight));
        }
        Filter::Hsv(hsv) => {
            output.u32(12);
            output.i32(hsv.hue_degrees_milli);
            output.i32(hsv.saturation_milli);
            output.i32(hsv.value_milli);
        }
        Filter::ColorBalance(balance) => {
            output.u32(13);
            output.i32(balance.red_milli);
            output.i32(balance.green_milli);
            output.i32(balance.blue_milli);
        }
    }
    Ok(())
}

pub(super) fn decode_filter(input: &mut PayloadReader<'_>) -> Result<Filter, CoreError> {
    Ok(match input.u32()? {
        1 => Filter::SharpenWeak,
        2 => Filter::SharpenStrong,
        3 => Filter::BlurWeak,
        4 => Filter::BlurStrong,
        5 => Filter::GaussianBlur {
            radius: input.u32()?,
            strength_milli: input.u32()?,
        },
        6 => Filter::UnsharpMask {
            radius: input.u32()?,
            amount_milli: input.u32()?,
            threshold: u16::try_from(input.u32()?)
                .map_err(|_| CoreError::InvalidArgument("batch filter threshold is invalid"))?,
        },
        7 => Filter::Invert {
            channel: parse_channel(input.u32()?)?,
        },
        8 => Filter::AutoContrast,
        9 => Filter::BrightnessContrast {
            brightness_milli: input.i32()?,
            contrast_milli: input.i32()?,
        },
        10 => {
            let channel = parse_channel(input.u32()?)?;
            let interpolation = match input.u32()? {
                1 => CurveInterpolation::Bezier,
                2 => CurveInterpolation::BSpline,
                _ => {
                    return Err(CoreError::InvalidArgument(
                        "batch curve interpolation is unknown",
                    ));
                }
            };
            let count = input.count(MAX_CURVE_POINTS)?;
            let mut points = Vec::with_capacity(count);
            for _ in 0..count {
                points.push(CurvePoint {
                    input: u16::try_from(input.u32()?)
                        .map_err(|_| CoreError::InvalidArgument("batch curve input is invalid"))?,
                    output: u16::try_from(input.u32()?)
                        .map_err(|_| CoreError::InvalidArgument("batch curve output is invalid"))?,
                });
            }
            Filter::ToneCurve {
                channel,
                interpolation,
                points,
            }
        }
        11 => Filter::Levels(Levels {
            channel: parse_channel(input.u32()?)?,
            input_shadow: input.u16()?,
            input_gamma_milli: input.u32()?,
            input_highlight: input.u16()?,
            output_shadow: input.u16()?,
            output_highlight: input.u16()?,
        }),
        12 => Filter::Hsv(HsvAdjustment {
            hue_degrees_milli: input.i32()?,
            saturation_milli: input.i32()?,
            value_milli: input.i32()?,
        }),
        13 => Filter::ColorBalance(ColorBalance {
            red_milli: input.i32()?,
            green_milli: input.i32()?,
            blue_milli: input.i32()?,
        }),
        _ => return Err(CoreError::InvalidArgument("batch filter kind is unknown")),
    })
}
