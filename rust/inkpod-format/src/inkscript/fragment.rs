use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::emit::emit_inkscript_canonical;
use super::names::InkScriptGeneratedNames;
use super::parser::{
    InkScriptDocumentKind, InkScriptParsed, MAX_INKSCRIPT_CONTAINER_ELEMENTS,
    MAX_INKSCRIPT_REFERENCE_SEGMENTS,
};
use super::schema::{InkScriptSchemaView, is_identifier};
use super::source::{InkScriptSource, MAX_INKSCRIPT_SOURCE_BYTES, MAX_INKSCRIPT_STRING_BYTES};
use super::syntax::{
    InkScriptAsset, InkScriptBinding, InkScriptParameter, InkScriptProgramStatement,
    InkScriptRecord, InkScriptReferenceSegment, InkScriptSemanticDocument,
    InkScriptSemanticSection, InkScriptValue, build_inkscript_semantic,
};
use super::types::{
    InkScriptDeclarationModel, InkScriptTypeDiagnostic, InkScriptTypeDiagnosticCode,
    build_inkscript_declaration_model, resolve_step_result_segments,
};

/// A bounded source-order selection for fragment closure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InkScriptFragmentSelection {
    StepRange { first: u32, last_inclusive: u32 },
    EditorGroup(String),
}

/// One explicit replacement for a stable result produced outside the selected mutation range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptExternalResultBinding {
    producer_alias: String,
    reference_segments: Vec<InkScriptReferenceSegment>,
    binding_name: String,
    selector_entity: String,
    selector_fields: Vec<(String, InkScriptValue)>,
}

impl InkScriptExternalResultBinding {
    pub fn new(
        producer_alias: impl Into<String>,
        reference_segments: Vec<InkScriptReferenceSegment>,
        binding_name: impl Into<String>,
        selector_entity: impl Into<String>,
        selector_fields: Vec<(String, InkScriptValue)>,
    ) -> Self {
        Self {
            producer_alias: producer_alias.into(),
            reference_segments,
            binding_name: binding_name.into(),
            selector_entity: selector_entity.into(),
            selector_fields,
        }
    }
}

/// Immutable closure and destination-collision inputs. No source or destination model is mutated.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptFragmentRequest {
    selection: InkScriptFragmentSelection,
    external_result_bindings: Vec<InkScriptExternalResultBinding>,
    reserved_value_names: Vec<String>,
    reserved_asset_names: Vec<String>,
    reserved_group_keys: Vec<String>,
}

impl InkScriptFragmentRequest {
    pub const fn new(selection: InkScriptFragmentSelection) -> Self {
        Self {
            selection,
            external_result_bindings: Vec::new(),
            reserved_value_names: Vec::new(),
            reserved_asset_names: Vec::new(),
            reserved_group_keys: Vec::new(),
        }
    }

    pub fn with_external_result_bindings(
        mut self,
        bindings: Vec<InkScriptExternalResultBinding>,
    ) -> Self {
        self.external_result_bindings = bindings;
        self
    }

    pub fn with_reserved_value_names(mut self, names: Vec<String>) -> Self {
        self.reserved_value_names = names;
        self
    }

    pub fn with_reserved_asset_names(mut self, names: Vec<String>) -> Self {
        self.reserved_asset_names = names;
        self
    }

    pub fn with_reserved_group_keys(mut self, keys: Vec<String>) -> Self {
        self.reserved_group_keys = keys;
        self
    }
}

/// One independently parseable and typed canonical fragment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptClosedFragment {
    canonical_bytes: Vec<u8>,
}

impl InkScriptClosedFragment {
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
}

/// Computes a closed fragment without implicitly importing a mutation outside the selected range.
/// External stable results require an explicit strict selector replacement. Failure and resource
/// rejection publish no partial output and never modify the parsed input.
pub fn close_inkscript_fragment(
    parsed: &InkScriptParsed<'_>,
    schema: &InkScriptSchemaView<'_>,
    request: &InkScriptFragmentRequest,
) -> Result<InkScriptClosedFragment, InkScriptTypeDiagnostic> {
    let model = build_inkscript_declaration_model(parsed, schema)?;
    let source_id = model.source_id();
    let document_range = model.document_range();
    validate_request_bounds(request, source_id, document_range)?;
    let semantic = build_inkscript_semantic(parsed, schema).map_err(|error| {
        InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::InvalidSemanticModel,
            source_id,
            document_range,
            error.path(),
        )
    })?;

    let selected = selected_steps(&model, &request.selection)?;
    let parts = SemanticParts::from_document(&semantic);
    let mut selected_program = parts
        .steps
        .iter()
        .enumerate()
        .filter(|(index, _)| selected.contains(index))
        .map(|(_, step)| (*step).clone())
        .collect::<Vec<_>>();

    let aliases = parts
        .steps
        .iter()
        .enumerate()
        .filter_map(|(index, step)| match step {
            InkScriptProgramStatement::Step {
                result_alias: Some(alias),
                ..
            } => Some((alias.clone(), index)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let strict_bindings = validate_external_bindings(
        request,
        &model,
        schema,
        &aliases,
        &selected,
        source_id,
        document_range,
    )?;
    let mut used_external = Vec::new();
    for statement in &mut selected_program {
        if let InkScriptProgramStatement::Step { arguments, .. } = statement {
            rewrite_external_references(
                arguments,
                &aliases,
                &selected,
                &strict_bindings,
                &mut used_external,
                source_id,
                &model,
            )?;
        }
    }
    if used_external.len() != strict_bindings.len() {
        return Err(InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::InvalidStrictBinding,
            source_id,
            document_range,
            "fragment.external_result_bindings.unused",
        ));
    }

    let mut required_parameters = BTreeSet::new();
    let mut required_bindings = BTreeSet::new();
    let mut required_assets = Vec::new();
    let mut queue = VecDeque::new();
    let parameter_names = parts
        .parameters
        .iter()
        .map(|parameter| parameter.name.as_str())
        .collect::<BTreeSet<_>>();
    let binding_by_name = parts
        .bindings
        .iter()
        .map(|binding| (binding.name.as_str(), *binding))
        .collect::<BTreeMap<_, _>>();
    let strict_by_name = used_external
        .iter()
        .map(|index| {
            let binding = &strict_bindings[*index];
            (binding.name.as_str(), binding)
        })
        .collect::<BTreeMap<_, _>>();
    let selected_aliases = selected
        .iter()
        .filter_map(|index| match parts.steps[*index] {
            InkScriptProgramStatement::Step {
                result_alias: Some(alias),
                ..
            } => Some(alias.as_str()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    for statement in &selected_program {
        if let InkScriptProgramStatement::Step { arguments, .. } = statement {
            collect_closure_dependencies(
                arguments,
                &parameter_names,
                &binding_by_name,
                &strict_by_name,
                &selected_aliases,
                &mut required_parameters,
                &mut required_bindings,
                &mut required_assets,
                &mut queue,
                source_id,
                document_range,
            )?;
        }
    }
    while let Some(name) = queue.pop_front() {
        if let Some(binding) = binding_by_name.get(name.as_str()) {
            collect_closure_dependencies(
                &binding.selector,
                &parameter_names,
                &binding_by_name,
                &strict_by_name,
                &selected_aliases,
                &mut required_parameters,
                &mut required_bindings,
                &mut required_assets,
                &mut queue,
                source_id,
                document_range,
            )?;
        }
    }

    let parameters = parts
        .parameters
        .iter()
        .filter(|parameter| required_parameters.contains(parameter.name.as_str()))
        .map(|parameter| (*parameter).clone())
        .collect::<Vec<_>>();
    let mut bindings = parts
        .bindings
        .iter()
        .filter(|binding| required_bindings.contains(binding.name.as_str()))
        .map(|binding| (*binding).clone())
        .collect::<Vec<_>>();
    bindings.extend(
        used_external
            .iter()
            .map(|index| strict_bindings[*index].binding.clone()),
    );
    let (assets, asset_dedup) =
        close_assets(&required_assets, &parts.assets, source_id, document_range)?;
    rewrite_assets_in_bindings_and_program(&mut bindings, &mut selected_program, &asset_dedup);

    let requires = parts.requires.cloned().ok_or_else(|| {
        InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::InvalidSemanticModel,
            source_id,
            document_range,
            "fragment.requires",
        )
    })?;
    let mut sections = vec![InkScriptSemanticSection::Requires(requires)];
    if !parameters.is_empty() {
        sections.push(InkScriptSemanticSection::Parameters(parameters));
    }
    if !bindings.is_empty() {
        sections.push(InkScriptSemanticSection::Bindings(bindings));
    }
    sections.push(InkScriptSemanticSection::Program(selected_program));
    if !assets.is_empty() {
        sections.push(InkScriptSemanticSection::Assets(assets));
    }
    let mut fragment = InkScriptSemanticDocument {
        kind: InkScriptDocumentKind::Fragment,
        sections,
    };
    alpha_rename(&mut fragment, request, source_id, document_range)?;
    let canonical_bytes = emit_inkscript_canonical(&fragment, schema).map_err(|error| {
        InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::InvalidSemanticModel,
            source_id,
            document_range,
            error.path(),
        )
    })?;
    if canonical_bytes.len() > MAX_INKSCRIPT_SOURCE_BYTES {
        return Err(InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::ResourceLimit,
            source_id,
            document_range,
            "fragment.source_bytes",
        ));
    }
    let generated_source = InkScriptSource::new(source_id, &canonical_bytes).map_err(|_| {
        InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::InvalidSemanticModel,
            source_id,
            document_range,
            "fragment.generated_source",
        )
    })?;
    let generated_parsed = super::parser::parse_inkscript(&generated_source);
    build_inkscript_declaration_model(&generated_parsed, schema)?;
    Ok(InkScriptClosedFragment { canonical_bytes })
}

struct SemanticParts<'a> {
    requires: Option<&'a InkScriptRecord>,
    parameters: Vec<&'a InkScriptParameter>,
    bindings: Vec<&'a InkScriptBinding>,
    steps: Vec<&'a InkScriptProgramStatement>,
    assets: Vec<&'a InkScriptAsset>,
}

impl<'a> SemanticParts<'a> {
    fn from_document(document: &'a InkScriptSemanticDocument) -> Self {
        let mut result = Self {
            requires: None,
            parameters: Vec::new(),
            bindings: Vec::new(),
            steps: Vec::new(),
            assets: Vec::new(),
        };
        for section in &document.sections {
            match section {
                InkScriptSemanticSection::Requires(value) => result.requires = Some(value),
                InkScriptSemanticSection::Parameters(values) => result.parameters.extend(values),
                InkScriptSemanticSection::Bindings(values) => result.bindings.extend(values),
                InkScriptSemanticSection::Program(values) => result.steps.extend(
                    values
                        .iter()
                        .filter(|value| matches!(value, InkScriptProgramStatement::Step { .. })),
                ),
                InkScriptSemanticSection::Assets(values) => result.assets.extend(values),
                _ => {}
            }
        }
        result
    }
}

fn validate_request_bounds(
    request: &InkScriptFragmentRequest,
    source_id: super::diagnostic::InkScriptSourceId,
    range: super::diagnostic::InkScriptSourceRange,
) -> Result<(), InkScriptTypeDiagnostic> {
    let bounded = [
        request.external_result_bindings.len(),
        request.reserved_value_names.len(),
        request.reserved_asset_names.len(),
        request.reserved_group_keys.len(),
    ]
    .into_iter()
    .all(|length| length <= MAX_INKSCRIPT_CONTAINER_ELEMENTS);
    if !bounded {
        return Err(InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::ResourceLimit,
            source_id,
            range,
            "fragment.request",
        ));
    }
    Ok(())
}

fn selected_steps(
    model: &InkScriptDeclarationModel,
    selection: &InkScriptFragmentSelection,
) -> Result<BTreeSet<usize>, InkScriptTypeDiagnostic> {
    let (first, last) = match selection {
        InkScriptFragmentSelection::StepRange {
            first,
            last_inclusive,
        } => (*first, *last_inclusive),
        InkScriptFragmentSelection::EditorGroup(key) => {
            let Some(group) = model.groups().iter().find(|group| group.key() == key) else {
                return Err(InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::InvalidFragmentSelection,
                    model.source_id(),
                    model.document_range(),
                    "fragment.selection.editor_group",
                ));
            };
            let first = group.first_step();
            let last = group
                .first_step()
                .checked_add(group.step_count().saturating_sub(1))
                .ok_or_else(|| {
                    InkScriptTypeDiagnostic::new(
                        InkScriptTypeDiagnosticCode::NumericOverflow,
                        model.source_id(),
                        model.document_range(),
                        "fragment.selection",
                    )
                })?;
            (first, last)
        }
    };
    let first = usize::try_from(first).map_err(|_| {
        InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::NumericOverflow,
            model.source_id(),
            model.document_range(),
            "fragment.selection",
        )
    })?;
    let last = usize::try_from(last).map_err(|_| {
        InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::NumericOverflow,
            model.source_id(),
            model.document_range(),
            "fragment.selection",
        )
    })?;
    if first > last || last >= model.steps().len() {
        return Err(InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::InvalidFragmentSelection,
            model.source_id(),
            model.document_range(),
            "fragment.selection",
        ));
    }
    for group in model.groups() {
        let group_first = group.first_step() as usize;
        let group_last = group_first + group.step_count() as usize - 1;
        let overlaps = first <= group_last && group_first <= last;
        if overlaps && !(first <= group_first && group_last <= last) {
            return Err(InkScriptTypeDiagnostic::new(
                InkScriptTypeDiagnosticCode::InvalidFragmentSelection,
                model.source_id(),
                model.steps()[first].source_range(),
                "fragment.selection.partial_editor_group",
            ));
        }
    }
    Ok((first..=last).collect())
}

#[derive(Clone)]
struct StrictBinding {
    producer_alias: String,
    reference_segments: Vec<InkScriptReferenceSegment>,
    name: String,
    binding: InkScriptBinding,
}

#[allow(clippy::too_many_arguments)]
fn validate_external_bindings(
    request: &InkScriptFragmentRequest,
    model: &InkScriptDeclarationModel,
    schema: &InkScriptSchemaView<'_>,
    aliases: &BTreeMap<String, usize>,
    selected: &BTreeSet<usize>,
    source_id: super::diagnostic::InkScriptSourceId,
    range: super::diagnostic::InkScriptSourceRange,
) -> Result<Vec<StrictBinding>, InkScriptTypeDiagnostic> {
    let mut result = Vec::with_capacity(request.external_result_bindings.len());
    let mut keys = Vec::<(String, Vec<InkScriptReferenceSegment>)>::new();
    let mut names = BTreeSet::new();
    let existing_value_names = model
        .parameters()
        .iter()
        .map(|value| value.name())
        .chain(model.bindings().iter().map(|value| value.name()))
        .chain(
            model
                .steps()
                .iter()
                .filter_map(|value| value.result_alias()),
        )
        .collect::<BTreeSet<_>>();
    for value in &request.external_result_bindings {
        let Some(producer) = aliases.get(&value.producer_alias).copied() else {
            return Err(fragment_error(
                source_id,
                range,
                "fragment.external_result_bindings.producer",
            ));
        };
        if selected.contains(&producer)
            || value.reference_segments.len() > MAX_INKSCRIPT_REFERENCE_SEGMENTS
            || !is_identifier(&value.binding_name)
            || !is_identifier(&value.selector_entity)
            || existing_value_names.contains(value.binding_name.as_str())
            || keys.iter().any(|(alias, segments)| {
                alias == &value.producer_alias && segments == &value.reference_segments
            })
            || !names.insert(value.binding_name.clone())
        {
            return Err(fragment_error(
                source_id,
                range,
                "fragment.external_result_bindings",
            ));
        }
        keys.push((
            value.producer_alias.clone(),
            value.reference_segments.clone(),
        ));
        let actual = resolve_step_result_segments(
            model.steps()[producer].results(),
            &value.reference_segments,
            schema,
        )
        .map_err(|_| {
            fragment_error(
                source_id,
                model.steps()[producer].source_range(),
                "fragment.external_result_bindings.result",
            )
        })?;
        let Some(selector_type) = schema.selector_result_type(&value.selector_entity) else {
            return Err(fragment_error(
                source_id,
                range,
                "fragment.external_result_bindings.selector",
            ));
        };
        if actual.name() != selector_type
            || value.selector_fields.len() > MAX_INKSCRIPT_CONTAINER_ELEMENTS
        {
            return Err(fragment_error(
                source_id,
                range,
                "fragment.external_result_bindings.type",
            ));
        }
        let mut fields = BTreeMap::new();
        for (name, field) in &value.selector_fields {
            if fields.insert(name.clone(), field.clone()).is_some() || !closed_literal(field) {
                return Err(fragment_error(
                    source_id,
                    range,
                    "fragment.external_result_bindings.fields",
                ));
            }
        }
        let strict_uuid = matches!(
            fields.get("source_document_uuid"),
            Some(InkScriptValue::Uuid(_))
        );
        let strict_id = matches!(
            fields.get("persistent_id"),
            Some(InkScriptValue::Integer(value)) if value.parse::<u64>().is_ok_and(|value| value != 0)
        );
        let cardinality = match fields.get("cardinality") {
            None => true,
            Some(InkScriptValue::Enum(value)) => value == "one",
            Some(_) => false,
        };
        let missing = match fields.get("missing") {
            None => true,
            Some(InkScriptValue::Enum(value)) => value == "error",
            Some(_) => false,
        };
        if !strict_uuid || !strict_id || !cardinality || !missing {
            return Err(fragment_error(
                source_id,
                range,
                "fragment.external_result_bindings.strict_selector",
            ));
        }
        result.push(StrictBinding {
            producer_alias: value.producer_alias.clone(),
            reference_segments: value.reference_segments.clone(),
            name: value.binding_name.clone(),
            binding: InkScriptBinding {
                name: value.binding_name.clone(),
                entity: value.selector_entity.clone(),
                selector: InkScriptRecord(fields),
            },
        });
    }
    Ok(result)
}

fn rewrite_external_references(
    record: &mut InkScriptRecord,
    aliases: &BTreeMap<String, usize>,
    selected: &BTreeSet<usize>,
    strict_bindings: &[StrictBinding],
    used: &mut Vec<usize>,
    source_id: super::diagnostic::InkScriptSourceId,
    model: &InkScriptDeclarationModel,
) -> Result<(), InkScriptTypeDiagnostic> {
    for value in record.0.values_mut() {
        rewrite_external_value(
            value,
            aliases,
            selected,
            strict_bindings,
            used,
            source_id,
            model,
        )?;
    }
    Ok(())
}

fn rewrite_external_value(
    value: &mut InkScriptValue,
    aliases: &BTreeMap<String, usize>,
    selected: &BTreeSet<usize>,
    strict_bindings: &[StrictBinding],
    used: &mut Vec<usize>,
    source_id: super::diagnostic::InkScriptSourceId,
    model: &InkScriptDeclarationModel,
) -> Result<(), InkScriptTypeDiagnostic> {
    match value {
        InkScriptValue::Reference { root, segments } => {
            let Some(producer) = aliases.get(root).copied() else {
                return Ok(());
            };
            if selected.contains(&producer) {
                return Ok(());
            }
            let Some((index, binding)) = strict_bindings.iter().enumerate().find(|(_, binding)| {
                binding.producer_alias == *root && binding.reference_segments == *segments
            }) else {
                return Err(InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::ExternalMutationDependency,
                    source_id,
                    model.steps()[producer].source_range(),
                    format!("fragment.external_result.{root}"),
                ));
            };
            *root = binding.name.clone();
            segments.clear();
            if !used.contains(&index) {
                used.push(index);
            }
        }
        InkScriptValue::Constructor { arguments, .. } | InkScriptValue::List(arguments) => {
            for argument in arguments {
                rewrite_external_value(
                    argument,
                    aliases,
                    selected,
                    strict_bindings,
                    used,
                    source_id,
                    model,
                )?;
            }
        }
        InkScriptValue::Record(record) => rewrite_external_references(
            record,
            aliases,
            selected,
            strict_bindings,
            used,
            source_id,
            model,
        )?,
        _ => {}
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn collect_closure_dependencies(
    record: &InkScriptRecord,
    parameters: &BTreeSet<&str>,
    bindings: &BTreeMap<&str, &InkScriptBinding>,
    strict: &BTreeMap<&str, &StrictBinding>,
    selected_aliases: &BTreeSet<&str>,
    required_parameters: &mut BTreeSet<String>,
    required_bindings: &mut BTreeSet<String>,
    required_assets: &mut Vec<String>,
    queue: &mut VecDeque<String>,
    source_id: super::diagnostic::InkScriptSourceId,
    range: super::diagnostic::InkScriptSourceRange,
) -> Result<(), InkScriptTypeDiagnostic> {
    for value in record.0.values() {
        collect_closure_value(
            value,
            parameters,
            bindings,
            strict,
            selected_aliases,
            required_parameters,
            required_bindings,
            required_assets,
            queue,
            source_id,
            range,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn collect_closure_value(
    value: &InkScriptValue,
    parameters: &BTreeSet<&str>,
    bindings: &BTreeMap<&str, &InkScriptBinding>,
    strict: &BTreeMap<&str, &StrictBinding>,
    selected_aliases: &BTreeSet<&str>,
    required_parameters: &mut BTreeSet<String>,
    required_bindings: &mut BTreeSet<String>,
    required_assets: &mut Vec<String>,
    queue: &mut VecDeque<String>,
    source_id: super::diagnostic::InkScriptSourceId,
    range: super::diagnostic::InkScriptSourceRange,
) -> Result<(), InkScriptTypeDiagnostic> {
    match value {
        InkScriptValue::Reference { root, .. } if parameters.contains(root.as_str()) => {
            required_parameters.insert(root.clone());
        }
        InkScriptValue::Reference { root, .. }
            if bindings.contains_key(root.as_str()) || strict.contains_key(root.as_str()) =>
        {
            if required_bindings.insert(root.clone()) {
                queue.push_back(root.clone());
            }
        }
        InkScriptValue::Reference { root, .. } if selected_aliases.contains(root.as_str()) => {}
        InkScriptValue::Reference { root, .. } => {
            return Err(InkScriptTypeDiagnostic::new(
                InkScriptTypeDiagnosticCode::InvalidSemanticModel,
                source_id,
                range,
                format!("fragment.reference.{root}"),
            ));
        }
        InkScriptValue::AssetReference(name) => {
            if !required_assets.contains(name) {
                required_assets.push(name.clone());
            }
        }
        InkScriptValue::Constructor { arguments, .. } | InkScriptValue::List(arguments) => {
            for argument in arguments {
                collect_closure_value(
                    argument,
                    parameters,
                    bindings,
                    strict,
                    selected_aliases,
                    required_parameters,
                    required_bindings,
                    required_assets,
                    queue,
                    source_id,
                    range,
                )?;
            }
        }
        InkScriptValue::Record(record) => collect_closure_dependencies(
            record,
            parameters,
            bindings,
            strict,
            selected_aliases,
            required_parameters,
            required_bindings,
            required_assets,
            queue,
            source_id,
            range,
        )?,
        _ => {}
    }
    Ok(())
}

fn close_assets(
    required: &[String],
    assets: &[&InkScriptAsset],
    source_id: super::diagnostic::InkScriptSourceId,
    range: super::diagnostic::InkScriptSourceRange,
) -> Result<(Vec<InkScriptAsset>, BTreeMap<String, String>), InkScriptTypeDiagnostic> {
    let by_name = assets
        .iter()
        .map(|asset| (asset.name.as_str(), *asset))
        .collect::<BTreeMap<_, _>>();
    let mut by_id = BTreeMap::<String, InkScriptAsset>::new();
    let mut result = Vec::new();
    let mut rewrites = BTreeMap::new();
    for name in required {
        let Some(asset) = by_name.get(name.as_str()) else {
            return Err(InkScriptTypeDiagnostic::new(
                InkScriptTypeDiagnosticCode::UndefinedAssetSymbol,
                source_id,
                range,
                format!("fragment.asset.{name}"),
            ));
        };
        let Some(InkScriptValue::Digest(asset_id)) = asset.body.0.get("asset_id") else {
            return Err(InkScriptTypeDiagnostic::new(
                InkScriptTypeDiagnosticCode::InvalidSemanticModel,
                source_id,
                range,
                format!("fragment.asset.{name}.asset_id"),
            ));
        };
        if let Some(canonical) = by_id.get(asset_id) {
            if canonical.body != asset.body {
                return Err(InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::InvalidSemanticModel,
                    source_id,
                    range,
                    format!("fragment.asset.{name}.descriptor"),
                ));
            }
            rewrites.insert(name.clone(), canonical.name.clone());
        } else {
            let asset = (*asset).clone();
            by_id.insert(asset_id.clone(), asset.clone());
            result.push(asset);
        }
    }
    Ok((result, rewrites))
}

fn rewrite_assets_in_bindings_and_program(
    bindings: &mut [InkScriptBinding],
    program: &mut [InkScriptProgramStatement],
    rewrites: &BTreeMap<String, String>,
) {
    for binding in bindings {
        rewrite_value_names_record(&mut binding.selector, &BTreeMap::new(), rewrites);
    }
    for statement in program {
        if let InkScriptProgramStatement::Step { arguments, .. } = statement {
            rewrite_value_names_record(arguments, &BTreeMap::new(), rewrites);
        }
    }
}

fn alpha_rename(
    fragment: &mut InkScriptSemanticDocument,
    request: &InkScriptFragmentRequest,
    source_id: super::diagnostic::InkScriptSourceId,
    range: super::diagnostic::InkScriptSourceRange,
) -> Result<(), InkScriptTypeDiagnostic> {
    let mut values =
        InkScriptGeneratedNames::new(request.reserved_value_names.iter().map(String::as_str))
            .map_err(|_| fragment_error(source_id, range, "fragment.reserved_value_names"))?;
    let mut value_rewrites = BTreeMap::new();
    for section in &mut fragment.sections {
        match section {
            InkScriptSemanticSection::Parameters(parameters) => {
                for parameter in parameters {
                    let renamed = values
                        .reserve_or_rename(&parameter.name)
                        .map_err(|_| fragment_error(source_id, range, "fragment.value_name"))?;
                    value_rewrites.insert(parameter.name.clone(), renamed.clone());
                    parameter.name = renamed;
                }
            }
            InkScriptSemanticSection::Bindings(bindings) => {
                for binding in bindings {
                    let renamed = values
                        .reserve_or_rename(&binding.name)
                        .map_err(|_| fragment_error(source_id, range, "fragment.value_name"))?;
                    value_rewrites.insert(binding.name.clone(), renamed.clone());
                    binding.name = renamed;
                }
            }
            InkScriptSemanticSection::Program(statements) => {
                for statement in statements {
                    if let InkScriptProgramStatement::Step {
                        result_alias: Some(alias),
                        ..
                    } = statement
                    {
                        let renamed = values
                            .reserve_or_rename(alias)
                            .map_err(|_| fragment_error(source_id, range, "fragment.value_name"))?;
                        value_rewrites.insert(alias.clone(), renamed.clone());
                        *alias = renamed;
                    }
                }
            }
            _ => {}
        }
    }

    let mut assets =
        InkScriptGeneratedNames::new(request.reserved_asset_names.iter().map(String::as_str))
            .map_err(|_| fragment_error(source_id, range, "fragment.reserved_asset_names"))?;
    let mut asset_rewrites = BTreeMap::new();
    for section in &mut fragment.sections {
        if let InkScriptSemanticSection::Assets(declarations) = section {
            for asset in declarations {
                let renamed = assets
                    .reserve_or_rename(&asset.name)
                    .map_err(|_| fragment_error(source_id, range, "fragment.asset_name"))?;
                asset_rewrites.insert(asset.name.clone(), renamed.clone());
                asset.name = renamed;
            }
        }
    }

    let mut used_groups = request
        .reserved_group_keys
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if used_groups.len() != request.reserved_group_keys.len()
        || used_groups
            .iter()
            .any(|key| key.is_empty() || key.len() > MAX_INKSCRIPT_STRING_BYTES)
    {
        return Err(fragment_error(
            source_id,
            range,
            "fragment.reserved_group_keys",
        ));
    }
    let mut group_rewrites = BTreeMap::<String, String>::new();
    for section in &mut fragment.sections {
        match section {
            InkScriptSemanticSection::Bindings(bindings) => {
                for binding in bindings {
                    rewrite_value_names_record(
                        &mut binding.selector,
                        &value_rewrites,
                        &asset_rewrites,
                    );
                }
            }
            InkScriptSemanticSection::Program(statements) => {
                for statement in statements {
                    if let InkScriptProgramStatement::Step {
                        editor_group,
                        arguments,
                        ..
                    } = statement
                    {
                        rewrite_value_names_record(arguments, &value_rewrites, &asset_rewrites);
                        if let Some(group) = editor_group {
                            let renamed = if let Some(existing) = group_rewrites.get(group) {
                                existing.clone()
                            } else {
                                let renamed =
                                    reserve_group(&mut used_groups, group).ok_or_else(|| {
                                        fragment_error(source_id, range, "fragment.editor_group")
                                    })?;
                                group_rewrites.insert(group.clone(), renamed.clone());
                                renamed
                            };
                            *group = renamed;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn rewrite_value_names_record(
    record: &mut InkScriptRecord,
    values: &BTreeMap<String, String>,
    assets: &BTreeMap<String, String>,
) {
    for value in record.0.values_mut() {
        rewrite_value_names(value, values, assets);
    }
}

fn rewrite_value_names(
    value: &mut InkScriptValue,
    values: &BTreeMap<String, String>,
    assets: &BTreeMap<String, String>,
) {
    match value {
        InkScriptValue::Reference { root, .. } => {
            if let Some(renamed) = values.get(root) {
                *root = renamed.clone();
            }
        }
        InkScriptValue::AssetReference(name) => {
            if let Some(renamed) = assets.get(name) {
                *name = renamed.clone();
            }
        }
        InkScriptValue::Constructor { arguments, .. } | InkScriptValue::List(arguments) => {
            for argument in arguments {
                rewrite_value_names(argument, values, assets);
            }
        }
        InkScriptValue::Record(record) => rewrite_value_names_record(record, values, assets),
        _ => {}
    }
}

fn reserve_group(used: &mut BTreeSet<String>, value: &str) -> Option<String> {
    if value.is_empty() || value.len() > MAX_INKSCRIPT_STRING_BYTES {
        return None;
    }
    if used.insert(value.to_owned()) {
        return Some(value.to_owned());
    }
    let mut suffix = 2u32;
    loop {
        let candidate = format!("{value}_{suffix}");
        if candidate.len() > MAX_INKSCRIPT_STRING_BYTES {
            return None;
        }
        if used.insert(candidate.clone()) {
            return Some(candidate);
        }
        suffix = suffix.checked_add(1)?;
    }
}

fn closed_literal(value: &InkScriptValue) -> bool {
    match value {
        InkScriptValue::Reference { .. } | InkScriptValue::AssetReference(_) => false,
        InkScriptValue::Constructor { arguments, .. } | InkScriptValue::List(arguments) => {
            arguments.iter().all(closed_literal)
        }
        InkScriptValue::Record(record) => record.0.values().all(closed_literal),
        _ => true,
    }
}

fn fragment_error(
    source_id: super::diagnostic::InkScriptSourceId,
    range: super::diagnostic::InkScriptSourceRange,
    path: &str,
) -> InkScriptTypeDiagnostic {
    InkScriptTypeDiagnostic::new(
        InkScriptTypeDiagnosticCode::InvalidStrictBinding,
        source_id,
        range,
        path,
    )
}
