use std::collections::BTreeSet;
use std::io;
use std::ops::Range;

use super::diagnostic::{
    InkScriptDiagnostic, InkScriptDiagnosticCode, InkScriptSourceRange, InkScriptSourceSpan,
};
use super::lexer::{
    InkScriptKeyword, InkScriptPunctuation, InkScriptToken, InkScriptTokenKind,
    lex_inkscript_with_limits,
};
use super::source::{INKSCRIPT_FILE_VERSION, InkScriptLexerLimits, InkScriptSource};

pub const MAX_INKSCRIPT_CST_NODES: usize = 2_097_152;
pub const MAX_INKSCRIPT_SECTIONS: usize = 9;
pub const MAX_INKSCRIPT_INPUTS: usize = 16_384;
pub const MAX_INKSCRIPT_PARAMETERS: usize = 4_096;
pub const MAX_INKSCRIPT_BINDINGS: usize = 65_536;
pub const MAX_INKSCRIPT_PROGRAM_STATEMENTS: usize = 65_536;
pub const MAX_INKSCRIPT_NESTING_DEPTH: usize = 64;
pub const MAX_INKSCRIPT_CONTAINER_ELEMENTS: usize = 65_536;
pub const MAX_INKSCRIPT_LIST_ELEMENTS: usize = 4_194_304;
pub const MAX_INKSCRIPT_REFERENCE_SEGMENTS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptParserLimits {
    lexer: InkScriptLexerLimits,
    nodes: usize,
    sections: usize,
    inputs: usize,
    parameters: usize,
    bindings: usize,
    program_statements: usize,
    nesting_depth: usize,
    container_elements: usize,
    list_elements: usize,
    reference_segments: usize,
}

impl InkScriptParserLimits {
    /// Returns the exact-current InkScript v2 lexer and CST resource limits.
    pub const fn exact_current() -> Self {
        Self {
            lexer: InkScriptLexerLimits::exact_current(),
            nodes: MAX_INKSCRIPT_CST_NODES,
            sections: MAX_INKSCRIPT_SECTIONS,
            inputs: MAX_INKSCRIPT_INPUTS,
            parameters: MAX_INKSCRIPT_PARAMETERS,
            bindings: MAX_INKSCRIPT_BINDINGS,
            program_statements: MAX_INKSCRIPT_PROGRAM_STATEMENTS,
            nesting_depth: MAX_INKSCRIPT_NESTING_DEPTH,
            container_elements: MAX_INKSCRIPT_CONTAINER_ELEMENTS,
            list_elements: MAX_INKSCRIPT_LIST_ELEMENTS,
            reference_segments: MAX_INKSCRIPT_REFERENCE_SEGMENTS,
        }
    }

    /// Uses a caller-lowered lexer envelope. Values in that envelope remain clamped to v2.
    pub const fn with_lexer_limits(mut self, lexer: InkScriptLexerLimits) -> Self {
        self.lexer = lexer;
        self
    }

    pub const fn with_node_limit(mut self, maximum: usize) -> Self {
        self.nodes = lowered(maximum, MAX_INKSCRIPT_CST_NODES);
        self
    }

    pub const fn with_section_limit(mut self, maximum: usize) -> Self {
        self.sections = lowered(maximum, MAX_INKSCRIPT_SECTIONS);
        self
    }

    pub const fn with_input_limit(mut self, maximum: usize) -> Self {
        self.inputs = lowered(maximum, MAX_INKSCRIPT_INPUTS);
        self
    }

    pub const fn with_parameter_limit(mut self, maximum: usize) -> Self {
        self.parameters = lowered(maximum, MAX_INKSCRIPT_PARAMETERS);
        self
    }

    pub const fn with_binding_limit(mut self, maximum: usize) -> Self {
        self.bindings = lowered(maximum, MAX_INKSCRIPT_BINDINGS);
        self
    }

    pub const fn with_program_statement_limit(mut self, maximum: usize) -> Self {
        self.program_statements = lowered(maximum, MAX_INKSCRIPT_PROGRAM_STATEMENTS);
        self
    }

    pub const fn with_nesting_depth_limit(mut self, maximum: usize) -> Self {
        self.nesting_depth = lowered(maximum, MAX_INKSCRIPT_NESTING_DEPTH);
        self
    }

    pub const fn with_container_element_limit(mut self, maximum: usize) -> Self {
        self.container_elements = lowered(maximum, MAX_INKSCRIPT_CONTAINER_ELEMENTS);
        self
    }

    pub const fn with_total_list_element_limit(mut self, maximum: usize) -> Self {
        self.list_elements = lowered(maximum, MAX_INKSCRIPT_LIST_ELEMENTS);
        self
    }

    pub const fn with_reference_segment_limit(mut self, maximum: usize) -> Self {
        self.reference_segments = lowered(maximum, MAX_INKSCRIPT_REFERENCE_SEGMENTS);
        self
    }
}

impl Default for InkScriptParserLimits {
    fn default() -> Self {
        Self::exact_current()
    }
}

const fn lowered(requested: usize, exact: usize) -> usize {
    if requested == 0 {
        1
    } else if requested < exact {
        requested
    } else {
        exact
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptDocumentKind {
    File,
    Fragment,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptCstNodeKind {
    File,
    Fragment,
    Header,
    RequiresSection,
    MetaSection,
    InputsSection,
    InputDeclaration,
    ParametersSection,
    ParameterDeclaration,
    BindingsSection,
    BindingDeclaration,
    ProgramSection,
    AssertStatement,
    StepStatement,
    EnabledMember,
    EditorGroupMember,
    InvokeMember,
    OutputSection,
    ExecutionSection,
    AssetsSection,
    AssetDeclaration,
    Record,
    Field,
    Value,
    List,
    Constructor,
    ArgumentList,
    AssetReference,
    Reference,
    ReferenceSegment,
    TypeReference,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptCstNode {
    kind: InkScriptCstNodeKind,
    span: InkScriptSourceSpan,
    token_range: Range<usize>,
    children: Vec<Self>,
}

impl InkScriptCstNode {
    /// Returns the structural role of this lossless CST node.
    pub const fn kind(&self) -> InkScriptCstNodeKind {
        self.kind
    }

    /// Returns the half-open byte span in the original source.
    pub const fn span(&self) -> InkScriptSourceSpan {
        self.span
    }

    /// Returns the source-order token indexes covered by this node.
    pub fn token_range(&self) -> Range<usize> {
        self.token_range.clone()
    }

    /// Returns the nested source-order CST nodes. Trivia remains available in `tokens()`.
    pub fn children(&self) -> &[Self] {
        &self.children
    }
}

#[derive(Debug)]
pub struct InkScriptCst<'source> {
    source: &'source InkScriptSource,
    document_kind: InkScriptDocumentKind,
    tokens: Vec<InkScriptToken>,
    root: InkScriptCstNode,
}

impl<'source> InkScriptCst<'source> {
    /// Returns the immutable caller-owned source borrowed for this CST's lifetime.
    pub const fn source(&self) -> &'source InkScriptSource {
        self.source
    }

    /// Returns whether the header selected a complete file, fragment, or neither.
    pub const fn document_kind(&self) -> InkScriptDocumentKind {
        self.document_kind
    }

    /// Returns every lexical token, including BOM, whitespace, comments, invalid tokens,
    /// original literal spelling, and the end sentinel when lexing completed.
    pub fn tokens(&self) -> &[InkScriptToken] {
        &self.tokens
    }

    /// Returns the lossless structural root, including recovery error nodes.
    pub const fn root(&self) -> &InkScriptCstNode {
        &self.root
    }

    /// Writes the exact unedited input bytes. This performs no normalization or repair.
    pub fn write_lossless(&self, writer: &mut impl io::Write) -> io::Result<()> {
        writer.write_all(self.source.bytes())
    }
}

#[derive(Debug)]
pub struct InkScriptParsed<'source> {
    cst: InkScriptCst<'source>,
    diagnostics: Vec<InkScriptDiagnostic>,
    complete: bool,
}

impl<'source> InkScriptParsed<'source> {
    /// Returns the CST for valid or invalid source. No semantic or executable form is
    /// published by this structural parser API.
    pub const fn cst(&self) -> &InkScriptCst<'source> {
        &self.cst
    }

    /// Returns lexical and parser diagnostics in stable source/production order.
    pub fn diagnostics(&self) -> &[InkScriptDiagnostic] {
        &self.diagnostics
    }

    /// Reports whether lexing and parsing reached end-of-source without a resource stop.
    pub const fn is_complete(&self) -> bool {
        self.complete
    }

    /// Reports exact-current structural validity. All diagnostics currently have error severity.
    pub fn is_valid(&self) -> bool {
        self.complete && self.diagnostics.is_empty()
    }
}

/// Parses a complete InkScript file or fragment with exact-current v2 limits.
///
/// The returned CST borrows the immutable source, retains all trivia and spelling, and
/// remains available after recoverable errors. Parsing performs no filesystem, Core, or
/// product action and never publishes a semantic AST or execution handle.
pub fn parse_inkscript(source: &InkScriptSource) -> InkScriptParsed<'_> {
    parse_inkscript_with_limits(source, InkScriptParserLimits::exact_current())
}

/// Parses with a caller-lowered exact-current resource envelope.
pub fn parse_inkscript_with_limits(
    source: &InkScriptSource,
    limits: InkScriptParserLimits,
) -> InkScriptParsed<'_> {
    Parser::new(source, limits).run()
}

pub(crate) fn parse_inkscript_value_tokens(
    source: &InkScriptSource,
    limits: InkScriptParserLimits,
) -> Option<Vec<InkScriptToken>> {
    Parser::new(source, limits).run_value()
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SectionTag {
    Requires,
    Meta,
    Inputs,
    Parameters,
    Bindings,
    Program,
    Output,
    Execution,
    Assets,
}

impl SectionTag {
    const fn node_kind(self) -> InkScriptCstNodeKind {
        match self {
            Self::Requires => InkScriptCstNodeKind::RequiresSection,
            Self::Meta => InkScriptCstNodeKind::MetaSection,
            Self::Inputs => InkScriptCstNodeKind::InputsSection,
            Self::Parameters => InkScriptCstNodeKind::ParametersSection,
            Self::Bindings => InkScriptCstNodeKind::BindingsSection,
            Self::Program => InkScriptCstNodeKind::ProgramSection,
            Self::Output => InkScriptCstNodeKind::OutputSection,
            Self::Execution => InkScriptCstNodeKind::ExecutionSection,
            Self::Assets => InkScriptCstNodeKind::AssetsSection,
        }
    }

    const fn allowed_in(self, kind: InkScriptDocumentKind) -> bool {
        match kind {
            InkScriptDocumentKind::File => true,
            InkScriptDocumentKind::Fragment => matches!(
                self,
                Self::Requires | Self::Parameters | Self::Bindings | Self::Program | Self::Assets
            ),
            InkScriptDocumentKind::Unknown => false,
        }
    }
}

struct StepResult {
    node: InkScriptCstNode,
    editor_group: Option<(String, InkScriptSourceRange)>,
}

struct Parser<'source> {
    source: &'source InkScriptSource,
    limits: InkScriptParserLimits,
    tokens: Vec<InkScriptToken>,
    diagnostics: Vec<InkScriptDiagnostic>,
    cursor: usize,
    complete: bool,
    stopped: bool,
    nodes: usize,
    nesting: usize,
    list_elements: usize,
    inputs: usize,
    parameters: usize,
    bindings: usize,
    program_statements: usize,
}

impl<'source> Parser<'source> {
    fn new(source: &'source InkScriptSource, limits: InkScriptParserLimits) -> Self {
        let lexed = lex_inkscript_with_limits(source, limits.lexer);
        let (tokens, diagnostics, complete) = lexed.into_parts();
        Self {
            source,
            limits,
            tokens,
            diagnostics,
            cursor: 0,
            complete,
            stopped: !complete,
            nodes: 1,
            nesting: 0,
            list_elements: 0,
            inputs: 0,
            parameters: 0,
            bindings: 0,
            program_statements: 0,
        }
    }

    fn run(mut self) -> InkScriptParsed<'source> {
        let mut children = Vec::new();
        let mut document_kind = InkScriptDocumentKind::Unknown;
        if !self.stopped {
            let (kind, header) = self.parse_header();
            document_kind = kind;
            if let Some(header) = header {
                children.push(header);
            }
            self.parse_sections(document_kind, &mut children);
        }

        let root_kind = match document_kind {
            InkScriptDocumentKind::File => InkScriptCstNodeKind::File,
            InkScriptDocumentKind::Fragment => InkScriptCstNodeKind::Fragment,
            InkScriptDocumentKind::Unknown => InkScriptCstNodeKind::Error,
        };
        let root = InkScriptCstNode {
            kind: root_kind,
            span: InkScriptSourceSpan::new(0, self.source.bytes().len() as u64),
            token_range: 0..self.tokens.len(),
            children,
        };
        InkScriptParsed {
            cst: InkScriptCst {
                source: self.source,
                document_kind,
                tokens: self.tokens,
                root,
            },
            diagnostics: self.diagnostics,
            complete: self.complete && !self.stopped,
        }
    }

    fn run_value(mut self) -> Option<Vec<InkScriptToken>> {
        let parsed = !self.stopped && self.parse_value().is_some();
        if parsed && self.peek_kind() != InkScriptTokenKind::EndOfSource {
            let range = self.current_range();
            self.report(InkScriptDiagnosticCode::UnexpectedToken, range);
        }
        (parsed && self.complete && !self.stopped && self.diagnostics.is_empty())
            .then_some(self.tokens)
    }

    fn parse_header(&mut self) -> (InkScriptDocumentKind, Option<InkScriptCstNode>) {
        let start = self.significant_index();
        let mut children = Vec::new();
        let kind = match self.peek_kind() {
            InkScriptTokenKind::Keyword(InkScriptKeyword::InkScript) => {
                self.consume();
                InkScriptDocumentKind::File
            }
            InkScriptTokenKind::Keyword(InkScriptKeyword::InkScriptFragment) => {
                self.consume();
                InkScriptDocumentKind::Fragment
            }
            _ => {
                if let Some(error) = self.unexpected_and_consume() {
                    children.push(error);
                }
                InkScriptDocumentKind::Unknown
            }
        };

        if kind != InkScriptDocumentKind::Unknown {
            if self.peek_kind() == InkScriptTokenKind::IntegerLiteral {
                let token = self.consume().expect("peeked token exists");
                let spelling = self.source.slice(token.span()).unwrap_or_default();
                match spelling.parse::<u32>() {
                    Ok(INKSCRIPT_FILE_VERSION) => {}
                    Ok(_) => {
                        self.report(InkScriptDiagnosticCode::UnsupportedVersion, token.range());
                    }
                    Err(_) => {
                        self.report(InkScriptDiagnosticCode::UnexpectedToken, token.range());
                    }
                }
            } else {
                let range = self.current_range();
                self.report(InkScriptDiagnosticCode::UnexpectedToken, range);
            }
            self.expect_punctuation(InkScriptPunctuation::Semicolon);
        }
        (
            kind,
            self.make_node(InkScriptCstNodeKind::Header, start, children),
        )
    }

    fn parse_sections(
        &mut self,
        document_kind: InkScriptDocumentKind,
        children: &mut Vec<InkScriptCstNode>,
    ) {
        let mut seen = BTreeSet::new();
        let mut section_count = 0_usize;
        while !self.at_end() && !self.stopped {
            let before = self.significant_index();
            if let Some(tag) = self.peek_section() {
                section_count = section_count.saturating_add(1);
                if section_count > self.limits.sections {
                    self.terminal_current(InkScriptDiagnosticCode::SectionLimitExceeded);
                    break;
                }
                let range = self.current_range();
                if !seen.insert(tag) {
                    self.report(InkScriptDiagnosticCode::DuplicateSection, range);
                }
                if !tag.allowed_in(document_kind) {
                    self.report(InkScriptDiagnosticCode::SectionNotAllowed, range);
                }
                if let Some(section) = self.parse_section(tag) {
                    children.push(section);
                }
            } else if let Some(error) = self.unexpected_and_consume() {
                children.push(error);
            }
            self.ensure_progress(before);
        }
        self.report_missing_sections(document_kind, &seen);
    }

    fn report_missing_sections(
        &mut self,
        document_kind: InkScriptDocumentKind,
        seen: &BTreeSet<SectionTag>,
    ) {
        let required: &[SectionTag] = match document_kind {
            InkScriptDocumentKind::File => &[
                SectionTag::Requires,
                SectionTag::Inputs,
                SectionTag::Program,
                SectionTag::Output,
                SectionTag::Execution,
            ],
            InkScriptDocumentKind::Fragment => &[SectionTag::Requires, SectionTag::Program],
            InkScriptDocumentKind::Unknown => &[],
        };
        for section in required {
            if !seen.contains(section) && !self.stopped {
                let range = self.current_range();
                self.report(InkScriptDiagnosticCode::MissingSection, range);
            }
        }
    }

    fn parse_section(&mut self, tag: SectionTag) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        self.consume();
        let mut children = Vec::new();
        let child = match tag {
            SectionTag::Requires
            | SectionTag::Meta
            | SectionTag::Output
            | SectionTag::Execution => self.parse_record(),
            SectionTag::Inputs => self.parse_inputs_section(),
            SectionTag::Parameters => self.parse_parameters_section(),
            SectionTag::Bindings => self.parse_bindings_section(),
            SectionTag::Program => self.parse_program_section(),
            SectionTag::Assets => self.parse_assets_section(),
        };
        if let Some(child) = child {
            children.push(child);
        }
        self.make_node(tag.node_kind(), start, children)
    }

    fn parse_inputs_section(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        let mut children = Vec::new();
        if !self.take_punctuation(InkScriptPunctuation::LeftBrace) {
            return self.expected_error(start);
        }
        while !self.at_punctuation(InkScriptPunctuation::RightBrace)
            && !self.at_end()
            && !self.stopped
        {
            let before = self.significant_index();
            if matches!(
                self.peek_kind(),
                InkScriptTokenKind::Keyword(
                    InkScriptKeyword::File
                        | InkScriptKeyword::Folder
                        | InkScriptKeyword::CurrentDocument
                        | InkScriptKeyword::CurrentSequence
                )
            ) {
                self.inputs = self.inputs.saturating_add(1);
                if self.inputs > self.limits.inputs {
                    self.terminal_current(InkScriptDiagnosticCode::InputLimitExceeded);
                    break;
                }
                if let Some(node) = self.parse_input_declaration() {
                    children.push(node);
                }
            } else if let Some(error) = self.recover_container_item() {
                children.push(error);
            }
            self.ensure_progress(before);
        }
        self.expect_punctuation(InkScriptPunctuation::RightBrace);
        self.make_node(InkScriptCstNodeKind::Record, start, children)
    }

    fn parse_input_declaration(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        let keyword = match self.peek_kind() {
            InkScriptTokenKind::Keyword(keyword) => keyword,
            _ => return self.expected_error(start),
        };
        self.consume();
        let mut children = Vec::new();
        if matches!(keyword, InkScriptKeyword::File | InkScriptKeyword::Folder) {
            self.expect_kind(InkScriptTokenKind::StringLiteral);
        }
        if self.at_punctuation(InkScriptPunctuation::LeftBrace)
            && let Some(record) = self.parse_record()
        {
            children.push(record);
        }
        self.expect_punctuation(InkScriptPunctuation::Semicolon);
        self.make_node(InkScriptCstNodeKind::InputDeclaration, start, children)
    }

    fn parse_parameters_section(&mut self) -> Option<InkScriptCstNode> {
        self.parse_declaration_section(
            InkScriptKeyword::Param,
            InkScriptDiagnosticCode::ParameterLimitExceeded,
            |parser| {
                parser.parameters = parser.parameters.saturating_add(1);
                parser.parameters <= parser.limits.parameters
            },
            Self::parse_parameter_declaration,
        )
    }

    fn parse_bindings_section(&mut self) -> Option<InkScriptCstNode> {
        self.parse_declaration_section(
            InkScriptKeyword::Let,
            InkScriptDiagnosticCode::BindingLimitExceeded,
            |parser| {
                parser.bindings = parser.bindings.saturating_add(1);
                parser.bindings <= parser.limits.bindings
            },
            Self::parse_binding_declaration,
        )
    }

    fn parse_assets_section(&mut self) -> Option<InkScriptCstNode> {
        self.parse_declaration_section(
            InkScriptKeyword::Asset,
            InkScriptDiagnosticCode::ContainerElementLimitExceeded,
            |_| true,
            Self::parse_asset_declaration,
        )
    }

    fn parse_declaration_section(
        &mut self,
        starter: InkScriptKeyword,
        limit_code: InkScriptDiagnosticCode,
        mut within_limit: impl FnMut(&mut Self) -> bool,
        parse: fn(&mut Self) -> Option<InkScriptCstNode>,
    ) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        let mut children = Vec::new();
        if !self.take_punctuation(InkScriptPunctuation::LeftBrace) {
            return self.expected_error(start);
        }
        let mut count = 0_usize;
        while !self.at_punctuation(InkScriptPunctuation::RightBrace)
            && !self.at_end()
            && !self.stopped
        {
            let before = self.significant_index();
            if self.peek_kind() == InkScriptTokenKind::Keyword(starter) {
                count = count.saturating_add(1);
                if count > self.limits.container_elements || !within_limit(self) {
                    self.terminal_current(limit_code);
                    break;
                }
                if let Some(node) = parse(self) {
                    children.push(node);
                }
            } else if let Some(error) = self.recover_container_item() {
                children.push(error);
            }
            self.ensure_progress(before);
        }
        self.expect_punctuation(InkScriptPunctuation::RightBrace);
        self.make_node(InkScriptCstNodeKind::Record, start, children)
    }

    fn parse_parameter_declaration(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        self.consume();
        let mut children = Vec::new();
        self.expect_identifier();
        self.expect_punctuation(InkScriptPunctuation::Colon);
        if let Some(type_ref) = self.parse_type_reference() {
            children.push(type_ref);
        }
        self.expect_punctuation(InkScriptPunctuation::Equals);
        if let Some(value) = self.parse_value() {
            children.push(value);
        }
        if self.at_punctuation(InkScriptPunctuation::LeftBrace)
            && let Some(record) = self.parse_record()
        {
            children.push(record);
        }
        self.expect_punctuation(InkScriptPunctuation::Semicolon);
        self.make_node(InkScriptCstNodeKind::ParameterDeclaration, start, children)
    }

    fn parse_binding_declaration(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        self.consume();
        let mut children = Vec::new();
        self.expect_identifier();
        self.expect_punctuation(InkScriptPunctuation::Equals);
        self.expect_keyword(InkScriptKeyword::Select);
        self.expect_identifier();
        if let Some(record) = self.parse_record() {
            children.push(record);
        }
        self.expect_punctuation(InkScriptPunctuation::Semicolon);
        self.make_node(InkScriptCstNodeKind::BindingDeclaration, start, children)
    }

    fn parse_asset_declaration(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        self.consume();
        let mut children = Vec::new();
        self.expect_identifier();
        if let Some(record) = self.parse_record() {
            children.push(record);
        }
        self.expect_punctuation(InkScriptPunctuation::Semicolon);
        self.make_node(InkScriptCstNodeKind::AssetDeclaration, start, children)
    }

    fn parse_program_section(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        let mut children = Vec::new();
        if !self.take_punctuation(InkScriptPunctuation::LeftBrace) {
            return self.expected_error(start);
        }
        let mut closed_groups = BTreeSet::new();
        let mut active_group: Option<String> = None;
        while !self.at_punctuation(InkScriptPunctuation::RightBrace)
            && !self.at_end()
            && !self.stopped
        {
            let before = self.significant_index();
            let mut group = None;
            match self.peek_kind() {
                InkScriptTokenKind::Keyword(InkScriptKeyword::Assert) => {
                    self.bump_program_count();
                    if let Some(node) = self.parse_assert_statement() {
                        children.push(node);
                    }
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::Step) => {
                    self.bump_program_count();
                    if let Some(result) = self.parse_step_statement() {
                        group = result.editor_group;
                        children.push(result.node);
                    }
                }
                _ => {
                    if let Some(error) = self.recover_container_item() {
                        children.push(error);
                    }
                }
            }
            if !self.stopped {
                let next_group = group.as_ref().map(|(key, _)| key.clone());
                if next_group != active_group {
                    if let Some(previous) = active_group.take() {
                        closed_groups.insert(previous);
                    }
                    if let Some((key, range)) = group {
                        if closed_groups.contains(&key) {
                            self.report(InkScriptDiagnosticCode::NoncontiguousEditorGroup, range);
                        }
                        active_group = Some(key);
                    }
                }
            }
            self.ensure_progress(before);
        }
        self.expect_punctuation(InkScriptPunctuation::RightBrace);
        self.make_node(InkScriptCstNodeKind::Record, start, children)
    }

    fn bump_program_count(&mut self) {
        self.program_statements = self.program_statements.saturating_add(1);
        if self.program_statements > self.limits.program_statements {
            self.terminal_current(InkScriptDiagnosticCode::ProgramStatementLimitExceeded);
        }
    }

    fn parse_assert_statement(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        self.consume();
        let mut children = Vec::new();
        self.expect_identifier();
        if let Some(record) = self.parse_record() {
            children.push(record);
        }
        self.expect_punctuation(InkScriptPunctuation::Semicolon);
        self.make_node(InkScriptCstNodeKind::AssertStatement, start, children)
    }

    fn parse_step_statement(&mut self) -> Option<StepResult> {
        let start = self.significant_index();
        self.consume();
        self.expect_kind(InkScriptTokenKind::StringLiteral);
        if self.take_keyword(InkScriptKeyword::As) {
            self.expect_identifier();
        }
        if !self.take_punctuation(InkScriptPunctuation::LeftBrace) {
            let node = self.expected_error(start)?;
            return Some(StepResult {
                node,
                editor_group: None,
            });
        }

        let mut children = Vec::new();
        let mut enabled = false;
        let mut invoke = false;
        let mut editor_group_seen = false;
        let mut editor_group = None;
        while !self.at_punctuation(InkScriptPunctuation::RightBrace)
            && !self.at_end()
            && !self.stopped
        {
            let before = self.significant_index();
            let member_range = self.current_range();
            match self.peek_kind() {
                InkScriptTokenKind::Keyword(InkScriptKeyword::Enabled) => {
                    if enabled {
                        self.report(InkScriptDiagnosticCode::DuplicateMember, member_range);
                    }
                    enabled = true;
                    if let Some(node) = self.parse_enabled_member() {
                        children.push(node);
                    }
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::EditorGroup) => {
                    if editor_group_seen {
                        self.report(InkScriptDiagnosticCode::DuplicateMember, member_range);
                    }
                    editor_group_seen = true;
                    let (node, key) = self.parse_editor_group_member();
                    if editor_group.is_none() {
                        editor_group = key;
                    }
                    if let Some(node) = node {
                        children.push(node);
                    }
                }
                InkScriptTokenKind::Keyword(InkScriptKeyword::Invoke) => {
                    if invoke {
                        self.report(InkScriptDiagnosticCode::DuplicateMember, member_range);
                    }
                    invoke = true;
                    if let Some(node) = self.parse_invoke_member() {
                        children.push(node);
                    }
                }
                _ => {
                    if let Some(error) = self.recover_container_item() {
                        children.push(error);
                    }
                }
            }
            self.ensure_progress(before);
        }
        let end_range = self.current_range();
        if !enabled {
            self.report(InkScriptDiagnosticCode::MissingMember, end_range);
        }
        if !invoke {
            self.report(InkScriptDiagnosticCode::MissingMember, end_range);
        }
        self.expect_punctuation(InkScriptPunctuation::RightBrace);
        let node = self.make_node(InkScriptCstNodeKind::StepStatement, start, children)?;
        Some(StepResult { node, editor_group })
    }

    fn parse_enabled_member(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        self.consume();
        self.expect_punctuation(InkScriptPunctuation::Equals);
        match self.peek_kind() {
            InkScriptTokenKind::Keyword(InkScriptKeyword::True | InkScriptKeyword::False) => {
                self.consume();
            }
            _ => {
                let range = self.current_range();
                self.report(InkScriptDiagnosticCode::UnexpectedToken, range);
                if !self.at_end() {
                    self.consume();
                }
            }
        }
        self.expect_punctuation(InkScriptPunctuation::Semicolon);
        self.make_node(InkScriptCstNodeKind::EnabledMember, start, Vec::new())
    }

    fn parse_editor_group_member(
        &mut self,
    ) -> (
        Option<InkScriptCstNode>,
        Option<(String, InkScriptSourceRange)>,
    ) {
        let start = self.significant_index();
        self.consume();
        self.expect_punctuation(InkScriptPunctuation::Equals);
        let mut result = None;
        if self.peek_kind() == InkScriptTokenKind::StringLiteral {
            let token = self.consume().expect("peeked token exists");
            let decoded = self
                .source
                .slice(token.span())
                .and_then(decode_string_literal)
                .unwrap_or_default();
            if decoded.is_empty() {
                self.report(InkScriptDiagnosticCode::InvalidEditorGroup, token.range());
            } else {
                result = Some((decoded, token.range()));
            }
        } else {
            let range = self.current_range();
            self.report(InkScriptDiagnosticCode::UnexpectedToken, range);
        }
        self.expect_punctuation(InkScriptPunctuation::Semicolon);
        (
            self.make_node(InkScriptCstNodeKind::EditorGroupMember, start, Vec::new()),
            result,
        )
    }

    fn parse_invoke_member(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        self.consume();
        let mut children = Vec::new();
        self.expect_identifier();
        if let Some(record) = self.parse_record() {
            children.push(record);
        }
        self.expect_punctuation(InkScriptPunctuation::Semicolon);
        self.make_node(InkScriptCstNodeKind::InvokeMember, start, children)
    }

    fn parse_record(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        if !self.enter_nesting() {
            return None;
        }
        if !self.take_punctuation(InkScriptPunctuation::LeftBrace) {
            self.leave_nesting();
            return self.expected_error(start);
        }
        let mut children = Vec::new();
        let mut fields = BTreeSet::new();
        let mut count = 0_usize;
        while !self.at_punctuation(InkScriptPunctuation::RightBrace)
            && !self.at_end()
            && !self.stopped
        {
            let before = self.significant_index();
            if self.can_start_field() {
                count = count.saturating_add(1);
                if count > self.limits.container_elements {
                    self.terminal_current(InkScriptDiagnosticCode::ContainerElementLimitExceeded);
                    break;
                }
                let (node, name, range) = self.parse_field();
                if let Some(name) = name
                    && !fields.insert(name)
                {
                    self.report(InkScriptDiagnosticCode::DuplicateField, range);
                }
                if let Some(node) = node {
                    children.push(node);
                }
            } else if let Some(error) = self.recover_container_item() {
                children.push(error);
            }
            self.ensure_progress(before);
        }
        self.expect_punctuation(InkScriptPunctuation::RightBrace);
        self.leave_nesting();
        self.make_node(InkScriptCstNodeKind::Record, start, children)
    }

    fn parse_field(
        &mut self,
    ) -> (
        Option<InkScriptCstNode>,
        Option<String>,
        InkScriptSourceRange,
    ) {
        let start = self.significant_index();
        let range = self.current_range();
        let name = self.take_field_name();
        self.expect_punctuation(InkScriptPunctuation::Equals);
        let mut children = Vec::new();
        if let Some(value) = self.parse_value() {
            children.push(value);
        }
        self.expect_punctuation(InkScriptPunctuation::Semicolon);
        (
            self.make_node(InkScriptCstNodeKind::Field, start, children),
            name,
            range,
        )
    }

    fn parse_value(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        let mut children = Vec::new();
        match self.peek_kind() {
            InkScriptTokenKind::Keyword(
                InkScriptKeyword::True | InkScriptKeyword::False | InkScriptKeyword::None,
            )
            | InkScriptTokenKind::IntegerLiteral
            | InkScriptTokenKind::DecimalLiteral
            | InkScriptTokenKind::StringLiteral
            | InkScriptTokenKind::UuidLiteral
            | InkScriptTokenKind::DigestLiteral
            | InkScriptTokenKind::Base64Literal => {
                self.consume();
            }
            InkScriptTokenKind::Punctuation(InkScriptPunctuation::Dollar) => {
                if let Some(reference) = self.parse_reference() {
                    children.push(reference);
                }
            }
            InkScriptTokenKind::Punctuation(InkScriptPunctuation::LeftBracket) => {
                if let Some(list) = self.parse_list() {
                    children.push(list);
                }
            }
            InkScriptTokenKind::Punctuation(InkScriptPunctuation::LeftBrace) => {
                if let Some(record) = self.parse_record() {
                    children.push(record);
                }
            }
            InkScriptTokenKind::Keyword(InkScriptKeyword::Asset) => {
                if let Some(reference) = self.parse_asset_reference() {
                    children.push(reference);
                }
            }
            InkScriptTokenKind::Word => {
                self.consume();
                if self.at_punctuation(InkScriptPunctuation::LeftParenthesis) {
                    let mut constructor_children = Vec::new();
                    if let Some(arguments) = self.parse_argument_list() {
                        constructor_children.push(arguments);
                    }
                    if let Some(constructor) = self.make_node(
                        InkScriptCstNodeKind::Constructor,
                        start,
                        constructor_children,
                    ) {
                        children.push(constructor);
                    }
                }
            }
            InkScriptTokenKind::Keyword(_) => {
                let range = self.current_range();
                self.report(InkScriptDiagnosticCode::ReservedIdentifier, range);
                self.consume();
            }
            _ => {
                return self.expected_error(start);
            }
        }
        self.make_node(InkScriptCstNodeKind::Value, start, children)
    }

    fn parse_asset_reference(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        self.consume();
        if !self.take_punctuation(InkScriptPunctuation::LeftParenthesis) {
            let range = self.current_range();
            self.report(InkScriptDiagnosticCode::UnexpectedToken, range);
            return self.make_node(InkScriptCstNodeKind::AssetReference, start, Vec::new());
        }
        self.expect_identifier();
        self.expect_punctuation(InkScriptPunctuation::RightParenthesis);
        self.make_node(InkScriptCstNodeKind::AssetReference, start, Vec::new())
    }

    fn parse_argument_list(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        if !self.enter_nesting() {
            return None;
        }
        self.consume();
        let mut children = Vec::new();
        let mut count = 0_usize;
        while !self.at_punctuation(InkScriptPunctuation::RightParenthesis)
            && !self.at_end()
            && !self.stopped
        {
            count = count.saturating_add(1);
            if count > self.limits.container_elements {
                self.terminal_current(InkScriptDiagnosticCode::ContainerElementLimitExceeded);
                break;
            }
            if let Some(value) = self.parse_value() {
                children.push(value);
            }
            if !self.take_punctuation(InkScriptPunctuation::Comma) {
                break;
            }
        }
        self.expect_punctuation(InkScriptPunctuation::RightParenthesis);
        self.leave_nesting();
        self.make_node(InkScriptCstNodeKind::ArgumentList, start, children)
    }

    fn parse_list(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        if !self.enter_nesting() {
            return None;
        }
        self.consume();
        let mut children = Vec::new();
        let mut count = 0_usize;
        while !self.at_punctuation(InkScriptPunctuation::RightBracket)
            && !self.at_end()
            && !self.stopped
        {
            count = count.saturating_add(1);
            if count > self.limits.container_elements {
                self.terminal_current(InkScriptDiagnosticCode::ContainerElementLimitExceeded);
                break;
            }
            self.list_elements = self.list_elements.saturating_add(1);
            if self.list_elements > self.limits.list_elements {
                self.terminal_current(InkScriptDiagnosticCode::ListElementLimitExceeded);
                break;
            }
            if let Some(value) = self.parse_value() {
                children.push(value);
            }
            if !self.take_punctuation(InkScriptPunctuation::Comma) {
                break;
            }
        }
        self.expect_punctuation(InkScriptPunctuation::RightBracket);
        self.leave_nesting();
        self.make_node(InkScriptCstNodeKind::List, start, children)
    }

    fn parse_reference(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        self.consume();
        self.expect_identifier();
        let mut children = Vec::new();
        let mut segments = 0_usize;
        while matches!(
            self.peek_kind(),
            InkScriptTokenKind::Punctuation(
                InkScriptPunctuation::Dot | InkScriptPunctuation::LeftBracket
            )
        ) && !self.stopped
        {
            segments = segments.saturating_add(1);
            if segments > self.limits.reference_segments {
                self.terminal_current(InkScriptDiagnosticCode::ReferenceSegmentLimitExceeded);
                break;
            }
            let segment_start = self.significant_index();
            if self.take_punctuation(InkScriptPunctuation::Dot) {
                self.take_field_name();
            } else {
                self.consume();
                if self.peek_kind() == InkScriptTokenKind::IntegerLiteral {
                    let token = self.consume().expect("peeked token exists");
                    let spelling = self.source.slice(token.span()).unwrap_or_default();
                    if spelling.starts_with('-') {
                        self.report(InkScriptDiagnosticCode::UnexpectedToken, token.range());
                    }
                } else {
                    let range = self.current_range();
                    self.report(InkScriptDiagnosticCode::UnexpectedToken, range);
                }
                self.expect_punctuation(InkScriptPunctuation::RightBracket);
            }
            if let Some(node) = self.make_node(
                InkScriptCstNodeKind::ReferenceSegment,
                segment_start,
                Vec::new(),
            ) {
                children.push(node);
            }
        }
        self.make_node(InkScriptCstNodeKind::Reference, start, children)
    }

    fn parse_type_reference(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        let mut children = Vec::new();
        match self.peek_kind() {
            InkScriptTokenKind::Word => {
                self.consume();
            }
            InkScriptTokenKind::Keyword(InkScriptKeyword::List | InkScriptKeyword::Nullable) => {
                if !self.enter_nesting() {
                    return None;
                }
                self.consume();
                self.expect_punctuation(InkScriptPunctuation::LessThan);
                if let Some(inner) = self.parse_type_reference() {
                    children.push(inner);
                }
                self.expect_punctuation(InkScriptPunctuation::GreaterThan);
                self.leave_nesting();
            }
            InkScriptTokenKind::Keyword(_) => {
                let range = self.current_range();
                self.report(InkScriptDiagnosticCode::ReservedIdentifier, range);
                self.consume();
            }
            _ => return self.expected_error(start),
        }
        self.make_node(InkScriptCstNodeKind::TypeReference, start, children)
    }

    fn enter_nesting(&mut self) -> bool {
        if self.nesting >= self.limits.nesting_depth {
            self.terminal_current(InkScriptDiagnosticCode::NestingLimitExceeded);
            return false;
        }
        self.nesting += 1;
        true
    }

    fn leave_nesting(&mut self) {
        self.nesting = self.nesting.saturating_sub(1);
    }

    fn peek_section(&mut self) -> Option<SectionTag> {
        Some(match self.peek_kind() {
            InkScriptTokenKind::Keyword(InkScriptKeyword::Requires) => SectionTag::Requires,
            InkScriptTokenKind::Keyword(InkScriptKeyword::Meta) => SectionTag::Meta,
            InkScriptTokenKind::Keyword(InkScriptKeyword::Inputs) => SectionTag::Inputs,
            InkScriptTokenKind::Keyword(InkScriptKeyword::Parameters) => SectionTag::Parameters,
            InkScriptTokenKind::Keyword(InkScriptKeyword::Bindings) => SectionTag::Bindings,
            InkScriptTokenKind::Keyword(InkScriptKeyword::Program) => SectionTag::Program,
            InkScriptTokenKind::Keyword(InkScriptKeyword::Output) => SectionTag::Output,
            InkScriptTokenKind::Keyword(InkScriptKeyword::Execution) => SectionTag::Execution,
            InkScriptTokenKind::Keyword(InkScriptKeyword::Assets) => SectionTag::Assets,
            _ => return None,
        })
    }

    fn can_start_field(&mut self) -> bool {
        matches!(
            self.peek_kind(),
            InkScriptTokenKind::Word | InkScriptTokenKind::Keyword(_)
        )
    }

    fn take_field_name(&mut self) -> Option<String> {
        match self.peek_kind() {
            InkScriptTokenKind::Word | InkScriptTokenKind::Keyword(_) => {
                let token = self.consume().expect("peeked token exists");
                Some(
                    self.source
                        .slice(token.span())
                        .unwrap_or_default()
                        .to_owned(),
                )
            }
            _ => {
                let range = self.current_range();
                self.report(InkScriptDiagnosticCode::UnexpectedToken, range);
                None
            }
        }
    }

    fn expect_identifier(&mut self) -> Option<String> {
        match self.peek_kind() {
            InkScriptTokenKind::Word => {
                let token = self.consume().expect("peeked token exists");
                Some(
                    self.source
                        .slice(token.span())
                        .unwrap_or_default()
                        .to_owned(),
                )
            }
            InkScriptTokenKind::Keyword(_) => {
                let token = self.consume().expect("peeked token exists");
                self.report(InkScriptDiagnosticCode::ReservedIdentifier, token.range());
                None
            }
            _ => {
                let range = self.current_range();
                self.report(InkScriptDiagnosticCode::UnexpectedToken, range);
                None
            }
        }
    }

    fn expect_kind(&mut self, expected: InkScriptTokenKind) -> bool {
        if self.peek_kind() == expected {
            self.consume();
            true
        } else {
            let range = self.current_range();
            self.report(InkScriptDiagnosticCode::UnexpectedToken, range);
            false
        }
    }

    fn expect_keyword(&mut self, expected: InkScriptKeyword) -> bool {
        if self.take_keyword(expected) {
            true
        } else {
            let range = self.current_range();
            self.report(InkScriptDiagnosticCode::UnexpectedToken, range);
            false
        }
    }

    fn take_keyword(&mut self, expected: InkScriptKeyword) -> bool {
        if self.peek_kind() == InkScriptTokenKind::Keyword(expected) {
            self.consume();
            true
        } else {
            false
        }
    }

    fn expect_punctuation(&mut self, expected: InkScriptPunctuation) -> bool {
        if self.take_punctuation(expected) {
            true
        } else {
            let range = self.current_range();
            self.report(InkScriptDiagnosticCode::UnexpectedToken, range);
            false
        }
    }

    fn take_punctuation(&mut self, expected: InkScriptPunctuation) -> bool {
        if self.at_punctuation(expected) {
            self.consume();
            true
        } else {
            false
        }
    }

    fn at_punctuation(&mut self, expected: InkScriptPunctuation) -> bool {
        self.peek_kind() == InkScriptTokenKind::Punctuation(expected)
    }

    fn at_end(&mut self) -> bool {
        self.peek_kind() == InkScriptTokenKind::EndOfSource
    }

    fn peek_kind(&mut self) -> InkScriptTokenKind {
        self.skip_trivia();
        self.tokens
            .get(self.cursor)
            .map(InkScriptToken::kind)
            .unwrap_or(InkScriptTokenKind::EndOfSource)
    }

    fn significant_index(&mut self) -> usize {
        self.skip_trivia();
        self.cursor
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

    fn consume(&mut self) -> Option<InkScriptToken> {
        self.skip_trivia();
        let token = self.tokens.get(self.cursor).copied()?;
        if token.kind() != InkScriptTokenKind::EndOfSource {
            self.cursor += 1;
        }
        Some(token)
    }

    fn current_range(&mut self) -> InkScriptSourceRange {
        self.skip_trivia();
        self.tokens
            .get(self.cursor)
            .map(InkScriptToken::range)
            .unwrap_or_else(|| self.eof_range())
    }

    fn eof_range(&self) -> InkScriptSourceRange {
        let offset = self.source.bytes().len() as u64;
        self.source
            .line_map()
            .range(InkScriptSourceSpan::new(offset, offset))
            .expect("source end is a valid UTF-8 boundary")
    }

    fn report(&mut self, code: InkScriptDiagnosticCode, range: InkScriptSourceRange) {
        if self.stopped {
            return;
        }
        let maximum = self.limits.lexer.diagnostics();
        if self.diagnostics.len() >= maximum.saturating_sub(1) {
            self.terminal_at(
                InkScriptDiagnosticCode::ParserDiagnosticLimitExceeded,
                range,
            );
            return;
        }
        self.diagnostics
            .push(InkScriptDiagnostic::error(code, self.source.id(), range));
    }

    fn terminal_current(&mut self, code: InkScriptDiagnosticCode) {
        let range = self.current_range();
        self.terminal_at(code, range);
    }

    fn terminal_at(&mut self, code: InkScriptDiagnosticCode, range: InkScriptSourceRange) {
        if self.diagnostics.len() < self.limits.lexer.diagnostics() {
            self.diagnostics
                .push(InkScriptDiagnostic::error(code, self.source.id(), range));
        }
        self.stopped = true;
    }

    fn unexpected_and_consume(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        let range = self.current_range();
        self.report(InkScriptDiagnosticCode::UnexpectedToken, range);
        if !self.at_end() {
            self.consume();
        }
        self.make_node(InkScriptCstNodeKind::Error, start, Vec::new())
    }

    fn expected_error(&mut self, start: usize) -> Option<InkScriptCstNode> {
        let range = self.current_range();
        self.report(InkScriptDiagnosticCode::UnexpectedToken, range);
        if !self.at_end() {
            self.consume();
        }
        self.make_node(InkScriptCstNodeKind::Error, start, Vec::new())
    }

    fn recover_container_item(&mut self) -> Option<InkScriptCstNode> {
        let start = self.significant_index();
        let range = self.current_range();
        self.report(InkScriptDiagnosticCode::UnexpectedToken, range);
        while !self.at_end()
            && !self.at_punctuation(InkScriptPunctuation::RightBrace)
            && !self.at_punctuation(InkScriptPunctuation::Semicolon)
            && !self.stopped
        {
            self.consume();
        }
        self.take_punctuation(InkScriptPunctuation::Semicolon);
        self.make_node(InkScriptCstNodeKind::Error, start, Vec::new())
    }

    fn make_node(
        &mut self,
        kind: InkScriptCstNodeKind,
        start: usize,
        children: Vec<InkScriptCstNode>,
    ) -> Option<InkScriptCstNode> {
        if self.nodes >= self.limits.nodes {
            if !self.stopped {
                self.terminal_current(InkScriptDiagnosticCode::NodeLimitExceeded);
            }
            return None;
        }
        self.nodes += 1;
        let end = self.cursor.min(self.tokens.len());
        let span_start = self
            .tokens
            .get(start)
            .map(InkScriptToken::span)
            .map_or(self.source.bytes().len() as u64, InkScriptSourceSpan::start);
        let span_end = if end > start {
            self.tokens
                .get(end - 1)
                .map(InkScriptToken::span)
                .map_or(span_start, InkScriptSourceSpan::end)
        } else {
            span_start
        };
        Some(InkScriptCstNode {
            kind,
            span: InkScriptSourceSpan::new(span_start, span_end),
            token_range: start..end,
            children,
        })
    }

    fn ensure_progress(&mut self, before: usize) {
        if !self.stopped && !self.at_end() && self.significant_index() == before {
            self.consume();
        }
    }
}

fn decode_string_literal(spelling: &str) -> Option<String> {
    let inner = spelling.strip_prefix('"')?.strip_suffix('"')?;
    let mut chars = inner.chars();
    let mut result = String::new();
    while let Some(character) = chars.next() {
        if character != '\\' {
            result.push(character);
            continue;
        }
        match chars.next()? {
            '"' => result.push('"'),
            '\\' => result.push('\\'),
            'n' => result.push('\n'),
            'r' => result.push('\r'),
            't' => result.push('\t'),
            'u' => {
                if chars.next()? != '{' {
                    return None;
                }
                let mut digits = String::new();
                loop {
                    let next = chars.next()?;
                    if next == '}' {
                        break;
                    }
                    digits.push(next);
                }
                let scalar = u32::from_str_radix(&digits, 16).ok()?;
                result.push(char::from_u32(scalar)?);
            }
            _ => return None,
        }
    }
    Some(result)
}
