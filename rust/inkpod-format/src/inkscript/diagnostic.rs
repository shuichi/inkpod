#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InkScriptSourceId(u64);

impl InkScriptSourceId {
    /// Creates an opaque caller-owned source identity.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the fixed-width identity value supplied by the caller.
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptSourcePosition {
    line: u32,
    column: u32,
}

impl InkScriptSourcePosition {
    /// Creates a 1-based Unicode-scalar source position.
    pub const fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }

    /// Returns the 1-based line.
    pub const fn line(self) -> u32 {
        self.line
    }

    /// Returns the 1-based Unicode-scalar column.
    pub const fn column(self) -> u32 {
        self.column
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptSourceSpan {
    start: u64,
    end: u64,
}

impl InkScriptSourceSpan {
    /// Creates a half-open byte span measured from the original source start.
    pub const fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    /// Returns the inclusive byte start.
    pub const fn start(self) -> u64 {
        self.start
    }

    /// Returns the exclusive byte end.
    pub const fn end(self) -> u64 {
        self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptSourceRange {
    span: InkScriptSourceSpan,
    start: InkScriptSourcePosition,
    end: InkScriptSourcePosition,
}

impl InkScriptSourceRange {
    pub(crate) const fn new(
        span: InkScriptSourceSpan,
        start: InkScriptSourcePosition,
        end: InkScriptSourcePosition,
    ) -> Self {
        Self { span, start, end }
    }

    /// Returns the authoritative half-open UTF-8 byte span.
    pub const fn span(self) -> InkScriptSourceSpan {
        self.span
    }

    /// Returns the 1-based display start.
    pub const fn start(self) -> InkScriptSourcePosition {
        self.start
    }

    /// Returns the 1-based display end.
    pub const fn end(self) -> InkScriptSourcePosition {
        self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptDiagnosticSeverity {
    Error,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InkScriptDiagnosticCode {
    SourceTooLarge,
    InvalidUtf8,
    RawNul,
    StandaloneCarriageReturn,
    UnexpectedCharacter,
    IdentifierTooLong,
    NumericLiteralTooLong,
    InvalidNumber,
    UnterminatedString,
    InvalidStringCharacter,
    InvalidEscape,
    InvalidUnicodeEscape,
    StringTooLong,
    InvalidUuidLiteral,
    InvalidDigestLiteral,
    UnterminatedBase64,
    InvalidBase64Character,
    InvalidBase64Encoding,
    InlineAssetTooLarge,
    TokenLimitExceeded,
    DiagnosticLimitExceeded,
    UnexpectedToken,
    UnsupportedVersion,
    MissingSection,
    DuplicateSection,
    SectionNotAllowed,
    DuplicateMember,
    MissingMember,
    DuplicateField,
    ReservedIdentifier,
    InvalidEditorGroup,
    NoncontiguousEditorGroup,
    NodeLimitExceeded,
    NestingLimitExceeded,
    SectionLimitExceeded,
    ContainerElementLimitExceeded,
    ListElementLimitExceeded,
    ReferenceSegmentLimitExceeded,
    InputLimitExceeded,
    ParameterLimitExceeded,
    BindingLimitExceeded,
    ProgramStatementLimitExceeded,
    ParserDiagnosticLimitExceeded,
}

impl InkScriptDiagnosticCode {
    /// Returns the stable, locale-independent diagnostic identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceTooLarge => "INKS-LEX-0001",
            Self::InvalidUtf8 => "INKS-LEX-0002",
            Self::RawNul => "INKS-LEX-0003",
            Self::StandaloneCarriageReturn => "INKS-LEX-0004",
            Self::UnexpectedCharacter => "INKS-LEX-0005",
            Self::IdentifierTooLong => "INKS-LEX-0006",
            Self::NumericLiteralTooLong => "INKS-LEX-0007",
            Self::InvalidNumber => "INKS-LEX-0008",
            Self::UnterminatedString => "INKS-LEX-0009",
            Self::InvalidStringCharacter => "INKS-LEX-0010",
            Self::InvalidEscape => "INKS-LEX-0011",
            Self::InvalidUnicodeEscape => "INKS-LEX-0012",
            Self::StringTooLong => "INKS-LEX-0013",
            Self::InvalidUuidLiteral => "INKS-LEX-0014",
            Self::InvalidDigestLiteral => "INKS-LEX-0015",
            Self::UnterminatedBase64 => "INKS-LEX-0016",
            Self::InvalidBase64Character => "INKS-LEX-0017",
            Self::InvalidBase64Encoding => "INKS-LEX-0018",
            Self::InlineAssetTooLarge => "INKS-LEX-0019",
            Self::TokenLimitExceeded => "INKS-LEX-0020",
            Self::DiagnosticLimitExceeded => "INKS-LEX-0021",
            Self::UnexpectedToken => "INKS-PARSE-0001",
            Self::UnsupportedVersion => "INKS-PARSE-0002",
            Self::MissingSection => "INKS-PARSE-0003",
            Self::DuplicateSection => "INKS-PARSE-0004",
            Self::SectionNotAllowed => "INKS-PARSE-0005",
            Self::DuplicateMember => "INKS-PARSE-0006",
            Self::MissingMember => "INKS-PARSE-0007",
            Self::DuplicateField => "INKS-PARSE-0008",
            Self::ReservedIdentifier => "INKS-PARSE-0009",
            Self::InvalidEditorGroup => "INKS-PARSE-0010",
            Self::NoncontiguousEditorGroup => "INKS-PARSE-0011",
            Self::NodeLimitExceeded => "INKS-PARSE-0012",
            Self::NestingLimitExceeded => "INKS-PARSE-0013",
            Self::SectionLimitExceeded => "INKS-PARSE-0014",
            Self::ContainerElementLimitExceeded => "INKS-PARSE-0015",
            Self::ListElementLimitExceeded => "INKS-PARSE-0016",
            Self::ReferenceSegmentLimitExceeded => "INKS-PARSE-0017",
            Self::InputLimitExceeded => "INKS-PARSE-0018",
            Self::ParameterLimitExceeded => "INKS-PARSE-0019",
            Self::BindingLimitExceeded => "INKS-PARSE-0020",
            Self::ProgramStatementLimitExceeded => "INKS-PARSE-0021",
            Self::ParserDiagnosticLimitExceeded => "INKS-PARSE-0022",
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::SourceTooLarge => "InkScript source exceeds its byte limit",
            Self::InvalidUtf8 => "InkScript source is not valid UTF-8",
            Self::RawNul => "NUL is not allowed in InkScript source",
            Self::StandaloneCarriageReturn => "a carriage return must be followed by a line feed",
            Self::UnexpectedCharacter => "the character cannot start an InkScript token",
            Self::IdentifierTooLong => "identifier or keyword token exceeds its byte limit",
            Self::NumericLiteralTooLong => "numeric literal exceeds its byte limit",
            Self::InvalidNumber => "numeric literal is not valid InkScript syntax",
            Self::UnterminatedString => "quoted string is not terminated on the same line",
            Self::InvalidStringCharacter => "quoted string contains a forbidden control character",
            Self::InvalidEscape => "quoted string contains an invalid escape",
            Self::InvalidUnicodeEscape => "Unicode escape does not name one non-NUL scalar",
            Self::StringTooLong => "decoded UTF-8 string exceeds its byte limit",
            Self::InvalidUuidLiteral => "UUID literal is not lowercase canonical hyphenated UUID",
            Self::InvalidDigestLiteral => "BLAKE3 literal is not 64 lowercase hexadecimal digits",
            Self::UnterminatedBase64 => "Base64 literal is not terminated",
            Self::InvalidBase64Character => "Base64 literal contains a forbidden character",
            Self::InvalidBase64Encoding => "Base64 literal has invalid padding or unused bits",
            Self::InlineAssetTooLarge => "decoded inline asset exceeds its byte limit",
            Self::TokenLimitExceeded => "InkScript token count exceeds its limit",
            Self::DiagnosticLimitExceeded => "InkScript diagnostic count exceeds its limit",
            Self::UnexpectedToken => "unexpected token in InkScript source",
            Self::UnsupportedVersion => "InkScript version is not the exact current version",
            Self::MissingSection => "a required InkScript section is missing",
            Self::DuplicateSection => "an InkScript section appears more than once",
            Self::SectionNotAllowed => "the section is not allowed in this InkScript document kind",
            Self::DuplicateMember => "a closed syntax member appears more than once",
            Self::MissingMember => "a required closed syntax member is missing",
            Self::DuplicateField => "a record field appears more than once",
            Self::ReservedIdentifier => "a reserved keyword cannot be used as an identifier",
            Self::InvalidEditorGroup => "editor_group must contain a non-empty local key",
            Self::NoncontiguousEditorGroup => {
                "all steps in an editor_group must form one contiguous run"
            }
            Self::NodeLimitExceeded => "InkScript CST node count exceeds its limit",
            Self::NestingLimitExceeded => "InkScript syntax nesting exceeds its limit",
            Self::SectionLimitExceeded => "InkScript section count exceeds its limit",
            Self::ContainerElementLimitExceeded => {
                "an InkScript container exceeds its element limit"
            }
            Self::ListElementLimitExceeded => {
                "InkScript list elements exceed their aggregate limit"
            }
            Self::ReferenceSegmentLimitExceeded => {
                "an InkScript reference path exceeds its segment limit"
            }
            Self::InputLimitExceeded => "InkScript input declarations exceed their limit",
            Self::ParameterLimitExceeded => "InkScript parameters exceed their limit",
            Self::BindingLimitExceeded => "InkScript bindings exceed their limit",
            Self::ProgramStatementLimitExceeded => {
                "InkScript program statements exceed their limit"
            }
            Self::ParserDiagnosticLimitExceeded => {
                "InkScript parser diagnostic count exceeds its limit"
            }
        }
    }

    const fn hint(self) -> Option<&'static str> {
        match self {
            Self::StandaloneCarriageReturn => Some("use LF or CRLF line endings"),
            Self::InvalidUtf8 => Some("save the source as UTF-8"),
            Self::InvalidNumber => {
                Some("remove leading zeroes and use digits after a decimal point")
            }
            Self::UnterminatedString => Some("close the string before the line ending"),
            Self::UnterminatedBase64 => Some("close the literal with three quotation marks"),
            Self::UnsupportedVersion => Some("use the exact-current InkScript version"),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptDiagnostic {
    code: InkScriptDiagnosticCode,
    severity: InkScriptDiagnosticSeverity,
    source_id: InkScriptSourceId,
    range: InkScriptSourceRange,
    path: Vec<String>,
}

impl InkScriptDiagnostic {
    pub(crate) fn error(
        code: InkScriptDiagnosticCode,
        source_id: InkScriptSourceId,
        range: InkScriptSourceRange,
    ) -> Self {
        Self {
            code,
            severity: InkScriptDiagnosticSeverity::Error,
            source_id,
            range,
            path: Vec::new(),
        }
    }

    /// Returns the stable diagnostic code.
    pub const fn code(&self) -> InkScriptDiagnosticCode {
        self.code
    }

    /// Returns the diagnostic severity.
    pub const fn severity(&self) -> InkScriptDiagnosticSeverity {
        self.severity
    }

    /// Returns the opaque source identity supplied by the caller.
    pub const fn source_id(&self) -> InkScriptSourceId {
        self.source_id
    }

    /// Returns the byte and display range in the original source.
    pub const fn range(&self) -> InkScriptSourceRange {
        self.range
    }

    /// Returns a short locale-independent diagnostic message.
    pub const fn message(&self) -> &'static str {
        self.code.message()
    }

    /// Returns the semantic field path. Lexical diagnostics have an empty path.
    pub fn path(&self) -> &[String] {
        &self.path
    }

    /// Returns a repair hint when one is stable and unambiguous.
    pub const fn hint(&self) -> Option<&'static str> {
        self.code.hint()
    }
}
