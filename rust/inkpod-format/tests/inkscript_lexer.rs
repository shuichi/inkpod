use inkpod_format::{
    INKSCRIPT_FILE_VERSION, InkScriptDiagnosticCode, InkScriptKeyword, InkScriptLexerLimits,
    InkScriptPunctuation, InkScriptSource, InkScriptSourceId, InkScriptSourcePosition,
    InkScriptSourceSpan, InkScriptTokenKind, MAX_INKSCRIPT_DIAGNOSTICS,
    MAX_INKSCRIPT_IDENTIFIER_BYTES, MAX_INKSCRIPT_INLINE_ASSET_BYTES, MAX_INKSCRIPT_NUMERIC_BYTES,
    MAX_INKSCRIPT_SOURCE_BYTES, MAX_INKSCRIPT_STRING_BYTES, MAX_INKSCRIPT_TOKENS, lex_inkscript,
    lex_inkscript_with_limits,
};

fn fixture_source(bytes: &[u8]) -> InkScriptSource {
    InkScriptSource::new(InkScriptSourceId::new(7), bytes).expect("fixture must be valid UTF-8")
}

fn significant_kinds(source: &InkScriptSource) -> Vec<InkScriptTokenKind> {
    lex_inkscript(source)
        .tokens()
        .iter()
        .filter(|token| !token.kind().is_trivia())
        .map(|token| token.kind())
        .collect()
}

fn diagnostic_codes(bytes: &[u8]) -> Vec<InkScriptDiagnosticCode> {
    let source = fixture_source(bytes);
    lex_inkscript(&source)
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code())
        .collect()
}

#[test]
fn public_lexer_accepts_v1_tokens_and_uses_maximal_munch() {
    let source = fixture_source(
        br#"inkscript 1; uuid"550e8400-e29b-41d4-a716-446655440000" blake3"0000000000000000000000000000000000000000000000000000000000000000" base64"""QUJD""" uuid "plain" 1.25 -0 // tail
"#,
    );
    let lexed = lex_inkscript(&source);
    assert!(lexed.is_complete());
    assert!(lexed.diagnostics().is_empty());
    assert_eq!(
        significant_kinds(&source),
        vec![
            InkScriptTokenKind::Keyword(InkScriptKeyword::InkScript),
            InkScriptTokenKind::IntegerLiteral,
            InkScriptTokenKind::Punctuation(InkScriptPunctuation::Semicolon),
            InkScriptTokenKind::UuidLiteral,
            InkScriptTokenKind::DigestLiteral,
            InkScriptTokenKind::Base64Literal,
            InkScriptTokenKind::Keyword(InkScriptKeyword::Uuid),
            InkScriptTokenKind::StringLiteral,
            InkScriptTokenKind::DecimalLiteral,
            InkScriptTokenKind::IntegerLiteral,
            InkScriptTokenKind::EndOfSource,
        ]
    );
    assert!(lexed.tokens().iter().any(|token| {
        token.kind() == InkScriptTokenKind::LineComment
            && source.slice(token.span()) == Some("// tail")
    }));
}

#[test]
fn public_lexer_exposes_the_exact_current_v1_keywords_limits_and_codes() {
    assert_eq!(INKSCRIPT_FILE_VERSION, 1);
    assert_eq!(MAX_INKSCRIPT_SOURCE_BYTES, 128 * 1024 * 1024);
    assert_eq!(MAX_INKSCRIPT_IDENTIFIER_BYTES, 128);
    assert_eq!(MAX_INKSCRIPT_NUMERIC_BYTES, 128);
    assert_eq!(MAX_INKSCRIPT_TOKENS, 4_194_304);
    assert_eq!(MAX_INKSCRIPT_DIAGNOSTICS, 256);
    assert_eq!(MAX_INKSCRIPT_STRING_BYTES, 32 * 1024);
    assert_eq!(MAX_INKSCRIPT_INLINE_ASSET_BYTES, 32 * 1024 * 1024);

    let spellings = [
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
    let text = spellings.join(" ");
    let source = fixture_source(text.as_bytes());
    let lexed = lex_inkscript(&source);
    let keywords = lexed
        .tokens()
        .iter()
        .filter_map(|token| match token.kind() {
            InkScriptTokenKind::Keyword(keyword) => Some(keyword.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(keywords, spellings);

    let codes = [
        InkScriptDiagnosticCode::SourceTooLarge,
        InkScriptDiagnosticCode::InvalidUtf8,
        InkScriptDiagnosticCode::RawNul,
        InkScriptDiagnosticCode::StandaloneCarriageReturn,
        InkScriptDiagnosticCode::UnexpectedCharacter,
        InkScriptDiagnosticCode::IdentifierTooLong,
        InkScriptDiagnosticCode::NumericLiteralTooLong,
        InkScriptDiagnosticCode::InvalidNumber,
        InkScriptDiagnosticCode::UnterminatedString,
        InkScriptDiagnosticCode::InvalidStringCharacter,
        InkScriptDiagnosticCode::InvalidEscape,
        InkScriptDiagnosticCode::InvalidUnicodeEscape,
        InkScriptDiagnosticCode::StringTooLong,
        InkScriptDiagnosticCode::InvalidUuidLiteral,
        InkScriptDiagnosticCode::InvalidDigestLiteral,
        InkScriptDiagnosticCode::UnterminatedBase64,
        InkScriptDiagnosticCode::InvalidBase64Character,
        InkScriptDiagnosticCode::InvalidBase64Encoding,
        InkScriptDiagnosticCode::InlineAssetTooLarge,
        InkScriptDiagnosticCode::TokenLimitExceeded,
        InkScriptDiagnosticCode::DiagnosticLimitExceeded,
    ];
    for (index, code) in codes.into_iter().enumerate() {
        assert_eq!(code.as_str(), format!("INKS-LEX-{:04}", index + 1));
    }
}

#[test]
fn public_source_preserves_bom_crlf_and_unicode_scalar_positions() {
    let source = fixture_source(b"\xef\xbb\xbfinkscript\r\n// \xc3\xa9\r\ntrue");
    assert!(source.has_utf8_bom());
    assert_eq!(
        source.bytes(),
        b"\xef\xbb\xbfinkscript\r\n// \xc3\xa9\r\ntrue"
    );
    let lexed = lex_inkscript(&source);
    assert!(lexed.diagnostics().is_empty());

    let bom = &lexed.tokens()[0];
    assert_eq!(bom.kind(), InkScriptTokenKind::Utf8Bom);
    assert_eq!(bom.span(), InkScriptSourceSpan::new(0, 3));
    assert_eq!(bom.range().start(), InkScriptSourcePosition::new(1, 1));
    assert_eq!(bom.range().end(), InkScriptSourcePosition::new(1, 1));

    let comment = lexed
        .tokens()
        .iter()
        .find(|token| token.kind() == InkScriptTokenKind::LineComment)
        .expect("comment token must exist");
    assert_eq!(comment.range().start(), InkScriptSourcePosition::new(2, 1));
    assert_eq!(comment.range().end(), InkScriptSourcePosition::new(2, 5));
    assert_eq!(
        source.line_map().position(comment.span().end()),
        Some(InkScriptSourcePosition::new(2, 5))
    );

    let true_token = lexed
        .tokens()
        .iter()
        .find(|token| token.kind() == InkScriptTokenKind::Keyword(InkScriptKeyword::True))
        .expect("true token must exist");
    assert_eq!(
        true_token.range().start(),
        InkScriptSourcePosition::new(3, 1)
    );
}

#[test]
fn public_source_rejects_invalid_utf8_and_source_overflow_before_copying() {
    let error = InkScriptSource::new(InkScriptSourceId::new(11), b"ok\xf0\x28\x8c\x28")
        .expect_err("invalid UTF-8 must be rejected");
    assert_eq!(error.code(), InkScriptDiagnosticCode::InvalidUtf8);
    assert_eq!(error.source_id(), InkScriptSourceId::new(11));
    assert_eq!(error.range().span(), InkScriptSourceSpan::new(2, 3));

    let limits = InkScriptLexerLimits::exact_current().with_source_byte_limit(4);
    let error = InkScriptSource::with_limits(InkScriptSourceId::new(12), b"abcde", limits)
        .expect_err("source limit must be checked before ownership is taken");
    assert_eq!(error.code(), InkScriptDiagnosticCode::SourceTooLarge);
    assert_eq!(error.range().span(), InkScriptSourceSpan::new(4, 5));

    let source = fixture_source(b"abcde");
    let lexed = lex_inkscript_with_limits(&source, limits);
    assert!(!lexed.is_complete());
    assert!(lexed.tokens().is_empty());
    assert_eq!(
        lexed.diagnostics()[0].code(),
        InkScriptDiagnosticCode::SourceTooLarge
    );
}

#[test]
fn public_lexer_reports_nul_cr_escape_and_number_errors_then_recovers() {
    let source = fixture_source(b"\0\r \"bad\\q\" \"bad\\u{d800}\" 01 true");
    let lexed = lex_inkscript(&source);
    assert!(lexed.is_complete());
    assert_eq!(
        lexed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code())
            .collect::<Vec<_>>(),
        vec![
            InkScriptDiagnosticCode::RawNul,
            InkScriptDiagnosticCode::StandaloneCarriageReturn,
            InkScriptDiagnosticCode::InvalidEscape,
            InkScriptDiagnosticCode::InvalidUnicodeEscape,
            InkScriptDiagnosticCode::InvalidNumber,
        ]
    );
    assert!(
        lexed
            .tokens()
            .iter()
            .any(|token| { token.kind() == InkScriptTokenKind::Keyword(InkScriptKeyword::True) })
    );
}

#[test]
fn public_lexer_accepts_v1_escapes_and_treats_nonleading_bom_as_source_text() {
    let source = fixture_source(br#""\"\\\n\r\t\u{e9}\u{1F600}" base64"""//8=""""#);
    let lexed = lex_inkscript(&source);
    assert!(lexed.is_complete());
    assert!(lexed.diagnostics().is_empty());
    assert!(
        lexed
            .tokens()
            .iter()
            .any(|token| token.kind() == InkScriptTokenKind::StringLiteral)
    );
    assert!(
        lexed
            .tokens()
            .iter()
            .any(|token| token.kind() == InkScriptTokenKind::Base64Literal)
    );
    assert!(
        !lexed
            .tokens()
            .iter()
            .any(|token| token.kind() == InkScriptTokenKind::LineComment)
    );

    assert_eq!(
        diagnostic_codes("true\u{feff}false".as_bytes()),
        vec![InkScriptDiagnosticCode::UnexpectedCharacter]
    );
    assert_eq!(
        diagnostic_codes(br#""\u{0}""#),
        vec![InkScriptDiagnosticCode::InvalidUnicodeEscape]
    );
}

#[test]
fn public_lexer_validates_compound_literals_and_base64_unused_bits() {
    assert_eq!(
        diagnostic_codes(br#"uuid"550E8400-e29b-41d4-a716-446655440000""#),
        vec![InkScriptDiagnosticCode::InvalidUuidLiteral]
    );
    assert_eq!(
        diagnostic_codes(br#"blake3"abcd""#),
        vec![InkScriptDiagnosticCode::InvalidDigestLiteral]
    );
    assert_eq!(
        diagnostic_codes(br#"base64"""AB==""""#),
        vec![InkScriptDiagnosticCode::InvalidBase64Encoding]
    );
    assert_eq!(
        diagnostic_codes(br#"base64"""AA=A""""#),
        vec![InkScriptDiagnosticCode::InvalidBase64Encoding]
    );
}

#[test]
fn public_lexer_rejects_truncated_string_escape_base64_and_crlf() {
    assert_eq!(
        diagnostic_codes(br#""unterminated"#),
        vec![InkScriptDiagnosticCode::UnterminatedString]
    );
    assert_eq!(
        diagnostic_codes(br#""escape\"#),
        vec![InkScriptDiagnosticCode::UnterminatedString]
    );
    assert_eq!(
        diagnostic_codes(br#"base64"""QUJD"#),
        vec![InkScriptDiagnosticCode::UnterminatedBase64]
    );
    assert_eq!(
        diagnostic_codes(b"true\r"),
        vec![InkScriptDiagnosticCode::StandaloneCarriageReturn]
    );
}

#[test]
fn public_lexer_stops_at_identifier_numeric_token_and_diagnostic_limits() {
    let identifier_limits = InkScriptLexerLimits::exact_current().with_identifier_byte_limit(4);
    let source = fixture_source(b"abcde followed_by_unscanned_input");
    let lexed = lex_inkscript_with_limits(&source, identifier_limits);
    assert!(!lexed.is_complete());
    assert_eq!(
        lexed.diagnostics()[0].code(),
        InkScriptDiagnosticCode::IdentifierTooLong
    );
    assert!(lexed.tokens().is_empty());

    let numeric_limits = InkScriptLexerLimits::exact_current().with_numeric_byte_limit(4);
    let source = fixture_source(b"12345 7");
    let lexed = lex_inkscript_with_limits(&source, numeric_limits);
    assert!(!lexed.is_complete());
    assert_eq!(
        lexed.diagnostics()[0].code(),
        InkScriptDiagnosticCode::NumericLiteralTooLong
    );

    let token_limits = InkScriptLexerLimits::exact_current().with_token_limit(2);
    let source = fixture_source(b"a b c");
    let lexed = lex_inkscript_with_limits(&source, token_limits);
    assert!(!lexed.is_complete());
    assert_eq!(
        lexed.diagnostics()[0].code(),
        InkScriptDiagnosticCode::TokenLimitExceeded
    );
    assert_eq!(lexed.tokens().len(), 2);

    let diagnostic_limits = InkScriptLexerLimits::exact_current().with_diagnostic_limit(3);
    let source = fixture_source(b"@@@@@@");
    let lexed = lex_inkscript_with_limits(&source, diagnostic_limits);
    assert!(!lexed.is_complete());
    assert_eq!(lexed.diagnostics().len(), 3);
    assert_eq!(
        lexed.diagnostics()[2].code(),
        InkScriptDiagnosticCode::DiagnosticLimitExceeded
    );
}

#[test]
fn public_lexer_enforces_decoded_string_and_inline_asset_limits() {
    let string_limits = InkScriptLexerLimits::exact_current().with_string_byte_limit(4);
    let source = fixture_source("\"ééé\" trailing".as_bytes());
    let lexed = lex_inkscript_with_limits(&source, string_limits);
    assert!(!lexed.is_complete());
    assert_eq!(
        lexed.diagnostics()[0].code(),
        InkScriptDiagnosticCode::StringTooLong
    );

    let asset_limits = InkScriptLexerLimits::exact_current().with_inline_asset_byte_limit(3);
    let source = fixture_source(br#"base64"""QUJDRA==""" trailing"#);
    let lexed = lex_inkscript_with_limits(&source, asset_limits);
    assert!(!lexed.is_complete());
    assert_eq!(
        lexed.diagnostics()[0].code(),
        InkScriptDiagnosticCode::InlineAssetTooLarge
    );
}

#[test]
fn public_lexer_empty_input_is_a_stable_no_op_and_source_is_owned() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<InkScriptSource>();

    let mut bytes = b"true".to_vec();
    let source = InkScriptSource::new(InkScriptSourceId::new(21), &bytes).unwrap();
    bytes.fill(b'x');
    assert_eq!(source.bytes(), b"true");

    let empty = fixture_source(b"");
    let first = lex_inkscript(&empty);
    let second = lex_inkscript(&empty);
    assert!(first.is_complete());
    assert!(first.diagnostics().is_empty());
    assert_eq!(first.tokens(), second.tokens());
    assert_eq!(first.tokens()[0].kind(), InkScriptTokenKind::EndOfSource);
}

#[test]
fn public_lexer_malformed_and_truncation_corpus_never_panics() {
    let valid = "inkscript 1;\nprogram { step \"é\" { enabled = true; invoke x {}; }; }\n";
    for length in 0..=valid.len() {
        match InkScriptSource::new(InkScriptSourceId::new(31), &valid.as_bytes()[..length]) {
            Ok(source) => {
                let _ = lex_inkscript(&source);
            }
            Err(diagnostic) => {
                assert_eq!(diagnostic.code(), InkScriptDiagnosticCode::InvalidUtf8);
            }
        }
    }

    for bytes in [
        &b"\xff\xfe"[..],
        &b"\0\0\0"[..],
        &b"\"\\u{}\""[..],
        &b"\"\\u{110000}\""[..],
        &b"base64\"\"\"A===\"\"\""[..],
        &b"base64\"\"\"////=\"\"\""[..],
        &b"---"[..],
    ] {
        if let Ok(source) = InkScriptSource::new(InkScriptSourceId::new(32), bytes) {
            let _ = lex_inkscript(&source);
        }
    }
}
