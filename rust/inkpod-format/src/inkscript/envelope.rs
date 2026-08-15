use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use super::parser::InkScriptDocumentKind;
use super::schema::{INKSCRIPT_PROCEDURE_CATALOG_VERSION, INKSCRIPT_REQUIRED_REPLAY_EPOCH};
use super::source::INKSCRIPT_FILE_VERSION;
use super::syntax::{
    InkScriptInput, InkScriptInputKind, InkScriptRecord, InkScriptSemanticDocument,
    InkScriptSemanticSection, InkScriptValue,
};

/// Maximum execution delay between two planned items in InkScript file version 1.
pub const MAX_INKSCRIPT_WAIT_MS: u32 = 3_600_000;

/// Stable failure categories produced while typing an orchestration envelope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptEnvelopeErrorCode {
    /// The semantic document is a fragment rather than a complete file.
    NotCompleteFile,
    /// A required complete-file section is absent from the semantic document.
    MissingSection,
    /// A value does not have the language-schema type required by its field.
    InvalidType,
    /// An integer cannot be represented by the required fixed-width type.
    NumericOverflow,
    /// `requires.procedure_catalog` does not equal the exact-current catalog version.
    NonCurrentProcedureCatalog,
    /// `requires.replay_epoch` does not equal the exact-current replay epoch.
    NonCurrentReplayEpoch,
    /// A metadata extension does not use a bounded reverse-DNS ASCII key/value record.
    InvalidMetadataExtension,
    /// A metadata extension key occurs more than once.
    DuplicateMetadataExtension,
    /// An input cell selection is zero, reversed, or invalid for the input kind.
    InvalidCellRange,
    /// Recursive folder input is not available in file version 1.
    UnsupportedRecursiveInput,
    /// The output record is not one of the closed exact-current variants.
    InvalidOutput,
    /// The selected output policy is incompatible with a statically known input kind.
    IncompatibleOutputPolicy,
    /// The execution policy has an unknown value or exceeds its fixed bound.
    InvalidExecutionPolicy,
}

impl InkScriptEnvelopeErrorCode {
    /// Returns the locale-independent diagnostic spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotCompleteFile => "not_complete_file",
            Self::MissingSection => "missing_section",
            Self::InvalidType => "invalid_type",
            Self::NumericOverflow => "numeric_overflow",
            Self::NonCurrentProcedureCatalog => "noncurrent_procedure_catalog",
            Self::NonCurrentReplayEpoch => "noncurrent_replay_epoch",
            Self::InvalidMetadataExtension => "invalid_metadata_extension",
            Self::DuplicateMetadataExtension => "duplicate_metadata_extension",
            Self::InvalidCellRange => "invalid_cell_range",
            Self::UnsupportedRecursiveInput => "unsupported_recursive_input",
            Self::InvalidOutput => "invalid_output",
            Self::IncompatibleOutputPolicy => "incompatible_output_policy",
            Self::InvalidExecutionPolicy => "invalid_execution_policy",
        }
    }
}

/// A typed-envelope failure with a stable code and non-localized field path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptEnvelopeError {
    code: InkScriptEnvelopeErrorCode,
    path: String,
}

impl InkScriptEnvelopeError {
    fn new(code: InkScriptEnvelopeErrorCode, path: impl Into<String>) -> Self {
        Self {
            code,
            path: path.into(),
        }
    }

    /// Returns the stable failure category.
    pub const fn code(&self) -> InkScriptEnvelopeErrorCode {
        self.code
    }

    /// Returns the non-localized semantic field path associated with the failure.
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl fmt::Display for InkScriptEnvelopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} at {}", self.code.as_str(), self.path)
    }
}

impl Error for InkScriptEnvelopeError {}

/// Exact-current versions required by a complete InkScript file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptRequirements {
    procedure_catalog_version: u32,
    replay_epoch: u32,
}

impl InkScriptRequirements {
    /// Returns the exact procedure catalog version checked during conversion.
    pub const fn procedure_catalog_version(self) -> u32 {
        self.procedure_catalog_version
    }

    /// Returns the exact replay epoch checked during conversion.
    pub const fn replay_epoch(self) -> u32 {
        self.replay_epoch
    }
}

/// One non-semantic reverse-DNS metadata extension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptMetadataExtension {
    key: String,
    value: String,
}

impl InkScriptMetadataExtension {
    /// Returns the preserved extension key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the preserved extension value.
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Non-semantic, bounded file metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InkScriptMetadata {
    name: Option<String>,
    description: Option<String>,
    extensions: Vec<InkScriptMetadataExtension>,
}

impl InkScriptMetadata {
    /// Returns the optional display name without applying Unicode normalization.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the optional description without applying Unicode normalization.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Returns metadata extensions in declaration order.
    pub fn extensions(&self) -> &[InkScriptMetadataExtension] {
        &self.extensions
    }
}

/// Closed input declaration kinds in InkScript file version 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptInputDeclarationKind {
    /// One native Cell file path intent.
    File,
    /// One non-recursive folder enumeration path intent.
    Folder,
    /// The document fixed by an issue-time command context.
    CurrentDocument,
    /// The sequence fixed by an issue-time command context.
    CurrentSequence,
}

/// A typed display-number selection applied when an input is expanded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptCellSelection {
    /// Select every available item.
    All,
    /// Select a nonzero inclusive display-number range.
    Inclusive {
        /// First selected display number.
        first: u32,
        /// Last selected display number.
        last: u32,
    },
}

/// One typed input declaration. Path text is preserved but never opened by this model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptInputDeclaration {
    kind: InkScriptInputDeclarationKind,
    path_text: Option<String>,
    cells: InkScriptCellSelection,
}

impl InkScriptInputDeclaration {
    /// Returns the closed input kind.
    pub const fn kind(&self) -> InkScriptInputDeclarationKind {
        self.kind
    }

    /// Returns source path-intent text for file/folder declarations.
    ///
    /// This text does not grant authority and has not been opened or resolved.
    pub fn path_text(&self) -> Option<&str> {
        self.path_text.as_deref()
    }

    /// Returns the validated display-number selection.
    pub const fn cells(&self) -> InkScriptCellSelection {
        self.cells
    }
}

/// The only output format accepted by InkScript file version 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptOutputFormat {
    /// Exact-current native `.inkpod` output.
    Inkpod,
}

/// Numbering direction for duplicate and new-save output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptNumberDirection {
    /// Add the zero-based planned item ordinal.
    Ascending,
    /// Subtract the zero-based planned item ordinal.
    Descending,
}

/// Shared fields of duplicate and new-save output variants.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptNumberedOutput {
    folder: String,
    cell_folder: bool,
    basename: String,
    start_number: u32,
    direction: InkScriptNumberDirection,
}

impl InkScriptNumberedOutput {
    /// Returns output-folder path-intent text. Empty means the input parent at plan time.
    pub fn folder(&self) -> &str {
        &self.folder
    }

    /// Returns whether planning should insert a source-stem folder.
    pub const fn cell_folder(&self) -> bool {
        self.cell_folder
    }

    /// Returns the requested basename. Empty retains the policy-specific default meaning.
    pub fn basename(&self) -> &str {
        &self.basename
    }

    /// Returns the first output display number.
    pub const fn start_number(&self) -> u32 {
        self.start_number
    }

    /// Returns the checked numbering direction.
    pub const fn direction(&self) -> InkScriptNumberDirection {
        self.direction
    }
}

/// Closed output policy union for InkScript file version 1.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InkScriptOutput {
    /// Create an independently named native output without changing document identity.
    Duplicate(InkScriptNumberedOutput),
    /// Create a newly named native output without changing live path authority.
    NewSave(InkScriptNumberedOutput),
    /// Replace each eligible closed file-backed input after later authority/confirmation gates.
    ExplicitOverwrite,
}

impl InkScriptOutput {
    /// Returns the closed native output format.
    pub const fn format(&self) -> InkScriptOutputFormat {
        InkScriptOutputFormat::Inkpod
    }
}

/// Per-item failure handling after an item reaches a terminal outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptExecutionFailure {
    /// Continue with the next preview-ordered item after a failure.
    Continue,
    /// Mark later items not started after a failure.
    Stop,
}

/// Typed execution policy. Run scope and dry-run remain transient run options.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptExecutionPolicy {
    failure: InkScriptExecutionFailure,
    wait_ms: u32,
    preview_before_save: bool,
}

impl InkScriptExecutionPolicy {
    /// Returns the failure-continuation policy.
    pub const fn failure(self) -> InkScriptExecutionFailure {
        self.failure
    }

    /// Returns the bounded delay between completed and next items.
    pub const fn wait_ms(self) -> u32 {
        self.wait_ms
    }

    /// Returns whether the interactive frontend must show the execution preview before saving.
    pub const fn preview_before_save(self) -> bool {
        self.preview_before_save
    }
}

/// Filesystem capability described by source path-intent text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptPathIntentAccess {
    /// Read one native file.
    Read,
    /// Enumerate one folder without recursive traversal in version 1.
    Enumerate,
    /// Create output beneath a later-authorized folder.
    Create,
    /// Replace eligible expanded input files after later confirmation.
    Replace,
}

/// One immutable path-intent preview row. It is descriptive and grants no authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptPathIntent {
    access: InkScriptPathIntentAccess,
    input_index: Option<usize>,
    text: String,
}

impl InkScriptPathIntent {
    /// Returns the requested filesystem capability.
    pub const fn access(&self) -> InkScriptPathIntentAccess {
        self.access
    }

    /// Returns the source input declaration index, or `None` for an output-root intent.
    pub const fn input_index(&self) -> Option<usize> {
        self.input_index
    }

    /// Returns the unresolved source path text.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Deterministically ordered, authority-free path intentions derived from an envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptPathIntentPreview {
    intents: Vec<InkScriptPathIntent>,
}

impl InkScriptPathIntentPreview {
    /// Returns intent rows in input declaration order followed by output intents.
    pub fn intents(&self) -> &[InkScriptPathIntent] {
        &self.intents
    }
}

/// Immutable, Core-independent typed orchestration sections of a complete InkScript file.
///
/// Construction validates exact-current requirements and closed field values. It performs no path
/// resolution, file open, directory enumeration, Core mutation, task creation, or output write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptOrchestrationEnvelope {
    requirements: InkScriptRequirements,
    metadata: InkScriptMetadata,
    inputs: Vec<InkScriptInputDeclaration>,
    output: InkScriptOutput,
    execution: InkScriptExecutionPolicy,
}

impl InkScriptOrchestrationEnvelope {
    /// Returns the exact file grammar version accepted before this model was built.
    pub const fn file_version(&self) -> u32 {
        INKSCRIPT_FILE_VERSION
    }

    /// Returns the exact-current catalog/replay requirements.
    pub const fn requirements(&self) -> InkScriptRequirements {
        self.requirements
    }

    /// Returns non-semantic metadata.
    pub const fn metadata(&self) -> &InkScriptMetadata {
        &self.metadata
    }

    /// Returns input declarations in source order.
    pub fn inputs(&self) -> &[InkScriptInputDeclaration] {
        &self.inputs
    }

    /// Returns the closed output variant.
    pub const fn output(&self) -> &InkScriptOutput {
        &self.output
    }

    /// Returns the bounded execution policy.
    pub const fn execution(&self) -> InkScriptExecutionPolicy {
        self.execution
    }

    /// Builds an owned, deterministic preview of unresolved path text and requested access.
    ///
    /// No filesystem operation or authority acquisition occurs.
    pub fn path_intent_preview(&self) -> InkScriptPathIntentPreview {
        let mut intents = Vec::new();
        for (index, input) in self.inputs.iter().enumerate() {
            let access = match input.kind {
                InkScriptInputDeclarationKind::File => Some(InkScriptPathIntentAccess::Read),
                InkScriptInputDeclarationKind::Folder => Some(InkScriptPathIntentAccess::Enumerate),
                InkScriptInputDeclarationKind::CurrentDocument
                | InkScriptInputDeclarationKind::CurrentSequence => None,
            };
            if let (Some(access), Some(text)) = (access, input.path_text.as_ref()) {
                intents.push(InkScriptPathIntent {
                    access,
                    input_index: Some(index),
                    text: text.clone(),
                });
            }
        }
        match &self.output {
            InkScriptOutput::Duplicate(output) | InkScriptOutput::NewSave(output) => {
                intents.push(InkScriptPathIntent {
                    access: InkScriptPathIntentAccess::Create,
                    input_index: None,
                    text: output.folder.clone(),
                });
            }
            InkScriptOutput::ExplicitOverwrite => {
                for (index, input) in self.inputs.iter().enumerate() {
                    if let Some(text) = input.path_text.as_ref() {
                        intents.push(InkScriptPathIntent {
                            access: InkScriptPathIntentAccess::Replace,
                            input_index: Some(index),
                            text: text.clone(),
                        });
                    }
                }
            }
        }
        InkScriptPathIntentPreview { intents }
    }
}

/// Converts command-independent semantic syntax into a typed orchestration envelope.
///
/// The conversion is a pure owned copy. Success and failure leave the source semantic tree
/// unchanged. Path text is retained only as an unresolved intent; filesystem authority and all
/// path identity checks belong to later planning milestones.
pub fn build_inkscript_orchestration_envelope(
    document: &InkScriptSemanticDocument,
) -> Result<InkScriptOrchestrationEnvelope, InkScriptEnvelopeError> {
    if document.kind != InkScriptDocumentKind::File {
        return Err(error(
            InkScriptEnvelopeErrorCode::NotCompleteFile,
            "document",
        ));
    }

    let mut requires = None;
    let mut meta = None;
    let mut inputs = None;
    let mut output = None;
    let mut execution = None;
    let mut has_program = false;
    for section in &document.sections {
        match section {
            InkScriptSemanticSection::Requires(record) => requires = Some(record),
            InkScriptSemanticSection::Meta(record) => meta = Some(record),
            InkScriptSemanticSection::Inputs(declarations) => inputs = Some(declarations),
            InkScriptSemanticSection::Program(_) => has_program = true,
            InkScriptSemanticSection::Output(record) => output = Some(record),
            InkScriptSemanticSection::Execution(record) => execution = Some(record),
            InkScriptSemanticSection::Parameters(_)
            | InkScriptSemanticSection::Bindings(_)
            | InkScriptSemanticSection::Assets(_) => {}
        }
    }
    if !has_program {
        return Err(error(InkScriptEnvelopeErrorCode::MissingSection, "program"));
    }

    let requirements = type_requirements(
        requires.ok_or_else(|| error(InkScriptEnvelopeErrorCode::MissingSection, "requires"))?,
    )?;
    let metadata = meta.map_or_else(|| Ok(InkScriptMetadata::default()), type_metadata)?;
    let inputs = type_inputs(
        inputs.ok_or_else(|| error(InkScriptEnvelopeErrorCode::MissingSection, "inputs"))?,
    )?;
    let output = type_output(
        output.ok_or_else(|| error(InkScriptEnvelopeErrorCode::MissingSection, "output"))?,
        &inputs,
    )?;
    let execution = type_execution(
        execution.ok_or_else(|| error(InkScriptEnvelopeErrorCode::MissingSection, "execution"))?,
    )?;

    Ok(InkScriptOrchestrationEnvelope {
        requirements,
        metadata,
        inputs,
        output,
        execution,
    })
}

fn type_requirements(
    record: &InkScriptRecord,
) -> Result<InkScriptRequirements, InkScriptEnvelopeError> {
    let procedure_catalog_version = required_u32(record, "procedure_catalog", "requires")?;
    if procedure_catalog_version != INKSCRIPT_PROCEDURE_CATALOG_VERSION {
        return Err(error(
            InkScriptEnvelopeErrorCode::NonCurrentProcedureCatalog,
            "requires.procedure_catalog",
        ));
    }
    let replay_epoch = required_u32(record, "replay_epoch", "requires")?;
    if replay_epoch != INKSCRIPT_REQUIRED_REPLAY_EPOCH {
        return Err(error(
            InkScriptEnvelopeErrorCode::NonCurrentReplayEpoch,
            "requires.replay_epoch",
        ));
    }
    Ok(InkScriptRequirements {
        procedure_catalog_version,
        replay_epoch,
    })
}

fn type_metadata(record: &InkScriptRecord) -> Result<InkScriptMetadata, InkScriptEnvelopeError> {
    let name = optional_string(record, "name", "meta")?;
    let description = optional_string(record, "description", "meta")?;
    let mut extensions = Vec::new();
    let mut keys = BTreeSet::new();
    if let Some(value) = record.0.get("extensions") {
        let values = match value {
            InkScriptValue::List(values) => values,
            _ => return Err(invalid_type("meta.extensions")),
        };
        extensions.reserve(values.len());
        for (index, value) in values.iter().enumerate() {
            let extension = match value {
                InkScriptValue::Record(extension) => extension,
                _ => return Err(invalid_type(format!("meta.extensions[{index}]"))),
            };
            let key = required_string(extension, "key", &format!("meta.extensions[{index}]"))?;
            let value = required_string(extension, "value", &format!("meta.extensions[{index}]"))?;
            if !is_reverse_dns_ascii(&key) {
                return Err(error(
                    InkScriptEnvelopeErrorCode::InvalidMetadataExtension,
                    format!("meta.extensions[{index}].key"),
                ));
            }
            if !keys.insert(key.clone()) {
                return Err(error(
                    InkScriptEnvelopeErrorCode::DuplicateMetadataExtension,
                    format!("meta.extensions[{index}].key"),
                ));
            }
            extensions.push(InkScriptMetadataExtension { key, value });
        }
    }
    Ok(InkScriptMetadata {
        name,
        description,
        extensions,
    })
}

fn type_inputs(
    inputs: &[InkScriptInput],
) -> Result<Vec<InkScriptInputDeclaration>, InkScriptEnvelopeError> {
    inputs
        .iter()
        .enumerate()
        .map(|(index, input)| type_input(input, index))
        .collect()
}

fn type_input(
    input: &InkScriptInput,
    index: usize,
) -> Result<InkScriptInputDeclaration, InkScriptEnvelopeError> {
    let path = format!("inputs[{index}]");
    let (kind, expects_path) = match input.kind {
        InkScriptInputKind::File => (InkScriptInputDeclarationKind::File, true),
        InkScriptInputKind::Folder => (InkScriptInputDeclarationKind::Folder, true),
        InkScriptInputKind::CurrentDocument => {
            (InkScriptInputDeclarationKind::CurrentDocument, false)
        }
        InkScriptInputKind::CurrentSequence => {
            (InkScriptInputDeclarationKind::CurrentSequence, false)
        }
    };
    if expects_path != input.path.is_some() {
        return Err(invalid_type(format!("{path}.path")));
    }
    let cells = match input.options.0.get("cells") {
        None => InkScriptCellSelection::All,
        Some(InkScriptValue::Enum(value)) if value == "all" => InkScriptCellSelection::All,
        Some(InkScriptValue::Constructor { name, arguments }) if name == "range" => {
            if arguments.len() != 2 {
                return Err(error(
                    InkScriptEnvelopeErrorCode::InvalidCellRange,
                    format!("{path}.cells"),
                ));
            }
            let first = value_u32(&arguments[0], &format!("{path}.cells.first"))?;
            let last = value_u32(&arguments[1], &format!("{path}.cells.last"))?;
            if first == 0 || last < first {
                return Err(error(
                    InkScriptEnvelopeErrorCode::InvalidCellRange,
                    format!("{path}.cells"),
                ));
            }
            InkScriptCellSelection::Inclusive { first, last }
        }
        Some(_) => return Err(invalid_type(format!("{path}.cells"))),
    };
    if kind == InkScriptInputDeclarationKind::CurrentDocument
        && cells != InkScriptCellSelection::All
    {
        return Err(error(
            InkScriptEnvelopeErrorCode::InvalidCellRange,
            format!("{path}.cells"),
        ));
    }
    if let Some(value) = input.options.0.get("recursive") {
        match value {
            InkScriptValue::Boolean(false) => {}
            InkScriptValue::Boolean(true) => {
                return Err(error(
                    InkScriptEnvelopeErrorCode::UnsupportedRecursiveInput,
                    format!("{path}.recursive"),
                ));
            }
            _ => return Err(invalid_type(format!("{path}.recursive"))),
        }
    }
    Ok(InkScriptInputDeclaration {
        kind,
        path_text: input.path.clone(),
        cells,
    })
}

fn type_output(
    record: &InkScriptRecord,
    inputs: &[InkScriptInputDeclaration],
) -> Result<InkScriptOutput, InkScriptEnvelopeError> {
    if enum_value(record, "format", "output")? != "inkpod" {
        return Err(error(
            InkScriptEnvelopeErrorCode::InvalidOutput,
            "output.format",
        ));
    }
    match enum_value(record, "policy", "output")? {
        "duplicate" => type_numbered_output(record).map(InkScriptOutput::Duplicate),
        "new_save" => type_numbered_output(record).map(InkScriptOutput::NewSave),
        "explicit_overwrite" => {
            if inputs.iter().any(|input| {
                matches!(
                    input.kind,
                    InkScriptInputDeclarationKind::CurrentDocument
                        | InkScriptInputDeclarationKind::CurrentSequence
                )
            }) {
                return Err(error(
                    InkScriptEnvelopeErrorCode::IncompatibleOutputPolicy,
                    "output.policy",
                ));
            }
            Ok(InkScriptOutput::ExplicitOverwrite)
        }
        _ => Err(error(
            InkScriptEnvelopeErrorCode::InvalidOutput,
            "output.policy",
        )),
    }
}

fn type_numbered_output(
    record: &InkScriptRecord,
) -> Result<InkScriptNumberedOutput, InkScriptEnvelopeError> {
    let folder = required_string(record, "folder", "output")?;
    let cell_folder = required_bool(record, "cell_folder", "output")?;
    let basename = required_string(record, "basename", "output")?;
    let start_number = required_u32(record, "start_number", "output")?;
    let direction = match enum_value(record, "direction", "output")? {
        "ascending" => InkScriptNumberDirection::Ascending,
        "descending" => InkScriptNumberDirection::Descending,
        _ => {
            return Err(error(
                InkScriptEnvelopeErrorCode::InvalidOutput,
                "output.direction",
            ));
        }
    };
    Ok(InkScriptNumberedOutput {
        folder,
        cell_folder,
        basename,
        start_number,
        direction,
    })
}

fn type_execution(
    record: &InkScriptRecord,
) -> Result<InkScriptExecutionPolicy, InkScriptEnvelopeError> {
    let failure = match enum_value(record, "failure", "execution")? {
        "continue" => InkScriptExecutionFailure::Continue,
        "stop" => InkScriptExecutionFailure::Stop,
        _ => {
            return Err(error(
                InkScriptEnvelopeErrorCode::InvalidExecutionPolicy,
                "execution.failure",
            ));
        }
    };
    let wait_ms = required_u32(record, "wait_ms", "execution")?;
    if wait_ms > MAX_INKSCRIPT_WAIT_MS {
        return Err(error(
            InkScriptEnvelopeErrorCode::InvalidExecutionPolicy,
            "execution.wait_ms",
        ));
    }
    let preview_before_save = required_bool(record, "preview_before_save", "execution")?;
    Ok(InkScriptExecutionPolicy {
        failure,
        wait_ms,
        preview_before_save,
    })
}

fn optional_string(
    record: &InkScriptRecord,
    name: &str,
    owner: &str,
) -> Result<Option<String>, InkScriptEnvelopeError> {
    record
        .0
        .get(name)
        .map(|value| match value {
            InkScriptValue::String(value) => Ok(value.clone()),
            _ => Err(invalid_type(format!("{owner}.{name}"))),
        })
        .transpose()
}

fn required_string(
    record: &InkScriptRecord,
    name: &str,
    owner: &str,
) -> Result<String, InkScriptEnvelopeError> {
    match record.0.get(name) {
        Some(InkScriptValue::String(value)) => Ok(value.clone()),
        _ => Err(invalid_type(format!("{owner}.{name}"))),
    }
}

fn required_bool(
    record: &InkScriptRecord,
    name: &str,
    owner: &str,
) -> Result<bool, InkScriptEnvelopeError> {
    match record.0.get(name) {
        Some(InkScriptValue::Boolean(value)) => Ok(*value),
        _ => Err(invalid_type(format!("{owner}.{name}"))),
    }
}

fn required_u32(
    record: &InkScriptRecord,
    name: &str,
    owner: &str,
) -> Result<u32, InkScriptEnvelopeError> {
    let value = record
        .0
        .get(name)
        .ok_or_else(|| invalid_type(format!("{owner}.{name}")))?;
    value_u32(value, &format!("{owner}.{name}"))
}

fn value_u32(value: &InkScriptValue, path: &str) -> Result<u32, InkScriptEnvelopeError> {
    let InkScriptValue::Integer(value) = value else {
        return Err(invalid_type(path));
    };
    value
        .parse::<u32>()
        .map_err(|_| error(InkScriptEnvelopeErrorCode::NumericOverflow, path))
}

fn enum_value<'a>(
    record: &'a InkScriptRecord,
    name: &str,
    owner: &str,
) -> Result<&'a str, InkScriptEnvelopeError> {
    match record.0.get(name) {
        Some(InkScriptValue::Enum(value)) => Ok(value),
        _ => Err(invalid_type(format!("{owner}.{name}"))),
    }
}

fn is_reverse_dns_ascii(value: &str) -> bool {
    let mut labels = value.split('.');
    let Some(first) = labels.next() else {
        return false;
    };
    let Some(second) = labels.next() else {
        return false;
    };
    valid_dns_label(first) && valid_dns_label(second) && labels.all(valid_dns_label)
}

fn valid_dns_label(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
}

fn invalid_type(path: impl Into<String>) -> InkScriptEnvelopeError {
    error(InkScriptEnvelopeErrorCode::InvalidType, path)
}

fn error(code: InkScriptEnvelopeErrorCode, path: impl Into<String>) -> InkScriptEnvelopeError {
    InkScriptEnvelopeError::new(code, path)
}
