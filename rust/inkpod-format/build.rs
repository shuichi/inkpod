use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(u64),
    String(String),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
}

struct Parser<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Parser<'a> {
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
            let byte = self.peek().ok_or_else(|| "short JSON string".to_owned())?;
            if byte == b'"' {
                self.cursor += 1;
                return Ok(result);
            }
            if byte == b'\\' {
                self.cursor += 1;
                let escaped = self.peek().ok_or_else(|| "short JSON escape".to_owned())?;
                self.cursor += 1;
                match escaped {
                    b'"' => result.push('"'),
                    b'\\' => result.push('\\'),
                    b'/' => result.push('/'),
                    b'b' => result.push('\u{8}'),
                    b'f' => result.push('\u{c}'),
                    b'n' => result.push('\n'),
                    b'r' => result.push('\r'),
                    b't' => result.push('\t'),
                    b'u' => result.push(self.unicode_escape()?),
                    _ => return Err(format!("invalid JSON escape at {}", self.cursor - 1)),
                }
                continue;
            }
            if byte < 0x20 {
                return Err(format!("control byte in JSON string at {}", self.cursor));
            }
            let tail = std::str::from_utf8(&self.bytes[self.cursor..])
                .map_err(|error| format!("invalid JSON UTF-8: {error}"))?;
            let character = tail
                .chars()
                .next()
                .ok_or_else(|| "short JSON string".to_owned())?;
            result.push(character);
            self.cursor += character.len_utf8();
        }
    }

    fn unicode_escape(&mut self) -> Result<char, String> {
        let high = self.hex_quad()?;
        let scalar = if (0xd800..=0xdbff).contains(&high) {
            self.expect(b'\\')?;
            self.expect(b'u')?;
            let low = self.hex_quad()?;
            if !(0xdc00..=0xdfff).contains(&low) {
                return Err("invalid JSON surrogate pair".to_owned());
            }
            0x1_0000 + ((u32::from(high) - 0xd800) << 10) + (u32::from(low) - 0xdc00)
        } else if (0xdc00..=0xdfff).contains(&high) {
            return Err("unpaired JSON low surrogate".to_owned());
        } else {
            u32::from(high)
        };
        char::from_u32(scalar).ok_or_else(|| "invalid JSON scalar".to_owned())
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
            .map_err(|error| format!("invalid JSON number: {error}"))
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
            Err(format!("expected JSON byte at {}", self.cursor))
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

fn main() {
    if let Err(error) = generate() {
        panic!("failed to generate exact-current InkScript registry projection: {error}");
    }
}

fn generate() -> Result<(), String> {
    let crate_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("missing manifest dir")?);
    let language = crate_dir.join("../../schemas/inkscript/language-v2.json");
    let catalog = crate_dir.join("../../schemas/inkscript/catalog-v3.json");
    println!("cargo:rerun-if-changed={}", language.display());
    println!("cargo:rerun-if-changed={}", catalog.display());
    println!("cargo:rerun-if-changed=build.rs");
    let root = Parser::parse(&fs::read(&language).map_err(|error| error.to_string())?)?;
    let registry_schema_version = number(member(&root, "registry_schema_version")?)?;
    let file_version = number(member(&root, "file_version")?)?;
    let procedure_catalog_version = number(member(&root, "procedure_catalog_version")?)?;
    let required_replay_epoch = number(member(&root, "required_replay_epoch")?)?;
    if string(member(&root, "kind")?)? != "inkpod.inkscript.language"
        || registry_schema_version != 2
        || file_version != 2
        || procedure_catalog_version != 3
        || required_replay_epoch != 24
    {
        return Err("language registry identity/version mismatch".to_owned());
    }
    let catalog_bytes = fs::read(&catalog).map_err(|error| error.to_string())?;
    let catalog_root = Parser::parse(&catalog_bytes)?;
    let catalog_version = number(member(&catalog_root, "catalog_version")?)?;
    let command_count = array(member(&catalog_root, "entries")?)?.len();
    let catalog_fingerprint = fnv1a64(&catalog_bytes);
    const FROZEN_CATALOG_V3_FNV1A64: u64 = 0xd94d_c4d8_adbc_8040;
    if string(member(&catalog_root, "kind")?)? != "inkpod.inkscript.catalog"
        || number(member(&catalog_root, "registry_schema_version")?)? != 2
        || number(member(&catalog_root, "file_version")?)? != file_version
        || catalog_version != procedure_catalog_version
        || number(member(&catalog_root, "required_replay_epoch")?)? != required_replay_epoch
        || !boolean(member(&catalog_root, "production")?)?
        || command_count != 75
        || catalog_fingerprint != FROZEN_CATALOG_V3_FNV1A64
    {
        return Err(format!(
            "production catalog v3 identity/freeze mismatch: version={catalog_version}, commands={command_count}, fingerprint={catalog_fingerprint:016x}"
        ));
    }

    let mut generated = String::from(
        "// @generated from the exact-current InkScript language and production catalog registries; do not edit.\n",
    );
    writeln!(
        generated,
        "const GENERATED_PROCEDURE_CATALOG_VERSION: u32 = {procedure_catalog_version};"
    )
    .unwrap();
    writeln!(
        generated,
        "const GENERATED_REQUIRED_REPLAY_EPOCH: u32 = {required_replay_epoch};\n"
    )
    .unwrap();
    writeln!(
        generated,
        "const GENERATED_PRODUCTION_CATALOG_COMMAND_COUNT: usize = {command_count};"
    )
    .unwrap();
    writeln!(
        generated,
        "const GENERATED_PRODUCTION_CATALOG_FINGERPRINT: u64 = 0x{catalog_fingerprint:016x};\n"
    )
    .unwrap();
    emit_type_names(&mut generated, &root)?;
    emit_types(&mut generated, &root)?;
    emit_enums(&mut generated, &root)?;
    emit_constructors(&mut generated, &root)?;
    emit_section_order(&mut generated, &root)?;
    emit_collection(
        &mut generated,
        "GENERATED_RECORDS",
        array(member(&root, "records")?)?,
        "fields",
    )?;
    emit_selectors(&mut generated, &root)?;
    emit_assertions(&mut generated, &root)?;
    emit_id_namespaces(&mut generated, &root)?;
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?)
        .join("inkscript_language_schema.rs");
    fs::write(output, generated).map_err(|error| error.to_string())
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .filter(|byte| **byte != b'\r')
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

fn emit_section_order(output: &mut String, root: &Json) -> Result<(), String> {
    let canonicalization = member(root, "canonicalization")?;
    let section_order = array(member(canonicalization, "section_order")?)?;
    writeln!(output, "const GENERATED_SECTION_ORDER: &[&str] = &[").unwrap();
    for section in section_order {
        writeln!(output, "    {:?},", string(section)?).unwrap();
    }
    writeln!(output, "];\n").unwrap();
    Ok(())
}

fn emit_type_names(output: &mut String, root: &Json) -> Result<(), String> {
    let mut names = std::collections::BTreeSet::new();
    for collection in ["types", "enums", "records"] {
        for entry in array(member(root, collection)?)? {
            let name = string(member(entry, "name")?)?;
            if !names.insert(name) {
                return Err(format!("duplicate language type {name:?}"));
            }
        }
    }
    writeln!(output, "const GENERATED_TYPE_NAMES: &[&str] = &[").unwrap();
    for name in names {
        writeln!(output, "    {name:?},").unwrap();
    }
    writeln!(output, "];\n").unwrap();
    Ok(())
}

fn emit_types(output: &mut String, root: &Json) -> Result<(), String> {
    writeln!(output, "const GENERATED_TYPES: &[GeneratedType] = &[").unwrap();
    for entry in array(member(root, "types")?)? {
        let name = string(member(entry, "name")?)?;
        let kind = string(member(entry, "kind")?)?;
        writeln!(
            output,
            "    GeneratedType {{ name: {name:?}, kind: {kind:?} }},"
        )
        .unwrap();
    }
    for entry in array(member(root, "enums")?)? {
        let name = string(member(entry, "name")?)?;
        writeln!(
            output,
            "    GeneratedType {{ name: {name:?}, kind: \"enum\" }},"
        )
        .unwrap();
    }
    for entry in array(member(root, "records")?)? {
        let name = string(member(entry, "name")?)?;
        writeln!(
            output,
            "    GeneratedType {{ name: {name:?}, kind: \"record\" }},"
        )
        .unwrap();
    }
    writeln!(output, "];\n").unwrap();
    Ok(())
}

fn emit_enums(output: &mut String, root: &Json) -> Result<(), String> {
    writeln!(output, "const GENERATED_ENUMS: &[GeneratedEnum] = &[").unwrap();
    for entry in array(member(root, "enums")?)? {
        let name = string(member(entry, "name")?)?;
        let members = string_slice_expression(array(member(entry, "members")?)?)?;
        writeln!(
            output,
            "    GeneratedEnum {{ name: {name:?}, members: {members} }},"
        )
        .unwrap();
    }
    writeln!(output, "];\n").unwrap();
    Ok(())
}

fn emit_constructors(output: &mut String, root: &Json) -> Result<(), String> {
    writeln!(
        output,
        "const GENERATED_CONSTRUCTORS: &[InkScriptConstructorSchema] = &["
    )
    .unwrap();
    for entry in array(member(root, "constructors")?)? {
        let name = string(member(entry, "name")?)?;
        let result = string(member(entry, "result")?)?;
        writeln!(
            output,
            "    InkScriptConstructorSchema {{ name: {name:?}, result: {result:?}, arguments: &["
        )
        .unwrap();
        for argument in array(member(entry, "arguments")?)? {
            let argument_name = string(member(argument, "name")?)?;
            let type_name = string(member(argument, "type")?)?;
            let constraints = string_slice_expression(array(member(argument, "constraints")?)?)?;
            writeln!(
                output,
                "        InkScriptConstructorArgumentSchema {{ name: {argument_name:?}, type_name: {type_name:?}, constraints: {constraints} }},"
            )
            .unwrap();
        }
        writeln!(output, "    ] }},").unwrap();
    }
    writeln!(output, "];\n").unwrap();
    Ok(())
}

fn emit_selectors(output: &mut String, root: &Json) -> Result<(), String> {
    writeln!(
        output,
        "const GENERATED_SELECTORS: &[GeneratedSelector] = &["
    )
    .unwrap();
    for entry in array(member(root, "selector_entities")?)? {
        let name = string(member(entry, "name")?)?;
        let reference_type = string(member(entry, "reference_type")?)?;
        let owner = selector_owner_expression(string(member(entry, "owner")?)?)?;
        let initial_order = selector_order_expression(string(member(entry, "initial_order")?)?)?;
        writeln!(
            output,
            "    GeneratedSelector {{ name: {name:?}, reference_type: {reference_type:?}, owner: {owner}, initial_order: {initial_order}, fields: &["
        )
        .unwrap();
        emit_fields(output, array(member(entry, "filters")?)?)?;
        writeln!(output, "    ] }},").unwrap();
    }
    writeln!(output, "];\n").unwrap();
    Ok(())
}

fn emit_assertions(output: &mut String, root: &Json) -> Result<(), String> {
    writeln!(
        output,
        "const GENERATED_ASSERTIONS: &[GeneratedAssertion] = &["
    )
    .unwrap();
    for entry in array(member(root, "assert_kinds")?)? {
        let name = string(member(entry, "name")?)?;
        let comparison = assert_comparison_expression(string(member(entry, "comparison")?)?)?;
        if string(member(entry, "failure")?)? != "item_failure_without_mutation" {
            return Err(format!(
                "unsupported assertion failure contract for {name:?}"
            ));
        }
        writeln!(
            output,
            "    GeneratedAssertion {{ name: {name:?}, comparison: {comparison}, fields: &["
        )
        .unwrap();
        emit_fields(output, array(member(entry, "fields")?)?)?;
        writeln!(output, "    ] }},").unwrap();
    }
    writeln!(output, "];\n").unwrap();
    Ok(())
}

fn emit_id_namespaces(output: &mut String, root: &Json) -> Result<(), String> {
    writeln!(
        output,
        "const GENERATED_ID_NAMESPACES: &[GeneratedIdNamespace] = &["
    )
    .unwrap();
    for entry in array(member(root, "persistent_id_namespaces")?)? {
        let tag = string(member(entry, "tag")?)?;
        let order = number(member(entry, "order")?)?;
        writeln!(
            output,
            "    GeneratedIdNamespace {{ tag: {tag:?}, order: {order} }},"
        )
        .unwrap();
    }
    writeln!(output, "];\n").unwrap();
    Ok(())
}

fn emit_collection(
    output: &mut String,
    const_name: &str,
    entries: &[Json],
    fields_member: &str,
) -> Result<(), String> {
    writeln!(output, "const {const_name}: &[GeneratedRecord] = &[").unwrap();
    for entry in entries {
        let name = string(member(entry, "name")?)?;
        writeln!(output, "    GeneratedRecord {{ name: {name:?}, fields: &[").unwrap();
        emit_fields(output, array(member(entry, fields_member)?)?)?;
        writeln!(output, "    ] }},").unwrap();
    }
    writeln!(output, "];\n").unwrap();
    Ok(())
}

fn emit_fields(output: &mut String, fields: &[Json]) -> Result<(), String> {
    for field in fields {
        let field_name = string(member(field, "name")?)?;
        let type_name = string(member(field, "type")?)?;
        let required = boolean(member(field, "required")?)?;
        let order = number(member(field, "canonical_order")?)?;
        let default = default_expression(member(field, "default")?, required)?;
        let constraints = string_slice_expression(array(member(field, "constraints")?)?)?;
        writeln!(
            output,
            "        InkScriptFieldSchema {{ name: {field_name:?}, type_name: {type_name:?}, required: {required}, default: {default}, canonical_order: {order}, constraints: {constraints} }},"
        )
        .unwrap();
    }
    Ok(())
}

fn selector_owner_expression(value: &str) -> Result<&'static str, String> {
    match value {
        "document" => Ok("InkScriptSelectorOwner::Document"),
        "layer" => Ok("InkScriptSelectorOwner::Layer"),
        "plane" => Ok("InkScriptSelectorOwner::Plane"),
        "light_table_set" => Ok("InkScriptSelectorOwner::LightTableSet"),
        _ => Err(format!("unknown selector owner {value:?}")),
    }
}

fn selector_order_expression(value: &str) -> Result<&'static str, String> {
    match value {
        "document_tree" => Ok("InkScriptSelectorOrder::DocumentTree"),
        "guide_order" => Ok("InkScriptSelectorOrder::Guide"),
        "singleton" => Ok("InkScriptSelectorOrder::Singleton"),
        "light_table_order" => Ok("InkScriptSelectorOrder::LightTable"),
        _ => Err(format!("unknown selector initial order {value:?}")),
    }
}

fn assert_comparison_expression(value: &str) -> Result<&'static str, String> {
    match value {
        "exact_typed_field_equality" => Ok("InkScriptAssertComparison::DocumentFields"),
        "exact_object_property_equality" => Ok("InkScriptAssertComparison::ObjectProperties"),
        "exact_selection_state_equality" => Ok("InkScriptAssertComparison::SelectionState"),
        _ => Err(format!("unknown assert comparison {value:?}")),
    }
}

fn string_slice_expression(values: &[Json]) -> Result<String, String> {
    let mut result = String::from("&[");
    for value in values {
        write!(result, "{:?},", string(value)?).unwrap();
    }
    result.push(']');
    Ok(result)
}

fn default_expression(value: &Json, required: bool) -> Result<String, String> {
    if required {
        return Ok("None".to_owned());
    }
    Ok(match value {
        Json::Null => "Some(InkScriptSchemaDefault::None)".to_owned(),
        Json::Bool(value) => {
            format!("Some(InkScriptSchemaDefault::Boolean({value}))")
        }
        Json::String(value) => {
            format!("Some(InkScriptSchemaDefault::Enum({value:?}))")
        }
        Json::Array(values) if values.is_empty() => {
            "Some(InkScriptSchemaDefault::EmptyList)".to_owned()
        }
        Json::Object(values) if values.is_empty() => {
            "Some(InkScriptSchemaDefault::EmptyRecord)".to_owned()
        }
        _ => return Err("unsupported non-empty language registry default".to_owned()),
    })
}

fn object(value: &Json) -> Result<&BTreeMap<String, Json>, String> {
    match value {
        Json::Object(value) => Ok(value),
        _ => Err("expected JSON object".to_owned()),
    }
}

fn array(value: &Json) -> Result<&[Json], String> {
    match value {
        Json::Array(value) => Ok(value),
        _ => Err("expected JSON array".to_owned()),
    }
}

fn string(value: &Json) -> Result<&str, String> {
    match value {
        Json::String(value) => Ok(value),
        _ => Err("expected JSON string".to_owned()),
    }
}

fn number(value: &Json) -> Result<u64, String> {
    match value {
        Json::Number(value) => Ok(*value),
        _ => Err("expected JSON number".to_owned()),
    }
}

fn boolean(value: &Json) -> Result<bool, String> {
    match value {
        Json::Bool(value) => Ok(*value),
        _ => Err("expected JSON boolean".to_owned()),
    }
}

fn member<'a>(value: &'a Json, name: &str) -> Result<&'a Json, String> {
    object(value)?
        .get(name)
        .ok_or_else(|| format!("missing JSON member {name:?}"))
}
