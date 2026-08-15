use super::parser::InkScriptDocumentKind;
use super::schema::{
    InkScriptFieldSchema, InkScriptSchemaView, InkScriptSemanticError, InkScriptSemanticErrorCode,
};
use super::source::INKSCRIPT_FILE_VERSION;
use super::syntax::{
    InkScriptInput, InkScriptInputKind, InkScriptProgramStatement, InkScriptRecord,
    InkScriptReferenceSegment, InkScriptSemanticDocument, InkScriptSemanticSection,
    InkScriptTypeReference, InkScriptValue, enum_field, error, normalize_record, unwrap_type,
};

/// Emits canonical BOM-free UTF-8 bytes for a validated complete file or fragment.
///
/// The schema is mandatory. Emission revalidates every closed record so an AST composed under a
/// different command view cannot silently fall back to source or dictionary field order.
pub fn emit_inkscript_canonical(
    document: &InkScriptSemanticDocument,
    schema: &InkScriptSchemaView<'_>,
) -> Result<Vec<u8>, InkScriptSemanticError> {
    let mut emitter = Emitter {
        output: String::new(),
        schema,
    };
    emitter.document(document)?;
    Ok(emitter.output.into_bytes())
}

struct Emitter<'a, 'schema> {
    output: String,
    schema: &'a InkScriptSchemaView<'schema>,
}

impl Emitter<'_, '_> {
    fn document(
        &mut self,
        document: &InkScriptSemanticDocument,
    ) -> Result<(), InkScriptSemanticError> {
        let header = match document.kind {
            InkScriptDocumentKind::File => "inkscript",
            InkScriptDocumentKind::Fragment => "inkscript_fragment",
            InkScriptDocumentKind::Unknown => {
                return Err(error(InkScriptSemanticErrorCode::InvalidSyntax, "header"));
            }
        };
        self.output
            .push_str(&format!("{header} {INKSCRIPT_FILE_VERSION};\n"));
        for section in &document.sections {
            self.output.push('\n');
            self.section(section)?;
        }
        Ok(())
    }

    fn section(
        &mut self,
        section: &InkScriptSemanticSection,
    ) -> Result<(), InkScriptSemanticError> {
        match section {
            InkScriptSemanticSection::Requires(record) => {
                self.output.push_str("requires ");
                self.named_record(record, "requires_record", 0)?;
                self.output.push('\n');
            }
            InkScriptSemanticSection::Meta(record) => {
                self.output.push_str("meta ");
                self.named_record(record, "meta_record", 0)?;
                self.output.push('\n');
            }
            InkScriptSemanticSection::Inputs(inputs) => self.inputs(inputs)?,
            InkScriptSemanticSection::Parameters(parameters) => {
                self.output.push_str("parameters {");
                if parameters.is_empty() {
                    self.output.push_str("}\n");
                    return Ok(());
                }
                self.output.push('\n');
                for parameter in parameters {
                    self.indent(1);
                    self.output.push_str("param ");
                    self.output.push_str(&parameter.name);
                    self.output.push_str(": ");
                    self.type_reference(&parameter.declared_type);
                    self.output.push_str(" = ");
                    self.value(
                        &parameter.default_value,
                        &parameter.declared_type.schema_name(),
                        1,
                    )?;
                    if !parameter.metadata.0.is_empty() {
                        self.output.push(' ');
                        self.named_record(&parameter.metadata, "parameter_metadata", 1)?;
                    }
                    self.output.push_str(";\n");
                }
                self.output.push_str("}\n");
            }
            InkScriptSemanticSection::Bindings(bindings) => {
                self.output.push_str("bindings {");
                if bindings.is_empty() {
                    self.output.push_str("}\n");
                    return Ok(());
                }
                self.output.push('\n');
                for binding in bindings {
                    self.indent(1);
                    self.output.push_str("let ");
                    self.output.push_str(&binding.name);
                    self.output.push_str(" = select ");
                    self.output.push_str(&binding.entity);
                    self.output.push(' ');
                    let fields = self.schema.selector(&binding.entity).ok_or_else(|| {
                        error(
                            InkScriptSemanticErrorCode::UnknownRecordSchema,
                            &binding.entity,
                        )
                    })?;
                    self.record(&binding.selector, fields, 1, &binding.entity)?;
                    self.output.push_str(";\n");
                }
                self.output.push_str("}\n");
            }
            InkScriptSemanticSection::Program(statements) => self.program(statements)?,
            InkScriptSemanticSection::Output(record) => {
                self.output.push_str("output ");
                let schema_name = output_schema(record)?;
                self.named_record(record, schema_name, 0)?;
                self.output.push('\n');
            }
            InkScriptSemanticSection::Execution(record) => {
                self.output.push_str("execution ");
                self.named_record(record, "execution_record", 0)?;
                self.output.push('\n');
            }
            InkScriptSemanticSection::Assets(assets) => {
                self.output.push_str("assets {");
                if assets.is_empty() {
                    self.output.push_str("}\n");
                    return Ok(());
                }
                self.output.push('\n');
                for asset in assets {
                    self.indent(1);
                    self.output.push_str("asset ");
                    self.output.push_str(&asset.name);
                    self.output.push(' ');
                    self.named_record(&asset.body, "canonical_raster_asset", 1)?;
                    self.output.push_str(";\n");
                }
                self.output.push_str("}\n");
            }
        }
        Ok(())
    }

    fn inputs(&mut self, inputs: &[InkScriptInput]) -> Result<(), InkScriptSemanticError> {
        self.output.push_str("inputs {");
        if inputs.is_empty() {
            self.output.push_str("}\n");
            return Ok(());
        }
        self.output.push('\n');
        for input in inputs {
            self.indent(1);
            let schema_name = match input.kind {
                InkScriptInputKind::File => {
                    self.output.push_str("file ");
                    self.string(input.path.as_deref().ok_or_else(|| {
                        error(InkScriptSemanticErrorCode::InvalidSyntax, "input.path")
                    })?);
                    "file_input_options"
                }
                InkScriptInputKind::Folder => {
                    self.output.push_str("folder ");
                    self.string(input.path.as_deref().ok_or_else(|| {
                        error(InkScriptSemanticErrorCode::InvalidSyntax, "input.path")
                    })?);
                    "folder_input_options"
                }
                InkScriptInputKind::CurrentDocument => {
                    self.output.push_str("current_document");
                    "current_document_input_options"
                }
                InkScriptInputKind::CurrentSequence => {
                    self.output.push_str("current_sequence");
                    "current_sequence_input_options"
                }
            };
            if !input.options.0.is_empty() {
                self.output.push(' ');
                self.named_record(&input.options, schema_name, 1)?;
            }
            self.output.push_str(";\n");
        }
        self.output.push_str("}\n");
        Ok(())
    }

    fn program(
        &mut self,
        statements: &[InkScriptProgramStatement],
    ) -> Result<(), InkScriptSemanticError> {
        self.output.push_str("program {");
        if statements.is_empty() {
            self.output.push_str("}\n");
            return Ok(());
        }
        self.output.push('\n');
        for statement in statements {
            match statement {
                InkScriptProgramStatement::Assert { kind, arguments } => {
                    self.indent(1);
                    self.output.push_str("assert ");
                    self.output.push_str(kind);
                    self.output.push(' ');
                    let fields = self.schema.assertion(kind).ok_or_else(|| {
                        error(InkScriptSemanticErrorCode::UnknownRecordSchema, kind)
                    })?;
                    self.record(arguments, fields, 1, kind)?;
                    self.output.push_str(";\n");
                }
                InkScriptProgramStatement::Step {
                    label,
                    result_alias,
                    enabled,
                    editor_group,
                    command,
                    arguments,
                } => {
                    self.indent(1);
                    self.output.push_str("step ");
                    self.string(label);
                    if let Some(alias) = result_alias {
                        self.output.push_str(" as ");
                        self.output.push_str(alias);
                    }
                    self.output.push_str(" {\n");
                    self.indent(2);
                    self.output.push_str(if *enabled {
                        "enabled = true;\n"
                    } else {
                        "enabled = false;\n"
                    });
                    if let Some(group) = editor_group {
                        self.indent(2);
                        self.output.push_str("editor_group = ");
                        self.string(group);
                        self.output.push_str(";\n");
                    }
                    self.indent(2);
                    self.output.push_str("invoke ");
                    self.output.push_str(command);
                    self.output.push(' ');
                    let fields = self.schema.command(command).ok_or_else(|| {
                        error(InkScriptSemanticErrorCode::UnknownCommandSchema, command)
                    })?;
                    self.record(arguments, fields, 2, command)?;
                    self.output.push_str(";\n");
                    self.indent(1);
                    self.output.push_str("}\n");
                }
            }
        }
        self.output.push_str("}\n");
        Ok(())
    }

    fn named_record(
        &mut self,
        record: &InkScriptRecord,
        schema_name: &str,
        indent: usize,
    ) -> Result<(), InkScriptSemanticError> {
        let fields = self
            .schema
            .record(schema_name)
            .ok_or_else(|| error(InkScriptSemanticErrorCode::UnknownRecordSchema, schema_name))?;
        self.record(record, fields, indent, schema_name)
    }

    fn record(
        &mut self,
        record: &InkScriptRecord,
        fields: &[InkScriptFieldSchema],
        indent: usize,
        path: &str,
    ) -> Result<(), InkScriptSemanticError> {
        let normalized = normalize_record(record.clone(), fields, self.schema, path)?;
        if normalized != *record {
            return Err(error(InkScriptSemanticErrorCode::InvalidSchema, path));
        }
        if record.0.is_empty() {
            self.output.push_str("{}");
            return Ok(());
        }
        self.output.push_str("{\n");
        for field in fields {
            let Some(value) = record.0.get(field.name) else {
                continue;
            };
            self.indent(indent + 1);
            self.output.push_str(field.name);
            self.output.push_str(" = ");
            self.value(value, field.type_name, indent + 1)?;
            self.output.push_str(";\n");
        }
        self.indent(indent);
        self.output.push('}');
        Ok(())
    }

    fn value(
        &mut self,
        value: &InkScriptValue,
        type_name: &str,
        indent: usize,
    ) -> Result<(), InkScriptSemanticError> {
        let nullable_inner = unwrap_type(type_name, "nullable<");
        let type_name = nullable_inner.unwrap_or(type_name);
        match value {
            InkScriptValue::Boolean(value) => {
                self.output.push_str(if *value { "true" } else { "false" })
            }
            InkScriptValue::Integer(value) | InkScriptValue::Decimal(value) => {
                self.output.push_str(value)
            }
            InkScriptValue::String(value) => self.string(value),
            InkScriptValue::Uuid(value) => {
                self.output.push_str("uuid");
                self.string(value);
            }
            InkScriptValue::Digest(value) => {
                self.output.push_str("blake3");
                self.string(value);
            }
            InkScriptValue::Base64(value) => self.base64(value, indent),
            InkScriptValue::None => self.output.push_str("none"),
            InkScriptValue::Enum(value) => self.output.push_str(value),
            InkScriptValue::Constructor { name, arguments } => {
                self.output.push_str(name);
                self.output.push('(');
                for (index, argument) in arguments.iter().enumerate() {
                    if index != 0 {
                        self.output.push_str(", ");
                    }
                    self.value(argument, "literal_value", indent)?;
                }
                self.output.push(')');
            }
            InkScriptValue::AssetReference(name) => {
                self.output.push_str("asset(");
                self.output.push_str(name);
                self.output.push(')');
            }
            InkScriptValue::Reference { root, segments } => {
                self.output.push('$');
                self.output.push_str(root);
                for segment in segments {
                    match segment {
                        InkScriptReferenceSegment::Field(name) => {
                            self.output.push('.');
                            self.output.push_str(name);
                        }
                        InkScriptReferenceSegment::Index(index) => {
                            self.output.push('[');
                            self.output.push_str(index);
                            self.output.push(']');
                        }
                    }
                }
            }
            InkScriptValue::List(values) => {
                if values.is_empty() {
                    self.output.push_str("[]");
                } else {
                    let element_type = unwrap_type(type_name, "list<").unwrap_or("literal_value");
                    self.output.push_str("[\n");
                    for value in values {
                        self.indent(indent + 1);
                        self.value(value, element_type, indent + 1)?;
                        self.output.push_str(",\n");
                    }
                    self.indent(indent);
                    self.output.push(']');
                }
            }
            InkScriptValue::Record(record) => {
                let fields = self.schema.record(type_name).ok_or_else(|| {
                    error(InkScriptSemanticErrorCode::UnknownRecordSchema, type_name)
                })?;
                self.record(record, fields, indent, type_name)?;
            }
        }
        Ok(())
    }

    fn type_reference(&mut self, value: &InkScriptTypeReference) {
        match value {
            InkScriptTypeReference::Named(name) => self.output.push_str(name),
            InkScriptTypeReference::List(child) => {
                self.output.push_str("list<");
                self.type_reference(child);
                self.output.push('>');
            }
            InkScriptTypeReference::Nullable(child) => {
                self.output.push_str("nullable<");
                self.type_reference(child);
                self.output.push('>');
            }
        }
    }

    fn string(&mut self, value: &str) {
        self.output.push('"');
        for character in value.chars() {
            match character {
                '"' => self.output.push_str("\\\""),
                '\\' => self.output.push_str("\\\\"),
                '\n' => self.output.push_str("\\n"),
                '\r' => self.output.push_str("\\r"),
                '\t' => self.output.push_str("\\t"),
                '\0'..='\u{1f}' => self
                    .output
                    .push_str(&format!("\\u{{{:x}}}", u32::from(character))),
                _ => self.output.push(character),
            }
        }
        self.output.push('"');
    }

    fn base64(&mut self, value: &[u8], indent: usize) {
        let encoded = encode_base64(value);
        self.output.push_str("base64\"\"\"\n");
        for chunk in encoded.as_bytes().chunks(76) {
            self.indent(indent + 1);
            self.output
                .push_str(std::str::from_utf8(chunk).expect("base64 is ASCII"));
            self.output.push('\n');
        }
        self.indent(indent);
        self.output.push_str("\"\"\"");
    }

    fn indent(&mut self, depth: usize) {
        for _ in 0..depth {
            self.output.push_str("    ");
        }
    }
}

fn output_schema(record: &InkScriptRecord) -> Result<&'static str, InkScriptSemanticError> {
    match enum_field(record, "policy") {
        Some("duplicate") => Ok("output_duplicate"),
        Some("new_save") => Ok("output_new_save"),
        Some("explicit_overwrite") => Ok("output_explicit_overwrite"),
        _ => Err(error(
            InkScriptSemanticErrorCode::UnknownRecordSchema,
            "output.policy",
        )),
    }
}

fn encode_base64(value: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(value.len().div_ceil(3) * 4);
    for chunk in value.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        result.push(char::from(ALPHABET[usize::from(a >> 2)]));
        result.push(char::from(
            ALPHABET[usize::from(((a & 0x03) << 4) | (b >> 4))],
        ));
        if chunk.len() > 1 {
            result.push(char::from(
                ALPHABET[usize::from(((b & 0x0f) << 2) | (c >> 6))],
            ));
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(char::from(ALPHABET[usize::from(c & 0x3f)]));
        } else {
            result.push('=');
        }
    }
    result
}
