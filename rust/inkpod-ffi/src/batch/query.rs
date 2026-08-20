use super::*;

fn operation_at<'a>(graph: *const InkpodBatchGraph, index: u64) -> Result<&'a BatchOperation, u32> {
    if graph.is_null() || !is_aligned(graph) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch graph is null or misaligned",
        ));
    }
    let index = usize::try_from(index).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch operation index is not representable",
        )
    })?;
    // SAFETY: The caller keeps the immutable graph handle alive for the call.
    unsafe { &*graph }
        .graph
        .operations
        .get(index)
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch operation index is outside bounds",
            )
        })
}

const fn channel_code(channel: Channel) -> u32 {
    match channel {
        Channel::Rgb => INKPOD_FILTER_CHANNEL_RGB,
        Channel::Red => INKPOD_FILTER_CHANNEL_RED,
        Channel::Green => INKPOD_FILTER_CHANNEL_GREEN,
        Channel::Blue => INKPOD_FILTER_CHANNEL_BLUE,
    }
}

const fn interpolation_code(interpolation: CurveInterpolation) -> u32 {
    match interpolation {
        CurveInterpolation::Bezier => INKPOD_CURVE_BEZIER,
        CurveInterpolation::BSpline => INKPOD_CURVE_BSPLINE,
    }
}

fn write_filter_info(filter: &Filter, output: &mut InkpodBatchOperationInfo) {
    output.filter_channel = INKPOD_FILTER_CHANNEL_RGB;
    output.filter_interpolation = INKPOD_CURVE_BEZIER;
    match filter {
        Filter::SharpenWeak => output.filter_kind = INKPOD_FILTER_SHARPEN_WEAK,
        Filter::SharpenStrong => output.filter_kind = INKPOD_FILTER_SHARPEN_STRONG,
        Filter::BlurWeak => output.filter_kind = INKPOD_FILTER_BLUR_WEAK,
        Filter::BlurStrong => output.filter_kind = INKPOD_FILTER_BLUR_STRONG,
        Filter::GaussianBlur {
            radius,
            strength_milli,
        } => {
            output.filter_kind = INKPOD_FILTER_GAUSSIAN_BLUR;
            output.filter_parameters[0] = *radius as i32;
            output.filter_parameters[1] = *strength_milli as i32;
        }
        Filter::UnsharpMask {
            radius,
            amount_milli,
            threshold,
        } => {
            output.filter_kind = INKPOD_FILTER_UNSHARP_MASK;
            output.filter_parameters[0] = *radius as i32;
            output.filter_parameters[1] = *amount_milli as i32;
            output.filter_parameters[2] = i32::from(*threshold);
        }
        Filter::Invert { channel } => {
            output.filter_kind = INKPOD_FILTER_INVERT;
            output.filter_channel = channel_code(*channel);
        }
        Filter::AutoContrast => output.filter_kind = INKPOD_FILTER_AUTO_CONTRAST,
        Filter::BrightnessContrast {
            brightness_milli,
            contrast_milli,
        } => {
            output.filter_kind = INKPOD_FILTER_BRIGHTNESS_CONTRAST;
            output.filter_parameters[0] = *brightness_milli;
            output.filter_parameters[1] = *contrast_milli;
        }
        Filter::ToneCurve {
            channel,
            interpolation,
            points,
        } => {
            output.filter_kind = INKPOD_FILTER_TONE_CURVE;
            output.filter_channel = channel_code(*channel);
            output.filter_interpolation = interpolation_code(*interpolation);
            output.curve_point_count = points.len() as u64;
        }
        Filter::Levels(levels) => {
            output.filter_kind = INKPOD_FILTER_LEVELS;
            output.filter_channel = channel_code(levels.channel);
            output.filter_parameters = [
                i32::from(levels.input_shadow),
                levels.input_gamma_milli as i32,
                i32::from(levels.input_highlight),
                i32::from(levels.output_shadow),
                i32::from(levels.output_highlight),
            ];
        }
        Filter::Hsv(options) => {
            output.filter_kind = INKPOD_FILTER_HSV;
            output.filter_parameters[0] = options.hue_degrees_milli;
            output.filter_parameters[1] = options.saturation_milli;
            output.filter_parameters[2] = options.value_milli;
        }
        Filter::ColorBalance(options) => {
            output.filter_kind = INKPOD_FILTER_COLOR_BALANCE;
            output.filter_parameters[0] = options.red_milli;
            output.filter_parameters[1] = options.green_milli;
            output.filter_parameters[2] = options.blue_milli;
        }
    }
}

fn operation_info(
    operation: &BatchOperation,
    struct_size: u32,
) -> Result<InkpodBatchOperationInfo, u32> {
    let mut output = InkpodBatchOperationInfo {
        struct_size,
        version: operation.version,
        flags: (if operation.enabled {
            INKPOD_BATCH_OPERATION_ENABLED
        } else {
            0
        }) | (if operation.configure_each_run {
            INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN
        } else {
            0
        }),
        color_0: InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            ..InkpodColorValue::default()
        },
        color_1: InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            ..InkpodColorValue::default()
        },
        ..InkpodBatchOperationInfo::default()
    };
    if let Some(target) = operation.target {
        output.layer_id = target.layer_id.unwrap_or(0);
        output.plane_id = target.plane_id.unwrap_or(0);
        output.layer_kind = target.layer_kind.map_or(0, layer_kind_code);
        output.plane_kind = target.plane_kind.map_or(0, plane_type_code);
        output.missing_policy = match target.missing_policy {
            BatchMissingTargetPolicy::Skip => INKPOD_BATCH_MISSING_SKIP,
            BatchMissingTargetPolicy::Error => INKPOD_BATCH_MISSING_ERROR,
        };
    }
    match &operation.kind {
        BatchOperationKind::ColorReplace(pairs) => {
            output.kind = INKPOD_BATCH_OPERATION_COLOR_REPLACE;
            output.color_pair_count = pairs.len() as u64;
        }
        BatchOperationKind::ContinuousFill(seeds) => {
            output.kind = INKPOD_BATCH_OPERATION_CONTINUOUS_FILL;
            output.seed_count = seeds.len() as u64;
        }
        BatchOperationKind::Separation(options) => {
            output.kind = INKPOD_BATCH_OPERATION_SEPARATION;
            output.color_count = options.colors.len() as u64;
            output.color_0 = color_value_record(options.replacement)?;
            output.parameters[0] = i64::from(options.invert);
            output.parameters[1] = match options.destination {
                BatchSeparationDestination::ReplaceSource => INKPOD_BATCH_SEPARATION_REPLACE_SOURCE,
                BatchSeparationDestination::SelectionMask => INKPOD_BATCH_SEPARATION_SELECTION_MASK,
                BatchSeparationDestination::MainLinePlane => {
                    INKPOD_BATCH_SEPARATION_MAIN_LINE_PLANE
                }
                BatchSeparationDestination::ColorPlane => INKPOD_BATCH_SEPARATION_COLOR_PLANE,
                BatchSeparationDestination::NativeFile => INKPOD_BATCH_SEPARATION_NATIVE_FILE,
            };
        }
        BatchOperationKind::Visibility { visible } => {
            output.kind = INKPOD_BATCH_OPERATION_VISIBILITY;
            output.parameters[0] = i64::from(*visible);
        }
        BatchOperationKind::Filter(filter) => {
            output.kind = INKPOD_BATCH_OPERATION_FILTER;
            write_filter_info(filter, &mut output);
        }
        BatchOperationKind::BoundaryAirbrush(effect) => {
            output.kind = INKPOD_BATCH_OPERATION_BOUNDARY_AIRBRUSH;
            output.color_count = effect.colors.len() as u64;
            output.parameters[0] = i64::from(effect.width);
            output.parameters[1] = i64::from(effect.strength_milli);
        }
        BatchOperationKind::DustRemoval(options) => {
            output.kind = INKPOD_BATCH_OPERATION_DUST_REMOVAL;
            output.parameters[0] = match options.mode {
                DustMode::RemoveForeground => 1,
                DustMode::FillTransparentHoles => 2,
                DustMode::ReplaceColorOutliers => 3,
            };
            output.parameters[1] = i64::from(options.maximum_pixels);
        }
        BatchOperationKind::Mirror(axis) => {
            output.kind = INKPOD_BATCH_OPERATION_MIRROR;
            output.parameters[0] = match axis {
                MirrorAxis::Horizontal => 1,
                MirrorAxis::Vertical => 2,
            };
        }
        BatchOperationKind::Rotate90(direction) => {
            output.kind = INKPOD_BATCH_OPERATION_ROTATE_90;
            output.parameters[0] = match direction {
                RotateDirection::Left90 => 1,
                RotateDirection::Right90 => 2,
            };
        }
        BatchOperationKind::Resize(resize) => {
            output.kind = INKPOD_BATCH_OPERATION_RESIZE;
            output.parameters[0] = i64::from(resize.width);
            output.parameters[1] = i64::from(resize.height);
            output.parameters[2] = i64::from(resize.dpi_x_milli);
            output.parameters[3] = i64::from(resize.dpi_y_milli);
            output.parameters[4] = i64::from(resize.resample);
            output.parameters[5] = match resize.anchor {
                ResizeAnchor::TopLeft => 1,
                ResizeAnchor::TopRight => 2,
                ResizeAnchor::Center => 3,
                ResizeAnchor::BottomLeft => 4,
                ResizeAnchor::BottomRight => 5,
            };
        }
        BatchOperationKind::ConvertPlane {
            destination_kind,
            destination_format,
        } => {
            output.kind = INKPOD_BATCH_OPERATION_CONVERT_PLANE;
            output.parameters[0] = i64::from(plane_type_code(*destination_kind));
            output.parameters[1] = i64::from(storage_format_code(*destination_format));
        }
    }
    Ok(output)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_get_operation(
    graph: *const InkpodBatchGraph,
    index: u64,
    out_info: *mut InkpodBatchOperationInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodBatchOperationInfo") }
        {
            return status;
        }
        let operation = match operation_at(graph, index) {
            Ok(operation) => operation,
            Err(status) => return status,
        };
        let info = match operation_info(operation, unsafe { (*out_info).struct_size }) {
            Ok(info) => info,
            Err(status) => return status,
        };
        unsafe { out_info.write(info) };
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_get_operation_color(
    graph: *const InkpodBatchGraph,
    operation_index: u64,
    color_index: u64,
    out_color: *mut InkpodColorValue,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if let Err(status) = unsafe { validate_struct(out_color.cast_const(), "InkpodColorValue") }
        {
            return status;
        }
        let operation = match operation_at(graph, operation_index) {
            Ok(operation) => operation,
            Err(status) => return status,
        };
        let Ok(color_index) = usize::try_from(color_index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch color index is not representable",
            );
        };
        let color = match &operation.kind {
            BatchOperationKind::Separation(options) => options.colors.get(color_index).copied(),
            BatchOperationKind::BoundaryAirbrush(effect) => effect
                .colors
                .get(color_index)
                .copied()
                .map(PixelValue::Rgba16),
            _ => None,
        };
        let Some(color) = color else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch color index is outside bounds",
            );
        };
        match write_color_value(unsafe { &mut *out_color }, color) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(status) => status,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_get_operation_color_pair(
    graph: *const InkpodBatchGraph,
    operation_index: u64,
    pair_index: u64,
    out_pair: *mut InkpodBatchColorPairInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if let Err(status) =
            unsafe { validate_struct(out_pair.cast_const(), "InkpodBatchColorPairInput") }
        {
            return status;
        }
        let operation = match operation_at(graph, operation_index) {
            Ok(operation) => operation,
            Err(status) => return status,
        };
        let Ok(pair_index) = usize::try_from(pair_index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch color-pair index is not representable",
            );
        };
        let BatchOperationKind::ColorReplace(pairs) = &operation.kind else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch operation has no color pairs",
            );
        };
        let Some(pair) = pairs.get(pair_index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch color-pair index is outside bounds",
            );
        };
        let output = unsafe { &mut *out_pair };
        output.enabled = u32::from(pair.enabled);
        output.reserved = 0;
        output.old_color = match color_value_record(pair.old) {
            Ok(color) => color,
            Err(status) => return status,
        };
        output.new_color = match color_value_record(pair.new) {
            Ok(color) => color,
            Err(status) => return status,
        };
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_get_operation_seed(
    graph: *const InkpodBatchGraph,
    operation_index: u64,
    seed_index: u64,
    out_seed: *mut InkpodBatchSeedInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if let Err(status) =
            unsafe { validate_struct(out_seed.cast_const(), "InkpodBatchSeedInput") }
        {
            return status;
        }
        let operation = match operation_at(graph, operation_index) {
            Ok(operation) => operation,
            Err(status) => return status,
        };
        let Ok(seed_index) = usize::try_from(seed_index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch seed index is not representable",
            );
        };
        let BatchOperationKind::ContinuousFill(seeds) = &operation.kind else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch operation has no fill seeds",
            );
        };
        let Some(seed) = seeds.get(seed_index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch seed index is outside bounds",
            );
        };
        let output = unsafe { &mut *out_seed };
        output.flags = (if seed.enabled {
            INKPOD_BATCH_SEED_ENABLED
        } else {
            0
        }) | (if seed.expected_source.is_some() {
            INKPOD_BATCH_SEED_HAS_EXPECTED_COLOR
        } else {
            0
        });
        output.x = seed.x;
        output.y = seed.y;
        output.tolerance = u32::from(seed.tolerance);
        output.gap_close = u32::from(seed.gap_close);
        output.reserved = 0;
        output.fill_color = match color_value_record(seed.color) {
            Ok(color) => color,
            Err(status) => return status,
        };
        output.expected_color =
            match color_value_record(seed.expected_source.unwrap_or(PixelValue::Rgba([0; 4]))) {
                Ok(color) => color,
                Err(status) => return status,
            };
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_get_operation_curve_point(
    graph: *const InkpodBatchGraph,
    operation_index: u64,
    point_index: u64,
    out_point: *mut InkpodCurvePoint,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if let Err(status) = unsafe { validate_struct(out_point.cast_const(), "InkpodCurvePoint") }
        {
            return status;
        }
        let operation = match operation_at(graph, operation_index) {
            Ok(operation) => operation,
            Err(status) => return status,
        };
        let Ok(point_index) = usize::try_from(point_index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch curve-point index is not representable",
            );
        };
        let BatchOperationKind::Filter(Filter::ToneCurve { points, .. }) = &operation.kind else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch operation has no curve points",
            );
        };
        let Some(point) = points.get(point_index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch curve-point index is outside bounds",
            );
        };
        let output = unsafe { &mut *out_point };
        output.reserved = 0;
        output.input = u32::from(point.input);
        output.output = u32::from(point.output);
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_clone_with_operations(
    graph: *const InkpodBatchGraph,
    operations: *const InkpodBatchOperationInput,
    operation_count: u64,
    operation_stride_bytes: u64,
    out_graph: *mut *mut InkpodBatchGraph,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if graph.is_null() || !is_aligned(graph) || out_graph.is_null() || !is_aligned(out_graph) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch run-copy pointer is null or misaligned",
            );
        }
        if !unsafe { out_graph.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch run-copy output already owns a handle",
            );
        }
        let parsed = match unsafe {
            parse_operation_records(operations, operation_count, operation_stride_bytes)
        } {
            Ok(operations) => operations,
            Err(status) => return status,
        };
        let source = &unsafe { &*graph }.graph;
        if parsed.len() != source.operations.len() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch run-copy operation count does not match source graph",
            );
        }
        if parsed.iter().any(|operation| operation.configure_each_run) {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch run-copy contains unresolved per-run configuration",
            );
        }
        let mut run = source.clone();
        run.operations = parsed;
        if let Err(error) = run.validate() {
            return map_core_error(error);
        }
        unsafe { out_graph.write(Box::into_raw(Box::new(InkpodBatchGraph { graph: run }))) };
        INKPOD_STATUS_OK
    })
}
