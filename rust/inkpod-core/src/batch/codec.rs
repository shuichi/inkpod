use super::validation::validate_operation;
use super::*;

pub(super) fn input_kind_code(kind: BatchInputKind) -> u32 {
    match kind {
        BatchInputKind::File => INPUT_FILE,
        BatchInputKind::Folder => INPUT_FOLDER,
        BatchInputKind::CurrentSequence => INPUT_CURRENT_SEQUENCE,
    }
}

pub(super) fn parse_input_kind(value: u32) -> Result<BatchInputKind, CoreError> {
    match value {
        INPUT_FILE => Ok(BatchInputKind::File),
        INPUT_FOLDER => Ok(BatchInputKind::Folder),
        INPUT_CURRENT_SEQUENCE => Ok(BatchInputKind::CurrentSequence),
        _ => Err(CoreError::InvalidArgument("batch input kind is unknown")),
    }
}

pub(super) fn output_policy_code(policy: BatchOutputPolicy) -> u32 {
    match policy {
        BatchOutputPolicy::Duplicate => OUTPUT_DUPLICATE,
        BatchOutputPolicy::NewSave => OUTPUT_NEW_SAVE,
        BatchOutputPolicy::ExplicitOverwrite => OUTPUT_OVERWRITE,
    }
}

pub(super) fn parse_output_policy(value: u32) -> Result<BatchOutputPolicy, CoreError> {
    match value {
        OUTPUT_DUPLICATE => Ok(BatchOutputPolicy::Duplicate),
        OUTPUT_NEW_SAVE => Ok(BatchOutputPolicy::NewSave),
        OUTPUT_OVERWRITE => Ok(BatchOutputPolicy::ExplicitOverwrite),
        _ => Err(CoreError::InvalidArgument("batch output policy is unknown")),
    }
}

pub(super) fn failure_policy_code(policy: BatchFailurePolicy) -> u32 {
    match policy {
        BatchFailurePolicy::Continue => FAILURE_CONTINUE,
        BatchFailurePolicy::Stop => FAILURE_STOP,
    }
}

pub(super) fn parse_failure_policy(value: u32) -> Result<BatchFailurePolicy, CoreError> {
    match value {
        FAILURE_CONTINUE => Ok(BatchFailurePolicy::Continue),
        FAILURE_STOP => Ok(BatchFailurePolicy::Stop),
        _ => Err(CoreError::InvalidArgument(
            "batch failure policy is unknown",
        )),
    }
}

pub(super) fn operation_to_file(
    operation: &BatchOperation,
) -> Result<FileBatchOperation, CoreError> {
    validate_operation(operation)?;
    let (kind, payload) = encode_operation_kind(&operation.kind)?;
    let target = operation
        .target
        .map_or(FileBatchTarget::default(), |target| FileBatchTarget {
            layer_id: target.layer_id.unwrap_or(0),
            plane_id: target.plane_id.unwrap_or(0),
            layer_kind: target.layer_kind.map_or(0, layer_kind_code),
            plane_kind: target.plane_kind.map_or(0, plane_kind_code),
            missing_policy: match target.missing_policy {
                BatchMissingTargetPolicy::Skip => MISSING_SKIP,
                BatchMissingTargetPolicy::Error => MISSING_ERROR,
            },
        });
    Ok(FileBatchOperation {
        version: operation.version,
        kind,
        flags: (if operation.enabled { OP_ENABLED } else { 0 })
            | (if operation.configure_each_run {
                OP_CONFIGURE_EACH_RUN
            } else {
                0
            }),
        target,
        payload,
    })
}

pub(super) fn operation_from_file(file: FileBatchOperation) -> Result<BatchOperation, CoreError> {
    if file.flags & !(OP_ENABLED | OP_CONFIGURE_EACH_RUN) != 0 {
        return Err(CoreError::InvalidArgument(
            "batch operation flags are invalid",
        ));
    }
    let target = if file.target == FileBatchTarget::default() {
        None
    } else {
        Some(BatchTargetSelector {
            layer_id: (file.target.layer_id != 0).then_some(file.target.layer_id),
            plane_id: (file.target.plane_id != 0).then_some(file.target.plane_id),
            layer_kind: (file.target.layer_kind != 0)
                .then(|| parse_layer_kind(file.target.layer_kind))
                .transpose()?,
            plane_kind: (file.target.plane_kind != 0)
                .then(|| parse_plane_kind(file.target.plane_kind))
                .transpose()?,
            missing_policy: match file.target.missing_policy {
                MISSING_SKIP => BatchMissingTargetPolicy::Skip,
                MISSING_ERROR => BatchMissingTargetPolicy::Error,
                _ => {
                    return Err(CoreError::InvalidArgument(
                        "batch missing-target policy is unknown",
                    ));
                }
            },
        })
    };
    let operation = BatchOperation {
        version: file.version,
        enabled: file.flags & OP_ENABLED != 0,
        configure_each_run: file.flags & OP_CONFIGURE_EACH_RUN != 0,
        target,
        kind: decode_operation_kind(file.kind, &file.payload)?,
    };
    validate_operation(&operation)?;
    Ok(operation)
}

pub(super) fn encode_operation_kind(
    kind: &BatchOperationKind,
) -> Result<(u32, Vec<u8>), CoreError> {
    let mut output = PayloadWriter::default();
    let code = match kind {
        BatchOperationKind::ColorReplace(pairs) => {
            output.u32(pairs.len() as u32);
            for pair in pairs {
                output.u32(u32::from(pair.enabled));
                output.pixel(pair.old);
                output.pixel(pair.new);
            }
            OP_COLOR_REPLACE
        }
        BatchOperationKind::ContinuousFill(seeds) => {
            output.u32(seeds.len() as u32);
            for seed in seeds {
                output.u32(seed.x);
                output.u32(seed.y);
                output.pixel(seed.color);
                output.u32(u32::from(seed.tolerance));
                output.u32(u32::from(seed.gap_close));
                output.u32(u32::from(seed.expected_source.is_some()));
                output.pixel(seed.expected_source.unwrap_or(PixelValue::Rgba([0; 4])));
            }
            OP_CONTINUOUS_FILL
        }
        BatchOperationKind::Separation(options) => {
            output.u32(options.colors.len() as u32);
            for color in &options.colors {
                output.pixel(*color);
            }
            output.pixel(options.replacement);
            output.u32(u32::from(options.invert));
            OP_SEPARATION
        }
        BatchOperationKind::Visibility { visible } => {
            output.u32(u32::from(*visible));
            OP_VISIBILITY
        }
        BatchOperationKind::LineWidth(mode) => {
            let (mode, value) = match mode {
                VectorWidthMode::Add(value) => (1, *value),
                VectorWidthMode::Subtract(value) => (2, *value),
                VectorWidthMode::Scale(value) => (3, *value),
                VectorWidthMode::Constant(value) => (4, *value),
            };
            output.u32(mode);
            output.u32(value.to_bits());
            OP_LINE_WIDTH
        }
        BatchOperationKind::Filter(filter) => {
            encode_filter(&mut output, filter)?;
            OP_FILTER
        }
        BatchOperationKind::BoundaryAirbrush(effect) => {
            output.u32(effect.colors.len() as u32);
            for color in &effect.colors {
                for component in color {
                    output.u32(u32::from(*component));
                }
            }
            output.u32(effect.width);
            output.u32(effect.strength_milli);
            OP_BOUNDARY_AIRBRUSH
        }
        BatchOperationKind::DustRemoval(options) => {
            output.u32(match options.mode {
                super::DustMode::RemoveForeground => 1,
                super::DustMode::FillTransparentHoles => 2,
                super::DustMode::ReplaceColorOutliers => 3,
            });
            output.u32(options.maximum_pixels);
            OP_DUST_REMOVAL
        }
        BatchOperationKind::Mirror(axis) => {
            output.u32(match axis {
                MirrorAxis::Horizontal => 1,
                MirrorAxis::Vertical => 2,
            });
            OP_MIRROR
        }
        BatchOperationKind::Rotate90(direction) => {
            output.u32(match direction {
                RotateDirection::Left90 => 1,
                RotateDirection::Right90 => 2,
            });
            OP_ROTATE_90
        }
        BatchOperationKind::Resize(resize) => {
            output.u32(resize.width);
            output.u32(resize.height);
            output.u32(resize.dpi_x_milli);
            output.u32(resize.dpi_y_milli);
            output.u32(u32::from(resize.resample));
            output.u32(resize_anchor_code(resize.anchor));
            OP_RESIZE
        }
        BatchOperationKind::ConvertPlane {
            destination_kind,
            destination_format,
        } => {
            output.u32(plane_kind_code(*destination_kind));
            output.u32(pixel_format_code(*destination_format));
            OP_CONVERT_PLANE
        }
    };
    Ok((code, output.bytes))
}

pub(super) fn decode_operation_kind(
    code: u32,
    payload: &[u8],
) -> Result<BatchOperationKind, CoreError> {
    let mut input = PayloadReader::new(payload);
    let kind = match code {
        OP_COLOR_REPLACE => {
            let count = input.count(MAX_BATCH_COLOR_PAIRS)?;
            let mut pairs = Vec::with_capacity(count);
            for _ in 0..count {
                pairs.push(BatchColorPair {
                    enabled: input.boolean()?,
                    old: input.pixel()?,
                    new: input.pixel()?,
                });
            }
            BatchOperationKind::ColorReplace(pairs)
        }
        OP_CONTINUOUS_FILL => {
            let count = input.count(MAX_BATCH_SEEDS)?;
            let mut seeds = Vec::with_capacity(count);
            for _ in 0..count {
                let x = input.u32()?;
                let y = input.u32()?;
                let color = input.pixel()?;
                let tolerance = u16::try_from(input.u32()?)
                    .map_err(|_| CoreError::InvalidArgument("batch fill tolerance is invalid"))?;
                let gap_close = u8::try_from(input.u32()?)
                    .map_err(|_| CoreError::InvalidArgument("batch gap-close value is invalid"))?;
                let has_expected = input.boolean()?;
                let expected = input.pixel()?;
                seeds.push(BatchSeed {
                    x,
                    y,
                    color,
                    tolerance,
                    gap_close,
                    expected_source: has_expected.then_some(expected),
                });
            }
            BatchOperationKind::ContinuousFill(seeds)
        }
        OP_SEPARATION => {
            let count = input.count(MAX_BATCH_COLORS)?;
            let mut colors = Vec::with_capacity(count);
            for _ in 0..count {
                colors.push(input.pixel()?);
            }
            BatchOperationKind::Separation(BatchSeparation {
                colors,
                replacement: input.pixel()?,
                invert: input.boolean()?,
            })
        }
        OP_VISIBILITY => BatchOperationKind::Visibility {
            visible: input.boolean()?,
        },
        OP_LINE_WIDTH => {
            let mode = input.u32()?;
            let value = f32::from_bits(input.u32()?);
            BatchOperationKind::LineWidth(match mode {
                1 => VectorWidthMode::Add(value),
                2 => VectorWidthMode::Subtract(value),
                3 => VectorWidthMode::Scale(value),
                4 => VectorWidthMode::Constant(value),
                _ => {
                    return Err(CoreError::InvalidArgument(
                        "batch line-width mode is unknown",
                    ));
                }
            })
        }
        OP_FILTER => BatchOperationKind::Filter(decode_filter(&mut input)?),
        OP_BOUNDARY_AIRBRUSH => {
            let count = input.count(MAX_BATCH_COLORS)?;
            let mut colors = Vec::with_capacity(count);
            for _ in 0..count {
                let mut color = [0_u16; 4];
                for component in &mut color {
                    *component = u16::try_from(input.u32()?).map_err(|_| {
                        CoreError::InvalidArgument("batch boundary color is invalid")
                    })?;
                }
                colors.push(color);
            }
            BatchOperationKind::BoundaryAirbrush(BoundaryAirbrush {
                colors,
                width: input.u32()?,
                strength_milli: input.u32()?,
            })
        }
        OP_DUST_REMOVAL => BatchOperationKind::DustRemoval(DustRemoval {
            mode: match input.u32()? {
                1 => super::DustMode::RemoveForeground,
                2 => super::DustMode::FillTransparentHoles,
                3 => super::DustMode::ReplaceColorOutliers,
                _ => return Err(CoreError::InvalidArgument("batch dust mode is unknown")),
            },
            maximum_pixels: input.u32()?,
        }),
        OP_MIRROR => BatchOperationKind::Mirror(match input.u32()? {
            1 => MirrorAxis::Horizontal,
            2 => MirrorAxis::Vertical,
            _ => return Err(CoreError::InvalidArgument("batch mirror axis is unknown")),
        }),
        OP_ROTATE_90 => BatchOperationKind::Rotate90(match input.u32()? {
            1 => RotateDirection::Left90,
            2 => RotateDirection::Right90,
            _ => {
                return Err(CoreError::InvalidArgument(
                    "batch rotation direction is unknown",
                ));
            }
        }),
        OP_RESIZE => BatchOperationKind::Resize(DocumentResize {
            width: input.u32()?,
            height: input.u32()?,
            dpi_x_milli: input.u32()?,
            dpi_y_milli: input.u32()?,
            resample: input.boolean()?,
            anchor: parse_resize_anchor(input.u32()?)?,
        }),
        OP_CONVERT_PLANE => BatchOperationKind::ConvertPlane {
            destination_kind: parse_plane_kind(input.u32()?)?,
            destination_format: parse_pixel_format(input.u32()?)?,
        },
        _ => {
            return Err(CoreError::InvalidArgument(
                "batch operation kind is unknown",
            ));
        }
    };
    input.finish()?;
    Ok(kind)
}

fn encode_filter(output: &mut PayloadWriter, filter: &Filter) -> Result<(), CoreError> {
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
                super::CurveInterpolation::Bezier => 1,
                super::CurveInterpolation::BSpline => 2,
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

fn decode_filter(input: &mut PayloadReader<'_>) -> Result<Filter, CoreError> {
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
                1 => super::CurveInterpolation::Bezier,
                2 => super::CurveInterpolation::BSpline,
                _ => {
                    return Err(CoreError::InvalidArgument(
                        "batch curve interpolation is unknown",
                    ));
                }
            };
            let count = input.count(super::MAX_CURVE_POINTS)?;
            let mut points = Vec::with_capacity(count);
            for _ in 0..count {
                points.push(super::CurvePoint {
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
        11 => Filter::Levels(super::Levels {
            channel: parse_channel(input.u32()?)?,
            input_shadow: input.u16()?,
            input_gamma_milli: input.u32()?,
            input_highlight: input.u16()?,
            output_shadow: input.u16()?,
            output_highlight: input.u16()?,
        }),
        12 => Filter::Hsv(super::HsvAdjustment {
            hue_degrees_milli: input.i32()?,
            saturation_milli: input.i32()?,
            value_milli: input.i32()?,
        }),
        13 => Filter::ColorBalance(super::ColorBalance {
            red_milli: input.i32()?,
            green_milli: input.i32()?,
            blue_milli: input.i32()?,
        }),
        _ => return Err(CoreError::InvalidArgument("batch filter kind is unknown")),
    })
}

pub(super) fn channel_code(channel: super::Channel) -> u32 {
    match channel {
        super::Channel::Rgb => 1,
        super::Channel::Red => 2,
        super::Channel::Green => 3,
        super::Channel::Blue => 4,
    }
}

pub(super) fn parse_channel(value: u32) -> Result<super::Channel, CoreError> {
    match value {
        1 => Ok(super::Channel::Rgb),
        2 => Ok(super::Channel::Red),
        3 => Ok(super::Channel::Green),
        4 => Ok(super::Channel::Blue),
        _ => Err(CoreError::InvalidArgument(
            "batch filter channel is unknown",
        )),
    }
}

pub(super) fn layer_kind_code(kind: LayerKind) -> u32 {
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

pub(super) fn parse_layer_kind(value: u32) -> Result<LayerKind, CoreError> {
    match value {
        1 => Ok(LayerKind::BinaryColoring),
        2 => Ok(LayerKind::GrayscaleColoring),
        3 => Ok(LayerKind::Raster),
        4 => Ok(LayerKind::Selection),
        5 => Ok(LayerKind::Frame),
        6 => Ok(LayerKind::VanishingPoint),
        7 => Ok(LayerKind::Adjustment),
        8 => Ok(LayerKind::Text),
        9 => Ok(LayerKind::Annotation),
        10 => Ok(LayerKind::VectorColoring),
        _ => Err(CoreError::InvalidArgument("batch layer kind is unknown")),
    }
}

pub(super) fn plane_kind_code(kind: PlaneType) -> u32 {
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

pub(super) fn parse_plane_kind(value: u32) -> Result<PlaneType, CoreError> {
    match value {
        1 => Ok(PlaneType::MainLine),
        2 => Ok(PlaneType::Color),
        3 => Ok(PlaneType::Raster),
        4 => Ok(PlaneType::Selection),
        5 => Ok(PlaneType::VectorMainLine),
        6 => Ok(PlaneType::ColorTrace),
        7 => Ok(PlaneType::VectorFill),
        _ => Err(CoreError::InvalidArgument("batch plane kind is unknown")),
    }
}

pub(super) fn pixel_format_code(format: PixelFormat) -> u32 {
    match format {
        PixelFormat::BinaryMask8 => 1,
        PixelFormat::Grayscale8 => 2,
        PixelFormat::Grayscale16 => 3,
        PixelFormat::StraightRgba8 => 4,
        PixelFormat::StraightRgba16 => 5,
        PixelFormat::PremultipliedBgra8 => 6,
    }
}

pub(super) fn parse_pixel_format(value: u32) -> Result<PixelFormat, CoreError> {
    match value {
        1 => Ok(PixelFormat::BinaryMask8),
        2 => Ok(PixelFormat::Grayscale8),
        3 => Ok(PixelFormat::Grayscale16),
        4 => Ok(PixelFormat::StraightRgba8),
        5 => Ok(PixelFormat::StraightRgba16),
        6 => Ok(PixelFormat::PremultipliedBgra8),
        _ => Err(CoreError::InvalidArgument(
            "batch destination pixel format is unknown",
        )),
    }
}

pub(super) fn resize_anchor_code(anchor: ResizeAnchor) -> u32 {
    match anchor {
        ResizeAnchor::TopLeft => 1,
        ResizeAnchor::TopRight => 2,
        ResizeAnchor::Center => 3,
        ResizeAnchor::BottomLeft => 4,
        ResizeAnchor::BottomRight => 5,
    }
}

pub(super) fn parse_resize_anchor(value: u32) -> Result<ResizeAnchor, CoreError> {
    match value {
        1 => Ok(ResizeAnchor::TopLeft),
        2 => Ok(ResizeAnchor::TopRight),
        3 => Ok(ResizeAnchor::Center),
        4 => Ok(ResizeAnchor::BottomLeft),
        5 => Ok(ResizeAnchor::BottomRight),
        _ => Err(CoreError::InvalidArgument("batch resize anchor is unknown")),
    }
}

#[derive(Default)]
struct PayloadWriter {
    bytes: Vec<u8>,
}

impl PayloadWriter {
    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn pixel(&mut self, value: PixelValue) {
        match value {
            PixelValue::Binary(value) => {
                self.u32(1);
                self.u32(u32::from(value));
            }
            PixelValue::Grayscale8(value) => {
                self.u32(2);
                self.u32(u32::from(value));
            }
            PixelValue::Grayscale16(value) => {
                self.u32(3);
                self.u32(u32::from(value));
            }
            PixelValue::Rgba(value) => {
                self.u32(4);
                for component in value {
                    self.u32(u32::from(component));
                }
            }
            PixelValue::Rgba16(value) => {
                self.u32(5);
                for component in value {
                    self.u32(u32::from(component));
                }
            }
        }
    }
}

struct PayloadReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> PayloadReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn u32(&mut self) -> Result<u32, CoreError> {
        let end = self
            .cursor
            .checked_add(4)
            .ok_or(CoreError::InvalidArgument(
                "batch operation payload offset overflows",
            ))?;
        let bytes: [u8; 4] = self
            .bytes
            .get(self.cursor..end)
            .ok_or(CoreError::InvalidArgument(
                "batch operation payload is truncated",
            ))?
            .try_into()
            .map_err(|_| CoreError::InvalidArgument("batch u32 payload is truncated"))?;
        self.cursor = end;
        Ok(u32::from_le_bytes(bytes))
    }

    fn i32(&mut self) -> Result<i32, CoreError> {
        Ok(i32::from_le_bytes(self.u32()?.to_le_bytes()))
    }

    fn u16(&mut self) -> Result<u16, CoreError> {
        u16::try_from(self.u32()?)
            .map_err(|_| CoreError::InvalidArgument("batch u16 payload is invalid"))
    }

    fn boolean(&mut self) -> Result<bool, CoreError> {
        match self.u32()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CoreError::InvalidArgument(
                "batch boolean payload is invalid",
            )),
        }
    }

    fn count(&mut self, maximum: usize) -> Result<usize, CoreError> {
        let count = self.u32()? as usize;
        if count > maximum {
            return Err(CoreError::InvalidArgument(
                "batch payload count exceeds the bounded limit",
            ));
        }
        Ok(count)
    }

    fn pixel(&mut self) -> Result<PixelValue, CoreError> {
        match self.u32()? {
            1 => Ok(PixelValue::Binary(u8::try_from(self.u32()?).map_err(
                |_| CoreError::InvalidArgument("batch binary color is invalid"),
            )?)),
            2 => Ok(PixelValue::Grayscale8(u8::try_from(self.u32()?).map_err(
                |_| CoreError::InvalidArgument("batch grayscale color is invalid"),
            )?)),
            3 => Ok(PixelValue::Grayscale16(self.u16()?)),
            4 => {
                let mut value = [0_u8; 4];
                for component in &mut value {
                    *component = u8::try_from(self.u32()?)
                        .map_err(|_| CoreError::InvalidArgument("batch RGBA8 color is invalid"))?;
                }
                Ok(PixelValue::Rgba(value))
            }
            5 => {
                let mut value = [0_u16; 4];
                for component in &mut value {
                    *component = self.u16()?;
                }
                Ok(PixelValue::Rgba16(value))
            }
            _ => Err(CoreError::InvalidArgument(
                "batch pixel payload kind is unknown",
            )),
        }
    }

    fn finish(&self) -> Result<(), CoreError> {
        if self.cursor == self.bytes.len() {
            Ok(())
        } else {
            Err(CoreError::InvalidArgument(
                "batch operation payload has trailing bytes",
            ))
        }
    }
}
