use std::collections::{BTreeMap, BTreeSet};

use super::catalog::{
    CatalogAssetSummary, CatalogError, CatalogWorkEstimate, InkScriptCatalogView,
    InkScriptPortability, InkScriptPortabilityClass,
};
use inkpod_format::{InkScriptAssertComparison, InkScriptSchemaView, InkScriptSelectorOwner};
use inkpod_format::{
    InkScriptDeclarationModel, InkScriptDependencyNodeKind, InkScriptSelectorCardinality,
    InkScriptSelectorMissingPolicy, InkScriptTypedAssert, InkScriptTypedBinding,
    InkScriptTypedProgramNode, InkScriptTypedValue, InkScriptTypedValueKind,
};

const ID_ALLOCATION_DIGEST_CONTEXT: &str = "inkpod.inkscript.id-allocation-digest.v1";

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct InkScriptEntityReference {
    pub(crate) entity: String,
    pub(crate) persistent_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum InkScriptComparableValue {
    Boolean(bool),
    U64(u64),
    I64(i64),
    Q16(i64),
    String(String),
    Uuid(String),
    Digest(String),
    Enum(String),
    Rect {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InkScriptEntitySnapshot {
    pub(crate) reference: InkScriptEntityReference,
    pub(crate) owner: Option<InkScriptEntityReference>,
    pub(crate) properties: BTreeMap<String, InkScriptComparableValue>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InkScriptSelectionSnapshot {
    pub(crate) empty: bool,
    pub(crate) bounds: Option<(i32, i32, u32, u32)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InkScriptInitialDocumentSnapshot {
    pub(crate) source_document_uuid: String,
    pub(crate) state_digest: String,
    pub(crate) id_allocations: Vec<(String, u64)>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) dpi_x: i64,
    pub(crate) dpi_y: i64,
    pub(crate) color_space: String,
    pub(crate) entities: Vec<InkScriptEntitySnapshot>,
    pub(crate) selection: InkScriptSelectionSnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum InkScriptBoundValue {
    One(InkScriptEntityReference),
    All(Vec<InkScriptEntityReference>),
    Skipped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InkScriptPreparedStatement {
    AssertPassed,
    AssertDeferred,
    StepReady,
    Disabled,
    Skipped,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InkScriptInitialPreparation {
    pub(crate) bindings: BTreeMap<String, InkScriptBoundValue>,
    pub(crate) statements: Vec<InkScriptPreparedStatement>,
    pub(crate) portability: InkScriptPortability,
    pub(crate) work: CatalogWorkEstimate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Stable failures while binding a compiled InkScript program to one initial document snapshot.
pub enum InkScriptBindingError {
    /// The captured initial snapshot violates the exact-current snapshot contract.
    InvalidSnapshot,
    /// A required selector matched no entity.
    MissingSelector,
    /// A scalar selector matched more than one entity.
    AmbiguousSelector,
    /// A selector result has the wrong owner relation.
    OwnerMismatch,
    /// An exact-source or state precondition is stale.
    StalePrecondition,
    /// An executable assertion failed.
    AssertFailed,
    /// A bound value has the wrong exact type.
    TypeMismatch,
    /// Checked binding or resource arithmetic overflowed.
    Overflow,
    /// The catalog rejected the binding or its resource formula.
    Catalog(CatalogError),
}

impl From<CatalogError> for InkScriptBindingError {
    fn from(value: CatalogError) -> Self {
        Self::Catalog(value)
    }
}

#[cfg(test)]
fn prepare_inkscript_initial_state(
    model: &InkScriptDeclarationModel,
    schema: &InkScriptSchemaView<'_>,
    catalog: &InkScriptCatalogView,
    snapshot: &InkScriptInitialDocumentSnapshot,
) -> Result<InkScriptInitialPreparation, InkScriptBindingError> {
    let values = model
        .parameters()
        .iter()
        .map(|parameter| {
            (
                parameter.name().to_owned(),
                parameter.default_value().clone(),
            )
        })
        .collect();
    let arguments = model
        .steps()
        .iter()
        .map(|step| step.arguments().clone())
        .collect::<Vec<_>>();
    prepare_inkscript_initial_state_with_parameters(
        model,
        schema,
        catalog,
        snapshot,
        &values,
        &arguments,
        &BTreeMap::new(),
    )
}

pub(crate) fn prepare_inkscript_initial_state_with_parameters(
    model: &InkScriptDeclarationModel,
    schema: &InkScriptSchemaView<'_>,
    catalog: &InkScriptCatalogView,
    snapshot: &InkScriptInitialDocumentSnapshot,
    parameter_values: &BTreeMap<String, InkScriptTypedValue>,
    frozen_arguments: &[InkScriptTypedValue],
    asset_summaries: &BTreeMap<String, CatalogAssetSummary>,
) -> Result<InkScriptInitialPreparation, InkScriptBindingError> {
    if frozen_arguments.len() != model.steps().len() {
        return Err(InkScriptBindingError::TypeMismatch);
    }
    validate_snapshot(schema, snapshot)?;
    let mut bindings = BTreeMap::new();
    for binding in model.bindings() {
        if selector_references_skipped_binding(binding.selector(), &bindings) {
            bindings.insert(binding.name().to_owned(), InkScriptBoundValue::Skipped);
            continue;
        }
        let parameter_refs = parameter_values
            .iter()
            .map(|(name, value)| (name.clone(), value))
            .collect::<BTreeMap<_, _>>();
        let value = resolve_binding(binding, snapshot, &parameter_refs, &bindings)?;
        bindings.insert(binding.name().to_owned(), value);
    }

    let mut skipped_bindings = bindings
        .iter()
        .filter_map(|(name, value)| {
            matches!(value, InkScriptBoundValue::Skipped).then_some(name.clone())
        })
        .collect::<BTreeSet<_>>();
    let mut skipped_steps = BTreeSet::new();
    let mut skipped_asserts = BTreeSet::new();
    loop {
        let before = skipped_bindings.len() + skipped_steps.len() + skipped_asserts.len();
        for edge in model.dependency_edges() {
            let dependency_skipped = match edge.dependency().kind() {
                InkScriptDependencyNodeKind::Binding => {
                    skipped_bindings.contains(edge.dependency().name())
                }
                InkScriptDependencyNodeKind::StepResult => edge
                    .dependency()
                    .step_index()
                    .is_some_and(|index| skipped_steps.contains(&index)),
                _ => false,
            };
            if !dependency_skipped {
                continue;
            }
            match edge.consumer().kind() {
                InkScriptDependencyNodeKind::Binding => {
                    skipped_bindings.insert(edge.consumer().name().to_owned());
                }
                InkScriptDependencyNodeKind::Assert => {
                    if let Some(index) = edge.consumer().program_index() {
                        skipped_asserts.insert(index);
                    }
                }
                InkScriptDependencyNodeKind::Step => {
                    if let Some(index) = edge.consumer().step_index() {
                        skipped_steps.insert(index);
                    }
                }
                _ => {}
            }
        }
        if before == skipped_bindings.len() + skipped_steps.len() + skipped_asserts.len() {
            break;
        }
    }

    let mut statements = Vec::with_capacity(model.program().len());
    let mut portability_class = InkScriptPortabilityClass::Portable;
    let mut preconditions = BTreeSet::new();
    let mut work = CatalogWorkEstimate {
        max_invocations: 0,
        max_output_ids: 0,
        max_asset_bytes: 0,
        max_work_units: 0,
        max_output_growth: 0,
    };
    for (program_index, node) in model.program().iter().enumerate() {
        match *node {
            InkScriptTypedProgramNode::Assert(index) => {
                let assertion = &model.assertions()[index];
                if skipped_asserts.contains(&assertion.program_index()) {
                    statements.push(InkScriptPreparedStatement::Skipped);
                } else {
                    statements.push(evaluate_assertion(assertion, snapshot, &bindings)?);
                }
            }
            InkScriptTypedProgramNode::Step(index) => {
                let step = &model.steps()[index];
                if !step.enabled() {
                    statements.push(InkScriptPreparedStatement::Disabled);
                    continue;
                }
                let index_u32 =
                    u32::try_from(index).map_err(|_| InkScriptBindingError::Overflow)?;
                if skipped_steps.contains(&index_u32) {
                    statements.push(InkScriptPreparedStatement::Skipped);
                    continue;
                }
                let entry = catalog.entry(step.command())?;
                let _editor_contract = entry.editor;
                let arguments = &frozen_arguments[index];
                let portability = catalog.evaluate_portability(step.command(), arguments)?;
                portability_class = portability_class.max(portability.class);
                preconditions.extend(portability.required_preconditions);
                add_work(
                    &mut work,
                    catalog.evaluate_work_with_assets(
                        step.command(),
                        arguments,
                        asset_summaries,
                    )?,
                )?;
                statements.push(InkScriptPreparedStatement::StepReady);
            }
        }
        debug_assert_eq!(statements.len(), program_index + 1);
    }
    Ok(InkScriptInitialPreparation {
        bindings,
        statements,
        portability: InkScriptPortability {
            class: portability_class,
            required_preconditions: preconditions.into_iter().collect(),
        },
        work,
    })
}

fn validate_snapshot(
    schema: &InkScriptSchemaView<'_>,
    snapshot: &InkScriptInitialDocumentSnapshot,
) -> Result<(), InkScriptBindingError> {
    if snapshot.width == 0
        || snapshot.height == 0
        || snapshot.dpi_x <= 0
        || snapshot.dpi_y <= 0
        || snapshot.selection.empty != snapshot.selection.bounds.is_none()
    {
        return Err(InkScriptBindingError::InvalidSnapshot);
    }
    let mut references = BTreeSet::new();
    for entity in &snapshot.entities {
        if entity.reference.persistent_id == 0 || !references.insert(entity.reference.clone()) {
            return Err(InkScriptBindingError::InvalidSnapshot);
        }
        let owner_relation = schema
            .selector_owner(&entity.reference.entity)
            .ok_or(InkScriptBindingError::InvalidSnapshot)?;
        let valid_owner = match owner_relation {
            InkScriptSelectorOwner::Document => entity.owner.is_none(),
            InkScriptSelectorOwner::Layer => entity
                .owner
                .as_ref()
                .is_some_and(|owner| owner.entity == "layer" && owner.persistent_id != 0),
            InkScriptSelectorOwner::Plane => entity
                .owner
                .as_ref()
                .is_some_and(|owner| owner.entity == "plane" && owner.persistent_id != 0),
            InkScriptSelectorOwner::LightTableSet => entity
                .owner
                .as_ref()
                .is_some_and(|owner| owner.entity == "light_table_set" && owner.persistent_id != 0),
        };
        if !valid_owner {
            return Err(InkScriptBindingError::OwnerMismatch);
        }
    }
    let _ = id_allocation_digest(schema, &snapshot.id_allocations)?;
    Ok(())
}

fn resolve_binding(
    binding: &InkScriptTypedBinding,
    snapshot: &InkScriptInitialDocumentSnapshot,
    parameters: &BTreeMap<String, &InkScriptTypedValue>,
    bindings: &BTreeMap<String, InkScriptBoundValue>,
) -> Result<InkScriptBoundValue, InkScriptBindingError> {
    let _initial_order = binding.initial_order();
    let fields = typed_record(binding.selector())?;
    if let Some(expected_uuid) = fields.get("source_document_uuid") {
        let expected_uuid = comparable_from_typed(expected_uuid, parameters)?;
        if expected_uuid != InkScriptComparableValue::Uuid(snapshot.source_document_uuid.clone()) {
            return Err(InkScriptBindingError::StalePrecondition);
        }
    }
    let owner_filter = owner_filter_name(binding.owner());
    let expected_owner = owner_filter
        .and_then(|name| fields.get(name))
        .map(|value| entity_reference_from_typed(value, bindings))
        .transpose()?;
    let mut matches = Vec::new();
    for entity in snapshot
        .entities
        .iter()
        .filter(|entity| entity.reference.entity == binding.entity())
    {
        if expected_owner
            .as_ref()
            .is_some_and(|owner| entity.owner.as_ref() != Some(owner))
        {
            continue;
        }
        if let Some(expected) = fields.get("persistent_id") {
            let InkScriptComparableValue::U64(expected) =
                comparable_from_typed(expected, parameters)?
            else {
                return Err(InkScriptBindingError::TypeMismatch);
            };
            if entity.reference.persistent_id != expected {
                continue;
            }
        }
        let mut matched = true;
        for (name, expected) in fields {
            if matches!(
                name.as_str(),
                "cardinality"
                    | "missing"
                    | "source_document_uuid"
                    | "persistent_id"
                    | "layer"
                    | "plane"
                    | "set"
            ) {
                continue;
            }
            let expected = comparable_from_typed(expected, parameters)?;
            if entity.properties.get(name) != Some(&expected) {
                matched = false;
                break;
            }
        }
        if matched {
            matches.push(entity.reference.clone());
        }
    }
    if matches.is_empty() {
        if fields.contains_key("persistent_id") {
            return Err(InkScriptBindingError::StalePrecondition);
        }
        return match binding.missing() {
            InkScriptSelectorMissingPolicy::Error => Err(InkScriptBindingError::MissingSelector),
            InkScriptSelectorMissingPolicy::SkipDependents => Ok(InkScriptBoundValue::Skipped),
        };
    }
    match binding.cardinality() {
        InkScriptSelectorCardinality::One if matches.len() == 1 => {
            Ok(InkScriptBoundValue::One(matches.remove(0)))
        }
        InkScriptSelectorCardinality::One => Err(InkScriptBindingError::AmbiguousSelector),
        InkScriptSelectorCardinality::First => Ok(InkScriptBoundValue::One(matches.remove(0))),
        InkScriptSelectorCardinality::All => Ok(InkScriptBoundValue::All(matches)),
    }
}

fn evaluate_assertion(
    assertion: &InkScriptTypedAssert,
    snapshot: &InkScriptInitialDocumentSnapshot,
    bindings: &BTreeMap<String, InkScriptBoundValue>,
) -> Result<InkScriptPreparedStatement, InkScriptBindingError> {
    let fields = typed_record(assertion.arguments())?;
    let passed = match assertion.comparison() {
        InkScriptAssertComparison::DocumentFields => {
            let id_digest = id_allocation_digest_from_snapshot(snapshot)?;
            fields.iter().all(|(name, expected)| {
                let actual = match name.as_str() {
                    "source_document_uuid" => {
                        InkScriptComparableValue::Uuid(snapshot.source_document_uuid.clone())
                    }
                    "state_digest" => {
                        InkScriptComparableValue::Digest(snapshot.state_digest.clone())
                    }
                    "id_allocation_digest" => InkScriptComparableValue::Digest(id_digest.clone()),
                    "width" => InkScriptComparableValue::U64(u64::from(snapshot.width)),
                    "height" => InkScriptComparableValue::U64(u64::from(snapshot.height)),
                    "dpi_x" => InkScriptComparableValue::Q16(snapshot.dpi_x),
                    "dpi_y" => InkScriptComparableValue::Q16(snapshot.dpi_y),
                    "color_space" => InkScriptComparableValue::Enum(snapshot.color_space.clone()),
                    _ => return false,
                };
                comparable_from_typed(expected, &BTreeMap::new())
                    .is_ok_and(|expected| expected == actual)
            })
        }
        InkScriptAssertComparison::ObjectProperties => {
            let Some(target) = fields.get("target") else {
                return Err(InkScriptBindingError::TypeMismatch);
            };
            let InkScriptTypedValueKind::Reference { root, .. } = target.kind() else {
                return Err(InkScriptBindingError::TypeMismatch);
            };
            let Some(bound) = bindings.get(root) else {
                return Ok(InkScriptPreparedStatement::AssertDeferred);
            };
            let InkScriptBoundValue::One(reference) = bound else {
                return Err(InkScriptBindingError::TypeMismatch);
            };
            let entity = snapshot
                .entities
                .iter()
                .find(|entity| entity.reference == *reference)
                .ok_or(InkScriptBindingError::StalePrecondition)?;
            fields
                .iter()
                .filter(|(name, _)| *name != "target")
                .all(|(name, expected)| {
                    comparable_from_typed(expected, &BTreeMap::new())
                        .is_ok_and(|expected| entity.properties.get(name) == Some(&expected))
                })
        }
        InkScriptAssertComparison::SelectionState => fields.iter().all(|(name, expected)| {
            let actual = match name.as_str() {
                "empty" => InkScriptComparableValue::Boolean(snapshot.selection.empty),
                "bounds" => match snapshot.selection.bounds {
                    Some((x, y, width, height)) => InkScriptComparableValue::Rect {
                        x,
                        y,
                        width,
                        height,
                    },
                    None => return matches!(expected.kind(), InkScriptTypedValueKind::None),
                },
                _ => return false,
            };
            comparable_from_typed(expected, &BTreeMap::new())
                .is_ok_and(|expected| expected == actual)
        }),
    };
    if passed {
        Ok(InkScriptPreparedStatement::AssertPassed)
    } else if assertion.kind() == "document"
        && fields.keys().any(|field| {
            matches!(
                field.as_str(),
                "source_document_uuid" | "state_digest" | "id_allocation_digest"
            )
        })
    {
        Err(InkScriptBindingError::StalePrecondition)
    } else {
        Err(InkScriptBindingError::AssertFailed)
    }
}

fn typed_record(
    value: &InkScriptTypedValue,
) -> Result<&BTreeMap<String, InkScriptTypedValue>, InkScriptBindingError> {
    match value.kind() {
        InkScriptTypedValueKind::Record(fields) => Ok(fields),
        _ => Err(InkScriptBindingError::TypeMismatch),
    }
}

fn comparable_from_typed(
    value: &InkScriptTypedValue,
    parameters: &BTreeMap<String, &InkScriptTypedValue>,
) -> Result<InkScriptComparableValue, InkScriptBindingError> {
    match value.kind() {
        InkScriptTypedValueKind::Boolean(value) => Ok(InkScriptComparableValue::Boolean(*value)),
        InkScriptTypedValueKind::U32(value) => Ok(InkScriptComparableValue::U64(u64::from(*value))),
        InkScriptTypedValueKind::U64(value) => Ok(InkScriptComparableValue::U64(*value)),
        InkScriptTypedValueKind::I32(value) => Ok(InkScriptComparableValue::I64(i64::from(*value))),
        InkScriptTypedValueKind::I64(value) => Ok(InkScriptComparableValue::I64(*value)),
        InkScriptTypedValueKind::Q16(value) => Ok(InkScriptComparableValue::Q16(*value)),
        InkScriptTypedValueKind::String(value) => {
            Ok(InkScriptComparableValue::String(value.clone()))
        }
        InkScriptTypedValueKind::Uuid(value) => Ok(InkScriptComparableValue::Uuid(value.clone())),
        InkScriptTypedValueKind::Digest(value) => {
            Ok(InkScriptComparableValue::Digest(value.clone()))
        }
        InkScriptTypedValueKind::Enum(value) => Ok(InkScriptComparableValue::Enum(value.clone())),
        InkScriptTypedValueKind::Constructor { name, arguments } if name == "rect" => {
            let [x, y, width, height] = arguments.as_slice() else {
                return Err(InkScriptBindingError::TypeMismatch);
            };
            let (
                InkScriptTypedValueKind::I32(x),
                InkScriptTypedValueKind::I32(y),
                InkScriptTypedValueKind::U32(width),
                InkScriptTypedValueKind::U32(height),
            ) = (x.kind(), y.kind(), width.kind(), height.kind())
            else {
                return Err(InkScriptBindingError::TypeMismatch);
            };
            Ok(InkScriptComparableValue::Rect {
                x: *x,
                y: *y,
                width: *width,
                height: *height,
            })
        }
        InkScriptTypedValueKind::Reference { root, segments } if segments.is_empty() => {
            if let Some(parameter) = parameters.get(root) {
                comparable_from_typed(parameter, parameters)
            } else {
                Err(InkScriptBindingError::TypeMismatch)
            }
        }
        InkScriptTypedValueKind::None => Err(InkScriptBindingError::TypeMismatch),
        _ => Err(InkScriptBindingError::TypeMismatch),
    }
}

fn entity_reference_from_typed(
    value: &InkScriptTypedValue,
    bindings: &BTreeMap<String, InkScriptBoundValue>,
) -> Result<InkScriptEntityReference, InkScriptBindingError> {
    let InkScriptTypedValueKind::Reference { root, segments } = value.kind() else {
        return Err(InkScriptBindingError::TypeMismatch);
    };
    if !segments.is_empty() {
        return Err(InkScriptBindingError::TypeMismatch);
    }
    match bindings.get(root) {
        Some(InkScriptBoundValue::One(reference)) => Ok(reference.clone()),
        _ => Err(InkScriptBindingError::TypeMismatch),
    }
}

fn selector_references_skipped_binding(
    selector: &InkScriptTypedValue,
    bindings: &BTreeMap<String, InkScriptBoundValue>,
) -> bool {
    match selector.kind() {
        InkScriptTypedValueKind::Reference { root, .. } => {
            matches!(bindings.get(root), Some(InkScriptBoundValue::Skipped))
        }
        InkScriptTypedValueKind::Constructor { arguments, .. }
        | InkScriptTypedValueKind::List(arguments) => arguments
            .iter()
            .any(|value| selector_references_skipped_binding(value, bindings)),
        InkScriptTypedValueKind::Record(fields) => fields
            .values()
            .any(|value| selector_references_skipped_binding(value, bindings)),
        _ => false,
    }
}

fn owner_filter_name(owner: InkScriptSelectorOwner) -> Option<&'static str> {
    match owner {
        InkScriptSelectorOwner::Document => None,
        InkScriptSelectorOwner::Layer => Some("layer"),
        InkScriptSelectorOwner::Plane => Some("plane"),
        InkScriptSelectorOwner::LightTableSet => Some("set"),
    }
}

fn add_work(
    total: &mut CatalogWorkEstimate,
    value: CatalogWorkEstimate,
) -> Result<(), InkScriptBindingError> {
    total.max_invocations = total
        .max_invocations
        .checked_add(value.max_invocations)
        .ok_or(InkScriptBindingError::Overflow)?;
    total.max_output_ids = total
        .max_output_ids
        .checked_add(value.max_output_ids)
        .ok_or(InkScriptBindingError::Overflow)?;
    total.max_asset_bytes = total
        .max_asset_bytes
        .checked_add(value.max_asset_bytes)
        .ok_or(InkScriptBindingError::Overflow)?;
    total.max_work_units = total
        .max_work_units
        .checked_add(value.max_work_units)
        .ok_or(InkScriptBindingError::Overflow)?;
    total.max_output_growth = total
        .max_output_growth
        .checked_add(value.max_output_growth)
        .ok_or(InkScriptBindingError::Overflow)?;
    Ok(())
}

pub(super) fn id_allocation_digest_from_snapshot(
    snapshot: &InkScriptInitialDocumentSnapshot,
) -> Result<String, InkScriptBindingError> {
    let mut payload = Vec::new();
    let count = u32::try_from(snapshot.id_allocations.len())
        .map_err(|_| InkScriptBindingError::Overflow)?;
    payload.extend_from_slice(&count.to_le_bytes());
    for (tag, next_id) in &snapshot.id_allocations {
        if *next_id == 0 || !tag.is_ascii() {
            return Err(InkScriptBindingError::InvalidSnapshot);
        }
        let length = u16::try_from(tag.len()).map_err(|_| InkScriptBindingError::Overflow)?;
        payload.extend_from_slice(&length.to_le_bytes());
        payload.extend_from_slice(tag.as_bytes());
        payload.extend_from_slice(&next_id.to_le_bytes());
    }
    Ok(blake3::derive_key(ID_ALLOCATION_DIGEST_CONTEXT, &payload)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn id_allocation_digest(
    schema: &InkScriptSchemaView<'_>,
    allocations: &[(String, u64)],
) -> Result<String, InkScriptBindingError> {
    if schema.id_namespaces().len() != allocations.len() {
        return Err(InkScriptBindingError::InvalidSnapshot);
    }
    let mut expected = schema.id_namespaces().iter().collect::<Vec<_>>();
    expected.sort_by_key(|namespace| namespace.order());
    if expected
        .iter()
        .zip(allocations)
        .any(|(namespace, (tag, _))| namespace.tag() != tag)
    {
        return Err(InkScriptBindingError::InvalidSnapshot);
    }
    let snapshot = InkScriptInitialDocumentSnapshot {
        source_document_uuid: String::new(),
        state_digest: String::new(),
        id_allocations: allocations.to_vec(),
        width: 1,
        height: 1,
        dpi_x: 1,
        dpi_y: 1,
        color_space: String::new(),
        entities: Vec::new(),
        selection: InkScriptSelectionSnapshot {
            empty: true,
            bounds: None,
        },
    };
    id_allocation_digest_from_snapshot(&snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::catalog::{
        CatalogBooleanExpression, CatalogCommandDomain, CatalogEditorMetadata, CatalogEntry,
        CatalogNumericExpression, CatalogPortabilityEvaluator, CatalogWorkFormula,
    };
    use inkpod_format::{
        InkScriptCommandSchema, InkScriptFieldSchema, InkScriptSchemaView, InkScriptSource,
        InkScriptSourceId, build_inkscript_declaration_model, parse_inkscript,
    };

    const USE_LAYER_FIELDS: &[InkScriptFieldSchema] =
        &[InkScriptFieldSchema::required("layer", "layer_ref", 0)];
    const TEST_COMMANDS: &[InkScriptCommandSchema] = &[
        InkScriptCommandSchema::new("use_layer", USE_LAYER_FIELDS),
        InkScriptCommandSchema::new("independent", &[]),
    ];

    fn schema() -> InkScriptSchemaView<'static> {
        InkScriptSchemaView::exact_current(&[], TEST_COMMANDS).unwrap()
    }

    fn model(text: &str) -> InkScriptDeclarationModel {
        let source = InkScriptSource::new(InkScriptSourceId::new(206), text.as_bytes()).unwrap();
        let parsed = parse_inkscript(&source);
        assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
        build_inkscript_declaration_model(&parsed, &schema()).unwrap()
    }

    fn catalog() -> InkScriptCatalogView {
        let entries = TEST_COMMANDS
            .iter()
            .copied()
            .map(|schema| CatalogEntry {
                schema,
                domain: CatalogCommandDomain::DocumentMutation,
                results: Vec::new(),
                assets: Vec::new(),
                portability: CatalogPortabilityEvaluator {
                    rules: vec![(
                        CatalogBooleanExpression::Literal(schema.name() == "use_layer"),
                        InkScriptPortability {
                            class: InkScriptPortabilityClass::RequiresBinding,
                            required_preconditions: vec!["semantic_target"],
                        },
                    )],
                    default: InkScriptPortability {
                        class: InkScriptPortabilityClass::Portable,
                        required_preconditions: Vec::new(),
                    },
                },
                work: CatalogWorkFormula {
                    max_invocations: CatalogNumericExpression::Literal(1),
                    max_output_ids: CatalogNumericExpression::Literal(0),
                    max_asset_bytes: CatalogNumericExpression::Literal(0),
                    max_work_units: CatalogNumericExpression::Literal(1),
                    max_output_growth: CatalogNumericExpression::Literal(0),
                },
                editor: CatalogEditorMetadata {
                    family: "test",
                    legacy_projection: None,
                    allow_skip_dependents: true,
                },
            })
            .collect();
        InkScriptCatalogView::test_only(entries).unwrap()
    }

    fn allocations() -> Vec<(String, u64)> {
        [
            "document_stable",
            "procedure",
            "state",
            "journal_event",
            "branch",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, tag)| (tag.to_owned(), u64::try_from(index).unwrap() + 10))
        .collect()
    }

    fn layer(id: u64, name: &str) -> InkScriptEntitySnapshot {
        InkScriptEntitySnapshot {
            reference: InkScriptEntityReference {
                entity: "layer".to_owned(),
                persistent_id: id,
            },
            owner: None,
            properties: BTreeMap::from([
                (
                    "kind".to_owned(),
                    InkScriptComparableValue::Enum("raster".to_owned()),
                ),
                (
                    "name".to_owned(),
                    InkScriptComparableValue::String(name.to_owned()),
                ),
                (
                    "visible".to_owned(),
                    InkScriptComparableValue::Boolean(true),
                ),
                (
                    "editable".to_owned(),
                    InkScriptComparableValue::Boolean(true),
                ),
                (
                    "layer_kind".to_owned(),
                    InkScriptComparableValue::Enum("raster".to_owned()),
                ),
            ]),
        }
    }

    fn snapshot() -> InkScriptInitialDocumentSnapshot {
        InkScriptInitialDocumentSnapshot {
            source_document_uuid: "00112233-4455-6677-8899-aabbccddeeff".to_owned(),
            state_digest: "1111111111111111111111111111111111111111111111111111111111111111"
                .to_owned(),
            id_allocations: allocations(),
            width: 1920,
            height: 1080,
            dpi_x: 72 * 65_536,
            dpi_y: 72 * 65_536,
            color_space: "srgb".to_owned(),
            entities: vec![layer(1, "A"), layer(2, "Duplicate"), layer(3, "Duplicate")],
            selection: InkScriptSelectionSnapshot {
                empty: false,
                bounds: Some((0, 0, 2, 2)),
            },
        }
    }

    #[test]
    fn one_first_all_and_ambiguity_use_initial_snapshot_order() {
        let declaration = model(
            r#"inkscript_fragment 2;
requires { procedure_catalog = 4; replay_epoch = 25; }
bindings {
    let one_value = select layer { name = "A"; cardinality = one; };
    let first_value = select layer { name = "Duplicate"; cardinality = first; };
    let all_values = select layer { name = "Duplicate"; cardinality = all; };
}
program { step "Independent" { enabled = true; invoke independent {}; } }
"#,
        );
        let prepared =
            prepare_inkscript_initial_state(&declaration, &schema(), &catalog(), &snapshot())
                .unwrap();
        assert!(matches!(
            prepared.bindings["one_value"],
            InkScriptBoundValue::One(InkScriptEntityReference {
                persistent_id: 1,
                ..
            })
        ));
        assert!(matches!(
            prepared.bindings["first_value"],
            InkScriptBoundValue::One(InkScriptEntityReference {
                persistent_id: 2,
                ..
            })
        ));
        assert!(matches!(
            &prepared.bindings["all_values"],
            InkScriptBoundValue::All(values)
                if values.iter().map(|value| value.persistent_id).collect::<Vec<_>>() == [2, 3]
        ));

        let ambiguous = model(
            r#"inkscript_fragment 2; requires { procedure_catalog = 4; replay_epoch = 25; }
bindings { let target = select layer { name = "Duplicate"; cardinality = one; }; }
program {}"#,
        );
        assert_eq!(
            prepare_inkscript_initial_state(&ambiguous, &schema(), &catalog(), &snapshot())
                .unwrap_err(),
            InkScriptBindingError::AmbiguousSelector
        );
    }

    #[test]
    fn skip_dependents_propagates_to_asserts_and_steps_but_not_static_disabled_state() {
        let declaration = model(
            r#"inkscript_fragment 2; requires { procedure_catalog = 4; replay_epoch = 25; }
bindings { let absent = select layer { name = "Absent"; missing = skip_dependents; }; }
program {
    assert object { target = $absent; visible = true; };
    step "Dependent" { enabled = true; invoke use_layer { layer = $absent; }; }
    step "Disabled dependent" { enabled = false; invoke use_layer { layer = $absent; }; }
    step "Independent" { enabled = true; invoke independent {}; }
}"#,
        );
        let unchanged = snapshot();
        let prepared =
            prepare_inkscript_initial_state(&declaration, &schema(), &catalog(), &unchanged)
                .unwrap();
        assert_eq!(
            prepared.statements,
            [
                InkScriptPreparedStatement::Skipped,
                InkScriptPreparedStatement::Skipped,
                InkScriptPreparedStatement::Disabled,
                InkScriptPreparedStatement::StepReady,
            ]
        );
        assert_eq!(prepared.work.max_invocations, 1);
        assert_eq!(unchanged, snapshot());
    }

    #[test]
    fn strict_selector_and_document_object_selection_asserts_are_atomic() {
        let current = snapshot();
        let digest = id_allocation_digest(&schema(), &current.id_allocations).unwrap();
        let declaration = model(&format!(
            r#"inkscript_fragment 2; requires {{ procedure_catalog = 4; replay_epoch = 25; }}
bindings {{ let target = select layer {{ source_document_uuid = uuid"00112233-4455-6677-8899-aabbccddeeff"; persistent_id = 1; }}; }}
program {{
    assert document {{ source_document_uuid = uuid"00112233-4455-6677-8899-aabbccddeeff"; state_digest = blake3"1111111111111111111111111111111111111111111111111111111111111111"; id_allocation_digest = blake3"{digest}"; width = 1920; }};
    assert object {{ target = $target; name = "A"; visible = true; layer_kind = raster; }};
    assert selection {{ empty = false; bounds = rect(0, 0, 2, 2); }};
    step "Independent" {{ enabled = true; invoke independent {{}}; }}
}}"#
        ));
        let prepared =
            prepare_inkscript_initial_state(&declaration, &schema(), &catalog(), &current).unwrap();
        assert_eq!(
            prepared.statements,
            [
                InkScriptPreparedStatement::AssertPassed,
                InkScriptPreparedStatement::AssertPassed,
                InkScriptPreparedStatement::AssertPassed,
                InkScriptPreparedStatement::StepReady,
            ]
        );

        let mut stale_uuid = current.clone();
        stale_uuid.source_document_uuid = "11112233-4455-6677-8899-aabbccddeeff".to_owned();
        let mut stale_id = current.clone();
        stale_id
            .entities
            .retain(|entity| entity.reference.persistent_id != 1);
        let mut stale_state = current.clone();
        stale_state.state_digest =
            "2222222222222222222222222222222222222222222222222222222222222222".to_owned();
        let mut stale_allocation = current.clone();
        stale_allocation.id_allocations[0].1 += 1;
        for stale in [stale_uuid, stale_id, stale_state, stale_allocation] {
            assert_eq!(
                prepare_inkscript_initial_state(&declaration, &schema(), &catalog(), &stale)
                    .unwrap_err(),
                InkScriptBindingError::StalePrecondition
            );
        }
        let mut object_mismatch = current.clone();
        object_mismatch.entities[0].properties.insert(
            "visible".to_owned(),
            InkScriptComparableValue::Boolean(false),
        );
        assert_eq!(
            prepare_inkscript_initial_state(&declaration, &schema(), &catalog(), &object_mismatch)
                .unwrap_err(),
            InkScriptBindingError::AssertFailed
        );
        let mut selection_mismatch = current.clone();
        selection_mismatch.selection.bounds = Some((0, 0, 3, 2));
        assert_eq!(
            prepare_inkscript_initial_state(
                &declaration,
                &schema(),
                &catalog(),
                &selection_mismatch
            )
            .unwrap_err(),
            InkScriptBindingError::AssertFailed
        );
        assert_eq!(current, snapshot());

        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<InkScriptInitialDocumentSnapshot>();
        assert_send_sync::<InkScriptInitialPreparation>();
    }

    #[test]
    fn missing_and_owner_mismatch_fail_without_publishing_partial_bindings() {
        let missing = model(
            r#"inkscript_fragment 2; requires { procedure_catalog = 4; replay_epoch = 25; }
bindings { let target = select layer { name = "Absent"; missing = error; }; }
program {}"#,
        );
        assert_eq!(
            prepare_inkscript_initial_state(&missing, &schema(), &catalog(), &snapshot())
                .unwrap_err(),
            InkScriptBindingError::MissingSelector
        );

        let mut invalid_owner = snapshot();
        invalid_owner.entities.push(InkScriptEntitySnapshot {
            reference: InkScriptEntityReference {
                entity: "plane".to_owned(),
                persistent_id: 9,
            },
            owner: Some(InkScriptEntityReference {
                entity: "guide".to_owned(),
                persistent_id: 1,
            }),
            properties: BTreeMap::new(),
        });
        let empty = model(
            "inkscript_fragment 2; requires { procedure_catalog = 4; replay_epoch = 25; } program {}",
        );
        assert_eq!(
            prepare_inkscript_initial_state(&empty, &schema(), &catalog(), &invalid_owner)
                .unwrap_err(),
            InkScriptBindingError::OwnerMismatch
        );
    }
}
