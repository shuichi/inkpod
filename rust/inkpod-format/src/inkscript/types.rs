use std::collections::{BTreeMap, BTreeSet};

use super::diagnostic::{InkScriptSourceId, InkScriptSourceRange};
use super::parser::{InkScriptCstNode, InkScriptCstNodeKind, InkScriptParsed};
use super::schema::{
    InkScriptAssertComparison, InkScriptFieldSchema, InkScriptResultAvailability,
    InkScriptResultCardinality, InkScriptSchemaView, InkScriptSelectorOrder,
    InkScriptSelectorOwner,
};
use super::syntax::{
    InkScriptBinding, InkScriptProgramStatement, InkScriptRecord, InkScriptReferenceSegment,
    InkScriptSemanticSection, InkScriptTypeReference, InkScriptValue, build_inkscript_semantic,
};

/// Exact language-v2 maximum for reference edges in one dependency graph.
pub const MAX_INKSCRIPT_DEPENDENCY_EDGES: usize = 4_194_304;
const MAX_RUN_VALUE_FREEZE_DEPTH: usize = 64;

/// Caller-lowerable limits for type and dependency analysis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptAnalysisLimits {
    dependency_edges: usize,
}

impl InkScriptAnalysisLimits {
    /// Returns the exact-current language-v2 envelope.
    pub const fn exact_current() -> Self {
        Self {
            dependency_edges: MAX_INKSCRIPT_DEPENDENCY_EDGES,
        }
    }

    /// Lowers the dependency-edge limit. Zero becomes one and the language maximum cannot be
    /// raised by a caller.
    pub const fn with_dependency_edges(mut self, maximum: usize) -> Self {
        self.dependency_edges = if maximum == 0 {
            1
        } else if maximum < MAX_INKSCRIPT_DEPENDENCY_EDGES {
            maximum
        } else {
            MAX_INKSCRIPT_DEPENDENCY_EDGES
        };
        self
    }
}

/// Stable declaration-typing and run-parameter failure categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptTypeDiagnosticCode {
    InvalidSyntax,
    InvalidSemanticModel,
    UnknownType,
    UnknownConstructor,
    ConstructorArity,
    TypeMismatch,
    NumericOverflow,
    ValueOutOfRange,
    LiteralRequired,
    DuplicateValueSymbol,
    DuplicateAssetSymbol,
    UndefinedValueSymbol,
    UndefinedAssetSymbol,
    ForwardReference,
    DependencyCycle,
    UnknownResultField,
    InvalidResultIndex,
    ResultCardinalityMismatch,
    UnavailableResult,
    ExternalMutationDependency,
    InvalidFragmentSelection,
    InvalidStrictBinding,
    InvalidStrictPrecondition,
    ResourceLimit,
    InvalidRunParameter,
    MissingRunParameter,
}

impl InkScriptTypeDiagnosticCode {
    /// Returns the locale-independent diagnostic spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidSyntax => "invalid_syntax",
            Self::InvalidSemanticModel => "invalid_semantic_model",
            Self::UnknownType => "unknown_type",
            Self::UnknownConstructor => "unknown_constructor",
            Self::ConstructorArity => "constructor_arity",
            Self::TypeMismatch => "type_mismatch",
            Self::NumericOverflow => "numeric_overflow",
            Self::ValueOutOfRange => "value_out_of_range",
            Self::LiteralRequired => "literal_required",
            Self::DuplicateValueSymbol => "duplicate_value_symbol",
            Self::DuplicateAssetSymbol => "duplicate_asset_symbol",
            Self::UndefinedValueSymbol => "undefined_value_symbol",
            Self::UndefinedAssetSymbol => "undefined_asset_symbol",
            Self::ForwardReference => "forward_reference",
            Self::DependencyCycle => "dependency_cycle",
            Self::UnknownResultField => "unknown_result_field",
            Self::InvalidResultIndex => "invalid_result_index",
            Self::ResultCardinalityMismatch => "result_cardinality_mismatch",
            Self::UnavailableResult => "unavailable_result",
            Self::ExternalMutationDependency => "external_mutation_dependency",
            Self::InvalidFragmentSelection => "invalid_fragment_selection",
            Self::InvalidStrictBinding => "invalid_strict_binding",
            Self::InvalidStrictPrecondition => "invalid_strict_precondition",
            Self::ResourceLimit => "resource_limit",
            Self::InvalidRunParameter => "invalid_run_parameter",
            Self::MissingRunParameter => "missing_run_parameter",
        }
    }
}

/// A source-bound, locale-independent declaration typing diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptTypeDiagnostic {
    code: InkScriptTypeDiagnosticCode,
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
    path: String,
}

impl InkScriptTypeDiagnostic {
    pub(crate) fn new(
        code: InkScriptTypeDiagnosticCode,
        source_id: InkScriptSourceId,
        range: InkScriptSourceRange,
        path: impl Into<String>,
    ) -> Self {
        Self {
            code,
            source_id,
            range,
            path: path.into(),
        }
    }

    /// Returns the stable failure category.
    pub const fn code(&self) -> InkScriptTypeDiagnosticCode {
        self.code
    }

    /// Returns the caller-owned source identity.
    pub const fn source_id(&self) -> InkScriptSourceId {
        self.source_id
    }

    /// Returns the authoritative UTF-8 byte and display range of the owning declaration.
    pub const fn range(&self) -> InkScriptSourceRange {
        self.range
    }

    /// Returns the non-localized declaration/value path.
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// An exact resolved schema type. Construction is restricted to schema-checked APIs.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct InkScriptResolvedType(String);

impl InkScriptResolvedType {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Returns the exact schema spelling, including `list<T>` or `nullable<T>` wrappers.
    pub fn name(&self) -> &str {
        &self.0
    }
}

/// The owned payload of one schema-typed value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InkScriptTypedValueKind {
    Boolean(bool),
    U32(u32),
    I32(i32),
    U64(u64),
    I64(i64),
    Q16(i64),
    String(String),
    Uuid(String),
    Digest(String),
    Base64(Vec<u8>),
    Enum(String),
    Constructor {
        name: String,
        arguments: Vec<InkScriptTypedValue>,
    },
    None,
    List(Vec<InkScriptTypedValue>),
    Record(BTreeMap<String, InkScriptTypedValue>),
    AssetReference(String),
    Reference {
        root: String,
        segments: Vec<InkScriptReferenceSegment>,
    },
}

/// An immutable, fully range-checked value with its actual schema type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptTypedValue {
    value_type: InkScriptResolvedType,
    kind: InkScriptTypedValueKind,
}

impl InkScriptTypedValue {
    pub(crate) fn new(value_type: impl Into<String>, kind: InkScriptTypedValueKind) -> Self {
        Self {
            value_type: InkScriptResolvedType::new(value_type),
            kind,
        }
    }

    /// Returns the actual schema type after closed-sum and nullable matching.
    pub fn value_type(&self) -> &InkScriptResolvedType {
        &self.value_type
    }

    /// Returns the actual schema type spelling.
    pub fn type_name(&self) -> &str {
        self.value_type.name()
    }

    /// Returns the immutable typed payload.
    pub const fn kind(&self) -> &InkScriptTypedValueKind {
        &self.kind
    }
}

/// One immutable, schema-typed parameter declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptTypedParameter {
    name: String,
    declared_type: InkScriptResolvedType,
    default_value: InkScriptTypedValue,
    label: Option<String>,
    asks_each_run: bool,
    source_range: InkScriptSourceRange,
}

impl InkScriptTypedParameter {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn declared_type(&self) -> &InkScriptResolvedType {
        &self.declared_type
    }

    pub fn default_value(&self) -> &InkScriptTypedValue {
        &self.default_value
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub const fn asks_each_run(&self) -> bool {
        self.asks_each_run
    }

    pub const fn source_range(&self) -> InkScriptSourceRange {
        self.source_range
    }
}

/// One selector declaration whose result role has been resolved but not executed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptTypedBinding {
    name: String,
    entity: String,
    owner: InkScriptSelectorOwner,
    initial_order: InkScriptSelectorOrder,
    cardinality: InkScriptSelectorCardinality,
    missing: InkScriptSelectorMissingPolicy,
    result_type: InkScriptResolvedType,
    selector: InkScriptTypedValue,
    source_range: InkScriptSourceRange,
}

impl InkScriptTypedBinding {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn result_type(&self) -> &InkScriptResolvedType {
        &self.result_type
    }

    pub fn selector(&self) -> &InkScriptTypedValue {
        &self.selector
    }

    pub const fn source_range(&self) -> InkScriptSourceRange {
        self.source_range
    }

    /// Returns the exact selector entity name.
    pub fn entity(&self) -> &str {
        &self.entity
    }

    /// Returns the selector owner relation.
    pub const fn owner(&self) -> InkScriptSelectorOwner {
        self.owner
    }

    /// Returns the canonical initial-snapshot ordering.
    pub const fn initial_order(&self) -> InkScriptSelectorOrder {
        self.initial_order
    }

    /// Returns the selector cardinality policy.
    pub const fn cardinality(&self) -> InkScriptSelectorCardinality {
        self.cardinality
    }

    /// Returns the missing-selector policy.
    pub const fn missing(&self) -> InkScriptSelectorMissingPolicy {
        self.missing
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Closed selector cardinality policy.
pub enum InkScriptSelectorCardinality {
    One,
    First,
    All,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Closed missing-selector policy.
pub enum InkScriptSelectorMissingPolicy {
    Error,
    SkipDependents,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// One typed assertion evaluated against the immutable initial snapshot.
pub struct InkScriptTypedAssert {
    kind: String,
    comparison: InkScriptAssertComparison,
    arguments: InkScriptTypedValue,
    source_range: InkScriptSourceRange,
    program_index: u32,
}

impl InkScriptTypedAssert {
    /// Returns the exact assertion kind.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Returns the closed comparison contract.
    pub const fn comparison(&self) -> InkScriptAssertComparison {
        self.comparison
    }

    /// Returns the immutable typed arguments.
    pub const fn arguments(&self) -> &InkScriptTypedValue {
        &self.arguments
    }

    /// Returns the source-order program index.
    pub const fn program_index(&self) -> u32 {
        self.program_index
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Source-order node in a typed InkScript program.
pub enum InkScriptTypedProgramNode {
    Assert(usize),
    Step(usize),
}

/// One closed, typed asset declaration. Payload ingestion belongs to its later owner milestone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptTypedAsset {
    name: String,
    body: InkScriptTypedValue,
    source_range: InkScriptSourceRange,
}

impl InkScriptTypedAsset {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn body(&self) -> &InkScriptTypedValue {
        &self.body
    }

    pub const fn source_range(&self) -> InkScriptSourceRange {
        self.source_range
    }
}

/// One closed result field of a typed step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptTypedStepResult {
    name: String,
    value_type: InkScriptResolvedType,
    availability: InkScriptResultAvailability,
    cardinality: InkScriptResultCardinality,
}

impl InkScriptTypedStepResult {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value_type(&self) -> &InkScriptResolvedType {
        &self.value_type
    }

    pub const fn availability(&self) -> InkScriptResultAvailability {
        self.availability
    }

    pub const fn cardinality(&self) -> InkScriptResultCardinality {
        self.cardinality
    }
}

/// One immutable typed invocation. Execution and result materialization belong to later owners.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptTypedStep {
    label: String,
    result_alias: Option<String>,
    enabled: bool,
    editor_group: Option<String>,
    command: String,
    arguments: InkScriptTypedValue,
    results: Vec<InkScriptTypedStepResult>,
    source_range: InkScriptSourceRange,
}

impl InkScriptTypedStep {
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn result_alias(&self) -> Option<&str> {
        self.result_alias.as_deref()
    }

    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn editor_group(&self) -> Option<&str> {
        self.editor_group.as_deref()
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn arguments(&self) -> &InkScriptTypedValue {
        &self.arguments
    }

    pub fn results(&self) -> &[InkScriptTypedStepResult] {
        &self.results
    }

    pub const fn source_range(&self) -> InkScriptSourceRange {
        self.source_range
    }
}

/// One contiguous non-semantic editor group in step-index space.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptStepGroup {
    key: String,
    first_step: u32,
    step_count: u32,
}

impl InkScriptStepGroup {
    pub fn key(&self) -> &str {
        &self.key
    }

    pub const fn first_step(&self) -> u32 {
        self.first_step
    }

    pub const fn step_count(&self) -> u32 {
        self.step_count
    }
}

/// Stable node categories used by type checking and fragment closure.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum InkScriptDependencyNodeKind {
    Parameter,
    Binding,
    Assert,
    Step,
    StepResult,
    Asset,
}

/// One owned dependency-graph node. Step indices are source-order indices, not persistent IDs.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct InkScriptDependencyNode {
    kind: InkScriptDependencyNodeKind,
    name: String,
    step_index: Option<u32>,
    program_index: Option<u32>,
}

impl InkScriptDependencyNode {
    pub const fn kind(&self) -> InkScriptDependencyNodeKind {
        self.kind
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn step_index(&self) -> Option<u32> {
        self.step_index
    }

    pub const fn program_index(&self) -> Option<u32> {
        self.program_index
    }
}

/// One occurrence-preserving reference edge in deterministic source/schema order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptDependencyEdge {
    consumer: InkScriptDependencyNode,
    dependency: InkScriptDependencyNode,
}

impl InkScriptDependencyEdge {
    pub const fn consumer(&self) -> &InkScriptDependencyNode {
        &self.consumer
    }

    pub const fn dependency(&self) -> &InkScriptDependencyNode {
        &self.dependency
    }
}

/// The immutable declaration/type environment used by later compiler milestones.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptDeclarationModel {
    source_id: InkScriptSourceId,
    document_range: InkScriptSourceRange,
    parameters: Vec<InkScriptTypedParameter>,
    bindings: Vec<InkScriptTypedBinding>,
    assets: Vec<InkScriptTypedAsset>,
    assertions: Vec<InkScriptTypedAssert>,
    steps: Vec<InkScriptTypedStep>,
    program: Vec<InkScriptTypedProgramNode>,
    groups: Vec<InkScriptStepGroup>,
    dependency_edges: Vec<InkScriptDependencyEdge>,
}

impl InkScriptDeclarationModel {
    pub const fn source_id(&self) -> InkScriptSourceId {
        self.source_id
    }

    pub const fn document_range(&self) -> InkScriptSourceRange {
        self.document_range
    }

    pub fn parameters(&self) -> &[InkScriptTypedParameter] {
        &self.parameters
    }

    pub fn bindings(&self) -> &[InkScriptTypedBinding] {
        &self.bindings
    }

    pub fn assets(&self) -> &[InkScriptTypedAsset] {
        &self.assets
    }

    pub fn steps(&self) -> &[InkScriptTypedStep] {
        &self.steps
    }

    pub fn groups(&self) -> &[InkScriptStepGroup] {
        &self.groups
    }

    pub fn dependency_edges(&self) -> &[InkScriptDependencyEdge] {
        &self.dependency_edges
    }

    /// Returns typed assertions in declaration order.
    pub fn assertions(&self) -> &[InkScriptTypedAssert] {
        &self.assertions
    }

    /// Returns assertions and steps in exact source order.
    pub fn program(&self) -> &[InkScriptTypedProgramNode] {
        &self.program
    }
}

/// One explicit non-interactive decision for an `ask = each_run` parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InkScriptRunParameterChoice {
    AcceptDefault { name: String },
    Override { name: String, value: InkScriptValue },
}

impl InkScriptRunParameterChoice {
    fn name(&self) -> &str {
        match self {
            Self::AcceptDefault { name } | Self::Override { name, .. } => name,
        }
    }
}

/// A run-settings outcome. Cancel publishes no run copy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InkScriptRunParameterDecision {
    Cancel,
    Resolve(Vec<InkScriptRunParameterChoice>),
}

/// One parameter value in an immutable per-run copy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptRunParameterValue {
    name: String,
    value: InkScriptTypedValue,
}

impl InkScriptRunParameterValue {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &InkScriptTypedValue {
        &self.value
    }
}

/// A complete immutable parameter copy ready for a later job model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptRunParameters {
    values: Vec<InkScriptRunParameterValue>,
}

impl InkScriptRunParameters {
    pub fn values(&self) -> &[InkScriptRunParameterValue] {
        &self.values
    }

    /// Replaces parameter-root references recursively with this immutable run's values.
    /// Binding and step-result references remain unresolved for the Core binding stage.
    pub fn freeze_value(
        &self,
        value: &InkScriptTypedValue,
    ) -> Result<InkScriptTypedValue, InkScriptTypeDiagnosticCode> {
        freeze_run_value(value, &self.values, 0)
    }
}

fn freeze_run_value(
    value: &InkScriptTypedValue,
    parameters: &[InkScriptRunParameterValue],
    depth: usize,
) -> Result<InkScriptTypedValue, InkScriptTypeDiagnosticCode> {
    if depth >= MAX_RUN_VALUE_FREEZE_DEPTH {
        return Err(InkScriptTypeDiagnosticCode::ResourceLimit);
    }
    let kind = match value.kind() {
        InkScriptTypedValueKind::Reference { root, segments } => {
            let Some(parameter) = parameters.iter().find(|value| value.name == *root) else {
                return Ok(value.clone());
            };
            if !segments.is_empty() {
                return Err(InkScriptTypeDiagnosticCode::UndefinedValueSymbol);
            }
            return Ok(parameter.value.clone());
        }
        InkScriptTypedValueKind::Constructor { name, arguments } => {
            InkScriptTypedValueKind::Constructor {
                name: name.clone(),
                arguments: arguments
                    .iter()
                    .map(|value| freeze_run_value(value, parameters, depth + 1))
                    .collect::<Result<_, _>>()?,
            }
        }
        InkScriptTypedValueKind::List(values) => InkScriptTypedValueKind::List(
            values
                .iter()
                .map(|value| freeze_run_value(value, parameters, depth + 1))
                .collect::<Result<_, _>>()?,
        ),
        InkScriptTypedValueKind::Record(fields) => InkScriptTypedValueKind::Record(
            fields
                .iter()
                .map(|(name, value)| {
                    Ok((
                        name.clone(),
                        freeze_run_value(value, parameters, depth + 1)?,
                    ))
                })
                .collect::<Result<_, InkScriptTypeDiagnosticCode>>()?,
        ),
        other => other.clone(),
    };
    Ok(InkScriptTypedValue::new(value.type_name(), kind))
}

struct DeclarationRanges {
    document: InkScriptSourceRange,
    parameters: Vec<InkScriptSourceRange>,
    bindings: Vec<InkScriptSourceRange>,
    assets: Vec<InkScriptSourceRange>,
    steps: Vec<InkScriptSourceRange>,
    program: Vec<InkScriptSourceRange>,
}

/// Resolves approved language-v2 declaration types and namespaces without filesystem, Core, or
/// job access. Failure publishes no partial declaration model.
pub fn build_inkscript_declaration_model(
    parsed: &InkScriptParsed<'_>,
    schema: &InkScriptSchemaView<'_>,
) -> Result<InkScriptDeclarationModel, InkScriptTypeDiagnostic> {
    build_inkscript_declaration_model_with_limits(
        parsed,
        schema,
        InkScriptAnalysisLimits::exact_current(),
    )
}

/// Builds the same atomic model with a caller-lowered dependency-edge envelope.
pub fn build_inkscript_declaration_model_with_limits(
    parsed: &InkScriptParsed<'_>,
    schema: &InkScriptSchemaView<'_>,
    limits: InkScriptAnalysisLimits,
) -> Result<InkScriptDeclarationModel, InkScriptTypeDiagnostic> {
    let source = parsed.cst().source();
    let ranges = declaration_ranges(parsed);
    if !parsed.is_valid() {
        return Err(InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::InvalidSyntax,
            source.id(),
            ranges.document,
            "document",
        ));
    }
    let semantic = build_inkscript_semantic(parsed, schema).map_err(|error| {
        InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::InvalidSemanticModel,
            source.id(),
            ranges.document,
            error.path(),
        )
    })?;

    let mut parameter_syntax = Vec::new();
    let mut binding_syntax = Vec::new();
    let mut asset_syntax = Vec::new();
    let mut program_syntax = Vec::new();
    let mut step_syntax = Vec::new();
    let mut result_aliases = Vec::new();
    let mut step_index = 0usize;
    for section in semantic.sections() {
        match section {
            InkScriptSemanticSection::Parameters(values) => parameter_syntax.extend(values),
            InkScriptSemanticSection::Bindings(values) => binding_syntax.extend(values),
            InkScriptSemanticSection::Assets(values) => asset_syntax.extend(values),
            InkScriptSemanticSection::Program(statements) => {
                for statement in statements {
                    program_syntax.push(statement);
                    if let super::syntax::InkScriptProgramStatement::Step { result_alias, .. } =
                        statement
                    {
                        step_syntax.push(statement);
                        if let Some(alias) = result_alias {
                            result_aliases.push((alias, step_index));
                        }
                        step_index += 1;
                    }
                }
            }
            _ => {}
        }
    }

    let parameter_ranges =
        align_ranges(&ranges.parameters, parameter_syntax.len(), ranges.document);
    let binding_ranges = align_ranges(&ranges.bindings, binding_syntax.len(), ranges.document);
    let asset_ranges = align_ranges(&ranges.assets, asset_syntax.len(), ranges.document);
    let program_ranges = align_ranges(&ranges.program, program_syntax.len(), ranges.document);

    let mut value_names = BTreeMap::<String, SymbolKind>::new();
    for (index, parameter) in parameter_syntax.iter().enumerate() {
        insert_value_name(
            &mut value_names,
            &parameter.name,
            SymbolKind::Parameter(index),
            source.id(),
            parameter_ranges[index],
        )?;
    }
    for (index, binding) in binding_syntax.iter().enumerate() {
        insert_value_name(
            &mut value_names,
            &binding.name,
            SymbolKind::Binding(index),
            source.id(),
            binding_ranges[index],
        )?;
    }
    for (alias, index) in result_aliases {
        insert_value_name(
            &mut value_names,
            alias,
            SymbolKind::StepResult(index),
            source.id(),
            ranges.steps.get(index).copied().unwrap_or(ranges.document),
        )?;
    }

    let mut asset_names = BTreeSet::new();
    for (index, asset) in asset_syntax.iter().enumerate() {
        if !asset_names.insert(asset.name.clone()) {
            return Err(InkScriptTypeDiagnostic::new(
                InkScriptTypeDiagnosticCode::DuplicateAssetSymbol,
                source.id(),
                asset_ranges[index],
                format!("assets.{}", asset.name),
            ));
        }
    }

    let mut parameters = Vec::with_capacity(parameter_syntax.len());
    let mut parameter_types = Vec::with_capacity(parameter_syntax.len());
    for (index, parameter) in parameter_syntax.iter().enumerate() {
        let range = parameter_ranges[index];
        let path = format!("parameters.{}", parameter.name);
        let declared_type =
            resolve_type_reference(&parameter.declared_type, schema, source.id(), range, &path)?;
        if !is_closed_literal(&parameter.default_value) {
            return Err(InkScriptTypeDiagnostic::new(
                InkScriptTypeDiagnosticCode::LiteralRequired,
                source.id(),
                range,
                format!("{path}.default"),
            ));
        }
        let mut no_references = |_root: &str, _segments: &[InkScriptReferenceSegment]| {
            Err(InkScriptTypeDiagnosticCode::LiteralRequired)
        };
        let default_value = type_value(
            &parameter.default_value,
            &declared_type,
            schema,
            &asset_names,
            &mut no_references,
            source.id(),
            range,
            &format!("{path}.default"),
        )?;
        let label = string_field(&parameter.metadata, "label").map(str::to_owned);
        let asks_each_run = match enum_field(&parameter.metadata, "ask").unwrap_or("never") {
            "never" => false,
            "each_run" => true,
            _ => {
                return Err(InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::ValueOutOfRange,
                    source.id(),
                    range,
                    format!("{path}.ask"),
                ));
            }
        };
        parameter_types.push(declared_type.clone());
        parameters.push(InkScriptTypedParameter {
            name: parameter.name.clone(),
            declared_type,
            default_value,
            label,
            asks_each_run,
            source_range: range,
        });
    }

    let binding_result_types = binding_syntax
        .iter()
        .enumerate()
        .map(|(index, binding)| {
            selector_result_type(binding, schema, source.id(), binding_ranges[index])
        })
        .collect::<Result<Vec<_>, _>>()?;
    let binding_dependencies =
        collect_binding_dependencies(&binding_syntax, &value_names, source.id(), &binding_ranges)?;
    reject_dependency_cycles(
        &binding_dependencies,
        source.id(),
        &binding_ranges,
        &binding_syntax,
    )?;
    for (index, dependencies) in binding_dependencies.iter().enumerate() {
        if dependencies.iter().any(|dependency| *dependency >= index) {
            return Err(InkScriptTypeDiagnostic::new(
                InkScriptTypeDiagnosticCode::ForwardReference,
                source.id(),
                binding_ranges[index],
                format!("bindings.{}", binding_syntax[index].name),
            ));
        }
    }

    let mut bindings = Vec::with_capacity(binding_syntax.len());
    for (index, binding) in binding_syntax.iter().enumerate() {
        let range = binding_ranges[index];
        let fields = schema.selector(&binding.entity).ok_or_else(|| {
            InkScriptTypeDiagnostic::new(
                InkScriptTypeDiagnosticCode::UnknownType,
                source.id(),
                range,
                format!("bindings.{}.selector", binding.name),
            )
        })?;
        let mut resolve_reference = |root: &str, segments: &[InkScriptReferenceSegment]| {
            let root_type = match value_names.get(root) {
                Some(SymbolKind::Parameter(parameter)) => parameter_types[*parameter].clone(),
                Some(SymbolKind::Binding(binding_index)) if *binding_index < index => {
                    binding_result_types[*binding_index].clone()
                }
                Some(SymbolKind::Binding(_)) | Some(SymbolKind::StepResult(_)) => {
                    return Err(InkScriptTypeDiagnosticCode::ForwardReference);
                }
                None => return Err(InkScriptTypeDiagnosticCode::UndefinedValueSymbol),
            };
            resolve_reference_segments(root_type, segments, schema)
        };
        let selector = type_record(
            &binding.selector,
            &format!("{}_selector", binding.entity),
            fields,
            schema,
            &asset_names,
            &mut resolve_reference,
            source.id(),
            range,
            &format!("bindings.{}.selector", binding.name),
        )?;
        let selector_schema = schema
            .selector_schema(&binding.entity)
            .expect("semantic analysis accepted this exact selector entity");
        let cardinality = selector_cardinality(&selector, source.id(), range, &binding.name)?;
        let missing = selector_missing_policy(&selector, source.id(), range, &binding.name)?;
        bindings.push(InkScriptTypedBinding {
            name: binding.name.clone(),
            entity: binding.entity.clone(),
            owner: selector_schema.owner,
            initial_order: selector_schema.initial_order,
            cardinality,
            missing,
            result_type: binding_result_types[index].clone(),
            selector,
            source_range: range,
        });
    }

    let mut assets = Vec::with_capacity(asset_syntax.len());
    for (index, asset) in asset_syntax.iter().enumerate() {
        let range = asset_ranges[index];
        let fields = schema
            .record("canonical_raster_asset")
            .expect("approved language schema has canonical raster assets");
        let mut no_references = |_root: &str, _segments: &[InkScriptReferenceSegment]| {
            Err(InkScriptTypeDiagnosticCode::UndefinedValueSymbol)
        };
        let body = type_record(
            &asset.body,
            "canonical_raster_asset",
            fields,
            schema,
            &asset_names,
            &mut no_references,
            source.id(),
            range,
            &format!("assets.{}", asset.name),
        )?;
        assets.push(InkScriptTypedAsset {
            name: asset.name.clone(),
            body,
            source_range: range,
        });
    }

    let step_ranges = align_ranges(&ranges.steps, step_syntax.len(), ranges.document);
    let AnalyzedProgram {
        assertions,
        steps,
        program,
        groups,
        dependency_edges,
    } = analyze_program(
        &binding_syntax,
        &program_syntax,
        &step_syntax,
        &value_names,
        &parameter_types,
        &binding_result_types,
        &asset_names,
        schema,
        source.id(),
        &binding_ranges,
        &program_ranges,
        &step_ranges,
        limits,
    )?;

    Ok(InkScriptDeclarationModel {
        source_id: source.id(),
        document_range: ranges.document,
        parameters,
        bindings,
        assets,
        assertions,
        steps,
        program,
        groups,
        dependency_edges,
    })
}

/// Resolves every `ask = each_run` parameter explicitly. Cancel and every invalid choice return
/// without publishing a partial run copy or mutating the declaration model/defaults.
pub fn resolve_inkscript_run_parameters(
    model: &InkScriptDeclarationModel,
    schema: &InkScriptSchemaView<'_>,
    decision: InkScriptRunParameterDecision,
) -> Result<Option<InkScriptRunParameters>, InkScriptTypeDiagnostic> {
    let InkScriptRunParameterDecision::Resolve(choices) = decision else {
        return Ok(None);
    };
    let mut by_name = BTreeMap::new();
    for choice in &choices {
        let name = choice.name();
        let Some(parameter) = model
            .parameters
            .iter()
            .find(|parameter| parameter.name == name)
        else {
            return Err(run_error(
                model,
                None,
                InkScriptTypeDiagnosticCode::InvalidRunParameter,
                format!("run_parameters.{name}"),
            ));
        };
        if !parameter.asks_each_run || by_name.insert(name, choice).is_some() {
            return Err(run_error(
                model,
                Some(parameter),
                InkScriptTypeDiagnosticCode::InvalidRunParameter,
                format!("run_parameters.{name}"),
            ));
        }
    }

    for parameter in &model.parameters {
        if parameter.asks_each_run && !by_name.contains_key(parameter.name.as_str()) {
            return Err(run_error(
                model,
                Some(parameter),
                InkScriptTypeDiagnosticCode::MissingRunParameter,
                format!("run_parameters.{}", parameter.name),
            ));
        }
    }

    let asset_names = model
        .assets
        .iter()
        .map(|asset| asset.name.clone())
        .collect::<BTreeSet<_>>();
    let mut values = Vec::with_capacity(model.parameters.len());
    for parameter in &model.parameters {
        let value = match by_name.get(parameter.name.as_str()) {
            None | Some(InkScriptRunParameterChoice::AcceptDefault { .. }) => {
                parameter.default_value.clone()
            }
            Some(InkScriptRunParameterChoice::Override { value, .. }) => {
                if !is_closed_literal(value) {
                    return Err(run_error(
                        model,
                        Some(parameter),
                        InkScriptTypeDiagnosticCode::LiteralRequired,
                        format!("run_parameters.{}", parameter.name),
                    ));
                }
                let mut no_references = |_root: &str, _segments: &[InkScriptReferenceSegment]| {
                    Err(InkScriptTypeDiagnosticCode::LiteralRequired)
                };
                type_value(
                    value,
                    &parameter.declared_type,
                    schema,
                    &asset_names,
                    &mut no_references,
                    model.source_id,
                    parameter.source_range,
                    &format!("run_parameters.{}", parameter.name),
                )?
            }
        };
        values.push(InkScriptRunParameterValue {
            name: parameter.name.clone(),
            value,
        });
    }
    Ok(Some(InkScriptRunParameters { values }))
}

#[derive(Clone, Copy)]
enum SymbolKind {
    Parameter(usize),
    Binding(usize),
    StepResult(usize),
}

#[derive(Clone)]
enum DependencyUse {
    Value {
        root: String,
        segments: Vec<InkScriptReferenceSegment>,
    },
    Asset(String),
}

struct AnalyzedProgram {
    assertions: Vec<InkScriptTypedAssert>,
    steps: Vec<InkScriptTypedStep>,
    program: Vec<InkScriptTypedProgramNode>,
    groups: Vec<InkScriptStepGroup>,
    dependency_edges: Vec<InkScriptDependencyEdge>,
}

#[allow(clippy::too_many_arguments)]
fn analyze_program(
    binding_syntax: &[&InkScriptBinding],
    program_syntax: &[&InkScriptProgramStatement],
    step_syntax: &[&InkScriptProgramStatement],
    value_names: &BTreeMap<String, SymbolKind>,
    parameter_types: &[InkScriptResolvedType],
    binding_result_types: &[InkScriptResolvedType],
    asset_names: &BTreeSet<String>,
    schema: &InkScriptSchemaView<'_>,
    source_id: InkScriptSourceId,
    binding_ranges: &[InkScriptSourceRange],
    program_ranges: &[InkScriptSourceRange],
    step_ranges: &[InkScriptSourceRange],
    limits: InkScriptAnalysisLimits,
) -> Result<AnalyzedProgram, InkScriptTypeDiagnostic> {
    let mut step_results = Vec::with_capacity(step_syntax.len());
    let mut step_enabled = Vec::with_capacity(step_syntax.len());
    for (index, statement) in step_syntax.iter().enumerate() {
        let InkScriptProgramStatement::Step {
            result_alias,
            enabled,
            command,
            ..
        } = statement
        else {
            unreachable!("step syntax is collected from step statements")
        };
        let command_schema = schema.command_schema(command).ok_or_else(|| {
            InkScriptTypeDiagnostic::new(
                InkScriptTypeDiagnosticCode::InvalidSemanticModel,
                source_id,
                step_ranges[index],
                format!("program.steps[{index}].command.{command}"),
            )
        })?;
        if result_alias.is_some() && command_schema.results.is_empty() {
            return Err(InkScriptTypeDiagnostic::new(
                InkScriptTypeDiagnosticCode::UnavailableResult,
                source_id,
                step_ranges[index],
                format!("program.steps[{index}].result_alias"),
            ));
        }
        step_results.push(
            command_schema
                .results
                .iter()
                .copied()
                .map(|result| InkScriptTypedStepResult {
                    name: result.name.to_owned(),
                    value_type: InkScriptResolvedType::new(result.resolved_type()),
                    availability: result.availability,
                    cardinality: result.cardinality,
                })
                .collect::<Vec<_>>(),
        );
        step_enabled.push(*enabled);
    }

    let mut dependency_edges = Vec::new();
    for (index, binding) in binding_syntax.iter().enumerate() {
        let consumer = InkScriptDependencyNode {
            kind: InkScriptDependencyNodeKind::Binding,
            name: binding.name.clone(),
            step_index: None,
            program_index: None,
        };
        let mut uses = Vec::new();
        collect_dependency_uses_record(&binding.selector, &mut uses);
        for dependency in uses {
            let dependency = dependency_node(
                dependency,
                value_names,
                step_syntax,
                asset_names,
                source_id,
                binding_ranges[index],
                &format!("bindings.{}", binding.name),
            )?;
            push_dependency_edge(
                &mut dependency_edges,
                consumer.clone(),
                dependency,
                limits,
                source_id,
                binding_ranges[index],
            )?;
        }
    }

    let mut steps = Vec::with_capacity(step_syntax.len());
    let step_program_indices = program_syntax
        .iter()
        .enumerate()
        .filter_map(|(index, statement)| {
            matches!(statement, InkScriptProgramStatement::Step { .. }).then_some(index)
        })
        .collect::<Vec<_>>();
    for (index, statement) in step_syntax.iter().enumerate() {
        let InkScriptProgramStatement::Step {
            label,
            result_alias,
            enabled,
            editor_group,
            command,
            arguments,
        } = statement
        else {
            unreachable!("step syntax is collected from step statements")
        };
        let command_schema = schema
            .command_schema(command)
            .expect("semantic analysis accepted this exact command schema");
        let range = step_ranges[index];
        let mut resolve_reference = |root: &str, segments: &[InkScriptReferenceSegment]| {
            let root_type = match value_names.get(root) {
                Some(SymbolKind::Parameter(parameter)) => parameter_types[*parameter].clone(),
                Some(SymbolKind::Binding(binding)) => binding_result_types[*binding].clone(),
                Some(SymbolKind::StepResult(producer)) if *producer == index => {
                    return Err(InkScriptTypeDiagnosticCode::DependencyCycle);
                }
                Some(SymbolKind::StepResult(producer)) if *producer > index => {
                    return Err(InkScriptTypeDiagnosticCode::ForwardReference);
                }
                Some(SymbolKind::StepResult(producer)) => {
                    if !step_enabled[*producer] {
                        return Err(InkScriptTypeDiagnosticCode::UnavailableResult);
                    }
                    resolve_step_result_segments(&step_results[*producer], segments, schema)?
                }
                None => return Err(InkScriptTypeDiagnosticCode::UndefinedValueSymbol),
            };
            if matches!(value_names.get(root), Some(SymbolKind::StepResult(_))) {
                Ok(root_type)
            } else {
                resolve_reference_segments(root_type, segments, schema)
            }
        };
        let arguments = type_record(
            arguments,
            &format!("{command}_invocation"),
            command_schema.fields,
            schema,
            asset_names,
            &mut resolve_reference,
            source_id,
            range,
            &format!("program.steps[{index}].invoke.{command}"),
        )?;

        let consumer = InkScriptDependencyNode {
            kind: InkScriptDependencyNodeKind::Step,
            name: command.clone(),
            step_index: Some(u32::try_from(index).map_err(|_| {
                InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::NumericOverflow,
                    source_id,
                    range,
                    format!("program.steps[{index}]"),
                )
            })?),
            program_index: Some(u32::try_from(step_program_indices[index]).map_err(|_| {
                InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::NumericOverflow,
                    source_id,
                    range,
                    format!("program.steps[{index}]"),
                )
            })?),
        };
        let mut uses = Vec::new();
        collect_dependency_uses_record(
            match statement {
                InkScriptProgramStatement::Step { arguments, .. } => arguments,
                InkScriptProgramStatement::Assert { .. } => unreachable!(),
            },
            &mut uses,
        );
        for dependency in uses {
            let dependency = dependency_node(
                dependency,
                value_names,
                step_syntax,
                asset_names,
                source_id,
                range,
                &format!("program.steps[{index}]"),
            )?;
            push_dependency_edge(
                &mut dependency_edges,
                consumer.clone(),
                dependency,
                limits,
                source_id,
                range,
            )?;
        }
        steps.push(InkScriptTypedStep {
            label: label.clone(),
            result_alias: result_alias.clone(),
            enabled: *enabled,
            editor_group: editor_group.clone(),
            command: command.clone(),
            arguments,
            results: step_results[index].clone(),
            source_range: range,
        });
    }

    let mut assertions = Vec::new();
    let mut program = Vec::with_capacity(program_syntax.len());
    let mut preceding_steps = 0usize;
    for (program_index, statement) in program_syntax.iter().enumerate() {
        let range = program_ranges[program_index];
        match statement {
            InkScriptProgramStatement::Step { .. } => {
                program.push(InkScriptTypedProgramNode::Step(preceding_steps));
                preceding_steps += 1;
            }
            InkScriptProgramStatement::Assert { kind, arguments } => {
                let assertion_schema = schema.assertion_schema(kind).ok_or_else(|| {
                    InkScriptTypeDiagnostic::new(
                        InkScriptTypeDiagnosticCode::InvalidSemanticModel,
                        source_id,
                        range,
                        format!("program.assertions[{program_index}].{kind}"),
                    )
                })?;
                let mut resolve_reference = |root: &str, segments: &[InkScriptReferenceSegment]| {
                    let root_type = match value_names.get(root) {
                        Some(SymbolKind::Parameter(parameter)) => {
                            parameter_types[*parameter].clone()
                        }
                        Some(SymbolKind::Binding(binding)) => {
                            binding_result_types[*binding].clone()
                        }
                        Some(SymbolKind::StepResult(producer)) if *producer >= preceding_steps => {
                            return Err(InkScriptTypeDiagnosticCode::ForwardReference);
                        }
                        Some(SymbolKind::StepResult(producer)) => {
                            if !step_enabled[*producer] {
                                return Err(InkScriptTypeDiagnosticCode::UnavailableResult);
                            }
                            resolve_step_result_segments(
                                &step_results[*producer],
                                segments,
                                schema,
                            )?
                        }
                        None => {
                            return Err(InkScriptTypeDiagnosticCode::UndefinedValueSymbol);
                        }
                    };
                    if matches!(value_names.get(root), Some(SymbolKind::StepResult(_))) {
                        Ok(root_type)
                    } else {
                        resolve_reference_segments(root_type, segments, schema)
                    }
                };
                let typed_arguments = type_record(
                    arguments,
                    &format!("{kind}_assert"),
                    assertion_schema.fields,
                    schema,
                    asset_names,
                    &mut resolve_reference,
                    source_id,
                    range,
                    &format!("program.assertions[{program_index}].{kind}"),
                )?;
                let converted_index = u32::try_from(program_index).map_err(|_| {
                    InkScriptTypeDiagnostic::new(
                        InkScriptTypeDiagnosticCode::NumericOverflow,
                        source_id,
                        range,
                        format!("program.assertions[{program_index}]"),
                    )
                })?;
                let consumer = InkScriptDependencyNode {
                    kind: InkScriptDependencyNodeKind::Assert,
                    name: kind.clone(),
                    step_index: None,
                    program_index: Some(converted_index),
                };
                let mut uses = Vec::new();
                collect_dependency_uses_record(arguments, &mut uses);
                for dependency in uses {
                    let dependency = dependency_node(
                        dependency,
                        value_names,
                        step_syntax,
                        asset_names,
                        source_id,
                        range,
                        &format!("program.assertions[{program_index}].{kind}"),
                    )?;
                    push_dependency_edge(
                        &mut dependency_edges,
                        consumer.clone(),
                        dependency,
                        limits,
                        source_id,
                        range,
                    )?;
                }
                program.push(InkScriptTypedProgramNode::Assert(assertions.len()));
                assertions.push(InkScriptTypedAssert {
                    kind: kind.clone(),
                    comparison: assertion_schema.comparison,
                    arguments: typed_arguments,
                    source_range: range,
                    program_index: converted_index,
                });
            }
        }
    }

    let groups = build_step_groups(&steps, source_id)?;
    Ok(AnalyzedProgram {
        assertions,
        steps,
        program,
        groups,
        dependency_edges,
    })
}

pub(crate) fn resolve_step_result_segments(
    results: &[InkScriptTypedStepResult],
    segments: &[InkScriptReferenceSegment],
    schema: &InkScriptSchemaView<'_>,
) -> Result<InkScriptResolvedType, InkScriptTypeDiagnosticCode> {
    let Some((InkScriptReferenceSegment::Field(field), remaining)) = segments.split_first() else {
        return Err(InkScriptTypeDiagnosticCode::UnknownResultField);
    };
    let result = results
        .iter()
        .find(|result| result.name == *field)
        .ok_or(InkScriptTypeDiagnosticCode::UnknownResultField)?;
    if result.cardinality == InkScriptResultCardinality::Scalar
        && matches!(remaining.first(), Some(InkScriptReferenceSegment::Index(_)))
    {
        return Err(InkScriptTypeDiagnosticCode::InvalidResultIndex);
    }
    resolve_reference_segments(result.value_type.clone(), remaining, schema)
}

fn collect_dependency_uses_record(record: &InkScriptRecord, uses: &mut Vec<DependencyUse>) {
    for value in record.0.values() {
        collect_dependency_uses(value, uses);
    }
}

fn collect_dependency_uses(value: &InkScriptValue, uses: &mut Vec<DependencyUse>) {
    match value {
        InkScriptValue::Reference { root, segments } => uses.push(DependencyUse::Value {
            root: root.clone(),
            segments: segments.clone(),
        }),
        InkScriptValue::AssetReference(name) => uses.push(DependencyUse::Asset(name.clone())),
        InkScriptValue::Constructor { arguments, .. } | InkScriptValue::List(arguments) => {
            for argument in arguments {
                collect_dependency_uses(argument, uses);
            }
        }
        InkScriptValue::Record(record) => collect_dependency_uses_record(record, uses),
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn dependency_node(
    dependency: DependencyUse,
    symbols: &BTreeMap<String, SymbolKind>,
    steps: &[&InkScriptProgramStatement],
    asset_names: &BTreeSet<String>,
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
    path: &str,
) -> Result<InkScriptDependencyNode, InkScriptTypeDiagnostic> {
    match dependency {
        DependencyUse::Asset(name) => {
            if !asset_names.contains(&name) {
                return Err(InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::UndefinedAssetSymbol,
                    source_id,
                    range,
                    format!("{path}.asset.{name}"),
                ));
            }
            Ok(InkScriptDependencyNode {
                kind: InkScriptDependencyNodeKind::Asset,
                name,
                step_index: None,
                program_index: None,
            })
        }
        DependencyUse::Value { root, segments } => match symbols.get(&root) {
            Some(SymbolKind::Parameter(_)) => Ok(InkScriptDependencyNode {
                kind: InkScriptDependencyNodeKind::Parameter,
                name: root,
                step_index: None,
                program_index: None,
            }),
            Some(SymbolKind::Binding(_)) => Ok(InkScriptDependencyNode {
                kind: InkScriptDependencyNodeKind::Binding,
                name: root,
                step_index: None,
                program_index: None,
            }),
            Some(SymbolKind::StepResult(index)) => {
                let alias = match steps[*index] {
                    InkScriptProgramStatement::Step {
                        result_alias: Some(alias),
                        ..
                    } => alias.clone(),
                    _ => root,
                };
                let _ = segments;
                Ok(InkScriptDependencyNode {
                    kind: InkScriptDependencyNodeKind::StepResult,
                    name: alias,
                    step_index: Some(u32::try_from(*index).map_err(|_| {
                        InkScriptTypeDiagnostic::new(
                            InkScriptTypeDiagnosticCode::NumericOverflow,
                            source_id,
                            range,
                            path,
                        )
                    })?),
                    program_index: None,
                })
            }
            None => Err(InkScriptTypeDiagnostic::new(
                InkScriptTypeDiagnosticCode::UndefinedValueSymbol,
                source_id,
                range,
                format!("{path}.${root}"),
            )),
        },
    }
}

fn push_dependency_edge(
    edges: &mut Vec<InkScriptDependencyEdge>,
    consumer: InkScriptDependencyNode,
    dependency: InkScriptDependencyNode,
    limits: InkScriptAnalysisLimits,
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
) -> Result<(), InkScriptTypeDiagnostic> {
    if edges.len() >= limits.dependency_edges {
        return Err(InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::ResourceLimit,
            source_id,
            range,
            "dependency_edges",
        ));
    }
    edges.push(InkScriptDependencyEdge {
        consumer,
        dependency,
    });
    Ok(())
}

fn build_step_groups(
    steps: &[InkScriptTypedStep],
    source_id: InkScriptSourceId,
) -> Result<Vec<InkScriptStepGroup>, InkScriptTypeDiagnostic> {
    let mut groups = Vec::<InkScriptStepGroup>::new();
    for (index, step) in steps.iter().enumerate() {
        let Some(key) = step.editor_group.as_ref() else {
            continue;
        };
        let index = u32::try_from(index).map_err(|_| {
            InkScriptTypeDiagnostic::new(
                InkScriptTypeDiagnosticCode::NumericOverflow,
                source_id,
                step.source_range,
                "program.editor_group",
            )
        })?;
        if let Some(group) = groups.last_mut().filter(|group| group.key == *key) {
            group.step_count = group.step_count.checked_add(1).ok_or_else(|| {
                InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::NumericOverflow,
                    source_id,
                    step.source_range,
                    "program.editor_group",
                )
            })?;
        } else {
            groups.push(InkScriptStepGroup {
                key: key.clone(),
                first_step: index,
                step_count: 1,
            });
        }
    }
    Ok(groups)
}

fn insert_value_name(
    symbols: &mut BTreeMap<String, SymbolKind>,
    name: &str,
    symbol: SymbolKind,
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
) -> Result<(), InkScriptTypeDiagnostic> {
    if symbols.insert(name.to_owned(), symbol).is_some() {
        return Err(InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::DuplicateValueSymbol,
            source_id,
            range,
            format!("value_namespace.{name}"),
        ));
    }
    Ok(())
}

fn selector_result_type(
    binding: &InkScriptBinding,
    schema: &InkScriptSchemaView<'_>,
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
) -> Result<InkScriptResolvedType, InkScriptTypeDiagnostic> {
    let reference_type = schema
        .selector_result_type(&binding.entity)
        .ok_or_else(|| {
            InkScriptTypeDiagnostic::new(
                InkScriptTypeDiagnosticCode::UnknownType,
                source_id,
                range,
                format!("bindings.{}.entity", binding.name),
            )
        })?;
    if enum_field(&binding.selector, "cardinality") == Some("all") {
        Ok(InkScriptResolvedType::new(format!(
            "list<{reference_type}>"
        )))
    } else {
        Ok(InkScriptResolvedType::new(reference_type))
    }
}

fn selector_cardinality(
    selector: &InkScriptTypedValue,
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
    name: &str,
) -> Result<InkScriptSelectorCardinality, InkScriptTypeDiagnostic> {
    match typed_enum_field(selector, "cardinality").unwrap_or("one") {
        "one" => Ok(InkScriptSelectorCardinality::One),
        "first" => Ok(InkScriptSelectorCardinality::First),
        "all" => Ok(InkScriptSelectorCardinality::All),
        _ => Err(InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::ValueOutOfRange,
            source_id,
            range,
            format!("bindings.{name}.cardinality"),
        )),
    }
}

fn selector_missing_policy(
    selector: &InkScriptTypedValue,
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
    name: &str,
) -> Result<InkScriptSelectorMissingPolicy, InkScriptTypeDiagnostic> {
    match typed_enum_field(selector, "missing").unwrap_or("error") {
        "error" => Ok(InkScriptSelectorMissingPolicy::Error),
        "skip_dependents" => Ok(InkScriptSelectorMissingPolicy::SkipDependents),
        _ => Err(InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::ValueOutOfRange,
            source_id,
            range,
            format!("bindings.{name}.missing"),
        )),
    }
}

fn collect_binding_dependencies(
    bindings: &[&InkScriptBinding],
    symbols: &BTreeMap<String, SymbolKind>,
    source_id: InkScriptSourceId,
    ranges: &[InkScriptSourceRange],
) -> Result<Vec<Vec<usize>>, InkScriptTypeDiagnostic> {
    let mut result = Vec::with_capacity(bindings.len());
    for (index, binding) in bindings.iter().enumerate() {
        let mut roots = Vec::new();
        collect_reference_roots_record(&binding.selector, &mut roots);
        let mut dependencies = BTreeSet::new();
        for root in roots {
            match symbols.get(root) {
                Some(SymbolKind::Parameter(_)) => {}
                Some(SymbolKind::Binding(binding_index)) => {
                    dependencies.insert(*binding_index);
                }
                Some(SymbolKind::StepResult(_)) => {
                    return Err(InkScriptTypeDiagnostic::new(
                        InkScriptTypeDiagnosticCode::ForwardReference,
                        source_id,
                        ranges[index],
                        format!("bindings.{}.reference.{root}", binding.name),
                    ));
                }
                None => {
                    return Err(InkScriptTypeDiagnostic::new(
                        InkScriptTypeDiagnosticCode::UndefinedValueSymbol,
                        source_id,
                        ranges[index],
                        format!("bindings.{}.reference.{root}", binding.name),
                    ));
                }
            }
        }
        result.push(dependencies.into_iter().collect());
    }
    Ok(result)
}

fn reject_dependency_cycles(
    dependencies: &[Vec<usize>],
    source_id: InkScriptSourceId,
    ranges: &[InkScriptSourceRange],
    bindings: &[&InkScriptBinding],
) -> Result<(), InkScriptTypeDiagnostic> {
    let mut remaining = dependencies.iter().map(Vec::len).collect::<Vec<_>>();
    let mut consumers = vec![Vec::new(); dependencies.len()];
    for (consumer, values) in dependencies.iter().enumerate() {
        for dependency in values {
            consumers[*dependency].push(consumer);
        }
    }
    let mut ready = remaining
        .iter()
        .enumerate()
        .filter_map(|(index, count)| (*count == 0).then_some(index))
        .collect::<BTreeSet<_>>();
    let mut visited = 0usize;
    while let Some(index) = ready.pop_first() {
        visited += 1;
        for consumer in &consumers[index] {
            remaining[*consumer] -= 1;
            if remaining[*consumer] == 0 {
                ready.insert(*consumer);
            }
        }
    }
    if visited != dependencies.len() {
        let index = remaining
            .iter()
            .position(|count| *count != 0)
            .expect("unvisited binding exists");
        return Err(InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::DependencyCycle,
            source_id,
            ranges[index],
            format!("bindings.{}", bindings[index].name),
        ));
    }
    Ok(())
}

fn resolve_type_reference(
    value: &InkScriptTypeReference,
    schema: &InkScriptSchemaView<'_>,
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
    path: &str,
) -> Result<InkScriptResolvedType, InkScriptTypeDiagnostic> {
    match value {
        InkScriptTypeReference::Named(name) => {
            if schema.type_kind(name).is_none() {
                return Err(InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::UnknownType,
                    source_id,
                    range,
                    format!("{path}.type.{name}"),
                ));
            }
            Ok(InkScriptResolvedType::new(name))
        }
        InkScriptTypeReference::List(child) => {
            let child = resolve_type_reference(child, schema, source_id, range, path)?;
            Ok(InkScriptResolvedType::new(format!(
                "list<{}>",
                child.name()
            )))
        }
        InkScriptTypeReference::Nullable(child) => {
            let child = resolve_type_reference(child, schema, source_id, range, path)?;
            Ok(InkScriptResolvedType::new(format!(
                "nullable<{}>",
                child.name()
            )))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn type_value(
    value: &InkScriptValue,
    expected: &InkScriptResolvedType,
    schema: &InkScriptSchemaView<'_>,
    asset_names: &BTreeSet<String>,
    resolve_reference: &mut impl FnMut(
        &str,
        &[InkScriptReferenceSegment],
    )
        -> Result<InkScriptResolvedType, InkScriptTypeDiagnosticCode>,
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
    path: &str,
) -> Result<InkScriptTypedValue, InkScriptTypeDiagnostic> {
    if let Some(inner) = unwrap_type(expected.name(), "nullable<") {
        if matches!(value, InkScriptValue::None) {
            return Ok(InkScriptTypedValue::new(
                expected.name(),
                InkScriptTypedValueKind::None,
            ));
        }
        return type_value(
            value,
            &InkScriptResolvedType::new(inner),
            schema,
            asset_names,
            resolve_reference,
            source_id,
            range,
            path,
        );
    }
    if let Some(element) = unwrap_type(expected.name(), "list<") {
        if matches!(value, InkScriptValue::Reference { .. }) {
            // A typed result or selector may provide the complete list without scalar/list
            // coercion. The reference arm below performs exact compatibility checking.
        } else if let InkScriptValue::List(values) = value {
            let element_type = InkScriptResolvedType::new(element);
            let typed = values
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    type_value(
                        value,
                        &element_type,
                        schema,
                        asset_names,
                        resolve_reference,
                        source_id,
                        range,
                        &format!("{path}[{index}]"),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(InkScriptTypedValue::new(
                expected.name(),
                InkScriptTypedValueKind::List(typed),
            ));
        } else {
            return Err(type_error(source_id, range, path));
        }
    }

    let typed = match value {
        InkScriptValue::Boolean(value) if expected.name() == "bool" => {
            InkScriptTypedValue::new("bool", InkScriptTypedValueKind::Boolean(*value))
        }
        InkScriptValue::Integer(value) => type_integer(value, expected, source_id, range, path)?,
        InkScriptValue::Decimal(value) if expected.name() == "q16" => InkScriptTypedValue::new(
            "q16",
            InkScriptTypedValueKind::Q16(decimal_to_q16(value).ok_or_else(|| {
                InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::NumericOverflow,
                    source_id,
                    range,
                    path,
                )
            })?),
        ),
        InkScriptValue::String(value)
            if matches!(schema.type_kind(expected.name()), Some("string")) =>
        {
            InkScriptTypedValue::new(
                expected.name(),
                InkScriptTypedValueKind::String(value.clone()),
            )
        }
        InkScriptValue::Uuid(value) if expected.name() == "uuid" => {
            InkScriptTypedValue::new("uuid", InkScriptTypedValueKind::Uuid(value.clone()))
        }
        InkScriptValue::Digest(value) if expected.name() == "digest" => {
            InkScriptTypedValue::new("digest", InkScriptTypedValueKind::Digest(value.clone()))
        }
        InkScriptValue::Base64(value) if expected.name() == "base64" => {
            InkScriptTypedValue::new("base64", InkScriptTypedValueKind::Base64(value.clone()))
        }
        InkScriptValue::Enum(member) => {
            let Some(members) = schema.enum_members(expected.name()) else {
                return Err(type_error(source_id, range, path));
            };
            if !members.contains(&member.as_str()) {
                return Err(InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::ValueOutOfRange,
                    source_id,
                    range,
                    path,
                ));
            }
            InkScriptTypedValue::new(
                expected.name(),
                InkScriptTypedValueKind::Enum(member.clone()),
            )
        }
        InkScriptValue::Constructor { name, arguments } => type_constructor(
            name,
            arguments,
            expected,
            schema,
            asset_names,
            resolve_reference,
            source_id,
            range,
            path,
        )?,
        InkScriptValue::Record(record) => {
            let Some(fields) = schema.record(expected.name()) else {
                return Err(type_error(source_id, range, path));
            };
            type_record(
                record,
                expected.name(),
                fields,
                schema,
                asset_names,
                resolve_reference,
                source_id,
                range,
                path,
            )?
        }
        InkScriptValue::AssetReference(name) if expected.name() == "asset_ref" => {
            if !asset_names.contains(name) {
                return Err(InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::UndefinedAssetSymbol,
                    source_id,
                    range,
                    format!("{path}.asset.{name}"),
                ));
            }
            InkScriptTypedValue::new(
                "asset_ref",
                InkScriptTypedValueKind::AssetReference(name.clone()),
            )
        }
        InkScriptValue::Reference { root, segments } => {
            let actual = resolve_reference(root, segments).map_err(|code| {
                InkScriptTypeDiagnostic::new(code, source_id, range, format!("{path}.${root}"))
            })?;
            if !types_compatible(actual.name(), expected.name()) {
                let cardinality_mismatch = unwrap_type(actual.name(), "list<").is_some()
                    != unwrap_type(expected.name(), "list<").is_some();
                return Err(InkScriptTypeDiagnostic::new(
                    if cardinality_mismatch {
                        InkScriptTypeDiagnosticCode::ResultCardinalityMismatch
                    } else {
                        InkScriptTypeDiagnosticCode::TypeMismatch
                    },
                    source_id,
                    range,
                    path,
                ));
            }
            InkScriptTypedValue::new(
                actual.name(),
                InkScriptTypedValueKind::Reference {
                    root: root.clone(),
                    segments: segments.clone(),
                },
            )
        }
        _ => return Err(type_error(source_id, range, path)),
    };
    if !types_compatible(typed.type_name(), expected.name()) {
        return Err(type_error(source_id, range, path));
    }
    Ok(typed)
}

#[allow(clippy::too_many_arguments)]
fn type_constructor(
    name: &str,
    values: &[InkScriptValue],
    expected: &InkScriptResolvedType,
    schema: &InkScriptSchemaView<'_>,
    asset_names: &BTreeSet<String>,
    resolve_reference: &mut impl FnMut(
        &str,
        &[InkScriptReferenceSegment],
    )
        -> Result<InkScriptResolvedType, InkScriptTypeDiagnosticCode>,
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
    path: &str,
) -> Result<InkScriptTypedValue, InkScriptTypeDiagnostic> {
    let constructor = schema.constructor(name).ok_or_else(|| {
        InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::UnknownConstructor,
            source_id,
            range,
            format!("{path}.{name}"),
        )
    })?;
    if constructor.arguments.len() != values.len() {
        return Err(InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::ConstructorArity,
            source_id,
            range,
            format!("{path}.{name}"),
        ));
    }
    if !types_compatible(constructor.result, expected.name()) {
        return Err(type_error(source_id, range, path));
    }
    let mut arguments = Vec::with_capacity(values.len());
    for (value, argument) in values.iter().zip(constructor.arguments) {
        let typed = type_value(
            value,
            &InkScriptResolvedType::new(argument.type_name),
            schema,
            asset_names,
            resolve_reference,
            source_id,
            range,
            &format!("{path}.{name}.{}", argument.name),
        )?;
        validate_constraints(
            &typed,
            argument.constraints,
            source_id,
            range,
            &format!("{path}.{name}.{}", argument.name),
        )?;
        arguments.push(typed);
    }
    for (index, argument) in constructor.arguments.iter().enumerate() {
        for constraint in argument.constraints {
            if let Some(other_name) = constraint.strip_prefix("greater-than-or-equal:") {
                let other = constructor
                    .arguments
                    .iter()
                    .position(|value| value.name == other_name)
                    .expect("approved schema constraint names an argument");
                if integer_magnitude(&arguments[index]) < integer_magnitude(&arguments[other]) {
                    return Err(InkScriptTypeDiagnostic::new(
                        InkScriptTypeDiagnosticCode::ValueOutOfRange,
                        source_id,
                        range,
                        format!("{path}.{name}.{}", argument.name),
                    ));
                }
            }
        }
    }
    if constructor.result == "q16" && name == "q16" {
        let InkScriptTypedValueKind::I64(raw) = arguments[0].kind else {
            unreachable!("approved q16 constructor takes i64")
        };
        return Ok(InkScriptTypedValue::new(
            "q16",
            InkScriptTypedValueKind::Q16(raw),
        ));
    }
    Ok(InkScriptTypedValue::new(
        constructor.result,
        InkScriptTypedValueKind::Constructor {
            name: name.to_owned(),
            arguments,
        },
    ))
}

#[allow(clippy::too_many_arguments)]
fn type_record(
    record: &InkScriptRecord,
    type_name: &str,
    fields: &[InkScriptFieldSchema],
    schema: &InkScriptSchemaView<'_>,
    asset_names: &BTreeSet<String>,
    resolve_reference: &mut impl FnMut(
        &str,
        &[InkScriptReferenceSegment],
    )
        -> Result<InkScriptResolvedType, InkScriptTypeDiagnosticCode>,
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
    path: &str,
) -> Result<InkScriptTypedValue, InkScriptTypeDiagnostic> {
    let mut typed = BTreeMap::new();
    for (name, value) in &record.0 {
        let field = fields
            .iter()
            .find(|field| field.name == name)
            .ok_or_else(|| {
                InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::InvalidSemanticModel,
                    source_id,
                    range,
                    format!("{path}.{name}"),
                )
            })?;
        let value = type_value(
            value,
            &InkScriptResolvedType::new(field.type_name),
            schema,
            asset_names,
            resolve_reference,
            source_id,
            range,
            &format!("{path}.{name}"),
        )?;
        validate_constraints(
            &value,
            field.constraints,
            source_id,
            range,
            &format!("{path}.{name}"),
        )?;
        typed.insert(name.clone(), value);
    }
    validate_record_constraints(&typed, fields, source_id, range, path)?;
    Ok(InkScriptTypedValue::new(
        type_name,
        InkScriptTypedValueKind::Record(typed),
    ))
}

fn type_integer(
    value: &str,
    expected: &InkScriptResolvedType,
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
    path: &str,
) -> Result<InkScriptTypedValue, InkScriptTypeDiagnostic> {
    let overflow = || {
        InkScriptTypeDiagnostic::new(
            InkScriptTypeDiagnosticCode::NumericOverflow,
            source_id,
            range,
            path,
        )
    };
    match expected.name() {
        "u32" => value
            .parse::<u32>()
            .map(|value| InkScriptTypedValue::new("u32", InkScriptTypedValueKind::U32(value)))
            .map_err(|_| overflow()),
        "i32" => value
            .parse::<i32>()
            .map(|value| InkScriptTypedValue::new("i32", InkScriptTypedValueKind::I32(value)))
            .map_err(|_| overflow()),
        "u64" => value
            .parse::<u64>()
            .map(|value| InkScriptTypedValue::new("u64", InkScriptTypedValueKind::U64(value)))
            .map_err(|_| overflow()),
        "i64" => value
            .parse::<i64>()
            .map(|value| InkScriptTypedValue::new("i64", InkScriptTypedValueKind::I64(value)))
            .map_err(|_| overflow()),
        _ => Err(type_error(source_id, range, path)),
    }
}

fn validate_constraints(
    value: &InkScriptTypedValue,
    constraints: &[&str],
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
    path: &str,
) -> Result<(), InkScriptTypeDiagnostic> {
    if matches!(value.kind(), InkScriptTypedValueKind::None) {
        return Ok(());
    }
    for constraint in constraints {
        let valid = if *constraint == "nonzero" || *constraint == "positive" {
            integer_magnitude(value).is_some_and(|value| value > 0)
        } else if let Some(maximum) = constraint
            .strip_prefix("0..")
            .and_then(|value| value.parse::<u64>().ok())
        {
            integer_magnitude(value).is_some_and(|value| value <= maximum)
        } else if let Some(required) = constraint.strip_prefix("must-equal:") {
            match value.kind() {
                InkScriptTypedValueKind::String(value) | InkScriptTypedValueKind::Enum(value) => {
                    value == required
                }
                _ => false,
            }
        } else if *constraint == "must-be-false-in-v2" {
            matches!(value.kind(), InkScriptTypedValueKind::Boolean(false))
        } else {
            true
        };
        if !valid {
            return Err(InkScriptTypeDiagnostic::new(
                InkScriptTypeDiagnosticCode::ValueOutOfRange,
                source_id,
                range,
                path,
            ));
        }
    }
    Ok(())
}

fn validate_record_constraints(
    values: &BTreeMap<String, InkScriptTypedValue>,
    fields: &[InkScriptFieldSchema],
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
    path: &str,
) -> Result<(), InkScriptTypeDiagnostic> {
    let present = |name: &str| {
        values
            .get(name)
            .is_some_and(|value| !matches!(value.kind(), InkScriptTypedValueKind::None))
    };
    for field in fields {
        for constraint in field.constraints {
            if let Some(names) = constraint.strip_prefix("exactly-one-of:") {
                let mut choices = names.split(',');
                let first = choices.next();
                let second = choices.next();
                let extra = choices.next();
                let (Some(first), Some(second), None) = (first, second, extra) else {
                    return Err(InkScriptTypeDiagnostic::new(
                        InkScriptTypeDiagnosticCode::InvalidStrictPrecondition,
                        source_id,
                        range,
                        format!("{path}.{}", field.name),
                    ));
                };
                if [first, second]
                    .into_iter()
                    .filter(|name| present(name))
                    .count()
                    != 1
                {
                    return Err(InkScriptTypeDiagnostic::new(
                        InkScriptTypeDiagnosticCode::InvalidStrictPrecondition,
                        source_id,
                        range,
                        format!("{path}.{}", field.name),
                    ));
                }
            }
            let strict_missing = constraint
                .strip_prefix("required-with:")
                .is_some_and(|other| present(other) && !present(field.name))
                || constraint
                    .strip_prefix("requires:")
                    .is_some_and(|other| present(field.name) && !present(other));
            if strict_missing {
                return Err(InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::InvalidStrictPrecondition,
                    source_id,
                    range,
                    format!("{path}.{}", field.name),
                ));
            }
            if *constraint == "all-forbidden"
                && matches!(
                    values.get(field.name).map(InkScriptTypedValue::kind),
                    Some(InkScriptTypedValueKind::Enum(value)) if value == "all"
                )
            {
                return Err(InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::ValueOutOfRange,
                    source_id,
                    range,
                    format!("{path}.{}", field.name),
                ));
            }
            let target_type = values.get("target").map(InkScriptTypedValue::type_name);
            if present(field.name)
                && ((*constraint == "layer-only" && target_type != Some("layer_ref"))
                    || (*constraint == "plane-only" && target_type != Some("plane_ref")))
            {
                return Err(InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::TypeMismatch,
                    source_id,
                    range,
                    format!("{path}.{}", field.name),
                ));
            }
            if *constraint == "none-when-empty"
                && matches!(
                    values.get("empty").map(InkScriptTypedValue::kind),
                    Some(InkScriptTypedValueKind::Boolean(true))
                )
                && present(field.name)
            {
                return Err(InkScriptTypeDiagnostic::new(
                    InkScriptTypeDiagnosticCode::ValueOutOfRange,
                    source_id,
                    range,
                    format!("{path}.{}", field.name),
                ));
            }
        }
    }
    Ok(())
}

fn resolve_reference_segments(
    mut value_type: InkScriptResolvedType,
    segments: &[InkScriptReferenceSegment],
    schema: &InkScriptSchemaView<'_>,
) -> Result<InkScriptResolvedType, InkScriptTypeDiagnosticCode> {
    for segment in segments {
        match segment {
            InkScriptReferenceSegment::Field(name) => {
                let fields = schema
                    .record(value_type.name())
                    .ok_or(InkScriptTypeDiagnosticCode::TypeMismatch)?;
                let field = fields
                    .iter()
                    .find(|field| field.name == name)
                    .ok_or(InkScriptTypeDiagnosticCode::TypeMismatch)?;
                value_type = InkScriptResolvedType::new(field.type_name);
            }
            InkScriptReferenceSegment::Index(index) => {
                index
                    .parse::<u64>()
                    .map_err(|_| InkScriptTypeDiagnosticCode::NumericOverflow)?;
                let element = unwrap_type(value_type.name(), "list<")
                    .ok_or(InkScriptTypeDiagnosticCode::TypeMismatch)?;
                value_type = InkScriptResolvedType::new(element);
            }
        }
    }
    Ok(value_type)
}

fn types_compatible(actual: &str, expected: &str) -> bool {
    actual == expected
        || (expected == "pixel_value"
            && matches!(actual, "mask8" | "gray8" | "gray16" | "rgba8" | "rgba16"))
        || (expected == "entity_ref"
            && matches!(
                actual,
                "layer_ref"
                    | "plane_ref"
                    | "guide_ref"
                    | "vector_path_ref"
                    | "vector_fill_ref"
                    | "annotation_ref"
                    | "shooting_frame_ref"
                    | "vanishing_point_ref"
                    | "light_table_set_ref"
                    | "light_table_item_ref"
            ))
}

fn is_closed_literal(value: &InkScriptValue) -> bool {
    match value {
        InkScriptValue::Reference { .. } | InkScriptValue::AssetReference(_) => false,
        InkScriptValue::Constructor { arguments, .. } | InkScriptValue::List(arguments) => {
            arguments.iter().all(is_closed_literal)
        }
        InkScriptValue::Record(record) => record.0.values().all(is_closed_literal),
        _ => true,
    }
}

fn collect_reference_roots_record<'a>(record: &'a InkScriptRecord, roots: &mut Vec<&'a str>) {
    for value in record.0.values() {
        collect_reference_roots(value, roots);
    }
}

fn collect_reference_roots<'a>(value: &'a InkScriptValue, roots: &mut Vec<&'a str>) {
    match value {
        InkScriptValue::Reference { root, .. } => roots.push(root),
        InkScriptValue::Constructor { arguments, .. } | InkScriptValue::List(arguments) => {
            for argument in arguments {
                collect_reference_roots(argument, roots);
            }
        }
        InkScriptValue::Record(record) => collect_reference_roots_record(record, roots),
        _ => {}
    }
}

fn declaration_ranges(parsed: &InkScriptParsed<'_>) -> DeclarationRanges {
    let source = parsed.cst().source();
    let line_map = source.line_map();
    let document = line_map
        .range(parsed.cst().root().span())
        .expect("CST root span belongs to its source");
    let mut parameters = Vec::new();
    let mut bindings = Vec::new();
    let mut assets = Vec::new();
    let mut steps = Vec::new();
    let mut program = Vec::new();
    collect_declaration_ranges(
        parsed.cst().root(),
        &line_map,
        &mut parameters,
        &mut bindings,
        &mut assets,
        &mut steps,
        &mut program,
    );
    DeclarationRanges {
        document,
        parameters,
        bindings,
        assets,
        steps,
        program,
    }
}

fn collect_declaration_ranges(
    node: &InkScriptCstNode,
    line_map: &super::source::InkScriptLineMap<'_>,
    parameters: &mut Vec<InkScriptSourceRange>,
    bindings: &mut Vec<InkScriptSourceRange>,
    assets: &mut Vec<InkScriptSourceRange>,
    steps: &mut Vec<InkScriptSourceRange>,
    program: &mut Vec<InkScriptSourceRange>,
) {
    let range = || {
        line_map
            .range(node.span())
            .expect("CST declaration span belongs to its source")
    };
    match node.kind() {
        InkScriptCstNodeKind::ParameterDeclaration => parameters.push(range()),
        InkScriptCstNodeKind::BindingDeclaration => bindings.push(range()),
        InkScriptCstNodeKind::AssetDeclaration => assets.push(range()),
        InkScriptCstNodeKind::AssertStatement => {
            program.push(range());
        }
        InkScriptCstNodeKind::StepStatement => {
            steps.push(range());
            program.push(range());
        }
        _ => {}
    }
    for child in node.children() {
        collect_declaration_ranges(
            child, line_map, parameters, bindings, assets, steps, program,
        );
    }
}

fn align_ranges(
    ranges: &[InkScriptSourceRange],
    length: usize,
    fallback: InkScriptSourceRange,
) -> Vec<InkScriptSourceRange> {
    (0..length)
        .map(|index| ranges.get(index).copied().unwrap_or(fallback))
        .collect()
}

fn string_field<'a>(record: &'a InkScriptRecord, name: &str) -> Option<&'a str> {
    match record.0.get(name) {
        Some(InkScriptValue::String(value)) => Some(value),
        _ => None,
    }
}

fn enum_field<'a>(record: &'a InkScriptRecord, name: &str) -> Option<&'a str> {
    match record.0.get(name) {
        Some(InkScriptValue::Enum(value)) => Some(value),
        _ => None,
    }
}

fn typed_enum_field<'a>(record: &'a InkScriptTypedValue, name: &str) -> Option<&'a str> {
    let InkScriptTypedValueKind::Record(fields) = record.kind() else {
        return None;
    };
    match fields.get(name).map(InkScriptTypedValue::kind) {
        Some(InkScriptTypedValueKind::Enum(value)) => Some(value),
        _ => None,
    }
}

fn unwrap_type<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value.strip_prefix(prefix)?.strip_suffix('>')
}

fn type_error(
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
    path: &str,
) -> InkScriptTypeDiagnostic {
    InkScriptTypeDiagnostic::new(
        InkScriptTypeDiagnosticCode::TypeMismatch,
        source_id,
        range,
        path,
    )
}

fn run_error(
    model: &InkScriptDeclarationModel,
    parameter: Option<&InkScriptTypedParameter>,
    code: InkScriptTypeDiagnosticCode,
    path: impl Into<String>,
) -> InkScriptTypeDiagnostic {
    InkScriptTypeDiagnostic::new(
        code,
        model.source_id,
        parameter.map_or(model.document_range, |value| value.source_range),
        path,
    )
}

fn integer_magnitude(value: &InkScriptTypedValue) -> Option<u64> {
    match value.kind() {
        InkScriptTypedValueKind::U32(value) => Some(u64::from(*value)),
        InkScriptTypedValueKind::I32(value) => u64::try_from(*value).ok(),
        InkScriptTypedValueKind::U64(value) => Some(*value),
        InkScriptTypedValueKind::I64(value) => u64::try_from(*value).ok(),
        InkScriptTypedValueKind::Q16(value) => u64::try_from(*value).ok(),
        _ => None,
    }
}

fn decimal_to_q16(value: &str) -> Option<i64> {
    let (negative, unsigned) = value
        .strip_prefix('-')
        .map_or((false, value), |value| (true, value));
    let (whole, fraction) = unsigned.split_once('.')?;
    let whole = whole.parse::<u128>().ok()?;
    let mut product = fraction
        .bytes()
        .rev()
        .map(|digit| u32::from(digit - b'0'))
        .collect::<Vec<_>>();
    let mut carry = 0u32;
    for digit in &mut product {
        let current = *digit * 65_536 + carry;
        *digit = current % 10;
        carry = current / 10;
    }
    while carry != 0 {
        product.push(carry % 10);
        carry /= 10;
    }
    let fractional_digits = fraction.len();
    if product.len() < fractional_digits {
        product.resize(fractional_digits, 0);
    }
    let mut fractional_raw = 0u64;
    for digit in product.iter().skip(fractional_digits).rev() {
        fractional_raw = fractional_raw
            .checked_mul(10)?
            .checked_add(u64::from(*digit))?;
    }
    let remainder_high = product
        .get(fractional_digits.wrapping_sub(1))
        .copied()
        .unwrap_or(0);
    let remainder_low_nonzero = fractional_digits > 1
        && product[..fractional_digits - 1]
            .iter()
            .any(|digit| *digit != 0);
    if remainder_high > 5
        || (remainder_high == 5 && (remainder_low_nonzero || fractional_raw % 2 == 1))
    {
        fractional_raw = fractional_raw.checked_add(1)?;
    }
    let magnitude = whole
        .checked_mul(65_536)?
        .checked_add(u128::from(fractional_raw))?;
    if negative {
        if magnitude > (1u128 << 63) {
            None
        } else if magnitude == (1u128 << 63) {
            Some(i64::MIN)
        } else {
            Some(-(magnitude as i64))
        }
    } else {
        i64::try_from(magnitude).ok()
    }
}
