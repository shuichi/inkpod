use super::diagnostic::{
    InkScriptDiagnostic, InkScriptDiagnosticCode, InkScriptSourcePosition, InkScriptSourceRange,
    InkScriptSourceSpan,
};
use super::source::{InkScriptLexerLimits, InkScriptSource};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InkScriptKeyword {
    InkScript,
    InkScriptFragment,
    Requires,
    Meta,
    Inputs,
    Parameters,
    Bindings,
    Program,
    Output,
    Execution,
    Assets,
    File,
    Folder,
    CurrentDocument,
    CurrentSequence,
    Param,
    Let,
    Select,
    Assert,
    Step,
    As,
    Enabled,
    Invoke,
    EditorGroup,
    Asset,
    True,
    False,
    None,
    Uuid,
    Blake3,
    Base64,
    List,
    Nullable,
}

impl InkScriptKeyword {
    /// Returns the exact lowercase v1 spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InkScript => "inkscript",
            Self::InkScriptFragment => "inkscript_fragment",
            Self::Requires => "requires",
            Self::Meta => "meta",
            Self::Inputs => "inputs",
            Self::Parameters => "parameters",
            Self::Bindings => "bindings",
            Self::Program => "program",
            Self::Output => "output",
            Self::Execution => "execution",
            Self::Assets => "assets",
            Self::File => "file",
            Self::Folder => "folder",
            Self::CurrentDocument => "current_document",
            Self::CurrentSequence => "current_sequence",
            Self::Param => "param",
            Self::Let => "let",
            Self::Select => "select",
            Self::Assert => "assert",
            Self::Step => "step",
            Self::As => "as",
            Self::Enabled => "enabled",
            Self::Invoke => "invoke",
            Self::EditorGroup => "editor_group",
            Self::Asset => "asset",
            Self::True => "true",
            Self::False => "false",
            Self::None => "none",
            Self::Uuid => "uuid",
            Self::Blake3 => "blake3",
            Self::Base64 => "base64",
            Self::List => "list",
            Self::Nullable => "nullable",
        }
    }

    fn from_word(word: &str) -> Option<Self> {
        Some(match word {
            "inkscript" => Self::InkScript,
            "inkscript_fragment" => Self::InkScriptFragment,
            "requires" => Self::Requires,
            "meta" => Self::Meta,
            "inputs" => Self::Inputs,
            "parameters" => Self::Parameters,
            "bindings" => Self::Bindings,
            "program" => Self::Program,
            "output" => Self::Output,
            "execution" => Self::Execution,
            "assets" => Self::Assets,
            "file" => Self::File,
            "folder" => Self::Folder,
            "current_document" => Self::CurrentDocument,
            "current_sequence" => Self::CurrentSequence,
            "param" => Self::Param,
            "let" => Self::Let,
            "select" => Self::Select,
            "assert" => Self::Assert,
            "step" => Self::Step,
            "as" => Self::As,
            "enabled" => Self::Enabled,
            "invoke" => Self::Invoke,
            "editor_group" => Self::EditorGroup,
            "asset" => Self::Asset,
            "true" => Self::True,
            "false" => Self::False,
            "none" => Self::None,
            "uuid" => Self::Uuid,
            "blake3" => Self::Blake3,
            "base64" => Self::Base64,
            "list" => Self::List,
            "nullable" => Self::Nullable,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InkScriptPunctuation {
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    LeftParenthesis,
    RightParenthesis,
    Semicolon,
    Colon,
    Equals,
    Comma,
    Dollar,
    Dot,
    LessThan,
    GreaterThan,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InkScriptTokenKind {
    Utf8Bom,
    Space,
    Tab,
    Newline,
    LineComment,
    Keyword(InkScriptKeyword),
    Word,
    IntegerLiteral,
    DecimalLiteral,
    StringLiteral,
    UuidLiteral,
    DigestLiteral,
    Base64Literal,
    Punctuation(InkScriptPunctuation),
    Invalid,
    EndOfSource,
}

impl InkScriptTokenKind {
    /// Reports whether this token has no semantic meaning and must be retained only
    /// for the future lossless CST.
    pub const fn is_trivia(self) -> bool {
        matches!(
            self,
            Self::Utf8Bom | Self::Space | Self::Tab | Self::Newline | Self::LineComment
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptToken {
    kind: InkScriptTokenKind,
    range: InkScriptSourceRange,
}

impl InkScriptToken {
    /// Returns the lexical category.
    pub const fn kind(&self) -> InkScriptTokenKind {
        self.kind
    }

    /// Returns the authoritative half-open UTF-8 byte span.
    pub const fn span(&self) -> InkScriptSourceSpan {
        self.range.span()
    }

    /// Returns the original byte and 1-based scalar display range.
    pub const fn range(&self) -> InkScriptSourceRange {
        self.range
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptLexed {
    tokens: Vec<InkScriptToken>,
    diagnostics: Vec<InkScriptDiagnostic>,
    complete: bool,
}

impl InkScriptLexed {
    /// Returns source-order tokens, including trivia and a final sentinel when complete.
    pub fn tokens(&self) -> &[InkScriptToken] {
        &self.tokens
    }

    /// Returns stable source-order lexical diagnostics.
    pub fn diagnostics(&self) -> &[InkScriptDiagnostic] {
        &self.diagnostics
    }

    /// Reports whether the lexer reached end-of-source. Ordinary lexical errors can
    /// recover and remain complete; a resource limit makes this false.
    pub const fn is_complete(&self) -> bool {
        self.complete
    }

    pub(crate) fn into_parts(self) -> (Vec<InkScriptToken>, Vec<InkScriptDiagnostic>, bool) {
        (self.tokens, self.diagnostics, self.complete)
    }
}

/// Tokenizes an immutable source with the exact-current InkScript v1 limits.
///
/// This function has no Core, filesystem, or global state and does not mutate the
/// source. It returns a lossless token stream even when recoverable diagnostics exist.
pub fn lex_inkscript(source: &InkScriptSource) -> InkScriptLexed {
    lex_inkscript_with_limits(source, InkScriptLexerLimits::exact_current())
}

/// Tokenizes with a caller-lowered v1 resource envelope.
pub fn lex_inkscript_with_limits(
    source: &InkScriptSource,
    limits: InkScriptLexerLimits,
) -> InkScriptLexed {
    Lexer::new(source, limits).run()
}

#[derive(Clone, Copy)]
struct Mark {
    offset: usize,
    position: InkScriptSourcePosition,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum StringFlavor {
    Ordinary,
    Uuid,
    Digest,
}

struct Lexer<'a> {
    source: &'a InkScriptSource,
    bytes: &'a [u8],
    limits: InkScriptLexerLimits,
    cursor: usize,
    line: u32,
    column: u32,
    tokens: Vec<InkScriptToken>,
    diagnostics: Vec<InkScriptDiagnostic>,
    stopped: bool,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a InkScriptSource, limits: InkScriptLexerLimits) -> Self {
        Self {
            source,
            bytes: source.bytes(),
            limits,
            cursor: 0,
            line: 1,
            column: 1,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
            stopped: false,
        }
    }

    fn run(mut self) -> InkScriptLexed {
        if self.bytes.len() > self.limits.source_bytes() {
            let start = self.limits.source_bytes();
            let end = start.saturating_add(1).min(self.bytes.len());
            let position = (0..=3)
                .find_map(|back| {
                    start
                        .checked_sub(back)
                        .and_then(|offset| self.source.line_map().position(offset as u64))
                })
                .unwrap_or(InkScriptSourcePosition::new(1, 1));
            self.terminal(
                InkScriptDiagnosticCode::SourceTooLarge,
                InkScriptSourceRange::new(
                    InkScriptSourceSpan::new(start as u64, end as u64),
                    position,
                    position,
                ),
            );
        }
        while self.cursor < self.bytes.len() && !self.stopped {
            if self.tokens.len() >= self.limits.tokens() {
                let mark = self.mark();
                self.terminal(
                    InkScriptDiagnosticCode::TokenLimitExceeded,
                    self.range(mark),
                );
                break;
            }
            self.scan_next();
        }
        if !self.stopped {
            let mark = self.mark();
            self.emit(InkScriptTokenKind::EndOfSource, mark);
        }
        InkScriptLexed {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
            complete: !self.stopped,
        }
    }

    fn scan_next(&mut self) {
        let mark = self.mark();
        if self.cursor == 0 && self.source.has_utf8_bom() {
            self.cursor += 3;
            self.emit(InkScriptTokenKind::Utf8Bom, mark);
            return;
        }

        match self.bytes[self.cursor] {
            b' ' => {
                self.advance_ascii();
                self.emit(InkScriptTokenKind::Space, mark);
            }
            b'\t' => {
                self.advance_ascii();
                self.emit(InkScriptTokenKind::Tab, mark);
            }
            b'\n' => {
                self.advance_newline(1);
                self.emit(InkScriptTokenKind::Newline, mark);
            }
            b'\r' if self.bytes.get(self.cursor + 1) == Some(&b'\n') => {
                self.advance_newline(2);
                self.emit(InkScriptTokenKind::Newline, mark);
            }
            b'\r' => self.invalid_scalar(mark, InkScriptDiagnosticCode::StandaloneCarriageReturn),
            0 => self.invalid_scalar(mark, InkScriptDiagnosticCode::RawNul),
            b'/' if self.bytes.get(self.cursor + 1) == Some(&b'/') => {
                self.scan_line_comment(mark);
            }
            b'a'..=b'z' => self.scan_word(mark),
            b'0'..=b'9' => self.scan_number(mark),
            b'-' if self
                .bytes
                .get(self.cursor + 1)
                .is_some_and(u8::is_ascii_digit) =>
            {
                self.scan_number(mark);
            }
            b'"' => self.scan_string(mark, StringFlavor::Ordinary),
            byte => {
                if let Some(punctuation) = punctuation(byte) {
                    self.advance_ascii();
                    self.emit(InkScriptTokenKind::Punctuation(punctuation), mark);
                } else {
                    self.invalid_scalar(mark, InkScriptDiagnosticCode::UnexpectedCharacter);
                }
            }
        }
    }

    fn scan_line_comment(&mut self, mark: Mark) {
        self.advance_ascii();
        self.advance_ascii();
        while self.cursor < self.bytes.len()
            && !matches!(self.bytes[self.cursor], 0 | b'\r' | b'\n')
        {
            self.advance_scalar();
        }
        self.emit(InkScriptTokenKind::LineComment, mark);
    }

    fn scan_word(&mut self, mark: Mark) {
        while self
            .bytes
            .get(self.cursor)
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
        {
            self.advance_ascii();
            if self.cursor - mark.offset > self.limits.identifier_bytes() {
                self.terminal(InkScriptDiagnosticCode::IdentifierTooLong, self.range(mark));
                return;
            }
        }

        let word = self
            .source
            .text()
            .get(mark.offset..self.cursor)
            .unwrap_or("");
        if self.bytes.get(self.cursor) == Some(&b'"') {
            match word {
                "uuid" => {
                    self.scan_string(mark, StringFlavor::Uuid);
                    return;
                }
                "blake3" => {
                    self.scan_string(mark, StringFlavor::Digest);
                    return;
                }
                "base64" if self.bytes.get(self.cursor..self.cursor + 3) == Some(b"\"\"\"") => {
                    self.scan_base64(mark);
                    return;
                }
                _ => {}
            }
        }

        let kind = InkScriptKeyword::from_word(word)
            .map(InkScriptTokenKind::Keyword)
            .unwrap_or(InkScriptTokenKind::Word);
        self.emit(kind, mark);
    }

    fn scan_number(&mut self, mark: Mark) {
        if self.bytes[self.cursor] == b'-' {
            self.advance_ascii();
            if !self.check_numeric_length(mark) {
                return;
            }
        }
        let integer_start = self.cursor;
        while self.bytes.get(self.cursor).is_some_and(u8::is_ascii_digit) {
            self.advance_ascii();
            if !self.check_numeric_length(mark) {
                return;
            }
        }
        let integer_length = self.cursor - integer_start;
        let leading_zero = integer_length > 1 && self.bytes[integer_start] == b'0';
        let mut decimal = false;
        let mut fraction_length = 0;
        if self.bytes.get(self.cursor) == Some(&b'.') {
            decimal = true;
            self.advance_ascii();
            if !self.check_numeric_length(mark) {
                return;
            }
            while self.bytes.get(self.cursor).is_some_and(u8::is_ascii_digit) {
                fraction_length += 1;
                self.advance_ascii();
                if !self.check_numeric_length(mark) {
                    return;
                }
            }
        }

        if leading_zero || (decimal && fraction_length == 0) {
            if self.recoverable(InkScriptDiagnosticCode::InvalidNumber, self.range(mark)) {
                self.emit(InkScriptTokenKind::Invalid, mark);
            }
            return;
        }
        self.emit(
            if decimal {
                InkScriptTokenKind::DecimalLiteral
            } else {
                InkScriptTokenKind::IntegerLiteral
            },
            mark,
        );
    }

    fn check_numeric_length(&mut self, mark: Mark) -> bool {
        if self.cursor - mark.offset > self.limits.numeric_bytes() {
            self.terminal(
                InkScriptDiagnosticCode::NumericLiteralTooLong,
                self.range(mark),
            );
            false
        } else {
            true
        }
    }

    fn scan_string(&mut self, mark: Mark, flavor: StringFlavor) {
        self.advance_ascii();
        let mut decoded_bytes = 0_usize;
        let mut captured = Vec::new();
        let mut used_escape = false;
        let mut valid = true;

        loop {
            if self.cursor >= self.bytes.len() {
                if self.recoverable(
                    InkScriptDiagnosticCode::UnterminatedString,
                    self.range(mark),
                ) {
                    self.emit(InkScriptTokenKind::Invalid, mark);
                }
                return;
            }
            match self.bytes[self.cursor] {
                b'"' => {
                    self.advance_ascii();
                    break;
                }
                b'\n' | b'\r' => {
                    if self.recoverable(
                        InkScriptDiagnosticCode::UnterminatedString,
                        self.range(mark),
                    ) {
                        self.emit(InkScriptTokenKind::Invalid, mark);
                    }
                    return;
                }
                0 => {
                    let error_mark = self.mark();
                    self.advance_ascii();
                    if !self.recoverable(InkScriptDiagnosticCode::RawNul, self.range(error_mark)) {
                        return;
                    }
                    valid = false;
                }
                b'\\' => {
                    let Some(escape_valid) =
                        self.scan_escape(mark, flavor, &mut decoded_bytes, &mut captured)
                    else {
                        return;
                    };
                    used_escape = true;
                    valid &= escape_valid;
                }
                byte if byte <= 0x1f => {
                    let error_mark = self.mark();
                    self.advance_ascii();
                    if !self.recoverable(
                        InkScriptDiagnosticCode::InvalidStringCharacter,
                        self.range(error_mark),
                    ) {
                        return;
                    }
                    valid = false;
                }
                _ => {
                    let character = self.advance_scalar();
                    if !self.add_decoded_bytes(mark, &mut decoded_bytes, character.len_utf8()) {
                        return;
                    }
                    if flavor != StringFlavor::Ordinary {
                        let mut buffer = [0_u8; 4];
                        captured.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
                    }
                }
            }
        }

        let kind = match flavor {
            StringFlavor::Ordinary => InkScriptTokenKind::StringLiteral,
            StringFlavor::Uuid => {
                if used_escape || !is_canonical_uuid(&captured) {
                    if !self.recoverable(
                        InkScriptDiagnosticCode::InvalidUuidLiteral,
                        self.range(mark),
                    ) {
                        return;
                    }
                    valid = false;
                }
                InkScriptTokenKind::UuidLiteral
            }
            StringFlavor::Digest => {
                if used_escape || !is_canonical_digest(&captured) {
                    if !self.recoverable(
                        InkScriptDiagnosticCode::InvalidDigestLiteral,
                        self.range(mark),
                    ) {
                        return;
                    }
                    valid = false;
                }
                InkScriptTokenKind::DigestLiteral
            }
        };
        self.emit(
            if valid {
                kind
            } else {
                InkScriptTokenKind::Invalid
            },
            mark,
        );
    }

    fn scan_escape(
        &mut self,
        string_mark: Mark,
        flavor: StringFlavor,
        decoded_bytes: &mut usize,
        captured: &mut Vec<u8>,
    ) -> Option<bool> {
        let escape_mark = self.mark();
        self.advance_ascii();
        if self.cursor >= self.bytes.len() {
            if self.recoverable(
                InkScriptDiagnosticCode::UnterminatedString,
                self.range(string_mark),
            ) {
                self.emit(InkScriptTokenKind::Invalid, string_mark);
            }
            return None;
        }
        match self.bytes[self.cursor] {
            b'"' | b'\\' | b'n' | b'r' | b't' => {
                let escaped = self.bytes[self.cursor];
                self.advance_ascii();
                if !self.add_decoded_bytes(string_mark, decoded_bytes, 1) {
                    return None;
                }
                if flavor != StringFlavor::Ordinary {
                    captured.push(match escaped {
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        value => value,
                    });
                }
            }
            b'u' => {
                self.advance_ascii();
                if self.bytes.get(self.cursor) != Some(&b'{') {
                    return self.invalid_escape(
                        InkScriptDiagnosticCode::InvalidEscape,
                        self.range(escape_mark),
                    );
                }
                self.advance_ascii();
                let mut value = 0_u32;
                let mut digits = 0_u32;
                while digits < 7
                    && self
                        .bytes
                        .get(self.cursor)
                        .is_some_and(u8::is_ascii_hexdigit)
                {
                    value = value
                        .saturating_mul(16)
                        .saturating_add(hex_value(self.bytes[self.cursor]) as u32);
                    digits += 1;
                    self.advance_ascii();
                }
                let closed = self.bytes.get(self.cursor) == Some(&b'}');
                if closed {
                    self.advance_ascii();
                }
                let scalar = if (1..=6).contains(&digits) && closed {
                    char::from_u32(value).filter(|character| *character != '\0')
                } else {
                    None
                };
                let Some(scalar) = scalar else {
                    return self.invalid_escape(
                        InkScriptDiagnosticCode::InvalidUnicodeEscape,
                        self.range(escape_mark),
                    );
                };
                if !self.add_decoded_bytes(string_mark, decoded_bytes, scalar.len_utf8()) {
                    return None;
                }
                if flavor != StringFlavor::Ordinary {
                    let mut buffer = [0_u8; 4];
                    captured.extend_from_slice(scalar.encode_utf8(&mut buffer).as_bytes());
                }
            }
            _ => {
                self.advance_scalar();
                return self.invalid_escape(
                    InkScriptDiagnosticCode::InvalidEscape,
                    self.range(escape_mark),
                );
            }
        }
        Some(true)
    }

    fn invalid_escape(
        &mut self,
        code: InkScriptDiagnosticCode,
        range: InkScriptSourceRange,
    ) -> Option<bool> {
        self.recoverable(code, range).then_some(false)
    }

    fn add_decoded_bytes(&mut self, mark: Mark, total: &mut usize, amount: usize) -> bool {
        let Some(next) = total.checked_add(amount) else {
            self.terminal(InkScriptDiagnosticCode::StringTooLong, self.range(mark));
            return false;
        };
        if next > self.limits.string_bytes() {
            self.terminal(InkScriptDiagnosticCode::StringTooLong, self.range(mark));
            return false;
        }
        *total = next;
        true
    }

    fn scan_base64(&mut self, mark: Mark) {
        self.advance_ascii();
        self.advance_ascii();
        self.advance_ascii();
        let mut significant = 0_u64;
        let mut padding = 0_u8;
        let mut saw_padding = false;
        let mut invalid_encoding = false;
        let mut syntactically_valid = true;
        let mut last = [0_u8; 4];

        loop {
            if self.cursor >= self.bytes.len() {
                if self.recoverable(
                    InkScriptDiagnosticCode::UnterminatedBase64,
                    self.range(mark),
                ) {
                    self.emit(InkScriptTokenKind::Invalid, mark);
                }
                return;
            }
            if self.bytes.get(self.cursor..self.cursor + 3) == Some(b"\"\"\"") {
                self.advance_ascii();
                self.advance_ascii();
                self.advance_ascii();
                break;
            }
            match self.bytes[self.cursor] {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' => {
                    if saw_padding {
                        invalid_encoding = true;
                    }
                    let byte = self.bytes[self.cursor];
                    self.advance_ascii();
                    if !self.record_base64_byte(mark, byte, &mut significant, &mut last) {
                        return;
                    }
                }
                b'=' => {
                    saw_padding = true;
                    padding = padding.saturating_add(1);
                    self.advance_ascii();
                    if !self.record_base64_byte(mark, b'=', &mut significant, &mut last) {
                        return;
                    }
                }
                b' ' | b'\t' => {
                    self.advance_ascii();
                }
                b'\n' => self.advance_newline(1),
                b'\r' if self.bytes.get(self.cursor + 1) == Some(&b'\n') => {
                    self.advance_newline(2);
                }
                b'\r' => {
                    let error_mark = self.mark();
                    self.advance_scalar();
                    if !self.recoverable(
                        InkScriptDiagnosticCode::StandaloneCarriageReturn,
                        self.range(error_mark),
                    ) {
                        return;
                    }
                    syntactically_valid = false;
                }
                0 => {
                    let error_mark = self.mark();
                    self.advance_ascii();
                    if !self.recoverable(InkScriptDiagnosticCode::RawNul, self.range(error_mark)) {
                        return;
                    }
                    syntactically_valid = false;
                }
                _ => {
                    let error_mark = self.mark();
                    self.advance_scalar();
                    if !self.recoverable(
                        InkScriptDiagnosticCode::InvalidBase64Character,
                        self.range(error_mark),
                    ) {
                        return;
                    }
                    syntactically_valid = false;
                }
            }
        }

        if syntactically_valid
            && (invalid_encoding || !valid_base64_ending(significant, padding, last))
        {
            if !self.recoverable(
                InkScriptDiagnosticCode::InvalidBase64Encoding,
                self.range(mark),
            ) {
                return;
            }
            syntactically_valid = false;
        }
        let decoded = significant
            .checked_div(4)
            .and_then(|groups| groups.checked_mul(3))
            .and_then(|bytes| bytes.checked_sub(u64::from(padding)));
        if decoded.is_some_and(|bytes| bytes > self.limits.inline_asset_bytes() as u64) {
            self.terminal(
                InkScriptDiagnosticCode::InlineAssetTooLarge,
                self.range(mark),
            );
            return;
        }
        self.emit(
            if syntactically_valid {
                InkScriptTokenKind::Base64Literal
            } else {
                InkScriptTokenKind::Invalid
            },
            mark,
        );
    }

    fn record_base64_byte(
        &mut self,
        mark: Mark,
        byte: u8,
        significant: &mut u64,
        last: &mut [u8; 4],
    ) -> bool {
        let Some(next) = significant.checked_add(1) else {
            self.terminal(
                InkScriptDiagnosticCode::InlineAssetTooLarge,
                self.range(mark),
            );
            return false;
        };
        last[*significant as usize % 4] = byte;
        *significant = next;
        let minimum_decoded = significant
            .checked_div(4)
            .and_then(|groups| groups.checked_mul(3))
            .unwrap_or(u64::MAX)
            .saturating_sub(2);
        if minimum_decoded > self.limits.inline_asset_bytes() as u64 {
            self.terminal(
                InkScriptDiagnosticCode::InlineAssetTooLarge,
                self.range(mark),
            );
            return false;
        }
        true
    }

    fn invalid_scalar(&mut self, mark: Mark, code: InkScriptDiagnosticCode) {
        self.advance_scalar();
        if self.recoverable(code, self.range(mark)) {
            self.emit(InkScriptTokenKind::Invalid, mark);
        }
    }

    fn emit(&mut self, kind: InkScriptTokenKind, mark: Mark) {
        self.tokens.push(InkScriptToken {
            kind,
            range: self.range(mark),
        });
    }

    fn recoverable(&mut self, code: InkScriptDiagnosticCode, range: InkScriptSourceRange) -> bool {
        if self.diagnostics.len() + 1 >= self.limits.diagnostics() {
            self.terminal(InkScriptDiagnosticCode::DiagnosticLimitExceeded, range);
            return false;
        }
        self.diagnostics
            .push(InkScriptDiagnostic::error(code, self.source.id(), range));
        true
    }

    fn terminal(&mut self, code: InkScriptDiagnosticCode, range: InkScriptSourceRange) {
        let diagnostic = InkScriptDiagnostic::error(code, self.source.id(), range);
        if self.diagnostics.len() < self.limits.diagnostics() {
            self.diagnostics.push(diagnostic);
        } else if let Some(last) = self.diagnostics.last_mut() {
            *last = diagnostic;
        }
        self.stopped = true;
    }

    fn mark(&self) -> Mark {
        Mark {
            offset: self.cursor,
            position: InkScriptSourcePosition::new(self.line, self.column),
        }
    }

    fn range(&self, mark: Mark) -> InkScriptSourceRange {
        InkScriptSourceRange::new(
            InkScriptSourceSpan::new(mark.offset as u64, self.cursor as u64),
            mark.position,
            InkScriptSourcePosition::new(self.line, self.column),
        )
    }

    fn advance_ascii(&mut self) {
        self.cursor += 1;
        self.column += 1;
    }

    fn advance_newline(&mut self, bytes: usize) {
        self.cursor += bytes;
        self.line += 1;
        self.column = 1;
    }

    fn advance_scalar(&mut self) -> char {
        let character = self
            .source
            .text()
            .get(self.cursor..)
            .and_then(|text| text.chars().next())
            .expect("lexer cursor remains on a UTF-8 boundary");
        self.cursor += character.len_utf8();
        self.column += 1;
        character
    }
}

fn punctuation(byte: u8) -> Option<InkScriptPunctuation> {
    Some(match byte {
        b'{' => InkScriptPunctuation::LeftBrace,
        b'}' => InkScriptPunctuation::RightBrace,
        b'[' => InkScriptPunctuation::LeftBracket,
        b']' => InkScriptPunctuation::RightBracket,
        b'(' => InkScriptPunctuation::LeftParenthesis,
        b')' => InkScriptPunctuation::RightParenthesis,
        b';' => InkScriptPunctuation::Semicolon,
        b':' => InkScriptPunctuation::Colon,
        b'=' => InkScriptPunctuation::Equals,
        b',' => InkScriptPunctuation::Comma,
        b'$' => InkScriptPunctuation::Dollar,
        b'.' => InkScriptPunctuation::Dot,
        b'<' => InkScriptPunctuation::LessThan,
        b'>' => InkScriptPunctuation::GreaterThan,
        _ => return None,
    })
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

fn is_canonical_uuid(bytes: &[u8]) -> bool {
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                *byte == b'-'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(byte)
            }
        })
}

fn is_canonical_digest(bytes: &[u8]) -> bool {
    bytes.len() == 64
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

fn valid_base64_ending(significant: u64, padding: u8, last: [u8; 4]) -> bool {
    if significant % 4 != 0 || padding > 2 {
        return false;
    }
    match padding {
        0 => !last.contains(&b'='),
        1 => {
            last[3] == b'='
                && last[2] != b'='
                && base64_value(last[2]).is_some_and(|value| value & 0x03 == 0)
        }
        2 => {
            last[2] == b'='
                && last[3] == b'='
                && base64_value(last[1]).is_some_and(|value| value & 0x0f == 0)
        }
        _ => false,
    }
}

fn base64_value(byte: u8) -> Option<u8> {
    Some(match byte {
        b'A'..=b'Z' => byte - b'A',
        b'a'..=b'z' => byte - b'a' + 26,
        b'0'..=b'9' => byte - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    })
}
