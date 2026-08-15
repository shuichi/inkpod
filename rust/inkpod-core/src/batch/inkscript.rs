//! Private pre-ratification grouped projection for legacy Batch operations.

use super::validation::validate_operation;
use super::*;
use crate::primitive::{
    CanonicalInvocation, LegacyImageAdapterError, LegacyImageScriptStep, LegacySimpleAdapterError,
    LegacySimpleScriptStep,
};

#[derive(Clone, Debug)]
enum LegacyBatchScriptStep {
    Simple(LegacySimpleScriptStep),
    Image(LegacyImageScriptStep),
    VectorWidth {
        invocation: CanonicalInvocation,
        editor_group: String,
    },
}

#[derive(Clone, Debug)]
enum LegacyBatchProjection {
    ColorReplace,
    ContinuousFill {
        expected_sources: Vec<Option<PixelValue>>,
    },
    Separation,
    Visibility {
        editable: bool,
        opacity_milli: u32,
        name: String,
        plane: bool,
    },
    LineWidth,
    Filter,
    BoundaryAirbrush,
    DustRemoval,
    Mirror,
    Rotate,
    Resize,
    ConvertPlane,
}

#[derive(Clone, Debug)]
struct LegacyBatchScriptGroup {
    editor_group: String,
    operation_enabled: bool,
    configure_each_run: bool,
    target: Option<BatchTargetSelector>,
    resolved_target: Option<(u64, Option<u64>)>,
    projection: LegacyBatchProjection,
    steps: Vec<LegacyBatchScriptStep>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum LegacyBatchProjectionError {
    InvalidOperation,
    MissingTarget,
    AmbiguousTarget,
    EmptyVectorTarget,
    ImageAdapter(LegacyImageAdapterError),
    SimpleAdapter(LegacySimpleAdapterError),
    NotProjectable,
}

impl From<LegacyImageAdapterError> for LegacyBatchProjectionError {
    fn from(error: LegacyImageAdapterError) -> Self {
        Self::ImageAdapter(error)
    }
}

impl From<LegacySimpleAdapterError> for LegacyBatchProjectionError {
    fn from(error: LegacySimpleAdapterError) -> Self {
        Self::SimpleAdapter(error)
    }
}

impl LegacyBatchScriptStep {
    fn enabled(&self) -> bool {
        match self {
            Self::Simple(_) | Self::VectorWidth { .. } => true,
            Self::Image(step) => step.enabled(),
        }
    }

    fn editor_group(&self) -> Option<&str> {
        match self {
            Self::Simple(_) => None,
            Self::Image(step) => step.editor_group(),
            Self::VectorWidth { editor_group, .. } => Some(editor_group),
        }
    }

    fn to_canonical(&self) -> Result<CanonicalInvocation, LegacyBatchProjectionError> {
        match self {
            Self::Simple(step) => step.to_canonical().map_err(Into::into),
            Self::Image(step) => step.to_canonical().map_err(Into::into),
            Self::VectorWidth { invocation, .. } => Ok(invocation.clone()),
        }
    }
}

impl LegacyBatchScriptGroup {
    fn from_operation(
        core: &Core,
        operation: &BatchOperation,
        editor_group: &str,
    ) -> Result<Self, LegacyBatchProjectionError> {
        validate_operation(operation).map_err(|_| LegacyBatchProjectionError::InvalidOperation)?;
        if editor_group.is_empty() {
            return Err(LegacyBatchProjectionError::InvalidOperation);
        }
        let resolved_target = operation
            .target
            .as_ref()
            .map(|target| resolve_target_exact(core, target))
            .transpose()?;
        let target_plane = || {
            resolved_target
                .and_then(|(_, plane_id)| plane_id)
                .ok_or(LegacyBatchProjectionError::MissingTarget)
        };
        let target_pair = || resolved_target.ok_or(LegacyBatchProjectionError::MissingTarget);
        let image = |invocation: &CanonicalInvocation, enabled| {
            LegacyImageScriptStep::from_canonical(invocation, enabled, editor_group)
                .map(LegacyBatchScriptStep::Image)
                .map_err(LegacyBatchProjectionError::from)
        };
        let simple = |invocation: &CanonicalInvocation| {
            LegacySimpleScriptStep::from_canonical(invocation)
                .map(LegacyBatchScriptStep::Simple)
                .map_err(LegacyBatchProjectionError::from)
        };

        let (projection, steps) = match &operation.kind {
            BatchOperationKind::ColorReplace(pairs) => {
                let invocation = CanonicalInvocation::ReplaceRasterColors {
                    plane_id: target_plane()?,
                    pairs: pairs.clone(),
                };
                (
                    LegacyBatchProjection::ColorReplace,
                    vec![image(&invocation, true)?],
                )
            }
            BatchOperationKind::ContinuousFill(seeds) => {
                let (layer_id, plane_id) = target_pair()?;
                let plane_id = plane_id.ok_or(LegacyBatchProjectionError::MissingTarget)?;
                let mut steps = Vec::with_capacity(seeds.len());
                let mut expected_sources = Vec::with_capacity(seeds.len());
                for seed in seeds {
                    let invocation = CanonicalInvocation::ApplyFill {
                        request: FillRequest {
                            operation: FillOperation::Seed,
                            seed_x: seed.x,
                            seed_y: seed.y,
                            color: seed.color,
                            selection: None,
                            use_document_selection: false,
                            tolerance: seed.tolerance,
                            detached_regions: false,
                            overflow_abort: true,
                            gap_close: seed.gap_close,
                            transparent_only: false,
                            inclusion_mode: InclusionMode::None,
                            inclusion_colors: Vec::new(),
                            extension_distance: 0,
                        },
                        target: crate::EditorTarget { layer_id, plane_id },
                        use_light_table_boundary: false,
                        use_light_table_color: false,
                    };
                    steps.push(image(&invocation, seed.enabled)?);
                    expected_sources.push(seed.expected_source);
                }
                (
                    LegacyBatchProjection::ContinuousFill { expected_sources },
                    steps,
                )
            }
            BatchOperationKind::Separation(options) => {
                let invocation = CanonicalInvocation::SeparateRasterColors {
                    plane_id: target_plane()?,
                    options: options.clone(),
                };
                (
                    LegacyBatchProjection::Separation,
                    vec![image(&invocation, true)?],
                )
            }
            BatchOperationKind::Visibility { visible } => {
                let (layer_id, plane_id) = target_pair()?;
                let layer = core
                    .layers()
                    .map_err(|_| LegacyBatchProjectionError::MissingTarget)?
                    .into_iter()
                    .find(|layer| layer.id == layer_id)
                    .ok_or(LegacyBatchProjectionError::MissingTarget)?;
                if let Some(plane_id) = plane_id {
                    let plane = layer
                        .planes
                        .iter()
                        .find(|plane| plane.id == plane_id)
                        .ok_or(LegacyBatchProjectionError::MissingTarget)?;
                    let invocation = CanonicalInvocation::SetPlaneProperties {
                        plane_id,
                        visible: *visible,
                        editable: plane.editable,
                        opacity_milli: plane.opacity_milli,
                        name: plane.name.clone(),
                    };
                    (
                        LegacyBatchProjection::Visibility {
                            editable: plane.editable,
                            opacity_milli: plane.opacity_milli,
                            name: plane.name.clone(),
                            plane: true,
                        },
                        vec![simple(&invocation)?],
                    )
                } else {
                    let invocation = CanonicalInvocation::SetLayerProperties {
                        layer_id,
                        visible: *visible,
                        editable: layer.editable,
                        opacity_milli: layer.opacity_milli,
                        name: layer.name.clone(),
                    };
                    (
                        LegacyBatchProjection::Visibility {
                            editable: layer.editable,
                            opacity_milli: layer.opacity_milli,
                            name: layer.name.clone(),
                            plane: false,
                        },
                        vec![simple(&invocation)?],
                    )
                }
            }
            BatchOperationKind::LineWidth(mode) => {
                let plane_id = target_plane()?;
                let path_ids = core
                    .vector_paths()
                    .map_err(|_| LegacyBatchProjectionError::MissingTarget)?
                    .into_iter()
                    .filter(|path| path.plane_id == plane_id)
                    .map(|path| path.id)
                    .collect::<Vec<_>>();
                if path_ids.is_empty() {
                    return Err(LegacyBatchProjectionError::EmptyVectorTarget);
                }
                (
                    LegacyBatchProjection::LineWidth,
                    vec![LegacyBatchScriptStep::VectorWidth {
                        invocation: CanonicalInvocation::VectorCorrectWidth {
                            path_ids,
                            mode: *mode,
                        },
                        editor_group: editor_group.to_owned(),
                    }],
                )
            }
            BatchOperationKind::Filter(filter) => {
                let invocation = CanonicalInvocation::ApplyFilter {
                    plane_id: target_plane()?,
                    filter: filter.clone(),
                };
                (
                    LegacyBatchProjection::Filter,
                    vec![image(&invocation, true)?],
                )
            }
            BatchOperationKind::BoundaryAirbrush(effect) => {
                let invocation = CanonicalInvocation::ApplyBoundaryAirbrush {
                    plane_id: target_plane()?,
                    effect: effect.clone(),
                };
                (
                    LegacyBatchProjection::BoundaryAirbrush,
                    vec![image(&invocation, true)?],
                )
            }
            BatchOperationKind::DustRemoval(options) => {
                let invocation = CanonicalInvocation::ApplyDustRemoval {
                    plane_id: target_plane()?,
                    shape: None,
                    options: *options,
                };
                (
                    LegacyBatchProjection::DustRemoval,
                    vec![image(&invocation, true)?],
                )
            }
            BatchOperationKind::Mirror(axis) => {
                let invocation = CanonicalInvocation::MirrorDocument { axis: *axis };
                (LegacyBatchProjection::Mirror, vec![simple(&invocation)?])
            }
            BatchOperationKind::Rotate90(direction) => {
                let invocation = CanonicalInvocation::RotateDocument {
                    direction: *direction,
                };
                (LegacyBatchProjection::Rotate, vec![simple(&invocation)?])
            }
            BatchOperationKind::Resize(resize) => {
                let invocation = CanonicalInvocation::ResizeDocument { resize: *resize };
                (LegacyBatchProjection::Resize, vec![simple(&invocation)?])
            }
            BatchOperationKind::ConvertPlane {
                destination_kind,
                destination_format,
            } => {
                let invocation = CanonicalInvocation::ConvertPlane {
                    plane_id: target_plane()?,
                    destination_kind: *destination_kind,
                    destination_format: *destination_format,
                };
                (
                    LegacyBatchProjection::ConvertPlane,
                    vec![simple(&invocation)?],
                )
            }
        };
        Ok(Self {
            editor_group: editor_group.to_owned(),
            operation_enabled: operation.enabled,
            configure_each_run: operation.configure_each_run,
            target: operation.target,
            resolved_target,
            projection,
            steps,
        })
    }

    fn step_count(&self) -> usize {
        self.steps.len()
    }

    fn enabled_step_count(&self) -> usize {
        self.steps.iter().filter(|step| step.enabled()).count()
    }

    fn canonical_invocations(
        &self,
    ) -> Result<Vec<CanonicalInvocation>, LegacyBatchProjectionError> {
        if !self.operation_enabled {
            return Ok(Vec::new());
        }
        self.validate_group_identity()?;
        self.steps
            .iter()
            .filter(|step| step.enabled())
            .map(LegacyBatchScriptStep::to_canonical)
            .collect()
    }

    fn to_operation(&self) -> Result<BatchOperation, LegacyBatchProjectionError> {
        self.validate_group_identity()?;
        let canonical = self
            .steps
            .iter()
            .map(LegacyBatchScriptStep::to_canonical)
            .collect::<Result<Vec<_>, _>>()?;
        let kind = match &self.projection {
            LegacyBatchProjection::ColorReplace => match canonical.as_slice() {
                [CanonicalInvocation::ReplaceRasterColors { plane_id, pairs }]
                    if self.matches_plane(*plane_id) =>
                {
                    BatchOperationKind::ColorReplace(pairs.clone())
                }
                _ => return Err(LegacyBatchProjectionError::NotProjectable),
            },
            LegacyBatchProjection::ContinuousFill { expected_sources } => {
                if canonical.len() != expected_sources.len() {
                    return Err(LegacyBatchProjectionError::NotProjectable);
                }
                let mut seeds = Vec::with_capacity(canonical.len());
                for ((step, invocation), expected_source) in self
                    .steps
                    .iter()
                    .zip(canonical.iter())
                    .zip(expected_sources.iter())
                {
                    let CanonicalInvocation::ApplyFill {
                        request,
                        target,
                        use_light_table_boundary: false,
                        use_light_table_color: false,
                    } = invocation
                    else {
                        return Err(LegacyBatchProjectionError::NotProjectable);
                    };
                    if !self.matches_pair(target.layer_id, target.plane_id)
                        || request.operation != FillOperation::Seed
                        || request.selection.is_some()
                        || request.use_document_selection
                        || request.detached_regions
                        || !request.overflow_abort
                        || request.transparent_only
                        || request.inclusion_mode != InclusionMode::None
                        || !request.inclusion_colors.is_empty()
                        || request.extension_distance != 0
                    {
                        return Err(LegacyBatchProjectionError::NotProjectable);
                    }
                    seeds.push(BatchSeed {
                        enabled: step.enabled(),
                        x: request.seed_x,
                        y: request.seed_y,
                        color: request.color,
                        tolerance: request.tolerance,
                        gap_close: request.gap_close,
                        expected_source: *expected_source,
                    });
                }
                BatchOperationKind::ContinuousFill(seeds)
            }
            LegacyBatchProjection::Separation => match canonical.as_slice() {
                [CanonicalInvocation::SeparateRasterColors { plane_id, options }]
                    if self.matches_plane(*plane_id) =>
                {
                    BatchOperationKind::Separation(options.clone())
                }
                _ => return Err(LegacyBatchProjectionError::NotProjectable),
            },
            LegacyBatchProjection::Visibility {
                editable,
                opacity_milli,
                name,
                plane,
            } => match canonical.as_slice() {
                [
                    CanonicalInvocation::SetPlaneProperties {
                        plane_id,
                        visible,
                        editable: actual_editable,
                        opacity_milli: actual_opacity,
                        name: actual_name,
                    },
                ] if *plane
                    && self.matches_plane(*plane_id)
                    && actual_editable == editable
                    && actual_opacity == opacity_milli
                    && actual_name == name =>
                {
                    BatchOperationKind::Visibility { visible: *visible }
                }
                [
                    CanonicalInvocation::SetLayerProperties {
                        layer_id,
                        visible,
                        editable: actual_editable,
                        opacity_milli: actual_opacity,
                        name: actual_name,
                    },
                ] if !*plane
                    && self.matches_layer(*layer_id)
                    && actual_editable == editable
                    && actual_opacity == opacity_milli
                    && actual_name == name =>
                {
                    BatchOperationKind::Visibility { visible: *visible }
                }
                _ => return Err(LegacyBatchProjectionError::NotProjectable),
            },
            LegacyBatchProjection::LineWidth => match canonical.as_slice() {
                [CanonicalInvocation::VectorCorrectWidth { mode, .. }] => {
                    BatchOperationKind::LineWidth(*mode)
                }
                _ => return Err(LegacyBatchProjectionError::NotProjectable),
            },
            LegacyBatchProjection::Filter => match canonical.as_slice() {
                [CanonicalInvocation::ApplyFilter { plane_id, filter }]
                    if self.matches_plane(*plane_id) =>
                {
                    BatchOperationKind::Filter(filter.clone())
                }
                _ => return Err(LegacyBatchProjectionError::NotProjectable),
            },
            LegacyBatchProjection::BoundaryAirbrush => match canonical.as_slice() {
                [CanonicalInvocation::ApplyBoundaryAirbrush { plane_id, effect }]
                    if self.matches_plane(*plane_id) =>
                {
                    BatchOperationKind::BoundaryAirbrush(effect.clone())
                }
                _ => return Err(LegacyBatchProjectionError::NotProjectable),
            },
            LegacyBatchProjection::DustRemoval => match canonical.as_slice() {
                [
                    CanonicalInvocation::ApplyDustRemoval {
                        plane_id,
                        shape: None,
                        options,
                    },
                ] if self.matches_plane(*plane_id) => BatchOperationKind::DustRemoval(*options),
                _ => return Err(LegacyBatchProjectionError::NotProjectable),
            },
            LegacyBatchProjection::Mirror => match canonical.as_slice() {
                [CanonicalInvocation::MirrorDocument { axis }] => BatchOperationKind::Mirror(*axis),
                _ => return Err(LegacyBatchProjectionError::NotProjectable),
            },
            LegacyBatchProjection::Rotate => match canonical.as_slice() {
                [CanonicalInvocation::RotateDocument { direction }] => {
                    BatchOperationKind::Rotate90(*direction)
                }
                _ => return Err(LegacyBatchProjectionError::NotProjectable),
            },
            LegacyBatchProjection::Resize => match canonical.as_slice() {
                [CanonicalInvocation::ResizeDocument { resize }] => {
                    BatchOperationKind::Resize(*resize)
                }
                _ => return Err(LegacyBatchProjectionError::NotProjectable),
            },
            LegacyBatchProjection::ConvertPlane => match canonical.as_slice() {
                [
                    CanonicalInvocation::ConvertPlane {
                        plane_id,
                        destination_kind,
                        destination_format,
                    },
                ] if self.matches_plane(*plane_id) => BatchOperationKind::ConvertPlane {
                    destination_kind: *destination_kind,
                    destination_format: *destination_format,
                },
                _ => return Err(LegacyBatchProjectionError::NotProjectable),
            },
        };
        Ok(BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: self.operation_enabled,
            configure_each_run: self.configure_each_run,
            target: self.target,
            kind,
        })
    }

    fn validate_group_identity(&self) -> Result<(), LegacyBatchProjectionError> {
        if self.editor_group.is_empty() {
            return Err(LegacyBatchProjectionError::NotProjectable);
        }
        if matches!(
            self.projection,
            LegacyBatchProjection::ContinuousFill { .. }
        ) && self
            .steps
            .iter()
            .any(|step| step.editor_group() != Some(self.editor_group.as_str()))
        {
            return Err(LegacyBatchProjectionError::NotProjectable);
        }
        Ok(())
    }

    fn matches_layer(&self, layer_id: u64) -> bool {
        self.resolved_target
            .is_some_and(|(layer, _)| layer == layer_id)
    }

    fn matches_plane(&self, plane_id: u64) -> bool {
        self.resolved_target
            .is_some_and(|(_, plane)| plane == Some(plane_id))
    }

    fn matches_pair(&self, layer_id: u64, plane_id: u64) -> bool {
        self.resolved_target == Some((layer_id, Some(plane_id)))
    }

    #[cfg(test)]
    fn break_editor_group_for_test(&mut self) {
        self.editor_group.push_str("_advanced");
    }
}

fn resolve_target_exact(
    core: &Core,
    selector: &BatchTargetSelector,
) -> Result<(u64, Option<u64>), LegacyBatchProjectionError> {
    let layers = core
        .layers()
        .map_err(|_| LegacyBatchProjectionError::MissingTarget)?;
    let matching_layers = layers
        .iter()
        .filter(|layer| {
            selector.layer_id.is_none_or(|id| layer.id == id)
                && selector.layer_kind.is_none_or(|kind| layer.kind == kind)
        })
        .collect::<Vec<_>>();
    let [layer] = matching_layers.as_slice() else {
        return if matching_layers.is_empty() {
            Err(LegacyBatchProjectionError::MissingTarget)
        } else {
            Err(LegacyBatchProjectionError::AmbiguousTarget)
        };
    };
    if selector.plane_id.is_none() && selector.plane_kind.is_none() {
        return Ok((layer.id, None));
    }
    let matching_planes = layer
        .planes
        .iter()
        .filter(|plane| {
            selector.plane_id.is_none_or(|id| plane.id == id)
                && selector.plane_kind.is_none_or(|kind| plane.kind == kind)
        })
        .collect::<Vec<_>>();
    let [plane] = matching_planes.as_slice() else {
        return if matching_planes.is_empty() {
            Err(LegacyBatchProjectionError::MissingTarget)
        } else {
            Err(LegacyBatchProjectionError::AmbiguousTarget)
        };
    };
    Ok((layer.id, Some(plane.id)))
}

#[cfg(test)]
mod tests {
    use super::super::operations::apply_operation;
    use super::*;
    use crate::primitive::canonical_document_state;
    use crate::{
        BrushShape, ColorBalance, CurveInterpolation, CurvePoint, DustMode, HsvAdjustment, Levels,
        PaintTool, PointF32, StartColorPredicate, Stroke, StrokeSample, VectorCubicSegment,
        VectorPathInput,
    };

    fn core() -> Core {
        let mut core = Core::new();
        core.new_cell(8, 6, crate::DEFAULT_DPI_MILLI, crate::DEFAULT_DPI_MILLI)
            .unwrap();
        core
    }

    fn color_target(core: &Core, missing_policy: BatchMissingTargetPolicy) -> BatchTargetSelector {
        let layer = &core.layers().unwrap()[0];
        let plane = layer
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneType::Color)
            .unwrap();
        BatchTargetSelector {
            layer_id: Some(layer.id),
            plane_id: Some(plane.id),
            layer_kind: Some(layer.kind),
            plane_kind: Some(plane.kind),
            missing_policy,
        }
    }

    fn operation(core: &Core, kind: BatchOperationKind) -> BatchOperation {
        BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            configure_each_run: false,
            target: Some(color_target(core, BatchMissingTargetPolicy::Error)),
            kind,
        }
    }

    fn filters() -> Vec<Filter> {
        vec![
            Filter::SharpenWeak,
            Filter::SharpenStrong,
            Filter::BlurWeak,
            Filter::BlurStrong,
            Filter::GaussianBlur {
                radius: 2,
                strength_milli: 500,
            },
            Filter::UnsharpMask {
                radius: 2,
                amount_milli: 750,
                threshold: 12,
            },
            Filter::Invert {
                channel: Channel::Green,
            },
            Filter::AutoContrast,
            Filter::BrightnessContrast {
                brightness_milli: -100,
                contrast_milli: 200,
            },
            Filter::ToneCurve {
                channel: Channel::Blue,
                interpolation: CurveInterpolation::BSpline,
                points: vec![
                    CurvePoint {
                        input: 0,
                        output: 1,
                    },
                    CurvePoint {
                        input: u16::MAX,
                        output: u16::MAX - 1,
                    },
                ],
            },
            Filter::Levels(Levels {
                channel: Channel::Red,
                input_shadow: 1,
                input_gamma_milli: 1_100,
                input_highlight: u16::MAX - 1,
                output_shadow: 2,
                output_highlight: u16::MAX - 2,
            }),
            Filter::Hsv(HsvAdjustment {
                hue_degrees_milli: 45_000,
                saturation_milli: 100,
                value_milli: -100,
            }),
            Filter::ColorBalance(ColorBalance {
                red_milli: 100,
                green_milli: -100,
                blue_milli: 50,
            }),
        ]
    }

    fn painted_core() -> Core {
        let mut core = core();
        core.set_active_plane(ActivePlane::Color).unwrap();
        core.apply_stroke(&Stroke {
            tool: PaintTool::Pencil,
            plane: ActivePlane::Color,
            color: [10, 20, 30, 255],
            diameter: 1.0,
            shape: BrushShape::Round,
            smoothing: 0,
            start_color: StartColorPredicate::Any,
            auto_erase: false,
            pressure_size: false,
            coordinate_space: crate::CoordinateSpace::Document,
            samples: vec![StrokeSample {
                x: 1.0,
                y: 1.0,
                pressure: 1.0,
            }],
        })
        .unwrap();
        core
    }

    fn digest(core: &Core) -> crate::DocumentStateDigest {
        canonical_document_state(core.document.as_ref().unwrap())
            .unwrap()
            .1
    }

    #[test]
    fn continuous_fill_is_one_seed_per_step_and_round_trips_only_as_one_group() {
        let core = core();
        let operation = operation(
            &core,
            BatchOperationKind::ContinuousFill(vec![
                BatchSeed {
                    enabled: true,
                    x: 1,
                    y: 2,
                    color: PixelValue::Rgba([1, 2, 3, 255]),
                    tolerance: 7,
                    gap_close: 2,
                    expected_source: Some(PixelValue::Rgba([9, 8, 7, 255])),
                },
                BatchSeed {
                    enabled: false,
                    x: 3,
                    y: 4,
                    color: PixelValue::Rgba16([257, 514, 771, u16::MAX]),
                    tolerance: 11,
                    gap_close: 1,
                    expected_source: None,
                },
            ]),
        );

        let group = LegacyBatchScriptGroup::from_operation(&core, &operation, "batch_0").unwrap();
        assert_eq!(group.step_count(), 2);
        assert_eq!(group.enabled_step_count(), 1);
        assert_eq!(group.canonical_invocations().unwrap().len(), 1);
        assert_eq!(group.to_operation().unwrap(), operation);

        let mut advanced = group.clone();
        advanced.break_editor_group_for_test();
        assert_eq!(
            advanced.to_operation(),
            Err(LegacyBatchProjectionError::NotProjectable)
        );
    }

    #[test]
    fn exact_depth_pairs_destination_missing_policy_and_native_separation_round_trip() {
        let core = core();
        let operations = [
            operation(
                &core,
                BatchOperationKind::ColorReplace(vec![
                    BatchColorPair {
                        enabled: true,
                        old: PixelValue::Rgba([1, 2, 3, 4]),
                        new: PixelValue::Rgba([5, 6, 7, 8]),
                    },
                    BatchColorPair {
                        enabled: false,
                        old: PixelValue::Rgba16([1, 2, 3, 4]),
                        new: PixelValue::Rgba16([5, 6, 7, 8]),
                    },
                ]),
            ),
            BatchOperation {
                target: Some(color_target(&core, BatchMissingTargetPolicy::Skip)),
                kind: BatchOperationKind::Separation(BatchSeparation {
                    colors: vec![PixelValue::Rgba16([11, 22, 33, u16::MAX])],
                    replacement: PixelValue::Rgba16([44, 55, 66, u16::MAX]),
                    invert: true,
                    destination: BatchSeparationDestination::NativeFile,
                }),
                ..operation(&core, BatchOperationKind::Filter(Filter::SharpenWeak))
            },
        ];

        for (index, operation) in operations.iter().enumerate() {
            let group =
                LegacyBatchScriptGroup::from_operation(&core, operation, &format!("batch_{index}"))
                    .unwrap();
            assert_eq!(group.to_operation().unwrap(), *operation);
        }
    }

    #[test]
    fn disabled_operation_lowers_to_zero_commits_without_losing_rows() {
        let core = core();
        let mut operation = operation(
            &core,
            BatchOperationKind::ColorReplace(vec![BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([1, 1, 1, 255]),
                new: PixelValue::Rgba([2, 2, 2, 255]),
            }]),
        );
        operation.enabled = false;
        operation.configure_each_run = true;
        let group =
            LegacyBatchScriptGroup::from_operation(&core, &operation, "batch_disabled").unwrap();
        assert!(group.canonical_invocations().unwrap().is_empty());
        assert_eq!(group.to_operation().unwrap(), operation);
    }

    #[test]
    fn every_filter_boundary_dust_and_simple_operation_round_trip_without_reregistering_m07() {
        let core = core();
        let mut operations = filters()
            .into_iter()
            .map(|filter| operation(&core, BatchOperationKind::Filter(filter)))
            .collect::<Vec<_>>();
        operations.extend([
            operation(
                &core,
                BatchOperationKind::BoundaryAirbrush(BoundaryAirbrush {
                    colors: vec![[0, 0, 0, u16::MAX], [u16::MAX; 4]],
                    width: 3,
                    strength_milli: 750,
                }),
            ),
            operation(
                &core,
                BatchOperationKind::DustRemoval(DustRemoval {
                    mode: DustMode::RemoveForeground,
                    maximum_pixels: 4,
                }),
            ),
            BatchOperation {
                version: BATCH_OPERATION_VERSION,
                enabled: true,
                configure_each_run: false,
                target: None,
                kind: BatchOperationKind::Mirror(MirrorAxis::Horizontal),
            },
            BatchOperation {
                version: BATCH_OPERATION_VERSION,
                enabled: true,
                configure_each_run: false,
                target: None,
                kind: BatchOperationKind::Rotate90(RotateDirection::Right90),
            },
            BatchOperation {
                version: BATCH_OPERATION_VERSION,
                enabled: true,
                configure_each_run: false,
                target: None,
                kind: BatchOperationKind::Resize(DocumentResize {
                    width: 10,
                    height: 7,
                    dpi_x_milli: 96_000,
                    dpi_y_milli: 120_000,
                    anchor: ResizeAnchor::BottomRight,
                    resample: true,
                }),
            },
            operation(
                &core,
                BatchOperationKind::ConvertPlane {
                    destination_kind: PlaneType::Raster,
                    destination_format: PixelFormat::StraightRgba8,
                },
            ),
        ]);
        for (index, operation) in operations.iter().enumerate() {
            let group = LegacyBatchScriptGroup::from_operation(
                &core,
                operation,
                &format!("catalog_{index}"),
            )
            .unwrap();
            assert_eq!(
                group.to_operation().unwrap(),
                *operation,
                "operation {index}"
            );
        }
    }

    #[test]
    fn line_width_projects_to_existing_canonical_invocation_without_an_m08_catalog_entry() {
        let mut core = core();
        let (_, layer_id) = core
            .create_layer(LayerKind::VectorColoring, "Vector")
            .unwrap();
        let (plane_id, _, _) = core.vector_layer_planes(layer_id).unwrap();
        core.vector_add_path(
            plane_id,
            VectorPathInput {
                segments: vec![VectorCubicSegment {
                    p0: PointF32 { x: 1.0, y: 1.0 },
                    p1: PointF32 { x: 2.0, y: 1.0 },
                    p2: PointF32 { x: 3.0, y: 2.0 },
                    p3: PointF32 { x: 4.0, y: 2.0 },
                    width_start: 1.0,
                    width_end: 2.0,
                }],
                color: PixelValue::Rgba([0, 0, 0, 255]),
                closed: false,
            },
        )
        .unwrap();
        let operation = BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            configure_each_run: false,
            target: Some(BatchTargetSelector {
                layer_id: Some(layer_id),
                plane_id: Some(plane_id),
                layer_kind: Some(LayerKind::VectorColoring),
                plane_kind: Some(PlaneType::VectorMainLine),
                missing_policy: BatchMissingTargetPolicy::Error,
            }),
            kind: BatchOperationKind::LineWidth(VectorWidthMode::Scale(1.5)),
        };
        let group = LegacyBatchScriptGroup::from_operation(&core, &operation, "width").unwrap();
        assert!(matches!(
            group.canonical_invocations().unwrap().as_slice(),
            [CanonicalInvocation::VectorCorrectWidth { path_ids, .. }] if path_ids.len() == 1
        ));
        assert_eq!(group.to_operation().unwrap(), operation);
    }

    #[test]
    fn projected_and_direct_canonical_execution_match_and_failures_are_atomic() {
        let mut direct = painted_core();
        let mut scripted = painted_core();
        let replacement_operation = operation(
            &direct,
            BatchOperationKind::ColorReplace(vec![BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([10, 20, 30, 255]),
                new: PixelValue::Rgba([30, 20, 10, 255]),
            }]),
        );
        let group =
            LegacyBatchScriptGroup::from_operation(&scripted, &replacement_operation, "replace")
                .unwrap();
        apply_operation(&mut direct, &replacement_operation, |_, _| true).unwrap();
        for invocation in group.canonical_invocations().unwrap() {
            scripted.execute_canonical_invocation(invocation).unwrap();
        }
        assert_eq!(digest(&direct), digest(&scripted));
        assert_eq!(direct.current_state, scripted.current_state);
        assert_eq!(direct.document_revision, scripted.document_revision);
        assert_eq!(direct.history_entries(), scripted.history_entries());
        assert_eq!(direct.next_id, scripted.next_id);
        assert_eq!(direct.savepoint, scripted.savepoint);

        let mut unchanged = painted_core();
        let no_op = operation(
            &unchanged,
            BatchOperationKind::ColorReplace(vec![BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([200, 201, 202, 255]),
                new: PixelValue::Rgba([1, 2, 3, 255]),
            }]),
        );
        let group = LegacyBatchScriptGroup::from_operation(&unchanged, &no_op, "no_op").unwrap();
        let before = digest(&unchanged);
        let revision = unchanged.document_revision;
        let history = unchanged.history_entries();
        let next_id = unchanged.next_id;
        for invocation in group.canonical_invocations().unwrap() {
            unchanged.execute_canonical_invocation(invocation).unwrap();
        }
        assert_eq!(digest(&unchanged), before);
        assert_eq!(unchanged.document_revision, revision);
        assert_eq!(unchanged.history_entries(), history);
        assert_eq!(unchanged.next_id, next_id);

        let mut cancelled = painted_core();
        let before = digest(&cancelled);
        let revision = cancelled.document_revision;
        let history = cancelled.history_entries();
        let plane_id = color_target(&cancelled, BatchMissingTargetPolicy::Error)
            .plane_id
            .unwrap();
        assert_eq!(
            apply_color_replacement(
                &mut cancelled,
                plane_id,
                &[BatchColorPair {
                    enabled: true,
                    old: PixelValue::Rgba([10, 20, 30, 255]),
                    new: PixelValue::Rgba([1, 2, 3, 255]),
                }],
                &mut |_, _| false,
            ),
            Err(CoreError::Cancelled)
        );
        assert_eq!(digest(&cancelled), before);
        assert_eq!(cancelled.document_revision, revision);
        assert_eq!(cancelled.history_entries(), history);

        let stale_step = LegacyImageScriptStep::from_canonical(
            &CanonicalInvocation::ApplyFilter {
                plane_id: u64::MAX,
                filter: Filter::SharpenWeak,
            },
            true,
            "stale",
        )
        .unwrap();
        let before = digest(&cancelled);
        let revision = cancelled.document_revision;
        let history = cancelled.history_entries();
        let next_id = cancelled.next_id;
        assert!(
            cancelled
                .execute_canonical_invocation(stale_step.to_canonical().unwrap())
                .is_err()
        );
        assert_eq!(digest(&cancelled), before);
        assert_eq!(cancelled.document_revision, revision);
        assert_eq!(cancelled.history_entries(), history);
        assert_eq!(cancelled.next_id, next_id);

        let invalid = BatchOperation {
            kind: BatchOperationKind::ColorReplace(vec![
                BatchColorPair {
                    enabled: true,
                    old: PixelValue::Rgba([1, 1, 1, 255]),
                    new: PixelValue::Rgba([2, 2, 2, 255]),
                };
                MAX_BATCH_COLOR_PAIRS + 1
            ]),
            ..operation(&cancelled, BatchOperationKind::Filter(Filter::SharpenWeak))
        };
        assert!(matches!(
            LegacyBatchScriptGroup::from_operation(&cancelled, &invalid, "overflow"),
            Err(LegacyBatchProjectionError::InvalidOperation)
        ));
    }

    #[test]
    fn target_missing_ambiguity_and_private_ownership_are_explicit() {
        let mut core = core();
        let missing = BatchOperation {
            target: Some(BatchTargetSelector {
                layer_id: Some(u64::MAX),
                plane_id: Some(u64::MAX),
                layer_kind: None,
                plane_kind: None,
                missing_policy: BatchMissingTargetPolicy::Skip,
            }),
            ..operation(&core, BatchOperationKind::Filter(Filter::SharpenWeak))
        };
        assert!(matches!(
            LegacyBatchScriptGroup::from_operation(&core, &missing, "missing"),
            Err(LegacyBatchProjectionError::MissingTarget)
        ));
        core.create_layer(LayerKind::BinaryColoring, "Second")
            .unwrap();
        let ambiguous = BatchOperation {
            target: Some(BatchTargetSelector::color_plane()),
            ..operation(&core, BatchOperationKind::Filter(Filter::SharpenWeak))
        };
        assert!(matches!(
            LegacyBatchScriptGroup::from_operation(&core, &ambiguous, "ambiguous"),
            Err(LegacyBatchProjectionError::AmbiguousTarget)
        ));

        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<LegacyBatchScriptGroup>();
        assert_send_sync::<LegacyBatchProjectionError>();
    }
}
