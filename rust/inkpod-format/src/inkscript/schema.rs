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

/// The argument record for one exact command name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptCommandSchema {
    pub(crate) name: &'static str,
    pub(crate) fields: &'static [InkScriptFieldSchema],
}

impl InkScriptCommandSchema {
    pub const fn new(name: &'static str, fields: &'static [InkScriptFieldSchema]) -> Self {
        Self { name, fields }
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

    pub(crate) fn selector(&self, name: &str) -> Option<&'static [InkScriptFieldSchema]> {
        generated_record(GENERATED_SELECTORS, name)
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

include!(concat!(env!("OUT_DIR"), "/inkscript_language_schema.rs"));

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
