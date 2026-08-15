use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(u64),
    String(String),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> JsonParser<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Json, String> {
        let mut parser = Self { bytes, cursor: 0 };
        let value = parser.value()?;
        parser.whitespace();
        if parser.cursor != bytes.len() {
            return Err(format!("trailing JSON bytes at {}", parser.cursor));
        }
        Ok(value)
    }

    fn value(&mut self) -> Result<Json, String> {
        self.whitespace();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Json::String),
            Some(b't') => self.keyword(b"true", Json::Bool(true)),
            Some(b'f') => self.keyword(b"false", Json::Bool(false)),
            Some(b'n') => self.keyword(b"null", Json::Null),
            Some(b'0'..=b'9') => self.number(),
            _ => Err(format!("invalid JSON value at {}", self.cursor)),
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut values = BTreeMap::new();
        self.whitespace();
        if self.take(b'}') {
            return Ok(Json::Object(values));
        }
        loop {
            self.whitespace();
            let key = self.string()?;
            self.whitespace();
            self.expect(b':')?;
            let value = self.value()?;
            if values.insert(key.clone(), value).is_some() {
                return Err(format!("duplicate JSON key {key:?}"));
            }
            self.whitespace();
            if self.take(b'}') {
                break;
            }
            self.expect(b',')?;
        }
        Ok(Json::Object(values))
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut values = Vec::new();
        self.whitespace();
        if self.take(b']') {
            return Ok(Json::Array(values));
        }
        loop {
            values.push(self.value()?);
            self.whitespace();
            if self.take(b']') {
                break;
            }
            self.expect(b',')?;
        }
        Ok(Json::Array(values))
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut result = String::new();
        loop {
            let byte = self
                .peek()
                .ok_or_else(|| "unterminated JSON string".to_owned())?;
            if byte == b'"' {
                self.cursor += 1;
                return Ok(result);
            }
            if byte == b'\\' {
                self.cursor += 1;
                let escaped = self
                    .peek()
                    .ok_or_else(|| "unterminated JSON escape".to_owned())?;
                self.cursor += 1;
                match escaped {
                    b'"' => result.push('"'),
                    b'\\' => result.push('\\'),
                    b'/' => result.push('/'),
                    b'b' => result.push('\u{0008}'),
                    b'f' => result.push('\u{000c}'),
                    b'n' => result.push('\n'),
                    b'r' => result.push('\r'),
                    b't' => result.push('\t'),
                    b'u' => {
                        let value = self.hex_quad()?;
                        if (0xd800..=0xdbff).contains(&value) {
                            self.expect(b'\\')?;
                            self.expect(b'u')?;
                            let low = self.hex_quad()?;
                            if !(0xdc00..=0xdfff).contains(&low) {
                                return Err("invalid low surrogate in JSON string".to_owned());
                            }
                            let scalar = 0x1_0000
                                + ((u32::from(value) - 0xd800) << 10)
                                + (u32::from(low) - 0xdc00);
                            result.push(
                                char::from_u32(scalar)
                                    .ok_or_else(|| "invalid JSON scalar".to_owned())?,
                            );
                        } else if (0xdc00..=0xdfff).contains(&value) {
                            return Err("unpaired low surrogate in JSON string".to_owned());
                        } else {
                            result.push(
                                char::from_u32(u32::from(value))
                                    .ok_or_else(|| "invalid JSON scalar".to_owned())?,
                            );
                        }
                    }
                    _ => return Err(format!("invalid JSON escape at {}", self.cursor - 1)),
                }
                continue;
            }
            if byte < 0x20 {
                return Err(format!("control byte in JSON string at {}", self.cursor));
            }
            let tail = std::str::from_utf8(&self.bytes[self.cursor..])
                .map_err(|error| format!("invalid UTF-8 in JSON string: {error}"))?;
            let character = tail
                .chars()
                .next()
                .ok_or_else(|| "unterminated JSON string".to_owned())?;
            result.push(character);
            self.cursor += character.len_utf8();
        }
    }

    fn hex_quad(&mut self) -> Result<u16, String> {
        let end = self
            .cursor
            .checked_add(4)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| "short JSON unicode escape".to_owned())?;
        let text = std::str::from_utf8(&self.bytes[self.cursor..end])
            .map_err(|error| format!("invalid JSON unicode escape: {error}"))?;
        self.cursor = end;
        u16::from_str_radix(text, 16).map_err(|_| "invalid JSON unicode escape".to_owned())
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.cursor;
        if self.take(b'0') {
            if matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(format!("leading zero in JSON number at {start}"));
            }
        } else {
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.cursor += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.cursor])
            .map_err(|error| format!("invalid JSON number: {error}"))?;
        text.parse::<u64>()
            .map(Json::Number)
            .map_err(|error| format!("invalid JSON number {text:?}: {error}"))
    }

    fn keyword(&mut self, keyword: &[u8], value: Json) -> Result<Json, String> {
        let end = self.cursor.saturating_add(keyword.len());
        if self.bytes.get(self.cursor..end) != Some(keyword) {
            return Err(format!("invalid JSON keyword at {}", self.cursor));
        }
        self.cursor = end;
        Ok(value)
    }

    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.cursor += 1;
        }
    }

    fn expect(&mut self, expected: u8) -> Result<(), String> {
        if self.take(expected) {
            Ok(())
        } else {
            Err(format!(
                "expected JSON byte {:?} at {}",
                char::from(expected),
                self.cursor
            ))
        }
    }

    fn take(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.cursor).copied()
    }
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("Core crate must be below the repository root")
        .to_path_buf()
}

fn load_json(relative: &str) -> Json {
    let path = repository().join(relative);
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    JsonParser::parse(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn object(value: &Json) -> &BTreeMap<String, Json> {
    match value {
        Json::Object(value) => value,
        _ => panic!("expected JSON object, got {value:?}"),
    }
}

fn array(value: &Json) -> &[Json] {
    match value {
        Json::Array(value) => value,
        _ => panic!("expected JSON array, got {value:?}"),
    }
}

fn string(value: &Json) -> &str {
    match value {
        Json::String(value) => value,
        _ => panic!("expected JSON string, got {value:?}"),
    }
}

fn number(value: &Json) -> u64 {
    match value {
        Json::Number(value) => *value,
        _ => panic!("expected JSON number, got {value:?}"),
    }
}

fn member<'a>(value: &'a Json, name: &str) -> &'a Json {
    object(value)
        .get(name)
        .unwrap_or_else(|| panic!("missing JSON member {name:?}"))
}

fn assert_exact_keys(value: &Json, expected: &[&str]) {
    let actual = object(value)
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
}

fn assert_all_object_schemas_are_closed(value: &Json, path: &str) {
    match value {
        Json::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                assert_all_object_schemas_are_closed(child, &format!("{path}[{index}]"));
            }
        }
        Json::Object(values) => {
            if values.get("type") == Some(&Json::String("object".to_owned())) {
                assert_eq!(
                    values.get("additionalProperties"),
                    Some(&Json::Bool(false)),
                    "object schema {path} must be closed"
                );
                assert!(
                    matches!(values.get("properties"), Some(Json::Object(_))),
                    "object schema {path} must enumerate properties"
                );
            }
            for (name, child) in values {
                assert_all_object_schemas_are_closed(child, &format!("{path}.{name}"));
            }
        }
        Json::Null | Json::Bool(_) | Json::Number(_) | Json::String(_) => {}
    }
}

fn named<'a>(values: &'a Json, name: &str) -> &'a Json {
    array(values)
        .iter()
        .find(|value| string(member(value, "name")) == name)
        .unwrap_or_else(|| panic!("missing named registry entry {name:?}"))
}

fn registry_type_names(language: &Json) -> BTreeSet<&str> {
    let mut names = BTreeSet::new();
    for collection in ["types", "enums", "records"] {
        for value in array(member(language, collection)) {
            let name = string(member(value, "name"));
            assert!(names.insert(name), "duplicate language type {name}");
        }
    }
    names
}

fn base_type(reference: &str) -> &str {
    reference
        .strip_prefix("list<")
        .and_then(|value| value.strip_suffix('>'))
        .or_else(|| {
            reference
                .strip_prefix("nullable<")
                .and_then(|value| value.strip_suffix('>'))
        })
        .map(base_type)
        .unwrap_or(reference)
}

fn validate_fields(fields: &Json, type_names: &BTreeSet<&str>, owner: &str) {
    let mut names = BTreeSet::new();
    let mut orders = BTreeSet::new();
    for field in array(fields) {
        assert_exact_keys(
            field,
            &[
                "canonical_order",
                "constraints",
                "default",
                "name",
                "required",
                "type",
            ],
        );
        let name = string(member(field, "name"));
        assert!(names.insert(name), "duplicate {owner} field {name}");
        let field_type = string(member(field, "type"));
        assert!(
            type_names.contains(base_type(field_type)),
            "{owner}.{name} references unknown type {field_type}"
        );
        assert!(
            orders.insert(number(member(field, "canonical_order"))),
            "duplicate canonical order in {owner}"
        );
        assert!(matches!(member(field, "required"), Json::Bool(_)));
        assert!(matches!(member(field, "constraints"), Json::Array(_)));
    }
}

#[test]
fn inkscript_registry_meta_schema_is_closed_and_draft_is_private() {
    let meta = load_json("schemas/inkscript/registry-schema-v1.json");
    assert_exact_keys(
        &meta,
        &[
            "$defs",
            "$id",
            "$schema",
            "oneOf",
            "title",
            "unevaluatedProperties",
        ],
    );
    assert_eq!(
        string(member(&meta, "$schema")),
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_all_object_schemas_are_closed(&meta, "registry-schema-v1");
    let definitions = object(member(&meta, "$defs"));
    for required in [
        "language_registry",
        "catalog_registry",
        "owner_manifest",
        "numeric_expression",
        "boolean_expression",
        "portability_evaluator",
        "work_formula",
        "command_entry",
        "stable_id_role",
        "asset_role",
    ] {
        assert!(
            definitions.contains_key(required),
            "missing schema {required}"
        );
    }

    let draft = load_json("schemas/inkscript/catalog-v1.draft.json");
    assert_exact_keys(
        &draft,
        &[
            "catalog_version",
            "entries",
            "file_version",
            "kind",
            "production",
            "registry_schema_version",
            "required_replay_epoch",
        ],
    );
    assert_eq!(
        string(member(&draft, "kind")),
        "inkpod.inkscript.catalog-draft"
    );
    assert_eq!(member(&draft, "production"), &Json::Bool(false));
    assert!(array(member(&draft, "entries")).is_empty());
    assert!(
        !repository()
            .join("schemas/inkscript/catalog-v1.json")
            .exists(),
        "catalog v1 must not be frozen before the completeness gate"
    );
    assert!(
        !repository()
            .join("docs/inkscript-command-reference.md")
            .exists(),
        "the generated command reference must not exist before catalog freeze"
    );
}

#[test]
fn inkscript_registry_json_rejects_duplicate_malformed_and_overflowing_input() {
    for malformed in [
        br#"{"version":1,"version":1}"#.as_slice(),
        br#"{"version":18446744073709551616}"#.as_slice(),
        br#"{"version":01}"#.as_slice(),
        br#"{"text":"\ud800"}"#.as_slice(),
        br#"{"closed":true} trailing"#.as_slice(),
    ] {
        assert!(
            JsonParser::parse(malformed).is_err(),
            "malformed JSON was accepted: {}",
            String::from_utf8_lossy(malformed)
        );
    }
}

#[test]
fn inkscript_language_core_is_closed_and_references_resolve() {
    let language = load_json("schemas/inkscript/language-v1.json");
    assert_exact_keys(
        &language,
        &[
            "assert_kinds",
            "asset_kinds",
            "canonicalization",
            "constructors",
            "enums",
            "file_version",
            "grammar",
            "input_kinds",
            "kind",
            "persistent_id_namespaces",
            "procedure_catalog_version",
            "records",
            "registry_schema_version",
            "required_replay_epoch",
            "resource_limits",
            "sections",
            "selector_entities",
            "types",
        ],
    );
    assert_eq!(
        string(member(&language, "kind")),
        "inkpod.inkscript.language"
    );

    let grammar = member(&language, "grammar");
    assert_exact_keys(grammar, &["escapes", "keywords", "rules"]);
    let expected_keywords = [
        "as",
        "assert",
        "asset",
        "assets",
        "base64",
        "bindings",
        "blake3",
        "current_document",
        "current_sequence",
        "editor_group",
        "enabled",
        "execution",
        "false",
        "file",
        "folder",
        "inkscript",
        "inkscript_fragment",
        "inputs",
        "invoke",
        "let",
        "list",
        "meta",
        "none",
        "nullable",
        "output",
        "param",
        "parameters",
        "program",
        "requires",
        "select",
        "step",
        "true",
        "uuid",
    ];
    let keywords = array(member(grammar, "keywords"))
        .iter()
        .map(string)
        .collect::<BTreeSet<_>>();
    assert_eq!(keywords, expected_keywords.into_iter().collect());
    let expected_escapes = ["\\\"", "\\\\", "\\n", "\\r", "\\t", "\\u{1..6 hex}"];
    let escapes = array(member(grammar, "escapes"))
        .iter()
        .map(string)
        .collect::<BTreeSet<_>>();
    assert_eq!(escapes, expected_escapes.into_iter().collect());

    let rules = array(member(grammar, "rules"));
    let mut rule_names = BTreeSet::new();
    for rule in rules {
        assert_exact_keys(rule, &["ebnf", "name", "references"]);
        let name = string(member(rule, "name"));
        assert!(rule_names.insert(name), "duplicate grammar rule {name}");
        assert!(!string(member(rule, "ebnf")).is_empty());
    }
    for rule in rules {
        for reference in array(member(rule, "references")) {
            let reference = string(reference);
            assert!(
                rule_names.contains(reference),
                "grammar rule {} references undefined nonterminal {reference}",
                string(member(rule, "name"))
            );
        }
    }
    for required in ["file", "fragment", "string", "value", "type_ref"] {
        assert!(
            rule_names.contains(required),
            "missing grammar rule {required}"
        );
    }

    let types = registry_type_names(&language);
    for record in array(member(&language, "records")) {
        validate_fields(
            member(record, "fields"),
            &types,
            string(member(record, "name")),
        );
    }
    let mut constructor_names = BTreeSet::new();
    for constructor in array(member(&language, "constructors")) {
        let constructor_name = string(member(constructor, "name"));
        assert!(
            constructor_names.insert(constructor_name),
            "duplicate constructor {constructor_name}"
        );
        assert!(types.contains(string(member(constructor, "result"))));
        let mut argument_names = BTreeSet::new();
        for argument in array(member(constructor, "arguments")) {
            let name = string(member(argument, "name"));
            assert!(argument_names.insert(name));
            assert!(types.contains(base_type(string(member(argument, "type")))));
        }
    }
    assert_eq!(
        constructor_names,
        [
            "gray16", "gray8", "mask8", "point", "q16", "range", "rect", "rgba16", "rgba8"
        ]
        .into_iter()
        .collect()
    );
    let mut section_names = BTreeSet::new();
    for section in array(member(&language, "sections")) {
        assert!(section_names.insert(string(member(section, "name"))));
        assert!(types.contains(string(member(section, "body_type"))));
    }
    assert_eq!(
        section_names,
        [
            "assets",
            "bindings",
            "execution",
            "inputs",
            "meta",
            "output",
            "parameters",
            "program",
            "requires",
        ]
        .into_iter()
        .collect()
    );
    let mut input_names = BTreeSet::new();
    for input in array(member(&language, "input_kinds")) {
        assert!(input_names.insert(string(member(input, "name"))));
        assert!(types.contains(string(member(input, "options_type"))));
    }
    assert_eq!(
        input_names,
        ["current_document", "current_sequence", "file", "folder"]
            .into_iter()
            .collect()
    );
    let mut selector_names = BTreeSet::new();
    for selector in array(member(&language, "selector_entities")) {
        assert!(selector_names.insert(string(member(selector, "name"))));
        assert!(types.contains(string(member(selector, "reference_type"))));
        validate_fields(
            member(selector, "filters"),
            &types,
            string(member(selector, "name")),
        );
    }
    assert_eq!(
        selector_names,
        [
            "annotation",
            "guide",
            "layer",
            "light_table_item",
            "light_table_set",
            "plane",
            "shooting_frame",
            "vanishing_point",
            "vector_fill",
            "vector_path",
        ]
        .into_iter()
        .collect()
    );
    let mut assert_names = BTreeSet::new();
    for assertion in array(member(&language, "assert_kinds")) {
        assert!(assert_names.insert(string(member(assertion, "name"))));
        validate_fields(
            member(assertion, "fields"),
            &types,
            string(member(assertion, "name")),
        );
    }
    assert_eq!(
        assert_names,
        ["document", "object", "selection"].into_iter().collect()
    );
    let mut asset_names = BTreeSet::new();
    for asset in array(member(&language, "asset_kinds")) {
        assert!(asset_names.insert(string(member(asset, "name"))));
        assert!(types.contains(string(member(asset, "descriptor_type"))));
        assert!(matches!(member(asset, "length_formula"), Json::Object(_)));
    }
    assert_eq!(asset_names, ["canonical_raster"].into_iter().collect());

    let namespaces = array(member(&language, "persistent_id_namespaces"));
    let mut tags = BTreeSet::new();
    let mut namespace_orders = BTreeSet::new();
    for namespace in namespaces {
        assert_exact_keys(namespace, &["order", "tag"]);
        assert!(tags.insert(string(member(namespace, "tag"))));
        assert!(namespace_orders.insert(number(member(namespace, "order"))));
    }
    assert_eq!(tags.len(), 5);
    assert_eq!(namespace_orders, (0..5).collect());
    let limits = array(member(&language, "resource_limits"));
    let mut limit_names = BTreeSet::new();
    for limit in limits {
        assert!(number(member(limit, "maximum")) > 0);
        assert!(limit_names.insert(string(member(limit, "name"))));
    }
    assert_eq!(limits.len(), 32);

    let _ = named(member(&language, "records"), "requires_record");
    let _ = named(member(&language, "records"), "canonical_raster_descriptor");
    let _ = named(member(&language, "selector_entities"), "plane");
    let _ = named(member(&language, "assert_kinds"), "document");
}

#[derive(Debug, Eq, PartialEq)]
struct CatalogEntry {
    rust_name: String,
    id: String,
    canonical_name: String,
    schema_version: u64,
    semantics_revision: u64,
    replayable: bool,
}

fn parse_primitive_ids(source: &str) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for line in source.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("pub const ") else {
            continue;
        };
        let Some((name, value)) = rest.split_once(": Self = Self(") else {
            continue;
        };
        let Some(value) = value.strip_suffix(");") else {
            continue;
        };
        if value.starts_with("0x") {
            result.insert(name.to_owned(), value.replace('_', ""));
        }
    }
    result
}

fn split_macro_arguments(arguments: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    for (index, character) in arguments.char_indices() {
        if character == '"' {
            quoted = !quoted;
        } else if character == ',' && !quoted {
            result.push(arguments[start..index].trim().to_owned());
            start = index + 1;
        }
    }
    result.push(arguments[start..].trim().to_owned());
    result
}

fn parse_catalog_entries(repository: &Path) -> Vec<CatalogEntry> {
    let model_path = repository.join("rust/inkpod-core/src/primitive/model.rs");
    let catalog_path = repository.join("rust/inkpod-core/src/primitive/catalog.rs");
    let model = fs::read_to_string(&model_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", model_path.display()));
    let source = fs::read_to_string(&catalog_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", catalog_path.display()));
    let ids = parse_primitive_ids(&model);
    let catalog = source
        .split_once("const PRIMITIVE_CATALOG")
        .expect("primitive catalog declaration must exist")
        .1
        .split_once("\n];")
        .expect("primitive catalog terminator must exist")
        .0;
    let mut entries = Vec::new();
    let mut remainder = catalog;
    while let Some((_, after)) = remainder.split_once("entry!(") {
        let end = after.find(')').expect("entry macro must close");
        let arguments = split_macro_arguments(&after[..end]);
        assert!(matches!(arguments.len(), 5 | 6));
        let rust_name = arguments[0].clone();
        entries.push(CatalogEntry {
            id: ids
                .get(&rust_name)
                .unwrap_or_else(|| panic!("missing PrimitiveId constant {rust_name}"))
                .clone(),
            rust_name,
            schema_version: arguments[1]
                .parse()
                .expect("schema version must be numeric"),
            canonical_name: arguments[2].trim_matches('"').to_owned(),
            semantics_revision: arguments[3]
                .parse()
                .expect("semantics revision must be numeric"),
            replayable: arguments.get(5).is_none_or(|value| value != "session"),
        });
        remainder = &after[end + 1..];
    }
    entries
}

#[test]
fn inkscript_owner_manifest_is_a_bijection_with_replayable_primitives() {
    let manifest = load_json("schemas/inkscript/owner-manifest-v1.json");
    assert_exact_keys(
        &manifest,
        &[
            "batch_traceability",
            "excluded_primitives",
            "kind",
            "owners",
            "replay_contract",
            "registry_schema_version",
        ],
    );
    let mut actual = BTreeMap::new();
    for owner in array(member(&manifest, "owners")) {
        assert_exact_keys(
            owner,
            &[
                "canonical_name",
                "command_name",
                "owner_milestone",
                "planned_equivalence_test",
                "primitive_id",
                "primitive_rust",
                "primitive_schema_version",
                "semantics_revision",
            ],
        );
        let rust_name = string(member(owner, "primitive_rust"));
        assert!(
            actual.insert(rust_name, owner).is_none(),
            "duplicate owner for {rust_name}"
        );
        let milestone = string(member(owner, "owner_milestone"));
        let numeric = milestone
            .strip_prefix('M')
            .expect("owner milestone must use the milestone namespace")
            .trim_end_matches(['A', 'B'])
            .parse::<u8>()
            .expect("owner milestone must have a numeric body");
        assert!(matches!(numeric, 7..=9 | 15..=22));
        let command = string(member(owner, "command_name"));
        assert!(
            !command.is_empty()
                && command
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        );
        assert!(string(member(owner, "planned_equivalence_test")).starts_with("INKS-EQ-"));
    }

    let catalog = parse_catalog_entries(&repository());
    let replayable = catalog
        .iter()
        .filter(|entry| entry.replayable)
        .collect::<Vec<_>>();
    assert_eq!(replayable.len(), 84);
    assert_eq!(actual.len(), replayable.len());
    for entry in replayable {
        let owner = actual
            .get(entry.rust_name.as_str())
            .unwrap_or_else(|| panic!("unassigned replayable primitive {}", entry.rust_name));
        assert_eq!(string(member(owner, "primitive_id")), entry.id);
        assert_eq!(
            string(member(owner, "canonical_name")),
            entry.canonical_name
        );
        assert_eq!(
            number(member(owner, "primitive_schema_version")),
            entry.schema_version
        );
        assert_eq!(
            number(member(owner, "semantics_revision")),
            entry.semantics_revision
        );
    }

    let excluded = array(member(&manifest, "excluded_primitives"));
    assert_eq!(excluded.len(), 1);
    assert_eq!(
        string(member(&excluded[0], "primitive_rust")),
        "LIGHT_TABLE_SWAP_WITH_ACTIVE"
    );
    assert_eq!(string(member(&excluded[0], "reason")), "session_only");
}

#[test]
fn inkscript_versions_and_traceability_match_repository_contracts() {
    let language = load_json("schemas/inkscript/language-v1.json");
    let draft = load_json("schemas/inkscript/catalog-v1.draft.json");
    let manifest = load_json("schemas/inkscript/owner-manifest-v1.json");
    for value in [&language, &draft] {
        assert_eq!(number(member(value, "registry_schema_version")), 1);
        assert_eq!(number(member(value, "file_version")), 1);
        assert_eq!(number(member(value, "required_replay_epoch")), 23);
    }
    assert_eq!(number(member(&language, "procedure_catalog_version")), 1);
    assert_eq!(number(member(&draft, "catalog_version")), 1);

    let contract = member(&manifest, "replay_contract");
    assert_exact_keys(
        contract,
        &[
            "c_abi_version",
            "inkpod_top_level_version",
            "inkscript_file_version",
            "procedure_catalog_version",
            "replay_epoch",
        ],
    );
    assert_eq!(number(member(contract, "inkscript_file_version")), 1);
    assert_eq!(number(member(contract, "procedure_catalog_version")), 1);
    assert_eq!(number(member(contract, "replay_epoch")), 23);
    assert_eq!(number(member(contract, "inkpod_top_level_version")), 26);
    assert_eq!(number(member(contract, "c_abi_version")), 14);

    let repository = repository();
    let model = fs::read_to_string(repository.join("rust/inkpod-core/src/primitive/model.rs"))
        .expect("primitive model must be readable");
    let format = fs::read_to_string(repository.join("rust/inkpod-format/src/procedure.rs"))
        .expect("format contract must be readable");
    let header = fs::read_to_string(repository.join("include/inkpod/core_ffi.h"))
        .expect("ABI header must be readable");
    assert!(model.contains("pub const CURRENT: Self = Self(23);"));
    assert!(model.contains("pub const PROCEDURE_FORMAT_VERSION: u32 = 26;"));
    assert!(format.contains("pub const FORMAT_VERSION: u32 = 26;"));
    assert!(header.contains("#define INKPOD_ABI_VERSION UINT32_C(14)"));

    let spec = fs::read_to_string(repository.join("SPEC.md")).expect("SPEC must be readable");
    let compatibility = fs::read_to_string(repository.join("docs/compatibility.md"))
        .expect("compatibility must be readable");
    let traceability = fs::read_to_string(repository.join("docs/inkscript-traceability.md"))
        .expect("InkScript traceability must be readable");
    for requirement in [
        "BATCH-001",
        "BATCH-002",
        "BATCH-003",
        "BATCH-004",
        "SCRIPT-001",
        "SCRIPT-002",
        "SCRIPT-003",
        "SCRIPT-004",
        "SCRIPT-005",
    ] {
        assert!(
            spec.contains(&format!("`{requirement}`")),
            "SPEC lacks {requirement}"
        );
        assert!(
            compatibility.contains(&format!("`{requirement}`")),
            "compatibility lacks {requirement}"
        );
        assert!(
            traceability.contains(&format!("`{requirement}`")),
            "traceability lacks {requirement}"
        );
    }
    assert_eq!(array(member(&manifest, "batch_traceability")).len(), 4);
}

fn collect_production_sources(root: &Path, output: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to enumerate {}: {error}", root.display()))
    {
        let path = entry.expect("directory entry must be readable").path();
        if path.is_dir() {
            collect_production_sources(&path, output);
        } else {
            output.push(path);
        }
    }
}

#[test]
fn inkscript_private_draft_is_unreachable_from_production() {
    let repository = repository();
    let mut sources = Vec::new();
    for root in [
        "rust/inkpod-core/src",
        "rust/inkpod-format/src",
        "rust/inkpod-ffi/src",
        "rust/inkpod-image/src",
        "apps/windows",
        "include",
    ] {
        collect_production_sources(&repository.join(root), &mut sources);
    }
    for source in sources {
        let contents = fs::read(&source)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()));
        let text = String::from_utf8_lossy(&contents);
        assert!(
            !text.contains("catalog-v1.draft.json")
                && !text.contains("inkpod.inkscript.catalog-draft")
                && !text.contains("schemas/inkscript"),
            "production source {} reaches the private InkScript draft",
            source.display()
        );
    }
}

#[test]
fn inkscript_typed_frontend_models_are_unreachable_from_core_ffi_and_windows() {
    let repository = repository();
    let mut sources = Vec::new();
    for root in [
        "rust/inkpod-core/src",
        "rust/inkpod-ffi/src",
        "apps/windows",
        "include",
    ] {
        collect_production_sources(&repository.join(root), &mut sources);
    }
    for source in sources {
        let contents = fs::read(&source)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()));
        let text = String::from_utf8_lossy(&contents);
        assert!(
            !text.contains("build_inkscript_orchestration_envelope")
                && !text.contains("InkScriptOrchestrationEnvelope")
                && !text.contains("InkScriptPathIntentPreview")
                && !text.contains("build_inkscript_declaration_model")
                && !text.contains("InkScriptDeclarationModel")
                && !text.contains("resolve_inkscript_run_parameters")
                && !text.contains("close_inkscript_fragment")
                && !text.contains("InkScriptClosedFragment")
                && !text.contains("InkScriptTypedStep")
                && !text.contains("InkScriptDependencyEdge")
                && !text.contains("prepare_inkscript_initial_state")
                && !text.contains("InkScriptCatalogView")
                && !text.contains("InkScriptInitialDocumentSnapshot"),
            "production source {} reaches a private typed InkScript model",
            source.display()
        );
    }

    let format_public_root = fs::read_to_string(repository.join("rust/inkpod-format/src/lib.rs"))
        .expect("format public root must be readable");
    for private_name in [
        "prepare_inkscript_initial_state",
        "InkScriptCatalogView",
        "InkScriptInitialDocumentSnapshot",
        "InkScriptInitialPreparation",
    ] {
        assert!(
            !format_public_root.contains(private_name),
            "crate-internal InkScript API {private_name} was publicly re-exported"
        );
    }
}
