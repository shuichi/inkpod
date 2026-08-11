//! Typed canonical invocations for document primitives outside the original kernel slice.

use super::*;
use crate::geometry::{CanonicalGeometry, CanonicalGeometryPoint, CanonicalGeometrySegment};
use crate::history::PixelChange;
use crate::selection::FloatingDestination;
use crate::stroke::DocumentStrokeSample;
use crate::*;
use inkpod_image::{
    CANONICAL_DOCUMENT_ONE, canonical_q16_from_f32, canonical_q16_from_f64,
    canonical_turns_from_degrees_f64, canonical_unit_u16_from_f32,
};
use std::sync::Arc;

const INVOCATION_SCHEMA_VERSION: u16 = 2;
const MAX_CANONICAL_GEOMETRY_BOUNDARY_POINTS: usize = MAX_GEOMETRY_POINTS * 32;
type InvocationApply<'a> = Box<dyn FnOnce(&mut Core) -> Result<InvocationResult, CoreError> + 'a>;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CanonicalInvocation {
    UpdatePaperFrames {
        frames: FrameMetadata,
    },
    CreateLayer {
        kind: LayerKind,
        name: String,
    },
    DuplicateLayer {
        layer_id: u64,
    },
    DeleteLayer {
        layer_id: u64,
    },
    ReorderLayer {
        layer_id: u64,
        destination_index: u64,
    },
    SetLayerProperties {
        layer_id: u64,
        visible: bool,
        editable: bool,
        opacity_milli: u32,
        name: String,
    },
    CreatePlane {
        layer_id: u64,
        kind: PlaneType,
        format: PixelFormat,
        name: String,
    },
    DuplicatePlane {
        plane_id: u64,
    },
    DeletePlane {
        plane_id: u64,
    },
    ReorderPlane {
        plane_id: u64,
        destination_index: u64,
    },
    SetPlaneProperties {
        plane_id: u64,
        visible: bool,
        editable: bool,
        opacity_milli: u32,
        name: String,
    },
    ConvertPlane {
        plane_id: u64,
        destination_kind: PlaneType,
        destination_format: PixelFormat,
    },
    MergePlane {
        plane_id: u64,
    },
    ConvertLayer {
        layer_id: u64,
        destination: LayerKind,
    },
    MergeLayer {
        layer_id: u64,
    },
    DeleteHiddenLayers,
    EditTargets {
        targets: Vec<EditTarget>,
        command: EditTargetCommand,
    },
    AddGuide {
        axis: GuideAxis,
        position: i32,
    },
    MoveGuide {
        guide_id: u64,
        position: i32,
    },
    DeleteGuide {
        guide_id: u64,
    },
    SetGrid {
        grid: GridConfig,
    },
    DeleteAllGuides,
    ApplyFill {
        request: FillRequest,
        target: EditorTarget,
        use_light_table_boundary: bool,
        use_light_table_color: bool,
    },
    ApplyGeometry {
        geometry: CanonicalGeometry,
    },
    ApplyGradient {
        plane_id: u64,
        gradient: Gradient,
    },
    ApplyBoundaryAirbrush {
        plane_id: u64,
        effect: BoundaryAirbrush,
    },
    ApplyBlur {
        plane_id: u64,
        radius: u32,
        strength_milli: u32,
    },
    ApplyAirbrush {
        plane_id: u64,
        stroke: AirbrushStroke,
    },
    ApplyAirbrushGesture {
        plane_id: u64,
        gesture: AirbrushGesture,
    },
    ApplyStamp {
        plane_id: u64,
        stamp: Stamp,
    },
    ApplyStampGesture {
        plane_id: u64,
        gesture: StampGesture,
    },
    ApplyBlurTool {
        plane_id: u64,
        shape: SelectionShape,
        radius: u32,
        strength_milli: u32,
    },
    ApplyBlurPressureTrace {
        plane_id: u64,
        samples: Vec<DocumentStrokeSample>,
        diameter: f32,
        radius: u32,
        strength_milli: u32,
    },
    ApplyDustRemoval {
        plane_id: u64,
        shape: Option<SelectionShape>,
        options: DustRemoval,
    },
    EditPlaneAlpha {
        plane_id: u64,
        alpha: TileRaster,
    },
    ApplyAlphaGradient {
        plane_id: u64,
        gradient: Gradient,
    },
    ApplyFilter {
        plane_id: u64,
        filter: Filter,
    },
    CreateAdjustmentLayer {
        name: String,
        adjustment: Adjustment,
    },
    UpdateAdjustmentLayer {
        layer_id: u64,
        adjustment: Adjustment,
    },
    ReplaceRasterColors {
        plane_id: u64,
        pairs: Vec<BatchColorPair>,
    },
    ScopedColorReplace {
        plane_id: u64,
        mode: ScopedColorReplaceMode,
        target: PixelValue,
        replacement: PixelValue,
        region: Option<SelectionShape>,
    },
    SeparateRasterColors {
        plane_id: u64,
        options: BatchSeparation,
    },
    RestoreSelectedPixels {
        plane_id: u64,
        changes: Vec<PixelChange>,
    },
    ApplySelection {
        shape: SelectionShape,
        operation: SelectionOperation,
        interpretation: RangeInterpretation,
        options: SelectionConstructionOptions,
        target: EditorTarget,
    },
    InvertSelection,
    ClearSelection,
    ResizeSelection {
        pixels: i32,
    },
    SelectColor {
        color: PixelValue,
        tolerance: u16,
        different: bool,
        operation: SelectionOperation,
        target: EditorTarget,
    },
    SelectionToLayer {
        name: String,
    },
    SelectionFromLayer {
        layer_id: u64,
        operation: SelectionLayerOperation,
    },
    ClearSelectedContent {
        target: EditorTarget,
    },
    CommitFloating {
        floating: FloatingSelection,
    },
    MirrorDocument {
        axis: MirrorAxis,
    },
    RotateDocument {
        direction: RotateDirection,
    },
    ResizeDocument {
        resize: DocumentResize,
    },
    VectorAddPath {
        plane_id: u64,
        input: VectorPathInput,
    },
    VectorAddFill {
        plane_id: u64,
        boundary_path_ids: Vec<u64>,
        color: PixelValue,
    },
    VectorErase {
        plane_id: u64,
        point: PointF32,
        radius: f32,
        mode: VectorEraseMode,
    },
    VectorConnect {
        plane_id: u64,
        maximum_gap: f32,
    },
    VectorCorrectWidth {
        path_ids: Vec<u64>,
        mode: VectorWidthMode,
    },
    RasterizeVectorLayer {
        layer_id: u64,
        antialias: bool,
        name: String,
    },
    VectorizeRasterPlane {
        source_plane_id: u64,
        target_vector_layer_id: u64,
        alpha_threshold: u8,
    },
    VectorizeRasterPlaneIntoNewLayer {
        source_plane_id: u64,
        alpha_threshold: u8,
        name: String,
    },
    LightTableSetGlobalOpacity {
        opacity_milli: u32,
    },
    LightTableCreateSet {
        name: String,
    },
    LightTableDuplicateSet {
        set_id: u64,
    },
    LightTableDeleteSet {
        set_id: u64,
    },
    LightTableRenameSet {
        set_id: u64,
        name: String,
    },
    LightTableReorderSet {
        set_id: u64,
        destination_index: u64,
    },
    LightTableSetActive {
        set_id: u64,
    },
    LightTableAddItem {
        input: LightTableItemInput,
    },
    LightTableUpdateItemProperties {
        item_id: u64,
        properties: LightTableItemProperties,
    },
    LightTableUpdateItem {
        item_id: u64,
        input: LightTableItemInput,
    },
    LightTableRemoveItem {
        item_id: u64,
    },
    LightTableReorderItem {
        item_id: u64,
        destination_index: u64,
    },
    LightTableBulkRegister {
        target_set_id: u64,
        inputs: Vec<LightTableItemInput>,
    },
}

fn canonical_f32_q16(value: f32) -> Result<f32, CoreError> {
    let fixed = canonical_q16_from_f32(value).ok_or(CoreError::InvalidArgument(
        "canonical binary32 scalar is not representable",
    ))?;
    Ok(fixed as f32 / CANONICAL_DOCUMENT_ONE as f32)
}

fn canonical_f64_q16(value: f64) -> Result<f64, CoreError> {
    let fixed = canonical_q16_from_f64(value).ok_or(CoreError::InvalidArgument(
        "canonical binary64 scalar is not representable",
    ))?;
    Ok(fixed as f64 / CANONICAL_DOCUMENT_ONE as f64)
}

fn canonical_point(point: PointF32) -> Result<PointF32, CoreError> {
    Ok(PointF32 {
        x: canonical_f32_q16(point.x)?,
        y: canonical_f32_q16(point.y)?,
    })
}

fn canonical_document_stroke_sample(
    sample: DocumentStrokeSample,
) -> Result<DocumentStrokeSample, CoreError> {
    let pressure = canonical_unit_u16_from_f32(sample.pressure).ok_or(
        CoreError::InvalidArgument("canonical pressure is outside the unit interval"),
    )?;
    Ok(DocumentStrokeSample {
        point: DocumentPointF32::new(
            canonical_f32_q16(sample.point.x)?,
            canonical_f32_q16(sample.point.y)?,
        )?,
        pressure: f32::from(pressure) / f32::from(u16::MAX),
    })
}

fn canonical_selection_shape(shape: SelectionShape) -> Result<SelectionShape, CoreError> {
    Ok(match shape {
        SelectionShape::Rectangle(_) | SelectionShape::Ellipse(_) | SelectionShape::Wand { .. } => {
            shape
        }
        SelectionShape::RectangleGesture { anchor, current } => SelectionShape::RectangleGesture {
            anchor: canonical_point(anchor)?,
            current: canonical_point(current)?,
        },
        SelectionShape::EllipseGesture { anchor, current } => SelectionShape::EllipseGesture {
            anchor: canonical_point(anchor)?,
            current: canonical_point(current)?,
        },
        SelectionShape::Lasso(points) => SelectionShape::Lasso(
            points
                .into_iter()
                .map(canonical_point)
                .collect::<Result<_, _>>()?,
        ),
        SelectionShape::Polyline(points) => SelectionShape::Polyline(
            points
                .into_iter()
                .map(canonical_point)
                .collect::<Result<_, _>>()?,
        ),
        SelectionShape::Trace { points, diameter } => SelectionShape::Trace {
            points: points
                .into_iter()
                .map(canonical_point)
                .collect::<Result<_, _>>()?,
            diameter: canonical_f32_q16(diameter)?,
        },
        SelectionShape::TraceBrush { samples, diameter } => SelectionShape::TraceBrush {
            samples: samples
                .into_iter()
                .map(|sample| {
                    let pressure = canonical_unit_u16_from_f32(sample.pressure).ok_or(
                        CoreError::InvalidArgument(
                            "canonical selection pressure is outside the unit interval",
                        ),
                    )?;
                    Ok(SelectionSample {
                        x: canonical_f32_q16(sample.x)?,
                        y: canonical_f32_q16(sample.y)?,
                        pressure: f32::from(pressure) / f32::from(u16::MAX),
                    })
                })
                .collect::<Result<_, CoreError>>()?,
            diameter: canonical_f32_q16(diameter)?,
        },
    })
}

fn canonical_selection_options(
    options: SelectionConstructionOptions,
) -> Result<SelectionConstructionOptions, CoreError> {
    if options.aspect_ratio_q16 > (4_096_u32 << 16)
        || options.trace.view_zoom_q16 <= 0
        || options.trace.view_zoom_q16 > (64_i64 << 16)
    {
        return Err(CoreError::InvalidArgument(
            "selection construction options are outside their bounds",
        ));
    }
    Ok(options)
}

fn canonical_vector_width_mode(mode: VectorWidthMode) -> Result<VectorWidthMode, CoreError> {
    Ok(match mode {
        VectorWidthMode::Add(value) => VectorWidthMode::Add(canonical_f32_q16(value)?),
        VectorWidthMode::Subtract(value) => VectorWidthMode::Subtract(canonical_f32_q16(value)?),
        VectorWidthMode::Scale(value) => VectorWidthMode::Scale(canonical_f32_q16(value)?),
        VectorWidthMode::Constant(value) => VectorWidthMode::Constant(canonical_f32_q16(value)?),
    })
}

#[derive(Clone, Debug)]
pub(crate) struct RuntimeInvocation {
    invocation: Arc<CanonicalInvocation>,
    arguments: Arc<[u8]>,
}

impl PartialEq for RuntimeInvocation {
    fn eq(&self, other: &Self) -> bool {
        self.invocation.primitive_id() == other.invocation.primitive_id()
            && self.arguments == other.arguments
    }
}

impl Eq for RuntimeInvocation {}

impl RuntimeInvocation {
    fn new(invocation: CanonicalInvocation) -> Result<Self, CoreError> {
        let arguments = invocation.canonical_arguments()?.into();
        Ok(Self {
            invocation: Arc::new(invocation),
            arguments,
        })
    }

    pub(super) fn invocation(&self) -> &CanonicalInvocation {
        &self.invocation
    }

    pub(super) fn arguments(&self) -> &[u8] {
        &self.arguments
    }

    pub(crate) fn from_persistent(
        primitive_id: PrimitiveId,
        schema_version: u16,
        arguments: &[u8],
        assets: &crate::asset::AssetStore,
    ) -> Result<Option<Self>, CoreError> {
        if matches!(
            primitive_id,
            PrimitiveId::SET_MAIN_LINE_COLOR
                | PrimitiveId::REPLACE_PALETTE
                | PrimitiveId::REPLACE_COLOR_CHART
                | PrimitiveId::APPLY_RASTER_STROKE
                | PrimitiveId::IMPORT_RASTER_ASSET
        ) {
            return Ok(None);
        }
        if schema_version != INVOCATION_SCHEMA_VERSION {
            return Err(CoreError::Format(
                "persisted primitive invocation schema is unsupported".to_owned(),
            ));
        }
        let invocation = decode_persistent_invocation(primitive_id, arguments, assets)?;
        let runtime = Self::new(invocation.canonicalized()?)?;
        if runtime.arguments() != arguments {
            return Err(CoreError::Format(
                "persisted primitive invocation is not canonical".to_owned(),
            ));
        }
        Ok(Some(runtime))
    }
}

fn decode_persistent_invocation(
    primitive_id: PrimitiveId,
    arguments: &[u8],
    assets: &crate::asset::AssetStore,
) -> Result<CanonicalInvocation, CoreError> {
    let mut reader = CanonicalReader::new(arguments);
    if reader.u32()? != primitive_id.get() {
        return Err(CoreError::Format(
            "persisted primitive ID does not match its canonical arguments".to_owned(),
        ));
    }
    let invocation = if primitive_id == PrimitiveId::UPDATE_PAPER_FRAMES {
        CanonicalInvocation::UpdatePaperFrames {
            frames: reader.frames()?,
        }
    } else if primitive_id == PrimitiveId::CREATE_LAYER {
        CanonicalInvocation::CreateLayer {
            kind: reader.layer_kind()?,
            name: reader.string()?,
        }
    } else if primitive_id == PrimitiveId::DUPLICATE_LAYER {
        CanonicalInvocation::DuplicateLayer {
            layer_id: reader.u64()?,
        }
    } else if primitive_id == PrimitiveId::DELETE_LAYER {
        CanonicalInvocation::DeleteLayer {
            layer_id: reader.u64()?,
        }
    } else if primitive_id == PrimitiveId::REORDER_LAYER {
        CanonicalInvocation::ReorderLayer {
            layer_id: reader.u64()?,
            destination_index: reader.u64()?,
        }
    } else if primitive_id == PrimitiveId::SET_LAYER_PROPERTIES {
        CanonicalInvocation::SetLayerProperties {
            layer_id: reader.u64()?,
            visible: reader.boolean()?,
            editable: reader.boolean()?,
            opacity_milli: reader.u32()?,
            name: reader.string()?,
        }
    } else if primitive_id == PrimitiveId::CREATE_PLANE {
        CanonicalInvocation::CreatePlane {
            layer_id: reader.u64()?,
            kind: reader.plane_type()?,
            format: reader.pixel_format()?,
            name: reader.string()?,
        }
    } else if primitive_id == PrimitiveId::DUPLICATE_PLANE {
        CanonicalInvocation::DuplicatePlane {
            plane_id: reader.u64()?,
        }
    } else if primitive_id == PrimitiveId::DELETE_PLANE {
        CanonicalInvocation::DeletePlane {
            plane_id: reader.u64()?,
        }
    } else if primitive_id == PrimitiveId::REORDER_PLANE {
        CanonicalInvocation::ReorderPlane {
            plane_id: reader.u64()?,
            destination_index: reader.u64()?,
        }
    } else if primitive_id == PrimitiveId::SET_PLANE_PROPERTIES {
        CanonicalInvocation::SetPlaneProperties {
            plane_id: reader.u64()?,
            visible: reader.boolean()?,
            editable: reader.boolean()?,
            opacity_milli: reader.u32()?,
            name: reader.string()?,
        }
    } else if primitive_id == PrimitiveId::CONVERT_PLANE {
        CanonicalInvocation::ConvertPlane {
            plane_id: reader.u64()?,
            destination_kind: reader.plane_type()?,
            destination_format: reader.pixel_format()?,
        }
    } else if primitive_id == PrimitiveId::MERGE_PLANE {
        CanonicalInvocation::MergePlane {
            plane_id: reader.u64()?,
        }
    } else if primitive_id == PrimitiveId::CONVERT_LAYER {
        CanonicalInvocation::ConvertLayer {
            layer_id: reader.u64()?,
            destination: reader.layer_kind()?,
        }
    } else if primitive_id == PrimitiveId::MERGE_LAYER {
        CanonicalInvocation::MergeLayer {
            layer_id: reader.u64()?,
        }
    } else if primitive_id == PrimitiveId::DELETE_HIDDEN_LAYERS {
        CanonicalInvocation::DeleteHiddenLayers
    } else if primitive_id == PrimitiveId::EDIT_TARGETS {
        let count = usize::try_from(reader.u32()?)
            .map_err(|_| CoreError::Format("edit-target count is not representable".to_owned()))?;
        if count == 0 || count > MAX_EDIT_TARGETS {
            return Err(CoreError::Format(
                "invalid canonical edit-target count".to_owned(),
            ));
        }
        let mut targets = Vec::with_capacity(count);
        for _ in 0..count {
            targets.push(match reader.u32()? {
                1 => EditTarget::Layer(reader.u64()?),
                2 => EditTarget::Plane(EditorTarget {
                    layer_id: reader.u64()?,
                    plane_id: reader.u64()?,
                }),
                _ => {
                    return Err(CoreError::Format(
                        "unknown canonical edit-target kind".to_owned(),
                    ));
                }
            });
        }
        CanonicalInvocation::EditTargets {
            targets,
            command: read_edit_target_command(&mut reader)?,
        }
    } else if primitive_id == PrimitiveId::ADD_GUIDE {
        CanonicalInvocation::AddGuide {
            axis: reader.guide_axis()?,
            position: reader.i32()?,
        }
    } else if primitive_id == PrimitiveId::MOVE_GUIDE {
        CanonicalInvocation::MoveGuide {
            guide_id: reader.u64()?,
            position: reader.i32()?,
        }
    } else if primitive_id == PrimitiveId::DELETE_GUIDE {
        CanonicalInvocation::DeleteGuide {
            guide_id: reader.u64()?,
        }
    } else if primitive_id == PrimitiveId::SET_GRID {
        CanonicalInvocation::SetGrid {
            grid: reader.grid()?,
        }
    } else if primitive_id == PrimitiveId::DELETE_ALL_GUIDES {
        CanonicalInvocation::DeleteAllGuides
    } else if primitive_id == PrimitiveId::APPLY_FILL {
        CanonicalInvocation::ApplyFill {
            request: reader.fill_request()?,
            target: reader.editor_target()?,
            use_light_table_boundary: reader.boolean()?,
            use_light_table_color: reader.boolean()?,
        }
    } else if primitive_id == PrimitiveId::APPLY_GEOMETRY {
        CanonicalInvocation::ApplyGeometry {
            geometry: reader.geometry()?,
        }
    } else if primitive_id == PrimitiveId::APPLY_GRADIENT {
        CanonicalInvocation::ApplyGradient {
            plane_id: reader.u64()?,
            gradient: reader.gradient()?,
        }
    } else if primitive_id == PrimitiveId::APPLY_BOUNDARY_AIRBRUSH {
        CanonicalInvocation::ApplyBoundaryAirbrush {
            plane_id: reader.u64()?,
            effect: reader.boundary_airbrush()?,
        }
    } else if primitive_id == PrimitiveId::APPLY_BLUR {
        CanonicalInvocation::ApplyBlur {
            plane_id: reader.u64()?,
            radius: reader.u32()?,
            strength_milli: reader.u32()?,
        }
    } else if primitive_id == PrimitiveId::APPLY_AIRBRUSH {
        CanonicalInvocation::ApplyAirbrush {
            plane_id: reader.u64()?,
            stroke: reader.airbrush_stroke()?,
        }
    } else if primitive_id == PrimitiveId::APPLY_AIRBRUSH_GESTURE {
        CanonicalInvocation::ApplyAirbrushGesture {
            plane_id: reader.u64()?,
            gesture: reader.airbrush_gesture()?,
        }
    } else if primitive_id == PrimitiveId::APPLY_STAMP {
        CanonicalInvocation::ApplyStamp {
            plane_id: reader.u64()?,
            stamp: reader.stamp()?,
        }
    } else if primitive_id == PrimitiveId::APPLY_STAMP_GESTURE {
        CanonicalInvocation::ApplyStampGesture {
            plane_id: reader.u64()?,
            gesture: reader.stamp_gesture()?,
        }
    } else if primitive_id == PrimitiveId::APPLY_BLUR_TOOL {
        let plane_id = reader.u64()?;
        match reader.u32()? {
            1 => CanonicalInvocation::ApplyBlurTool {
                plane_id,
                shape: reader.selection_shape()?,
                radius: reader.u32()?,
                strength_milli: reader.u32()?,
            },
            2 => CanonicalInvocation::ApplyBlurPressureTrace {
                plane_id,
                samples: reader.stroke_samples()?,
                diameter: reader.q16_f32()?,
                radius: reader.u32()?,
                strength_milli: reader.u32()?,
            },
            _ => return Err(reader.invalid("blur-tool invocation kind is invalid")),
        }
    } else if primitive_id == PrimitiveId::APPLY_DUST_REMOVAL {
        let plane_id = reader.u64()?;
        let has_shape = reader.boolean()?;
        CanonicalInvocation::ApplyDustRemoval {
            plane_id,
            shape: has_shape.then(|| reader.selection_shape()).transpose()?,
            options: reader.dust_removal()?,
        }
    } else if primitive_id == PrimitiveId::EDIT_PLANE_ALPHA {
        CanonicalInvocation::EditPlaneAlpha {
            plane_id: reader.u64()?,
            alpha: reader.raster()?,
        }
    } else if primitive_id == PrimitiveId::APPLY_ALPHA_GRADIENT {
        CanonicalInvocation::ApplyAlphaGradient {
            plane_id: reader.u64()?,
            gradient: reader.gradient()?,
        }
    } else if primitive_id == PrimitiveId::APPLY_FILTER {
        CanonicalInvocation::ApplyFilter {
            plane_id: reader.u64()?,
            filter: reader.filter()?,
        }
    } else if primitive_id == PrimitiveId::CREATE_ADJUSTMENT_LAYER {
        CanonicalInvocation::CreateAdjustmentLayer {
            name: reader.string()?,
            adjustment: reader.adjustment()?,
        }
    } else if primitive_id == PrimitiveId::UPDATE_ADJUSTMENT_LAYER {
        CanonicalInvocation::UpdateAdjustmentLayer {
            layer_id: reader.u64()?,
            adjustment: reader.adjustment()?,
        }
    } else if primitive_id == PrimitiveId::REPLACE_RASTER_COLORS {
        CanonicalInvocation::ReplaceRasterColors {
            plane_id: reader.u64()?,
            pairs: reader.batch_color_pairs()?,
        }
    } else if primitive_id == PrimitiveId::SCOPED_COLOR_REPLACE {
        CanonicalInvocation::ScopedColorReplace {
            plane_id: reader.u64()?,
            mode: scoped_color_replace_mode(reader.u32()?)?,
            target: reader.pixel()?,
            replacement: reader.pixel()?,
            region: reader
                .boolean()?
                .then(|| reader.selection_shape())
                .transpose()?,
        }
    } else if primitive_id == PrimitiveId::SEPARATE_RASTER_COLORS {
        CanonicalInvocation::SeparateRasterColors {
            plane_id: reader.u64()?,
            options: reader.batch_separation()?,
        }
    } else if primitive_id == PrimitiveId::RESTORE_SELECTED_PIXELS {
        CanonicalInvocation::RestoreSelectedPixels {
            plane_id: reader.u64()?,
            changes: reader.pixel_changes()?,
        }
    } else if primitive_id == PrimitiveId::APPLY_SELECTION {
        CanonicalInvocation::ApplySelection {
            shape: reader.selection_shape()?,
            operation: reader.selection_operation()?,
            interpretation: reader.range_interpretation()?,
            options: reader.selection_construction_options()?,
            target: reader.editor_target()?,
        }
    } else if primitive_id == PrimitiveId::INVERT_SELECTION {
        CanonicalInvocation::InvertSelection
    } else if primitive_id == PrimitiveId::CLEAR_SELECTION {
        CanonicalInvocation::ClearSelection
    } else if primitive_id == PrimitiveId::RESIZE_SELECTION {
        CanonicalInvocation::ResizeSelection {
            pixels: reader.i32()?,
        }
    } else if primitive_id == PrimitiveId::SELECT_COLOR {
        CanonicalInvocation::SelectColor {
            color: reader.pixel()?,
            tolerance: reader.u16()?,
            different: reader.boolean()?,
            operation: reader.selection_operation()?,
            target: reader.editor_target()?,
        }
    } else if primitive_id == PrimitiveId::SELECTION_TO_LAYER {
        CanonicalInvocation::SelectionToLayer {
            name: reader.string()?,
        }
    } else if primitive_id == PrimitiveId::SELECTION_FROM_LAYER {
        CanonicalInvocation::SelectionFromLayer {
            layer_id: reader.u64()?,
            operation: reader.selection_layer_operation()?,
        }
    } else if primitive_id == PrimitiveId::CLEAR_SELECTED_CONTENT {
        CanonicalInvocation::ClearSelectedContent {
            target: reader.editor_target()?,
        }
    } else if primitive_id == PrimitiveId::COMMIT_FLOATING {
        CanonicalInvocation::CommitFloating {
            floating: reader.floating()?,
        }
    } else if primitive_id == PrimitiveId::MIRROR_DOCUMENT {
        CanonicalInvocation::MirrorDocument {
            axis: reader.mirror_axis()?,
        }
    } else if primitive_id == PrimitiveId::ROTATE_DOCUMENT {
        CanonicalInvocation::RotateDocument {
            direction: reader.rotate_direction()?,
        }
    } else if primitive_id == PrimitiveId::RESIZE_DOCUMENT {
        CanonicalInvocation::ResizeDocument {
            resize: reader.document_resize()?,
        }
    } else if primitive_id == PrimitiveId::VECTOR_ADD_PATH {
        CanonicalInvocation::VectorAddPath {
            plane_id: reader.u64()?,
            input: reader.vector_path()?,
        }
    } else if primitive_id == PrimitiveId::VECTOR_ADD_FILL {
        CanonicalInvocation::VectorAddFill {
            plane_id: reader.u64()?,
            boundary_path_ids: reader.ids()?,
            color: reader.pixel()?,
        }
    } else if primitive_id == PrimitiveId::VECTOR_ERASE {
        CanonicalInvocation::VectorErase {
            plane_id: reader.u64()?,
            point: reader.point()?,
            radius: reader.q16_f32()?,
            mode: reader.vector_erase_mode()?,
        }
    } else if primitive_id == PrimitiveId::VECTOR_CONNECT {
        CanonicalInvocation::VectorConnect {
            plane_id: reader.u64()?,
            maximum_gap: reader.q16_f32()?,
        }
    } else if primitive_id == PrimitiveId::VECTOR_CORRECT_WIDTH {
        CanonicalInvocation::VectorCorrectWidth {
            path_ids: reader.ids()?,
            mode: reader.vector_width_mode()?,
        }
    } else if primitive_id == PrimitiveId::RASTERIZE_VECTOR_LAYER {
        CanonicalInvocation::RasterizeVectorLayer {
            layer_id: reader.u64()?,
            antialias: reader.boolean()?,
            name: reader.string()?,
        }
    } else if primitive_id == PrimitiveId::VECTORIZE_RASTER_PLANE {
        CanonicalInvocation::VectorizeRasterPlane {
            source_plane_id: reader.u64()?,
            target_vector_layer_id: reader.u64()?,
            alpha_threshold: reader.u8()?,
        }
    } else if primitive_id == PrimitiveId::VECTORIZE_RASTER_PLANE_INTO_NEW_LAYER {
        CanonicalInvocation::VectorizeRasterPlaneIntoNewLayer {
            source_plane_id: reader.u64()?,
            alpha_threshold: reader.u8()?,
            name: reader.string()?,
        }
    } else if primitive_id == PrimitiveId::LIGHT_TABLE_SET_GLOBAL_OPACITY {
        CanonicalInvocation::LightTableSetGlobalOpacity {
            opacity_milli: reader.u32()?,
        }
    } else if primitive_id == PrimitiveId::LIGHT_TABLE_CREATE_SET {
        CanonicalInvocation::LightTableCreateSet {
            name: reader.string()?,
        }
    } else if primitive_id == PrimitiveId::LIGHT_TABLE_DUPLICATE_SET {
        CanonicalInvocation::LightTableDuplicateSet {
            set_id: reader.u64()?,
        }
    } else if primitive_id == PrimitiveId::LIGHT_TABLE_DELETE_SET {
        CanonicalInvocation::LightTableDeleteSet {
            set_id: reader.u64()?,
        }
    } else if primitive_id == PrimitiveId::LIGHT_TABLE_RENAME_SET {
        CanonicalInvocation::LightTableRenameSet {
            set_id: reader.u64()?,
            name: reader.string()?,
        }
    } else if primitive_id == PrimitiveId::LIGHT_TABLE_REORDER_SET {
        CanonicalInvocation::LightTableReorderSet {
            set_id: reader.u64()?,
            destination_index: reader.u64()?,
        }
    } else if primitive_id == PrimitiveId::LIGHT_TABLE_SET_ACTIVE {
        CanonicalInvocation::LightTableSetActive {
            set_id: reader.u64()?,
        }
    } else if primitive_id == PrimitiveId::LIGHT_TABLE_ADD_ITEM {
        CanonicalInvocation::LightTableAddItem {
            input: reader.light_table_item(assets)?,
        }
    } else if primitive_id == PrimitiveId::LIGHT_TABLE_UPDATE_ITEM_PROPERTIES {
        CanonicalInvocation::LightTableUpdateItemProperties {
            item_id: reader.u64()?,
            properties: reader.light_table_properties()?,
        }
    } else if primitive_id == PrimitiveId::LIGHT_TABLE_UPDATE_ITEM {
        CanonicalInvocation::LightTableUpdateItem {
            item_id: reader.u64()?,
            input: reader.light_table_item(assets)?,
        }
    } else if primitive_id == PrimitiveId::LIGHT_TABLE_REMOVE_ITEM {
        CanonicalInvocation::LightTableRemoveItem {
            item_id: reader.u64()?,
        }
    } else if primitive_id == PrimitiveId::LIGHT_TABLE_REORDER_ITEM {
        CanonicalInvocation::LightTableReorderItem {
            item_id: reader.u64()?,
            destination_index: reader.u64()?,
        }
    } else if primitive_id == PrimitiveId::LIGHT_TABLE_BULK_REGISTER {
        let target_set_id = reader.u64()?;
        let count = usize::try_from(reader.u32()?).map_err(|_| {
            CoreError::Format("light-table bulk item count is not representable".to_owned())
        })?;
        if count == 0 || count > crate::animation::MAX_LIGHT_TABLE_ITEMS {
            return Err(CoreError::Format(
                "canonical light-table bulk item count is outside bounds".to_owned(),
            ));
        }
        let mut inputs = Vec::with_capacity(count);
        for _ in 0..count {
            inputs.push(reader.light_table_item(assets)?);
        }
        CanonicalInvocation::LightTableBulkRegister {
            target_set_id,
            inputs,
        }
    } else {
        return Err(reader.invalid("persisted primitive invocation is unsupported"));
    };
    reader.finish()?;
    Ok(invocation)
}

#[derive(Clone, Debug)]
pub(crate) struct InvocationResult {
    pub(crate) dispatch: DispatchOutcome,
    pub(crate) output_ids: Vec<u64>,
    pub(crate) changed_pixels: u64,
}

impl InvocationResult {
    pub(crate) fn dispatch(dispatch: DispatchOutcome) -> Self {
        Self {
            dispatch,
            output_ids: Vec::new(),
            changed_pixels: 0,
        }
    }

    pub(crate) fn output(dispatch: DispatchOutcome, output_id: u64) -> Self {
        Self {
            dispatch,
            output_ids: vec![output_id],
            changed_pixels: 0,
        }
    }

    pub(crate) fn outputs(dispatch: DispatchOutcome, output_ids: Vec<u64>) -> Self {
        Self {
            dispatch,
            output_ids,
            changed_pixels: 0,
        }
    }

    pub(crate) fn fill(outcome: FillOutcome) -> Self {
        Self {
            dispatch: outcome.dispatch,
            output_ids: Vec::new(),
            changed_pixels: outcome.changed_pixels,
        }
    }
}

impl CanonicalInvocation {
    fn canonicalized(self) -> Result<Self, CoreError> {
        match self {
            Self::ApplyBlurTool {
                plane_id,
                shape,
                radius,
                strength_milli,
            } => Ok(Self::ApplyBlurTool {
                plane_id,
                shape: canonical_selection_shape(shape)?,
                radius,
                strength_milli,
            }),
            Self::ApplyBlurPressureTrace {
                plane_id,
                samples,
                diameter,
                radius,
                strength_milli,
            } => Ok(Self::ApplyBlurPressureTrace {
                plane_id,
                samples: samples
                    .into_iter()
                    .map(canonical_document_stroke_sample)
                    .collect::<Result<_, _>>()?,
                diameter: canonical_f32_q16(diameter)?,
                radius,
                strength_milli,
            }),
            Self::ApplyDustRemoval {
                plane_id,
                shape,
                options,
            } => Ok(Self::ApplyDustRemoval {
                plane_id,
                shape: shape.map(canonical_selection_shape).transpose()?,
                options,
            }),
            Self::ScopedColorReplace {
                plane_id,
                mode,
                target,
                replacement,
                region,
            } => Ok(Self::ScopedColorReplace {
                plane_id,
                mode,
                target,
                replacement,
                region: region.map(canonical_selection_shape).transpose()?,
            }),
            Self::ApplySelection {
                shape,
                operation,
                interpretation,
                options,
                target,
            } => Ok(Self::ApplySelection {
                shape: canonical_selection_shape(shape)?,
                operation,
                interpretation,
                options: canonical_selection_options(options)?,
                target,
            }),
            Self::CommitFloating { mut floating } => {
                floating.transform.translate_x = canonical_f64_q16(floating.transform.translate_x)?;
                floating.transform.translate_y = canonical_f64_q16(floating.transform.translate_y)?;
                floating.transform.scale_x = canonical_f64_q16(floating.transform.scale_x)?;
                floating.transform.scale_y = canonical_f64_q16(floating.transform.scale_y)?;
                let turns = canonical_turns_from_degrees_f64(floating.transform.rotation_degrees)
                    .ok_or(CoreError::InvalidArgument(
                    "floating rotation is not finite",
                ))?;
                floating.transform.rotation_degrees = f64::from(turns) * 360.0 / 4_294_967_296.0;
                Ok(Self::CommitFloating { floating })
            }
            Self::VectorAddPath {
                plane_id,
                mut input,
            } => {
                for segment in &mut input.segments {
                    segment.p0 = canonical_point(segment.p0)?;
                    segment.p1 = canonical_point(segment.p1)?;
                    segment.p2 = canonical_point(segment.p2)?;
                    segment.p3 = canonical_point(segment.p3)?;
                    segment.width_start = canonical_f32_q16(segment.width_start)?;
                    segment.width_end = canonical_f32_q16(segment.width_end)?;
                }
                Ok(Self::VectorAddPath { plane_id, input })
            }
            Self::VectorErase {
                plane_id,
                point,
                radius,
                mode,
            } => Ok(Self::VectorErase {
                plane_id,
                point: canonical_point(point)?,
                radius: canonical_f32_q16(radius)?,
                mode,
            }),
            Self::VectorConnect {
                plane_id,
                maximum_gap,
            } => Ok(Self::VectorConnect {
                plane_id,
                maximum_gap: canonical_f32_q16(maximum_gap)?,
            }),
            Self::VectorCorrectWidth { path_ids, mode } => Ok(Self::VectorCorrectWidth {
                path_ids,
                mode: canonical_vector_width_mode(mode)?,
            }),
            other => Ok(other),
        }
    }

    pub(super) const fn primitive_id(&self) -> PrimitiveId {
        match self {
            Self::UpdatePaperFrames { .. } => PrimitiveId::UPDATE_PAPER_FRAMES,
            Self::CreateLayer { .. } => PrimitiveId::CREATE_LAYER,
            Self::DuplicateLayer { .. } => PrimitiveId::DUPLICATE_LAYER,
            Self::DeleteLayer { .. } => PrimitiveId::DELETE_LAYER,
            Self::ReorderLayer { .. } => PrimitiveId::REORDER_LAYER,
            Self::SetLayerProperties { .. } => PrimitiveId::SET_LAYER_PROPERTIES,
            Self::CreatePlane { .. } => PrimitiveId::CREATE_PLANE,
            Self::DuplicatePlane { .. } => PrimitiveId::DUPLICATE_PLANE,
            Self::DeletePlane { .. } => PrimitiveId::DELETE_PLANE,
            Self::ReorderPlane { .. } => PrimitiveId::REORDER_PLANE,
            Self::SetPlaneProperties { .. } => PrimitiveId::SET_PLANE_PROPERTIES,
            Self::ConvertPlane { .. } => PrimitiveId::CONVERT_PLANE,
            Self::MergePlane { .. } => PrimitiveId::MERGE_PLANE,
            Self::ConvertLayer { .. } => PrimitiveId::CONVERT_LAYER,
            Self::MergeLayer { .. } => PrimitiveId::MERGE_LAYER,
            Self::DeleteHiddenLayers => PrimitiveId::DELETE_HIDDEN_LAYERS,
            Self::EditTargets { .. } => PrimitiveId::EDIT_TARGETS,
            Self::AddGuide { .. } => PrimitiveId::ADD_GUIDE,
            Self::MoveGuide { .. } => PrimitiveId::MOVE_GUIDE,
            Self::DeleteGuide { .. } => PrimitiveId::DELETE_GUIDE,
            Self::SetGrid { .. } => PrimitiveId::SET_GRID,
            Self::DeleteAllGuides => PrimitiveId::DELETE_ALL_GUIDES,
            Self::ApplyFill { .. } => PrimitiveId::APPLY_FILL,
            Self::ApplyGeometry { .. } => PrimitiveId::APPLY_GEOMETRY,
            Self::ApplyGradient { .. } => PrimitiveId::APPLY_GRADIENT,
            Self::ApplyBoundaryAirbrush { .. } => PrimitiveId::APPLY_BOUNDARY_AIRBRUSH,
            Self::ApplyBlur { .. } => PrimitiveId::APPLY_BLUR,
            Self::ApplyAirbrush { .. } => PrimitiveId::APPLY_AIRBRUSH,
            Self::ApplyAirbrushGesture { .. } => PrimitiveId::APPLY_AIRBRUSH_GESTURE,
            Self::ApplyStamp { .. } => PrimitiveId::APPLY_STAMP,
            Self::ApplyStampGesture { .. } => PrimitiveId::APPLY_STAMP_GESTURE,
            Self::ApplyBlurTool { .. } | Self::ApplyBlurPressureTrace { .. } => {
                PrimitiveId::APPLY_BLUR_TOOL
            }
            Self::ApplyDustRemoval { .. } => PrimitiveId::APPLY_DUST_REMOVAL,
            Self::EditPlaneAlpha { .. } => PrimitiveId::EDIT_PLANE_ALPHA,
            Self::ApplyAlphaGradient { .. } => PrimitiveId::APPLY_ALPHA_GRADIENT,
            Self::ApplyFilter { .. } => PrimitiveId::APPLY_FILTER,
            Self::CreateAdjustmentLayer { .. } => PrimitiveId::CREATE_ADJUSTMENT_LAYER,
            Self::UpdateAdjustmentLayer { .. } => PrimitiveId::UPDATE_ADJUSTMENT_LAYER,
            Self::ReplaceRasterColors { .. } => PrimitiveId::REPLACE_RASTER_COLORS,
            Self::ScopedColorReplace { .. } => PrimitiveId::SCOPED_COLOR_REPLACE,
            Self::SeparateRasterColors { .. } => PrimitiveId::SEPARATE_RASTER_COLORS,
            Self::RestoreSelectedPixels { .. } => PrimitiveId::RESTORE_SELECTED_PIXELS,
            Self::ApplySelection { .. } => PrimitiveId::APPLY_SELECTION,
            Self::InvertSelection => PrimitiveId::INVERT_SELECTION,
            Self::ClearSelection => PrimitiveId::CLEAR_SELECTION,
            Self::ResizeSelection { .. } => PrimitiveId::RESIZE_SELECTION,
            Self::SelectColor { .. } => PrimitiveId::SELECT_COLOR,
            Self::SelectionToLayer { .. } => PrimitiveId::SELECTION_TO_LAYER,
            Self::SelectionFromLayer { .. } => PrimitiveId::SELECTION_FROM_LAYER,
            Self::ClearSelectedContent { .. } => PrimitiveId::CLEAR_SELECTED_CONTENT,
            Self::CommitFloating { .. } => PrimitiveId::COMMIT_FLOATING,
            Self::MirrorDocument { .. } => PrimitiveId::MIRROR_DOCUMENT,
            Self::RotateDocument { .. } => PrimitiveId::ROTATE_DOCUMENT,
            Self::ResizeDocument { .. } => PrimitiveId::RESIZE_DOCUMENT,
            Self::VectorAddPath { .. } => PrimitiveId::VECTOR_ADD_PATH,
            Self::VectorAddFill { .. } => PrimitiveId::VECTOR_ADD_FILL,
            Self::VectorErase { .. } => PrimitiveId::VECTOR_ERASE,
            Self::VectorConnect { .. } => PrimitiveId::VECTOR_CONNECT,
            Self::VectorCorrectWidth { .. } => PrimitiveId::VECTOR_CORRECT_WIDTH,
            Self::RasterizeVectorLayer { .. } => PrimitiveId::RASTERIZE_VECTOR_LAYER,
            Self::VectorizeRasterPlane { .. } => PrimitiveId::VECTORIZE_RASTER_PLANE,
            Self::VectorizeRasterPlaneIntoNewLayer { .. } => {
                PrimitiveId::VECTORIZE_RASTER_PLANE_INTO_NEW_LAYER
            }
            Self::LightTableSetGlobalOpacity { .. } => PrimitiveId::LIGHT_TABLE_SET_GLOBAL_OPACITY,
            Self::LightTableCreateSet { .. } => PrimitiveId::LIGHT_TABLE_CREATE_SET,
            Self::LightTableDuplicateSet { .. } => PrimitiveId::LIGHT_TABLE_DUPLICATE_SET,
            Self::LightTableDeleteSet { .. } => PrimitiveId::LIGHT_TABLE_DELETE_SET,
            Self::LightTableRenameSet { .. } => PrimitiveId::LIGHT_TABLE_RENAME_SET,
            Self::LightTableReorderSet { .. } => PrimitiveId::LIGHT_TABLE_REORDER_SET,
            Self::LightTableSetActive { .. } => PrimitiveId::LIGHT_TABLE_SET_ACTIVE,
            Self::LightTableAddItem { .. } => PrimitiveId::LIGHT_TABLE_ADD_ITEM,
            Self::LightTableUpdateItemProperties { .. } => {
                PrimitiveId::LIGHT_TABLE_UPDATE_ITEM_PROPERTIES
            }
            Self::LightTableUpdateItem { .. } => PrimitiveId::LIGHT_TABLE_UPDATE_ITEM,
            Self::LightTableRemoveItem { .. } => PrimitiveId::LIGHT_TABLE_REMOVE_ITEM,
            Self::LightTableReorderItem { .. } => PrimitiveId::LIGHT_TABLE_REORDER_ITEM,
            Self::LightTableBulkRegister { .. } => PrimitiveId::LIGHT_TABLE_BULK_REGISTER,
        }
    }

    fn input_ids(&self) -> Vec<u64> {
        match self {
            Self::DuplicateLayer { layer_id }
            | Self::DeleteLayer { layer_id }
            | Self::ReorderLayer { layer_id, .. }
            | Self::SetLayerProperties { layer_id, .. }
            | Self::ConvertLayer { layer_id, .. }
            | Self::MergeLayer { layer_id } => vec![*layer_id],
            Self::EditTargets { targets, .. } => targets
                .iter()
                .flat_map(|target| match target {
                    EditTarget::Layer(layer_id) => vec![*layer_id],
                    EditTarget::Plane(target) => vec![target.layer_id, target.plane_id],
                })
                .collect(),
            Self::CreatePlane { layer_id, .. } => vec![*layer_id],
            Self::DuplicatePlane { plane_id }
            | Self::DeletePlane { plane_id }
            | Self::ReorderPlane { plane_id, .. }
            | Self::SetPlaneProperties { plane_id, .. }
            | Self::ConvertPlane { plane_id, .. }
            | Self::MergePlane { plane_id } => vec![*plane_id],
            Self::MoveGuide { guide_id, .. } | Self::DeleteGuide { guide_id } => vec![*guide_id],
            Self::ApplySelection { target, .. }
            | Self::SelectColor { target, .. }
            | Self::ClearSelectedContent { target }
            | Self::ApplyFill { target, .. } => {
                vec![target.layer_id, target.plane_id]
            }
            Self::ApplyGradient { plane_id, .. }
            | Self::ApplyBoundaryAirbrush { plane_id, .. }
            | Self::ApplyBlur { plane_id, .. }
            | Self::ApplyAirbrush { plane_id, .. }
            | Self::ApplyAirbrushGesture { plane_id, .. }
            | Self::ApplyStamp { plane_id, .. }
            | Self::ApplyStampGesture { plane_id, .. }
            | Self::ApplyBlurTool { plane_id, .. }
            | Self::ApplyBlurPressureTrace { plane_id, .. }
            | Self::ApplyDustRemoval { plane_id, .. }
            | Self::EditPlaneAlpha { plane_id, .. }
            | Self::ApplyAlphaGradient { plane_id, .. }
            | Self::ApplyFilter { plane_id, .. } => vec![*plane_id],
            Self::ApplyGeometry { geometry } => vec![geometry.plane_id],
            Self::ReplaceRasterColors { plane_id, .. }
            | Self::ScopedColorReplace { plane_id, .. }
            | Self::SeparateRasterColors { plane_id, .. }
            | Self::RestoreSelectedPixels { plane_id, .. } => vec![*plane_id],
            Self::UpdateAdjustmentLayer { layer_id, .. } => vec![*layer_id],
            Self::SelectionFromLayer { layer_id, .. } => vec![*layer_id],
            Self::CommitFloating { floating } => match &floating.destination {
                FloatingDestination::ExistingPlanes(plane_ids) => {
                    plane_ids.iter().map(|plane_id| plane_id.get()).collect()
                }
                FloatingDestination::NewPlane { layer_id, .. } => vec![layer_id.get()],
            },
            Self::VectorAddPath { plane_id, .. }
            | Self::VectorAddFill { plane_id, .. }
            | Self::VectorErase { plane_id, .. }
            | Self::VectorConnect { plane_id, .. } => vec![*plane_id],
            Self::VectorCorrectWidth { path_ids, .. } => path_ids.clone(),
            Self::RasterizeVectorLayer { layer_id, .. } => vec![*layer_id],
            Self::VectorizeRasterPlane {
                source_plane_id,
                target_vector_layer_id,
                ..
            } => vec![*source_plane_id, *target_vector_layer_id],
            Self::VectorizeRasterPlaneIntoNewLayer {
                source_plane_id, ..
            } => vec![*source_plane_id],
            Self::LightTableDuplicateSet { set_id }
            | Self::LightTableDeleteSet { set_id }
            | Self::LightTableRenameSet { set_id, .. }
            | Self::LightTableReorderSet { set_id, .. }
            | Self::LightTableSetActive { set_id }
            | Self::LightTableBulkRegister {
                target_set_id: set_id,
                ..
            } => vec![*set_id],
            Self::LightTableUpdateItemProperties { item_id, .. }
            | Self::LightTableUpdateItem { item_id, .. }
            | Self::LightTableRemoveItem { item_id }
            | Self::LightTableReorderItem { item_id, .. } => vec![*item_id],
            Self::UpdatePaperFrames { .. }
            | Self::CreateLayer { .. }
            | Self::DeleteHiddenLayers
            | Self::AddGuide { .. }
            | Self::SetGrid { .. }
            | Self::DeleteAllGuides
            | Self::InvertSelection
            | Self::ClearSelection
            | Self::ResizeSelection { .. }
            | Self::SelectionToLayer { .. }
            | Self::MirrorDocument { .. }
            | Self::RotateDocument { .. }
            | Self::ResizeDocument { .. } => Vec::new(),
            Self::LightTableSetGlobalOpacity { .. }
            | Self::LightTableCreateSet { .. }
            | Self::LightTableAddItem { .. }
            | Self::CreateAdjustmentLayer { .. } => Vec::new(),
        }
    }

    fn asset_ids(&self) -> Vec<AssetId> {
        match self {
            Self::LightTableAddItem { input } | Self::LightTableUpdateItem { input, .. } => {
                vec![input.source.asset_id()]
            }
            Self::LightTableBulkRegister { inputs, .. } => {
                let mut ids = inputs
                    .iter()
                    .map(|input| input.source.asset_id())
                    .collect::<Vec<_>>();
                ids.sort_unstable();
                ids.dedup();
                ids
            }
            _ => Vec::new(),
        }
    }

    fn apply(&self, core: &mut Core) -> Result<InvocationResult, CoreError> {
        match self {
            Self::UpdatePaperFrames { frames } => core
                .update_paper_frames(*frames)
                .map(InvocationResult::dispatch),
            Self::CreateLayer { kind, name } => core
                .create_layer(*kind, name)
                .map(|(dispatch, id)| InvocationResult::output(dispatch, id)),
            Self::DuplicateLayer { layer_id } => core
                .duplicate_layer(*layer_id)
                .map(|(dispatch, id)| InvocationResult::output(dispatch, id)),
            Self::DeleteLayer { layer_id } => {
                core.delete_layer(*layer_id).map(InvocationResult::dispatch)
            }
            Self::ReorderLayer {
                layer_id,
                destination_index,
            } => core
                .reorder_layer(
                    *layer_id,
                    usize::try_from(*destination_index).map_err(|_| {
                        CoreError::InvalidArgument("layer destination index is not representable")
                    })?,
                )
                .map(InvocationResult::dispatch),
            Self::SetLayerProperties {
                layer_id,
                visible,
                editable,
                opacity_milli,
                name,
            } => core
                .set_layer_properties(*layer_id, *visible, *editable, *opacity_milli, name)
                .map(InvocationResult::dispatch),
            Self::CreatePlane {
                layer_id,
                kind,
                format,
                name,
            } => core
                .create_plane(*layer_id, *kind, *format, name)
                .map(|(dispatch, id)| InvocationResult::output(dispatch, id)),
            Self::DuplicatePlane { plane_id } => core
                .duplicate_plane(*plane_id)
                .map(|(dispatch, id)| InvocationResult::output(dispatch, id)),
            Self::DeletePlane { plane_id } => {
                core.delete_plane(*plane_id).map(InvocationResult::dispatch)
            }
            Self::ReorderPlane {
                plane_id,
                destination_index,
            } => core
                .reorder_plane(
                    *plane_id,
                    usize::try_from(*destination_index).map_err(|_| {
                        CoreError::InvalidArgument("plane destination index is not representable")
                    })?,
                )
                .map(InvocationResult::dispatch),
            Self::SetPlaneProperties {
                plane_id,
                visible,
                editable,
                opacity_milli,
                name,
            } => core
                .set_plane_properties(*plane_id, *visible, *editable, *opacity_milli, name)
                .map(InvocationResult::dispatch),
            Self::ConvertPlane {
                plane_id,
                destination_kind,
                destination_format,
            } => core
                .convert_plane(*plane_id, *destination_kind, *destination_format)
                .map(InvocationResult::dispatch),
            Self::MergePlane { plane_id } => core
                .merge_plane_into_below(*plane_id)
                .map(InvocationResult::dispatch),
            Self::ConvertLayer {
                layer_id,
                destination,
            } => core
                .convert_layer(*layer_id, *destination)
                .map(InvocationResult::dispatch),
            Self::MergeLayer { layer_id } => core
                .merge_layer_into_below(*layer_id)
                .map(InvocationResult::dispatch),
            Self::DeleteHiddenLayers => core.delete_hidden_layers().map(InvocationResult::dispatch),
            Self::EditTargets { targets, command } => core
                .apply_edit_target_command_to(targets.clone(), *command)
                .map(|result| {
                    InvocationResult::outputs(
                        result.dispatch,
                        result
                            .output_targets
                            .iter()
                            .map(|target| match target {
                                EditTarget::Layer(layer_id) => *layer_id,
                                EditTarget::Plane(target) => target.plane_id,
                            })
                            .collect(),
                    )
                }),
            Self::AddGuide { axis, position } => core
                .add_guide(*axis, *position)
                .map(|(dispatch, id)| InvocationResult::output(dispatch, id)),
            Self::MoveGuide { guide_id, position } => core
                .move_guide(*guide_id, *position)
                .map(InvocationResult::dispatch),
            Self::DeleteGuide { guide_id } => {
                core.delete_guide(*guide_id).map(InvocationResult::dispatch)
            }
            Self::SetGrid { grid } => core.set_grid(*grid).map(InvocationResult::dispatch),
            Self::DeleteAllGuides => core.delete_all_guides().map(InvocationResult::dispatch),
            Self::ApplyFill {
                request,
                target,
                use_light_table_boundary,
                use_light_table_color,
            } => core
                .apply_fill_for_editor_target(
                    request,
                    *target,
                    *use_light_table_boundary,
                    *use_light_table_color,
                )
                .map(InvocationResult::fill),
            Self::ApplyGeometry { geometry } => {
                core.apply_canonical_geometry(geometry).map(|commit| {
                    let mut ids = Vec::with_capacity(2);
                    if commit.path_id != 0 {
                        ids.push(commit.path_id);
                    }
                    if commit.fill_id != 0 {
                        ids.push(commit.fill_id);
                    }
                    InvocationResult::outputs(commit.dispatch, ids)
                })
            }
            Self::ApplyGradient { plane_id, gradient } => core
                .apply_gradient_to_plane(*plane_id, gradient)
                .map(InvocationResult::dispatch),
            Self::ApplyBoundaryAirbrush { plane_id, effect } => core
                .apply_boundary_airbrush_to_plane(*plane_id, effect)
                .map(InvocationResult::dispatch),
            Self::ApplyBlur {
                plane_id,
                radius,
                strength_milli,
            } => core
                .apply_blur_to_plane(*plane_id, *radius, *strength_milli)
                .map(InvocationResult::dispatch),
            Self::ApplyAirbrush { plane_id, stroke } => core
                .apply_airbrush_to_plane(*plane_id, *stroke)
                .map(InvocationResult::dispatch),
            Self::ApplyAirbrushGesture { plane_id, gesture } => core
                .apply_airbrush_gesture_to_plane(*plane_id, gesture)
                .map(InvocationResult::dispatch),
            Self::ApplyStamp { plane_id, stamp } => core
                .apply_stamp_to_plane(*plane_id, *stamp)
                .map(InvocationResult::dispatch),
            Self::ApplyStampGesture { plane_id, gesture } => core
                .apply_stamp_gesture_to_plane(*plane_id, gesture)
                .map(InvocationResult::dispatch),
            Self::ApplyBlurTool {
                plane_id,
                shape,
                radius,
                strength_milli,
            } => core
                .apply_blur_tool_to_plane(*plane_id, shape, *radius, *strength_milli)
                .map(InvocationResult::dispatch),
            Self::ApplyBlurPressureTrace {
                plane_id,
                samples,
                diameter,
                radius,
                strength_milli,
            } => {
                let samples = samples
                    .iter()
                    .map(|sample| StrokeSample {
                        x: sample.point.x,
                        y: sample.point.y,
                        pressure: sample.pressure,
                    })
                    .collect::<Vec<_>>();
                core.apply_blur_tool_for_view(
                    0,
                    CoordinateSpace::Document,
                    *plane_id,
                    EffectRegionKind::Trace,
                    &samples,
                    *diameter,
                    true,
                    *radius,
                    *strength_milli,
                )
                .map(InvocationResult::dispatch)
            }
            Self::ApplyDustRemoval {
                plane_id,
                shape,
                options,
            } => core
                .apply_dust_removal_to_plane(*plane_id, shape.as_ref(), *options, |_, _| true)
                .map(InvocationResult::dispatch),
            Self::EditPlaneAlpha { plane_id, alpha } => core
                .edit_plane_alpha(*plane_id, alpha)
                .map(InvocationResult::dispatch),
            Self::ApplyAlphaGradient { plane_id, gradient } => core
                .apply_alpha_gradient_to_plane(*plane_id, gradient)
                .map(InvocationResult::dispatch),
            Self::ApplyFilter { plane_id, filter } => {
                core.begin_filter_preview(*plane_id, filter.clone())?;
                core.apply_filter_preview().map(InvocationResult::dispatch)
            }
            Self::CreateAdjustmentLayer { name, adjustment } => core
                .create_adjustment_layer(name, adjustment.clone())
                .map(|(dispatch, id)| InvocationResult::output(dispatch, id)),
            Self::UpdateAdjustmentLayer {
                layer_id,
                adjustment,
            } => core
                .update_adjustment_layer(*layer_id, adjustment.clone())
                .map(InvocationResult::dispatch),
            Self::ReplaceRasterColors { plane_id, pairs } => {
                crate::batch::apply_color_replacement(core, *plane_id, pairs, &mut |_, _| true)
                    .map(InvocationResult::dispatch)
            }
            Self::ScopedColorReplace {
                plane_id,
                mode,
                target,
                replacement,
                region,
            } => core
                .apply_scoped_color_replace_arguments(
                    *plane_id,
                    *mode,
                    *target,
                    *replacement,
                    region.as_ref(),
                )
                .map(InvocationResult::dispatch),
            Self::SeparateRasterColors { plane_id, options } => {
                crate::batch::apply_separation(core, *plane_id, options, &mut |_, _| true)
                    .map(InvocationResult::dispatch)
            }
            Self::RestoreSelectedPixels { plane_id, changes } => core
                .restore_selected_pixels(*plane_id, changes)
                .map(InvocationResult::dispatch),
            Self::ApplySelection {
                shape,
                operation,
                interpretation,
                options,
                target,
            } => core
                .apply_selection_with_options_for_editor_target(
                    shape,
                    *operation,
                    *interpretation,
                    *options,
                    *target,
                )
                .map(InvocationResult::dispatch),
            Self::InvertSelection => core.invert_selection().map(InvocationResult::dispatch),
            Self::ClearSelection => core.clear_selection().map(InvocationResult::dispatch),
            Self::ResizeSelection { pixels } => core
                .resize_selection(*pixels)
                .map(InvocationResult::dispatch),
            Self::SelectColor {
                color,
                tolerance,
                different,
                operation,
                target,
            } => core
                .select_color_for_editor_target(*color, *tolerance, *different, *operation, *target)
                .map(InvocationResult::dispatch),
            Self::SelectionToLayer { name } => core
                .selection_to_layer(name)
                .map(|(dispatch, id)| InvocationResult::output(dispatch, id)),
            Self::SelectionFromLayer {
                layer_id,
                operation,
            } => core
                .selection_from_layer(*layer_id, *operation)
                .map(InvocationResult::dispatch),
            Self::ClearSelectedContent { target } => core
                .clear_selected_content_for_editor_target(*target)
                .map(InvocationResult::dispatch),
            Self::CommitFloating { floating } => {
                core.floating = Some(floating.clone());
                core.commit_floating().map(InvocationResult::dispatch)
            }
            Self::MirrorDocument { axis } => {
                core.mirror_document(*axis).map(InvocationResult::dispatch)
            }
            Self::RotateDocument { direction } => core
                .rotate_document(*direction)
                .map(InvocationResult::dispatch),
            Self::ResizeDocument { resize } => core
                .resize_document(*resize)
                .map(InvocationResult::dispatch),
            Self::VectorAddPath { plane_id, input } => core
                .vector_add_path(*plane_id, input.clone())
                .map(|(dispatch, id)| InvocationResult::output(dispatch, id)),
            Self::VectorAddFill {
                plane_id,
                boundary_path_ids,
                color,
            } => core
                .vector_add_fill(*plane_id, boundary_path_ids, *color)
                .map(|(dispatch, id)| InvocationResult::output(dispatch, id)),
            Self::VectorErase {
                plane_id,
                point,
                radius,
                mode,
            } => core
                .vector_erase(*plane_id, *point, *radius, *mode)
                .map(InvocationResult::dispatch),
            Self::VectorConnect {
                plane_id,
                maximum_gap,
            } => core
                .vector_connect(*plane_id, *maximum_gap)
                .map(|(dispatch, id)| {
                    InvocationResult::outputs(dispatch, id.into_iter().collect())
                }),
            Self::VectorCorrectWidth { path_ids, mode } => core
                .vector_correct_width(path_ids, *mode)
                .map(InvocationResult::dispatch),
            Self::RasterizeVectorLayer {
                layer_id,
                antialias,
                name,
            } => core
                .rasterize_vector_layer_to_document(*layer_id, *antialias, name)
                .map(|(dispatch, id)| InvocationResult::output(dispatch, id)),
            Self::VectorizeRasterPlane {
                source_plane_id,
                target_vector_layer_id,
                alpha_threshold,
            } => core
                .vectorize_raster_plane(*source_plane_id, *target_vector_layer_id, *alpha_threshold)
                .map(|(dispatch, ids)| InvocationResult::outputs(dispatch, ids)),
            Self::VectorizeRasterPlaneIntoNewLayer {
                source_plane_id,
                alpha_threshold,
                name,
            } => core
                .vectorize_raster_plane_into_new_layer(*source_plane_id, *alpha_threshold, name)
                .map(|(dispatch, layer_id, fill_ids)| {
                    if layer_id == 0 {
                        return InvocationResult::outputs(dispatch, Vec::new());
                    }
                    let mut ids = Vec::with_capacity(fill_ids.len() + 1);
                    ids.push(layer_id);
                    ids.extend(fill_ids);
                    InvocationResult::outputs(dispatch, ids)
                }),
            Self::LightTableSetGlobalOpacity { opacity_milli } => core
                .light_table_set_global_opacity(*opacity_milli)
                .map(InvocationResult::dispatch),
            Self::LightTableCreateSet { name } => core
                .light_table_create_set(name.clone())
                .map(|(dispatch, id)| InvocationResult::output(dispatch, id)),
            Self::LightTableDuplicateSet { set_id } => core
                .light_table_duplicate_set(*set_id)
                .map(|(dispatch, id)| InvocationResult::output(dispatch, id)),
            Self::LightTableDeleteSet { set_id } => core
                .light_table_delete_set(*set_id)
                .map(InvocationResult::dispatch),
            Self::LightTableRenameSet { set_id, name } => core
                .light_table_rename_set(*set_id, name.clone())
                .map(InvocationResult::dispatch),
            Self::LightTableReorderSet {
                set_id,
                destination_index,
            } => core
                .light_table_reorder_set(
                    *set_id,
                    usize::try_from(*destination_index).map_err(|_| {
                        CoreError::InvalidArgument("light-table set index is not representable")
                    })?,
                )
                .map(InvocationResult::dispatch),
            Self::LightTableSetActive { set_id } => core
                .light_table_set_active(*set_id)
                .map(InvocationResult::dispatch),
            Self::LightTableAddItem { input } => core
                .light_table_add_item(input.clone())
                .map(|(dispatch, id)| InvocationResult::output(dispatch, id)),
            Self::LightTableUpdateItemProperties {
                item_id,
                properties,
            } => core
                .light_table_update_item_properties(*item_id, *properties)
                .map(InvocationResult::dispatch),
            Self::LightTableUpdateItem { item_id, input } => core
                .light_table_update_item(*item_id, input.clone())
                .map(InvocationResult::dispatch),
            Self::LightTableRemoveItem { item_id } => core
                .light_table_remove_item(*item_id)
                .map(InvocationResult::dispatch),
            Self::LightTableReorderItem {
                item_id,
                destination_index,
            } => core
                .light_table_reorder_item(
                    *item_id,
                    usize::try_from(*destination_index).map_err(|_| {
                        CoreError::InvalidArgument("light-table item index is not representable")
                    })?,
                )
                .map(InvocationResult::dispatch),
            Self::LightTableBulkRegister {
                target_set_id,
                inputs,
            } => core
                .light_table_bulk_register_resolved(*target_set_id, inputs.clone())
                .map(|(dispatch, ids)| InvocationResult::outputs(dispatch, ids)),
        }
    }

    fn canonical_arguments(&self) -> Result<Vec<u8>, CoreError> {
        let mut writer = CanonicalWriter::new(self.primitive_id());
        match self {
            Self::UpdatePaperFrames { frames } => writer.frames(*frames),
            Self::CreateLayer { kind, name } => {
                writer.u32(layer_kind_code(*kind));
                writer.string(name)?;
            }
            Self::DuplicateLayer { layer_id }
            | Self::DeleteLayer { layer_id }
            | Self::MergeLayer { layer_id } => writer.u64(*layer_id),
            Self::DeleteHiddenLayers => {}
            Self::EditTargets { targets, command } => {
                writer.u32(u32::try_from(targets.len()).map_err(|_| {
                    CoreError::InvalidArgument("edit-target count is not representable")
                })?);
                for target in targets {
                    match target {
                        EditTarget::Layer(layer_id) => {
                            writer.u32(1);
                            writer.u64(*layer_id);
                        }
                        EditTarget::Plane(target) => {
                            writer.u32(2);
                            writer.u64(target.layer_id);
                            writer.u64(target.plane_id);
                        }
                    }
                }
                write_edit_target_command(&mut writer, *command);
            }
            Self::ReorderLayer {
                layer_id,
                destination_index,
            } => {
                writer.u64(*layer_id);
                writer.u64(*destination_index);
            }
            Self::SetLayerProperties {
                layer_id,
                visible,
                editable,
                opacity_milli,
                name,
            } => {
                writer.u64(*layer_id);
                writer.boolean(*visible);
                writer.boolean(*editable);
                writer.u32(*opacity_milli);
                writer.string(name)?;
            }
            Self::CreatePlane {
                layer_id,
                kind,
                format,
                name,
            } => {
                writer.u64(*layer_id);
                writer.u32(plane_type_code(*kind));
                writer.u32(pixel_format_code(*format));
                writer.string(name)?;
            }
            Self::DuplicatePlane { plane_id }
            | Self::DeletePlane { plane_id }
            | Self::MergePlane { plane_id } => writer.u64(*plane_id),
            Self::ReorderPlane {
                plane_id,
                destination_index,
            } => {
                writer.u64(*plane_id);
                writer.u64(*destination_index);
            }
            Self::SetPlaneProperties {
                plane_id,
                visible,
                editable,
                opacity_milli,
                name,
            } => {
                writer.u64(*plane_id);
                writer.boolean(*visible);
                writer.boolean(*editable);
                writer.u32(*opacity_milli);
                writer.string(name)?;
            }
            Self::ConvertPlane {
                plane_id,
                destination_kind,
                destination_format,
            } => {
                writer.u64(*plane_id);
                writer.u32(plane_type_code(*destination_kind));
                writer.u32(pixel_format_code(*destination_format));
            }
            Self::ConvertLayer {
                layer_id,
                destination,
            } => {
                writer.u64(*layer_id);
                writer.u32(layer_kind_code(*destination));
            }
            Self::AddGuide { axis, position } => {
                writer.u32(guide_axis_code(*axis));
                writer.i32(*position);
            }
            Self::MoveGuide { guide_id, position } => {
                writer.u64(*guide_id);
                writer.i32(*position);
            }
            Self::DeleteGuide { guide_id } => writer.u64(*guide_id),
            Self::SetGrid { grid } => writer.grid(*grid),
            Self::DeleteAllGuides => {}
            Self::ApplyFill {
                request,
                target,
                use_light_table_boundary,
                use_light_table_color,
            } => {
                writer.fill_request(request)?;
                writer.editor_target(*target);
                writer.boolean(*use_light_table_boundary);
                writer.boolean(*use_light_table_color);
            }
            Self::ApplyGeometry { geometry } => writer.geometry(geometry)?,
            Self::ApplyGradient { plane_id, gradient } => {
                writer.u64(*plane_id);
                writer.gradient(gradient)?;
            }
            Self::ApplyBoundaryAirbrush { plane_id, effect } => {
                writer.u64(*plane_id);
                writer.boundary_airbrush(effect)?;
            }
            Self::ApplyBlur {
                plane_id,
                radius,
                strength_milli,
            } => {
                writer.u64(*plane_id);
                writer.u32(*radius);
                writer.u32(*strength_milli);
            }
            Self::ApplyAirbrush { plane_id, stroke } => {
                writer.u64(*plane_id);
                writer.airbrush_stroke(*stroke);
            }
            Self::ApplyAirbrushGesture { plane_id, gesture } => {
                writer.u64(*plane_id);
                writer.airbrush_gesture(gesture)?;
            }
            Self::ApplyStamp { plane_id, stamp } => {
                writer.u64(*plane_id);
                writer.stamp(*stamp);
            }
            Self::ApplyStampGesture { plane_id, gesture } => {
                writer.u64(*plane_id);
                writer.stamp_gesture(gesture)?;
            }
            Self::ApplyBlurTool {
                plane_id,
                shape,
                radius,
                strength_milli,
            } => {
                writer.u64(*plane_id);
                writer.u32(1);
                writer.selection_shape(shape)?;
                writer.u32(*radius);
                writer.u32(*strength_milli);
            }
            Self::ApplyBlurPressureTrace {
                plane_id,
                samples,
                diameter,
                radius,
                strength_milli,
            } => {
                writer.u64(*plane_id);
                writer.u32(2);
                writer.stroke_samples(samples)?;
                writer.q16_f32(*diameter)?;
                writer.u32(*radius);
                writer.u32(*strength_milli);
            }
            Self::ApplyDustRemoval {
                plane_id,
                shape,
                options,
            } => {
                writer.u64(*plane_id);
                writer.boolean(shape.is_some());
                if let Some(shape) = shape {
                    writer.selection_shape(shape)?;
                }
                writer.dust_removal(*options);
            }
            Self::EditPlaneAlpha { plane_id, alpha } => {
                writer.u64(*plane_id);
                writer.raster(alpha)?;
            }
            Self::ApplyAlphaGradient { plane_id, gradient } => {
                writer.u64(*plane_id);
                writer.gradient(gradient)?;
            }
            Self::ApplyFilter { plane_id, filter } => {
                writer.u64(*plane_id);
                writer.filter(filter)?;
            }
            Self::CreateAdjustmentLayer { name, adjustment } => {
                writer.string(name)?;
                writer.adjustment(adjustment)?;
            }
            Self::UpdateAdjustmentLayer {
                layer_id,
                adjustment,
            } => {
                writer.u64(*layer_id);
                writer.adjustment(adjustment)?;
            }
            Self::ReplaceRasterColors { plane_id, pairs } => {
                writer.u64(*plane_id);
                writer.batch_color_pairs(pairs)?;
            }
            Self::ScopedColorReplace {
                plane_id,
                mode,
                target,
                replacement,
                region,
            } => {
                writer.u64(*plane_id);
                writer.u32(scoped_color_replace_mode_code(*mode));
                writer.pixel(*target);
                writer.pixel(*replacement);
                writer.boolean(region.is_some());
                if let Some(region) = region {
                    writer.selection_shape(region)?;
                }
            }
            Self::SeparateRasterColors { plane_id, options } => {
                writer.u64(*plane_id);
                writer.batch_separation(options)?;
            }
            Self::RestoreSelectedPixels { plane_id, changes } => {
                writer.u64(*plane_id);
                writer.pixel_changes(changes)?;
            }
            Self::ApplySelection {
                shape,
                operation,
                interpretation,
                options,
                target,
            } => {
                writer.selection_shape(shape)?;
                writer.u32(selection_operation_code(*operation));
                writer.u32(range_interpretation_code(*interpretation));
                writer.selection_construction_options(*options);
                writer.editor_target(*target);
            }
            Self::InvertSelection | Self::ClearSelection => {}
            Self::ResizeSelection { pixels } => writer.i32(*pixels),
            Self::SelectColor {
                color,
                tolerance,
                different,
                operation,
                target,
            } => {
                writer.pixel(*color);
                writer.u16(*tolerance);
                writer.boolean(*different);
                writer.u32(selection_operation_code(*operation));
                writer.editor_target(*target);
            }
            Self::SelectionToLayer { name } => writer.string(name)?,
            Self::SelectionFromLayer {
                layer_id,
                operation,
            } => {
                writer.u64(*layer_id);
                writer.u32(selection_layer_operation_code(*operation));
            }
            Self::ClearSelectedContent { target } => writer.editor_target(*target),
            Self::CommitFloating { floating } => writer.floating(floating)?,
            Self::MirrorDocument { axis } => writer.u32(mirror_axis_code(*axis)),
            Self::RotateDocument { direction } => writer.u32(rotate_direction_code(*direction)),
            Self::ResizeDocument { resize } => writer.document_resize(*resize),
            Self::VectorAddPath { plane_id, input } => {
                writer.u64(*plane_id);
                writer.vector_path(input)?;
            }
            Self::VectorAddFill {
                plane_id,
                boundary_path_ids,
                color,
            } => {
                writer.u64(*plane_id);
                writer.ids(boundary_path_ids)?;
                writer.pixel(*color);
            }
            Self::VectorErase {
                plane_id,
                point,
                radius,
                mode,
            } => {
                writer.u64(*plane_id);
                writer.point(*point)?;
                writer.q16_f32(*radius)?;
                writer.u32(vector_erase_mode_code(*mode));
            }
            Self::VectorConnect {
                plane_id,
                maximum_gap,
            } => {
                writer.u64(*plane_id);
                writer.q16_f32(*maximum_gap)?;
            }
            Self::VectorCorrectWidth { path_ids, mode } => {
                writer.ids(path_ids)?;
                writer.vector_width_mode(*mode)?;
            }
            Self::RasterizeVectorLayer {
                layer_id,
                antialias,
                name,
            } => {
                writer.u64(*layer_id);
                writer.boolean(*antialias);
                writer.string(name)?;
            }
            Self::VectorizeRasterPlane {
                source_plane_id,
                target_vector_layer_id,
                alpha_threshold,
            } => {
                writer.u64(*source_plane_id);
                writer.u64(*target_vector_layer_id);
                writer.u8(*alpha_threshold);
            }
            Self::VectorizeRasterPlaneIntoNewLayer {
                source_plane_id,
                alpha_threshold,
                name,
            } => {
                writer.u64(*source_plane_id);
                writer.u8(*alpha_threshold);
                writer.string(name)?;
            }
            Self::LightTableSetGlobalOpacity { opacity_milli } => writer.u32(*opacity_milli),
            Self::LightTableCreateSet { name } => writer.string(name)?,
            Self::LightTableDuplicateSet { set_id }
            | Self::LightTableDeleteSet { set_id }
            | Self::LightTableSetActive { set_id } => writer.u64(*set_id),
            Self::LightTableRenameSet { set_id, name } => {
                writer.u64(*set_id);
                writer.string(name)?;
            }
            Self::LightTableReorderSet {
                set_id,
                destination_index,
            } => {
                writer.u64(*set_id);
                writer.u64(*destination_index);
            }
            Self::LightTableAddItem { input } => writer.light_table_item(input)?,
            Self::LightTableUpdateItemProperties {
                item_id,
                properties,
            } => {
                writer.u64(*item_id);
                writer.light_table_properties(*properties);
            }
            Self::LightTableUpdateItem { item_id, input } => {
                writer.u64(*item_id);
                writer.light_table_item(input)?;
            }
            Self::LightTableRemoveItem { item_id } => writer.u64(*item_id),
            Self::LightTableReorderItem {
                item_id,
                destination_index,
            } => {
                writer.u64(*item_id);
                writer.u64(*destination_index);
            }
            Self::LightTableBulkRegister {
                target_set_id,
                inputs,
            } => {
                writer.u64(*target_set_id);
                writer.u32(u32::try_from(inputs.len()).map_err(|_| {
                    CoreError::InvalidArgument("light-table bulk item count is not representable")
                })?);
                for input in inputs {
                    writer.light_table_item(input)?;
                }
            }
        }
        Ok(writer.finish())
    }
}

impl Core {
    pub(crate) fn execute_canonical_invocation(
        &mut self,
        invocation: CanonicalInvocation,
    ) -> Result<InvocationResult, CoreError> {
        self.execute_canonical_invocation_internal(invocation, None, None)
            .map(|(result, _)| result)
    }

    pub(crate) fn execute_canonical_invocation_with<F>(
        &mut self,
        invocation: CanonicalInvocation,
        apply: F,
    ) -> Result<InvocationResult, CoreError>
    where
        F: FnOnce(&mut Core) -> Result<InvocationResult, CoreError>,
    {
        self.execute_canonical_invocation_internal(invocation, None, Some(Box::new(apply)))
            .map(|(result, _)| result)
    }

    pub(super) fn replay_runtime_invocation(
        &mut self,
        procedure: &CanonicalProcedure,
        runtime: &RuntimeInvocation,
    ) -> Result<PrimitiveOutcome, CoreError> {
        let (result, procedure) = self.execute_canonical_invocation_internal(
            runtime.invocation().clone(),
            Some(procedure),
            None,
        )?;
        let procedure = procedure.ok_or(CoreError::InvalidState(
            "committed runtime invocation did not produce a procedure",
        ))?;
        Ok(PrimitiveOutcome::committed(result.dispatch, procedure))
    }

    fn execute_canonical_invocation_internal(
        &mut self,
        invocation: CanonicalInvocation,
        expected: Option<&CanonicalProcedure>,
        apply: Option<InvocationApply<'_>>,
    ) -> Result<(InvocationResult, Option<Arc<CanonicalProcedure>>), CoreError> {
        self.ensure_no_active_raster_stroke()?;
        self.ensure_canonical_state_cache_current()?;
        let pre_state_digest = self.document_state_digest()?;
        let runtime = RuntimeInvocation::new(invocation.canonicalized()?)?;
        let primitive_id = runtime.invocation().primitive_id();
        let input_ids = runtime.invocation().input_ids();
        let procedure_id = self.next_procedure;
        let following_procedure = procedure_id
            .checked_next()
            .ok_or(CoreError::InvalidState("procedure ID overflow"))?;

        let mut staged = self.clone();
        staged.canonical_invocation_active = true;
        let result = match apply {
            Some(apply) => apply(&mut staged)?,
            None => runtime.invocation().apply(&mut staged)?,
        };
        staged.canonical_invocation_active = false;
        if staged.document_revision == self.document_revision {
            if expected.is_some() {
                return Err(CoreError::InvalidState(
                    "committed procedure replays as a semantic no-op",
                ));
            }
            staged.canonical_invocation_active = false;
            if staged.staged_history.is_some() {
                return Err(CoreError::InvalidState(
                    "semantic no-op left a staged history entry",
                ));
            }
            *self = staged;
            return Ok((result, None));
        }
        let expected_revision = self.next_document_revision()?;
        if staged.document_revision != expected_revision
            || staged.history_cursor != self.history_cursor
            || staged.history.len() != self.history.len()
            || staged.current_state != self.next_state
            || staged.staged_history.is_none()
        {
            return Err(CoreError::InvalidState(
                "canonical invocation did not produce exactly one document commit",
            ));
        }
        let post_state_digest = staged.document_state_digest()?;
        let payload = Vec::new();
        let payload_digest = canonical_payload_digest(&payload)?;
        let procedure = Arc::new(CanonicalProcedure {
            procedure_id,
            primitive_id,
            primitive_schema_version: INVOCATION_SCHEMA_VERSION,
            replay_epoch: ReplayEpoch::CURRENT,
            base_state_id: self.current_state,
            committed_state_id: self.next_state,
            input_ids,
            output_ids: result.output_ids.clone(),
            asset_ids: runtime.invocation().asset_ids(),
            canonical_arguments: runtime.arguments().to_vec(),
            canonical_payload: payload,
            canonical_payload_digest: payload_digest,
            pre_state_digest,
            post_state_digest,
            runtime_invocation: Some(runtime.clone()),
        });

        if let Some(expected) = expected {
            if expected.primitive_id != procedure.primitive_id
                || expected.primitive_schema_version != procedure.primitive_schema_version
                || expected.input_ids != procedure.input_ids
                || expected.output_ids != procedure.output_ids
                || expected.asset_ids != procedure.asset_ids
                || expected.canonical_arguments != procedure.canonical_arguments
                || expected.canonical_payload != procedure.canonical_payload
                || expected.post_state_digest != procedure.post_state_digest
            {
                return Err(CoreError::InvalidArgument(
                    "procedure canonical fields do not match its invocation schema",
                ));
            }
        }

        let plan = self.prepare_canonical_commit(Arc::clone(&procedure))?;
        let branch_id = plan.branch_id();
        let pending = staged.staged_history.take().ok_or(CoreError::InvalidState(
            "canonical staged history entry is missing",
        ))?;
        if pending.before_state != self.current_state || pending.after_state != self.next_state {
            return Err(CoreError::InvalidState(
                "canonical staged history state IDs do not match the procedure",
            ));
        }
        staged.history.truncate(staged.history_cursor);
        staged
            .history
            .try_reserve(1)
            .map_err(|_| CoreError::InvalidState("history allocation failed"))?;
        staged.history.push(HistoryEntry {
            change: Some(pending.change),
            label: pending.label,
            before_state: pending.before_state,
            after_state: pending.after_state,
            procedure: Arc::clone(&procedure),
            branch_id,
        });
        staged.history_cursor = staged.history.len();

        staged.journal = std::mem::take(&mut self.journal);
        staged.active_branch = self.active_branch;
        staged.next_journal_event = self.next_journal_event;
        staged.next_branch = self.next_branch;
        staged.branch_tails = std::mem::take(&mut self.branch_tails);
        staged.publish_canonical_commit(plan);
        staged.next_procedure = following_procedure;
        staged.canonical_invocation_active = false;
        *self = staged;
        Ok((result, Some(procedure)))
    }

    pub(crate) const fn canonical_invocation_is_active(&self) -> bool {
        self.canonical_invocation_active
    }
}

pub(super) const fn schema_version(primitive_id: PrimitiveId) -> Option<u16> {
    let value = primitive_id.get();
    if value == PrimitiveId::UPDATE_PAPER_FRAMES.get()
        || value == PrimitiveId::CREATE_LAYER.get()
        || value == PrimitiveId::DUPLICATE_LAYER.get()
        || value == PrimitiveId::DELETE_LAYER.get()
        || value == PrimitiveId::REORDER_LAYER.get()
        || value == PrimitiveId::SET_LAYER_PROPERTIES.get()
        || value == PrimitiveId::CREATE_PLANE.get()
        || value == PrimitiveId::DUPLICATE_PLANE.get()
        || value == PrimitiveId::DELETE_PLANE.get()
        || value == PrimitiveId::REORDER_PLANE.get()
        || value == PrimitiveId::SET_PLANE_PROPERTIES.get()
        || value == PrimitiveId::CONVERT_PLANE.get()
        || value == PrimitiveId::MERGE_PLANE.get()
        || value == PrimitiveId::CONVERT_LAYER.get()
        || value == PrimitiveId::MERGE_LAYER.get()
        || value == PrimitiveId::DELETE_HIDDEN_LAYERS.get()
        || value == PrimitiveId::EDIT_TARGETS.get()
        || value == PrimitiveId::ADD_GUIDE.get()
        || value == PrimitiveId::MOVE_GUIDE.get()
        || value == PrimitiveId::DELETE_GUIDE.get()
        || value == PrimitiveId::SET_GRID.get()
        || value == PrimitiveId::DELETE_ALL_GUIDES.get()
        || value == PrimitiveId::APPLY_FILL.get()
        || value == PrimitiveId::APPLY_GEOMETRY.get()
        || value == PrimitiveId::APPLY_GRADIENT.get()
        || value == PrimitiveId::APPLY_BOUNDARY_AIRBRUSH.get()
        || value == PrimitiveId::APPLY_BLUR.get()
        || value == PrimitiveId::APPLY_AIRBRUSH.get()
        || value == PrimitiveId::APPLY_AIRBRUSH_GESTURE.get()
        || value == PrimitiveId::APPLY_STAMP.get()
        || value == PrimitiveId::APPLY_STAMP_GESTURE.get()
        || value == PrimitiveId::APPLY_BLUR_TOOL.get()
        || value == PrimitiveId::APPLY_DUST_REMOVAL.get()
        || value == PrimitiveId::EDIT_PLANE_ALPHA.get()
        || value == PrimitiveId::APPLY_ALPHA_GRADIENT.get()
        || value == PrimitiveId::APPLY_FILTER.get()
        || value == PrimitiveId::CREATE_ADJUSTMENT_LAYER.get()
        || value == PrimitiveId::UPDATE_ADJUSTMENT_LAYER.get()
        || value == PrimitiveId::REPLACE_RASTER_COLORS.get()
        || value == PrimitiveId::SCOPED_COLOR_REPLACE.get()
        || value == PrimitiveId::SEPARATE_RASTER_COLORS.get()
        || value == PrimitiveId::RESTORE_SELECTED_PIXELS.get()
        || value == PrimitiveId::APPLY_SELECTION.get()
        || value == PrimitiveId::INVERT_SELECTION.get()
        || value == PrimitiveId::CLEAR_SELECTION.get()
        || value == PrimitiveId::RESIZE_SELECTION.get()
        || value == PrimitiveId::SELECT_COLOR.get()
        || value == PrimitiveId::SELECTION_TO_LAYER.get()
        || value == PrimitiveId::SELECTION_FROM_LAYER.get()
        || value == PrimitiveId::CLEAR_SELECTED_CONTENT.get()
        || value == PrimitiveId::COMMIT_FLOATING.get()
        || value == PrimitiveId::MIRROR_DOCUMENT.get()
        || value == PrimitiveId::ROTATE_DOCUMENT.get()
        || value == PrimitiveId::RESIZE_DOCUMENT.get()
        || value == PrimitiveId::VECTOR_ADD_PATH.get()
        || value == PrimitiveId::VECTOR_ADD_FILL.get()
        || value == PrimitiveId::VECTOR_ERASE.get()
        || value == PrimitiveId::VECTOR_CONNECT.get()
        || value == PrimitiveId::VECTOR_CORRECT_WIDTH.get()
        || value == PrimitiveId::RASTERIZE_VECTOR_LAYER.get()
        || value == PrimitiveId::VECTORIZE_RASTER_PLANE.get()
        || value == PrimitiveId::VECTORIZE_RASTER_PLANE_INTO_NEW_LAYER.get()
        || value == PrimitiveId::LIGHT_TABLE_SET_GLOBAL_OPACITY.get()
        || value == PrimitiveId::LIGHT_TABLE_CREATE_SET.get()
        || value == PrimitiveId::LIGHT_TABLE_DUPLICATE_SET.get()
        || value == PrimitiveId::LIGHT_TABLE_DELETE_SET.get()
        || value == PrimitiveId::LIGHT_TABLE_RENAME_SET.get()
        || value == PrimitiveId::LIGHT_TABLE_REORDER_SET.get()
        || value == PrimitiveId::LIGHT_TABLE_SET_ACTIVE.get()
        || value == PrimitiveId::LIGHT_TABLE_ADD_ITEM.get()
        || value == PrimitiveId::LIGHT_TABLE_UPDATE_ITEM_PROPERTIES.get()
        || value == PrimitiveId::LIGHT_TABLE_UPDATE_ITEM.get()
        || value == PrimitiveId::LIGHT_TABLE_REMOVE_ITEM.get()
        || value == PrimitiveId::LIGHT_TABLE_REORDER_ITEM.get()
        || value == PrimitiveId::LIGHT_TABLE_BULK_REGISTER.get()
    {
        Some(INVOCATION_SCHEMA_VERSION)
    } else {
        None
    }
}

struct CanonicalReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> CanonicalReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn invalid(&self, message: &'static str) -> CoreError {
        CoreError::Format(message.to_owned())
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], CoreError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| self.invalid("canonical invocation offset overflows"))?;
        let result = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| self.invalid("canonical invocation is truncated"))?;
        self.offset = end;
        Ok(result)
    }

    fn finish(self) -> Result<(), CoreError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(self.invalid("canonical invocation has trailing bytes"))
        }
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], CoreError> {
        self.take(N)?
            .try_into()
            .map_err(|_| self.invalid("canonical invocation field is truncated"))
    }

    fn u8(&mut self) -> Result<u8, CoreError> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, CoreError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, CoreError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn i32(&mut self) -> Result<i32, CoreError> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, CoreError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    fn i64(&mut self) -> Result<i64, CoreError> {
        Ok(i64::from_le_bytes(self.array()?))
    }

    fn u128(&mut self) -> Result<u128, CoreError> {
        Ok(u128::from_le_bytes(self.array()?))
    }

    fn boolean(&mut self) -> Result<bool, CoreError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(self.invalid("canonical boolean is invalid")),
        }
    }

    fn count(&mut self, minimum_item_bytes: usize) -> Result<usize, CoreError> {
        let count = usize::try_from(self.u32()?)
            .map_err(|_| self.invalid("canonical item count is not representable"))?;
        if count > 1_048_576
            || count
                .checked_mul(minimum_item_bytes)
                .is_none_or(|minimum| minimum > self.bytes.len().saturating_sub(self.offset))
        {
            return Err(self.invalid("canonical item count exceeds its bounded payload"));
        }
        Ok(count)
    }

    fn string(&mut self) -> Result<String, CoreError> {
        let length = self.count(1)?;
        let bytes = self.take(length)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| self.invalid("canonical string is not valid UTF-8"))
    }

    fn q16_f32(&mut self) -> Result<f32, CoreError> {
        Ok(self.i64()? as f32 / CANONICAL_DOCUMENT_ONE as f32)
    }

    fn q16_f64(&mut self) -> Result<f64, CoreError> {
        Ok(self.i64()? as f64 / CANONICAL_DOCUMENT_ONE as f64)
    }

    fn unit_u16_f32(&mut self) -> Result<f32, CoreError> {
        Ok(f32::from(self.u16()?) / f32::from(u16::MAX))
    }

    fn rect(&mut self) -> Result<RectI32, CoreError> {
        Ok(RectI32 {
            x: self.i32()?,
            y: self.i32()?,
            width: self.i32()?,
            height: self.i32()?,
        })
    }

    fn frames(&mut self) -> Result<FrameMetadata, CoreError> {
        Ok(FrameMetadata {
            hundred_frame: self.rect()?,
            reference_frame: self.rect()?,
            drawing_frame: self.rect()?,
            safe_frame: self.rect()?,
            shooting_frame: self.rect()?,
            maximum_close_frame: self.rect()?,
            margins: Margins {
                left: self.u32()?,
                top: self.u32()?,
                right: self.u32()?,
                bottom: self.u32()?,
            },
        })
    }

    fn grid(&mut self) -> Result<GridConfig, CoreError> {
        Ok(GridConfig {
            origin_x: self.i32()?,
            origin_y: self.i32()?,
            spacing_x: self.u32()?,
            spacing_y: self.u32()?,
            subdivisions: self.u32()?,
        })
    }

    fn layer_kind(&mut self) -> Result<LayerKind, CoreError> {
        match self.u32()? {
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
            _ => Err(self.invalid("canonical layer kind is invalid")),
        }
    }

    fn plane_type(&mut self) -> Result<PlaneType, CoreError> {
        match self.u32()? {
            1 => Ok(PlaneType::MainLine),
            2 => Ok(PlaneType::Color),
            3 => Ok(PlaneType::Raster),
            4 => Ok(PlaneType::Selection),
            5 => Ok(PlaneType::VectorMainLine),
            6 => Ok(PlaneType::ColorTrace),
            7 => Ok(PlaneType::VectorFill),
            _ => Err(self.invalid("canonical plane kind is invalid")),
        }
    }

    fn pixel_format(&mut self) -> Result<PixelFormat, CoreError> {
        match self.u32()? {
            1 => Ok(PixelFormat::BinaryMask8),
            2 => Ok(PixelFormat::Grayscale8),
            3 => Ok(PixelFormat::Grayscale16),
            4 => Ok(PixelFormat::StraightRgba8),
            5 => Ok(PixelFormat::StraightRgba16),
            6 => Ok(PixelFormat::PremultipliedBgra8),
            _ => Err(self.invalid("canonical pixel format is invalid")),
        }
    }

    fn guide_axis(&mut self) -> Result<GuideAxis, CoreError> {
        match self.u32()? {
            1 => Ok(GuideAxis::Horizontal),
            2 => Ok(GuideAxis::Vertical),
            _ => Err(self.invalid("canonical guide axis is invalid")),
        }
    }

    fn selection_operation(&mut self) -> Result<SelectionOperation, CoreError> {
        match self.u32()? {
            1 => Ok(SelectionOperation::New),
            2 => Ok(SelectionOperation::Add),
            3 => Ok(SelectionOperation::Subtract),
            4 => Ok(SelectionOperation::Intersect),
            _ => Err(self.invalid("canonical selection operation is invalid")),
        }
    }

    fn selection_layer_operation(&mut self) -> Result<SelectionLayerOperation, CoreError> {
        match self.u32()? {
            1 => Ok(SelectionLayerOperation::Replace),
            2 => Ok(SelectionLayerOperation::Add),
            3 => Ok(SelectionLayerOperation::Subtract),
            _ => Err(self.invalid("canonical selection-layer operation is invalid")),
        }
    }

    fn mirror_axis(&mut self) -> Result<MirrorAxis, CoreError> {
        match self.u32()? {
            1 => Ok(MirrorAxis::Horizontal),
            2 => Ok(MirrorAxis::Vertical),
            _ => Err(self.invalid("canonical mirror axis is invalid")),
        }
    }

    fn rotate_direction(&mut self) -> Result<RotateDirection, CoreError> {
        match self.u32()? {
            1 => Ok(RotateDirection::Left90),
            2 => Ok(RotateDirection::Right90),
            _ => Err(self.invalid("canonical rotate direction is invalid")),
        }
    }

    fn fill_request(&mut self) -> Result<FillRequest, CoreError> {
        let operation = match self.u32()? {
            1 => FillOperation::Seed,
            2 => FillOperation::ClosedRegion,
            3 => FillOperation::Extend,
            _ => return Err(self.invalid("canonical fill operation is invalid")),
        };
        let seed_x = self.u32()?;
        let seed_y = self.u32()?;
        let color = self.pixel()?;
        let selection = self.boolean()?.then(|| self.rect()).transpose()?;
        let use_document_selection = self.boolean()?;
        let tolerance = self.u16()?;
        let detached_regions = self.boolean()?;
        let overflow_abort = self.boolean()?;
        let gap_close = self.u8()?;
        let transparent_only = self.boolean()?;
        let inclusion_mode = match self.u32()? {
            1 => InclusionMode::None,
            2 => InclusionMode::Specified,
            3 => InclusionMode::ExceptSpecified,
            _ => return Err(self.invalid("canonical inclusion mode is invalid")),
        };
        let color_count = self.count(5)?;
        let mut inclusion_colors = Vec::with_capacity(color_count);
        for _ in 0..color_count {
            inclusion_colors.push(self.pixel()?);
        }
        Ok(FillRequest {
            operation,
            seed_x,
            seed_y,
            color,
            selection,
            use_document_selection,
            tolerance,
            detached_regions,
            overflow_abort,
            gap_close,
            transparent_only,
            inclusion_mode,
            inclusion_colors,
            extension_distance: self.u32()?,
        })
    }

    fn color16(&mut self) -> Result<[u16; 4], CoreError> {
        Ok([self.u16()?, self.u16()?, self.u16()?, self.u16()?])
    }

    fn gradient(&mut self) -> Result<Gradient, CoreError> {
        let kind = match self.u32()? {
            1 => GradientKind::Linear,
            2 => GradientKind::Radial,
            _ => return Err(self.invalid("canonical gradient kind is invalid")),
        };
        let mode = match self.u32()? {
            1 => GradientMode::Composite,
            2 => GradientMode::Overwrite,
            _ => return Err(self.invalid("canonical gradient mode is invalid")),
        };
        let start_x_milli = self.i64()?;
        let start_y_milli = self.i64()?;
        let end_x_milli = self.i64()?;
        let end_y_milli = self.i64()?;
        let dither = self.boolean()?;
        let count = self.count(12)?;
        let mut stops = Vec::with_capacity(count);
        for _ in 0..count {
            stops.push(GradientStop {
                position_milli: self.u32()?,
                color: self.color16()?,
            });
        }
        Ok(Gradient {
            kind,
            mode,
            start_x_milli,
            start_y_milli,
            end_x_milli,
            end_y_milli,
            dither,
            stops,
        })
    }

    fn boundary_airbrush(&mut self) -> Result<BoundaryAirbrush, CoreError> {
        let count = self.count(8)?;
        let mut colors = Vec::with_capacity(count);
        for _ in 0..count {
            colors.push(self.color16()?);
        }
        Ok(BoundaryAirbrush {
            colors,
            width: self.u32()?,
            strength_milli: self.u32()?,
        })
    }

    fn airbrush_stroke(&mut self) -> Result<AirbrushStroke, CoreError> {
        Ok(AirbrushStroke {
            center_x_milli: self.i64()?,
            center_y_milli: self.i64()?,
            radius_milli: self.u32()?,
            hardness_milli: self.u32()?,
            opacity_milli: self.u32()?,
            color: self.color16()?,
        })
    }

    fn effect_samples(&mut self) -> Result<Vec<EffectSample>, CoreError> {
        let count = self.count(20)?;
        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            result.push(EffectSample {
                x_milli: self.i64()?,
                y_milli: self.i64()?,
                pressure_milli: self.u32()?,
            });
        }
        Ok(result)
    }

    fn airbrush_gesture(&mut self) -> Result<AirbrushGesture, CoreError> {
        Ok(AirbrushGesture {
            samples: self.effect_samples()?,
            radius_milli: self.u32()?,
            hardness_milli: self.u32()?,
            spacing_milli: self.u32()?,
            opacity_milli: self.u32()?,
            fade_milli: self.u32()?,
            pressure_size: self.boolean()?,
            pressure_opacity: self.boolean()?,
            continuous_dabs: self.u32()?,
            color: self.color16()?,
        })
    }

    fn stamp(&mut self) -> Result<Stamp, CoreError> {
        Ok(Stamp {
            source_x: self.i32()?,
            source_y: self.i32()?,
            destination_x: self.i32()?,
            destination_y: self.i32()?,
            width: self.u32()?,
            height: self.u32()?,
            opacity_milli: self.u32()?,
        })
    }

    fn stamp_gesture(&mut self) -> Result<StampGesture, CoreError> {
        let source_x_milli = self.i64()?;
        let source_y_milli = self.i64()?;
        let samples = self.effect_samples()?;
        let radius_milli = self.u32()?;
        let hardness_milli = self.u32()?;
        let spacing_milli = self.u32()?;
        let opacity_milli = self.u32()?;
        let shape = match self.u32()? {
            1 => StampShape::Round,
            2 => StampShape::Square,
            _ => return Err(self.invalid("canonical stamp shape is invalid")),
        };
        Ok(StampGesture {
            source_x_milli,
            source_y_milli,
            samples,
            radius_milli,
            hardness_milli,
            spacing_milli,
            opacity_milli,
            shape,
            pressure_size: self.boolean()?,
            pressure_opacity: self.boolean()?,
        })
    }

    fn stroke_samples(&mut self) -> Result<Vec<DocumentStrokeSample>, CoreError> {
        let count = self.count(18)?;
        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            result.push(DocumentStrokeSample {
                point: DocumentPointF32::new(self.q16_f32()?, self.q16_f32()?)?,
                pressure: self.unit_u16_f32()?,
            });
        }
        Ok(result)
    }

    fn dust_removal(&mut self) -> Result<DustRemoval, CoreError> {
        let mode = match self.u32()? {
            1 => DustMode::RemoveForeground,
            2 => DustMode::FillTransparentHoles,
            3 => DustMode::ReplaceColorOutliers,
            _ => return Err(self.invalid("canonical dust-removal mode is invalid")),
        };
        Ok(DustRemoval {
            mode,
            maximum_pixels: self.u32()?,
        })
    }

    fn raster(&mut self) -> Result<TileRaster, CoreError> {
        let width = self.u32()?;
        let height = self.u32()?;
        let format = self.pixel_format()?;
        let mut raster = TileRaster::new(width, height, format)?;
        let count = self.count(16)?;
        for _ in 0..count {
            let coord = TileCoord {
                x: self.u32()?,
                y: self.u32()?,
            };
            let tile_width = self.u32()?;
            let tile_height = self.u32()?;
            let length = usize::try_from(tile_width)
                .ok()
                .and_then(|width| width.checked_mul(usize::try_from(tile_height).ok()?))
                .and_then(|pixels| pixels.checked_mul(format.bytes_per_pixel()))
                .ok_or_else(|| self.invalid("canonical raster tile length overflows"))?;
            raster.insert_tile(TileData {
                coord,
                width: tile_width,
                height: tile_height,
                bytes: self.take(length)?.to_vec(),
                revision: 1,
            })?;
        }
        Ok(raster)
    }

    fn channel(&mut self) -> Result<Channel, CoreError> {
        match self.u32()? {
            1 => Ok(Channel::Rgb),
            2 => Ok(Channel::Red),
            3 => Ok(Channel::Green),
            4 => Ok(Channel::Blue),
            _ => Err(self.invalid("canonical channel is invalid")),
        }
    }

    fn interpolation(&mut self) -> Result<CurveInterpolation, CoreError> {
        match self.u32()? {
            1 => Ok(CurveInterpolation::Bezier),
            2 => Ok(CurveInterpolation::BSpline),
            _ => Err(self.invalid("canonical curve interpolation is invalid")),
        }
    }

    fn curve_points(&mut self) -> Result<Vec<CurvePoint>, CoreError> {
        let count = self.count(4)?;
        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            result.push(CurvePoint {
                input: self.u16()?,
                output: self.u16()?,
            });
        }
        Ok(result)
    }

    fn levels(&mut self) -> Result<Levels, CoreError> {
        Ok(Levels {
            channel: self.channel()?,
            input_shadow: self.u16()?,
            input_gamma_milli: self.u32()?,
            input_highlight: self.u16()?,
            output_shadow: self.u16()?,
            output_highlight: self.u16()?,
        })
    }

    fn filter(&mut self) -> Result<Filter, CoreError> {
        Ok(match self.u32()? {
            1 => Filter::SharpenWeak,
            2 => Filter::SharpenStrong,
            3 => Filter::BlurWeak,
            4 => Filter::BlurStrong,
            5 => Filter::GaussianBlur {
                radius: self.u32()?,
                strength_milli: self.u32()?,
            },
            6 => Filter::UnsharpMask {
                radius: self.u32()?,
                amount_milli: self.u32()?,
                threshold: self.u16()?,
            },
            7 => Filter::Invert {
                channel: self.channel()?,
            },
            8 => Filter::AutoContrast,
            9 => Filter::BrightnessContrast {
                brightness_milli: self.i32()?,
                contrast_milli: self.i32()?,
            },
            10 => Filter::ToneCurve {
                channel: self.channel()?,
                interpolation: self.interpolation()?,
                points: self.curve_points()?,
            },
            11 => Filter::Levels(self.levels()?),
            12 => Filter::Hsv(HsvAdjustment {
                hue_degrees_milli: self.i32()?,
                saturation_milli: self.i32()?,
                value_milli: self.i32()?,
            }),
            13 => Filter::ColorBalance(ColorBalance {
                red_milli: self.i32()?,
                green_milli: self.i32()?,
                blue_milli: self.i32()?,
            }),
            _ => return Err(self.invalid("canonical filter kind is invalid")),
        })
    }

    fn adjustment(&mut self) -> Result<Adjustment, CoreError> {
        Ok(match self.u32()? {
            1 => Adjustment::BrightnessContrast {
                brightness_milli: self.i32()?,
                contrast_milli: self.i32()?,
            },
            2 => Adjustment::ToneCurve {
                channel: self.channel()?,
                interpolation: self.interpolation()?,
                points: self.curve_points()?,
            },
            3 => Adjustment::Levels(self.levels()?),
            _ => return Err(self.invalid("canonical adjustment kind is invalid")),
        })
    }

    fn batch_color_pairs(&mut self) -> Result<Vec<BatchColorPair>, CoreError> {
        let count = self.count(11)?;
        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            result.push(BatchColorPair {
                enabled: self.boolean()?,
                old: self.pixel()?,
                new: self.pixel()?,
            });
        }
        Ok(result)
    }

    fn batch_separation(&mut self) -> Result<BatchSeparation, CoreError> {
        let count = self.count(5)?;
        let mut colors = Vec::with_capacity(count);
        for _ in 0..count {
            colors.push(self.pixel()?);
        }
        Ok(BatchSeparation {
            colors,
            replacement: self.pixel()?,
            invert: self.boolean()?,
            destination: match self.u32()? {
                1 => BatchSeparationDestination::ReplaceSource,
                2 => BatchSeparationDestination::SelectionMask,
                3 => BatchSeparationDestination::MainLinePlane,
                4 => BatchSeparationDestination::ColorPlane,
                5 => BatchSeparationDestination::NativeFile,
                _ => {
                    return Err(self.invalid("canonical batch separation destination is invalid"));
                }
            },
        })
    }

    fn pixel_changes(&mut self) -> Result<Vec<PixelChange>, CoreError> {
        let count = self.count(18)?;
        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            result.push(PixelChange {
                x: self.u32()?,
                y: self.u32()?,
                before: self.pixel()?,
                after: self.pixel()?,
            });
        }
        Ok(result)
    }

    fn editor_target(&mut self) -> Result<EditorTarget, CoreError> {
        Ok(EditorTarget {
            layer_id: self.u64()?,
            plane_id: self.u64()?,
        })
    }

    fn point(&mut self) -> Result<PointF32, CoreError> {
        Ok(PointF32 {
            x: self.q16_f32()?,
            y: self.q16_f32()?,
        })
    }

    fn points(&mut self) -> Result<Vec<PointF32>, CoreError> {
        let count = self.count(16)?;
        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            result.push(self.point()?);
        }
        Ok(result)
    }

    fn selection_shape(&mut self) -> Result<SelectionShape, CoreError> {
        Ok(match self.u32()? {
            1 => SelectionShape::Rectangle(self.rect()?),
            2 => SelectionShape::Ellipse(self.rect()?),
            3 => SelectionShape::Lasso(self.points()?),
            4 => SelectionShape::Polyline(self.points()?),
            5 => SelectionShape::Trace {
                points: self.points()?,
                diameter: self.q16_f32()?,
            },
            6 => SelectionShape::Wand {
                x: self.u32()?,
                y: self.u32()?,
                tolerance: self.u16()?,
                gap_close: self.u8()?,
            },
            7 => SelectionShape::RectangleGesture {
                anchor: self.point()?,
                current: self.point()?,
            },
            8 => SelectionShape::EllipseGesture {
                anchor: self.point()?,
                current: self.point()?,
            },
            9 => {
                let count = self.count(16)?;
                let mut samples = Vec::with_capacity(count);
                for _ in 0..count {
                    samples.push(SelectionSample {
                        x: self.q16_f32()?,
                        y: self.q16_f32()?,
                        pressure: f32::from(self.u16()?) / f32::from(u16::MAX),
                    });
                }
                SelectionShape::TraceBrush {
                    samples,
                    diameter: self.q16_f32()?,
                }
            }
            _ => return Err(self.invalid("canonical selection shape is invalid")),
        })
    }

    fn range_interpretation(&mut self) -> Result<RangeInterpretation, CoreError> {
        match self.u32()? {
            1 => Ok(RangeInterpretation::Normal),
            2 => Ok(RangeInterpretation::Tight),
            3 => Ok(RangeInterpretation::EnclosedInterior),
            4 => Ok(RangeInterpretation::Drawing),
            5 => Ok(RangeInterpretation::Boundary),
            _ => Err(self.invalid("canonical raster range interpretation is invalid")),
        }
    }

    fn selection_construction_options(
        &mut self,
    ) -> Result<SelectionConstructionOptions, CoreError> {
        let aspect_ratio_q16 = self.u32()?;
        let from_center = self.boolean()?;
        let constrain_rotation_45 = self.boolean()?;
        let rotation_turns = self.u32()?;
        let shape = match self.u32()? {
            1 => TraceBrushShape::Round,
            2 => TraceBrushShape::Square,
            _ => return Err(self.invalid("canonical trace brush shape is invalid")),
        };
        let pressure_size = self.boolean()?;
        let screen_size = self.boolean()?;
        let view_zoom_q16 = self.i64()?;
        Ok(SelectionConstructionOptions {
            aspect_ratio_q16,
            from_center,
            constrain_rotation_45,
            rotation_turns,
            trace: TraceBrushOptions {
                shape,
                pressure_size,
                screen_size,
                view_zoom_q16,
            },
        })
    }

    fn pixel(&mut self) -> Result<PixelValue, CoreError> {
        Ok(match self.u32()? {
            1 => PixelValue::Binary(self.u8()?),
            2 => PixelValue::Grayscale8(self.u8()?),
            3 => PixelValue::Grayscale16(self.u16()?),
            4 => PixelValue::Rgba(self.array()?),
            5 => PixelValue::Rgba16([self.u16()?, self.u16()?, self.u16()?, self.u16()?]),
            _ => return Err(self.invalid("canonical pixel value is invalid")),
        })
    }

    fn floating(&mut self) -> Result<FloatingSelection, CoreError> {
        let destination = match self.u8()? {
            0 => {
                let count = self.count(8)?;
                if count == 0 || count > MAX_PLANES_PER_LAYER {
                    return Err(self.invalid("canonical floating destination count is invalid"));
                }
                let mut plane_ids = Vec::with_capacity(count);
                for _ in 0..count {
                    plane_ids.push(PlaneId::from_raw(self.u64()?));
                }
                FloatingDestination::ExistingPlanes(plane_ids)
            }
            1 => FloatingDestination::NewPlane {
                layer_id: LayerId::from_raw(self.u64()?),
                kind: self.plane_type()?,
                format: self.pixel_format()?,
                name: self.string()?,
                opacity_milli: self.u32()?,
            },
            _ => return Err(self.invalid("canonical floating destination is invalid")),
        };
        let source_document_uuid = self.u128()?;
        let bounds = self.rect()?;
        let count = self.count(20)?;
        let mut planes = Vec::with_capacity(count);
        for _ in 0..count {
            let kind = self.plane_type()?;
            let pixel_format = self.pixel_format()?;
            let origin_x = self.i32()?;
            let origin_y = self.i32()?;
            let pixel_count = self.count(13)?;
            let mut pixels = Vec::with_capacity(pixel_count);
            for _ in 0..pixel_count {
                pixels.push(ClipboardPixel {
                    x: self.i32()?,
                    y: self.i32()?,
                    value: self.pixel()?,
                });
            }
            let path_count = self.count(32)?;
            let mut vector_paths = Vec::with_capacity(path_count);
            for _ in 0..path_count {
                let id = self.u64()?;
                let plane_id = self.u64()?;
                let input = self.vector_path()?;
                vector_paths.push(VectorPathInfo {
                    id,
                    plane_id,
                    segments: input.segments,
                    color: input.color,
                    closed: input.closed,
                    square_cross_section: self.boolean()?,
                });
            }
            let fill_count = self.count(32)?;
            let mut vector_fills = Vec::with_capacity(fill_count);
            for _ in 0..fill_count {
                vector_fills.push(VectorFillInfo {
                    id: self.u64()?,
                    plane_id: self.u64()?,
                    color: self.pixel()?,
                    boundary_path_ids: self.ids()?,
                });
            }
            planes.push(ClipboardPlane {
                kind,
                pixel_format,
                origin_x,
                origin_y,
                pixels,
                vector_paths,
                vector_fills,
            });
        }
        let translate_x = self.q16_f64()?;
        let translate_y = self.q16_f64()?;
        let scale_x = self.q16_f64()?;
        let scale_y = self.q16_f64()?;
        let rotation_degrees = f64::from(self.u32()?) * 360.0 / 4_294_967_296.0;
        Ok(FloatingSelection {
            payload: ClipboardPayload {
                source_document_uuid,
                bounds,
                planes,
            },
            destination,
            transform: FloatingTransform {
                translate_x,
                translate_y,
                scale_x,
                scale_y,
                rotation_degrees,
            },
            asset_ids: Vec::new(),
        })
    }

    fn document_resize(&mut self) -> Result<DocumentResize, CoreError> {
        let width = self.u32()?;
        let height = self.u32()?;
        let dpi_x_milli = self.u32()?;
        let dpi_y_milli = self.u32()?;
        let anchor = match self.u32()? {
            1 => ResizeAnchor::TopLeft,
            2 => ResizeAnchor::TopRight,
            3 => ResizeAnchor::Center,
            4 => ResizeAnchor::BottomLeft,
            5 => ResizeAnchor::BottomRight,
            _ => return Err(self.invalid("canonical resize anchor is invalid")),
        };
        Ok(DocumentResize {
            width,
            height,
            dpi_x_milli,
            dpi_y_milli,
            anchor,
            resample: self.boolean()?,
        })
    }

    fn ids(&mut self) -> Result<Vec<u64>, CoreError> {
        let count = self.count(8)?;
        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            result.push(self.u64()?);
        }
        Ok(result)
    }

    fn geometry(&mut self) -> Result<CanonicalGeometry, CoreError> {
        let plane_id = self.u64()?;
        let primitive = match self.u32()? {
            1 => GeometryPrimitive::Line,
            2 => GeometryPrimitive::Curve,
            3 => GeometryPrimitive::Rectangle,
            4 => GeometryPrimitive::Ellipse,
            5 => GeometryPrimitive::Polygon,
            6 => GeometryPrimitive::Polyline,
            _ => return Err(self.invalid("canonical geometry primitive is invalid")),
        };
        let flags = self.u32()?;
        if flags & !0x7 != 0 {
            return Err(self.invalid("canonical geometry flags are invalid"));
        }
        let cross_section = match self.u32()? {
            1 => GeometryCrossSection::Round,
            2 => GeometryCrossSection::Square,
            _ => return Err(self.invalid("canonical geometry cross-section is invalid")),
        };
        let outline_width_q16 = self.i64()?;
        let outline_color = self.pixel()?;
        let fill_color = self.pixel()?;
        let segment_count = self.count(80)?;
        if segment_count > MAX_GEOMETRY_POINTS * 2 {
            return Err(self.invalid("canonical geometry has too many segments"));
        }
        let mut segments = Vec::with_capacity(segment_count);
        for _ in 0..segment_count {
            let mut point = || -> Result<CanonicalGeometryPoint, CoreError> {
                Ok(CanonicalGeometryPoint {
                    x_q16: self.i64()?,
                    y_q16: self.i64()?,
                })
            };
            segments.push(CanonicalGeometrySegment {
                p0: point()?,
                p1: point()?,
                p2: point()?,
                p3: point()?,
                width_start_q16: self.i64()?,
                width_end_q16: self.i64()?,
            });
        }
        let boundary_count = self.count(16)?;
        if boundary_count > MAX_CANONICAL_GEOMETRY_BOUNDARY_POINTS {
            return Err(self.invalid("canonical geometry fill boundary is too large"));
        }
        let mut fill_boundary = Vec::with_capacity(boundary_count);
        for _ in 0..boundary_count {
            fill_boundary.push(CanonicalGeometryPoint {
                x_q16: self.i64()?,
                y_q16: self.i64()?,
            });
        }
        Ok(CanonicalGeometry {
            plane_id,
            primitive,
            segments,
            fill_boundary,
            outline_color,
            fill_color,
            outline_width_q16,
            cross_section,
            outline: flags & 0x1 != 0,
            fill: flags & 0x2 != 0,
            closed: flags & 0x4 != 0,
        })
    }

    fn vector_path(&mut self) -> Result<VectorPathInput, CoreError> {
        let count = self.count(48)?;
        let mut segments = Vec::with_capacity(count);
        for _ in 0..count {
            segments.push(VectorCubicSegment {
                p0: self.point()?,
                p1: self.point()?,
                p2: self.point()?,
                p3: self.point()?,
                width_start: self.q16_f32()?,
                width_end: self.q16_f32()?,
            });
        }
        Ok(VectorPathInput {
            segments,
            color: self.pixel()?,
            closed: self.boolean()?,
        })
    }

    fn vector_erase_mode(&mut self) -> Result<VectorEraseMode, CoreError> {
        match self.u32()? {
            1 => Ok(VectorEraseMode::Partial),
            2 => Ok(VectorEraseMode::ToIntersection),
            3 => Ok(VectorEraseMode::WholePath),
            _ => Err(self.invalid("canonical vector eraser mode is invalid")),
        }
    }

    fn vector_width_mode(&mut self) -> Result<VectorWidthMode, CoreError> {
        Ok(match self.u32()? {
            1 => VectorWidthMode::Add(self.q16_f32()?),
            2 => VectorWidthMode::Subtract(self.q16_f32()?),
            3 => VectorWidthMode::Scale(self.q16_f32()?),
            4 => VectorWidthMode::Constant(self.q16_f32()?),
            _ => return Err(self.invalid("canonical vector-width mode is invalid")),
        })
    }

    fn asset_id(&mut self) -> Result<AssetId, CoreError> {
        Ok(AssetId::from_bytes(self.array()?))
    }

    fn light_table_source(
        &mut self,
        assets: &crate::asset::AssetStore,
    ) -> Result<LightTableSource, CoreError> {
        let document_uuid = self.u128()?;
        let source_revision = self.u64()?;
        let reference_frame = self.rect()?;
        let dpi_x_milli = self.u32()?;
        let dpi_y_milli = self.u32()?;
        let asset_id = self.asset_id()?;
        let asset = assets
            .get(asset_id)
            .ok_or_else(|| self.invalid("canonical light-table asset is missing"))?;
        LightTableSource::from_record(
            document_uuid,
            source_revision,
            reference_frame,
            dpi_x_milli,
            dpi_y_milli,
            asset,
        )
    }

    fn light_table_display_mode(&mut self) -> Result<LightTableDisplayMode, CoreError> {
        match self.u32()? {
            1 => Ok(LightTableDisplayMode::Color),
            2 => Ok(LightTableDisplayMode::Monotone),
            3 => Ok(LightTableDisplayMode::Halftone),
            _ => Err(self.invalid("canonical light-table display mode is invalid")),
        }
    }

    fn light_table_properties(&mut self) -> Result<LightTableItemProperties, CoreError> {
        Ok(LightTableItemProperties {
            visible: self.boolean()?,
            opacity_milli: self.u32()?,
            display_mode: self.light_table_display_mode()?,
            display_color: self.pixel()?,
            translate_x_milli: self.i32()?,
            translate_y_milli: self.i32()?,
            scale_x_milli: self.u32()?,
            scale_y_milli: self.u32()?,
            rotation_milli_degrees: self.i32()?,
        })
    }

    fn light_table_item(
        &mut self,
        assets: &crate::asset::AssetStore,
    ) -> Result<LightTableItemInput, CoreError> {
        let name = self.string()?;
        let source = self.light_table_source(assets)?;
        let properties = self.light_table_properties()?;
        Ok(LightTableItemInput {
            name,
            source,
            visible: properties.visible,
            opacity_milli: properties.opacity_milli,
            display_mode: properties.display_mode,
            display_color: properties.display_color,
            translate_x_milli: properties.translate_x_milli,
            translate_y_milli: properties.translate_y_milli,
            scale_x_milli: properties.scale_x_milli,
            scale_y_milli: properties.scale_y_milli,
            rotation_milli_degrees: properties.rotation_milli_degrees,
        })
    }
}

struct CanonicalWriter {
    bytes: Vec<u8>,
}

impl CanonicalWriter {
    fn new(primitive_id: PrimitiveId) -> Self {
        let mut result = Self { bytes: Vec::new() };
        result.u32(primitive_id.get());
        result
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn boolean(&mut self, value: bool) {
        self.bytes.push(u8::from(value));
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u128(&mut self, value: u128) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn asset_id(&mut self, value: AssetId) {
        self.bytes.extend_from_slice(value.as_bytes());
    }

    fn q16_f32(&mut self, value: f32) -> Result<(), CoreError> {
        let value = canonical_q16_from_f32(value).ok_or(CoreError::InvalidArgument(
            "canonical binary32 scalar is not representable",
        ))?;
        self.i64(value);
        Ok(())
    }

    fn q16_f64(&mut self, value: f64) -> Result<(), CoreError> {
        let value = canonical_q16_from_f64(value).ok_or(CoreError::InvalidArgument(
            "canonical binary64 scalar is not representable",
        ))?;
        self.i64(value);
        Ok(())
    }

    fn unit_u16_f32(&mut self, value: f32) -> Result<(), CoreError> {
        let value = canonical_unit_u16_from_f32(value).ok_or(CoreError::InvalidArgument(
            "canonical unit scalar is outside bounds",
        ))?;
        self.u16(value);
        Ok(())
    }

    fn i32(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn string(&mut self, value: &str) -> Result<(), CoreError> {
        let length = u32::try_from(value.len())
            .map_err(|_| CoreError::InvalidArgument("canonical string is too long"))?;
        self.u32(length);
        self.bytes.extend_from_slice(value.as_bytes());
        Ok(())
    }

    fn rect(&mut self, rect: RectI32) {
        self.i32(rect.x);
        self.i32(rect.y);
        self.i32(rect.width);
        self.i32(rect.height);
    }

    fn frames(&mut self, frames: FrameMetadata) {
        self.rect(frames.hundred_frame);
        self.rect(frames.reference_frame);
        self.rect(frames.drawing_frame);
        self.rect(frames.safe_frame);
        self.rect(frames.shooting_frame);
        self.rect(frames.maximum_close_frame);
        self.u32(frames.margins.left);
        self.u32(frames.margins.top);
        self.u32(frames.margins.right);
        self.u32(frames.margins.bottom);
    }

    fn grid(&mut self, grid: GridConfig) {
        self.i32(grid.origin_x);
        self.i32(grid.origin_y);
        self.u32(grid.spacing_x);
        self.u32(grid.spacing_y);
        self.u32(grid.subdivisions);
    }

    fn fill_request(&mut self, request: &FillRequest) -> Result<(), CoreError> {
        self.u32(fill_operation_code(request.operation));
        self.u32(request.seed_x);
        self.u32(request.seed_y);
        self.pixel(request.color);
        self.boolean(request.selection.is_some());
        if let Some(selection) = request.selection {
            self.rect(selection);
        }
        self.boolean(request.use_document_selection);
        self.u16(request.tolerance);
        self.boolean(request.detached_regions);
        self.boolean(request.overflow_abort);
        self.u8(request.gap_close);
        self.boolean(request.transparent_only);
        self.u32(inclusion_mode_code(request.inclusion_mode));
        let color_count = u32::try_from(request.inclusion_colors.len())
            .map_err(|_| CoreError::InvalidArgument("too many canonical inclusion colors"))?;
        self.u32(color_count);
        for color in &request.inclusion_colors {
            self.pixel(*color);
        }
        self.u32(request.extension_distance);
        Ok(())
    }

    fn color16(&mut self, color: [u16; 4]) {
        for channel in color {
            self.u16(channel);
        }
    }

    fn gradient(&mut self, gradient: &Gradient) -> Result<(), CoreError> {
        self.u32(match gradient.kind {
            GradientKind::Linear => 1,
            GradientKind::Radial => 2,
        });
        self.u32(match gradient.mode {
            GradientMode::Composite => 1,
            GradientMode::Overwrite => 2,
        });
        self.i64(gradient.start_x_milli);
        self.i64(gradient.start_y_milli);
        self.i64(gradient.end_x_milli);
        self.i64(gradient.end_y_milli);
        self.boolean(gradient.dither);
        let count = u32::try_from(gradient.stops.len())
            .map_err(|_| CoreError::InvalidArgument("too many canonical gradient stops"))?;
        self.u32(count);
        for stop in &gradient.stops {
            self.u32(stop.position_milli);
            self.color16(stop.color);
        }
        Ok(())
    }

    fn boundary_airbrush(&mut self, effect: &BoundaryAirbrush) -> Result<(), CoreError> {
        let count = u32::try_from(effect.colors.len())
            .map_err(|_| CoreError::InvalidArgument("too many canonical boundary colors"))?;
        self.u32(count);
        for color in &effect.colors {
            self.color16(*color);
        }
        self.u32(effect.width);
        self.u32(effect.strength_milli);
        Ok(())
    }

    fn airbrush_stroke(&mut self, stroke: AirbrushStroke) {
        self.i64(stroke.center_x_milli);
        self.i64(stroke.center_y_milli);
        self.u32(stroke.radius_milli);
        self.u32(stroke.hardness_milli);
        self.u32(stroke.opacity_milli);
        self.color16(stroke.color);
    }

    fn effect_samples(&mut self, samples: &[EffectSample]) -> Result<(), CoreError> {
        let count = u32::try_from(samples.len())
            .map_err(|_| CoreError::InvalidArgument("too many canonical effect samples"))?;
        self.u32(count);
        for sample in samples {
            self.i64(sample.x_milli);
            self.i64(sample.y_milli);
            self.u32(sample.pressure_milli);
        }
        Ok(())
    }

    fn airbrush_gesture(&mut self, gesture: &AirbrushGesture) -> Result<(), CoreError> {
        self.effect_samples(&gesture.samples)?;
        self.u32(gesture.radius_milli);
        self.u32(gesture.hardness_milli);
        self.u32(gesture.spacing_milli);
        self.u32(gesture.opacity_milli);
        self.u32(gesture.fade_milli);
        self.boolean(gesture.pressure_size);
        self.boolean(gesture.pressure_opacity);
        self.u32(gesture.continuous_dabs);
        self.color16(gesture.color);
        Ok(())
    }

    fn stamp(&mut self, stamp: Stamp) {
        self.i32(stamp.source_x);
        self.i32(stamp.source_y);
        self.i32(stamp.destination_x);
        self.i32(stamp.destination_y);
        self.u32(stamp.width);
        self.u32(stamp.height);
        self.u32(stamp.opacity_milli);
    }

    fn stamp_gesture(&mut self, gesture: &StampGesture) -> Result<(), CoreError> {
        self.i64(gesture.source_x_milli);
        self.i64(gesture.source_y_milli);
        self.effect_samples(&gesture.samples)?;
        self.u32(gesture.radius_milli);
        self.u32(gesture.hardness_milli);
        self.u32(gesture.spacing_milli);
        self.u32(gesture.opacity_milli);
        self.u32(match gesture.shape {
            StampShape::Round => 1,
            StampShape::Square => 2,
        });
        self.boolean(gesture.pressure_size);
        self.boolean(gesture.pressure_opacity);
        Ok(())
    }

    fn stroke_samples(&mut self, samples: &[DocumentStrokeSample]) -> Result<(), CoreError> {
        let count = u32::try_from(samples.len())
            .map_err(|_| CoreError::InvalidArgument("too many canonical stroke samples"))?;
        self.u32(count);
        for sample in samples {
            self.q16_f32(sample.point.x)?;
            self.q16_f32(sample.point.y)?;
            self.unit_u16_f32(sample.pressure)?;
        }
        Ok(())
    }

    fn dust_removal(&mut self, options: DustRemoval) {
        self.u32(match options.mode {
            DustMode::RemoveForeground => 1,
            DustMode::FillTransparentHoles => 2,
            DustMode::ReplaceColorOutliers => 3,
        });
        self.u32(options.maximum_pixels);
    }

    fn raster(&mut self, raster: &TileRaster) -> Result<(), CoreError> {
        self.u32(raster.width());
        self.u32(raster.height());
        self.u32(pixel_format_code(raster.format()));
        let coords = raster.allocated_coords().collect::<Vec<_>>();
        let count = u32::try_from(coords.len())
            .map_err(|_| CoreError::InvalidArgument("too many canonical raster tiles"))?;
        self.u32(count);
        for coord in coords {
            let view = raster
                .tile_view(coord)
                .ok_or(CoreError::InvalidState("canonical raster tile disappeared"))?;
            self.u32(coord.x);
            self.u32(coord.y);
            self.u32(view.width());
            self.u32(view.height());
            let row_bytes = usize::try_from(view.width())
                .ok()
                .and_then(|width| width.checked_mul(raster.format().bytes_per_pixel()))
                .ok_or(CoreError::InvalidArgument("canonical raster row overflows"))?;
            let stride = usize::try_from(view.row_stride_bytes())
                .map_err(|_| CoreError::InvalidArgument("canonical raster stride overflows"))?;
            for y in 0..usize::try_from(view.height())
                .map_err(|_| CoreError::InvalidArgument("canonical raster height overflows"))?
            {
                let start = y.checked_mul(stride).ok_or(CoreError::InvalidArgument(
                    "canonical raster offset overflows",
                ))?;
                let end = start
                    .checked_add(row_bytes)
                    .ok_or(CoreError::InvalidArgument(
                        "canonical raster row end overflows",
                    ))?;
                self.bytes
                    .extend_from_slice(view.bytes().get(start..end).ok_or(
                        CoreError::InvalidState("canonical raster tile bytes are truncated"),
                    )?);
            }
        }
        Ok(())
    }

    fn curve_points(&mut self, points: &[CurvePoint]) -> Result<(), CoreError> {
        let count = u32::try_from(points.len())
            .map_err(|_| CoreError::InvalidArgument("too many canonical curve points"))?;
        self.u32(count);
        for point in points {
            self.u16(point.input);
            self.u16(point.output);
        }
        Ok(())
    }

    fn channel(&mut self, channel: Channel) {
        self.u32(match channel {
            Channel::Rgb => 1,
            Channel::Red => 2,
            Channel::Green => 3,
            Channel::Blue => 4,
        });
    }

    fn interpolation(&mut self, interpolation: CurveInterpolation) {
        self.u32(match interpolation {
            CurveInterpolation::Bezier => 1,
            CurveInterpolation::BSpline => 2,
        });
    }

    fn levels(&mut self, levels: &Levels) {
        self.channel(levels.channel);
        self.u16(levels.input_shadow);
        self.u32(levels.input_gamma_milli);
        self.u16(levels.input_highlight);
        self.u16(levels.output_shadow);
        self.u16(levels.output_highlight);
    }

    fn filter(&mut self, filter: &Filter) -> Result<(), CoreError> {
        match filter {
            Filter::SharpenWeak => self.u32(1),
            Filter::SharpenStrong => self.u32(2),
            Filter::BlurWeak => self.u32(3),
            Filter::BlurStrong => self.u32(4),
            Filter::GaussianBlur {
                radius,
                strength_milli,
            } => {
                self.u32(5);
                self.u32(*radius);
                self.u32(*strength_milli);
            }
            Filter::UnsharpMask {
                radius,
                amount_milli,
                threshold,
            } => {
                self.u32(6);
                self.u32(*radius);
                self.u32(*amount_milli);
                self.u16(*threshold);
            }
            Filter::Invert { channel } => {
                self.u32(7);
                self.channel(*channel);
            }
            Filter::AutoContrast => self.u32(8),
            Filter::BrightnessContrast {
                brightness_milli,
                contrast_milli,
            } => {
                self.u32(9);
                self.i32(*brightness_milli);
                self.i32(*contrast_milli);
            }
            Filter::ToneCurve {
                channel,
                interpolation,
                points,
            } => {
                self.u32(10);
                self.channel(*channel);
                self.interpolation(*interpolation);
                self.curve_points(points)?;
            }
            Filter::Levels(levels) => {
                self.u32(11);
                self.levels(levels);
            }
            Filter::Hsv(hsv) => {
                self.u32(12);
                self.i32(hsv.hue_degrees_milli);
                self.i32(hsv.saturation_milli);
                self.i32(hsv.value_milli);
            }
            Filter::ColorBalance(balance) => {
                self.u32(13);
                self.i32(balance.red_milli);
                self.i32(balance.green_milli);
                self.i32(balance.blue_milli);
            }
        }
        Ok(())
    }

    fn adjustment(&mut self, adjustment: &Adjustment) -> Result<(), CoreError> {
        match adjustment {
            Adjustment::BrightnessContrast {
                brightness_milli,
                contrast_milli,
            } => {
                self.u32(1);
                self.i32(*brightness_milli);
                self.i32(*contrast_milli);
            }
            Adjustment::ToneCurve {
                channel,
                interpolation,
                points,
            } => {
                self.u32(2);
                self.channel(*channel);
                self.interpolation(*interpolation);
                self.curve_points(points)?;
            }
            Adjustment::Levels(levels) => {
                self.u32(3);
                self.levels(levels);
            }
        }
        Ok(())
    }

    fn batch_color_pairs(&mut self, pairs: &[BatchColorPair]) -> Result<(), CoreError> {
        let count = u32::try_from(pairs.len())
            .map_err(|_| CoreError::InvalidArgument("too many canonical color pairs"))?;
        self.u32(count);
        for pair in pairs {
            self.boolean(pair.enabled);
            self.pixel(pair.old);
            self.pixel(pair.new);
        }
        Ok(())
    }

    fn batch_separation(&mut self, options: &BatchSeparation) -> Result<(), CoreError> {
        let count = u32::try_from(options.colors.len())
            .map_err(|_| CoreError::InvalidArgument("too many canonical separation colors"))?;
        self.u32(count);
        for color in &options.colors {
            self.pixel(*color);
        }
        self.pixel(options.replacement);
        self.boolean(options.invert);
        self.u32(match options.destination {
            BatchSeparationDestination::ReplaceSource => 1,
            BatchSeparationDestination::SelectionMask => 2,
            BatchSeparationDestination::MainLinePlane => 3,
            BatchSeparationDestination::ColorPlane => 4,
            BatchSeparationDestination::NativeFile => 5,
        });
        Ok(())
    }

    fn pixel_changes(&mut self, changes: &[PixelChange]) -> Result<(), CoreError> {
        let count = u32::try_from(changes.len())
            .map_err(|_| CoreError::InvalidArgument("too many canonical pixel changes"))?;
        self.u32(count);
        for change in changes {
            self.u32(change.x);
            self.u32(change.y);
            self.pixel(change.before);
            self.pixel(change.after);
        }
        Ok(())
    }

    fn editor_target(&mut self, target: EditorTarget) {
        self.u64(target.layer_id);
        self.u64(target.plane_id);
    }

    fn point(&mut self, point: PointF32) -> Result<(), CoreError> {
        self.q16_f32(point.x)?;
        self.q16_f32(point.y)
    }

    fn selection_shape(&mut self, shape: &SelectionShape) -> Result<(), CoreError> {
        match shape {
            SelectionShape::Rectangle(rect) => {
                self.u32(1);
                self.rect(*rect);
            }
            SelectionShape::Ellipse(rect) => {
                self.u32(2);
                self.rect(*rect);
            }
            SelectionShape::Lasso(points) => {
                self.u32(3);
                self.points(points)?;
            }
            SelectionShape::Polyline(points) => {
                self.u32(4);
                self.points(points)?;
            }
            SelectionShape::Trace { points, diameter } => {
                self.u32(5);
                self.points(points)?;
                self.q16_f32(*diameter)?;
            }
            SelectionShape::Wand {
                x,
                y,
                tolerance,
                gap_close,
            } => {
                self.u32(6);
                self.u32(*x);
                self.u32(*y);
                self.u16(*tolerance);
                self.u8(*gap_close);
            }
            SelectionShape::RectangleGesture { anchor, current } => {
                self.u32(7);
                self.point(*anchor)?;
                self.point(*current)?;
            }
            SelectionShape::EllipseGesture { anchor, current } => {
                self.u32(8);
                self.point(*anchor)?;
                self.point(*current)?;
            }
            SelectionShape::TraceBrush { samples, diameter } => {
                self.u32(9);
                let count = u32::try_from(samples.len()).map_err(|_| {
                    CoreError::InvalidArgument("canonical selection sample stream is too long")
                })?;
                self.u32(count);
                for sample in samples {
                    self.q16_f32(sample.x)?;
                    self.q16_f32(sample.y)?;
                    let pressure = canonical_unit_u16_from_f32(sample.pressure).ok_or(
                        CoreError::InvalidArgument("canonical selection pressure is invalid"),
                    )?;
                    self.u16(pressure);
                }
                self.q16_f32(*diameter)?;
            }
        }
        Ok(())
    }

    fn selection_construction_options(&mut self, options: SelectionConstructionOptions) {
        self.u32(options.aspect_ratio_q16);
        self.boolean(options.from_center);
        self.boolean(options.constrain_rotation_45);
        self.u32(options.rotation_turns);
        self.u32(match options.trace.shape {
            TraceBrushShape::Round => 1,
            TraceBrushShape::Square => 2,
        });
        self.boolean(options.trace.pressure_size);
        self.boolean(options.trace.screen_size);
        self.i64(options.trace.view_zoom_q16);
    }

    fn points(&mut self, points: &[PointF32]) -> Result<(), CoreError> {
        let count = u32::try_from(points.len())
            .map_err(|_| CoreError::InvalidArgument("canonical point stream is too long"))?;
        self.u32(count);
        for point in points {
            self.point(*point)?;
        }
        Ok(())
    }

    fn pixel(&mut self, value: PixelValue) {
        match value {
            PixelValue::Binary(value) => {
                self.u32(1);
                self.u8(value);
            }
            PixelValue::Grayscale8(value) => {
                self.u32(2);
                self.u8(value);
            }
            PixelValue::Grayscale16(value) => {
                self.u32(3);
                self.u16(value);
            }
            PixelValue::Rgba(value) => {
                self.u32(4);
                self.bytes.extend_from_slice(&value);
            }
            PixelValue::Rgba16(value) => {
                self.u32(5);
                for channel in value {
                    self.u16(channel);
                }
            }
        }
    }

    fn floating(&mut self, floating: &FloatingSelection) -> Result<(), CoreError> {
        match &floating.destination {
            FloatingDestination::ExistingPlanes(plane_ids) => {
                self.u8(0);
                self.u32(u32::try_from(plane_ids.len()).map_err(|_| {
                    CoreError::InvalidArgument("canonical destination count is not representable")
                })?);
                for plane_id in plane_ids {
                    self.u64(plane_id.get());
                }
            }
            FloatingDestination::NewPlane {
                layer_id,
                kind,
                format,
                name,
                opacity_milli,
            } => {
                self.u8(1);
                self.u64(layer_id.get());
                self.u32(plane_type_code(*kind));
                self.u32(pixel_format_code(*format));
                self.string(name)?;
                self.u32(*opacity_milli);
            }
        }
        self.u128(floating.payload.source_document_uuid);
        self.rect(floating.payload.bounds);
        let plane_count = u32::try_from(floating.payload.planes.len())
            .map_err(|_| CoreError::InvalidArgument("canonical clipboard has too many planes"))?;
        self.u32(plane_count);
        for plane in &floating.payload.planes {
            self.u32(plane_type_code(plane.kind));
            self.u32(pixel_format_code(plane.pixel_format));
            self.i32(plane.origin_x);
            self.i32(plane.origin_y);
            let pixel_count = u32::try_from(plane.pixels.len()).map_err(|_| {
                CoreError::InvalidArgument("canonical clipboard has too many pixels")
            })?;
            self.u32(pixel_count);
            for pixel in &plane.pixels {
                self.i32(pixel.x);
                self.i32(pixel.y);
                self.pixel(pixel.value);
            }
            self.u32(u32::try_from(plane.vector_paths.len()).map_err(|_| {
                CoreError::InvalidArgument("canonical clipboard has too many vector paths")
            })?);
            for path in &plane.vector_paths {
                self.u64(path.id);
                self.u64(path.plane_id);
                self.vector_path(&VectorPathInput {
                    segments: path.segments.clone(),
                    color: path.color,
                    closed: path.closed,
                })?;
                self.boolean(path.square_cross_section);
            }
            self.u32(u32::try_from(plane.vector_fills.len()).map_err(|_| {
                CoreError::InvalidArgument("canonical clipboard has too many vector fills")
            })?);
            for fill in &plane.vector_fills {
                self.u64(fill.id);
                self.u64(fill.plane_id);
                self.pixel(fill.color);
                self.ids(&fill.boundary_path_ids)?;
            }
        }
        self.q16_f64(floating.transform.translate_x)?;
        self.q16_f64(floating.transform.translate_y)?;
        self.q16_f64(floating.transform.scale_x)?;
        self.q16_f64(floating.transform.scale_y)?;
        self.u32(
            canonical_turns_from_degrees_f64(floating.transform.rotation_degrees).ok_or(
                CoreError::InvalidArgument("canonical rotation is not finite"),
            )?,
        );
        Ok(())
    }

    fn document_resize(&mut self, resize: DocumentResize) {
        self.u32(resize.width);
        self.u32(resize.height);
        self.u32(resize.dpi_x_milli);
        self.u32(resize.dpi_y_milli);
        self.u32(resize_anchor_code(resize.anchor));
        self.boolean(resize.resample);
    }

    fn ids(&mut self, ids: &[u64]) -> Result<(), CoreError> {
        let count = u32::try_from(ids.len())
            .map_err(|_| CoreError::InvalidArgument("too many canonical stable IDs"))?;
        self.u32(count);
        for id in ids {
            self.u64(*id);
        }
        Ok(())
    }

    fn geometry(&mut self, geometry: &CanonicalGeometry) -> Result<(), CoreError> {
        self.u64(geometry.plane_id);
        self.u32(match geometry.primitive {
            GeometryPrimitive::Line => 1,
            GeometryPrimitive::Curve => 2,
            GeometryPrimitive::Rectangle => 3,
            GeometryPrimitive::Ellipse => 4,
            GeometryPrimitive::Polygon => 5,
            GeometryPrimitive::Polyline => 6,
        });
        self.u32(
            u32::from(geometry.outline)
                | (u32::from(geometry.fill) << 1)
                | (u32::from(geometry.closed) << 2),
        );
        self.u32(match geometry.cross_section {
            GeometryCrossSection::Round => 1,
            GeometryCrossSection::Square => 2,
        });
        self.i64(geometry.outline_width_q16);
        self.pixel(geometry.outline_color);
        self.pixel(geometry.fill_color);
        self.u32(
            u32::try_from(geometry.segments.len()).map_err(|_| {
                CoreError::InvalidArgument("canonical geometry has too many segments")
            })?,
        );
        for segment in &geometry.segments {
            for point in [segment.p0, segment.p1, segment.p2, segment.p3] {
                self.i64(point.x_q16);
                self.i64(point.y_q16);
            }
            self.i64(segment.width_start_q16);
            self.i64(segment.width_end_q16);
        }
        self.u32(u32::try_from(geometry.fill_boundary.len()).map_err(|_| {
            CoreError::InvalidArgument("canonical geometry fill boundary is too large")
        })?);
        for point in &geometry.fill_boundary {
            self.i64(point.x_q16);
            self.i64(point.y_q16);
        }
        Ok(())
    }

    fn vector_path(&mut self, input: &VectorPathInput) -> Result<(), CoreError> {
        let count = u32::try_from(input.segments.len())
            .map_err(|_| CoreError::InvalidArgument("too many canonical vector segments"))?;
        self.u32(count);
        for segment in &input.segments {
            self.point(segment.p0)?;
            self.point(segment.p1)?;
            self.point(segment.p2)?;
            self.point(segment.p3)?;
            self.q16_f32(segment.width_start)?;
            self.q16_f32(segment.width_end)?;
        }
        self.pixel(input.color);
        self.boolean(input.closed);
        Ok(())
    }

    fn vector_width_mode(&mut self, mode: VectorWidthMode) -> Result<(), CoreError> {
        match mode {
            VectorWidthMode::Add(value) => {
                self.u32(1);
                self.q16_f32(value)?;
            }
            VectorWidthMode::Subtract(value) => {
                self.u32(2);
                self.q16_f32(value)?;
            }
            VectorWidthMode::Scale(value) => {
                self.u32(3);
                self.q16_f32(value)?;
            }
            VectorWidthMode::Constant(value) => {
                self.u32(4);
                self.q16_f32(value)?;
            }
        }
        Ok(())
    }

    fn light_table_source(&mut self, source: &LightTableSource) {
        self.u128(source.document_uuid);
        self.u64(source.source_revision);
        self.rect(source.reference_frame);
        self.u32(source.dpi_x_milli);
        self.u32(source.dpi_y_milli);
        self.asset_id(source.asset_id());
    }

    fn light_table_properties(&mut self, properties: LightTableItemProperties) {
        self.boolean(properties.visible);
        self.u32(properties.opacity_milli);
        self.u32(light_table_display_mode_code(properties.display_mode));
        self.pixel(properties.display_color);
        self.i32(properties.translate_x_milli);
        self.i32(properties.translate_y_milli);
        self.u32(properties.scale_x_milli);
        self.u32(properties.scale_y_milli);
        self.i32(properties.rotation_milli_degrees);
    }

    fn light_table_item(&mut self, input: &LightTableItemInput) -> Result<(), CoreError> {
        self.string(&input.name)?;
        self.light_table_source(&input.source);
        self.light_table_properties(LightTableItemProperties {
            visible: input.visible,
            opacity_milli: input.opacity_milli,
            display_mode: input.display_mode,
            display_color: input.display_color,
            translate_x_milli: input.translate_x_milli,
            translate_y_milli: input.translate_y_milli,
            scale_x_milli: input.scale_x_milli,
            scale_y_milli: input.scale_y_milli,
            rotation_milli_degrees: input.rotation_milli_degrees,
        });
        Ok(())
    }
}

fn write_edit_target_command(writer: &mut CanonicalWriter, command: EditTargetCommand) {
    match command {
        EditTargetCommand::Duplicate => writer.u32(1),
        EditTargetCommand::Delete => writer.u32(2),
        EditTargetCommand::SetVisibility(value) => {
            writer.u32(3);
            writer.boolean(value);
        }
        EditTargetCommand::SetEditability(value) => {
            writer.u32(4);
            writer.boolean(value);
        }
        EditTargetCommand::ConvertPlanes { kind, format } => {
            writer.u32(5);
            writer.u32(plane_type_code(kind));
            writer.u32(pixel_format_code(format));
        }
        EditTargetCommand::ConvertLayers { kind } => {
            writer.u32(6);
            writer.u32(layer_kind_code(kind));
        }
        EditTargetCommand::Merge => writer.u32(7),
    }
}

fn read_edit_target_command(
    reader: &mut CanonicalReader<'_>,
) -> Result<EditTargetCommand, CoreError> {
    match reader.u32()? {
        1 => Ok(EditTargetCommand::Duplicate),
        2 => Ok(EditTargetCommand::Delete),
        3 => Ok(EditTargetCommand::SetVisibility(reader.boolean()?)),
        4 => Ok(EditTargetCommand::SetEditability(reader.boolean()?)),
        5 => Ok(EditTargetCommand::ConvertPlanes {
            kind: reader.plane_type()?,
            format: reader.pixel_format()?,
        }),
        6 => Ok(EditTargetCommand::ConvertLayers {
            kind: reader.layer_kind()?,
        }),
        7 => Ok(EditTargetCommand::Merge),
        _ => Err(CoreError::Format(
            "unknown canonical edit-target command".to_owned(),
        )),
    }
}

const fn layer_kind_code(value: LayerKind) -> u32 {
    match value {
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

const fn plane_type_code(value: PlaneType) -> u32 {
    match value {
        PlaneType::MainLine => 1,
        PlaneType::Color => 2,
        PlaneType::Raster => 3,
        PlaneType::Selection => 4,
        PlaneType::VectorMainLine => 5,
        PlaneType::ColorTrace => 6,
        PlaneType::VectorFill => 7,
    }
}

const fn pixel_format_code(value: PixelFormat) -> u32 {
    match value {
        PixelFormat::BinaryMask8 => 1,
        PixelFormat::Grayscale8 => 2,
        PixelFormat::Grayscale16 => 3,
        PixelFormat::StraightRgba8 => 4,
        PixelFormat::StraightRgba16 => 5,
        PixelFormat::PremultipliedBgra8 => 6,
    }
}

const fn guide_axis_code(value: GuideAxis) -> u32 {
    match value {
        GuideAxis::Horizontal => 1,
        GuideAxis::Vertical => 2,
    }
}

const fn selection_operation_code(value: SelectionOperation) -> u32 {
    match value {
        SelectionOperation::New => 1,
        SelectionOperation::Add => 2,
        SelectionOperation::Subtract => 3,
        SelectionOperation::Intersect => 4,
    }
}

const fn range_interpretation_code(value: RangeInterpretation) -> u32 {
    match value {
        RangeInterpretation::Normal => 1,
        RangeInterpretation::Tight => 2,
        RangeInterpretation::EnclosedInterior => 3,
        RangeInterpretation::Drawing => 4,
        RangeInterpretation::Boundary => 5,
    }
}

const fn scoped_color_replace_mode_code(value: ScopedColorReplaceMode) -> u32 {
    match value {
        ScopedColorReplaceMode::RasterColor => 1,
        ScopedColorReplaceMode::RasterMainLine => 2,
        ScopedColorReplaceMode::VectorColorLine => 3,
        ScopedColorReplaceMode::VectorMainLine => 4,
        ScopedColorReplaceMode::VectorFill => 5,
    }
}

fn scoped_color_replace_mode(code: u32) -> Result<ScopedColorReplaceMode, CoreError> {
    match code {
        1 => Ok(ScopedColorReplaceMode::RasterColor),
        2 => Ok(ScopedColorReplaceMode::RasterMainLine),
        3 => Ok(ScopedColorReplaceMode::VectorColorLine),
        4 => Ok(ScopedColorReplaceMode::VectorMainLine),
        5 => Ok(ScopedColorReplaceMode::VectorFill),
        _ => Err(CoreError::Format(
            "unknown scoped color replacement mode".to_owned(),
        )),
    }
}

const fn selection_layer_operation_code(value: SelectionLayerOperation) -> u32 {
    match value {
        SelectionLayerOperation::Replace => 1,
        SelectionLayerOperation::Add => 2,
        SelectionLayerOperation::Subtract => 3,
    }
}

const fn fill_operation_code(value: FillOperation) -> u32 {
    match value {
        FillOperation::Seed => 1,
        FillOperation::ClosedRegion => 2,
        FillOperation::Extend => 3,
    }
}

const fn inclusion_mode_code(value: InclusionMode) -> u32 {
    match value {
        InclusionMode::None => 1,
        InclusionMode::Specified => 2,
        InclusionMode::ExceptSpecified => 3,
    }
}

const fn mirror_axis_code(value: MirrorAxis) -> u32 {
    match value {
        MirrorAxis::Horizontal => 1,
        MirrorAxis::Vertical => 2,
    }
}

const fn rotate_direction_code(value: RotateDirection) -> u32 {
    match value {
        RotateDirection::Left90 => 1,
        RotateDirection::Right90 => 2,
    }
}

const fn resize_anchor_code(value: ResizeAnchor) -> u32 {
    match value {
        ResizeAnchor::TopLeft => 1,
        ResizeAnchor::TopRight => 2,
        ResizeAnchor::Center => 3,
        ResizeAnchor::BottomLeft => 4,
        ResizeAnchor::BottomRight => 5,
    }
}

const fn vector_erase_mode_code(value: VectorEraseMode) -> u32 {
    match value {
        VectorEraseMode::Partial => 1,
        VectorEraseMode::ToIntersection => 2,
        VectorEraseMode::WholePath => 3,
    }
}

const fn light_table_display_mode_code(value: LightTableDisplayMode) -> u32 {
    match value {
        LightTableDisplayMode::Color => 1,
        LightTableDisplayMode::Monotone => 2,
        LightTableDisplayMode::Halftone => 3,
    }
}
