use std::collections::BTreeSet;

use super::parser::MAX_INKSCRIPT_CONTAINER_ELEMENTS;

/// Stable semantic/canonicalization failure categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptSemanticErrorCode {
    InvalidSyntax,
    InvalidSchema,
    UnknownRecordSchema,
    UnknownCommandSchema,
    UnknownFieldSchema,
    MissingRequiredField,
    InvalidGeneratedName,
}

impl InkScriptSemanticErrorCode {
    /// Returns the locale-independent diagnostic spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidSyntax => "invalid_syntax",
            Self::InvalidSchema => "invalid_schema",
            Self::UnknownRecordSchema => "unknown_record_schema",
            Self::UnknownCommandSchema => "unknown_command_schema",
            Self::UnknownFieldSchema => "unknown_field_schema",
            Self::MissingRequiredField => "missing_required_field",
            Self::InvalidGeneratedName => "invalid_generated_name",
        }
    }
}

/// A semantic/canonicalization error with a stable code and non-localized schema path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptSemanticError {
    code: InkScriptSemanticErrorCode,
    path: String,
}

impl InkScriptSemanticError {
    pub(crate) fn new(code: InkScriptSemanticErrorCode, path: impl Into<String>) -> Self {
        Self {
            code,
            path: path.into(),
        }
    }

    /// Returns the stable error category.
    pub const fn code(&self) -> InkScriptSemanticErrorCode {
        self.code
    }

    /// Returns the non-localized schema or syntax path associated with the error.
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// A schema default whose explicit spelling is omitted by canonical output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptSchemaDefault {
    None,
    Boolean(bool),
    Enum(&'static str),
    EmptyList,
    EmptyRecord,
}

/// One closed-record field from the composed language/catalog schema view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptFieldSchema {
    pub(crate) name: &'static str,
    pub(crate) type_name: &'static str,
    pub(crate) required: bool,
    pub(crate) default: Option<InkScriptSchemaDefault>,
    pub(crate) canonical_order: u32,
    pub(crate) constraints: &'static [&'static str],
}

impl InkScriptFieldSchema {
    /// Declares a required field. Required fields are emitted even when their value resembles a
    /// schema default.
    pub const fn required(
        name: &'static str,
        type_name: &'static str,
        canonical_order: u32,
    ) -> Self {
        Self {
            name,
            type_name,
            required: true,
            default: None,
            canonical_order,
            constraints: &[],
        }
    }

    /// Declares an optional field with its semantic default.
    pub const fn optional(
        name: &'static str,
        type_name: &'static str,
        default: InkScriptSchemaDefault,
        canonical_order: u32,
    ) -> Self {
        Self {
            name,
            type_name,
            required: false,
            default: Some(default),
            canonical_order,
            constraints: &[],
        }
    }
}

/// A named closed record contributed by a bounded test or command schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptRecordSchema {
    pub(crate) name: &'static str,
    pub(crate) fields: &'static [InkScriptFieldSchema],
}

impl InkScriptRecordSchema {
    pub const fn new(name: &'static str, fields: &'static [InkScriptFieldSchema]) -> Self {
        Self { name, fields }
    }
}

/// Static availability of one command-result field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptResultAvailability {
    AlwaysOnSuccess,
    OnlyOnChange,
}

/// Closed cardinality implemented by the bounded pre-catalog command schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptResultCardinality {
    Scalar,
    OrderedList,
}

/// One result field contributed by a bounded test command schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptCommandResultSchema {
    pub(crate) name: &'static str,
    pub(crate) element_type: &'static str,
    pub(crate) availability: InkScriptResultAvailability,
    pub(crate) cardinality: InkScriptResultCardinality,
    pub(crate) canonical_order: u32,
}

impl InkScriptCommandResultSchema {
    /// Declares a scalar result field.
    pub const fn scalar(
        name: &'static str,
        value_type: &'static str,
        availability: InkScriptResultAvailability,
        canonical_order: u32,
    ) -> Self {
        Self {
            name,
            element_type: value_type,
            availability,
            cardinality: InkScriptResultCardinality::Scalar,
            canonical_order,
        }
    }

    /// Declares a variable-length result field with deterministic element order.
    pub const fn ordered_list(
        name: &'static str,
        element_type: &'static str,
        availability: InkScriptResultAvailability,
        canonical_order: u32,
    ) -> Self {
        Self {
            name,
            element_type,
            availability,
            cardinality: InkScriptResultCardinality::OrderedList,
            canonical_order,
        }
    }

    pub(crate) fn resolved_type(self) -> String {
        match self.cardinality {
            InkScriptResultCardinality::Scalar => self.element_type.to_owned(),
            InkScriptResultCardinality::OrderedList => format!("list<{}>", self.element_type),
        }
    }
}

/// The argument record for one exact command name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptCommandSchema {
    pub(crate) name: &'static str,
    pub(crate) fields: &'static [InkScriptFieldSchema],
    pub(crate) results: &'static [InkScriptCommandResultSchema],
}

impl InkScriptCommandSchema {
    pub const fn new(name: &'static str, fields: &'static [InkScriptFieldSchema]) -> Self {
        Self {
            name,
            fields,
            results: &[],
        }
    }

    /// Declares one bounded test command and its closed result record.
    pub const fn with_results(
        name: &'static str,
        fields: &'static [InkScriptFieldSchema],
        results: &'static [InkScriptCommandResultSchema],
    ) -> Self {
        Self {
            name,
            fields,
            results,
        }
    }
}

/// Exact-current language-core schemas composed with a bounded private command schema set.
///
/// Before catalog ratification, callers must explicitly provide private command schemas; an empty
/// slice accepts no `invoke` command.
#[derive(Clone, Debug)]
pub struct InkScriptSchemaView<'schema> {
    records: &'schema [InkScriptRecordSchema],
    commands: &'schema [InkScriptCommandSchema],
}

impl<'schema> InkScriptSchemaView<'schema> {
    /// Composes the generated exact-current language v1 projection with caller-provided private
    /// schemas. The private catalog draft is never read by this API.
    pub fn exact_current(
        records: &'schema [InkScriptRecordSchema],
        commands: &'schema [InkScriptCommandSchema],
    ) -> Result<Self, InkScriptSemanticError> {
        if records.len() > MAX_INKSCRIPT_CONTAINER_ELEMENTS
            || commands.len() > MAX_INKSCRIPT_CONTAINER_ELEMENTS
        {
            return Err(invalid_schema("schema_view"));
        }
        let mut names = BTreeSet::new();
        for record in records {
            if !is_schema_name(record.name)
                || GENERATED_TYPE_NAMES.contains(&record.name)
                || !names.insert(record.name)
            {
                return Err(invalid_schema(record.name));
            }
            validate_fields(record.name, record.fields, records)?;
        }
        names.clear();
        for command in commands {
            if !is_identifier(command.name) || !names.insert(command.name) {
                return Err(invalid_schema(command.name));
            }
            validate_fields(command.name, command.fields, records)?;
            validate_results(command.name, command.results, records)?;
        }
        Ok(Self { records, commands })
    }

    pub(crate) fn record(&self, name: &str) -> Option<&[InkScriptFieldSchema]> {
        generated_record(GENERATED_RECORDS, name).or_else(|| {
            self.records
                .iter()
                .find(|record| record.name == name)
                .map(|record| record.fields)
        })
    }

    pub(crate) fn command(&self, name: &str) -> Option<&[InkScriptFieldSchema]> {
        self.commands
            .iter()
            .find(|command| command.name == name)
            .map(|command| command.fields)
    }

    pub(crate) fn command_schema(&self, name: &str) -> Option<&InkScriptCommandSchema> {
        self.commands.iter().find(|command| command.name == name)
    }

    pub(crate) fn selector(&self, name: &str) -> Option<&'static [InkScriptFieldSchema]> {
        generated_record(GENERATED_SELECTORS, name)
    }

    pub(crate) fn selector_result_type(&self, name: &str) -> Option<&'static str> {
        GENERATED_SELECTOR_RESULTS
            .iter()
            .find(|selector| selector.name == name)
            .map(|selector| selector.reference_type)
    }

    pub(crate) fn type_kind(&self, name: &str) -> Option<&'static str> {
        GENERATED_TYPES
            .iter()
            .find(|value_type| value_type.name == name)
            .map(|value_type| value_type.kind)
            .or_else(|| {
                self.records
                    .iter()
                    .any(|record| record.name == name)
                    .then_some("record")
            })
    }

    pub(crate) fn enum_members(&self, name: &str) -> Option<&'static [&'static str]> {
        GENERATED_ENUMS
            .iter()
            .find(|value_enum| value_enum.name == name)
            .map(|value_enum| value_enum.members)
    }

    pub(crate) fn constructor(&self, name: &str) -> Option<&'static GeneratedConstructor> {
        GENERATED_CONSTRUCTORS
            .iter()
            .find(|constructor| constructor.name == name)
    }

    pub(crate) fn assertion(&self, name: &str) -> Option<&'static [InkScriptFieldSchema]> {
        generated_record(GENERATED_ASSERTIONS, name)
    }

    pub(crate) fn section_order(&self, name: &str) -> Option<usize> {
        GENERATED_SECTION_ORDER
            .iter()
            .position(|section| *section == name)
    }
}

#[derive(Clone, Copy)]
struct GeneratedRecord {
    name: &'static str,
    fields: &'static [InkScriptFieldSchema],
}

#[derive(Clone, Copy)]
struct GeneratedType {
    name: &'static str,
    kind: &'static str,
}

#[derive(Clone, Copy)]
struct GeneratedEnum {
    name: &'static str,
    members: &'static [&'static str],
}

#[derive(Clone, Copy)]
pub(crate) struct GeneratedConstructorArgument {
    pub(crate) name: &'static str,
    pub(crate) type_name: &'static str,
    pub(crate) constraints: &'static [&'static str],
}

#[derive(Clone, Copy)]
pub(crate) struct GeneratedConstructor {
    pub(crate) name: &'static str,
    pub(crate) result: &'static str,
    pub(crate) arguments: &'static [GeneratedConstructorArgument],
}

#[derive(Clone, Copy)]
struct GeneratedSelectorResult {
    name: &'static str,
    reference_type: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/inkscript_language_schema.rs"));

/// Exact-current procedure catalog version required by InkScript files and fragments.
pub const INKSCRIPT_PROCEDURE_CATALOG_VERSION: u32 = GENERATED_PROCEDURE_CATALOG_VERSION;

/// Exact-current canonical replay epoch required by InkScript files and fragments.
pub const INKSCRIPT_REQUIRED_REPLAY_EPOCH: u32 = GENERATED_REQUIRED_REPLAY_EPOCH;

fn generated_record(
    records: &'static [GeneratedRecord],
    name: &str,
) -> Option<&'static [InkScriptFieldSchema]> {
    records
        .iter()
        .find(|record| record.name == name)
        .map(|record| record.fields)
}

fn validate_fields(
    owner: &str,
    fields: &[InkScriptFieldSchema],
    additional_records: &[InkScriptRecordSchema],
) -> Result<(), InkScriptSemanticError> {
    if fields.len() > MAX_INKSCRIPT_CONTAINER_ELEMENTS {
        return Err(invalid_schema(owner));
    }
    let mut names = BTreeSet::new();
    let mut orders = BTreeSet::new();
    for field in fields {
        if !is_word(field.name)
            || !is_schema_name(field.type_name)
            || !type_is_known(field.type_name, additional_records)
            || !names.insert(field.name)
            || !orders.insert(field.canonical_order)
        {
            return Err(invalid_schema(owner));
        }
    }
    if orders.iter().copied().ne(0..fields.len() as u32) {
        return Err(invalid_schema(owner));
    }
    Ok(())
}

fn validate_results(
    owner: &str,
    results: &[InkScriptCommandResultSchema],
    additional_records: &[InkScriptRecordSchema],
) -> Result<(), InkScriptSemanticError> {
    if results.len() > MAX_INKSCRIPT_CONTAINER_ELEMENTS {
        return Err(invalid_schema(owner));
    }
    let mut names = BTreeSet::new();
    let mut orders = BTreeSet::new();
    for result in results {
        if !is_word(result.name)
            || !is_schema_name(result.element_type)
            || !type_is_known(result.element_type, additional_records)
            || !names.insert(result.name)
            || !orders.insert(result.canonical_order)
        {
            return Err(invalid_schema(owner));
        }
    }
    if orders.iter().copied().ne(0..results.len() as u32) {
        return Err(invalid_schema(owner));
    }
    Ok(())
}

fn type_is_known(type_name: &str, additional_records: &[InkScriptRecordSchema]) -> bool {
    let base = base_type(type_name);
    GENERATED_TYPE_NAMES.contains(&base)
        || additional_records.iter().any(|record| record.name == base)
}

fn invalid_schema(path: &str) -> InkScriptSemanticError {
    InkScriptSemanticError::new(InkScriptSemanticErrorCode::InvalidSchema, path)
}

pub(crate) fn is_identifier(value: &str) -> bool {
    is_word(value) && !RESERVED.contains(&value) && value.len() <= 128
}

fn is_word(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn is_schema_name(value: &str) -> bool {
    is_word(base_type(value))
}

fn base_type(value: &str) -> &str {
    value
        .strip_prefix("list<")
        .and_then(|value| value.strip_suffix('>'))
        .or_else(|| {
            value
                .strip_prefix("nullable<")
                .and_then(|value| value.strip_suffix('>'))
        })
        .map(base_type)
        .unwrap_or(value)
}

const RESERVED: &[&str] = &[
    "inkscript",
    "inkscript_fragment",
    "requires",
    "meta",
    "inputs",
    "parameters",
    "bindings",
    "program",
    "output",
    "execution",
    "assets",
    "file",
    "folder",
    "current_document",
    "current_sequence",
    "param",
    "let",
    "select",
    "assert",
    "step",
    "as",
    "enabled",
    "invoke",
    "editor_group",
    "asset",
    "true",
    "false",
    "none",
    "uuid",
    "blake3",
    "base64",
    "list",
    "nullable",
];
