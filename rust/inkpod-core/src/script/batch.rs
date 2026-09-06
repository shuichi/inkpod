//! Typed Batch catalog bridge. Selectors bind once; the existing Batch executor owns edits.

use super::execute::ScriptRunError;
use crate::primitive::{InkScriptEntityKind, InkScriptRuntimeReferences, InvocationResult};
use crate::{
    BATCH_OPERATION_VERSION, BatchColorPair, BatchMissingTargetPolicy, BatchOperation,
    BatchOperationKind, BatchTargetSelector, Core, PlaneType,
};
use inkpod_format::{
    InkScriptCommandSchema, InkScriptEnumSchema, InkScriptFieldSchema, InkScriptRecordSchema,
    InkScriptTypedValue, InkScriptTypedValueKind,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const BATCH_ENUMS: &[InkScriptEnumSchema] = &[
    InkScriptEnumSchema::new(
        "batch_operation_kind",
        &["color_replace", "move_to_color_plane", "masking", "erase"],
    ),
    InkScriptEnumSchema::new("batch_target_kind", &["role", "strict", "references"]),
    InkScriptEnumSchema::new("batch_plane_kind", &["color", "raster"]),
    InkScriptEnumSchema::new("batch_missing_policy", &["error", "skip"]),
];
pub(super) const BATCH_RECORDS: &[InkScriptRecordSchema] = &[
    InkScriptRecordSchema::new(
        "batch_operation",
        &[
            InkScriptFieldSchema::required("kind", "batch_operation_kind", 0),
            InkScriptFieldSchema::required("enabled", "bool", 1),
            InkScriptFieldSchema::conditional(
                "targets",
                "list<batch_target>",
                2,
                &["present-for:kind=color_replace", "length:1..64"],
            ),
            InkScriptFieldSchema::conditional(
                "pairs",
                "list<batch_color_pair>",
                3,
                &["present-for:kind=color_replace", "length:1..4096"],
            ),
            InkScriptFieldSchema::conditional(
                "target",
                "batch_target",
                4,
                &["present-for:kind=move_to_color_plane,masking,erase"],
            ),
            InkScriptFieldSchema::conditional(
                "colors",
                "list<pixel_value>",
                5,
                &[
                    "present-for:kind=move_to_color_plane,masking,erase",
                    "length:1..4096",
                ],
            ),
        ],
    ),
    InkScriptRecordSchema::new(
        "batch_target",
        &[
            InkScriptFieldSchema::required("kind", "batch_target_kind", 0),
            InkScriptFieldSchema::conditional(
                "source_document_uuid",
                "uuid",
                1,
                &["present-for:kind=strict"],
            ),
            InkScriptFieldSchema::conditional(
                "persistent_layer_id",
                "nullable<u64>",
                2,
                &["present-for:kind=strict", "nonzero"],
            ),
            InkScriptFieldSchema::conditional(
                "persistent_plane_id",
                "nullable<u64>",
                3,
                &["present-for:kind=strict", "nonzero"],
            ),
            InkScriptFieldSchema::conditional(
                "plane_kind",
                "nullable<batch_plane_kind>",
                4,
                &["present-for:kind=role,strict"],
            ),
            InkScriptFieldSchema::conditional(
                "missing",
                "batch_missing_policy",
                5,
                &["present-for:kind=role,strict"],
            ),
            InkScriptFieldSchema::conditional(
                "layer",
                "nullable<layer_ref>",
                6,
                &["present-for:kind=references"],
            ),
            InkScriptFieldSchema::conditional(
                "plane",
                "plane_ref",
                7,
                &["present-for:kind=references"],
            ),
        ],
    ),
    InkScriptRecordSchema::new(
        "batch_color_pair",
        &[
            InkScriptFieldSchema::required("enabled", "bool", 0),
            InkScriptFieldSchema::required("old", "pixel_value", 1),
            InkScriptFieldSchema::required("new", "pixel_value", 2),
        ],
    ),
];
pub(super) const BATCH_COMMANDS: &[InkScriptCommandSchema] = &[InkScriptCommandSchema::new(
    "apply_batch_operations",
    &[
        InkScriptFieldSchema::required("operations", "list<batch_operation>", 0)
            .with_constraints(&["length:1..1024"]),
    ],
)];

#[derive(Clone)]
struct SourceOperation {
    enabled: bool,
    targets: Vec<InkScriptTypedValue>,
    kind: BatchOperationKind,
}

pub(super) struct BoundBatch {
    operations: Vec<BoundOperation>,
}

struct BoundOperation {
    targets: Vec<BoundTarget>,
    kind: BatchOperationKind,
}

enum BoundTarget {
    Fixed(Vec<BatchTargetSelector>),
    References {
        layer: InkScriptTypedValue,
        plane: InkScriptTypedValue,
    },
}

/// Checks every source operation, including disabled operations, without binding document IDs.
pub(super) fn validate_source(
    arguments: &InkScriptTypedValue,
    executable: bool,
) -> Result<(), ScriptRunError> {
    let operations = parse_operations(arguments)?;
    if executable && !operations.iter().any(|operation| operation.enabled) {
        return Err(ScriptRunError::InvalidStep);
    }
    Ok(())
}

fn parse_operations(
    arguments: &InkScriptTypedValue,
) -> Result<Vec<SourceOperation>, ScriptRunError> {
    let values = list(field(record(arguments)?, "operations")?)?;
    bounded(values.len(), 1, 1_024)?;
    let mut operations = Vec::new();
    operations
        .try_reserve_exact(values.len())
        .map_err(|_| ScriptRunError::ResourceLimit)?;
    for value in values {
        let fields = record(value)?;
        let enabled = boolean(field(fields, "enabled")?)?;
        let (targets, kind) = match enumeration(field(fields, "kind")?)? {
            "color_replace" => {
                let targets = list(field(fields, "targets")?)?;
                bounded(targets.len(), 1, 64)?;
                let values = list(field(fields, "pairs")?)?;
                bounded(values.len(), 1, 4_096)?;
                let mut pairs = Vec::new();
                pairs
                    .try_reserve_exact(values.len())
                    .map_err(|_| ScriptRunError::ResourceLimit)?;
                for value in values {
                    let pair = record(value)?;
                    pairs.push(BatchColorPair {
                        enabled: boolean(field(pair, "enabled")?)?,
                        old: pixel(field(pair, "old")?)?,
                        new: pixel(field(pair, "new")?)?,
                    });
                }
                (targets.to_vec(), BatchOperationKind::ColorReplace(pairs))
            }
            name @ ("move_to_color_plane" | "masking" | "erase") => {
                let values = list(field(fields, "colors")?)?;
                bounded(values.len(), 1, 4_096)?;
                let colors = values.iter().map(pixel).collect::<Result<Vec<_>, _>>()?;
                let kind = match name {
                    "move_to_color_plane" => BatchOperationKind::MoveToColorPlane(colors),
                    "masking" => BatchOperationKind::Masking(colors),
                    _ => BatchOperationKind::Erase(colors),
                };
                (vec![field(fields, "target")?.clone()], kind)
            }
            _ => return Err(ScriptRunError::InvalidStep),
        };
        for (index, target) in targets.iter().enumerate() {
            validate_target(target)?;
            if targets[..index].contains(target) {
                return Err(ScriptRunError::InvalidStep);
            }
        }
        // Color duplication and native pixel invariants have one owner in existing Batch code.
        crate::batch::validate_operation(&BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled,
            target: BatchTargetSelector::color_plane(),
            additional_targets: Vec::new(),
            kind: kind.clone(),
        })?;
        operations.push(SourceOperation {
            enabled,
            targets,
            kind,
        });
    }
    Ok(operations)
}

fn validate_target(target: &InkScriptTypedValue) -> Result<(), ScriptRunError> {
    let fields = record(target)?;
    match enumeration(field(fields, "kind")?)? {
        "role" => {
            plane_kind(field(fields, "plane_kind")?)?.ok_or(ScriptRunError::InvalidStep)?;
        }
        "strict" => {
            let layer = optional_id(field(fields, "persistent_layer_id")?)?;
            let plane = optional_id(field(fields, "persistent_plane_id")?)?;
            let kind = plane_kind(field(fields, "plane_kind")?)?;
            if (layer.is_none() && plane.is_none()) || (plane.is_none() && kind.is_none()) {
                return Err(ScriptRunError::InvalidStep);
            }
        }
        "references" => {}
        _ => return Err(ScriptRunError::InvalidStep),
    }
    Ok(())
}

/// Freezes role/strict selectors against the initial input. Producer references remain typed.
pub(super) fn bind(
    arguments: &InkScriptTypedValue,
    core: &Core,
    references: &InkScriptRuntimeReferences,
) -> Result<BoundBatch, ScriptRunError> {
    let mut operations = Vec::new();
    let mut count = 0usize;
    let mut work = 0u64;
    let dimensions = core.document_info()?;
    for source in parse_operations(arguments)?
        .into_iter()
        .filter(|operation| operation.enabled)
    {
        let mut targets = Vec::new();
        let mut seen = BTreeSet::new();
        for target in source.targets {
            let fields = record(&target)?;
            if enumeration(field(fields, "kind")?)? == "references" {
                let layer = field(fields, "layer")?.clone();
                let plane = field(fields, "plane")?.clone();
                match resolve_reference_target(core, references, &layer, &plane) {
                    Ok(target) => {
                        if seen.insert(target.plane_id) {
                            add_bound(&mut count, &mut work, target_work(core, &target)?)?;
                            targets.push(BoundTarget::Fixed(vec![target]));
                        }
                    }
                    Err(ScriptRunError::MissingResult) => {
                        add_bound(
                            &mut count,
                            &mut work,
                            u64::from(dimensions.width) * u64::from(dimensions.height),
                        )?;
                        targets.push(BoundTarget::References { layer, plane });
                    }
                    Err(error) => return Err(error),
                }
            } else {
                let resolved = resolve_initial_target(
                    core,
                    fields,
                    matches!(source.kind, BatchOperationKind::ColorReplace(_)),
                )?;
                let mut fixed = Vec::new();
                for target in resolved {
                    if seen.insert(target.plane_id) {
                        add_bound(&mut count, &mut work, target_work(core, &target)?)?;
                        fixed.push(target);
                    }
                }
                targets.push(BoundTarget::Fixed(fixed));
            }
        }
        operations.push(BoundOperation {
            targets,
            kind: source.kind,
        });
    }
    if operations.is_empty() {
        return Err(ScriptRunError::InvalidStep);
    }
    Ok(BoundBatch { operations })
}

impl BoundBatch {
    /// Resolves producer IDs and validates the entire expanded list before any Batch mutation.
    pub(super) fn execute(
        &self,
        core: &mut Core,
        references: &InkScriptRuntimeReferences,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<InvocationResult, ScriptRunError> {
        let mut operations = Vec::new();
        for operation in &self.operations {
            let mut seen = BTreeSet::new();
            for target in &operation.targets {
                if cancelled() {
                    return Err(ScriptRunError::Cancelled);
                }
                let resolved = match target {
                    BoundTarget::Fixed(values) => values.clone(),
                    BoundTarget::References { layer, plane } => {
                        vec![resolve_reference_target(core, references, layer, plane)?]
                    }
                };
                for target in resolved {
                    if !seen.insert(target.plane_id) {
                        continue;
                    }
                    if operations.len() == 1_024 {
                        return Err(ScriptRunError::ResourceLimit);
                    }
                    operations
                        .try_reserve(1)
                        .map_err(|_| ScriptRunError::ResourceLimit)?;
                    operations.push(BatchOperation {
                        version: BATCH_OPERATION_VERSION,
                        enabled: true,
                        target,
                        additional_targets: Vec::new(),
                        kind: operation.kind.clone(),
                    });
                }
            }
        }
        if cancelled() {
            return Err(ScriptRunError::Cancelled);
        }
        if operations.is_empty() {
            return Ok(InvocationResult::dispatch(core.noop_outcome()));
        }
        crate::batch::preflight_batch_operations(core, &operations)?;
        core.apply_batch_operations(&operations, cancelled)
            .map(InvocationResult::dispatch)
            .map_err(Into::into)
    }
}

fn resolve_initial_target(
    core: &Core,
    fields: &BTreeMap<String, InkScriptTypedValue>,
    all: bool,
) -> Result<Vec<BatchTargetSelector>, ScriptRunError> {
    let strict = enumeration(field(fields, "kind")?)? == "strict";
    let kind = plane_kind(field(fields, "plane_kind")?)?;
    let (layer_id, plane_id) = if strict {
        let InkScriptTypedValueKind::Uuid(uuid) = field(fields, "source_document_uuid")?.kind()
        else {
            return Err(ScriptRunError::InvalidStep);
        };
        let value = u128::from_str_radix(&uuid.replace('-', ""), 16)
            .map_err(|_| ScriptRunError::InvalidStep)?;
        if value != core.document_info()?.document_uuid {
            return Err(ScriptRunError::InvalidStep);
        }
        (
            optional_id(field(fields, "persistent_layer_id")?)?,
            optional_id(field(fields, "persistent_plane_id")?)?,
        )
    } else {
        (None, None)
    };
    let layers = core.layers()?;
    if let Some(plane_id) = plane_id {
        if let Some((owner, plane)) = layers.iter().find_map(|layer| {
            layer
                .planes
                .iter()
                .find(|plane| plane.id == plane_id)
                .map(|plane| (layer.id, plane))
        }) {
            if plane.kind == PlaneType::MainLine
                || layer_id.is_some_and(|id| id != owner)
                || kind.is_some_and(|kind| plane.kind != kind)
            {
                return Err(ScriptRunError::InvalidStep);
            }
        }
    }
    let mut result = Vec::new();
    for layer in &layers {
        if layer_id.is_some_and(|id| id != layer.id) {
            continue;
        }
        for plane in &layer.planes {
            if !matches!(plane.kind, PlaneType::Color | PlaneType::Raster)
                || plane_id.is_some_and(|id| id != plane.id)
                || kind.is_some_and(|kind| kind != plane.kind)
            {
                continue;
            }
            result.push(fixed_target(layer.id, plane.id, plane.kind));
            if !all {
                return Ok(result);
            }
        }
    }
    if result.is_empty() && enumeration(field(fields, "missing")?)? == "error" {
        return Err(ScriptRunError::InvalidStep);
    }
    Ok(result)
}

fn resolve_reference_target(
    core: &Core,
    references: &InkScriptRuntimeReferences,
    layer: &InkScriptTypedValue,
    plane: &InkScriptTypedValue,
) -> Result<BatchTargetSelector, ScriptRunError> {
    let plane_id = references
        .resolve(plane, InkScriptEntityKind::Plane)
        .map_err(|_| ScriptRunError::MissingResult)?;
    let layer_id = if matches!(layer.kind(), InkScriptTypedValueKind::None) {
        None
    } else {
        Some(
            references
                .resolve(layer, InkScriptEntityKind::Layer)
                .map_err(|_| ScriptRunError::MissingResult)?,
        )
    };
    for owner in core.layers()? {
        if let Some(plane) = owner.planes.iter().find(|plane| plane.id == plane_id) {
            if layer_id.is_some_and(|id| id != owner.id)
                || !matches!(plane.kind, PlaneType::Color | PlaneType::Raster)
            {
                return Err(ScriptRunError::InvalidStep);
            }
            return Ok(fixed_target(owner.id, plane_id, plane.kind));
        }
    }
    Err(ScriptRunError::InvalidStep)
}

fn fixed_target(layer: u64, plane: u64, kind: PlaneType) -> BatchTargetSelector {
    BatchTargetSelector {
        layer_id: Some(layer),
        plane_id: Some(plane),
        plane_kind: Some(kind),
        missing_policy: BatchMissingTargetPolicy::Error,
    }
}

fn target_work(core: &Core, target: &BatchTargetSelector) -> Result<u64, ScriptRunError> {
    let document = core.document.as_ref().ok_or(ScriptRunError::InvalidInput)?;
    let plane = document
        .plane_by_id(crate::identity::PlaneId::from_raw(
            target.plane_id.ok_or(ScriptRunError::InvalidStep)?,
        ))
        .ok_or(ScriptRunError::InvalidStep)?;
    Ok(u64::from(plane.raster.width()) * u64::from(plane.raster.height()))
}

fn add_bound(count: &mut usize, work: &mut u64, additional: u64) -> Result<(), ScriptRunError> {
    *count = count.checked_add(1).ok_or(ScriptRunError::ResourceLimit)?;
    *work = work
        .checked_add(additional)
        .ok_or(ScriptRunError::ResourceLimit)?;
    if *count > 1_024 || *work > crate::MAX_IMAGE_EDIT_PIXELS {
        return Err(ScriptRunError::ResourceLimit);
    }
    Ok(())
}

/// All dependency edges remain in the model/fragment closure. Only enabled nested operations
/// participate in runtime skip propagation.
pub(super) fn uses_runtime_dependency(arguments: &InkScriptTypedValue, name: &str) -> bool {
    let Ok(operations) = record(arguments)
        .and_then(|fields| field(fields, "operations"))
        .and_then(list)
    else {
        return true;
    };
    operations.iter().any(|operation| {
        record(operation).is_ok_and(|fields| {
            field(fields, "enabled").and_then(boolean).unwrap_or(true)
                && contains_reference(operation, name)
        })
    })
}

fn contains_reference(value: &InkScriptTypedValue, name: &str) -> bool {
    match value.kind() {
        InkScriptTypedValueKind::Reference { root, .. } => root == name,
        InkScriptTypedValueKind::Record(fields) => {
            fields.values().any(|value| contains_reference(value, name))
        }
        InkScriptTypedValueKind::List(values)
        | InkScriptTypedValueKind::Constructor {
            arguments: values, ..
        } => values.iter().any(|value| contains_reference(value, name)),
        _ => false,
    }
}

fn record(
    value: &InkScriptTypedValue,
) -> Result<&BTreeMap<String, InkScriptTypedValue>, ScriptRunError> {
    if let InkScriptTypedValueKind::Record(fields) = value.kind() {
        Ok(fields)
    } else {
        Err(ScriptRunError::InvalidStep)
    }
}
fn field<'a>(
    fields: &'a BTreeMap<String, InkScriptTypedValue>,
    name: &str,
) -> Result<&'a InkScriptTypedValue, ScriptRunError> {
    fields.get(name).ok_or(ScriptRunError::InvalidStep)
}
fn list(value: &InkScriptTypedValue) -> Result<&[InkScriptTypedValue], ScriptRunError> {
    if let InkScriptTypedValueKind::List(values) = value.kind() {
        Ok(values)
    } else {
        Err(ScriptRunError::InvalidStep)
    }
}
fn boolean(value: &InkScriptTypedValue) -> Result<bool, ScriptRunError> {
    if let InkScriptTypedValueKind::Boolean(value) = value.kind() {
        Ok(*value)
    } else {
        Err(ScriptRunError::InvalidStep)
    }
}
fn enumeration(value: &InkScriptTypedValue) -> Result<&str, ScriptRunError> {
    if let InkScriptTypedValueKind::Enum(value) = value.kind() {
        Ok(value)
    } else {
        Err(ScriptRunError::InvalidStep)
    }
}
fn optional_id(value: &InkScriptTypedValue) -> Result<Option<u64>, ScriptRunError> {
    match value.kind() {
        InkScriptTypedValueKind::None => Ok(None),
        InkScriptTypedValueKind::U64(value) if *value > 0 => Ok(Some(*value)),
        _ => Err(ScriptRunError::InvalidStep),
    }
}
fn plane_kind(value: &InkScriptTypedValue) -> Result<Option<PlaneType>, ScriptRunError> {
    if matches!(value.kind(), InkScriptTypedValueKind::None) {
        return Ok(None);
    }
    match enumeration(value)? {
        "color" => Ok(Some(PlaneType::Color)),
        "raster" => Ok(Some(PlaneType::Raster)),
        _ => Err(ScriptRunError::InvalidStep),
    }
}
fn pixel(value: &InkScriptTypedValue) -> Result<crate::PixelValue, ScriptRunError> {
    crate::primitive::inkscript_batch::pixel(value).map_err(|_| ScriptRunError::InvalidStep)
}
fn bounded(value: usize, minimum: usize, maximum: usize) -> Result<(), ScriptRunError> {
    if (minimum..=maximum).contains(&value) {
        Ok(())
    } else {
        Err(ScriptRunError::ResourceLimit)
    }
}
