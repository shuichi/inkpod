use std::collections::BTreeMap;

use super::lexer::{InkScriptKeyword, InkScriptPunctuation, InkScriptToken, InkScriptTokenKind};
use super::parser::{InkScriptDocumentKind, InkScriptParsed};
use super::schema::{
    InkScriptFieldSchema, InkScriptSchemaDefault, InkScriptSchemaView, InkScriptSemanticError,
    InkScriptSemanticErrorCode,
};
use super::source::INKSCRIPT_FILE_VERSION;

/// A trivia-free, Core-independent InkScript syntax document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptSemanticDocument {
    pub(crate) kind: InkScriptDocumentKind,
    pub(crate) sections: Vec<InkScriptSemanticSection>,
}

impl InkScriptSemanticDocument {
    /// Returns whether this syntax tree represents a complete file or a fragment.
    pub const fn kind(&self) -> InkScriptDocumentKind {
        self.kind
    }

    /// Returns sections in canonical schema order. Declaration order inside each section is
    /// retained.
    pub fn sections(&self) -> &[InkScriptSemanticSection] {
        &self.sections
    }
}

/// A command-independent semantic section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InkScriptSemanticSection {
    Requires(InkScriptRecord),
    Meta(InkScriptRecord),
    Inputs(Vec<InkScriptInput>),
    Parameters(Vec<InkScriptParameter>),
    Bindings(Vec<InkScriptBinding>),
    Program(Vec<InkScriptProgramStatement>),
    Output(InkScriptRecord),
    Execution(InkScriptRecord),
    Assets(Vec<InkScriptAsset>),
}

impl InkScriptSemanticSection {
    pub(crate) const fn schema_name(&self) -> &'static str {
        match self {
            Self::Requires(_) => "requires",
            Self::Meta(_) => "meta",
            Self::Inputs(_) => "inputs",
            Self::Parameters(_) => "parameters",
            Self::Bindings(_) => "bindings",
            Self::Program(_) => "program",
            Self::Output(_) => "output",
            Self::Execution(_) => "execution",
            Self::Assets(_) => "assets",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptInputKind {
    File,
    Folder,
    CurrentDocument,
    CurrentSequence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptInput {
    pub(crate) kind: InkScriptInputKind,
    pub(crate) path: Option<String>,
    pub(crate) options: InkScriptRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptParameter {
    pub(crate) name: String,
    pub(crate) declared_type: InkScriptTypeReference,
    pub(crate) default_value: InkScriptValue,
    pub(crate) metadata: InkScriptRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptBinding {
    pub(crate) name: String,
    pub(crate) entity: String,
    pub(crate) selector: InkScriptRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InkScriptProgramStatement {
    Assert {
        kind: String,
        arguments: InkScriptRecord,
    },
    Step {
        label: String,
        result_alias: Option<String>,
        enabled: bool,
        editor_group: Option<String>,
        command: String,
        arguments: InkScriptRecord,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptAsset {
    pub(crate) name: String,
    pub(crate) body: InkScriptRecord,
}

/// A closed record. Field source order is deliberately absent from the semantic representation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InkScriptRecord(pub(crate) BTreeMap<String, InkScriptValue>);

/// A normalized syntax value. Fixed-width conversion and command typing belong to later owner
/// milestones; integer and decimal values therefore remain exact mathematical spellings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InkScriptValue {
    Boolean(bool),
    Integer(String),
    Decimal(String),
    String(String),
    Uuid(String),
    Digest(String),
    Base64(Vec<u8>),
    None,
    Enum(String),
    Constructor {
        name: String,
        arguments: Vec<Self>,
    },
    AssetReference(String),
    Reference {
        root: String,
        segments: Vec<InkScriptReferenceSegment>,
    },
    List(Vec<Self>),
    Record(InkScriptRecord),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InkScriptReferenceSegment {
    Field(String),
    Index(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InkScriptTypeReference {
    Named(String),
    List(Box<Self>),
    Nullable(Box<Self>),
}

impl InkScriptTypeReference {
    pub(crate) fn schema_name(&self) -> String {
        match self {
            Self::Named(name) => name.clone(),
            Self::List(child) => format!("list<{}>", child.schema_name()),
            Self::Nullable(child) => format!("nullable<{}>", child.schema_name()),
        }
    }
}

/// Converts an exact-current valid CST into a normalized semantic syntax tree.
///
/// This performs no filesystem or Core operation. Invalid/recovered CSTs never produce an AST.
pub fn build_inkscript_semantic(
    parsed: &InkScriptParsed<'_>,
    schema: &InkScriptSchemaView<'_>,
) -> Result<InkScriptSemanticDocument, InkScriptSemanticError> {
    if !parsed.is_valid() {
        return Err(error(InkScriptSemanticErrorCode::InvalidSyntax, "document"));
    }
    SemanticParser::new(parsed, schema).parse()
}

struct SemanticParser<'a, 'schema> {
    kind: InkScriptDocumentKind,
    source: &'a super::source::InkScriptSource,
    tokens: &'a [InkScriptToken],
    cursor: usize,
    schema: &'a InkScriptSchemaView<'schema>,
}

impl<'a, 'schema> SemanticParser<'a, 'schema> {
    fn new(parsed: &'a InkScriptParsed<'_>, schema: &'a InkScriptSchemaView<'schema>) -> Self {
        Self {
            kind: parsed.cst().document_kind(),
            source: parsed.cst().source(),
            tokens: parsed.cst().tokens(),
            cursor: 0,
            schema,
        }
    }

    fn parse(mut self) -> Result<InkScriptSemanticDocument, InkScriptSemanticError> {
        match self.kind {
            InkScriptDocumentKind::File => self.expect_keyword(InkScriptKeyword::InkScript)?,
            InkScriptDocumentKind::Fragment => {
                self.expect_keyword(InkScriptKeyword::InkScriptFragment)?
            }
            InkScriptDocumentKind::Unknown => return Err(self.syntax("header")),
        }
        let version = self.take_spelling(InkScriptTokenKind::IntegerLiteral)?;
        if version != INKSCRIPT_FILE_VERSION.to_string() {
            return Err(self.syntax("header.version"));
        }
        self.expect_punctuation(InkScriptPunctuation::Semicolon)?;

        let mut sections = Vec::new();
        while self.peek_kind() != InkScriptTokenKind::EndOfSource {
            let section = match self.peek_kind() {
                InkScriptTokenKind::Keyword(InkScriptKeyword::Requires) => {
                    self.take();
                    InkScriptSemanticSection::Requires(self.record_named("requires_record")?)
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::Meta) => {
                    self.take();
                    InkScriptSemanticSection::Meta(self.record_named("meta_record")?)
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::Inputs) => {
                    self.take();
                    InkScriptSemanticSection::Inputs(self.inputs()?)
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::Parameters) => {
                    self.take();
                    InkScriptSemanticSection::Parameters(self.parameters()?)
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::Bindings) => {
                    self.take();
                    InkScriptSemanticSection::Bindings(self.bindings()?)
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::Program) => {
                    self.take();
                    InkScriptSemanticSection::Program(self.program()?)
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::Output) => {
                    self.take();
                    let record = self.record_raw()?;
                    InkScriptSemanticSection::Output(normalize_output(record, self.schema)?)
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::Execution) => {
                    self.take();
                    InkScriptSemanticSection::Execution(self.record_named("execution_record")?)
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::Assets) => {
                    self.take();
                    InkScriptSemanticSection::Assets(self.assets()?)
                }
                _ => return Err(self.syntax("section")),
            };
            sections.push(section);
        }
        if sections
            .iter()
            .any(|section| self.schema.section_order(section.schema_name()).is_none())
        {
            return Err(error(
                InkScriptSemanticErrorCode::InvalidSchema,
                "canonicalization.section_order",
            ));
        }
        sections.sort_by_key(|section| {
            self.schema
                .section_order(section.schema_name())
                .expect("section order was validated")
        });
        Ok(InkScriptSemanticDocument {
            kind: self.kind,
            sections,
        })
    }

    fn inputs(&mut self) -> Result<Vec<InkScriptInput>, InkScriptSemanticError> {
        self.expect_punctuation(InkScriptPunctuation::LeftBrace)?;
        let mut result = Vec::new();
        while !self.take_punctuation(InkScriptPunctuation::RightBrace) {
            let (kind, path_required, schema_name) = match self.peek_kind() {
                InkScriptTokenKind::Keyword(InkScriptKeyword::File) => {
                    (InkScriptInputKind::File, true, "file_input_options")
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::Folder) => {
                    (InkScriptInputKind::Folder, true, "folder_input_options")
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::CurrentDocument) => (
                    InkScriptInputKind::CurrentDocument,
                    false,
                    "current_document_input_options",
                ),
                InkScriptTokenKind::Keyword(InkScriptKeyword::CurrentSequence) => (
                    InkScriptInputKind::CurrentSequence,
                    false,
                    "current_sequence_input_options",
                ),
                _ => return Err(self.syntax("inputs")),
            };
            self.take();
            let path = if path_required {
                Some(self.string()?)
            } else {
                None
            };
            let options = if self.peek_kind()
                == InkScriptTokenKind::Punctuation(InkScriptPunctuation::LeftBrace)
            {
                self.record_named(schema_name)?
            } else {
                InkScriptRecord::default()
            };
            self.expect_punctuation(InkScriptPunctuation::Semicolon)?;
            result.push(InkScriptInput {
                kind,
                path,
                options,
            });
        }
        Ok(result)
    }

    fn parameters(&mut self) -> Result<Vec<InkScriptParameter>, InkScriptSemanticError> {
        self.expect_punctuation(InkScriptPunctuation::LeftBrace)?;
        let mut result = Vec::new();
        while !self.take_punctuation(InkScriptPunctuation::RightBrace) {
            self.expect_keyword(InkScriptKeyword::Param)?;
            let name = self.identifier()?;
            self.expect_punctuation(InkScriptPunctuation::Colon)?;
            let declared_type = self.type_reference()?;
            self.expect_punctuation(InkScriptPunctuation::Equals)?;
            let value = self.value()?;
            let default_value = normalize_value(value, &declared_type.schema_name(), self.schema)?;
            let metadata = if self.peek_kind()
                == InkScriptTokenKind::Punctuation(InkScriptPunctuation::LeftBrace)
            {
                self.record_named("parameter_metadata")?
            } else {
                InkScriptRecord::default()
            };
            self.expect_punctuation(InkScriptPunctuation::Semicolon)?;
            result.push(InkScriptParameter {
                name,
                declared_type,
                default_value,
                metadata,
            });
        }
        Ok(result)
    }

    fn bindings(&mut self) -> Result<Vec<InkScriptBinding>, InkScriptSemanticError> {
        self.expect_punctuation(InkScriptPunctuation::LeftBrace)?;
        let mut result = Vec::new();
        while !self.take_punctuation(InkScriptPunctuation::RightBrace) {
            self.expect_keyword(InkScriptKeyword::Let)?;
            let name = self.identifier()?;
            self.expect_punctuation(InkScriptPunctuation::Equals)?;
            self.expect_keyword(InkScriptKeyword::Select)?;
            let entity = self.identifier()?;
            let raw = self.record_raw()?;
            let fields = self
                .schema
                .selector(&entity)
                .ok_or_else(|| error(InkScriptSemanticErrorCode::UnknownRecordSchema, &entity))?;
            let selector = normalize_record(raw, fields, self.schema, &entity)?;
            self.expect_punctuation(InkScriptPunctuation::Semicolon)?;
            result.push(InkScriptBinding {
                name,
                entity,
                selector,
            });
        }
        Ok(result)
    }

    fn program(&mut self) -> Result<Vec<InkScriptProgramStatement>, InkScriptSemanticError> {
        self.expect_punctuation(InkScriptPunctuation::LeftBrace)?;
        let mut result = Vec::new();
        while !self.take_punctuation(InkScriptPunctuation::RightBrace) {
            match self.peek_kind() {
                InkScriptTokenKind::Keyword(InkScriptKeyword::Assert) => {
                    self.take();
                    let kind = self.identifier()?;
                    let raw = self.record_raw()?;
                    let fields = self.schema.assertion(&kind).ok_or_else(|| {
                        error(InkScriptSemanticErrorCode::UnknownRecordSchema, &kind)
                    })?;
                    let arguments = normalize_record(raw, fields, self.schema, &kind)?;
                    self.expect_punctuation(InkScriptPunctuation::Semicolon)?;
                    result.push(InkScriptProgramStatement::Assert { kind, arguments });
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::Step) => {
                    result.push(self.step()?);
                }
                _ => return Err(self.syntax("program")),
            }
        }
        Ok(result)
    }

    fn step(&mut self) -> Result<InkScriptProgramStatement, InkScriptSemanticError> {
        self.expect_keyword(InkScriptKeyword::Step)?;
        let label = self.string()?;
        let result_alias = if self.take_keyword(InkScriptKeyword::As) {
            Some(self.identifier()?)
        } else {
            None
        };
        self.expect_punctuation(InkScriptPunctuation::LeftBrace)?;
        let mut enabled = None;
        let mut editor_group = None;
        let mut invocation = None;
        while !self.take_punctuation(InkScriptPunctuation::RightBrace) {
            match self.peek_kind() {
                InkScriptTokenKind::Keyword(InkScriptKeyword::Enabled) => {
                    self.take();
                    self.expect_punctuation(InkScriptPunctuation::Equals)?;
                    enabled = Some(self.boolean()?);
                    self.expect_punctuation(InkScriptPunctuation::Semicolon)?;
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::EditorGroup) => {
                    self.take();
                    self.expect_punctuation(InkScriptPunctuation::Equals)?;
                    editor_group = Some(self.string()?);
                    self.expect_punctuation(InkScriptPunctuation::Semicolon)?;
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::Invoke) => {
                    self.take();
                    let command = self.identifier()?;
                    let raw = self.record_raw()?;
                    let fields = self.schema.command(&command).ok_or_else(|| {
                        error(InkScriptSemanticErrorCode::UnknownCommandSchema, &command)
                    })?;
                    let arguments = normalize_record(raw, fields, self.schema, &command)?;
                    self.expect_punctuation(InkScriptPunctuation::Semicolon)?;
                    invocation = Some((command, arguments));
                }
                _ => return Err(self.syntax("step")),
            }
        }
        let (command, arguments) = invocation.ok_or_else(|| self.syntax("step.invoke"))?;
        Ok(InkScriptProgramStatement::Step {
            label,
            result_alias,
            enabled: enabled.ok_or_else(|| self.syntax("step.enabled"))?,
            editor_group,
            command,
            arguments,
        })
    }

    fn assets(&mut self) -> Result<Vec<InkScriptAsset>, InkScriptSemanticError> {
        self.expect_punctuation(InkScriptPunctuation::LeftBrace)?;
        let mut result = Vec::new();
        while !self.take_punctuation(InkScriptPunctuation::RightBrace) {
            self.expect_keyword(InkScriptKeyword::Asset)?;
            let name = self.identifier()?;
            let raw = self.record_raw()?;
            let kind = enum_field(&raw, "kind").unwrap_or("canonical_raster");
            if kind != "canonical_raster" {
                return Err(error(InkScriptSemanticErrorCode::UnknownRecordSchema, kind));
            }
            let fields = self
                .schema
                .record("canonical_raster_asset")
                .expect("language schema exists");
            let body = normalize_record(raw, fields, self.schema, "canonical_raster_asset")?;
            self.expect_punctuation(InkScriptPunctuation::Semicolon)?;
            result.push(InkScriptAsset { name, body });
        }
        Ok(result)
    }

    fn record_named(&mut self, name: &str) -> Result<InkScriptRecord, InkScriptSemanticError> {
        let raw = self.record_raw()?;
        let fields = self
            .schema
            .record(name)
            .ok_or_else(|| error(InkScriptSemanticErrorCode::UnknownRecordSchema, name))?;
        normalize_record(raw, fields, self.schema, name)
    }

    fn record_raw(&mut self) -> Result<InkScriptRecord, InkScriptSemanticError> {
        self.expect_punctuation(InkScriptPunctuation::LeftBrace)?;
        let mut fields = BTreeMap::new();
        while !self.take_punctuation(InkScriptPunctuation::RightBrace) {
            let name = self.field_name()?;
            self.expect_punctuation(InkScriptPunctuation::Equals)?;
            let value = self.value()?;
            self.expect_punctuation(InkScriptPunctuation::Semicolon)?;
            if fields.insert(name, value).is_some() {
                return Err(self.syntax("record.duplicate"));
            }
        }
        Ok(InkScriptRecord(fields))
    }

    fn value(&mut self) -> Result<InkScriptValue, InkScriptSemanticError> {
        match self.peek_kind() {
            InkScriptTokenKind::Keyword(InkScriptKeyword::True) => {
                self.take();
                Ok(InkScriptValue::Boolean(true))
            }
            InkScriptTokenKind::Keyword(InkScriptKeyword::False) => {
                self.take();
                Ok(InkScriptValue::Boolean(false))
            }
            InkScriptTokenKind::Keyword(InkScriptKeyword::None) => {
                self.take();
                Ok(InkScriptValue::None)
            }
            InkScriptTokenKind::IntegerLiteral => Ok(InkScriptValue::Integer(normalize_integer(
                &self.take_spelling(InkScriptTokenKind::IntegerLiteral)?,
            ))),
            InkScriptTokenKind::DecimalLiteral => Ok(InkScriptValue::Decimal(normalize_decimal(
                &self.take_spelling(InkScriptTokenKind::DecimalLiteral)?,
            ))),
            InkScriptTokenKind::StringLiteral => Ok(InkScriptValue::String(self.string()?)),
            InkScriptTokenKind::UuidLiteral => {
                let spelling = self.take_spelling(InkScriptTokenKind::UuidLiteral)?;
                Ok(InkScriptValue::Uuid(decode_prefixed_string(
                    &spelling, "uuid",
                )?))
            }
            InkScriptTokenKind::DigestLiteral => {
                let spelling = self.take_spelling(InkScriptTokenKind::DigestLiteral)?;
                Ok(InkScriptValue::Digest(decode_prefixed_string(
                    &spelling, "blake3",
                )?))
            }
            InkScriptTokenKind::Base64Literal => {
                let spelling = self.take_spelling(InkScriptTokenKind::Base64Literal)?;
                Ok(InkScriptValue::Base64(decode_base64_literal(&spelling)?))
            }
            InkScriptTokenKind::Punctuation(InkScriptPunctuation::Dollar) => self.reference(),
            InkScriptTokenKind::Punctuation(InkScriptPunctuation::LeftBracket) => self.list(),
            InkScriptTokenKind::Punctuation(InkScriptPunctuation::LeftBrace) => {
                self.record_raw().map(InkScriptValue::Record)
            }
            InkScriptTokenKind::Keyword(InkScriptKeyword::Asset) => {
                self.take();
                self.expect_punctuation(InkScriptPunctuation::LeftParenthesis)?;
                let name = self.identifier()?;
                self.expect_punctuation(InkScriptPunctuation::RightParenthesis)?;
                Ok(InkScriptValue::AssetReference(name))
            }
            InkScriptTokenKind::Word => {
                let name = self.identifier()?;
                if self.take_punctuation(InkScriptPunctuation::LeftParenthesis) {
                    let mut arguments = Vec::new();
                    if !self.take_punctuation(InkScriptPunctuation::RightParenthesis) {
                        loop {
                            arguments.push(self.value()?);
                            if self.take_punctuation(InkScriptPunctuation::RightParenthesis) {
                                break;
                            }
                            self.expect_punctuation(InkScriptPunctuation::Comma)?;
                            if self.take_punctuation(InkScriptPunctuation::RightParenthesis) {
                                break;
                            }
                        }
                    }
                    Ok(InkScriptValue::Constructor { name, arguments })
                } else {
                    Ok(InkScriptValue::Enum(name))
                }
            }
            _ => Err(self.syntax("value")),
        }
    }

    fn list(&mut self) -> Result<InkScriptValue, InkScriptSemanticError> {
        self.expect_punctuation(InkScriptPunctuation::LeftBracket)?;
        let mut values = Vec::new();
        if !self.take_punctuation(InkScriptPunctuation::RightBracket) {
            loop {
                values.push(self.value()?);
                if self.take_punctuation(InkScriptPunctuation::RightBracket) {
                    break;
                }
                self.expect_punctuation(InkScriptPunctuation::Comma)?;
                if self.take_punctuation(InkScriptPunctuation::RightBracket) {
                    break;
                }
            }
        }
        Ok(InkScriptValue::List(values))
    }

    fn reference(&mut self) -> Result<InkScriptValue, InkScriptSemanticError> {
        self.expect_punctuation(InkScriptPunctuation::Dollar)?;
        let root = self.identifier()?;
        let mut segments = Vec::new();
        loop {
            if self.take_punctuation(InkScriptPunctuation::Dot) {
                segments.push(InkScriptReferenceSegment::Field(self.field_name()?));
            } else if self.take_punctuation(InkScriptPunctuation::LeftBracket) {
                let index =
                    normalize_integer(&self.take_spelling(InkScriptTokenKind::IntegerLiteral)?);
                self.expect_punctuation(InkScriptPunctuation::RightBracket)?;
                segments.push(InkScriptReferenceSegment::Index(index));
            } else {
                break;
            }
        }
        Ok(InkScriptValue::Reference { root, segments })
    }

    fn type_reference(&mut self) -> Result<InkScriptTypeReference, InkScriptSemanticError> {
        if self.take_keyword(InkScriptKeyword::List) {
            self.expect_punctuation(InkScriptPunctuation::LessThan)?;
            let child = self.type_reference()?;
            self.expect_punctuation(InkScriptPunctuation::GreaterThan)?;
            Ok(InkScriptTypeReference::List(Box::new(child)))
        } else if self.take_keyword(InkScriptKeyword::Nullable) {
            self.expect_punctuation(InkScriptPunctuation::LessThan)?;
            let child = self.type_reference()?;
            self.expect_punctuation(InkScriptPunctuation::GreaterThan)?;
            Ok(InkScriptTypeReference::Nullable(Box::new(child)))
        } else {
            self.identifier().map(InkScriptTypeReference::Named)
        }
    }

    fn string(&mut self) -> Result<String, InkScriptSemanticError> {
        decode_string(&self.take_spelling(InkScriptTokenKind::StringLiteral)?)
    }

    fn boolean(&mut self) -> Result<bool, InkScriptSemanticError> {
        if self.take_keyword(InkScriptKeyword::True) {
            Ok(true)
        } else if self.take_keyword(InkScriptKeyword::False) {
            Ok(false)
        } else {
            Err(self.syntax("boolean"))
        }
    }

    fn identifier(&mut self) -> Result<String, InkScriptSemanticError> {
        self.take_spelling(InkScriptTokenKind::Word)
    }

    fn field_name(&mut self) -> Result<String, InkScriptSemanticError> {
        match self.peek_kind() {
            InkScriptTokenKind::Word | InkScriptTokenKind::Keyword(_) => {
                let token = self.take().ok_or_else(|| self.syntax("field"))?;
                Ok(self
                    .source
                    .slice(token.span())
                    .ok_or_else(|| self.syntax("field"))?
                    .to_owned())
            }
            _ => Err(self.syntax("field")),
        }
    }

    fn expect_keyword(&mut self, keyword: InkScriptKeyword) -> Result<(), InkScriptSemanticError> {
        if self.take_keyword(keyword) {
            Ok(())
        } else {
            Err(self.syntax(keyword.as_str()))
        }
    }

    fn take_keyword(&mut self, keyword: InkScriptKeyword) -> bool {
        if self.peek_kind() == InkScriptTokenKind::Keyword(keyword) {
            self.take();
            true
        } else {
            false
        }
    }

    fn expect_punctuation(
        &mut self,
        punctuation: InkScriptPunctuation,
    ) -> Result<(), InkScriptSemanticError> {
        if self.take_punctuation(punctuation) {
            Ok(())
        } else {
            Err(self.syntax("punctuation"))
        }
    }

    fn take_punctuation(&mut self, punctuation: InkScriptPunctuation) -> bool {
        if self.peek_kind() == InkScriptTokenKind::Punctuation(punctuation) {
            self.take();
            true
        } else {
            false
        }
    }

    fn take_spelling(
        &mut self,
        kind: InkScriptTokenKind,
    ) -> Result<String, InkScriptSemanticError> {
        if self.peek_kind() != kind {
            return Err(self.syntax("token"));
        }
        let token = self.take().ok_or_else(|| self.syntax("token"))?;
        self.source
            .slice(token.span())
            .map(str::to_owned)
            .ok_or_else(|| self.syntax("token"))
    }

    fn peek_kind(&mut self) -> InkScriptTokenKind {
        self.skip_trivia();
        self.tokens
            .get(self.cursor)
            .map(InkScriptToken::kind)
            .unwrap_or(InkScriptTokenKind::EndOfSource)
    }

    fn take(&mut self) -> Option<InkScriptToken> {
        self.skip_trivia();
        let token = self.tokens.get(self.cursor).copied()?;
        if token.kind() != InkScriptTokenKind::EndOfSource {
            self.cursor += 1;
        }
        Some(token)
    }

    fn skip_trivia(&mut self) {
        while self
            .tokens
            .get(self.cursor)
            .is_some_and(|token| token.kind().is_trivia())
        {
            self.cursor += 1;
        }
    }

    fn syntax(&self, path: &str) -> InkScriptSemanticError {
        error(InkScriptSemanticErrorCode::InvalidSyntax, path)
    }
}

pub(crate) fn normalize_record(
    mut record: InkScriptRecord,
    fields: &[InkScriptFieldSchema],
    schema: &InkScriptSchemaView<'_>,
    path: &str,
) -> Result<InkScriptRecord, InkScriptSemanticError> {
    for name in record.0.keys() {
        if !fields.iter().any(|field| field.name == name) {
            return Err(error(
                InkScriptSemanticErrorCode::UnknownFieldSchema,
                format!("{path}.{name}"),
            ));
        }
    }
    for field in fields {
        match record.0.remove(field.name) {
            Some(value) => {
                let value = normalize_value(value, field.type_name, schema)?;
                if !field.required
                    && field
                        .default
                        .is_some_and(|default| value_matches_default(&value, default))
                {
                    continue;
                }
                record.0.insert(field.name.to_owned(), value);
            }
            None if field.required => {
                return Err(error(
                    InkScriptSemanticErrorCode::MissingRequiredField,
                    format!("{path}.{}", field.name),
                ));
            }
            None => {}
        }
    }
    Ok(record)
}

pub(crate) fn normalize_value(
    value: InkScriptValue,
    type_name: &str,
    schema: &InkScriptSchemaView<'_>,
) -> Result<InkScriptValue, InkScriptSemanticError> {
    if value == InkScriptValue::None && type_name.starts_with("nullable<") {
        return Ok(value);
    }
    let type_name = unwrap_type(type_name, "nullable<").unwrap_or(type_name);
    if let Some(element_type) = unwrap_type(type_name, "list<") {
        return match value {
            InkScriptValue::List(values) => values
                .into_iter()
                .map(|value| normalize_value(value, element_type, schema))
                .collect::<Result<Vec<_>, _>>()
                .map(InkScriptValue::List),
            other => Ok(other),
        };
    }
    if let InkScriptValue::Record(record) = value {
        let fields = schema
            .record(type_name)
            .ok_or_else(|| error(InkScriptSemanticErrorCode::UnknownRecordSchema, type_name))?;
        return normalize_record(record, fields, schema, type_name).map(InkScriptValue::Record);
    }
    Ok(value)
}

fn normalize_output(
    record: InkScriptRecord,
    schema: &InkScriptSchemaView<'_>,
) -> Result<InkScriptRecord, InkScriptSemanticError> {
    let policy = enum_field(&record, "policy").ok_or_else(|| {
        error(
            InkScriptSemanticErrorCode::MissingRequiredField,
            "output.policy",
        )
    })?;
    let schema_name = match policy {
        "duplicate" => "output_duplicate",
        "new_save" => "output_new_save",
        "explicit_overwrite" => "output_explicit_overwrite",
        _ => {
            return Err(error(
                InkScriptSemanticErrorCode::UnknownRecordSchema,
                "output.policy",
            ));
        }
    };
    normalize_record(
        record,
        schema.record(schema_name).expect("language schema exists"),
        schema,
        schema_name,
    )
}

pub(crate) fn enum_field<'a>(record: &'a InkScriptRecord, name: &str) -> Option<&'a str> {
    match record.0.get(name) {
        Some(InkScriptValue::Enum(value)) => Some(value),
        _ => None,
    }
}

pub(crate) fn unwrap_type<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value.strip_prefix(prefix)?.strip_suffix('>')
}

fn value_matches_default(value: &InkScriptValue, default: InkScriptSchemaDefault) -> bool {
    match default {
        InkScriptSchemaDefault::None => value == &InkScriptValue::None,
        InkScriptSchemaDefault::Boolean(expected) => value == &InkScriptValue::Boolean(expected),
        InkScriptSchemaDefault::Enum(expected) => {
            value == &InkScriptValue::Enum(expected.to_owned())
        }
        InkScriptSchemaDefault::EmptyList => {
            matches!(value, InkScriptValue::List(values) if values.is_empty())
        }
        InkScriptSchemaDefault::EmptyRecord => {
            matches!(value, InkScriptValue::Record(record) if record.0.is_empty())
        }
    }
}

fn normalize_integer(spelling: &str) -> String {
    let (negative, digits) = spelling
        .strip_prefix('-')
        .map_or((false, spelling), |digits| (true, digits));
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        "0".to_owned()
    } else if negative {
        format!("-{digits}")
    } else {
        digits.to_owned()
    }
}

fn normalize_decimal(spelling: &str) -> String {
    let (negative, unsigned) = spelling
        .strip_prefix('-')
        .map_or((false, spelling), |value| (true, value));
    let (integer, fraction) = unsigned.split_once('.').unwrap_or((unsigned, "0"));
    let integer = integer.trim_start_matches('0');
    let integer = if integer.is_empty() { "0" } else { integer };
    let fraction = fraction.trim_end_matches('0');
    let fraction = if fraction.is_empty() { "0" } else { fraction };
    let zero = integer == "0" && fraction == "0";
    format!(
        "{}{integer}.{fraction}",
        if negative && !zero { "-" } else { "" }
    )
}

fn decode_prefixed_string(spelling: &str, prefix: &str) -> Result<String, InkScriptSemanticError> {
    decode_string(
        spelling
            .strip_prefix(prefix)
            .ok_or_else(|| error(InkScriptSemanticErrorCode::InvalidSyntax, "literal"))?,
    )
}

fn decode_string(spelling: &str) -> Result<String, InkScriptSemanticError> {
    let inner = spelling
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| error(InkScriptSemanticErrorCode::InvalidSyntax, "string"))?;
    let mut chars = inner.chars();
    let mut result = String::new();
    while let Some(character) = chars.next() {
        if character != '\\' {
            result.push(character);
            continue;
        }
        match chars.next() {
            Some('"') => result.push('"'),
            Some('\\') => result.push('\\'),
            Some('n') => result.push('\n'),
            Some('r') => result.push('\r'),
            Some('t') => result.push('\t'),
            Some('u') => {
                if chars.next() != Some('{') {
                    return Err(error(InkScriptSemanticErrorCode::InvalidSyntax, "string"));
                }
                let digits = chars
                    .by_ref()
                    .take_while(|character| *character != '}')
                    .collect::<String>();
                let scalar = u32::from_str_radix(&digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or_else(|| error(InkScriptSemanticErrorCode::InvalidSyntax, "string"))?;
                result.push(scalar);
            }
            _ => return Err(error(InkScriptSemanticErrorCode::InvalidSyntax, "string")),
        }
    }
    Ok(result)
}

fn decode_base64_literal(spelling: &str) -> Result<Vec<u8>, InkScriptSemanticError> {
    let body = spelling
        .strip_prefix("base64\"\"\"")
        .and_then(|value| value.strip_suffix("\"\"\""))
        .ok_or_else(|| error(InkScriptSemanticErrorCode::InvalidSyntax, "base64"))?;
    let bytes = body
        .bytes()
        .filter(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        .collect::<Vec<_>>();
    let mut result = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        if chunk.len() != 4 {
            return Err(error(InkScriptSemanticErrorCode::InvalidSyntax, "base64"));
        }
        let a = base64_digit(chunk[0])?;
        let b = base64_digit(chunk[1])?;
        let c = if chunk[2] == b'=' {
            0
        } else {
            base64_digit(chunk[2])?
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            base64_digit(chunk[3])?
        };
        result.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            result.push((b << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            result.push((c << 6) | d);
        }
    }
    Ok(result)
}

fn base64_digit(byte: u8) -> Result<u8, InkScriptSemanticError> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(error(InkScriptSemanticErrorCode::InvalidSyntax, "base64")),
    }
}

pub(crate) fn error(
    code: InkScriptSemanticErrorCode,
    path: impl Into<String>,
) -> InkScriptSemanticError {
    InkScriptSemanticError::new(code, path)
}
