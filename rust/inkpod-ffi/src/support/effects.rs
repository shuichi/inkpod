use super::*;

pub(crate) fn parse_plane(value: u32) -> Result<ActivePlane, u32> {
    match value {
        INKPOD_PLANE_MAIN_LINE => Ok(ActivePlane::MainLine),
        INKPOD_PLANE_COLOR => Ok(ActivePlane::Color),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "plane is not a defined plane",
        )),
    }
}

pub(crate) fn parse_filter_channel(value: u32) -> Result<Channel, u32> {
    match value {
        INKPOD_FILTER_CHANNEL_RGB => Ok(Channel::Rgb),
        INKPOD_FILTER_CHANNEL_RED => Ok(Channel::Red),
        INKPOD_FILTER_CHANNEL_GREEN => Ok(Channel::Green),
        INKPOD_FILTER_CHANNEL_BLUE => Ok(Channel::Blue),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "filter channel is unknown",
        )),
    }
}

pub(crate) fn parse_curve_interpolation(value: u32) -> Result<CurveInterpolation, u32> {
    match value {
        INKPOD_CURVE_BEZIER => Ok(CurveInterpolation::Bezier),
        INKPOD_CURVE_BSPLINE => Ok(CurveInterpolation::BSpline),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "curve interpolation is unknown",
        )),
    }
}

pub(crate) unsafe fn parse_filter_input(input: &InkpodFilterInput) -> Result<Filter, u32> {
    if input.feature_flags != INKPOD_FEATURE_NONE {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "filter input contains unsupported feature flags",
        ));
    }
    let points = if input.point_count == 0 {
        if !input.points.is_null() || input.point_stride_bytes != 0 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "zero curve point count requires a null pointer and zero stride",
            ));
        }
        Vec::new()
    } else {
        if input.points.is_null() || !is_aligned(input.points) {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "curve point storage is null or misaligned",
            ));
        }
        let count = usize::try_from(input.point_count).map_err(|_| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "curve point count is not representable",
            )
        })?;
        let stride = if input.point_stride_bytes == 0 {
            size_of::<InkpodCurvePoint>()
        } else {
            input.point_stride_bytes as usize
        };
        let storage = count
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|offset| offset.checked_add(size_of::<InkpodCurvePoint>()));
        if input.point_count > inkpod_core::MAX_CURVE_POINTS as u64
            || stride < size_of::<InkpodCurvePoint>()
            || stride % align_of::<InkpodCurvePoint>() != 0
            || storage.is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "curve point count, stride, or storage size is invalid",
            ));
        }
        let mut points = Vec::with_capacity(count);
        for index in 0..count {
            // SAFETY: The checked count/stride span is readable by contract.
            let pointer = unsafe {
                input
                    .points
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodCurvePoint>()
            };
            // SAFETY: Every strided record exposes a readable size prefix.
            let struct_size = unsafe { validate_struct(pointer, "InkpodCurvePoint") }?;
            if u64::from(struct_size) > stride as u64 {
                return Err(fail(
                    INKPOD_STATUS_INCOMPATIBLE_ABI,
                    "InkpodCurvePoint.struct_size exceeds point stride",
                ));
            }
            // SAFETY: The complete known record is readable after validation.
            let record = unsafe { &*pointer };
            if record.reserved != 0
                || record.input > u16::MAX.into()
                || record.output > u16::MAX.into()
            {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "curve point record is invalid",
                ));
            }
            points.push(CurvePoint {
                input: record.input as u16,
                output: record.output as u16,
            });
        }
        points
    };
    let no_points = || {
        if points.is_empty() {
            Ok(())
        } else {
            Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "this filter does not accept curve points",
            ))
        }
    };
    match input.kind {
        INKPOD_FILTER_SHARPEN_WEAK => {
            no_points()?;
            Ok(Filter::SharpenWeak)
        }
        INKPOD_FILTER_SHARPEN_STRONG => {
            no_points()?;
            Ok(Filter::SharpenStrong)
        }
        INKPOD_FILTER_BLUR_WEAK => {
            no_points()?;
            Ok(Filter::BlurWeak)
        }
        INKPOD_FILTER_BLUR_STRONG => {
            no_points()?;
            Ok(Filter::BlurStrong)
        }
        INKPOD_FILTER_GAUSSIAN_BLUR => {
            no_points()?;
            Ok(Filter::GaussianBlur {
                radius: input.parameter_0.try_into().map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "Gaussian radius is negative",
                    )
                })?,
                strength_milli: input.parameter_1.try_into().map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "Gaussian strength is negative",
                    )
                })?,
            })
        }
        INKPOD_FILTER_INVERT => {
            no_points()?;
            Ok(Filter::Invert {
                channel: parse_filter_channel(input.channel)?,
            })
        }
        INKPOD_FILTER_AUTO_CONTRAST => {
            no_points()?;
            Ok(Filter::AutoContrast)
        }
        INKPOD_FILTER_BRIGHTNESS_CONTRAST => {
            no_points()?;
            Ok(Filter::BrightnessContrast {
                brightness_milli: input.parameter_0,
                contrast_milli: input.parameter_1,
            })
        }
        INKPOD_FILTER_TONE_CURVE => Ok(Filter::ToneCurve {
            channel: parse_filter_channel(input.channel)?,
            interpolation: parse_curve_interpolation(input.interpolation)?,
            points,
        }),
        INKPOD_FILTER_LEVELS => {
            no_points()?;
            Ok(Filter::Levels(Levels {
                channel: parse_filter_channel(input.channel)?,
                input_shadow: input.parameter_0.try_into().map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "levels input shadow is invalid",
                    )
                })?,
                input_gamma_milli: input
                    .parameter_1
                    .try_into()
                    .map_err(|_| fail(INKPOD_STATUS_INVALID_ARGUMENT, "levels gamma is invalid"))?,
                input_highlight: input.parameter_2.try_into().map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "levels input highlight is invalid",
                    )
                })?,
                output_shadow: input.parameter_3.try_into().map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "levels output shadow is invalid",
                    )
                })?,
                output_highlight: input.parameter_4.try_into().map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "levels output highlight is invalid",
                    )
                })?,
            }))
        }
        INKPOD_FILTER_HSV => {
            no_points()?;
            Ok(Filter::Hsv(HsvAdjustment {
                hue_degrees_milli: input.parameter_0,
                saturation_milli: input.parameter_1,
                value_milli: input.parameter_2,
            }))
        }
        INKPOD_FILTER_COLOR_BALANCE => {
            no_points()?;
            Ok(Filter::ColorBalance(ColorBalance {
                red_milli: input.parameter_0,
                green_milli: input.parameter_1,
                blue_milli: input.parameter_2,
            }))
        }
        INKPOD_FILTER_UNSHARP_MASK => {
            no_points()?;
            Ok(Filter::UnsharpMask {
                radius: input.parameter_0.try_into().map_err(|_| {
                    fail(INKPOD_STATUS_INVALID_ARGUMENT, "unsharp radius is negative")
                })?,
                amount_milli: input.parameter_1.try_into().map_err(|_| {
                    fail(INKPOD_STATUS_INVALID_ARGUMENT, "unsharp amount is negative")
                })?,
                threshold: input.parameter_2.try_into().map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "unsharp threshold is invalid",
                    )
                })?,
            })
        }
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "filter kind is unknown",
        )),
    }
}

pub(crate) fn write_filter_preview_info(
    output: &mut InkpodFilterPreviewInfo,
    info: inkpod_core::FilterPreviewInfo,
) {
    output.reserved = 0;
    output.plane_id = info.plane_id;
    output.base_checksum = info.base_checksum;
    output.preview_checksum = info.preview_checksum;
    output.preview_revision = info.preview_revision;
}

// SAFETY: `input` and every advertised strided stop record must remain readable
// for this call. All retained stop/color values are copied into the result.
pub(crate) unsafe fn parse_gradient_input(input: &InkpodGradientInput) -> Result<Gradient, u32> {
    if input.feature_flags & !INKPOD_GRADIENT_FLAG_CONSTRAIN_45 != 0 || input.dither > 1 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "gradient input contains unsupported flags or dither value",
        ));
    }
    let kind = match input.kind {
        INKPOD_GRADIENT_LINEAR => GradientKind::Linear,
        INKPOD_GRADIENT_RADIAL => GradientKind::Radial,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "gradient kind is unknown",
            ));
        }
    };
    let mode = match input.mode {
        INKPOD_GRADIENT_COMPOSITE => GradientMode::Composite,
        INKPOD_GRADIENT_OVERWRITE => GradientMode::Overwrite,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "gradient mode is unknown",
            ));
        }
    };
    let count = usize::try_from(input.stop_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "gradient stop count is not representable",
        )
    })?;
    let stride = usize::try_from(input.stop_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "gradient stop stride is not representable",
        )
    })?;
    let storage = count
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(size_of::<InkpodGradientStop>()));
    if !(3..=MAX_GRADIENT_STOPS).contains(&count)
        || input.stops.is_null()
        || !is_aligned(input.stops)
        || stride < size_of::<InkpodGradientStop>()
        || stride % align_of::<InkpodGradientStop>() != 0
        || storage.is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "gradient stop count, pointer, stride, or storage is invalid",
        ));
    }
    let mut stops = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: The checked count/stride span is readable by contract.
        let pointer = unsafe {
            input
                .stops
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodGradientStop>()
        };
        // SAFETY: Every strided record exposes a readable size prefix.
        let struct_size = unsafe { validate_struct(pointer, "InkpodGradientStop") }?;
        if u64::from(struct_size) > input.stop_stride_bytes {
            return Err(fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodGradientStop.struct_size exceeds stop stride",
            ));
        }
        // SAFETY: The complete known record is readable after validation.
        let record = unsafe { &*pointer };
        if record.reserved != 0 || record.reserved_2 != 0 {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "gradient stop contains unsupported reserved values",
            ));
        }
        // SAFETY: The nested complete color record is part of the validated stop.
        let color = unsafe { parse_color_value(&record.color) }?
            .rgba16()
            .ok_or_else(|| {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "gradient stop color must be RGBA8 or RGBA16",
                )
            })?;
        stops.push(GradientStop {
            position_milli: record.position_milli,
            color,
        });
    }
    let (end_x_milli, end_y_milli) = if input.feature_flags & INKPOD_GRADIENT_FLAG_CONSTRAIN_45 != 0
    {
        constrain_gradient_endpoint_45(
            input.start_x_milli,
            input.start_y_milli,
            input.end_x_milli,
            input.end_y_milli,
        )?
    } else {
        (input.end_x_milli, input.end_y_milli)
    };
    Ok(Gradient {
        kind,
        mode,
        start_x_milli: input.start_x_milli,
        start_y_milli: input.start_y_milli,
        end_x_milli,
        end_y_milli,
        dither: input.dither != 0,
        stops,
    })
}

fn constrain_gradient_endpoint_45(
    start_x: i64,
    start_y: i64,
    end_x: i64,
    end_y: i64,
) -> Result<(i64, i64), u32> {
    let dx = i128::from(end_x) - i128::from(start_x);
    let dy = i128::from(end_y) - i128::from(start_y);
    let dx2 = dx
        .unsigned_abs()
        .checked_mul(dx.unsigned_abs())
        .ok_or_else(|| fail(INKPOD_STATUS_INVALID_ARGUMENT, "gradient length overflows"))?;
    let dy2 = dy
        .unsigned_abs()
        .checked_mul(dy.unsigned_abs())
        .ok_or_else(|| fail(INKPOD_STATUS_INVALID_ARGUMENT, "gradient length overflows"))?;
    let length = integer_sqrt_u128(
        dx2.checked_add(dy2)
            .ok_or_else(|| fail(INKPOD_STATUS_INVALID_ARGUMENT, "gradient length overflows"))?,
    );
    let abs_x = dx.unsigned_abs();
    let abs_y = dy.unsigned_abs();
    let diagonal =
        abs_x.min(abs_y).saturating_mul(1_000_000) > abs_x.max(abs_y).saturating_mul(414_214);
    let component = if diagonal {
        ((length * 759_250_125 + (1_u128 << 29)) >> 30) as i128
    } else {
        length as i128
    };
    let signed =
        |magnitude: i128, original: i128| if original < 0 { -magnitude } else { magnitude };
    let (offset_x, offset_y) = if diagonal {
        (signed(component, dx), signed(component, dy))
    } else if abs_x >= abs_y {
        (signed(component, dx), 0)
    } else {
        (0, signed(component, dy))
    };
    let x = i128::from(start_x) + offset_x;
    let y = i128::from(start_y) + offset_y;
    Ok((
        x.try_into().map_err(|_| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "gradient X is outside bounds",
            )
        })?,
        y.try_into().map_err(|_| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "gradient Y is outside bounds",
            )
        })?,
    ))
}

const fn integer_sqrt_u128(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut bit = 1_u128 << 126;
    while bit > value {
        bit >>= 2;
    }
    let mut remainder = value;
    let mut root = 0_u128;
    while bit != 0 {
        if remainder >= root + bit {
            remainder -= root + bit;
            root = (root >> 1) + bit;
        } else {
            root >>= 1;
        }
        bit >>= 2;
    }
    root
}

// SAFETY: `input` and its borrowed nested color record are complete and readable.
pub(crate) unsafe fn parse_airbrush_input(
    input: &InkpodAirbrushInput,
) -> Result<AirbrushStroke, u32> {
    if input.feature_flags != INKPOD_FEATURE_NONE || input.reserved != 0 || input.reserved_2 != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "airbrush input contains unsupported flags or reserved values",
        ));
    }
    // SAFETY: The nested complete color record is part of the validated input.
    let color = unsafe { parse_color_value(&input.color) }?
        .rgba16()
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "airbrush color must be RGBA8 or RGBA16",
            )
        })?;
    Ok(AirbrushStroke {
        center_x_milli: input.center_x_milli,
        center_y_milli: input.center_y_milli,
        radius_milli: input.radius_milli,
        hardness_milli: input.hardness_milli,
        opacity_milli: input.opacity_milli,
        color,
    })
}

// SAFETY: `input` and every nested strided color record remain readable for
// this call. The returned colors own their copied values.
pub(crate) unsafe fn parse_boundary_airbrush_input(
    input: &InkpodBoundaryAirbrushInput,
) -> Result<BoundaryAirbrush, u32> {
    if input.feature_flags != INKPOD_FEATURE_NONE || input.reserved != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "boundary-airbrush input contains unsupported flags or reserved values",
        ));
    }
    // SAFETY: The nested array exposes its complete size prefix inside input.
    unsafe { validate_struct(&input.colors, "InkpodColorArray") }?;
    // SAFETY: The nested array and all advertised records are readable by contract.
    let colors = unsafe { parse_color_array(&input.colors) }?;
    if !(2..=MAX_GRADIENT_STOPS).contains(&colors.len()) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "boundary-airbrush color count is outside bounds",
        ));
    }
    let colors = colors
        .into_iter()
        .map(|color| {
            color.rgba16().ok_or_else(|| {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "boundary-airbrush colors must be RGBA8 or RGBA16",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BoundaryAirbrush {
        colors,
        width: input.width,
        strength_milli: input.strength_milli,
    })
}

// SAFETY: `input.pixels` advertises readable padded rows for this call. The
// returned sparse raster owns copied grayscale pixels.
pub(crate) unsafe fn parse_alpha_edit_input(
    input: &InkpodAlphaEditInput,
) -> Result<TileRaster, u32> {
    if input.feature_flags != INKPOD_FEATURE_NONE || input.reserved != 0 || input.reserved_2 != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "alpha-edit input contains unsupported flags or reserved values",
        ));
    }
    let format = match input.pixel_format {
        INKPOD_STORAGE_GRAYSCALE8 => PixelFormat::Grayscale8,
        INKPOD_STORAGE_GRAYSCALE16 => PixelFormat::Grayscale16,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "alpha-edit storage must be grayscale8 or grayscale16",
            ));
        }
    };
    let pixels = u64::from(input.width)
        .checked_mul(u64::from(input.height))
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "alpha-edit dimensions overflow",
            )
        })?;
    if input.width == 0
        || input.height == 0
        || input.width > MAX_RASTER_DIMENSION
        || input.height > MAX_RASTER_DIMENSION
        || pixels > MAX_IMAGE_EDIT_PIXELS
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "alpha-edit dimensions are outside bounds",
        ));
    }
    let bytes_per_pixel = format.bytes_per_pixel();
    let row_bytes = usize::try_from(input.width)
        .ok()
        .and_then(|width| width.checked_mul(bytes_per_pixel))
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "alpha-edit row size overflows",
            )
        })?;
    let stride = usize::try_from(input.row_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "alpha-edit row stride is not representable",
        )
    })?;
    let height = input.height as usize;
    let required = height
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(row_bytes))
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "alpha-edit byte range overflows",
            )
        })?;
    if input.pixels.is_null()
        || stride < row_bytes
        || required > isize::MAX as usize
        || required > MAX_COMMON_RASTER_BYTES
        || input.pixel_bytes < required as u64
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "alpha-edit pointer, stride, or byte length is invalid",
        ));
    }
    // SAFETY: `required` covers the final readable byte of every padded row.
    let bytes = unsafe { slice::from_raw_parts(input.pixels, required) };
    let mut raster = TileRaster::new(input.width, input.height, format)
        .map_err(|error| map_core_error(error.into()))?;
    for y in 0..height {
        for x in 0..input.width as usize {
            let offset = y * stride + x * bytes_per_pixel;
            let value = match format {
                PixelFormat::Grayscale8 => PixelValue::Grayscale8(bytes[offset]),
                PixelFormat::Grayscale16 => {
                    PixelValue::Grayscale16(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
                }
                _ => unreachable!("validated grayscale format"),
            };
            raster
                .set_pixel(x as u32, y as u32, value, 0)
                .map_err(|error| map_core_error(error.into()))?;
        }
    }
    Ok(raster)
}
