use super::diagnostic::{
    InkScriptDiagnostic, InkScriptDiagnosticCode, InkScriptSourceId, InkScriptSourcePosition,
    InkScriptSourceRange, InkScriptSourceSpan,
};

pub const INKSCRIPT_FILE_VERSION: u32 = 2;
pub const MAX_INKSCRIPT_SOURCE_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_INKSCRIPT_IDENTIFIER_BYTES: usize = 128;
pub const MAX_INKSCRIPT_NUMERIC_BYTES: usize = 128;
pub const MAX_INKSCRIPT_TOKENS: usize = 4_194_304;
pub const MAX_INKSCRIPT_DIAGNOSTICS: usize = 256;
pub const MAX_INKSCRIPT_STRING_BYTES: usize = 32 * 1024;
pub const MAX_INKSCRIPT_INLINE_ASSET_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_INKSCRIPT_INLINE_ASSET_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_INKSCRIPT_EXTERNAL_ASSET_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_INKSCRIPT_ASSET_TOTAL_BYTES: u64 = 768 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptLexerLimits {
    source_bytes: usize,
    identifier_bytes: usize,
    numeric_bytes: usize,
    tokens: usize,
    diagnostics: usize,
    string_bytes: usize,
    inline_asset_bytes: usize,
}

impl InkScriptLexerLimits {
    /// Returns the exact-current InkScript v2 resource limits.
    pub const fn exact_current() -> Self {
        Self {
            source_bytes: MAX_INKSCRIPT_SOURCE_BYTES,
            identifier_bytes: MAX_INKSCRIPT_IDENTIFIER_BYTES,
            numeric_bytes: MAX_INKSCRIPT_NUMERIC_BYTES,
            tokens: MAX_INKSCRIPT_TOKENS,
            diagnostics: MAX_INKSCRIPT_DIAGNOSTICS,
            string_bytes: MAX_INKSCRIPT_STRING_BYTES,
            inline_asset_bytes: MAX_INKSCRIPT_INLINE_ASSET_BYTES,
        }
    }

    /// Applies a smaller source-byte cap. Values above v2 are clamped to v2.
    pub const fn with_source_byte_limit(mut self, maximum: usize) -> Self {
        self.source_bytes = minimum_nonzero(maximum, MAX_INKSCRIPT_SOURCE_BYTES);
        self
    }

    /// Applies a smaller identifier/keyword cap. Values above v2 are clamped to v2.
    pub const fn with_identifier_byte_limit(mut self, maximum: usize) -> Self {
        self.identifier_bytes = minimum_nonzero(maximum, MAX_INKSCRIPT_IDENTIFIER_BYTES);
        self
    }

    /// Applies a smaller numeric-literal cap. Values above v2 are clamped to v2.
    pub const fn with_numeric_byte_limit(mut self, maximum: usize) -> Self {
        self.numeric_bytes = minimum_nonzero(maximum, MAX_INKSCRIPT_NUMERIC_BYTES);
        self
    }

    /// Applies a smaller token cap. The end-of-source sentinel is not counted.
    pub const fn with_token_limit(mut self, maximum: usize) -> Self {
        self.tokens = minimum_nonzero(maximum, MAX_INKSCRIPT_TOKENS);
        self
    }

    /// Applies a smaller diagnostic cap, retaining room for a terminal limit diagnostic.
    pub const fn with_diagnostic_limit(mut self, maximum: usize) -> Self {
        self.diagnostics = if maximum < 2 {
            2
        } else if maximum < MAX_INKSCRIPT_DIAGNOSTICS {
            maximum
        } else {
            MAX_INKSCRIPT_DIAGNOSTICS
        };
        self
    }

    /// Applies a smaller decoded-string byte cap. Values above v2 are clamped to v2.
    pub const fn with_string_byte_limit(mut self, maximum: usize) -> Self {
        self.string_bytes = minimum_nonzero(maximum, MAX_INKSCRIPT_STRING_BYTES);
        self
    }

    /// Applies a smaller decoded inline-asset cap. Values above v2 are clamped to v2.
    pub const fn with_inline_asset_byte_limit(mut self, maximum: usize) -> Self {
        self.inline_asset_bytes = minimum_nonzero(maximum, MAX_INKSCRIPT_INLINE_ASSET_BYTES);
        self
    }

    pub(crate) const fn source_bytes(self) -> usize {
        self.source_bytes
    }

    pub(crate) const fn identifier_bytes(self) -> usize {
        self.identifier_bytes
    }

    pub(crate) const fn numeric_bytes(self) -> usize {
        self.numeric_bytes
    }

    pub(crate) const fn tokens(self) -> usize {
        self.tokens
    }

    pub(crate) const fn diagnostics(self) -> usize {
        self.diagnostics
    }

    pub(crate) const fn string_bytes(self) -> usize {
        self.string_bytes
    }

    pub(crate) const fn inline_asset_bytes(self) -> usize {
        self.inline_asset_bytes
    }
}

impl Default for InkScriptLexerLimits {
    fn default() -> Self {
        Self::exact_current()
    }
}

const fn minimum_nonzero(requested: usize, exact: usize) -> usize {
    if requested == 0 {
        1
    } else if requested < exact {
        requested
    } else {
        exact
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptSource {
    id: InkScriptSourceId,
    bytes: Box<[u8]>,
    has_utf8_bom: bool,
}

impl InkScriptSource {
    /// Validates and takes an owned copy of a UTF-8 source under exact-current limits.
    ///
    /// Failure performs no partial publication. NUL and newline rules are lexical and
    /// are reported by [`crate::lex_inkscript`].
    pub fn new(id: InkScriptSourceId, bytes: &[u8]) -> Result<Self, InkScriptDiagnostic> {
        Self::with_limits(id, bytes, InkScriptLexerLimits::exact_current())
    }

    /// Validates and copies a source under a caller-lowered v2 resource envelope.
    pub fn with_limits(
        id: InkScriptSourceId,
        bytes: &[u8],
        limits: InkScriptLexerLimits,
    ) -> Result<Self, InkScriptDiagnostic> {
        if bytes.len() > limits.source_bytes() {
            let start = limits.source_bytes();
            let end = start.saturating_add(1).min(bytes.len());
            let position = position_in_valid_prefix(&bytes[..start], starts_with_bom(bytes));
            return Err(InkScriptDiagnostic::error(
                InkScriptDiagnosticCode::SourceTooLarge,
                id,
                InkScriptSourceRange::new(
                    InkScriptSourceSpan::new(start as u64, end as u64),
                    position,
                    position,
                ),
            ));
        }

        if let Err(error) = std::str::from_utf8(bytes) {
            let start = error.valid_up_to();
            let length = error.error_len().unwrap_or(1);
            let end = start.saturating_add(length).min(bytes.len());
            let position = position_in_valid_prefix(&bytes[..start], starts_with_bom(bytes));
            return Err(InkScriptDiagnostic::error(
                InkScriptDiagnosticCode::InvalidUtf8,
                id,
                InkScriptSourceRange::new(
                    InkScriptSourceSpan::new(start as u64, end as u64),
                    position,
                    position,
                ),
            ));
        }

        Ok(Self {
            id,
            bytes: bytes.to_vec().into_boxed_slice(),
            has_utf8_bom: starts_with_bom(bytes),
        })
    }

    /// Returns the opaque identity attached by the caller.
    pub const fn id(&self) -> InkScriptSourceId {
        self.id
    }

    /// Returns the unchanged caller byte sequence owned by this source.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the unchanged validated UTF-8 source.
    pub fn text(&self) -> &str {
        std::str::from_utf8(&self.bytes).expect("InkScriptSource preserves valid UTF-8")
    }

    /// Reports whether byte offset zero contains the permitted UTF-8 BOM.
    pub const fn has_utf8_bom(&self) -> bool {
        self.has_utf8_bom
    }

    /// Returns a non-owning byte-to-Unicode-scalar position mapper.
    pub const fn line_map(&self) -> InkScriptLineMap<'_> {
        InkScriptLineMap { source: self }
    }

    /// Returns text for a valid UTF-8-boundary span in this source.
    pub fn slice(&self, span: InkScriptSourceSpan) -> Option<&str> {
        let start = usize::try_from(span.start()).ok()?;
        let end = usize::try_from(span.end()).ok()?;
        self.text().get(start..end)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InkScriptLineMap<'a> {
    source: &'a InkScriptSource,
}

impl InkScriptLineMap<'_> {
    /// Maps a UTF-8 byte boundary to a 1-based line and Unicode-scalar column.
    ///
    /// The leading BOM consumes no column and CRLF consumes one line break. An
    /// offset inside a multi-byte scalar, BOM, or CRLF pair returns `None`.
    pub fn position(self, byte_offset: u64) -> Option<InkScriptSourcePosition> {
        let offset = usize::try_from(byte_offset).ok()?;
        position_in_source(self.source.bytes(), self.source.has_utf8_bom(), offset)
    }

    /// Maps both boundaries of a valid half-open byte span.
    pub fn range(self, span: InkScriptSourceSpan) -> Option<InkScriptSourceRange> {
        if span.start() > span.end() {
            return None;
        }
        Some(InkScriptSourceRange::new(
            span,
            self.position(span.start())?,
            self.position(span.end())?,
        ))
    }
}

fn starts_with_bom(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xef, 0xbb, 0xbf])
}

fn position_in_valid_prefix(bytes: &[u8], has_utf8_bom: bool) -> InkScriptSourcePosition {
    position_in_source(bytes, has_utf8_bom && bytes.len() >= 3, bytes.len())
        .unwrap_or(InkScriptSourcePosition::new(1, 1))
}

fn position_in_source(
    bytes: &[u8],
    has_utf8_bom: bool,
    offset: usize,
) -> Option<InkScriptSourcePosition> {
    if offset > bytes.len() {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    if !text.is_char_boundary(offset) {
        return None;
    }
    let mut cursor = 0;
    if has_utf8_bom {
        if offset == 0 {
            return Some(InkScriptSourcePosition::new(1, 1));
        }
        if offset < 3 {
            return None;
        }
        cursor = 3;
    }
    let mut line = 1_u32;
    let mut column = 1_u32;
    while cursor < offset {
        if bytes.get(cursor..cursor + 2) == Some(b"\r\n") {
            if cursor + 1 == offset {
                return None;
            }
            cursor += 2;
            line = line.checked_add(1)?;
            column = 1;
            continue;
        }
        let character = text.get(cursor..)?.chars().next()?;
        cursor = cursor.checked_add(character.len_utf8())?;
        if character == '\n' {
            line = line.checked_add(1)?;
            column = 1;
        } else {
            column = column.checked_add(1)?;
        }
    }
    Some(InkScriptSourcePosition::new(line, column))
}
