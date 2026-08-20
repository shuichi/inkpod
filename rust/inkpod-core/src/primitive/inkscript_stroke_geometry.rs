//! Private pre-ratification InkScript adapter for stroke, geometry, and raster import.

use super::inkscript_reference::{
    InkScriptEntityKind, InkScriptReferenceError, InkScriptRuntimeReferences,
};
use super::{CanonicalInvocation, CanonicalStrokeArguments};
use crate::geometry::{CanonicalGeometry, CanonicalGeometryPoint, CanonicalGeometrySegment};
use crate::{GeometryCrossSection, GeometryPrimitive, PixelValue};
use inkpod_format::{
    InkScriptCommandSchema, InkScriptEnumSchema, InkScriptFieldSchema, InkScriptRecordSchema,
    InkScriptTypedStep, InkScriptTypedValue, InkScriptTypedValueKind,
};
use std::collections::BTreeMap;

const MAX_STROKE_SAMPLES: usize = 1_048_576;
const MAX_GEOMETRY_SEGMENTS: usize = 512;
const MAX_GEOMETRY_BOUNDARY_POINTS: usize = 8_192;
const MAX_STROKE_DIAMETER_Q16: i64 = 256 * 65_536;
const MAX_GEOMETRY_WIDTH_Q16: i64 = 4_096 * 65_536;
const MAX_GEOMETRY_COORDINATE_Q16: i64 = 10_000_000 * 65_536;

pub(crate) const STROKE_GEOMETRY_ENUMS: &[InkScriptEnumSchema] = &[
    InkScriptEnumSchema::new("raster_stroke_tool", &["pencil", "brush", "eraser"]),
    InkScriptEnumSchema::new("raster_brush_shape", &["round", "square"]),
    InkScriptEnumSchema::new("raster_start_color", &["any", "exact_native"]),
    InkScriptEnumSchema::new(
        "geometry_primitive",
        &[
            "line",
            "curve",
            "rectangle",
            "ellipse",
            "polygon",
            "polyline",
        ],
    ),
    InkScriptEnumSchema::new("geometry_cross_section", &["round", "square"]),
];

const RASTER_STROKE_SAMPLE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("x", "q16", 0),
    InkScriptFieldSchema::required("y", "q16", 1),
    InkScriptFieldSchema::required("pressure", "u32", 2),
];
const CANONICAL_RASTER_STROKE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("tool", "raster_stroke_tool", 0),
    InkScriptFieldSchema::required("color", "pixel_value", 1),
    InkScriptFieldSchema::required("diameter", "q16", 2),
    InkScriptFieldSchema::required("shape", "raster_brush_shape", 3),
    InkScriptFieldSchema::required("smoothing", "u32", 4),
    InkScriptFieldSchema::required("start_color", "raster_start_color", 5),
    InkScriptFieldSchema::required("auto_erase", "bool", 6),
    InkScriptFieldSchema::required("pressure_size", "bool", 7),
    InkScriptFieldSchema::required("samples", "list<raster_stroke_sample>", 8),
];
const CANONICAL_GEOMETRY_SEGMENT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("p0", "point", 0),
    InkScriptFieldSchema::required("p1", "point", 1),
    InkScriptFieldSchema::required("p2", "point", 2),
    InkScriptFieldSchema::required("p3", "point", 3),
    InkScriptFieldSchema::required("width_start", "q16", 4),
    InkScriptFieldSchema::required("width_end", "q16", 5),
];

pub(crate) const STROKE_GEOMETRY_RECORDS: &[InkScriptRecordSchema] = &[
    InkScriptRecordSchema::new("raster_stroke_sample", RASTER_STROKE_SAMPLE_FIELDS),
    InkScriptRecordSchema::new("canonical_raster_stroke", CANONICAL_RASTER_STROKE_FIELDS),
    InkScriptRecordSchema::new(
        "canonical_geometry_segment",
        CANONICAL_GEOMETRY_SEGMENT_FIELDS,
    ),
];

const APPLY_RASTER_STROKE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("stroke", "canonical_raster_stroke", 1),
];
const APPLY_GEOMETRY_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("primitive", "geometry_primitive", 1),
    InkScriptFieldSchema::required("segments", "list<canonical_geometry_segment>", 2),
    InkScriptFieldSchema::required("fill_boundary", "list<point>", 3),
    InkScriptFieldSchema::required("outline_color", "pixel_value", 4),
    InkScriptFieldSchema::required("fill_color", "pixel_value", 5),
    InkScriptFieldSchema::required("outline_width", "q16", 6),
    InkScriptFieldSchema::required("cross_section", "geometry_cross_section", 7),
    InkScriptFieldSchema::required("outline", "bool", 8),
    InkScriptFieldSchema::required("fill", "bool", 9),
    InkScriptFieldSchema::required("closed", "bool", 10),
];
const IMPORT_RASTER_ASSET_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("raster", "asset_ref", 1),
];
pub(crate) const STROKE_GEOMETRY_COMMANDS: &[InkScriptCommandSchema] = &[
    InkScriptCommandSchema::new("apply_raster_stroke", APPLY_RASTER_STROKE_FIELDS),
    InkScriptCommandSchema::new("apply_geometry", APPLY_GEOMETRY_FIELDS),
    InkScriptCommandSchema::new("import_raster_asset", IMPORT_RASTER_ASSET_FIELDS),
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StrokeGeometryImportAction {
    RasterStroke(CanonicalStrokeArguments),
    Geometry(CanonicalInvocation),
    ImportRaster { plane_id: u64, asset_symbol: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StrokeGeometryImportAdapterError {
    InvalidTypedStep,
    InvalidValue,
    MissingReference,
    ResourceLimit,
    UnsupportedPrimitive,
}

impl StrokeGeometryImportAction {
    pub(crate) fn from_compiled(
        step: &InkScriptTypedStep,
        arguments: &InkScriptTypedValue,
        bindings: &InkScriptRuntimeReferences,
    ) -> Result<Self, StrokeGeometryImportAdapterError> {
        let fields = record(arguments)?;
        match step.command() {
            "apply_raster_stroke" => {
                let plane_id = plane_reference(field(fields, "plane_id")?, bindings)?;
                let stroke = raster_stroke(field(fields, "stroke")?, plane_id)?;
                Ok(Self::RasterStroke(stroke))
            }
            "apply_geometry" => {
                let plane_id = plane_reference(field(fields, "plane_id")?, bindings)?;
                Ok(Self::Geometry(CanonicalInvocation::ApplyGeometry {
                    geometry: canonical_geometry(fields, plane_id)?,
                }))
            }
            "import_raster_asset" => {
                let plane_id = plane_reference(field(fields, "plane_id")?, bindings)?;
                let asset_symbol = asset_reference(field(fields, "raster")?)?.to_owned();
                Ok(Self::ImportRaster {
                    plane_id,
                    asset_symbol,
                })
            }
            _ => Err(StrokeGeometryImportAdapterError::UnsupportedPrimitive),
        }
    }

    pub(crate) fn output_entity_kinds(
        &self,
        output_count: usize,
    ) -> Result<Vec<InkScriptEntityKind>, StrokeGeometryImportAdapterError> {
        match self {
            Self::Geometry(_) | Self::RasterStroke(_) | Self::ImportRaster { .. }
                if output_count == 0 =>
            {
                Ok(Vec::new())
            }
            Self::Geometry(_) | Self::RasterStroke(_) | Self::ImportRaster { .. } => {
                Err(StrokeGeometryImportAdapterError::InvalidValue)
            }
        }
    }
}

fn raster_stroke(
    value: &InkScriptTypedValue,
    target_plane_id: u64,
) -> Result<CanonicalStrokeArguments, StrokeGeometryImportAdapterError> {
    let fields = record(value)?;
    let samples = list(field(fields, "samples")?)?;
    if samples.is_empty() || samples.len() > MAX_STROKE_SAMPLES {
        return Err(StrokeGeometryImportAdapterError::ResourceLimit);
    }
    let mut canonical = Vec::new();
    reserve_exact(&mut canonical, samples.len())?;
    for sample in samples {
        let fields = record(sample)?;
        let x_q16 = q16(field(fields, "x")?)?;
        let y_q16 = q16(field(fields, "y")?)?;
        if x_q16.unsigned_abs() > MAX_GEOMETRY_COORDINATE_Q16 as u64
            || y_q16.unsigned_abs() > MAX_GEOMETRY_COORDINATE_Q16 as u64
        {
            return Err(StrokeGeometryImportAdapterError::InvalidValue);
        }
        canonical.push(super::raster::CanonicalStrokeSample {
            x_q16,
            y_q16,
            pressure: narrow_u16(field(fields, "pressure")?)?,
        });
    }
    let diameter_q16 = q16(field(fields, "diameter")?)?;
    if !(1..=MAX_STROKE_DIAMETER_Q16).contains(&diameter_q16) {
        return Err(StrokeGeometryImportAdapterError::InvalidValue);
    }
    let smoothing = narrow_u16(field(fields, "smoothing")?)?;
    if smoothing > 1_000 {
        return Err(StrokeGeometryImportAdapterError::InvalidValue);
    }
    Ok(CanonicalStrokeArguments {
        target_plane_id,
        tool_code: match enum_value(field(fields, "tool")?)? {
            "pencil" => 1,
            "brush" => 2,
            "eraser" => 3,
            _ => return Err(StrokeGeometryImportAdapterError::InvalidValue),
        },
        color: rgba_pixel(field(fields, "color")?)?,
        diameter_q16,
        shape_code: match enum_value(field(fields, "shape")?)? {
            "round" => 1,
            "square" => 2,
            _ => return Err(StrokeGeometryImportAdapterError::InvalidValue),
        },
        smoothing,
        start_color_code: match enum_value(field(fields, "start_color")?)? {
            "any" => 0,
            "exact_native" => 1,
            _ => return Err(StrokeGeometryImportAdapterError::InvalidValue),
        },
        auto_erase: boolean(field(fields, "auto_erase")?)?,
        pressure_size: boolean(field(fields, "pressure_size")?)?,
        payload: super::raster::encode_payload(&canonical)
            .map_err(|_| StrokeGeometryImportAdapterError::InvalidValue)?,
    })
}

fn canonical_geometry(
    fields: &BTreeMap<String, InkScriptTypedValue>,
    plane_id: u64,
) -> Result<CanonicalGeometry, StrokeGeometryImportAdapterError> {
    let segment_values = list(field(fields, "segments")?)?;
    if segment_values.is_empty() || segment_values.len() > MAX_GEOMETRY_SEGMENTS {
        return Err(StrokeGeometryImportAdapterError::ResourceLimit);
    }
    let mut segments = Vec::new();
    reserve_exact(&mut segments, segment_values.len())?;
    for segment in segment_values {
        let fields = record(segment)?;
        let width_start_q16 = q16(field(fields, "width_start")?)?;
        let width_end_q16 = q16(field(fields, "width_end")?)?;
        if !(1..=MAX_GEOMETRY_WIDTH_Q16).contains(&width_start_q16)
            || !(1..=MAX_GEOMETRY_WIDTH_Q16).contains(&width_end_q16)
        {
            return Err(StrokeGeometryImportAdapterError::InvalidValue);
        }
        segments.push(CanonicalGeometrySegment {
            p0: point(field(fields, "p0")?)?,
            p1: point(field(fields, "p1")?)?,
            p2: point(field(fields, "p2")?)?,
            p3: point(field(fields, "p3")?)?,
            width_start_q16,
            width_end_q16,
        });
    }
    let boundary_values = list(field(fields, "fill_boundary")?)?;
    if boundary_values.len() > MAX_GEOMETRY_BOUNDARY_POINTS {
        return Err(StrokeGeometryImportAdapterError::ResourceLimit);
    }
    let mut fill_boundary = Vec::new();
    reserve_exact(&mut fill_boundary, boundary_values.len())?;
    for value in boundary_values {
        fill_boundary.push(point(value)?);
    }
    let outline_width_q16 = q16(field(fields, "outline_width")?)?;
    if !(1..=MAX_GEOMETRY_WIDTH_Q16).contains(&outline_width_q16) {
        return Err(StrokeGeometryImportAdapterError::InvalidValue);
    }
    Ok(CanonicalGeometry {
        plane_id,
        primitive: match enum_value(field(fields, "primitive")?)? {
            "line" => GeometryPrimitive::Line,
            "curve" => GeometryPrimitive::Curve,
            "rectangle" => GeometryPrimitive::Rectangle,
            "ellipse" => GeometryPrimitive::Ellipse,
            "polygon" => GeometryPrimitive::Polygon,
            "polyline" => GeometryPrimitive::Polyline,
            _ => return Err(StrokeGeometryImportAdapterError::InvalidValue),
        },
        segments,
        fill_boundary,
        outline_color: rgba_pixel(field(fields, "outline_color")?)?,
        fill_color: rgba_pixel(field(fields, "fill_color")?)?,
        outline_width_q16,
        cross_section: match enum_value(field(fields, "cross_section")?)? {
            "round" => GeometryCrossSection::Round,
            "square" => GeometryCrossSection::Square,
            _ => return Err(StrokeGeometryImportAdapterError::InvalidValue),
        },
        outline: boolean(field(fields, "outline")?)?,
        fill: boolean(field(fields, "fill")?)?,
        closed: boolean(field(fields, "closed")?)?,
    })
}

fn record(
    value: &InkScriptTypedValue,
) -> Result<&BTreeMap<String, InkScriptTypedValue>, StrokeGeometryImportAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Record(fields) => Ok(fields),
        _ => Err(StrokeGeometryImportAdapterError::InvalidTypedStep),
    }
}

fn field<'a>(
    fields: &'a BTreeMap<String, InkScriptTypedValue>,
    name: &str,
) -> Result<&'a InkScriptTypedValue, StrokeGeometryImportAdapterError> {
    fields
        .get(name)
        .ok_or(StrokeGeometryImportAdapterError::InvalidTypedStep)
}

fn list(
    value: &InkScriptTypedValue,
) -> Result<&[InkScriptTypedValue], StrokeGeometryImportAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::List(values) => Ok(values),
        _ => Err(StrokeGeometryImportAdapterError::InvalidTypedStep),
    }
}

fn constructor<'a>(
    value: &'a InkScriptTypedValue,
    expected: &str,
) -> Result<&'a [InkScriptTypedValue], StrokeGeometryImportAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Constructor { name, arguments } if name == expected => {
            Ok(arguments)
        }
        _ => Err(StrokeGeometryImportAdapterError::InvalidValue),
    }
}

fn rgba_pixel(value: &InkScriptTypedValue) -> Result<PixelValue, StrokeGeometryImportAdapterError> {
    match value.type_name() {
        "rgba8" => {
            let values = constructor(value, "rgba8")?;
            Ok(PixelValue::Rgba([
                narrow_u8(&values[0])?,
                narrow_u8(&values[1])?,
                narrow_u8(&values[2])?,
                narrow_u8(&values[3])?,
            ]))
        }
        "rgba16" => {
            let values = constructor(value, "rgba16")?;
            Ok(PixelValue::Rgba16([
                narrow_u16(&values[0])?,
                narrow_u16(&values[1])?,
                narrow_u16(&values[2])?,
                narrow_u16(&values[3])?,
            ]))
        }
        _ => Err(StrokeGeometryImportAdapterError::InvalidValue),
    }
}

fn point(
    value: &InkScriptTypedValue,
) -> Result<CanonicalGeometryPoint, StrokeGeometryImportAdapterError> {
    let values = constructor(value, "point")?;
    let point = CanonicalGeometryPoint {
        x_q16: q16(&values[0])?,
        y_q16: q16(&values[1])?,
    };
    if point.x_q16.unsigned_abs() > MAX_GEOMETRY_COORDINATE_Q16 as u64
        || point.y_q16.unsigned_abs() > MAX_GEOMETRY_COORDINATE_Q16 as u64
    {
        return Err(StrokeGeometryImportAdapterError::InvalidValue);
    }
    Ok(point)
}

fn plane_reference(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
) -> Result<u64, StrokeGeometryImportAdapterError> {
    bindings
        .resolve(value, InkScriptEntityKind::Plane)
        .map_err(reference_error)
}

fn reference_error(error: InkScriptReferenceError) -> StrokeGeometryImportAdapterError {
    match error {
        InkScriptReferenceError::MissingReference => {
            StrokeGeometryImportAdapterError::MissingReference
        }
        InkScriptReferenceError::InvalidReference | InkScriptReferenceError::KindMismatch => {
            StrokeGeometryImportAdapterError::InvalidValue
        }
    }
}

fn asset_reference(value: &InkScriptTypedValue) -> Result<&str, StrokeGeometryImportAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::AssetReference(symbol) => Ok(symbol),
        _ => Err(StrokeGeometryImportAdapterError::InvalidValue),
    }
}

fn enum_value(value: &InkScriptTypedValue) -> Result<&str, StrokeGeometryImportAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Enum(value) => Ok(value),
        _ => Err(StrokeGeometryImportAdapterError::InvalidValue),
    }
}

fn boolean(value: &InkScriptTypedValue) -> Result<bool, StrokeGeometryImportAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Boolean(value) => Ok(*value),
        _ => Err(StrokeGeometryImportAdapterError::InvalidValue),
    }
}

fn q16(value: &InkScriptTypedValue) -> Result<i64, StrokeGeometryImportAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Q16(value) => Ok(*value),
        _ => Err(StrokeGeometryImportAdapterError::InvalidValue),
    }
}

fn narrow_u8(value: &InkScriptTypedValue) -> Result<u8, StrokeGeometryImportAdapterError> {
    u8::try_from(unsigned(value)?).map_err(|_| StrokeGeometryImportAdapterError::InvalidValue)
}

fn narrow_u16(value: &InkScriptTypedValue) -> Result<u16, StrokeGeometryImportAdapterError> {
    u16::try_from(unsigned(value)?).map_err(|_| StrokeGeometryImportAdapterError::InvalidValue)
}

fn unsigned(value: &InkScriptTypedValue) -> Result<u32, StrokeGeometryImportAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U32(value) => Ok(*value),
        _ => Err(StrokeGeometryImportAdapterError::InvalidValue),
    }
}

fn reserve_exact<T>(
    values: &mut Vec<T>,
    additional: usize,
) -> Result<(), StrokeGeometryImportAdapterError> {
    values
        .try_reserve_exact(additional)
        .map_err(|_| StrokeGeometryImportAdapterError::ResourceLimit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActivePlane, BrushShape, CoordinateSpace, Core, DEFAULT_DPI_MILLI, PaintTool,
        PrimitiveRequest, StartColorPredicate, Stroke, StrokeSample,
    };
    use inkpod_format::{
        InkScriptSchemaView, InkScriptSource, InkScriptSourceId, build_inkscript_declaration_model,
        parse_inkscript,
    };

    fn parsed_action(
        text: &str,
        plane_id: u64,
    ) -> Result<StrokeGeometryImportAction, StrokeGeometryImportAdapterError> {
        let source = InkScriptSource::new(InkScriptSourceId::new(217), text.as_bytes()).unwrap();
        let parsed = parse_inkscript(&source);
        assert!(parsed.is_valid(), "{:#?}", parsed.diagnostics());
        let schema = InkScriptSchemaView::exact_current_with_catalog(
            STROKE_GEOMETRY_ENUMS,
            &[],
            STROKE_GEOMETRY_RECORDS,
            STROKE_GEOMETRY_COMMANDS,
        )
        .unwrap();
        let model = build_inkscript_declaration_model(&parsed, &schema).unwrap();
        let mut references = InkScriptRuntimeReferences::default();
        references
            .insert("paint", InkScriptEntityKind::Plane, plane_id)
            .unwrap();
        StrokeGeometryImportAction::from_compiled(
            &model.steps()[0],
            model.steps()[0].arguments(),
            &references,
        )
    }

    fn action(text: &str, plane_id: u64) -> StrokeGeometryImportAction {
        parsed_action(text, plane_id).unwrap()
    }

    fn fragment(command: &str) -> String {
        format!(
            "inkscript_fragment 2;\nrequires {{ procedure_catalog = 3; replay_epoch = 24; }}\nbindings {{ let paint = select plane {{ source_document_uuid = uuid\"00000000-0000-0000-0000-000000000017\"; persistent_id = 1; }}; }}\nprogram {{ step \"Stroke geometry command\" {{ enabled = true; {command} }} }}\n"
        )
    }

    #[test]
    fn private_action_and_error_are_core_engine_send_sync_values() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<StrokeGeometryImportAction>();
        assert_send_sync::<StrokeGeometryImportAdapterError>();
    }

    #[test]
    fn raster_stroke_preserves_q16_native_color_and_sample_order() {
        let mut base = Core::new();
        base.new_cell(16, 16, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let target = base.document_info().unwrap().color_plane_id;
        let action = action(
            &fragment(
                "invoke apply_raster_stroke { plane_id = $paint; stroke = { tool = brush; color = rgba8(17, 34, 51, 255); diameter = q16(163840); shape = square; smoothing = 7; start_color = exact_native; auto_erase = false; pressure_size = true; samples = [{ x = q16(98304); y = q16(147456); pressure = 65535; }, { x = q16(229376); y = q16(147456); pressure = 32768; }]; }; };",
            ),
            target,
        );
        let StrokeGeometryImportAction::RasterStroke(arguments) = action else {
            panic!("stroke action expected")
        };
        let stroke = Stroke {
            tool: PaintTool::Brush,
            plane: ActivePlane::Color,
            color: [17, 34, 51, 255],
            diameter: 2.5,
            shape: BrushShape::Square,
            smoothing: 7,
            start_color: StartColorPredicate::ExactNative,
            auto_erase: false,
            pressure_size: true,
            coordinate_space: CoordinateSpace::Document,
            samples: vec![
                StrokeSample {
                    x: 1.5,
                    y: 2.25,
                    pressure: 1.0,
                },
                StrokeSample {
                    x: 3.5,
                    y: 2.25,
                    pressure: 32768.0 / 65535.0,
                },
            ],
        };
        let mut direct = base.clone();
        let expected_revision = direct.document_info().unwrap().document_revision;
        let direct_outcome = direct
            .execute_primitive(PrimitiveRequest::ApplyRasterStroke {
                expected_revision,
                target_plane_id: target,
                stroke,
            })
            .unwrap();
        let mut scripted = base;
        let scripted_outcome = scripted
            .execute_canonical_stroke_arguments(arguments)
            .unwrap();
        let direct_procedure = direct_outcome.procedure().unwrap();
        let scripted_procedure = scripted_outcome.procedure().unwrap();
        assert_eq!(
            direct_procedure.canonical_arguments(),
            scripted_procedure.canonical_arguments()
        );
        assert_eq!(
            direct_procedure.canonical_payload(),
            scripted_procedure.canonical_payload()
        );
        assert_eq!(direct_procedure.asset_ids(), scripted_procedure.asset_ids());
        assert_eq!(
            direct.document_state_digest().unwrap(),
            scripted.document_state_digest().unwrap()
        );
        assert_eq!(
            direct.document_info().unwrap(),
            scripted.document_info().unwrap()
        );
    }

    #[test]
    fn invalid_overflow_and_allocation_failures_precede_mutation() {
        let mut values = Vec::<u8>::new();
        assert_eq!(
            reserve_exact(&mut values, usize::MAX),
            Err(StrokeGeometryImportAdapterError::ResourceLimit)
        );
        assert!(values.is_empty());

        let mut core = Core::new();
        core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let target = core.document_info().unwrap().color_plane_id;
        let before = (
            core.document_state_digest().unwrap(),
            core.document_info().unwrap(),
            core.history_entries(),
        );
        let error = parsed_action(
            &fragment(
                "invoke apply_raster_stroke { plane_id = $paint; stroke = { tool = brush; color = rgba8(1, 2, 3, 255); diameter = q16(1); shape = round; smoothing = 0; start_color = any; auto_erase = false; pressure_size = false; samples = [{ x = q16(9223372036854775807); y = q16(0); pressure = 1; }]; }; };",
            ),
            target,
        )
        .unwrap_err();
        assert_eq!(error, StrokeGeometryImportAdapterError::InvalidValue);
        assert_eq!(
            (
                core.document_state_digest().unwrap(),
                core.document_info().unwrap(),
                core.history_entries(),
            ),
            before
        );
    }
}
