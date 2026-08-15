//! Private pre-ratification InkScript adapter for document-tree primitives.

use super::CanonicalInvocation;
use super::inkscript_reference::{
    InkScriptEntityKind, InkScriptReferenceError, InkScriptRuntimeReferences,
};
use crate::{
    EditTarget, EditTargetCommand, EditorTarget, FrameMetadata, LayerKind, MAX_EDIT_TARGETS,
    Margins, PixelFormat, PlaneType, PrimitiveId, RectI32,
};
use inkpod_format::{
    InkScriptCommandResultSchema, InkScriptCommandSchema, InkScriptConstructorArgumentSchema,
    InkScriptConstructorSchema, InkScriptFieldSchema, InkScriptRecordSchema,
    InkScriptResultAvailability, InkScriptSchemaView, InkScriptSource, InkScriptSourceId,
    InkScriptTypeDiagnosticCode, InkScriptTypedStep, InkScriptTypedValue, InkScriptTypedValueKind,
    build_inkscript_declaration_model, parse_inkscript,
};
use std::collections::BTreeMap;

const ADAPTER_SOURCE_UUID: &str = "00000000-0000-0000-0000-000000000015";
const MAX_NODE_NAME_BYTES: usize = 1_024;

const FRAME_RECT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("x", "i32", 0),
    InkScriptFieldSchema::required("y", "i32", 1),
    InkScriptFieldSchema::required("width", "i32", 2),
    InkScriptFieldSchema::required("height", "i32", 3),
];
const PAPER_MARGINS_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("left", "u32", 0),
    InkScriptFieldSchema::required("top", "u32", 1),
    InkScriptFieldSchema::required("right", "u32", 2),
    InkScriptFieldSchema::required("bottom", "u32", 3),
];
const PAPER_FRAMES_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("hundred_frame", "frame_rect_i32", 0),
    InkScriptFieldSchema::required("reference_frame", "frame_rect_i32", 1),
    InkScriptFieldSchema::required("drawing_frame", "frame_rect_i32", 2),
    InkScriptFieldSchema::required("safe_frame", "frame_rect_i32", 3),
    InkScriptFieldSchema::required("shooting_frame", "frame_rect_i32", 4),
    InkScriptFieldSchema::required("maximum_close_frame", "frame_rect_i32", 5),
    InkScriptFieldSchema::required("margins", "paper_margins", 6),
];

pub(crate) const DOCUMENT_TREE_RECORDS: &[InkScriptRecordSchema] = &[
    InkScriptRecordSchema::new("frame_rect_i32", FRAME_RECT_FIELDS),
    InkScriptRecordSchema::new("paper_margins", PAPER_MARGINS_FIELDS),
    InkScriptRecordSchema::new("paper_frames", PAPER_FRAMES_FIELDS),
    InkScriptRecordSchema::new("edit_target", &[]),
    InkScriptRecordSchema::new("edit_target_command", &[]),
];

const LAYER_TARGET_ARGUMENTS: &[InkScriptConstructorArgumentSchema] =
    &[InkScriptConstructorArgumentSchema::new(
        "layer",
        "layer_ref",
        &[],
    )];
const PLANE_TARGET_ARGUMENTS: &[InkScriptConstructorArgumentSchema] = &[
    InkScriptConstructorArgumentSchema::new("layer", "layer_ref", &[]),
    InkScriptConstructorArgumentSchema::new("plane", "plane_ref", &[]),
];
const BOOLEAN_COMMAND_ARGUMENTS: &[InkScriptConstructorArgumentSchema] =
    &[InkScriptConstructorArgumentSchema::new(
        "value",
        "bool",
        &[],
    )];
const CONVERT_PLANE_COMMAND_ARGUMENTS: &[InkScriptConstructorArgumentSchema] = &[
    InkScriptConstructorArgumentSchema::new("kind", "plane_kind", &[]),
    InkScriptConstructorArgumentSchema::new("format", "pixel_format", &[]),
];
const CONVERT_LAYER_COMMAND_ARGUMENTS: &[InkScriptConstructorArgumentSchema] =
    &[InkScriptConstructorArgumentSchema::new(
        "kind",
        "layer_kind",
        &[],
    )];

pub(crate) const DOCUMENT_TREE_CONSTRUCTORS: &[InkScriptConstructorSchema] = &[
    InkScriptConstructorSchema::new("layer_target", "edit_target", LAYER_TARGET_ARGUMENTS),
    InkScriptConstructorSchema::new("plane_target", "edit_target", PLANE_TARGET_ARGUMENTS),
    InkScriptConstructorSchema::new("duplicate_targets", "edit_target_command", &[]),
    InkScriptConstructorSchema::new("delete_targets", "edit_target_command", &[]),
    InkScriptConstructorSchema::new(
        "set_target_visibility",
        "edit_target_command",
        BOOLEAN_COMMAND_ARGUMENTS,
    ),
    InkScriptConstructorSchema::new(
        "set_target_editability",
        "edit_target_command",
        BOOLEAN_COMMAND_ARGUMENTS,
    ),
    InkScriptConstructorSchema::new(
        "convert_target_planes",
        "edit_target_command",
        CONVERT_PLANE_COMMAND_ARGUMENTS,
    ),
    InkScriptConstructorSchema::new(
        "convert_target_layers",
        "edit_target_command",
        CONVERT_LAYER_COMMAND_ARGUMENTS,
    ),
    InkScriptConstructorSchema::new("merge_targets", "edit_target_command", &[]),
];

const UPDATE_PAPER_FRAMES_FIELDS: &[InkScriptFieldSchema] =
    &[InkScriptFieldSchema::required("frames", "paper_frames", 0)];
const CREATE_LAYER_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("kind", "layer_kind", 0),
    InkScriptFieldSchema::required("name", "string", 1),
];
const LAYER_ID_FIELDS: &[InkScriptFieldSchema] =
    &[InkScriptFieldSchema::required("layer_id", "layer_ref", 0)];
const REORDER_LAYER_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("layer_id", "layer_ref", 0),
    InkScriptFieldSchema::required("destination_index", "u64", 1),
];
const CREATE_PLANE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("layer_id", "layer_ref", 0),
    InkScriptFieldSchema::required("kind", "plane_kind", 1),
    InkScriptFieldSchema::required("format", "pixel_format", 2),
    InkScriptFieldSchema::required("name", "string", 3),
];
const PLANE_ID_FIELDS: &[InkScriptFieldSchema] =
    &[InkScriptFieldSchema::required("plane_id", "plane_ref", 0)];
const REORDER_PLANE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("destination_index", "u64", 1),
];
const EDIT_TARGETS_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("targets", "list<edit_target>", 0),
    InkScriptFieldSchema::required("command", "edit_target_command", 1),
];
const LAYER_RESULT: &[InkScriptCommandResultSchema] = &[InkScriptCommandResultSchema::scalar(
    "layer",
    "layer_ref",
    InkScriptResultAvailability::AlwaysOnSuccess,
    0,
)];
const PLANE_RESULT: &[InkScriptCommandResultSchema] = &[InkScriptCommandResultSchema::scalar(
    "plane",
    "plane_ref",
    InkScriptResultAvailability::AlwaysOnSuccess,
    0,
)];
const TARGET_RESULTS: &[InkScriptCommandResultSchema] = &[
    InkScriptCommandResultSchema::ordered_list(
        "layers",
        "layer_ref",
        InkScriptResultAvailability::AlwaysOnSuccess,
        0,
    ),
    InkScriptCommandResultSchema::ordered_list(
        "planes",
        "plane_ref",
        InkScriptResultAvailability::AlwaysOnSuccess,
        1,
    ),
];

pub(crate) const DOCUMENT_TREE_COMMANDS: &[InkScriptCommandSchema] = &[
    InkScriptCommandSchema::new("update_paper_frames", UPDATE_PAPER_FRAMES_FIELDS),
    InkScriptCommandSchema::with_results("create_layer", CREATE_LAYER_FIELDS, LAYER_RESULT),
    InkScriptCommandSchema::with_results("duplicate_layer", LAYER_ID_FIELDS, LAYER_RESULT),
    InkScriptCommandSchema::new("delete_layer", LAYER_ID_FIELDS),
    InkScriptCommandSchema::new("reorder_layer", REORDER_LAYER_FIELDS),
    InkScriptCommandSchema::with_results("create_plane", CREATE_PLANE_FIELDS, PLANE_RESULT),
    InkScriptCommandSchema::with_results("duplicate_plane", PLANE_ID_FIELDS, PLANE_RESULT),
    InkScriptCommandSchema::new("delete_plane", PLANE_ID_FIELDS),
    InkScriptCommandSchema::new("reorder_plane", REORDER_PLANE_FIELDS),
    InkScriptCommandSchema::new("merge_plane", PLANE_ID_FIELDS),
    InkScriptCommandSchema::new("merge_layer", LAYER_ID_FIELDS),
    InkScriptCommandSchema::new("delete_hidden_layers", &[]),
    InkScriptCommandSchema::with_results("edit_targets", EDIT_TARGETS_FIELDS, TARGET_RESULTS),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DocumentTreeCatalogEntry {
    pub(crate) command: &'static str,
    pub(crate) primitive_id: PrimitiveId,
    pub(crate) primitive_schema_version: u16,
    pub(crate) semantics_revision: u16,
    pub(crate) equivalence_test: &'static str,
}

pub(crate) const DOCUMENT_TREE_CATALOG: &[DocumentTreeCatalogEntry] = &[
    entry(
        "update_paper_frames",
        PrimitiveId::UPDATE_PAPER_FRAMES,
        2,
        "INKS-EQ-0001",
    ),
    entry("create_layer", PrimitiveId::CREATE_LAYER, 2, "INKS-EQ-0002"),
    entry(
        "duplicate_layer",
        PrimitiveId::DUPLICATE_LAYER,
        2,
        "INKS-EQ-0003",
    ),
    entry("delete_layer", PrimitiveId::DELETE_LAYER, 2, "INKS-EQ-0004"),
    entry(
        "reorder_layer",
        PrimitiveId::REORDER_LAYER,
        2,
        "INKS-EQ-0005",
    ),
    entry("create_plane", PrimitiveId::CREATE_PLANE, 2, "INKS-EQ-0007"),
    entry(
        "duplicate_plane",
        PrimitiveId::DUPLICATE_PLANE,
        2,
        "INKS-EQ-0008",
    ),
    entry("delete_plane", PrimitiveId::DELETE_PLANE, 2, "INKS-EQ-0009"),
    entry(
        "reorder_plane",
        PrimitiveId::REORDER_PLANE,
        2,
        "INKS-EQ-0010",
    ),
    entry("merge_plane", PrimitiveId::MERGE_PLANE, 2, "INKS-EQ-0013"),
    entry("merge_layer", PrimitiveId::MERGE_LAYER, 2, "INKS-EQ-0015"),
    entry(
        "delete_hidden_layers",
        PrimitiveId::DELETE_HIDDEN_LAYERS,
        2,
        "INKS-EQ-0016",
    ),
    entry("edit_targets", PrimitiveId::EDIT_TARGETS, 1, "INKS-EQ-0017"),
];

const fn entry(
    command: &'static str,
    primitive_id: PrimitiveId,
    semantics_revision: u16,
    equivalence_test: &'static str,
) -> DocumentTreeCatalogEntry {
    DocumentTreeCatalogEntry {
        command,
        primitive_id,
        primitive_schema_version: 2,
        semantics_revision,
        equivalence_test,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DocumentTreeAdapterError {
    InvalidSource,
    Type(InkScriptTypeDiagnosticCode),
    UnsupportedPrimitive,
    UnknownCommand,
    InvalidTypedStep,
    MissingReference,
    TargetMismatch,
    InvalidValue,
    ResourceLimit,
}

#[derive(Clone, Debug)]
pub(crate) struct DocumentTreeScriptStep {
    typed: InkScriptTypedStep,
    arguments: InkScriptTypedValue,
    references: InkScriptRuntimeReferences,
}

impl DocumentTreeScriptStep {
    pub(crate) fn from_canonical(
        invocation: &CanonicalInvocation,
    ) -> Result<Self, DocumentTreeAdapterError> {
        let mut source = String::from(
            "inkscript_fragment 1;\nrequires { procedure_catalog = 1; replay_epoch = 23; }\n",
        );
        let mut references = InkScriptRuntimeReferences::default();
        let (command, arguments, has_result) =
            lift_arguments(invocation, &mut source, &mut references)?;
        let result = if has_result { " as adapter_result" } else { "" };
        source.push_str(&format!(
            "program {{ step \"Canonical document tree adapter\"{result} {{ enabled = true; invoke {command} {{ {arguments} }}; }} }}\n"
        ));
        Self::from_source(&source, references)
    }

    fn from_source(
        source: &str,
        references: InkScriptRuntimeReferences,
    ) -> Result<Self, DocumentTreeAdapterError> {
        let source = InkScriptSource::new(InkScriptSourceId::new(15), source.as_bytes())
            .map_err(|_| DocumentTreeAdapterError::InvalidSource)?;
        let parsed = parse_inkscript(&source);
        if !parsed.is_valid() {
            return Err(DocumentTreeAdapterError::InvalidSource);
        }
        let schema = InkScriptSchemaView::exact_current_with_catalog(
            &[],
            DOCUMENT_TREE_CONSTRUCTORS,
            DOCUMENT_TREE_RECORDS,
            DOCUMENT_TREE_COMMANDS,
        )
        .map_err(|_| DocumentTreeAdapterError::InvalidTypedStep)?;
        let model = build_inkscript_declaration_model(&parsed, &schema)
            .map_err(|error| DocumentTreeAdapterError::Type(error.code()))?;
        if model.steps().len() != 1 || !model.steps()[0].enabled() {
            return Err(DocumentTreeAdapterError::InvalidTypedStep);
        }
        Ok(Self {
            arguments: model.steps()[0].arguments().clone(),
            typed: model.steps()[0].clone(),
            references,
        })
    }

    pub(crate) fn from_compiled(
        typed: &InkScriptTypedStep,
        arguments: InkScriptTypedValue,
        references: &InkScriptRuntimeReferences,
    ) -> Result<Self, DocumentTreeAdapterError> {
        Ok(Self {
            typed: typed.clone(),
            arguments,
            references: references.clone(),
        })
    }

    pub(crate) fn to_canonical(&self) -> Result<CanonicalInvocation, DocumentTreeAdapterError> {
        let arguments = record(&self.arguments)?;
        match self.typed.command() {
            "update_paper_frames" => Ok(CanonicalInvocation::UpdatePaperFrames {
                frames: paper_frames(field(arguments, "frames")?)?,
            }),
            "create_layer" => Ok(CanonicalInvocation::CreateLayer {
                kind: layer_kind(field(arguments, "kind")?)?,
                name: node_name(field(arguments, "name")?)?,
            }),
            "duplicate_layer" => Ok(CanonicalInvocation::DuplicateLayer {
                layer_id: entity_id(
                    field(arguments, "layer_id")?,
                    &self.references,
                    InkScriptEntityKind::Layer,
                )?,
            }),
            "delete_layer" => Ok(CanonicalInvocation::DeleteLayer {
                layer_id: entity_id(
                    field(arguments, "layer_id")?,
                    &self.references,
                    InkScriptEntityKind::Layer,
                )?,
            }),
            "reorder_layer" => Ok(CanonicalInvocation::ReorderLayer {
                layer_id: entity_id(
                    field(arguments, "layer_id")?,
                    &self.references,
                    InkScriptEntityKind::Layer,
                )?,
                destination_index: u64_value(field(arguments, "destination_index")?)?,
            }),
            "create_plane" => Ok(CanonicalInvocation::CreatePlane {
                layer_id: entity_id(
                    field(arguments, "layer_id")?,
                    &self.references,
                    InkScriptEntityKind::Layer,
                )?,
                kind: plane_kind(field(arguments, "kind")?)?,
                format: pixel_format(field(arguments, "format")?)?,
                name: node_name(field(arguments, "name")?)?,
            }),
            "duplicate_plane" => Ok(CanonicalInvocation::DuplicatePlane {
                plane_id: entity_id(
                    field(arguments, "plane_id")?,
                    &self.references,
                    InkScriptEntityKind::Plane,
                )?,
            }),
            "delete_plane" => Ok(CanonicalInvocation::DeletePlane {
                plane_id: entity_id(
                    field(arguments, "plane_id")?,
                    &self.references,
                    InkScriptEntityKind::Plane,
                )?,
            }),
            "reorder_plane" => Ok(CanonicalInvocation::ReorderPlane {
                plane_id: entity_id(
                    field(arguments, "plane_id")?,
                    &self.references,
                    InkScriptEntityKind::Plane,
                )?,
                destination_index: u64_value(field(arguments, "destination_index")?)?,
            }),
            "merge_plane" => Ok(CanonicalInvocation::MergePlane {
                plane_id: entity_id(
                    field(arguments, "plane_id")?,
                    &self.references,
                    InkScriptEntityKind::Plane,
                )?,
            }),
            "merge_layer" => Ok(CanonicalInvocation::MergeLayer {
                layer_id: entity_id(
                    field(arguments, "layer_id")?,
                    &self.references,
                    InkScriptEntityKind::Layer,
                )?,
            }),
            "delete_hidden_layers" => Ok(CanonicalInvocation::DeleteHiddenLayers),
            "edit_targets" => Ok(CanonicalInvocation::EditTargets {
                targets: edit_targets(field(arguments, "targets")?, &self.references)?,
                command: edit_target_command(field(arguments, "command")?)?,
            }),
            _ => Err(DocumentTreeAdapterError::UnknownCommand),
        }
    }

    pub(crate) fn output_entity_kinds(
        invocation: &CanonicalInvocation,
    ) -> Vec<InkScriptEntityKind> {
        match invocation {
            CanonicalInvocation::CreateLayer { .. }
            | CanonicalInvocation::DuplicateLayer { .. } => vec![InkScriptEntityKind::Layer],
            CanonicalInvocation::CreatePlane { .. }
            | CanonicalInvocation::DuplicatePlane { .. } => vec![InkScriptEntityKind::Plane],
            CanonicalInvocation::EditTargets { targets, command } => match command {
                EditTargetCommand::Duplicate => targets.iter().map(target_kind).collect(),
                EditTargetCommand::Merge => targets.first().map(target_kind).into_iter().collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }
}

type LiftedArguments = (&'static str, String, bool);

fn lift_arguments(
    invocation: &CanonicalInvocation,
    source: &mut String,
    references: &mut InkScriptRuntimeReferences,
) -> Result<LiftedArguments, DocumentTreeAdapterError> {
    Ok(match invocation {
        CanonicalInvocation::UpdatePaperFrames { frames } => (
            "update_paper_frames",
            format!("frames = {};", paper_frames_literal(*frames)),
            false,
        ),
        CanonicalInvocation::CreateLayer { kind, name } => {
            validate_node_name(name)?;
            (
                "create_layer",
                format!(
                    "kind = {}; name = {};",
                    layer_kind_name(*kind),
                    string_literal(name)
                ),
                true,
            )
        }
        CanonicalInvocation::DuplicateLayer { layer_id } => {
            bind(
                source,
                references,
                "target",
                InkScriptEntityKind::Layer,
                *layer_id,
            )?;
            ("duplicate_layer", "layer_id = $target;".to_owned(), true)
        }
        CanonicalInvocation::DeleteLayer { layer_id } => {
            bind(
                source,
                references,
                "target",
                InkScriptEntityKind::Layer,
                *layer_id,
            )?;
            ("delete_layer", "layer_id = $target;".to_owned(), false)
        }
        CanonicalInvocation::ReorderLayer {
            layer_id,
            destination_index,
        } => {
            bind(
                source,
                references,
                "target",
                InkScriptEntityKind::Layer,
                *layer_id,
            )?;
            (
                "reorder_layer",
                format!("layer_id = $target; destination_index = {destination_index};"),
                false,
            )
        }
        CanonicalInvocation::CreatePlane {
            layer_id,
            kind,
            format,
            name,
        } => {
            validate_node_name(name)?;
            bind(
                source,
                references,
                "owner",
                InkScriptEntityKind::Layer,
                *layer_id,
            )?;
            (
                "create_plane",
                format!(
                    "layer_id = $owner; kind = {}; format = {}; name = {};",
                    plane_kind_name(*kind),
                    pixel_format_name(*format)?,
                    string_literal(name)
                ),
                true,
            )
        }
        CanonicalInvocation::DuplicatePlane { plane_id } => {
            bind(
                source,
                references,
                "target",
                InkScriptEntityKind::Plane,
                *plane_id,
            )?;
            ("duplicate_plane", "plane_id = $target;".to_owned(), true)
        }
        CanonicalInvocation::DeletePlane { plane_id } => {
            bind(
                source,
                references,
                "target",
                InkScriptEntityKind::Plane,
                *plane_id,
            )?;
            ("delete_plane", "plane_id = $target;".to_owned(), false)
        }
        CanonicalInvocation::ReorderPlane {
            plane_id,
            destination_index,
        } => {
            bind(
                source,
                references,
                "target",
                InkScriptEntityKind::Plane,
                *plane_id,
            )?;
            (
                "reorder_plane",
                format!("plane_id = $target; destination_index = {destination_index};"),
                false,
            )
        }
        CanonicalInvocation::MergePlane { plane_id } => {
            bind(
                source,
                references,
                "target",
                InkScriptEntityKind::Plane,
                *plane_id,
            )?;
            ("merge_plane", "plane_id = $target;".to_owned(), false)
        }
        CanonicalInvocation::MergeLayer { layer_id } => {
            bind(
                source,
                references,
                "target",
                InkScriptEntityKind::Layer,
                *layer_id,
            )?;
            ("merge_layer", "layer_id = $target;".to_owned(), false)
        }
        CanonicalInvocation::DeleteHiddenLayers => ("delete_hidden_layers", String::new(), false),
        CanonicalInvocation::EditTargets { targets, command } => {
            if targets.is_empty() || targets.len() > MAX_EDIT_TARGETS {
                return Err(DocumentTreeAdapterError::ResourceLimit);
            }
            source.push_str("bindings { ");
            let mut values = Vec::with_capacity(targets.len());
            for (index, target) in targets.iter().enumerate() {
                match target {
                    EditTarget::Layer(layer_id) => {
                        let name = format!("target_{index}");
                        bind_declaration(
                            source,
                            references,
                            &name,
                            InkScriptEntityKind::Layer,
                            *layer_id,
                        )?;
                        values.push(format!("layer_target(${name})"));
                    }
                    EditTarget::Plane(target) => {
                        let owner = format!("owner_{index}");
                        let plane = format!("target_{index}");
                        bind_declaration(
                            source,
                            references,
                            &owner,
                            InkScriptEntityKind::Layer,
                            target.layer_id,
                        )?;
                        bind_declaration(
                            source,
                            references,
                            &plane,
                            InkScriptEntityKind::Plane,
                            target.plane_id,
                        )?;
                        values.push(format!("plane_target(${owner}, ${plane})"));
                    }
                }
            }
            source.push_str("}\n");
            (
                "edit_targets",
                format!(
                    "targets = [{}]; command = {};",
                    values.join(", "),
                    edit_target_command_literal(*command)?
                ),
                matches!(
                    command,
                    EditTargetCommand::Duplicate | EditTargetCommand::Merge
                ),
            )
        }
        _ => return Err(DocumentTreeAdapterError::UnsupportedPrimitive),
    })
}

fn bind(
    source: &mut String,
    references: &mut InkScriptRuntimeReferences,
    name: &str,
    kind: InkScriptEntityKind,
    persistent_id: u64,
) -> Result<(), DocumentTreeAdapterError> {
    source.push_str("bindings { ");
    bind_declaration(source, references, name, kind, persistent_id)?;
    source.push_str("}\n");
    Ok(())
}

fn bind_declaration(
    source: &mut String,
    references: &mut InkScriptRuntimeReferences,
    name: &str,
    kind: InkScriptEntityKind,
    persistent_id: u64,
) -> Result<(), DocumentTreeAdapterError> {
    let entity = match kind {
        InkScriptEntityKind::Layer => "layer",
        InkScriptEntityKind::Plane => "plane",
        InkScriptEntityKind::Guide
        | InkScriptEntityKind::VectorPath
        | InkScriptEntityKind::VectorFill => {
            return Err(DocumentTreeAdapterError::TargetMismatch);
        }
    };
    references
        .insert(name, kind, persistent_id)
        .map_err(reference_error)?;
    source.push_str(&format!(
        "let {name} = select {entity} {{ source_document_uuid = uuid\"{ADAPTER_SOURCE_UUID}\"; persistent_id = {persistent_id}; }}; "
    ));
    Ok(())
}

fn edit_targets(
    value: &InkScriptTypedValue,
    references: &InkScriptRuntimeReferences,
) -> Result<Vec<EditTarget>, DocumentTreeAdapterError> {
    let InkScriptTypedValueKind::List(values) = value.kind() else {
        return Err(DocumentTreeAdapterError::InvalidTypedStep);
    };
    if values.is_empty() || values.len() > MAX_EDIT_TARGETS {
        return Err(DocumentTreeAdapterError::ResourceLimit);
    }
    values
        .iter()
        .map(|value| {
            let (name, arguments) = constructor(value, "edit_target")?;
            match (name, arguments.as_slice()) {
                ("layer_target", [layer]) => Ok(EditTarget::Layer(entity_id(
                    layer,
                    references,
                    InkScriptEntityKind::Layer,
                )?)),
                ("plane_target", [layer, plane]) => Ok(EditTarget::Plane(EditorTarget {
                    layer_id: entity_id(layer, references, InkScriptEntityKind::Layer)?,
                    plane_id: entity_id(plane, references, InkScriptEntityKind::Plane)?,
                })),
                _ => Err(DocumentTreeAdapterError::InvalidTypedStep),
            }
        })
        .collect()
}

fn edit_target_command(
    value: &InkScriptTypedValue,
) -> Result<EditTargetCommand, DocumentTreeAdapterError> {
    let (name, arguments) = constructor(value, "edit_target_command")?;
    match (name, arguments.as_slice()) {
        ("duplicate_targets", []) => Ok(EditTargetCommand::Duplicate),
        ("delete_targets", []) => Ok(EditTargetCommand::Delete),
        ("set_target_visibility", [value]) => Ok(EditTargetCommand::SetVisibility(boolean(value)?)),
        ("set_target_editability", [value]) => {
            Ok(EditTargetCommand::SetEditability(boolean(value)?))
        }
        ("convert_target_planes", [kind, format]) => Ok(EditTargetCommand::ConvertPlanes {
            kind: plane_kind(kind)?,
            format: pixel_format(format)?,
        }),
        ("convert_target_layers", [kind]) => Ok(EditTargetCommand::ConvertLayers {
            kind: layer_kind(kind)?,
        }),
        ("merge_targets", []) => Ok(EditTargetCommand::Merge),
        _ => Err(DocumentTreeAdapterError::InvalidTypedStep),
    }
}

fn paper_frames(value: &InkScriptTypedValue) -> Result<FrameMetadata, DocumentTreeAdapterError> {
    if value.type_name() != "paper_frames" {
        return Err(DocumentTreeAdapterError::InvalidTypedStep);
    }
    let fields = record(value)?;
    Ok(FrameMetadata {
        hundred_frame: frame_rect(field(fields, "hundred_frame")?)?,
        reference_frame: frame_rect(field(fields, "reference_frame")?)?,
        drawing_frame: frame_rect(field(fields, "drawing_frame")?)?,
        safe_frame: frame_rect(field(fields, "safe_frame")?)?,
        shooting_frame: frame_rect(field(fields, "shooting_frame")?)?,
        maximum_close_frame: frame_rect(field(fields, "maximum_close_frame")?)?,
        margins: paper_margins(field(fields, "margins")?)?,
    })
}

fn frame_rect(value: &InkScriptTypedValue) -> Result<RectI32, DocumentTreeAdapterError> {
    if value.type_name() != "frame_rect_i32" {
        return Err(DocumentTreeAdapterError::InvalidTypedStep);
    }
    let fields = record(value)?;
    Ok(RectI32 {
        x: i32_value(field(fields, "x")?)?,
        y: i32_value(field(fields, "y")?)?,
        width: i32_value(field(fields, "width")?)?,
        height: i32_value(field(fields, "height")?)?,
    })
}

fn paper_margins(value: &InkScriptTypedValue) -> Result<Margins, DocumentTreeAdapterError> {
    if value.type_name() != "paper_margins" {
        return Err(DocumentTreeAdapterError::InvalidTypedStep);
    }
    let fields = record(value)?;
    Ok(Margins {
        left: u32_value(field(fields, "left")?)?,
        top: u32_value(field(fields, "top")?)?,
        right: u32_value(field(fields, "right")?)?,
        bottom: u32_value(field(fields, "bottom")?)?,
    })
}

fn record(
    value: &InkScriptTypedValue,
) -> Result<&BTreeMap<String, InkScriptTypedValue>, DocumentTreeAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Record(fields) => Ok(fields),
        _ => Err(DocumentTreeAdapterError::InvalidTypedStep),
    }
}

fn field<'a>(
    fields: &'a BTreeMap<String, InkScriptTypedValue>,
    name: &str,
) -> Result<&'a InkScriptTypedValue, DocumentTreeAdapterError> {
    fields
        .get(name)
        .ok_or(DocumentTreeAdapterError::InvalidTypedStep)
}

fn constructor<'a>(
    value: &'a InkScriptTypedValue,
    expected_type: &str,
) -> Result<(&'a str, &'a Vec<InkScriptTypedValue>), DocumentTreeAdapterError> {
    if value.type_name() != expected_type {
        return Err(DocumentTreeAdapterError::InvalidTypedStep);
    }
    match value.kind() {
        InkScriptTypedValueKind::Constructor { name, arguments } => Ok((name, arguments)),
        _ => Err(DocumentTreeAdapterError::InvalidTypedStep),
    }
}

fn entity_id(
    value: &InkScriptTypedValue,
    references: &InkScriptRuntimeReferences,
    expected: InkScriptEntityKind,
) -> Result<u64, DocumentTreeAdapterError> {
    references.resolve(value, expected).map_err(reference_error)
}

fn reference_error(error: InkScriptReferenceError) -> DocumentTreeAdapterError {
    match error {
        InkScriptReferenceError::InvalidReference => DocumentTreeAdapterError::InvalidTypedStep,
        InkScriptReferenceError::MissingReference => DocumentTreeAdapterError::MissingReference,
        InkScriptReferenceError::KindMismatch => DocumentTreeAdapterError::TargetMismatch,
    }
}

fn boolean(value: &InkScriptTypedValue) -> Result<bool, DocumentTreeAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Boolean(value) => Ok(*value),
        _ => Err(DocumentTreeAdapterError::InvalidTypedStep),
    }
}

fn u32_value(value: &InkScriptTypedValue) -> Result<u32, DocumentTreeAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U32(value) => Ok(*value),
        _ => Err(DocumentTreeAdapterError::InvalidTypedStep),
    }
}

fn i32_value(value: &InkScriptTypedValue) -> Result<i32, DocumentTreeAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::I32(value) => Ok(*value),
        _ => Err(DocumentTreeAdapterError::InvalidTypedStep),
    }
}

fn u64_value(value: &InkScriptTypedValue) -> Result<u64, DocumentTreeAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U64(value) => Ok(*value),
        _ => Err(DocumentTreeAdapterError::InvalidTypedStep),
    }
}

fn node_name(value: &InkScriptTypedValue) -> Result<String, DocumentTreeAdapterError> {
    let InkScriptTypedValueKind::String(value) = value.kind() else {
        return Err(DocumentTreeAdapterError::InvalidTypedStep);
    };
    validate_node_name(value)?;
    Ok(value.clone())
}

fn validate_node_name(name: &str) -> Result<(), DocumentTreeAdapterError> {
    if name.is_empty() || name.len() > MAX_NODE_NAME_BYTES || name.chars().any(char::is_control) {
        Err(DocumentTreeAdapterError::InvalidValue)
    } else {
        Ok(())
    }
}

fn layer_kind(value: &InkScriptTypedValue) -> Result<LayerKind, DocumentTreeAdapterError> {
    match enum_name(value)? {
        "binary_coloring" => Ok(LayerKind::BinaryColoring),
        "grayscale_coloring" => Ok(LayerKind::GrayscaleColoring),
        "raster" => Ok(LayerKind::Raster),
        "selection" => Ok(LayerKind::Selection),
        "frame" => Ok(LayerKind::Frame),
        "vanishing_point" => Ok(LayerKind::VanishingPoint),
        "adjustment" => Ok(LayerKind::Adjustment),
        "text" => Ok(LayerKind::Text),
        "annotation" => Ok(LayerKind::Annotation),
        "vector_coloring" => Ok(LayerKind::VectorColoring),
        _ => Err(DocumentTreeAdapterError::InvalidValue),
    }
}

fn plane_kind(value: &InkScriptTypedValue) -> Result<PlaneType, DocumentTreeAdapterError> {
    match enum_name(value)? {
        "main_line" => Ok(PlaneType::MainLine),
        "color" => Ok(PlaneType::Color),
        "raster" => Ok(PlaneType::Raster),
        "selection" => Ok(PlaneType::Selection),
        "vector_main_line" => Ok(PlaneType::VectorMainLine),
        "color_trace" => Ok(PlaneType::ColorTrace),
        "vector_fill" => Ok(PlaneType::VectorFill),
        _ => Err(DocumentTreeAdapterError::InvalidValue),
    }
}

fn pixel_format(value: &InkScriptTypedValue) -> Result<PixelFormat, DocumentTreeAdapterError> {
    match enum_name(value)? {
        "mask8" => Ok(PixelFormat::BinaryMask8),
        "gray8" => Ok(PixelFormat::Grayscale8),
        "gray16" => Ok(PixelFormat::Grayscale16),
        "rgba8" => Ok(PixelFormat::StraightRgba8),
        "rgba16" => Ok(PixelFormat::StraightRgba16),
        _ => Err(DocumentTreeAdapterError::InvalidValue),
    }
}

fn enum_name(value: &InkScriptTypedValue) -> Result<&str, DocumentTreeAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Enum(value) => Ok(value),
        _ => Err(DocumentTreeAdapterError::InvalidTypedStep),
    }
}

fn target_kind(target: &EditTarget) -> InkScriptEntityKind {
    match target {
        EditTarget::Layer(_) => InkScriptEntityKind::Layer,
        EditTarget::Plane(_) => InkScriptEntityKind::Plane,
    }
}

fn paper_frames_literal(frames: FrameMetadata) -> String {
    format!(
        "{{ hundred_frame = {}; reference_frame = {}; drawing_frame = {}; safe_frame = {}; shooting_frame = {}; maximum_close_frame = {}; margins = {{ left = {}; top = {}; right = {}; bottom = {}; }}; }}",
        frame_rect_literal(frames.hundred_frame),
        frame_rect_literal(frames.reference_frame),
        frame_rect_literal(frames.drawing_frame),
        frame_rect_literal(frames.safe_frame),
        frame_rect_literal(frames.shooting_frame),
        frame_rect_literal(frames.maximum_close_frame),
        frames.margins.left,
        frames.margins.top,
        frames.margins.right,
        frames.margins.bottom,
    )
}

fn frame_rect_literal(value: RectI32) -> String {
    format!(
        "{{ x = {}; y = {}; width = {}; height = {}; }}",
        value.x, value.y, value.width, value.height
    )
}

fn edit_target_command_literal(
    command: EditTargetCommand,
) -> Result<String, DocumentTreeAdapterError> {
    Ok(match command {
        EditTargetCommand::Duplicate => "duplicate_targets()".to_owned(),
        EditTargetCommand::Delete => "delete_targets()".to_owned(),
        EditTargetCommand::SetVisibility(value) => format!("set_target_visibility({value})"),
        EditTargetCommand::SetEditability(value) => format!("set_target_editability({value})"),
        EditTargetCommand::ConvertPlanes { kind, format } => format!(
            "convert_target_planes({}, {})",
            plane_kind_name(kind),
            pixel_format_name(format)?
        ),
        EditTargetCommand::ConvertLayers { kind } => {
            format!("convert_target_layers({})", layer_kind_name(kind))
        }
        EditTargetCommand::Merge => "merge_targets()".to_owned(),
    })
}

fn string_literal(value: &str) -> String {
    let mut result = String::with_capacity(value.len() + 2);
    result.push('"');
    for character in value.chars() {
        match character {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            _ => result.push(character),
        }
    }
    result.push('"');
    result
}

const fn layer_kind_name(value: LayerKind) -> &'static str {
    match value {
        LayerKind::BinaryColoring => "binary_coloring",
        LayerKind::GrayscaleColoring => "grayscale_coloring",
        LayerKind::Raster => "raster",
        LayerKind::Selection => "selection",
        LayerKind::Frame => "frame",
        LayerKind::VanishingPoint => "vanishing_point",
        LayerKind::Adjustment => "adjustment",
        LayerKind::Text => "text",
        LayerKind::Annotation => "annotation",
        LayerKind::VectorColoring => "vector_coloring",
    }
}

const fn plane_kind_name(value: PlaneType) -> &'static str {
    match value {
        PlaneType::MainLine => "main_line",
        PlaneType::Color => "color",
        PlaneType::Raster => "raster",
        PlaneType::Selection => "selection",
        PlaneType::VectorMainLine => "vector_main_line",
        PlaneType::ColorTrace => "color_trace",
        PlaneType::VectorFill => "vector_fill",
    }
}

fn pixel_format_name(value: PixelFormat) -> Result<&'static str, DocumentTreeAdapterError> {
    match value {
        PixelFormat::BinaryMask8 => Ok("mask8"),
        PixelFormat::Grayscale8 => Ok("gray8"),
        PixelFormat::Grayscale16 => Ok("gray16"),
        PixelFormat::StraightRgba8 => Ok("rgba8"),
        PixelFormat::StraightRgba16 => Ok("rgba16"),
        PixelFormat::PremultipliedBgra8 => Err(DocumentTreeAdapterError::InvalidValue),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitive::canonical_document_state;
    use crate::{Core, CoreError, DEFAULT_DPI_MILLI};

    fn core() -> Core {
        let mut core = Core::new();
        core.new_cell(8, 6, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core
    }

    fn execute_output(core: &mut Core, invocation: CanonicalInvocation) -> u64 {
        core.execute_canonical_invocation(invocation)
            .unwrap()
            .output_ids[0]
    }

    fn create_raster_layer(core: &mut Core, name: &str) -> u64 {
        execute_output(
            core,
            CanonicalInvocation::CreateLayer {
                kind: LayerKind::Raster,
                name: name.to_owned(),
            },
        )
    }

    fn create_raster_layer_and_plane(core: &mut Core, name: &str) -> (u64, u64) {
        let layer_id = create_raster_layer(core, name);
        let plane_id = core
            .layers()
            .unwrap()
            .into_iter()
            .find(|layer| layer.id == layer_id)
            .unwrap()
            .planes[0]
            .id;
        (layer_id, plane_id)
    }

    fn fixture(index: usize) -> (Core, CanonicalInvocation) {
        let mut core = core();
        let initial = core.layers().unwrap()[0].clone();
        let invocation = match index {
            0 => {
                let mut frames = core.document.as_ref().unwrap().frames;
                frames.margins.left = 1;
                CanonicalInvocation::UpdatePaperFrames { frames }
            }
            1 => CanonicalInvocation::CreateLayer {
                kind: LayerKind::Raster,
                name: "Created".to_owned(),
            },
            2 => CanonicalInvocation::DuplicateLayer {
                layer_id: initial.id,
            },
            3 => CanonicalInvocation::DeleteLayer {
                layer_id: create_raster_layer(&mut core, "Delete"),
            },
            4 => CanonicalInvocation::ReorderLayer {
                layer_id: create_raster_layer(&mut core, "Reorder"),
                destination_index: 0,
            },
            5 => {
                let layer_id = create_raster_layer(&mut core, "Plane owner");
                CanonicalInvocation::CreatePlane {
                    layer_id,
                    kind: PlaneType::Raster,
                    format: PixelFormat::StraightRgba8,
                    name: "Created plane".to_owned(),
                }
            }
            6 => {
                let (_, plane_id) = create_raster_layer_and_plane(&mut core, "Duplicate plane");
                CanonicalInvocation::DuplicatePlane { plane_id }
            }
            7 => {
                let (_, plane_id) = create_raster_layer_and_plane(&mut core, "Delete plane");
                CanonicalInvocation::DeletePlane {
                    plane_id: execute_output(
                        &mut core,
                        CanonicalInvocation::DuplicatePlane { plane_id },
                    ),
                }
            }
            8 => {
                let (_, plane_id) = create_raster_layer_and_plane(&mut core, "Reorder plane");
                CanonicalInvocation::ReorderPlane {
                    plane_id: execute_output(
                        &mut core,
                        CanonicalInvocation::DuplicatePlane { plane_id },
                    ),
                    destination_index: 0,
                }
            }
            9 => {
                let (_, plane_id) = create_raster_layer_and_plane(&mut core, "Merge plane");
                execute_output(&mut core, CanonicalInvocation::DuplicatePlane { plane_id });
                CanonicalInvocation::MergePlane { plane_id }
            }
            10 => {
                execute_output(
                    &mut core,
                    CanonicalInvocation::DuplicateLayer {
                        layer_id: initial.id,
                    },
                );
                CanonicalInvocation::MergeLayer {
                    layer_id: initial.id,
                }
            }
            11 => {
                let layer_id = create_raster_layer(&mut core, "Hidden");
                core.execute_canonical_invocation(CanonicalInvocation::SetLayerProperties {
                    layer_id,
                    visible: false,
                    editable: true,
                    opacity_milli: 1_000,
                    name: "Hidden".to_owned(),
                })
                .unwrap();
                CanonicalInvocation::DeleteHiddenLayers
            }
            12 => CanonicalInvocation::EditTargets {
                targets: vec![EditTarget::Layer(initial.id)],
                command: EditTargetCommand::Duplicate,
            },
            _ => panic!("unknown fixture"),
        };
        (core, invocation)
    }

    fn digest(core: &Core) -> crate::DocumentStateDigest {
        canonical_document_state(core.document.as_ref().unwrap())
            .unwrap()
            .1
    }

    #[test]
    fn exact_catalog_codec_and_executor_equivalence_cover_all_document_tree_primitives() {
        assert_eq!(DOCUMENT_TREE_CATALOG.len(), 13);
        for (index, metadata) in DOCUMENT_TREE_CATALOG.iter().enumerate() {
            let (direct_base, invocation) = fixture(index);
            let step = DocumentTreeScriptStep::from_canonical(&invocation)
                .unwrap_or_else(|error| panic!("codec fixture {index}: {error:?}"));
            let lowered = step.to_canonical().unwrap();
            assert_eq!(lowered, invocation, "codec fixture {index}");
            assert_eq!(metadata.command, step.typed.command());
            assert_eq!(metadata.primitive_id, invocation.primitive_id());
            assert_eq!(metadata.primitive_schema_version, 2);
            assert_eq!(
                metadata.semantics_revision,
                if metadata.command == "edit_targets" {
                    1
                } else {
                    2
                }
            );
            assert!(!metadata.equivalence_test.is_empty());

            let mut direct = direct_base.clone();
            let mut scripted = direct_base;
            let direct_result = direct.execute_canonical_invocation(invocation).unwrap();
            let scripted_result = scripted.execute_canonical_invocation(lowered).unwrap();
            assert_eq!(
                direct_result.output_ids, scripted_result.output_ids,
                "output fixture {index}"
            );
            assert_eq!(
                direct_result.dispatch.revision(),
                scripted_result.dispatch.revision(),
                "dispatch fixture {index}"
            );
            assert_eq!(digest(&direct), digest(&scripted), "digest fixture {index}");
            assert_eq!(direct.current_state, scripted.current_state);
            assert_eq!(direct.document_revision, scripted.document_revision);
            assert_eq!(direct.history_entries(), scripted.history_entries());
            assert_eq!(direct.next_id, scripted.next_id);
            assert_eq!(direct.savepoint, scripted.savepoint);
        }
        assert_eq!(
            DocumentTreeScriptStep::from_canonical(&CanonicalInvocation::ClearSelection)
                .unwrap_err(),
            DocumentTreeAdapterError::UnsupportedPrimitive
        );
    }

    #[test]
    fn no_op_invalid_stale_overflow_and_resource_failures_are_atomic() {
        let mut unchanged = core();
        let before = (
            digest(&unchanged),
            unchanged.document_revision,
            unchanged.history_entries(),
            unchanged.next_id,
            unchanged.current_state,
        );
        let no_op =
            DocumentTreeScriptStep::from_canonical(&CanonicalInvocation::DeleteHiddenLayers)
                .unwrap()
                .to_canonical()
                .unwrap();
        unchanged.execute_canonical_invocation(no_op).unwrap();
        assert_eq!(
            (
                digest(&unchanged),
                unchanged.document_revision,
                unchanged.history_entries(),
                unchanged.next_id,
                unchanged.current_state,
            ),
            before
        );

        assert_eq!(
            DocumentTreeScriptStep::from_canonical(&CanonicalInvocation::CreateLayer {
                kind: LayerKind::Raster,
                name: String::new(),
            })
            .unwrap_err(),
            DocumentTreeAdapterError::InvalidValue
        );
        let targets = vec![EditTarget::Layer(1); MAX_EDIT_TARGETS + 1];
        assert_eq!(
            DocumentTreeScriptStep::from_canonical(&CanonicalInvocation::EditTargets {
                targets,
                command: EditTargetCommand::Delete,
            })
            .unwrap_err(),
            DocumentTreeAdapterError::ResourceLimit
        );

        let valid = CanonicalInvocation::DeleteLayer { layer_id: 1 };
        let mut missing = DocumentTreeScriptStep::from_canonical(&valid).unwrap();
        missing.references.clear();
        assert_eq!(
            missing.to_canonical(),
            Err(DocumentTreeAdapterError::MissingReference)
        );
        let mut wrong_kind = DocumentTreeScriptStep::from_canonical(&valid).unwrap();
        wrong_kind.references.entry_mut("target").unwrap().kind = InkScriptEntityKind::Plane;
        assert_eq!(
            wrong_kind.to_canonical(),
            Err(DocumentTreeAdapterError::TargetMismatch)
        );

        for invalid in [
            CanonicalInvocation::DeleteLayer { layer_id: u64::MAX },
            CanonicalInvocation::ReorderLayer {
                layer_id: 1,
                destination_index: u64::MAX,
            },
        ] {
            let lowered = DocumentTreeScriptStep::from_canonical(&invalid)
                .unwrap()
                .to_canonical()
                .unwrap();
            let before = (
                digest(&unchanged),
                unchanged.document_revision,
                unchanged.history_entries(),
                unchanged.next_id,
                unchanged.current_state,
            );
            assert!(matches!(
                unchanged.execute_canonical_invocation(lowered),
                Err(CoreError::InvalidArgument(_))
            ));
            assert_eq!(
                (
                    digest(&unchanged),
                    unchanged.document_revision,
                    unchanged.history_entries(),
                    unchanged.next_id,
                    unchanged.current_state,
                ),
                before
            );
        }
    }

    #[test]
    fn private_adapter_values_are_send_sync_and_publish_no_executor() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DocumentTreeScriptStep>();
        assert_send_sync::<DocumentTreeCatalogEntry>();
        assert_eq!(DOCUMENT_TREE_COMMANDS.len(), DOCUMENT_TREE_CATALOG.len());
    }
}
