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

fn composed_catalog_type_names<'a>(language: &'a Json, catalog: &'a Json) -> BTreeSet<&'a str> {
    let mut names = registry_type_names(language);
    for collection in ["enums", "records"] {
        for value in array(member(catalog, collection)) {
            let name = string(member(value, "name"));
            assert!(names.insert(name), "duplicate composed catalog type {name}");
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
fn inkscript_registry_meta_schema_and_production_catalog_are_closed() {
    let meta = load_json("schemas/inkscript/registry-schema-v2.json");
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
    assert_all_object_schemas_are_closed(&meta, "registry-schema-v2");
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
    for registry in ["language_registry", "catalog_registry", "owner_manifest"] {
        let schema = definitions
            .get(registry)
            .unwrap_or_else(|| panic!("missing schema {registry}"));
        let version = member(
            member(member(schema, "properties"), "registry_schema_version"),
            "const",
        );
        assert_eq!(
            number(version),
            2,
            "{registry} must accept exact-current v2"
        );
        assert_ne!(number(version), 1, "{registry} must reject superseded v1");
    }

    let draft = load_json("schemas/inkscript/catalog-v2.json");
    assert_exact_keys(
        &draft,
        &[
            "catalog_version",
            "constructors",
            "enums",
            "entries",
            "file_version",
            "kind",
            "production",
            "records",
            "registry_schema_version",
            "required_replay_epoch",
        ],
    );
    assert_eq!(string(member(&draft, "kind")), "inkpod.inkscript.catalog");
    assert_eq!(member(&draft, "production"), &Json::Bool(true));
    assert_eq!(array(member(&draft, "entries")).len(), 84);
    assert_eq!(array(member(&draft, "enums")).len(), 33);
    assert_eq!(array(member(&draft, "records")).len(), 52);
    assert_eq!(array(member(&draft, "constructors")).len(), 11);
    assert!(
        !repository()
            .join("schemas/inkscript/registry-schema-v1.json")
            .exists(),
        "exact-current registry schema must reject the superseded v1 identity"
    );
    assert!(
        !repository()
            .join("schemas/inkscript/catalog-v2.draft.json")
            .exists(),
        "the pre-ratification draft must not remain beside the production catalog"
    );
    assert!(
        repository()
            .join("docs/inkscript-command-reference.md")
            .exists(),
        "the generated command reference must exist after catalog freeze"
    );
}

#[test]
fn inkscript_document_tree_catalog_entries_are_closed_typed_and_owner_exact() {
    let language = load_json("schemas/inkscript/language-v2.json");
    let draft = load_json("schemas/inkscript/catalog-v2.json");
    let manifest = load_json("schemas/inkscript/owner-manifest-v2.json");
    let type_names = composed_catalog_type_names(&language, &draft);
    let expected = BTreeMap::from([
        ("update_paper_frames", ("0x00010001", 2, 2, "INKS-EQ-0001")),
        ("create_layer", ("0x00020001", 2, 2, "INKS-EQ-0002")),
        ("duplicate_layer", ("0x00020002", 2, 2, "INKS-EQ-0003")),
        ("delete_layer", ("0x00020003", 2, 2, "INKS-EQ-0004")),
        ("reorder_layer", ("0x00020004", 2, 2, "INKS-EQ-0005")),
        ("create_plane", ("0x00020011", 2, 2, "INKS-EQ-0007")),
        ("duplicate_plane", ("0x00020012", 2, 2, "INKS-EQ-0008")),
        ("delete_plane", ("0x00020013", 2, 2, "INKS-EQ-0009")),
        ("reorder_plane", ("0x00020014", 2, 2, "INKS-EQ-0010")),
        ("merge_plane", ("0x00020017", 2, 2, "INKS-EQ-0013")),
        ("merge_layer", ("0x00020022", 2, 2, "INKS-EQ-0015")),
        ("delete_hidden_layers", ("0x00020023", 2, 2, "INKS-EQ-0016")),
        ("edit_targets", ("0x00020030", 2, 1, "INKS-EQ-0017")),
    ]);
    const OWNER_MILESTONE: &str = concat!("M", "15");
    let manifest_owners = array(member(&manifest, "owners"))
        .iter()
        .filter(|owner| string(member(owner, "owner_milestone")) == OWNER_MILESTONE)
        .map(|owner| (string(member(owner, "command_name")), owner))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(manifest_owners.len(), expected.len());

    let entries = array(member(&draft, "entries"))
        .iter()
        .filter(|entry| string(member(entry, "owner_milestone")) == OWNER_MILESTONE)
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), expected.len());
    let mut actual = BTreeSet::new();
    for entry in entries {
        let name = string(member(entry, "name"));
        assert!(actual.insert(name));
        let (primitive_id, schema, semantics, equivalence) = expected[name];
        assert_eq!(string(member(entry, "primitive_id")), primitive_id);
        assert_eq!(number(member(entry, "primitive_schema_version")), schema);
        assert_eq!(number(member(entry, "replay_epoch")), 23);
        assert_eq!(number(member(entry, "semantics_revision")), semantics);
        assert_eq!(string(member(entry, "equivalence_test")), equivalence);
        assert_eq!(
            string(member(member(entry, "editor"), "family")),
            "document_tree"
        );
        let owner = manifest_owners[name];
        assert_eq!(string(member(owner, "primitive_id")), primitive_id);
        assert_eq!(
            string(member(owner, "planned_equivalence_test")),
            equivalence
        );

        let mut argument_orders = BTreeSet::new();
        for argument in array(member(entry, "arguments")) {
            assert!(type_names.contains(base_type(string(member(argument, "type")))));
            assert!(argument_orders.insert(number(member(argument, "canonical_order"))));
        }
        assert!(
            argument_orders
                .iter()
                .copied()
                .eq(0..argument_orders.len() as u64)
        );
        for result in array(member(entry, "results")) {
            assert_exact_keys(
                result,
                &[
                    "availability",
                    "canonical_order",
                    "cardinality",
                    "name",
                    "namespace",
                    "output_id_ordinal",
                    "owner_role",
                    "type",
                ],
            );
            assert!(type_names.contains(base_type(string(member(result, "type")))));
            assert_eq!(string(member(result, "availability")), "always_on_success");
            assert_eq!(string(member(result, "namespace")), "document_stable");
        }
    }
    assert_eq!(actual, expected.keys().copied().collect());

    for record_name in [
        "frame_rect_i32",
        "paper_margins",
        "paper_frames",
        "edit_target",
        "edit_target_command",
    ] {
        let record = named(member(&draft, "records"), record_name);
        validate_fields(member(record, "fields"), &type_names, record_name);
    }
    let constructor_names = array(member(&draft, "constructors"))
        .iter()
        .map(|value| string(member(value, "name")))
        .collect::<BTreeSet<_>>();
    assert!(
        BTreeSet::from([
            "duplicate_targets",
            "delete_targets",
            "set_target_visibility",
            "set_target_editability",
            "convert_target_planes",
            "convert_target_layers",
            "merge_targets",
            "layer_target",
            "plane_target",
        ])
        .is_subset(&constructor_names)
    );
}

#[test]
fn inkscript_metadata_color_and_guide_entries_are_closed_typed_and_owner_exact() {
    let language = load_json("schemas/inkscript/language-v2.json");
    let draft = load_json("schemas/inkscript/catalog-v2.json");
    let manifest = load_json("schemas/inkscript/owner-manifest-v2.json");
    let type_names = composed_catalog_type_names(&language, &draft);
    let expected = BTreeMap::from([
        ("set_main_line_color", ("0x00030001", 1, 3, "INKS-EQ-0021")),
        ("replace_palette", ("0x00030002", 1, 3, "INKS-EQ-0022")),
        ("replace_color_chart", ("0x00030003", 1, 1, "INKS-EQ-0023")),
        ("add_guide", ("0x00040001", 2, 2, "INKS-EQ-0024")),
        ("move_guide", ("0x00040002", 2, 2, "INKS-EQ-0025")),
        ("delete_guide", ("0x00040003", 2, 2, "INKS-EQ-0026")),
        ("set_grid", ("0x00040010", 2, 2, "INKS-EQ-0027")),
        ("delete_all_guides", ("0x00040011", 2, 2, "INKS-EQ-0028")),
    ]);
    const OWNER_MILESTONE: &str = concat!("M", "16");
    let manifest_owners = array(member(&manifest, "owners"))
        .iter()
        .filter(|owner| string(member(owner, "owner_milestone")) == OWNER_MILESTONE)
        .map(|owner| (string(member(owner, "command_name")), owner))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(manifest_owners.len(), expected.len());

    let entries = array(member(&draft, "entries"))
        .iter()
        .filter(|entry| string(member(entry, "owner_milestone")) == OWNER_MILESTONE)
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), expected.len());
    let mut actual = BTreeSet::new();
    for entry in entries {
        assert_exact_keys(
            entry,
            &[
                "arguments",
                "cancellation_boundary",
                "editor",
                "equivalence_test",
                "name",
                "owner_milestone",
                "portability",
                "primitive_id",
                "primitive_schema_version",
                "replay_epoch",
                "results",
                "semantics_revision",
                "work",
            ],
        );
        let name = string(member(entry, "name"));
        assert!(actual.insert(name), "duplicate metadata command {name}");
        let (primitive_id, schema, semantics, equivalence) = expected[name];
        assert_eq!(string(member(entry, "primitive_id")), primitive_id);
        assert_eq!(number(member(entry, "primitive_schema_version")), schema);
        assert_eq!(number(member(entry, "replay_epoch")), 23);
        assert_eq!(number(member(entry, "semantics_revision")), semantics);
        assert_eq!(string(member(entry, "equivalence_test")), equivalence);
        assert_eq!(
            string(member(member(entry, "editor"), "family")),
            "metadata_color_guide"
        );
        let owner = manifest_owners[name];
        assert_eq!(string(member(owner, "primitive_id")), primitive_id);
        assert_eq!(number(member(owner, "primitive_schema_version")), schema);
        assert_eq!(number(member(owner, "semantics_revision")), semantics);
        assert_eq!(
            string(member(owner, "planned_equivalence_test")),
            equivalence
        );

        let mut argument_orders = BTreeSet::new();
        for argument in array(member(entry, "arguments")) {
            assert!(type_names.contains(base_type(string(member(argument, "type")))));
            assert!(argument_orders.insert(number(member(argument, "canonical_order"))));
        }
        assert!(
            argument_orders
                .iter()
                .copied()
                .eq(0..argument_orders.len() as u64)
        );
    }
    assert_eq!(actual, expected.keys().copied().collect());

    for record_name in ["color_chart_entry", "grid_config"] {
        let record = named(member(&draft, "records"), record_name);
        validate_fields(member(record, "fields"), &type_names, record_name);
    }
    let constructor_names = array(member(&draft, "constructors"))
        .iter()
        .map(|value| string(member(value, "name")))
        .collect::<BTreeSet<_>>();
    assert!(
        BTreeSet::from(["chart_name_text", "chart_name_scalars"]).is_subset(&constructor_names)
    );
    let add_guide = named(member(&draft, "entries"), "add_guide");
    assert_eq!(array(member(add_guide, "results")).len(), 1);
    let result = &array(member(add_guide, "results"))[0];
    assert_eq!(string(member(result, "name")), "guide");
    assert_eq!(string(member(result, "type")), "guide_ref");
    assert_eq!(string(member(result, "namespace")), "document_stable");
    assert_eq!(number(member(result, "output_id_ordinal")), 0);
}

#[test]
fn inkscript_stroke_geometry_and_import_entries_are_closed_typed_and_owner_exact() {
    let language = load_json("schemas/inkscript/language-v2.json");
    let draft = load_json("schemas/inkscript/catalog-v2.json");
    let manifest = load_json("schemas/inkscript/owner-manifest-v2.json");
    let type_names = composed_catalog_type_names(&language, &draft);
    let expected = BTreeMap::from([
        ("apply_raster_stroke", ("0x00050001", 3, 5, "INKS-EQ-0029")),
        ("apply_geometry", ("0x00050003", 2, 1, "INKS-EQ-0031")),
        ("import_raster_asset", ("0x00090001", 1, 1, "INKS-EQ-0071")),
    ]);
    const OWNER_MILESTONE: &str = concat!("M", "17");
    let manifest_owners = array(member(&manifest, "owners"))
        .iter()
        .filter(|owner| string(member(owner, "owner_milestone")) == OWNER_MILESTONE)
        .map(|owner| (string(member(owner, "command_name")), owner))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(manifest_owners.len(), expected.len());

    let entries = array(member(&draft, "entries"))
        .iter()
        .filter(|entry| string(member(entry, "owner_milestone")) == OWNER_MILESTONE)
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), expected.len());
    let mut actual = BTreeSet::new();
    for entry in entries {
        assert_exact_keys(
            entry,
            &[
                "arguments",
                "cancellation_boundary",
                "editor",
                "equivalence_test",
                "name",
                "owner_milestone",
                "portability",
                "primitive_id",
                "primitive_schema_version",
                "replay_epoch",
                "results",
                "semantics_revision",
                "work",
            ],
        );
        let name = string(member(entry, "name"));
        assert!(
            actual.insert(name),
            "duplicate stroke/geometry/import command {name}"
        );
        let (primitive_id, schema, semantics, equivalence) = expected[name];
        assert_eq!(string(member(entry, "primitive_id")), primitive_id);
        assert_eq!(number(member(entry, "primitive_schema_version")), schema);
        assert_eq!(number(member(entry, "replay_epoch")), 23);
        assert_eq!(number(member(entry, "semantics_revision")), semantics);
        assert_eq!(string(member(entry, "equivalence_test")), equivalence);
        assert_eq!(
            string(member(member(entry, "editor"), "family")),
            "stroke_geometry_import"
        );
        let owner = manifest_owners[name];
        assert_eq!(string(member(owner, "primitive_id")), primitive_id);
        assert_eq!(number(member(owner, "primitive_schema_version")), schema);
        assert_eq!(number(member(owner, "semantics_revision")), semantics);
        assert_eq!(
            string(member(owner, "planned_equivalence_test")),
            equivalence
        );

        let mut argument_orders = BTreeSet::new();
        for argument in array(member(entry, "arguments")) {
            assert!(type_names.contains(base_type(string(member(argument, "type")))));
            assert!(argument_orders.insert(number(member(argument, "canonical_order"))));
        }
        assert!(
            argument_orders
                .iter()
                .copied()
                .eq(0..argument_orders.len() as u64)
        );
    }
    assert_eq!(actual, expected.keys().copied().collect());

    for record_name in [
        "raster_stroke_sample",
        "canonical_raster_stroke",
        "canonical_geometry_segment",
    ] {
        let record = named(member(&draft, "records"), record_name);
        validate_fields(member(record, "fields"), &type_names, record_name);
    }
    let import = named(member(&draft, "entries"), "import_raster_asset");
    let raster_argument = array(member(import, "arguments"))
        .iter()
        .find(|argument| string(member(argument, "name")) == "raster")
        .expect("import raster argument");
    assert_eq!(string(member(raster_argument, "type")), "asset_ref");
    let role = member(raster_argument, "asset_role");
    assert_eq!(string(member(role, "name")), "source_raster");
    assert_eq!(string(member(role, "kind")), "canonical_raster");
    assert_eq!(member(role, "inline"), &Json::Bool(true));
    assert_eq!(member(role, "external"), &Json::Bool(true));
}

#[test]
fn inkscript_fill_and_gradient_entries_are_closed_typed_and_owner_exact() {
    let language = load_json("schemas/inkscript/language-v2.json");
    let draft = load_json("schemas/inkscript/catalog-v2.json");
    let manifest = load_json("schemas/inkscript/owner-manifest-v2.json");
    let type_names = composed_catalog_type_names(&language, &draft);
    const OWNER_MILESTONE: &str = concat!("M", "18A");

    let owner = array(member(&manifest, "owners"))
        .iter()
        .find(|owner| string(member(owner, "owner_milestone")) == OWNER_MILESTONE)
        .expect("fill/gradient owner");
    assert_eq!(string(member(owner, "command_name")), "apply_gradient");
    assert_eq!(string(member(owner, "primitive_id")), "0x00050010");
    assert_eq!(number(member(owner, "primitive_schema_version")), 2);
    assert_eq!(number(member(owner, "semantics_revision")), 2);
    assert_eq!(
        string(member(owner, "planned_equivalence_test")),
        "INKS-EQ-0032"
    );
    assert_eq!(
        array(member(&manifest, "owners"))
            .iter()
            .filter(|owner| string(member(owner, "owner_milestone")) == OWNER_MILESTONE)
            .count(),
        1
    );

    let entries = array(member(&draft, "entries"));
    let gradient = named(member(&draft, "entries"), "apply_gradient");
    assert_exact_keys(
        gradient,
        &[
            "arguments",
            "cancellation_boundary",
            "editor",
            "equivalence_test",
            "name",
            "owner_milestone",
            "portability",
            "primitive_id",
            "primitive_schema_version",
            "replay_epoch",
            "results",
            "semantics_revision",
            "work",
        ],
    );
    assert_eq!(string(member(gradient, "primitive_id")), "0x00050010");
    assert_eq!(number(member(gradient, "primitive_schema_version")), 2);
    assert_eq!(number(member(gradient, "replay_epoch")), 23);
    assert_eq!(number(member(gradient, "semantics_revision")), 2);
    assert_eq!(string(member(gradient, "owner_milestone")), OWNER_MILESTONE);
    assert_eq!(string(member(gradient, "equivalence_test")), "INKS-EQ-0032");
    assert_eq!(
        string(member(gradient, "cancellation_boundary")),
        "before_primitive"
    );
    assert_eq!(
        string(member(member(gradient, "editor"), "family")),
        "fill_gradient"
    );
    assert_eq!(array(member(gradient, "results")).len(), 0);
    let arguments = array(member(gradient, "arguments"));
    assert_eq!(arguments.len(), 2);
    assert_eq!(string(member(&arguments[0], "name")), "plane_id");
    assert_eq!(string(member(&arguments[0], "type")), "plane_ref");
    assert_eq!(string(member(&arguments[1], "name")), "gradient");
    assert_eq!(string(member(&arguments[1], "type")), "gradient_spec");
    for argument in arguments {
        assert!(type_names.contains(base_type(string(member(argument, "type")))));
    }
    let preconditions = array(member(
        member(member(gradient, "portability"), "default"),
        "required_preconditions",
    ));
    assert_eq!(
        preconditions.iter().map(string).collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "semantic_target",
            "state_coupled_raster",
            "state_coupled_selection",
        ])
    );

    for record_name in ["gradient_stop", "gradient_spec"] {
        let record = named(member(&draft, "records"), record_name);
        validate_fields(member(record, "fields"), &type_names, record_name);
    }
    let enums = array(member(&draft, "enums"))
        .iter()
        .map(|value| string(member(value, "name")))
        .collect::<BTreeSet<_>>();
    assert!(BTreeSet::from(["gradient_kind", "gradient_mode"]).is_subset(&enums));

    assert_eq!(
        entries
            .iter()
            .filter(|entry| string(member(entry, "name")) == "apply_fill")
            .count(),
        1,
        "legacy-image apply_fill must be reused without a second owner entry"
    );
    assert_eq!(
        string(member(
            named(member(&draft, "entries"), "apply_fill"),
            "owner_milestone"
        )),
        concat!("M", "08")
    );
    assert_eq!(
        entries
            .iter()
            .filter(|entry| string(member(entry, "owner_milestone")) == OWNER_MILESTONE)
            .count(),
        1
    );
}

#[test]
fn inkscript_gesture_alpha_adjustment_entries_are_closed_typed_and_owner_exact() {
    let language = load_json("schemas/inkscript/language-v2.json");
    let draft = load_json("schemas/inkscript/catalog-v2.json");
    let manifest = load_json("schemas/inkscript/owner-manifest-v2.json");
    let type_names = composed_catalog_type_names(&language, &draft);
    const OWNER_MILESTONE: &str = concat!("M", "18B");
    let expected = BTreeMap::from([
        ("apply_blur", ("0x00050012", 2, 2, "INKS-EQ-0034")),
        ("apply_airbrush", ("0x00050013", 2, 2, "INKS-EQ-0035")),
        (
            "apply_airbrush_gesture",
            ("0x00050014", 2, 2, "INKS-EQ-0036"),
        ),
        ("apply_stamp", ("0x00050015", 2, 2, "INKS-EQ-0037")),
        ("apply_stamp_gesture", ("0x00050016", 2, 2, "INKS-EQ-0038")),
        ("apply_blur_tool", ("0x00050017", 2, 2, "INKS-EQ-0039")),
        ("edit_plane_alpha", ("0x00050019", 2, 2, "INKS-EQ-0041")),
        ("apply_alpha_gradient", ("0x0005001a", 2, 2, "INKS-EQ-0042")),
        (
            "create_adjustment_layer",
            ("0x00050030", 2, 2, "INKS-EQ-0044"),
        ),
        (
            "update_adjustment_layer",
            ("0x00050031", 2, 2, "INKS-EQ-0045"),
        ),
        ("scoped_color_replace", ("0x00050043", 2, 1, "INKS-EQ-0049")),
    ]);
    let manifest_owners = array(member(&manifest, "owners"))
        .iter()
        .filter(|owner| string(member(owner, "owner_milestone")) == OWNER_MILESTONE)
        .map(|owner| (string(member(owner, "command_name")), owner))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(manifest_owners.len(), expected.len());

    let mut actual = BTreeSet::new();
    for entry in array(member(&draft, "entries"))
        .iter()
        .filter(|entry| string(member(entry, "owner_milestone")) == OWNER_MILESTONE)
    {
        assert_exact_keys(
            entry,
            &[
                "arguments",
                "cancellation_boundary",
                "editor",
                "equivalence_test",
                "name",
                "owner_milestone",
                "portability",
                "primitive_id",
                "primitive_schema_version",
                "replay_epoch",
                "results",
                "semantics_revision",
                "work",
            ],
        );
        let name = string(member(entry, "name"));
        assert!(actual.insert(name), "duplicate gesture command {name}");
        let (primitive_id, schema, semantics, equivalence) = expected[name];
        assert_eq!(string(member(entry, "primitive_id")), primitive_id);
        assert_eq!(number(member(entry, "primitive_schema_version")), schema);
        assert_eq!(number(member(entry, "replay_epoch")), 23);
        assert_eq!(number(member(entry, "semantics_revision")), semantics);
        assert_eq!(string(member(entry, "equivalence_test")), equivalence);
        assert_eq!(
            string(member(entry, "cancellation_boundary")),
            "before_primitive"
        );
        assert_eq!(string(member(entry, "owner_milestone")), OWNER_MILESTONE);
        for argument in array(member(entry, "arguments")) {
            assert!(type_names.contains(base_type(string(member(argument, "type")))));
        }
        let owner = manifest_owners[name];
        assert_eq!(string(member(owner, "primitive_id")), primitive_id);
        assert_eq!(number(member(owner, "primitive_schema_version")), schema);
        assert_eq!(number(member(owner, "semantics_revision")), semantics);
        assert_eq!(
            string(member(owner, "planned_equivalence_test")),
            equivalence
        );
    }
    assert_eq!(actual, expected.keys().copied().collect());

    for record_name in [
        "effect_sample",
        "airbrush_stroke",
        "airbrush_gesture",
        "stamp_spec",
        "stamp_gesture",
        "adjustment_spec",
    ] {
        let record = named(member(&draft, "records"), record_name);
        validate_fields(member(record, "fields"), &type_names, record_name);
    }
    let enum_names = array(member(&draft, "enums"))
        .iter()
        .map(|value| string(member(value, "name")))
        .collect::<BTreeSet<_>>();
    assert!(
        BTreeSet::from(["stamp_shape", "adjustment_kind", "scoped_color_mode"])
            .is_subset(&enum_names)
    );

    let alpha = named(member(&draft, "entries"), "edit_plane_alpha");
    let alpha_asset = array(member(alpha, "arguments"))
        .iter()
        .find(|argument| string(member(argument, "name")) == "alpha")
        .expect("alpha asset argument");
    assert_eq!(string(member(alpha_asset, "type")), "asset_ref");
    let role = member(alpha_asset, "asset_role");
    assert_eq!(string(member(role, "name")), "alpha_raster");
    assert_eq!(string(member(role, "kind")), "canonical_raster");
    assert_eq!(member(role, "inline"), &Json::Bool(true));
    assert_eq!(member(role, "external"), &Json::Bool(true));

    let create = named(member(&draft, "entries"), "create_adjustment_layer");
    let results = array(member(create, "results"));
    assert_eq!(results.len(), 1);
    assert_eq!(string(member(&results[0], "name")), "layer");
    assert_eq!(string(member(&results[0], "type")), "layer_ref");
    assert_eq!(number(member(&results[0], "output_id_ordinal")), 0);

    for reused in ["apply_boundary_airbrush", "apply_filter", "apply_gradient"] {
        assert_eq!(
            array(member(&draft, "entries"))
                .iter()
                .filter(|entry| string(member(entry, "name")) == reused)
                .count(),
            1,
            "prior owner entry {reused} must not be duplicated"
        );
        assert_ne!(
            string(member(
                named(member(&draft, "entries"), reused),
                "owner_milestone"
            )),
            OWNER_MILESTONE
        );
    }
}

#[test]
fn inkscript_selection_floating_entries_are_closed_typed_and_owner_exact() {
    let language = load_json("schemas/inkscript/language-v2.json");
    let draft = load_json("schemas/inkscript/catalog-v2.json");
    let manifest = load_json("schemas/inkscript/owner-manifest-v2.json");
    let type_names = composed_catalog_type_names(&language, &draft);
    const OWNER_MILESTONE: &str = concat!("M", "19");
    let expected = BTreeMap::from([
        (
            "restore_selected_pixels",
            ("0x00050042", 2, 2, "INKS-EQ-0048"),
        ),
        ("apply_selection", ("0x00060001", 2, 3, "INKS-EQ-0050")),
        ("invert_selection", ("0x00060002", 2, 2, "INKS-EQ-0051")),
        ("clear_selection", ("0x00060003", 2, 2, "INKS-EQ-0052")),
        ("resize_selection", ("0x00060004", 2, 2, "INKS-EQ-0053")),
        ("select_color", ("0x00060005", 2, 2, "INKS-EQ-0054")),
        (
            "select_output_color_guard",
            ("0x00060006", 2, 1, "INKS-EQ-0055"),
        ),
        ("selection_to_layer", ("0x00060010", 2, 2, "INKS-EQ-0056")),
        ("selection_from_layer", ("0x00060011", 2, 2, "INKS-EQ-0057")),
        (
            "clear_selected_content",
            ("0x00060020", 2, 2, "INKS-EQ-0058"),
        ),
        ("commit_floating", ("0x00060021", 3, 3, "INKS-EQ-0059")),
    ]);
    let manifest_owners = array(member(&manifest, "owners"))
        .iter()
        .filter(|owner| string(member(owner, "owner_milestone")) == OWNER_MILESTONE)
        .map(|owner| (string(member(owner, "command_name")), owner))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(manifest_owners.len(), expected.len());

    let mut actual = BTreeSet::new();
    for entry in array(member(&draft, "entries"))
        .iter()
        .filter(|entry| string(member(entry, "owner_milestone")) == OWNER_MILESTONE)
    {
        assert_exact_keys(
            entry,
            &[
                "arguments",
                "cancellation_boundary",
                "editor",
                "equivalence_test",
                "name",
                "owner_milestone",
                "portability",
                "primitive_id",
                "primitive_schema_version",
                "replay_epoch",
                "results",
                "semantics_revision",
                "work",
            ],
        );
        let name = string(member(entry, "name"));
        assert!(actual.insert(name), "duplicate selection command {name}");
        let (primitive_id, schema, semantics, equivalence) = expected[name];
        assert_eq!(string(member(entry, "primitive_id")), primitive_id);
        assert_eq!(number(member(entry, "primitive_schema_version")), schema);
        assert_eq!(number(member(entry, "replay_epoch")), 23);
        assert_eq!(number(member(entry, "semantics_revision")), semantics);
        assert_eq!(string(member(entry, "equivalence_test")), equivalence);
        assert_eq!(string(member(entry, "owner_milestone")), OWNER_MILESTONE);
        for argument in array(member(entry, "arguments")) {
            assert!(type_names.contains(base_type(string(member(argument, "type")))));
        }
        let owner = manifest_owners[name];
        assert_eq!(string(member(owner, "primitive_id")), primitive_id);
        assert_eq!(number(member(owner, "primitive_schema_version")), schema);
        assert_eq!(number(member(owner, "semantics_revision")), semantics);
        assert_eq!(
            string(member(owner, "planned_equivalence_test")),
            equivalence
        );
    }
    assert_eq!(actual, expected.keys().copied().collect());

    for record_name in [
        "selection_pixel_change",
        "selection_trace_options",
        "selection_construction_options",
        "floating_plane",
        "floating_transform",
        "floating_destination",
        "floating_payload",
    ] {
        let record = named(member(&draft, "records"), record_name);
        validate_fields(member(record, "fields"), &type_names, record_name);
    }
    let enum_names = array(member(&draft, "enums"))
        .iter()
        .map(|value| string(member(value, "name")))
        .collect::<BTreeSet<_>>();
    assert!(
        BTreeSet::from([
            "selection_operation",
            "range_interpretation",
            "trace_brush_shape",
            "selection_layer_operation",
            "output_color_guard_profile",
            "floating_destination_kind",
            "floating_anchor",
        ])
        .is_subset(&enum_names)
    );

    let floating = named(member(&draft, "entries"), "commit_floating");
    let payload = array(member(floating, "arguments"))
        .iter()
        .find(|argument| string(member(argument, "name")) == "payload")
        .expect("floating payload argument");
    let role = member(payload, "asset_role");
    assert_eq!(string(member(role, "name")), "floating_rasters");
    assert_eq!(string(member(role, "kind")), "canonical_raster");
    assert_eq!(member(role, "inline"), &Json::Bool(true));
    assert_eq!(member(role, "external"), &Json::Bool(true));

    let selection_layer = named(member(&draft, "entries"), "selection_to_layer");
    let results = array(member(selection_layer, "results"));
    assert_eq!(results.len(), 1);
    assert_eq!(string(member(&results[0], "name")), "layer");
    assert_eq!(string(member(&results[0], "type")), "layer_ref");
    assert_eq!(
        string(member(&results[0], "availability")),
        "always_on_success"
    );
    assert_eq!(number(member(&results[0], "output_id_ordinal")), 0);

    for transform in ["mirror_document", "rotate_document", "resize_document"] {
        assert_eq!(
            array(member(&draft, "entries"))
                .iter()
                .filter(|entry| string(member(entry, "name")) == transform)
                .count(),
            1,
            "preexisting document transform {transform} must not be re-registered"
        );
        assert_eq!(
            string(member(
                named(member(&draft, "entries"), transform),
                "owner_milestone"
            )),
            concat!("M", "07")
        );
    }
}

#[test]
fn inkscript_vector_entries_are_closed_typed_and_owner_exact() {
    let language = load_json("schemas/inkscript/language-v2.json");
    let draft = load_json("schemas/inkscript/catalog-v2.json");
    let manifest = load_json("schemas/inkscript/owner-manifest-v2.json");
    let type_names = composed_catalog_type_names(&language, &draft);
    const OWNER_MILESTONE: &str = concat!("M", "20");
    let expected = BTreeMap::from([
        ("vector_add_path", ("0x00080001", 2, 2, "INKS-EQ-0063")),
        ("vector_add_fill", ("0x00080002", 2, 2, "INKS-EQ-0064")),
        ("vector_erase", ("0x00080003", 2, 2, "INKS-EQ-0065")),
        ("vector_connect", ("0x00080004", 2, 2, "INKS-EQ-0066")),
        ("vector_correct_width", ("0x00080005", 2, 2, "INKS-EQ-0067")),
        (
            "rasterize_vector_layer",
            ("0x00080010", 2, 2, "INKS-EQ-0068"),
        ),
        (
            "vectorize_raster_plane",
            ("0x00080011", 2, 2, "INKS-EQ-0069"),
        ),
        (
            "vectorize_raster_plane_into_new_layer",
            ("0x00080012", 2, 2, "INKS-EQ-0070"),
        ),
    ]);
    let manifest_owners = array(member(&manifest, "owners"))
        .iter()
        .filter(|owner| string(member(owner, "owner_milestone")) == OWNER_MILESTONE)
        .map(|owner| (string(member(owner, "command_name")), owner))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(manifest_owners.len(), expected.len());

    let entries = array(member(&draft, "entries"))
        .iter()
        .filter(|entry| string(member(entry, "owner_milestone")) == OWNER_MILESTONE)
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), expected.len());
    let mut actual = BTreeSet::new();
    for entry in entries {
        assert_exact_keys(
            entry,
            &[
                "arguments",
                "cancellation_boundary",
                "editor",
                "equivalence_test",
                "name",
                "owner_milestone",
                "portability",
                "primitive_id",
                "primitive_schema_version",
                "replay_epoch",
                "results",
                "semantics_revision",
                "work",
            ],
        );
        let name = string(member(entry, "name"));
        assert!(actual.insert(name), "duplicate vector command {name}");
        let (primitive_id, schema, semantics, equivalence) = expected[name];
        assert_eq!(string(member(entry, "primitive_id")), primitive_id);
        assert_eq!(number(member(entry, "primitive_schema_version")), schema);
        assert_eq!(number(member(entry, "replay_epoch")), 23);
        assert_eq!(number(member(entry, "semantics_revision")), semantics);
        assert_eq!(string(member(entry, "equivalence_test")), equivalence);
        assert_eq!(string(member(entry, "owner_milestone")), OWNER_MILESTONE);
        assert_eq!(string(member(member(entry, "editor"), "family")), "vector");
        for argument in array(member(entry, "arguments")) {
            assert!(type_names.contains(base_type(string(member(argument, "type")))));
            assert_eq!(member(argument, "asset_role"), &Json::Null);
        }
        let owner = manifest_owners[name];
        assert_eq!(string(member(owner, "primitive_id")), primitive_id);
        assert_eq!(number(member(owner, "primitive_schema_version")), schema);
        assert_eq!(number(member(owner, "semantics_revision")), semantics);
        assert_eq!(
            string(member(owner, "planned_equivalence_test")),
            equivalence
        );
    }
    assert_eq!(actual, expected.keys().copied().collect());

    for record_name in ["vector_cubic_segment", "vector_path_input", "vector_width"] {
        let record = named(member(&draft, "records"), record_name);
        validate_fields(member(record, "fields"), &type_names, record_name);
    }
    let enum_names = array(member(&draft, "enums"))
        .iter()
        .map(|value| string(member(value, "name")))
        .collect::<BTreeSet<_>>();
    assert!(BTreeSet::from(["vector_erase_mode", "vector_width_operation"]).is_subset(&enum_names));

    let add_path = named(member(&draft, "entries"), "vector_add_path");
    let add_path_results = array(member(add_path, "results"));
    assert_eq!(add_path_results.len(), 1);
    assert_eq!(string(member(&add_path_results[0], "name")), "paths");
    assert_eq!(
        string(member(&add_path_results[0], "type")),
        "list<vector_path_ref>"
    );
    assert_eq!(number(member(&add_path_results[0], "output_id_ordinal")), 0);

    let into_new = named(
        member(&draft, "entries"),
        "vectorize_raster_plane_into_new_layer",
    );
    let results = array(member(into_new, "results"));
    assert_eq!(results.len(), 2);
    assert_eq!(string(member(&results[0], "name")), "layer");
    assert_eq!(number(member(&results[0], "output_id_ordinal")), 0);
    assert_eq!(string(member(&results[1], "name")), "fills");
    assert_eq!(number(member(&results[1], "output_id_ordinal")), 1);
    let mut result_ordinals = BTreeSet::new();
    for result in results {
        assert!(result_ordinals.insert(number(member(result, "output_id_ordinal"))));
    }
}

#[test]
fn inkscript_v2_annotation_frame_vanishing_contract_is_exact() {
    let language = load_json("schemas/inkscript/language-v2.json");
    let draft = load_json("schemas/inkscript/catalog-v2.json");
    let manifest = load_json("schemas/inkscript/owner-manifest-v2.json");
    assert_eq!(number(member(&language, "file_version")), 2);
    assert_eq!(number(member(&language, "procedure_catalog_version")), 2);
    assert_eq!(number(member(&draft, "file_version")), 2);
    assert_eq!(number(member(&draft, "catalog_version")), 2);
    let shooting_frame = named(member(&language, "selector_entities"), "shooting_frame");
    assert_eq!(string(member(shooting_frame, "owner")), "document");
    assert!(
        array(member(shooting_frame, "filters"))
            .iter()
            .all(|field| string(member(field, "name")) != "layer")
    );

    let type_names = composed_catalog_type_names(&language, &draft);
    let expected = BTreeMap::from([
        (
            "edit_annotations",
            ("0x00020040", 2, 1, "INKS-EQ-0018", "annotations"),
        ),
        (
            "edit_shooting_frame",
            ("0x00020050", 2, 1, "INKS-EQ-0019", "shooting_frames"),
        ),
        (
            "edit_vanishing_points",
            ("0x00020060", 2, 1, "INKS-EQ-0020", "vanishing_points"),
        ),
    ]);
    const OWNER_MILESTONE: &str = concat!("M", "21");
    let manifest_owners = array(member(&manifest, "owners"))
        .iter()
        .filter(|owner| string(member(owner, "owner_milestone")) == OWNER_MILESTONE)
        .map(|owner| (string(member(owner, "command_name")), owner))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(manifest_owners.len(), expected.len());
    let entries = array(member(&draft, "entries"))
        .iter()
        .filter(|entry| string(member(entry, "owner_milestone")) == OWNER_MILESTONE)
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), expected.len());
    for entry in entries {
        let name = string(member(entry, "name"));
        let (primitive_id, schema, semantics, equivalence, result_name) = expected[name];
        assert_eq!(string(member(entry, "primitive_id")), primitive_id);
        assert_eq!(number(member(entry, "primitive_schema_version")), schema);
        assert_eq!(number(member(entry, "replay_epoch")), 23);
        assert_eq!(number(member(entry, "semantics_revision")), semantics);
        assert_eq!(string(member(entry, "equivalence_test")), equivalence);
        assert_eq!(string(member(entry, "owner_milestone")), OWNER_MILESTONE);
        for argument in array(member(entry, "arguments")) {
            assert!(type_names.contains(base_type(string(member(argument, "type")))));
            assert_eq!(member(argument, "asset_role"), &Json::Null);
        }
        let results = array(member(entry, "results"));
        assert_eq!(results.len(), 1);
        assert_eq!(string(member(&results[0], "name")), result_name);
        assert_eq!(string(member(&results[0], "cardinality")), "ordered_list");
        assert_eq!(number(member(&results[0], "output_id_ordinal")), 0);
        assert!(!array(member(member(entry, "portability"), "rules")).is_empty());
    }
    for record_name in [
        "annotation_point_milli",
        "annotation_object_input",
        "annotation_edit",
        "shooting_frame_input",
        "shooting_frame_edit",
        "vanishing_point_input",
        "vanishing_point_edit",
    ] {
        validate_fields(
            member(named(member(&draft, "records"), record_name), "fields"),
            &type_names,
            record_name,
        );
    }
    assert!(
        !repository()
            .join("schemas/inkscript/language-v1.json")
            .exists()
    );
    assert!(
        !repository()
            .join("schemas/inkscript/catalog-v1.draft.json")
            .exists()
    );
    assert!(
        !repository()
            .join("schemas/inkscript/owner-manifest-v1.json")
            .exists()
    );
}

#[test]
fn inkscript_light_table_entries_are_replayable_asset_owned_and_session_commands_are_excluded() {
    let language = load_json("schemas/inkscript/language-v2.json");
    let draft = load_json("schemas/inkscript/catalog-v2.json");
    let manifest = load_json("schemas/inkscript/owner-manifest-v2.json");
    let type_names = composed_catalog_type_names(&language, &draft);
    const OWNER_MILESTONE: &str = concat!("M", "22");
    let expected = BTreeMap::from([
        (
            "light_table_set_global_opacity",
            ("0x000a0001", 2, 2, "INKS-EQ-0072"),
        ),
        (
            "light_table_create_set",
            ("0x000a0002", 2, 2, "INKS-EQ-0073"),
        ),
        (
            "light_table_duplicate_set",
            ("0x000a0003", 2, 2, "INKS-EQ-0074"),
        ),
        (
            "light_table_delete_set",
            ("0x000a0004", 2, 2, "INKS-EQ-0075"),
        ),
        (
            "light_table_rename_set",
            ("0x000a0005", 2, 2, "INKS-EQ-0076"),
        ),
        (
            "light_table_reorder_set",
            ("0x000a0006", 2, 2, "INKS-EQ-0077"),
        ),
        (
            "light_table_set_active",
            ("0x000a0007", 2, 2, "INKS-EQ-0078"),
        ),
        ("light_table_add_item", ("0x000a0010", 2, 2, "INKS-EQ-0079")),
        (
            "light_table_update_item_properties",
            ("0x000a0011", 2, 2, "INKS-EQ-0080"),
        ),
        (
            "light_table_update_item",
            ("0x000a0012", 2, 2, "INKS-EQ-0081"),
        ),
        (
            "light_table_remove_item",
            ("0x000a0013", 2, 2, "INKS-EQ-0082"),
        ),
        (
            "light_table_reorder_item",
            ("0x000a0014", 2, 2, "INKS-EQ-0083"),
        ),
        (
            "light_table_bulk_register",
            ("0x000a0016", 2, 1, "INKS-EQ-0084"),
        ),
    ]);
    let manifest_owners = array(member(&manifest, "owners"))
        .iter()
        .filter(|owner| string(member(owner, "owner_milestone")) == OWNER_MILESTONE)
        .map(|owner| (string(member(owner, "command_name")), owner))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(manifest_owners.len(), expected.len());
    let entries = array(member(&draft, "entries"))
        .iter()
        .filter(|entry| string(member(entry, "owner_milestone")) == OWNER_MILESTONE)
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), expected.len());

    for entry in entries {
        let name = string(member(entry, "name"));
        let (primitive_id, schema, semantics, equivalence) = expected[name];
        assert_eq!(string(member(entry, "primitive_id")), primitive_id);
        assert_eq!(number(member(entry, "primitive_schema_version")), schema);
        assert_eq!(number(member(entry, "replay_epoch")), 23);
        assert_eq!(number(member(entry, "semantics_revision")), semantics);
        assert_eq!(string(member(entry, "equivalence_test")), equivalence);
        assert_eq!(string(member(entry, "owner_milestone")), OWNER_MILESTONE);
        assert_eq!(
            string(member(member(entry, "editor"), "family")),
            "light_table"
        );
        for argument in array(member(entry, "arguments")) {
            assert!(type_names.contains(base_type(string(member(argument, "type")))));
        }
        let owner = manifest_owners[name];
        assert_eq!(string(member(owner, "primitive_id")), primitive_id);
        assert_eq!(
            string(member(owner, "planned_equivalence_test")),
            equivalence
        );
    }

    for record_name in [
        "light_table_source",
        "light_table_item_properties",
        "light_table_item_input",
    ] {
        validate_fields(
            member(named(member(&draft, "records"), record_name), "fields"),
            &type_names,
            record_name,
        );
    }
    for (command, argument) in [
        ("light_table_add_item", "input"),
        ("light_table_update_item", "input"),
        ("light_table_bulk_register", "inputs"),
    ] {
        let argument = named(
            member(named(member(&draft, "entries"), command), "arguments"),
            argument,
        );
        let role = member(argument, "asset_role");
        assert_eq!(string(member(role, "name")), "source_rasters");
        assert_eq!(string(member(role, "kind")), "canonical_raster");
        assert_eq!(member(role, "inline"), &Json::Bool(true));
        assert_eq!(member(role, "external"), &Json::Bool(true));
    }

    let expected_results = BTreeMap::from([
        (
            "light_table_create_set",
            ("set", "light_table_set_ref", "scalar"),
        ),
        (
            "light_table_duplicate_set",
            ("set", "light_table_set_ref", "scalar"),
        ),
        (
            "light_table_add_item",
            ("item", "light_table_item_ref", "scalar"),
        ),
        (
            "light_table_bulk_register",
            ("items", "list<light_table_item_ref>", "ordered_list"),
        ),
    ]);
    for (name, (result_name, result_type, cardinality)) in expected_results {
        let results = array(member(named(member(&draft, "entries"), name), "results"));
        assert_eq!(results.len(), 1);
        assert_eq!(string(member(&results[0], "name")), result_name);
        assert_eq!(string(member(&results[0], "type")), result_type);
        assert_eq!(string(member(&results[0], "cardinality")), cardinality);
        assert_eq!(number(member(&results[0], "output_id_ordinal")), 0);
    }

    assert!(
        array(member(&draft, "entries"))
            .iter()
            .all(|entry| string(member(entry, "name")) != "light_table_swap_with_active")
    );
    let excluded = array(member(&manifest, "excluded_primitives"))
        .iter()
        .find(|entry| string(member(entry, "canonical_name")) == "LightTableSwapWithActive")
        .expect("session-only Light Table swap exclusion must exist");
    assert_eq!(string(member(excluded, "reason")), "session_only");
}

#[test]
fn inkscript_legacy_simple_catalog_entries_are_closed_typed_and_owner_exact() {
    let language = load_json("schemas/inkscript/language-v2.json");
    let draft = load_json("schemas/inkscript/catalog-v2.json");
    let manifest = load_json("schemas/inkscript/owner-manifest-v2.json");
    let type_names = composed_catalog_type_names(&language, &draft);

    let expected = BTreeMap::from([
        ("set_layer_properties", ("0x00020005", 2, 2, "INKS-EQ-0006")),
        ("set_plane_properties", ("0x00020015", 2, 2, "INKS-EQ-0011")),
        ("convert_plane", ("0x00020016", 2, 2, "INKS-EQ-0012")),
        ("convert_layer", ("0x00020021", 2, 2, "INKS-EQ-0014")),
        ("mirror_document", ("0x00070001", 2, 2, "INKS-EQ-0060")),
        ("rotate_document", ("0x00070002", 2, 2, "INKS-EQ-0061")),
        ("resize_document", ("0x00070003", 2, 2, "INKS-EQ-0062")),
    ]);
    const OWNER_MILESTONE: &str = concat!("M", "07");
    let manifest_owners = array(member(&manifest, "owners"))
        .iter()
        .filter(|owner| string(member(owner, "owner_milestone")) == OWNER_MILESTONE)
        .map(|owner| (string(member(owner, "command_name")), owner))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(manifest_owners.len(), expected.len());

    let mut actual_names = BTreeSet::new();
    for entry in array(member(&draft, "entries"))
        .iter()
        .filter(|entry| string(member(entry, "owner_milestone")) == OWNER_MILESTONE)
    {
        assert_exact_keys(
            entry,
            &[
                "arguments",
                "cancellation_boundary",
                "editor",
                "equivalence_test",
                "name",
                "owner_milestone",
                "portability",
                "primitive_id",
                "primitive_schema_version",
                "replay_epoch",
                "results",
                "semantics_revision",
                "work",
            ],
        );
        let name = string(member(entry, "name"));
        assert!(
            actual_names.insert(name),
            "duplicate catalog command {name}"
        );
        let (primitive_id, schema, semantics, equivalence) = expected[name];
        assert_eq!(string(member(entry, "primitive_id")), primitive_id);
        assert_eq!(number(member(entry, "primitive_schema_version")), schema);
        assert_eq!(number(member(entry, "replay_epoch")), 23);
        assert_eq!(number(member(entry, "semantics_revision")), semantics);
        assert_eq!(string(member(entry, "owner_milestone")), OWNER_MILESTONE);
        assert_eq!(string(member(entry, "equivalence_test")), equivalence);
        assert!(array(member(entry, "results")).is_empty());

        let owner = manifest_owners[name];
        assert_eq!(string(member(owner, "primitive_id")), primitive_id);
        assert_eq!(number(member(owner, "primitive_schema_version")), schema);
        assert_eq!(number(member(owner, "semantics_revision")), semantics);
        assert_eq!(
            string(member(owner, "planned_equivalence_test")),
            equivalence
        );

        let mut argument_names = BTreeSet::new();
        let mut argument_orders = BTreeSet::new();
        for argument in array(member(entry, "arguments")) {
            assert_exact_keys(
                argument,
                &[
                    "asset_role",
                    "bound",
                    "canonical_order",
                    "default",
                    "name",
                    "nullable",
                    "required",
                    "stable_id_role",
                    "type",
                ],
            );
            let argument_name = string(member(argument, "name"));
            assert!(argument_names.insert(argument_name));
            assert!(argument_orders.insert(number(member(argument, "canonical_order"))));
            assert!(type_names.contains(base_type(string(member(argument, "type")))));
            assert_eq!(member(argument, "required"), &Json::Bool(true));
            assert_eq!(member(argument, "nullable"), &Json::Bool(false));
            assert_eq!(member(argument, "default"), &Json::Null);
            assert!(matches!(member(argument, "bound"), Json::Array(_)));
            assert!(matches!(member(argument, "asset_role"), Json::Null));
        }
        assert!(
            argument_orders
                .iter()
                .copied()
                .eq(0..argument_orders.len() as u64)
        );

        let portability = member(entry, "portability");
        assert_exact_keys(portability, &["default", "rules"]);
        assert!(array(member(portability, "rules")).is_empty());
        let default = member(portability, "default");
        assert_exact_keys(default, &["class", "required_preconditions"]);
        let expected_class = if name.contains("document") {
            "portable"
        } else {
            "requires_binding"
        };
        assert_eq!(string(member(default, "class")), expected_class);

        assert_exact_keys(
            member(entry, "work"),
            &[
                "max_asset_bytes",
                "max_invocations",
                "max_output_growth",
                "max_output_ids",
                "max_work_units",
            ],
        );
        let editor = member(entry, "editor");
        assert_exact_keys(
            editor,
            &["allow_skip_dependents", "family", "legacy_projection"],
        );
        assert_eq!(string(member(editor, "family")), "legacy_simple");
    }
    assert_eq!(actual_names, expected.keys().copied().collect());

    for value in array(member(&draft, "enums")) {
        assert_exact_keys(value, &["members", "name"]);
        assert!(!array(member(value, "members")).is_empty());
    }
    let resize = named(member(&draft, "records"), "document_resize");
    assert_exact_keys(resize, &["fields", "name"]);
    validate_fields(member(resize, "fields"), &type_names, "document_resize");
}

#[test]
fn inkscript_legacy_image_catalog_entries_are_closed_typed_and_owner_exact() {
    let language = load_json("schemas/inkscript/language-v2.json");
    let draft = load_json("schemas/inkscript/catalog-v2.json");
    let manifest = load_json("schemas/inkscript/owner-manifest-v2.json");
    let type_names = composed_catalog_type_names(&language, &draft);
    let expected = BTreeMap::from([
        ("apply_fill", ("0x00050002", 2, 2, "INKS-EQ-0030")),
        (
            "apply_boundary_airbrush",
            ("0x00050011", 2, 2, "INKS-EQ-0033"),
        ),
        ("apply_dust_removal", ("0x00050018", 2, 2, "INKS-EQ-0040")),
        ("apply_filter", ("0x00050020", 2, 2, "INKS-EQ-0043")),
        (
            "replace_raster_colors",
            ("0x00050040", 2, 2, "INKS-EQ-0046"),
        ),
        (
            "separate_raster_colors",
            ("0x00050041", 2, 3, "INKS-EQ-0047"),
        ),
    ]);
    const OWNER_MILESTONE: &str = concat!("M", "08");
    let manifest_owners = array(member(&manifest, "owners"))
        .iter()
        .filter(|owner| string(member(owner, "owner_milestone")) == OWNER_MILESTONE)
        .map(|owner| (string(member(owner, "command_name")), owner))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(manifest_owners.len(), expected.len());

    let entries = array(member(&draft, "entries"))
        .iter()
        .filter(|entry| string(member(entry, "owner_milestone")) == OWNER_MILESTONE)
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), expected.len());
    let mut actual = BTreeSet::new();
    for entry in entries {
        let name = string(member(entry, "name"));
        assert!(actual.insert(name));
        let (primitive_id, schema, semantics, equivalence) = expected[name];
        assert_eq!(string(member(entry, "primitive_id")), primitive_id);
        assert_eq!(number(member(entry, "primitive_schema_version")), schema);
        assert_eq!(number(member(entry, "replay_epoch")), 23);
        assert_eq!(number(member(entry, "semantics_revision")), semantics);
        assert_eq!(string(member(entry, "equivalence_test")), equivalence);
        assert!(array(member(entry, "results")).is_empty());
        assert_eq!(
            string(member(member(entry, "editor"), "family")),
            "legacy_image"
        );
        assert_eq!(
            string(member(
                member(member(entry, "portability"), "default"),
                "class"
            )),
            "requires_binding"
        );
        assert!(matches!(
            string(member(entry, "cancellation_boundary")),
            "before_primitive" | "bounded_work_chunk"
        ));
        let owner = manifest_owners[name];
        assert_eq!(string(member(owner, "primitive_id")), primitive_id);
        assert_eq!(
            string(member(owner, "planned_equivalence_test")),
            equivalence
        );

        let mut orders = BTreeSet::new();
        for argument in array(member(entry, "arguments")) {
            assert!(type_names.contains(base_type(string(member(argument, "type")))));
            assert!(orders.insert(number(member(argument, "canonical_order"))));
        }
        assert!(orders.iter().copied().eq(0..orders.len() as u64));
    }
    assert_eq!(actual, expected.keys().copied().collect());

    for record in array(member(&draft, "records")) {
        let name = string(member(record, "name"));
        validate_fields(member(record, "fields"), &type_names, name);
    }
    let vector_width = array(member(&draft, "entries"))
        .iter()
        .filter(|entry| string(member(entry, "name")) == "vector_correct_width")
        .collect::<Vec<_>>();
    assert_eq!(vector_width.len(), 1);
    assert_eq!(
        string(member(vector_width[0], "owner_milestone")),
        concat!("M", "20")
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
    let language = load_json("schemas/inkscript/language-v2.json");
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
    assert_eq!(
        number(member(
            named(member(&language, "resource_limits"), "inline_asset_bytes"),
            "maximum"
        )),
        inkpod_format::MAX_INKSCRIPT_INLINE_ASSET_BYTES as u64
    );
    assert_eq!(
        number(member(
            named(
                member(&language, "resource_limits"),
                "inline_asset_total_bytes"
            ),
            "maximum"
        )),
        inkpod_format::MAX_INKSCRIPT_INLINE_ASSET_TOTAL_BYTES
    );
    assert_eq!(
        number(member(
            named(member(&language, "resource_limits"), "external_asset_bytes"),
            "maximum"
        )),
        inkpod_format::MAX_INKSCRIPT_EXTERNAL_ASSET_BYTES
    );
    assert_eq!(
        number(member(
            named(member(&language, "resource_limits"), "asset_total_bytes"),
            "maximum"
        )),
        inkpod_format::MAX_INKSCRIPT_ASSET_TOTAL_BYTES
    );

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
    let manifest = load_json("schemas/inkscript/owner-manifest-v2.json");
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
fn inkscript_production_catalog_is_bijective_with_runtime_and_equivalence_evidence() {
    let catalog = load_json("schemas/inkscript/catalog-v2.json");
    let manifest = load_json("schemas/inkscript/owner-manifest-v2.json");
    let entries = array(member(&catalog, "entries"));
    let owners = array(member(&manifest, "owners"));
    assert_eq!(entries.len(), 84);
    assert_eq!(owners.len(), entries.len());
    assert_eq!(
        entries.len(),
        inkpod_format::INKSCRIPT_PRODUCTION_CATALOG_COMMAND_COUNT
    );
    assert_eq!(
        inkpod_format::INKSCRIPT_PRODUCTION_CATALOG_FINGERPRINT,
        0x988b_9725_dbdc_a0a2
    );

    let owners_by_command = owners
        .iter()
        .map(|owner| (string(member(owner, "command_name")), owner))
        .collect::<BTreeMap<_, _>>();
    let primitive_entries = parse_catalog_entries(&repository());
    let replayable_by_id = primitive_entries
        .iter()
        .filter(|entry| entry.replayable)
        .map(|entry| (entry.id.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let compile_source =
        fs::read_to_string(repository().join("rust/inkpod-core/src/script/compile.rs"))
            .expect("production compiler must be readable");
    let mut adapter_source = String::new();
    for entry in fs::read_dir(repository().join("rust/inkpod-core/src/primitive"))
        .expect("primitive source directory must be readable")
    {
        let path = entry
            .expect("primitive source entry must be readable")
            .path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("inkscript") && name.ends_with(".rs"))
        {
            adapter_source.push_str(
                &fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display())),
            );
        }
    }

    let mut commands = BTreeSet::new();
    let mut evidence = BTreeSet::new();
    for entry in entries {
        assert_exact_keys(
            entry,
            &[
                "arguments",
                "cancellation_boundary",
                "editor",
                "equivalence_test",
                "name",
                "owner_milestone",
                "portability",
                "primitive_id",
                "primitive_schema_version",
                "replay_epoch",
                "results",
                "semantics_revision",
                "work",
            ],
        );
        let command = string(member(entry, "name"));
        let test_id = string(member(entry, "equivalence_test"));
        assert!(commands.insert(command), "duplicate command {command}");
        assert!(
            evidence.insert(test_id.to_owned()),
            "duplicate equivalence ID {test_id}"
        );

        let owner = owners_by_command
            .get(command)
            .unwrap_or_else(|| panic!("catalog command {command} has no owner"));
        for field in [
            "primitive_id",
            "primitive_schema_version",
            "semantics_revision",
            "owner_milestone",
        ] {
            assert_eq!(
                member(entry, field),
                member(owner, field),
                "{command}.{field}"
            );
        }
        assert_eq!(
            member(entry, "equivalence_test"),
            member(owner, "planned_equivalence_test"),
            "{command}.equivalence_test"
        );
        let primitive_id = string(member(entry, "primitive_id"));
        let primitive = replayable_by_id
            .get(primitive_id)
            .unwrap_or_else(|| panic!("catalog command {command} has no replayable primitive"));
        assert_eq!(
            number(member(entry, "primitive_schema_version")),
            primitive.schema_version
        );
        assert_eq!(
            number(member(entry, "semantics_revision")),
            primitive.semantics_revision
        );

        let literal = format!("\"{command}\"");
        assert!(
            compile_source.contains(&literal),
            "catalog command {command} has no runtime catalog declaration"
        );
        assert!(
            adapter_source.contains(&literal),
            "catalog command {command} has no typed adapter declaration"
        );
        assert!(
            command
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
            "catalog command names cannot be derived Debug spellings"
        );
        assert_exact_keys(member(entry, "portability"), &["default", "rules"]);
        assert_exact_keys(
            member(entry, "work"),
            &[
                "max_asset_bytes",
                "max_invocations",
                "max_output_growth",
                "max_output_ids",
                "max_work_units",
            ],
        );
        assert_exact_keys(
            member(entry, "editor"),
            &["allow_skip_dependents", "family", "legacy_projection"],
        );
    }
    assert_eq!(commands.len(), replayable_by_id.len());
    assert_eq!(
        evidence,
        (1..=84)
            .map(|index| format!("INKS-EQ-{index:04}"))
            .collect::<BTreeSet<_>>()
    );

    let execution_tests =
        fs::read_to_string(repository().join("rust/inkpod-core/src/script/tests.rs"))
            .expect("script execution evidence must be readable");
    for evidence_name in [
        "staged_memory_and_native_dry_runs_match_the_direct_canonical_route",
        "document_tree_create_results_feed_later_steps_and_round_trip_history",
        "metadata_color_guide_results_round_trip_native_history_ids_and_savepoints",
        "stroke_geometry_import_execute_typed_assets_and_round_trip_native_history",
        "fill_gradient_execute_native_depth_q16_selection_tile_boundary_and_reopen",
        "gesture_alpha_adjustment_execute_matches_direct_and_round_trips_native_history",
        "selection_family_bounds_results_direct_equivalence_and_native_reopen",
        "vector_catalog_results_index_roles_direct_equivalence_and_native_reopen",
        "annotation_frame_catalog_results_direct_equivalence_and_native_reopen",
        "light_table_catalog_results_assets_direct_replay_and_native_reopen_are_exact",
    ] {
        assert!(
            execution_tests.contains(evidence_name),
            "missing family execution/replay evidence {evidence_name}"
        );
    }
}

fn fnv1a64_normalized(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .filter(|byte| **byte != b'\r')
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

#[test]
fn inkscript_generated_command_reference_has_no_drift() {
    let catalog = load_json("schemas/inkscript/catalog-v2.json");
    let reference_path = repository().join("docs/inkscript-command-reference.md");
    let bytes = fs::read(&reference_path).expect("generated command reference must be readable");
    assert_eq!(fnv1a64_normalized(&bytes), 0x47bf_3307_0c4c_c90d);
    let reference = std::str::from_utf8(&bytes).expect("reference must be UTF-8");
    assert!(reference.starts_with(
        "<!-- @generated by scripts/generate_inkscript_reference.py; do not edit. -->\n"
    ));
    assert!(reference.contains("| Catalog FNV-1a drift fingerprint | `988b9725dbdca0a2` |"));
    let headings = reference
        .lines()
        .filter_map(|line| line.strip_prefix("### `")?.strip_suffix('`'))
        .collect::<Vec<_>>();
    let expected = array(member(&catalog, "entries"))
        .iter()
        .map(|entry| string(member(entry, "name")))
        .collect::<Vec<_>>();
    assert_eq!(headings, expected);
    for entry in array(member(&catalog, "entries")) {
        let command = string(member(entry, "name"));
        let primitive = string(member(entry, "primitive_id"));
        let evidence = string(member(entry, "equivalence_test"));
        let heading = format!("### `{command}`\n");
        let section = reference
            .split_once(&heading)
            .unwrap_or_else(|| panic!("missing command reference section {command}"))
            .1
            .split("\n### `")
            .next()
            .expect("command reference section must terminate");
        assert!(section.contains(&format!("Primitive: `{primitive}`")));
        assert!(section.contains(&format!("equivalence `{evidence}`")));
        assert!(section.contains("- Arguments:"));
        assert!(section.contains("- Results:"));
        assert!(section.contains("- Portability:"));
        assert!(section.contains("- Work:"));
        assert!(section.contains("- Editor:"));
    }
}

#[test]
fn inkscript_versions_and_traceability_match_repository_contracts() {
    let language = load_json("schemas/inkscript/language-v2.json");
    let draft = load_json("schemas/inkscript/catalog-v2.json");
    let manifest = load_json("schemas/inkscript/owner-manifest-v2.json");
    for value in [&language, &draft] {
        assert_eq!(number(member(value, "registry_schema_version")), 2);
        assert_eq!(number(member(value, "file_version")), 2);
        assert_eq!(number(member(value, "required_replay_epoch")), 23);
    }
    assert_eq!(number(member(&manifest, "registry_schema_version")), 2);
    assert_eq!(number(member(&language, "procedure_catalog_version")), 2);
    assert_eq!(number(member(&draft, "catalog_version")), 2);

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
    assert_eq!(number(member(contract, "inkscript_file_version")), 2);
    assert_eq!(number(member(contract, "procedure_catalog_version")), 2);
    assert_eq!(number(member(contract, "replay_epoch")), 23);
    assert_eq!(number(member(contract, "inkpod_top_level_version")), 26);
    assert_eq!(number(member(contract, "c_abi_version")), 16);

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
    assert!(header.contains("#define INKPOD_ABI_VERSION UINT32_C(16)"));

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
fn inkscript_removed_draft_is_unreachable_from_production() {
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
            !text.contains("catalog-v2.draft.json")
                && !text.contains("inkpod.inkscript.catalog-draft"),
            "production source {} reaches the removed InkScript draft",
            source.display()
        );
    }
}

#[test]
fn inkscript_private_typed_models_remain_unreachable_from_core_ffi_and_windows() {
    let repository = repository();
    let mut sources = Vec::new();
    for root in ["rust/inkpod-ffi/src", "apps/windows", "include"] {
        collect_production_sources(&repository.join(root), &mut sources);
    }

    let core_source = repository.join("rust/inkpod-core/src");
    let private_adapters = [
        core_source.join("primitive/inkscript.rs"),
        core_source.join("primitive/inkscript_batch.rs"),
        core_source.join("primitive/inkscript_document_tree.rs"),
        core_source.join("primitive/inkscript_metadata.rs"),
        core_source.join("primitive/inkscript_reference.rs"),
        core_source.join("primitive/inkscript_stroke_geometry.rs"),
        core_source.join("primitive/inkscript_fill_gradient.rs"),
        core_source.join("primitive/inkscript_gesture_adjustment.rs"),
        core_source.join("primitive/inkscript_selection_floating.rs"),
        core_source.join("primitive/inkscript_vector.rs"),
        core_source.join("primitive/inkscript_annotation_frame.rs"),
        core_source.join("primitive/inkscript_light_table.rs"),
    ];
    let private_compiler = core_source.join("script");
    let mut core_sources = Vec::new();
    collect_production_sources(&core_source, &mut core_sources);
    for source in core_sources {
        if private_adapters.contains(&source) || source.starts_with(&private_compiler) {
            continue;
        }
        let text = fs::read_to_string(&source)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()));
        assert!(
            !text.contains("InkScriptTypedStep")
                && !text.contains("build_inkscript_declaration_model"),
            "Core source {} bypasses the private catalog adapter owners",
            source.display()
        );
    }
    let primitive_root = fs::read_to_string(core_source.join("primitive/mod.rs"))
        .expect("primitive module root must be readable");
    assert!(primitive_root.contains("mod inkscript;"));
    assert!(!primitive_root.contains("pub mod inkscript"));
    assert!(primitive_root.contains("mod inkscript_batch;"));
    assert!(!primitive_root.contains("pub mod inkscript_batch"));
    assert!(primitive_root.contains("mod inkscript_metadata;"));
    assert!(!primitive_root.contains("pub mod inkscript_metadata"));
    assert!(primitive_root.contains("mod inkscript_stroke_geometry;"));
    assert!(!primitive_root.contains("pub mod inkscript_stroke_geometry"));
    assert!(primitive_root.contains("mod inkscript_fill_gradient;"));
    assert!(!primitive_root.contains("pub mod inkscript_fill_gradient"));
    assert!(primitive_root.contains("mod inkscript_gesture_adjustment;"));
    assert!(!primitive_root.contains("pub mod inkscript_gesture_adjustment"));
    assert!(primitive_root.contains("mod inkscript_selection_floating;"));
    assert!(!primitive_root.contains("pub mod inkscript_selection_floating"));
    assert!(primitive_root.contains("mod inkscript_vector;"));
    assert!(!primitive_root.contains("pub mod inkscript_vector"));
    assert!(primitive_root.contains("mod inkscript_annotation_frame;"));
    assert!(!primitive_root.contains("pub mod inkscript_annotation_frame"));
    assert!(primitive_root.contains("mod inkscript_light_table;"));
    assert!(!primitive_root.contains("pub mod inkscript_light_table"));
    let core_public_root =
        fs::read_to_string(core_source.join("lib.rs")).expect("Core public root must be readable");
    assert!(!core_public_root.contains("LegacySimpleScriptStep"));
    assert!(!core_public_root.contains("LegacyImageScriptStep"));
    assert!(!core_public_root.contains("StrokeGeometryImportAction"));
    assert!(!core_public_root.contains("FillGradientScriptStep"));
    assert!(!core_public_root.contains("GestureAdjustmentScriptAction"));
    assert!(!core_public_root.contains("SelectionFloatingScriptAction"));
    assert!(!core_public_root.contains("VectorScriptStep"));
    assert!(!core_public_root.contains("AnnotationFrameScriptStep"));
    assert!(!core_public_root.contains("LightTableScriptAction"));
    assert!(!core_public_root.contains("FrozenScriptAssets"));
    assert!(!core_public_root.contains("AuthorizedAssetStream"));
    assert!(!core_public_root.contains("freeze_inkscript_assets"));
    let execution_bridge = repository.join("rust/inkpod-ffi/src/inkscript_execution.rs");
    for source in sources {
        let contents = fs::read(&source)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()));
        let text = String::from_utf8_lossy(&contents);
        let is_execution_bridge = source == execution_bridge;
        if is_execution_bridge {
            assert!(text.contains("inkpod_core::inkscript::abi_bridge::*"));
            assert!(!text.contains("inkpod_core::script"));
        }
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
                && !text.contains("InkScriptInitialDocumentSnapshot")
                && !text.contains("FrozenScriptAssets")
                && (!text.contains("AuthorizedAssetStream") || is_execution_bridge)
                && !text.contains("freeze_inkscript_assets"),
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
