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

/// A named closed enum contributed by a private command catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptEnumSchema {
    pub(crate) name: &'static str,
    pub(crate) members: &'static [&'static str],
}

impl InkScriptEnumSchema {
    /// Declares one catalog-owned enum and its complete member set.
    pub const fn new(name: &'static str, members: &'static [&'static str]) -> Self {
        Self { name, members }
    }
}

/// One ordered argument of a catalog-owned constructor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptConstructorArgumentSchema {
    pub(crate) name: &'static str,
    pub(crate) type_name: &'static str,
    pub(crate) constraints: &'static [&'static str],
}

impl InkScriptConstructorArgumentSchema {
    /// Declares one typed constructor argument.
    pub const fn new(
        name: &'static str,
        type_name: &'static str,
        constraints: &'static [&'static str],
    ) -> Self {
        Self {
            name,
            type_name,
            constraints,
        }
    }
}

/// A named constructor contributed by a private command catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptConstructorSchema {
    pub(crate) name: &'static str,
    pub(crate) result: &'static str,
    pub(crate) arguments: &'static [InkScriptConstructorArgumentSchema],
}

impl InkScriptConstructorSchema {
    /// Declares a constructor with an exact result type and ordered arguments.
    pub const fn new(
        name: &'static str,
        result: &'static str,
        arguments: &'static [InkScriptConstructorArgumentSchema],
    ) -> Self {
        Self {
            name,
            result,
            arguments,
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

    /// Returns the exact result field name.
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Returns the exact result availability contract.
    pub const fn availability(self) -> InkScriptResultAvailability {
        self.availability
    }

    /// Returns the result cardinality.
    pub const fn cardinality(self) -> InkScriptResultCardinality {
        self.cardinality
    }
}

/// The argument record for one exact command name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptCommandSchema {
    pub(crate) name: &'static str,
    pub(crate) fields: &'static [InkScriptFieldSchema],
    pub(crate) results: &'static [InkScriptCommandResultSchema],
}

/// Owner relation used to validate one initial-document selector entity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptSelectorOwner {
    /// The selected entity is owned directly by the document.
    Document,
    /// The selected entity is owned by a layer.
    Layer,
    /// The selected entity is owned by a plane.
    Plane,
    /// The selected entity is owned by a Light Table set.
    LightTableSet,
}

/// Stable initial ordering used by selector cardinality resolution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptSelectorOrder {
    /// Document tree order.
    DocumentTree,
    /// Guide declaration order.
    Guide,
    /// Vector object order.
    Vector,
    /// Annotation order.
    Annotation,
    /// A singleton entity.
    Singleton,
    /// Light Table set or item order.
    LightTable,
}

/// Closed comparison contract for an InkScript assertion kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptAssertComparison {
    /// Exact comparison against initial document fields.
    DocumentFields,
    /// Exact comparison against a bound object's initial properties.
    ObjectProperties,
    /// Exact comparison against the initial selection state.
    SelectionState,
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

    /// Returns the exact stable command name.
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Returns the exact ordered result schema.
    pub const fn results(self) -> &'static [InkScriptCommandResultSchema] {
        self.results
    }
}

/// Exact-current language-core schemas composed with a bounded private command schema set.
///
/// Before catalog ratification, callers must explicitly provide private command schemas; an empty
/// slice accepts no `invoke` command.
#[derive(Clone, Debug)]
pub struct InkScriptSchemaView<'schema> {
    enums: &'schema [InkScriptEnumSchema],
    constructors: &'schema [InkScriptConstructorSchema],
    records: &'schema [InkScriptRecordSchema],
    commands: &'schema [InkScriptCommandSchema],
}

impl<'schema> InkScriptSchemaView<'schema> {
    /// Composes the generated exact-current language v2 projection with caller-provided private
    /// schemas. The private catalog draft is never read by this API.
    pub fn exact_current(
        records: &'schema [InkScriptRecordSchema],
        commands: &'schema [InkScriptCommandSchema],
    ) -> Result<Self, InkScriptSemanticError> {
        Self::exact_current_with_catalog(&[], &[], records, commands)
    }

    /// Composes language v2 with the exact closed types and commands supplied by one private
    /// catalog owner. This does not read or expose the pre-ratification catalog draft.
    pub fn exact_current_with_catalog(
        enums: &'schema [InkScriptEnumSchema],
        constructors: &'schema [InkScriptConstructorSchema],
        records: &'schema [InkScriptRecordSchema],
        commands: &'schema [InkScriptCommandSchema],
    ) -> Result<Self, InkScriptSemanticError> {
        if enums.len() > MAX_INKSCRIPT_CONTAINER_ELEMENTS
            || constructors.len() > MAX_INKSCRIPT_CONTAINER_ELEMENTS
            || records.len() > MAX_INKSCRIPT_CONTAINER_ELEMENTS
            || commands.len() > MAX_INKSCRIPT_CONTAINER_ELEMENTS
        {
            return Err(invalid_schema("schema_view"));
        }
        let mut names = BTreeSet::new();
        for value_enum in enums {
            if !is_schema_name(value_enum.name)
                || GENERATED_TYPE_NAMES.contains(&value_enum.name)
                || !names.insert(value_enum.name)
                || value_enum.members.is_empty()
                || value_enum.members.len() > MAX_INKSCRIPT_CONTAINER_ELEMENTS
            {
                return Err(invalid_schema(value_enum.name));
            }
            let mut members = BTreeSet::new();
            if value_enum
                .members
                .iter()
                .any(|member| !is_word(member) || !members.insert(*member))
            {
                return Err(invalid_schema(value_enum.name));
            }
        }
        for record in records {
            if !is_schema_name(record.name)
                || GENERATED_TYPE_NAMES.contains(&record.name)
                || !names.insert(record.name)
            {
                return Err(invalid_schema(record.name));
            }
        }
        for record in records {
            validate_fields(record.name, record.fields, enums, records)?;
        }
        names.clear();
        for constructor in constructors {
            if !is_identifier(constructor.name)
                || GENERATED_CONSTRUCTORS
                    .iter()
                    .any(|generated| generated.name == constructor.name)
                || !names.insert(constructor.name)
                || constructor.arguments.len() > 16
                || !type_is_known(constructor.result, enums, records)
            {
                return Err(invalid_schema(constructor.name));
            }
            let mut arguments = BTreeSet::new();
            if constructor.arguments.iter().any(|argument| {
                !is_word(argument.name)
                    || !arguments.insert(argument.name)
                    || !type_is_known(argument.type_name, enums, records)
            }) {
                return Err(invalid_schema(constructor.name));
            }
        }
        names.clear();
        for command in commands {
            if !is_identifier(command.name) || !names.insert(command.name) {
                return Err(invalid_schema(command.name));
            }
            validate_fields(command.name, command.fields, enums, records)?;
            validate_results(command.name, command.results, enums, records)?;
        }
        Ok(Self {
            enums,
            constructors,
            records,
            commands,
        })
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
        self.selector_schema(name).map(|selector| selector.fields)
    }

    pub(crate) fn selector_result_type(&self, name: &str) -> Option<&'static str> {
        GENERATED_SELECTORS
            .iter()
            .find(|selector| selector.name == name)
            .map(|selector| selector.reference_type)
    }

    pub(crate) fn selector_schema(&self, name: &str) -> Option<&'static GeneratedSelector> {
        GENERATED_SELECTORS
            .iter()
            .find(|selector| selector.name == name)
    }

    /// Returns the owner relation for an exact language-v2 selector entity.
    pub fn selector_owner(&self, name: &str) -> Option<InkScriptSelectorOwner> {
        self.selector_schema(name).map(|selector| selector.owner)
    }

    pub(crate) fn type_kind(&self, name: &str) -> Option<&'static str> {
        GENERATED_TYPES
            .iter()
            .find(|value_type| value_type.name == name)
            .map(|value_type| value_type.kind)
            .or_else(|| {
                self.enums
                    .iter()
                    .any(|value_enum| value_enum.name == name)
                    .then_some("enum")
            })
            .or_else(|| {
                self.records
                    .iter()
                    .any(|record| record.name == name)
                    .then_some("record")
            })
    }

    pub(crate) fn enum_members(&self, name: &str) -> Option<&[&'static str]> {
        GENERATED_ENUMS
            .iter()
            .find(|value_enum| value_enum.name == name)
            .map(|value_enum| value_enum.members)
            .or_else(|| {
                self.enums
                    .iter()
                    .find(|value_enum| value_enum.name == name)
                    .map(|value_enum| value_enum.members)
            })
    }

    pub(crate) fn constructor(&self, name: &str) -> Option<&InkScriptConstructorSchema> {
        GENERATED_CONSTRUCTORS
            .iter()
            .find(|constructor| constructor.name == name)
            .or_else(|| {
                self.constructors
                    .iter()
                    .find(|constructor| constructor.name == name)
            })
    }

    pub(crate) fn assertion(&self, name: &str) -> Option<&'static [InkScriptFieldSchema]> {
        self.assertion_schema(name)
            .map(|assertion| assertion.fields)
    }

    pub(crate) fn assertion_schema(&self, name: &str) -> Option<&'static GeneratedAssertion> {
        GENERATED_ASSERTIONS
            .iter()
            .find(|assertion| assertion.name == name)
    }

    /// Returns persistent-ID namespaces in canonical schema order.
    pub fn id_namespaces(&self) -> &'static [GeneratedIdNamespace] {
        GENERATED_ID_NAMESPACES
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
pub(crate) struct GeneratedSelector {
    pub(crate) name: &'static str,
    pub(crate) reference_type: &'static str,
    pub(crate) owner: InkScriptSelectorOwner,
    pub(crate) initial_order: InkScriptSelectorOrder,
    pub(crate) fields: &'static [InkScriptFieldSchema],
}

#[derive(Clone, Copy)]
pub(crate) struct GeneratedAssertion {
    pub(crate) name: &'static str,
    pub(crate) comparison: InkScriptAssertComparison,
    pub(crate) fields: &'static [InkScriptFieldSchema],
}

/// One generated persistent-ID namespace used by strict allocation preconditions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedIdNamespace {
    tag: &'static str,
    order: u32,
}

impl GeneratedIdNamespace {
    /// Returns the stable ASCII namespace tag.
    pub const fn tag(self) -> &'static str {
        self.tag
    }

    /// Returns canonical schema order.
    pub const fn order(self) -> u32 {
        self.order
    }
}

include!(concat!(env!("OUT_DIR"), "/inkscript_language_schema.rs"));

/// Exact-current procedure catalog version required by InkScript files and fragments.
pub const INKSCRIPT_PROCEDURE_CATALOG_VERSION: u32 = GENERATED_PROCEDURE_CATALOG_VERSION;

/// Exact-current canonical replay epoch required by InkScript files and fragments.
pub const INKSCRIPT_REQUIRED_REPLAY_EPOCH: u32 = GENERATED_REQUIRED_REPLAY_EPOCH;

/// Number of commands in the immutable exact-current production catalog.
pub const INKSCRIPT_PRODUCTION_CATALOG_COMMAND_COUNT: usize =
    GENERATED_PRODUCTION_CATALOG_COMMAND_COUNT;

/// FNV-1a drift fingerprint of the immutable `catalog-v2.json` bytes after CRLF normalization.
///
/// This is a build/review sentinel rather than a security digest. A catalog change requires a new
/// exact-current catalog version and a new versioned resource instead of editing the frozen v2
/// bytes in place.
pub const INKSCRIPT_PRODUCTION_CATALOG_FINGERPRINT: u64 = GENERATED_PRODUCTION_CATALOG_FINGERPRINT;

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
    additional_enums: &[InkScriptEnumSchema],
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
            || !type_is_known(field.type_name, additional_enums, additional_records)
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
    additional_enums: &[InkScriptEnumSchema],
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
            || !type_is_known(result.element_type, additional_enums, additional_records)
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

fn type_is_known(
    type_name: &str,
    additional_enums: &[InkScriptEnumSchema],
    additional_records: &[InkScriptRecordSchema],
) -> bool {
    let base = base_type(type_name);
    GENERATED_TYPE_NAMES.contains(&base)
        || additional_enums
            .iter()
            .any(|value_enum| value_enum.name == base)
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

#[cfg(test)]
mod tests {
    use super::*;

    const ENUMS: &[InkScriptEnumSchema] = &[InkScriptEnumSchema::new(
        "catalog_mode",
        &["first", "second"],
    )];
    const RECORD_FIELDS: &[InkScriptFieldSchema] = &[
        InkScriptFieldSchema::required("mode", "catalog_mode", 0),
        InkScriptFieldSchema::required("count", "u32", 1),
    ];
    const RECORDS: &[InkScriptRecordSchema] =
        &[InkScriptRecordSchema::new("catalog_record", RECORD_FIELDS)];
    const CONSTRUCTOR_ARGUMENTS: &[InkScriptConstructorArgumentSchema] = &[
        InkScriptConstructorArgumentSchema::new("mode", "catalog_mode", &[]),
        InkScriptConstructorArgumentSchema::new("count", "u32", &["nonzero"]),
    ];
    const CONSTRUCTORS: &[InkScriptConstructorSchema] = &[InkScriptConstructorSchema::new(
        "catalog_value",
        "catalog_record",
        CONSTRUCTOR_ARGUMENTS,
    )];
    const COMMAND_FIELDS: &[InkScriptFieldSchema] =
        &[InkScriptFieldSchema::required("value", "catalog_record", 0)];
    const COMMANDS: &[InkScriptCommandSchema] =
        &[InkScriptCommandSchema::new("catalog_test", COMMAND_FIELDS)];

    #[test]
    fn exact_current_catalog_types_are_closed_and_resolvable() {
        let schema =
            InkScriptSchemaView::exact_current_with_catalog(ENUMS, CONSTRUCTORS, RECORDS, COMMANDS)
                .unwrap();
        assert_eq!(schema.type_kind("catalog_mode"), Some("enum"));
        assert_eq!(
            schema.enum_members("catalog_mode"),
            Some(&["first", "second"][..])
        );
        assert_eq!(schema.type_kind("catalog_record"), Some("record"));
        assert_eq!(schema.record("catalog_record"), Some(RECORD_FIELDS));
        assert_eq!(schema.constructor("catalog_value"), Some(&CONSTRUCTORS[0]));
        assert_eq!(schema.command("catalog_test"), Some(COMMAND_FIELDS));
    }

    #[test]
    fn catalog_types_reject_duplicates_unknowns_and_open_enums_atomically() {
        const EMPTY_ENUM: &[InkScriptEnumSchema] = &[InkScriptEnumSchema::new("empty", &[])];
        assert!(
            InkScriptSchemaView::exact_current_with_catalog(EMPTY_ENUM, &[], &[], &[]).is_err()
        );
        const DUPLICATE_ENUM: &[InkScriptEnumSchema] = &[
            InkScriptEnumSchema::new("duplicate", &["value"]),
            InkScriptEnumSchema::new("duplicate", &["other"]),
        ];
        assert!(
            InkScriptSchemaView::exact_current_with_catalog(DUPLICATE_ENUM, &[], &[], &[]).is_err()
        );
        const UNKNOWN_ARGUMENT: &[InkScriptConstructorArgumentSchema] =
            &[InkScriptConstructorArgumentSchema::new(
                "value",
                "unknown",
                &[],
            )];
        const UNKNOWN_CONSTRUCTOR: &[InkScriptConstructorSchema] =
            &[InkScriptConstructorSchema::new(
                "unknown_value",
                "u32",
                UNKNOWN_ARGUMENT,
            )];
        assert!(
            InkScriptSchemaView::exact_current_with_catalog(&[], UNKNOWN_CONSTRUCTOR, &[], &[],)
                .is_err()
        );
    }
}
