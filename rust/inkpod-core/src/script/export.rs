use std::collections::{BTreeMap, BTreeSet};

use inkpod_format::{
    InkScriptCommandSchema, InkScriptResultCardinality, InkScriptSemanticDocument, InkScriptSource,
    InkScriptSourceId, MAX_INKSCRIPT_INLINE_ASSET_TOTAL_BYTES, MAX_INKSCRIPT_PROGRAM_STATEMENTS,
    MAX_INKSCRIPT_SOURCE_BYTES, build_inkscript_declaration_model, build_inkscript_semantic,
    emit_inkscript_canonical, parse_inkscript,
};

use super::catalog::{InkScriptPortability, InkScriptPortabilityClass};
use super::compile::{ScriptSchemas, catalog};
use super::execute::initial_snapshot;
use crate::primitive::inkscript_metadata::MetadataColorGuideInvocation;
use crate::primitive::{
    CanonicalInvocation, CanonicalProcedure, InkScriptEntityKind, InkScriptRuntimeReferences,
    PrimitiveId, decode_color, decode_color_chart, decode_palette,
};
use crate::{
    AssetDescriptor, AssetId, AssetKind, Core, GuideAxis, JournalEntry, JournalEventId,
    PixelFormat, StateId,
};

/// Caller-lowerable resource envelope for one journal-to-fragment export query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InkScriptExportLimits {
    commits: usize,
    source_bytes: usize,
    asset_bytes: u64,
}

impl InkScriptExportLimits {
    /// Returns the exact-current InkScript language resource envelope.
    pub const fn exact_current() -> Self {
        Self {
            commits: MAX_INKSCRIPT_PROGRAM_STATEMENTS,
            source_bytes: MAX_INKSCRIPT_SOURCE_BYTES,
            asset_bytes: MAX_INKSCRIPT_INLINE_ASSET_TOTAL_BYTES,
        }
    }

    /// Lowers the maximum selected Commit count. Zero becomes one.
    pub const fn with_commits(mut self, maximum: usize) -> Self {
        self.commits = lower_nonzero(maximum, self.commits);
        self
    }

    /// Lowers the maximum canonical fragment byte count. Zero becomes one.
    pub const fn with_source_bytes(mut self, maximum: usize) -> Self {
        self.source_bytes = lower_nonzero(maximum, self.source_bytes);
        self
    }

    /// Lowers the maximum total inline asset payload. Zero becomes one.
    pub const fn with_asset_bytes(mut self, maximum: u64) -> Self {
        self.asset_bytes = if maximum == 0 {
            1
        } else if maximum < self.asset_bytes {
            maximum
        } else {
            self.asset_bytes
        };
        self
    }
}

const fn lower_nonzero(requested: usize, current: usize) -> usize {
    if requested == 0 {
        1
    } else if requested < current {
        requested
    } else {
        current
    }
}

/// Stable failure categories for journal-to-fragment export.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkScriptExportError {
    /// No Commit event was selected.
    EmptySelection,
    /// A selected event is Genesis control/history metadata rather than a Commit.
    NotACommit(JournalEventId),
    /// Events are duplicated, reordered, cross branches, or do not form one linear ancestor chain.
    NonLinearSelection,
    /// The selected canonical procedure has no exact typed runtime invocation.
    MissingRuntimeInvocation,
    /// A canonical primitive has no exact-current journal exporter codec.
    UnsupportedPrimitive(PrimitiveId),
    /// The journal, retained asset, or generated exact-current semantic model is inconsistent.
    InvalidSource,
    /// A caller-lowered or exact-current resource bound was exceeded.
    ResourceLimit,
    /// Cooperative cancellation was observed before publication.
    Cancelled,
}

/// Fragment-wide portability class evaluated from exact catalog arguments.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum InkScriptExportPortability {
    /// No destination binding is required by the selected invocations.
    Portable,
    /// At least one invocation requires explicit destination binding or a semantic precondition.
    RequiresBinding,
    /// At least one invocation may only execute against the exact source precondition.
    StrictSourceOnly,
}

/// One exact-current, canonical journal fragment owned independently of its source [`Core`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkScriptFragmentExport {
    text: String,
    semantic: InkScriptSemanticDocument,
    base_state_id: StateId,
    final_state_id: StateId,
    commit_count: usize,
    portability: InkScriptExportPortability,
    required_preconditions: Vec<String>,
}

impl InkScriptFragmentExport {
    /// Returns canonical BOM-free UTF-8 text with LF newlines and one trailing newline.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Borrows the exact semantic fragment AST used to emit [`Self::text`].
    pub const fn semantic(&self) -> &InkScriptSemanticDocument {
        &self.semantic
    }

    /// Returns the selected first Commit's parent state.
    pub const fn base_state_id(&self) -> StateId {
        self.base_state_id
    }

    /// Returns the selected last Commit's resulting state.
    pub const fn final_state_id(&self) -> StateId {
        self.final_state_id
    }

    /// Returns the number of selected canonical Commit records.
    pub const fn commit_count(&self) -> usize {
        self.commit_count
    }

    /// Returns the strongest exact catalog portability class in the fragment.
    pub const fn portability(&self) -> InkScriptExportPortability {
        self.portability
    }

    /// Returns the sorted union of exact catalog precondition names.
    pub fn required_preconditions(&self) -> &[String] {
        &self.required_preconditions
    }
}

/// Exports one ordered selection of journal Commit events as an exact-current canonical fragment.
///
/// The query captures canonical procedures, typed runtime invocations, assets, and state linkage
/// directly. Display summaries and thumbnails are not inputs. Cancellation or failure returns no
/// partial result and never changes document/editor revision, history, dirty/savepoint state,
/// caches, assets, or any persistent ID allocator.
pub fn export_inkscript_fragment(
    core: &Core,
    selection: &[JournalEventId],
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<InkScriptFragmentExport, InkScriptExportError> {
    export_inkscript_fragment_with_limits(
        core,
        selection,
        InkScriptExportLimits::exact_current(),
        cancelled,
    )
}

/// Exports with caller-lowered Commit, canonical-source, and inline-asset limits.
pub fn export_inkscript_fragment_with_limits(
    core: &Core,
    selection: &[JournalEventId],
    limits: InkScriptExportLimits,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<InkScriptFragmentExport, InkScriptExportError> {
    poll(cancelled)?;
    if selection.is_empty() {
        return Err(InkScriptExportError::EmptySelection);
    }
    if selection.len() > limits.commits {
        return Err(InkScriptExportError::ResourceLimit);
    }
    let (first_index, commits) = selected_commits(core, selection)?;
    let base = core
        .replay_prefix_for_inkscript_export(first_index, cancelled)
        .map_err(|error| {
            if error == crate::CoreError::Cancelled {
                InkScriptExportError::Cancelled
            } else {
                InkScriptExportError::InvalidSource
            }
        })?;
    let snapshot = initial_snapshot(&base).map_err(|_| InkScriptExportError::InvalidSource)?;
    let id_digest = super::bind::id_allocation_digest_from_snapshot(&snapshot)
        .map_err(|_| InkScriptExportError::InvalidSource)?;

    let schemas = ScriptSchemas::new();
    let schema = schemas
        .view()
        .map_err(|_| InkScriptExportError::InvalidSource)?;
    let catalog = catalog(&schemas.commands).map_err(|_| InkScriptExportError::InvalidSource)?;
    let mut strict = StrictBindings::new();
    let mut produced = BTreeMap::<u64, String>::new();
    let mut program = String::new();
    let mut strongest = InkScriptExportPortability::Portable;
    let mut required = BTreeSet::<String>::new();
    let mut commands = Vec::new();
    let mut assets = BTreeMap::<AssetId, ExportedAsset>::new();
    let mut asset_bytes = 0_u64;

    program.push_str("assert document { source_document_uuid = uuid\"");
    program.push_str(&snapshot.source_document_uuid);
    program.push_str("\"; state_digest = blake3\"");
    program.push_str(&snapshot.state_digest);
    program.push_str("\"; id_allocation_digest = blake3\"");
    program.push_str(&id_digest);
    program.push_str("\"; };\n");

    for (index, commit) in commits.iter().enumerate() {
        poll(cancelled)?;
        let step_number = index + 1;
        let alias = format!("step_{step_number}");
        let lifted = lift_procedure(
            commit.procedure(),
            ExportLiftContext {
                core,
                produced: &produced,
                strict: &mut strict,
                assets: &mut assets,
                asset_bytes: &mut asset_bytes,
                maximum_asset_bytes: limits.asset_bytes,
            },
        )?;
        let has_results = !commit.procedure().output_ids().is_empty();
        program.push_str("step \"Journal Commit ");
        program.push_str(&step_number.to_string());
        program.push('"');
        if has_results {
            program.push_str(" as ");
            program.push_str(&alias);
        }
        program.push_str(" { enabled = true; invoke ");
        program.push_str(lifted.command);
        program.push_str(" { ");
        program.push_str(&lifted.arguments);
        program.push_str(" }; }\n");
        commands.push(lifted.command);
        register_results(
            &catalog,
            &schemas.commands,
            lifted.command,
            &alias,
            commit.procedure().output_ids(),
            &lifted.output_kinds,
            &mut produced,
        )?;
    }

    let strict_owners = augment_light_table_owner_bindings(&snapshot, &mut strict)?;

    let mut source = String::from(
        "inkscript_fragment 2;\nrequires { procedure_catalog = 6; replay_epoch = 28; }\n",
    );
    if !strict.is_empty() {
        source.push_str("bindings {\n");
        let mut ordered = strict.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|(key, _)| (strict_owners.contains_key(*key), (*key).clone()));
        for ((entity, persistent_id), name) in ordered {
            source.push_str("let ");
            source.push_str(name);
            source.push_str(" = select ");
            source.push_str(entity);
            source.push_str(" { ");
            if let Some(owner) = strict_owners.get(&(entity.clone(), *persistent_id)) {
                let owner_name = strict
                    .get(owner)
                    .ok_or(InkScriptExportError::InvalidSource)?;
                source.push_str("set = $");
                source.push_str(owner_name);
                source.push_str("; ");
            }
            source.push_str("source_document_uuid = uuid\"");
            source.push_str(&snapshot.source_document_uuid);
            source.push_str("\"; persistent_id = ");
            source.push_str(&persistent_id.to_string());
            source.push_str("; };\n");
        }
        source.push_str("}\n");
    }
    source.push_str("program {\n");
    source.push_str(&program);
    source.push_str("}\n");
    if !assets.is_empty() {
        source.push_str("assets {\n");
        for asset in assets.values() {
            source.push_str(&asset.declaration);
            source.push('\n');
        }
        source.push_str("}\n");
    }
    if source.len() > limits.source_bytes {
        return Err(InkScriptExportError::ResourceLimit);
    }
    let source = InkScriptSource::new(InkScriptSourceId::new(24), source.as_bytes())
        .map_err(|_| InkScriptExportError::ResourceLimit)?;
    let parsed = parse_inkscript(&source);
    if !parsed.is_valid() {
        return Err(InkScriptExportError::InvalidSource);
    }
    let semantic = build_inkscript_semantic(&parsed, &schema)
        .map_err(|_| InkScriptExportError::InvalidSource)?;
    let model = build_inkscript_declaration_model(&parsed, &schema)
        .map_err(|_| InkScriptExportError::InvalidSource)?;
    if model.steps().len() != commands.len() {
        return Err(InkScriptExportError::InvalidSource);
    }
    for (step, command) in model.steps().iter().zip(commands) {
        let portability = catalog
            .evaluate_portability(command, step.arguments())
            .map_err(|_| InkScriptExportError::InvalidSource)?;
        strongest = strongest.max(public_portability(&portability));
        required.extend(
            portability
                .required_preconditions
                .into_iter()
                .map(str::to_owned),
        );
    }
    let canonical = emit_inkscript_canonical(&semantic, &schema)
        .map_err(|_| InkScriptExportError::InvalidSource)?;
    if canonical.len() > limits.source_bytes {
        return Err(InkScriptExportError::ResourceLimit);
    }
    let text = String::from_utf8(canonical).map_err(|_| InkScriptExportError::InvalidSource)?;
    poll(cancelled)?;
    Ok(InkScriptFragmentExport {
        text,
        semantic,
        base_state_id: commits[0].parent_state_id(),
        final_state_id: commits[commits.len() - 1].committed_state_id(),
        commit_count: commits.len(),
        portability: strongest,
        required_preconditions: required.into_iter().collect(),
    })
}

fn augment_light_table_owner_bindings(
    snapshot: &super::bind::InkScriptInitialDocumentSnapshot,
    strict: &mut StrictBindings,
) -> Result<StrictOwners, InkScriptExportError> {
    let item_keys = strict
        .keys()
        .filter(|(entity, _)| entity == "light_table_item")
        .cloned()
        .collect::<Vec<_>>();
    let mut owners = BTreeMap::new();
    for item_key in item_keys {
        let entity = snapshot
            .entities
            .iter()
            .find(|entity| {
                entity.reference.entity == item_key.0
                    && entity.reference.persistent_id == item_key.1
            })
            .ok_or(InkScriptExportError::InvalidSource)?;
        let owner = entity
            .owner
            .as_ref()
            .filter(|owner| owner.entity == "light_table_set")
            .ok_or(InkScriptExportError::InvalidSource)?;
        let owner_key = (owner.entity.clone(), owner.persistent_id);
        if !strict.contains_key(&owner_key) {
            let count = strict.len() + 1;
            strict.insert(
                owner_key.clone(),
                format!("external_light_table_set_{count}"),
            );
        }
        owners.insert(item_key, owner_key);
    }
    Ok(owners)
}

fn selected_commits<'a>(
    core: &'a Core,
    selection: &[JournalEventId],
) -> Result<(usize, Vec<&'a crate::JournalCommit>), InkScriptExportError> {
    let mut indices = Vec::new();
    let mut commits = Vec::new();
    for event in selection {
        let (index, entry) = core
            .journal_entries()
            .iter()
            .enumerate()
            .find(|(_, entry)| match entry {
                JournalEntry::Commit(commit) => commit.event_id() == *event,
                JournalEntry::HistoryMove(move_) => move_.event_id() == *event,
                JournalEntry::BranchCut(cut) => cut.event_id() == *event,
            })
            .ok_or(InkScriptExportError::NotACommit(*event))?;
        let JournalEntry::Commit(commit) = entry else {
            return Err(InkScriptExportError::NotACommit(*event));
        };
        indices.push(index);
        commits.push(commit);
    }
    if indices.windows(2).any(|pair| pair[0] >= pair[1])
        || commits.windows(2).any(|pair| {
            pair[1].parent_state_id() != pair[0].committed_state_id()
                || pair[1].branch_id() != pair[0].branch_id()
        })
    {
        return Err(InkScriptExportError::NonLinearSelection);
    }
    Ok((indices[0], commits))
}

struct LiftedInvocation {
    command: &'static str,
    arguments: String,
}

struct LiftedProcedure {
    command: &'static str,
    arguments: String,
    output_kinds: Vec<InkScriptEntityKind>,
}

struct ExportedAsset {
    symbol: String,
    declaration: String,
}

type EntityKey = (String, u64);
type StrictBindings = BTreeMap<EntityKey, String>;
type StrictOwners = BTreeMap<EntityKey, EntityKey>;

struct ExportLiftContext<'a> {
    core: &'a Core,
    produced: &'a BTreeMap<u64, String>,
    strict: &'a mut StrictBindings,
    assets: &'a mut BTreeMap<AssetId, ExportedAsset>,
    asset_bytes: &'a mut u64,
    maximum_asset_bytes: u64,
}

fn lift_procedure(
    procedure: &CanonicalProcedure,
    context: ExportLiftContext<'_>,
) -> Result<LiftedProcedure, InkScriptExportError> {
    let ExportLiftContext {
        core,
        produced,
        strict,
        assets,
        asset_bytes,
        maximum_asset_bytes,
    } = context;
    let metadata = match procedure.primitive_id() {
        PrimitiveId::SET_MAIN_LINE_COLOR => Some(MetadataColorGuideInvocation::SetMainLineColor(
            decode_color(procedure.canonical_arguments())
                .map_err(|_| InkScriptExportError::InvalidSource)?,
        )),
        PrimitiveId::REPLACE_PALETTE => Some(MetadataColorGuideInvocation::ReplacePalette(
            decode_palette(procedure.canonical_arguments())
                .map_err(|_| InkScriptExportError::InvalidSource)?,
        )),
        PrimitiveId::REPLACE_COLOR_CHART => {
            let (entries, locked) = decode_color_chart(procedure.canonical_arguments())
                .map_err(|_| InkScriptExportError::InvalidSource)?;
            Some(MetadataColorGuideInvocation::ReplaceColorChart { entries, locked })
        }
        _ => None,
    };
    if let Some(metadata) = metadata {
        let mut references = InkScriptRuntimeReferences::default();
        let mut ignored_bindings = String::new();
        let (command, arguments, _) = crate::primitive::inkscript_metadata::lift_arguments(
            &metadata,
            &mut ignored_bindings,
            &mut references,
        )
        .map_err(|_| InkScriptExportError::InvalidSource)?;
        let output_kinds =
            crate::primitive::inkscript_metadata::MetadataColorGuideScriptStep::output_entity_kinds(
                &metadata,
            );
        return Ok(LiftedProcedure {
            command,
            arguments,
            output_kinds,
        });
    }

    if procedure.primitive_id() == PrimitiveId::IMPORT_RASTER_ASSET {
        let plane_id = *procedure
            .input_ids()
            .first()
            .ok_or(InkScriptExportError::InvalidSource)?;
        let asset_id = *procedure
            .asset_ids()
            .first()
            .ok_or(InkScriptExportError::InvalidSource)?;
        let symbol = ensure_raster_asset(core, asset_id, assets, asset_bytes, maximum_asset_bytes)?;
        return Ok(LiftedProcedure {
            command: "import_raster_asset",
            arguments: format!(
                "plane_id = {}; raster = asset({symbol});",
                resolve_reference("plane", plane_id, produced, strict)
            ),
            output_kinds: Vec::new(),
        });
    }

    if procedure.primitive_id() == PrimitiveId::APPLY_RASTER_STROKE {
        let arguments =
            crate::primitive::decode_stroke_arguments_for_export(procedure, &core.assets)
                .map_err(|_| InkScriptExportError::InvalidSource)?;
        return Ok(LiftedProcedure {
            command: "apply_raster_stroke",
            arguments: raster_stroke_arguments(&arguments, produced, strict)?,
            output_kinds: Vec::new(),
        });
    }

    if procedure.primitive_id() == PrimitiveId::EDIT_PLANE_ALPHA {
        let runtime = procedure
            .runtime_invocation
            .as_ref()
            .ok_or(InkScriptExportError::MissingRuntimeInvocation)?;
        let CanonicalInvocation::EditPlaneAlpha { plane_id, alpha } = runtime.invocation() else {
            return Err(InkScriptExportError::InvalidSource);
        };
        let symbol = ensure_tile_raster_asset(alpha, assets, asset_bytes, maximum_asset_bytes)?;
        return Ok(LiftedProcedure {
            command: "edit_plane_alpha",
            arguments: format!(
                "plane_id = {}; alpha = asset({symbol});",
                resolve_reference("plane", *plane_id, produced, strict)
            ),
            output_kinds: Vec::new(),
        });
    }

    if procedure.primitive_id() == PrimitiveId::COMMIT_FLOATING {
        let runtime = procedure
            .runtime_invocation
            .as_ref()
            .ok_or(InkScriptExportError::MissingRuntimeInvocation)?;
        let CanonicalInvocation::CommitFloating { floating } = runtime.invocation() else {
            return Err(InkScriptExportError::InvalidSource);
        };
        if procedure.asset_ids().len() != floating.payload.planes.len() {
            return Err(InkScriptExportError::InvalidSource);
        }
        let mut plane_literals = Vec::with_capacity(floating.payload.planes.len());
        for (plane, asset_id) in floating.payload.planes.iter().zip(procedure.asset_ids()) {
            let symbol =
                ensure_raster_asset(core, *asset_id, assets, asset_bytes, maximum_asset_bytes)?;
            plane_literals.push(format!(
                "{{ kind = {}; pixel_format = {}; origin_x = {}; origin_y = {}; raster = asset({symbol}); }}",
                plane_kind_name(plane.kind),
                pixel_format_name(plane.pixel_format)?,
                plane.origin_x,
                plane.origin_y
            ));
        }
        return Ok(LiftedProcedure {
            command: "commit_floating",
            arguments: floating_arguments(floating, &plane_literals, produced, strict)?,
            output_kinds: Vec::new(),
        });
    }

    if matches!(
        procedure.primitive_id(),
        PrimitiveId::LIGHT_TABLE_ADD_ITEM
            | PrimitiveId::LIGHT_TABLE_UPDATE_ITEM
            | PrimitiveId::LIGHT_TABLE_BULK_REGISTER
    ) {
        let runtime = procedure
            .runtime_invocation
            .as_ref()
            .ok_or(InkScriptExportError::MissingRuntimeInvocation)?;
        let (command, prefix, inputs, output_kinds) = match runtime.invocation() {
            CanonicalInvocation::LightTableAddItem { input } => (
                "light_table_add_item",
                "input = ".to_owned(),
                std::slice::from_ref(input),
                vec![InkScriptEntityKind::LightTableItem],
            ),
            CanonicalInvocation::LightTableUpdateItem { item_id, input } => (
                "light_table_update_item",
                format!(
                    "item_id = {}; input = ",
                    resolve_reference("light_table_item", *item_id, produced, strict)
                ),
                std::slice::from_ref(input),
                Vec::new(),
            ),
            CanonicalInvocation::LightTableBulkRegister {
                target_set_id,
                inputs,
            } => (
                "light_table_bulk_register",
                format!(
                    "target_set_id = {}; inputs = ",
                    resolve_reference("light_table_set", *target_set_id, produced, strict)
                ),
                inputs.as_slice(),
                vec![InkScriptEntityKind::LightTableItem; inputs.len()],
            ),
            _ => return Err(InkScriptExportError::InvalidSource),
        };
        if inputs.len() != procedure.asset_ids().len()
            || output_kinds.len() != procedure.output_ids().len()
        {
            return Err(InkScriptExportError::InvalidSource);
        }
        let mut literals = Vec::with_capacity(inputs.len());
        for (input, asset_id) in inputs.iter().zip(procedure.asset_ids()) {
            if input.source.asset_id() != *asset_id {
                return Err(InkScriptExportError::InvalidSource);
            }
            let symbol =
                ensure_raster_asset(core, *asset_id, assets, asset_bytes, maximum_asset_bytes)?;
            literals.push(light_table_input_literal(input, &symbol));
        }
        let arguments = if command == "light_table_bulk_register" {
            format!("{prefix}[{}];", literals.join(", "))
        } else {
            format!("{prefix}{};", literals[0])
        };
        return Ok(LiftedProcedure {
            command,
            arguments,
            output_kinds,
        });
    }

    let runtime = procedure
        .runtime_invocation
        .as_ref()
        .ok_or(InkScriptExportError::MissingRuntimeInvocation)?;
    let lifted = lift_invocation(runtime.invocation(), produced, strict)?;
    Ok(LiftedProcedure {
        command: lifted.command,
        arguments: lifted.arguments,
        output_kinds: output_entity_kinds(runtime.invocation(), procedure.output_ids().len())?,
    })
}

fn ensure_tile_raster_asset(
    raster: &crate::TileRaster,
    assets: &mut BTreeMap<AssetId, ExportedAsset>,
    asset_bytes: &mut u64,
    maximum_asset_bytes: u64,
) -> Result<String, InkScriptExportError> {
    let input = crate::RasterAssetInput::from_tile_raster(raster, None)
        .map_err(|_| InkScriptExportError::InvalidSource)?;
    let mut store = crate::asset::AssetStore::default();
    let record = store
        .ingest_raster(input)
        .map_err(|_| InkScriptExportError::InvalidSource)?;
    let asset_id = record.id();
    if let Some(asset) = assets.get(&asset_id) {
        return Ok(asset.symbol.clone());
    }
    *asset_bytes = asset_bytes
        .checked_add(record.descriptor().logical_payload_length)
        .ok_or(InkScriptExportError::ResourceLimit)?;
    if *asset_bytes > maximum_asset_bytes {
        return Err(InkScriptExportError::ResourceLimit);
    }
    let symbol = format!("asset_{}", assets.len() + 1);
    let payload = record
        .canonical_payload()
        .map_err(|_| InkScriptExportError::InvalidSource)?;
    let declaration = raster_asset_declaration(&symbol, asset_id, &record.descriptor(), &payload)?;
    assets.insert(
        asset_id,
        ExportedAsset {
            symbol: symbol.clone(),
            declaration,
        },
    );
    Ok(symbol)
}

fn raster_stroke_arguments(
    arguments: &crate::primitive::CanonicalStrokeArguments,
    produced: &BTreeMap<u64, String>,
    strict: &mut StrictBindings,
) -> Result<String, InkScriptExportError> {
    let samples = crate::primitive::decode_stroke_payload(&arguments.payload)
        .map_err(|_| InkScriptExportError::InvalidSource)?;
    let samples = samples
        .iter()
        .map(|sample| {
            format!(
                "{{ x = {}; y = {}; pressure = {}; }}",
                fixed_q16_literal(sample.x_q16),
                fixed_q16_literal(sample.y_q16),
                sample.pressure
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "plane_id = {}; stroke = {{ tool = {}; color = {}; diameter = {}; shape = {}; smoothing = {}; start_color = {}; auto_erase = {}; pressure_size = {}; samples = [{samples}]; }};",
        resolve_reference("plane", arguments.target_plane_id, produced, strict),
        match arguments.tool_code {
            1 => "pencil",
            2 => "brush",
            3 => "eraser",
            _ => return Err(InkScriptExportError::InvalidSource),
        },
        pixel_literal(arguments.color),
        fixed_q16_literal(arguments.diameter_q16),
        match arguments.shape_code {
            1 => "round",
            2 => "square",
            _ => return Err(InkScriptExportError::InvalidSource),
        },
        arguments.smoothing,
        match arguments.start_color_code {
            0 => "any",
            1 => "exact_native",
            _ => return Err(InkScriptExportError::InvalidSource),
        },
        arguments.auto_erase,
        arguments.pressure_size
    ))
}

fn light_table_input_literal(input: &crate::LightTableItemInput, symbol: &str) -> String {
    format!(
        "{{ name = {}; source = {{ document_uuid = uuid\"{}\"; source_revision = {}; reference_frame = rect({}, {}, {}, {}); dpi_x_milli = {}; dpi_y_milli = {}; raster = asset({symbol}); }}; properties = {}; }}",
        string_literal(&input.name),
        uuid_literal(input.source.document_uuid),
        input.source.source_revision,
        input.source.reference_frame.x,
        input.source.reference_frame.y,
        input.source.reference_frame.width,
        input.source.reference_frame.height,
        input.source.dpi_x_milli,
        input.source.dpi_y_milli,
        light_table_properties_literal(crate::LightTableItemProperties {
            visible: input.visible,
            opacity_milli: input.opacity_milli,
            display_mode: input.display_mode,
            display_color: input.display_color,
            translate_x_milli: input.translate_x_milli,
            translate_y_milli: input.translate_y_milli,
            scale_x_milli: input.scale_x_milli,
            scale_y_milli: input.scale_y_milli,
            rotation_milli_degrees: input.rotation_milli_degrees,
        })
    )
}

fn light_table_properties_literal(properties: crate::LightTableItemProperties) -> String {
    format!(
        "{{ visible = {}; opacity_milli = {}; display_mode = {}; display_color = {}; translate_x_milli = {}; translate_y_milli = {}; scale_x_milli = {}; scale_y_milli = {}; rotation_milli_degrees = {}; }}",
        properties.visible,
        properties.opacity_milli,
        match properties.display_mode {
            crate::LightTableDisplayMode::Color => "color",
            crate::LightTableDisplayMode::Monotone => "monotone",
            crate::LightTableDisplayMode::Halftone => "halftone",
        },
        pixel_literal(properties.display_color),
        properties.translate_x_milli,
        properties.translate_y_milli,
        properties.scale_x_milli,
        properties.scale_y_milli,
        properties.rotation_milli_degrees
    )
}

fn floating_arguments(
    floating: &crate::selection::FloatingSelection,
    planes: &[String],
    produced: &BTreeMap<u64, String>,
    strict: &mut StrictBindings,
) -> Result<String, InkScriptExportError> {
    let destination = match &floating.destination {
        crate::selection::FloatingDestination::ExistingPlanes(ids) => format!(
            "{{ kind = existing_planes; existing_plane_ids = [{}]; new_layer_id = none; new_plane_kind = none; new_pixel_format = none; new_name = none; new_opacity_milli = none; }}",
            ids.iter()
                .map(|id| resolve_reference("plane", id.get(), produced, strict))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        crate::selection::FloatingDestination::NewPlane {
            layer_id,
            kind,
            format,
            name,
            opacity_milli,
        } => format!(
            "{{ kind = new_plane; existing_plane_ids = []; new_layer_id = {}; new_plane_kind = {}; new_pixel_format = {}; new_name = {}; new_opacity_milli = {opacity_milli}; }}",
            resolve_reference("layer", layer_id.get(), produced, strict),
            plane_kind_name(*kind),
            pixel_format_name(*format)?,
            string_literal(name)
        ),
    };
    let transform = &floating.transform;
    let target_x = inkpod_image::canonical_q16_from_f64(transform.target_x)
        .ok_or(InkScriptExportError::InvalidSource)?;
    let target_y = inkpod_image::canonical_q16_from_f64(transform.target_y)
        .ok_or(InkScriptExportError::InvalidSource)?;
    let scale_x = inkpod_image::canonical_q16_from_f64(transform.scale_x)
        .ok_or(InkScriptExportError::InvalidSource)?;
    let scale_y = inkpod_image::canonical_q16_from_f64(transform.scale_y)
        .ok_or(InkScriptExportError::InvalidSource)?;
    let turns = inkpod_image::canonical_turns_from_degrees_f64(transform.rotation_degrees)
        .ok_or(InkScriptExportError::InvalidSource)?;
    Ok(format!(
        "payload = {{ source_document_uuid = uuid\"{}\"; bounds = pixel_rect({}, {}, {}, {}); planes = [{}]; }}; destination = {destination}; transform = {{ anchor = {}; target_x = {}; target_y = {}; scale_x = {}; scale_y = {}; rotation_turns = {turns}; }};",
        uuid_literal(floating.payload.source_document_uuid),
        floating.payload.bounds.x,
        floating.payload.bounds.y,
        floating.payload.bounds.width,
        floating.payload.bounds.height,
        planes.join(", "),
        floating_anchor_name(transform.anchor),
        fixed_q16_literal(target_x),
        fixed_q16_literal(target_y),
        fixed_q16_literal(scale_x),
        fixed_q16_literal(scale_y),
    ))
}

fn uuid_literal(value: u128) -> String {
    let digits = format!("{value:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &digits[0..8],
        &digits[8..12],
        &digits[12..16],
        &digits[16..20],
        &digits[20..32]
    )
}

const fn floating_anchor_name(value: crate::FloatingTransformAnchor) -> &'static str {
    match value {
        crate::FloatingTransformAnchor::TopLeft => "top_left",
        crate::FloatingTransformAnchor::TopRight => "top_right",
        crate::FloatingTransformAnchor::Center => "center",
        crate::FloatingTransformAnchor::BottomLeft => "bottom_left",
        crate::FloatingTransformAnchor::BottomRight => "bottom_right",
    }
}

const fn plane_kind_name(value: crate::PlaneType) -> &'static str {
    match value {
        crate::PlaneType::MainLine => "main_line",
        crate::PlaneType::Color => "color",
        crate::PlaneType::Raster => "raster",
    }
}

fn pixel_format_name(value: PixelFormat) -> Result<&'static str, InkScriptExportError> {
    match value {
        PixelFormat::BinaryMask8 => Ok("mask8"),
        PixelFormat::Grayscale8 => Ok("gray8"),
        PixelFormat::Grayscale16 => Ok("gray16"),
        PixelFormat::StraightRgba8 => Ok("rgba8"),
        PixelFormat::StraightRgba16 => Ok("rgba16"),
        PixelFormat::PremultipliedBgra8 => Err(InkScriptExportError::InvalidSource),
    }
}

fn ensure_raster_asset(
    core: &Core,
    asset_id: AssetId,
    assets: &mut BTreeMap<AssetId, ExportedAsset>,
    asset_bytes: &mut u64,
    maximum_asset_bytes: u64,
) -> Result<String, InkScriptExportError> {
    if let Some(asset) = assets.get(&asset_id) {
        return Ok(asset.symbol.clone());
    }
    let record = core
        .assets
        .get(asset_id)
        .ok_or(InkScriptExportError::InvalidSource)?;
    *asset_bytes = asset_bytes
        .checked_add(record.descriptor().logical_payload_length)
        .ok_or(InkScriptExportError::ResourceLimit)?;
    if *asset_bytes > maximum_asset_bytes {
        return Err(InkScriptExportError::ResourceLimit);
    }
    let symbol = format!("asset_{}", assets.len() + 1);
    let payload = record
        .canonical_payload()
        .map_err(|_| InkScriptExportError::InvalidSource)?;
    let declaration = raster_asset_declaration(&symbol, asset_id, &record.descriptor(), &payload)?;
    assets.insert(
        asset_id,
        ExportedAsset {
            symbol: symbol.clone(),
            declaration,
        },
    );
    Ok(symbol)
}

fn output_entity_kinds(
    invocation: &CanonicalInvocation,
    output_count: usize,
) -> Result<Vec<InkScriptEntityKind>, InkScriptExportError> {
    let document_tree =
        crate::primitive::inkscript_document_tree::DocumentTreeScriptStep::output_entity_kinds(
            invocation,
        );
    if !document_tree.is_empty() {
        return if document_tree.len() == output_count {
            Ok(document_tree)
        } else {
            Err(InkScriptExportError::InvalidSource)
        };
    }
    let kinds = match invocation {
        CanonicalInvocation::AddGuide { .. } => vec![InkScriptEntityKind::Guide],
        CanonicalInvocation::SaveSelectionMask { .. } => {
            vec![InkScriptEntityKind::SavedSelectionMask]
        }
        CanonicalInvocation::EditShootingFrame { .. } => {
            vec![InkScriptEntityKind::ShootingFrame; output_count]
        }
        CanonicalInvocation::LightTableCreateSet { .. }
        | CanonicalInvocation::LightTableDuplicateSet { .. } => {
            vec![InkScriptEntityKind::LightTableSet]
        }
        CanonicalInvocation::ApplyGeometry { .. } if output_count == 0 => Vec::new(),
        _ if output_count == 0 => Vec::new(),
        _ => return Err(InkScriptExportError::InvalidSource),
    };
    if kinds.len() == output_count {
        Ok(kinds)
    } else {
        Err(InkScriptExportError::InvalidSource)
    }
}

fn lift_invocation(
    invocation: &CanonicalInvocation,
    produced: &BTreeMap<u64, String>,
    strict: &mut StrictBindings,
) -> Result<LiftedInvocation, InkScriptExportError> {
    let direct = match invocation {
        CanonicalInvocation::AddGuide { axis, position } => LiftedInvocation {
            command: "add_guide",
            arguments: format!(
                "axis = {}; position = {position};",
                match axis {
                    GuideAxis::Horizontal => "horizontal",
                    GuideAxis::Vertical => "vertical",
                }
            ),
        },
        CanonicalInvocation::MoveGuide { guide_id, position } => LiftedInvocation {
            command: "move_guide",
            arguments: format!(
                "guide_id = {}; position = {position};",
                resolve_reference("guide", *guide_id, produced, strict)
            ),
        },
        CanonicalInvocation::DeleteGuide { guide_id } => LiftedInvocation {
            command: "delete_guide",
            arguments: format!(
                "guide_id = {};",
                resolve_reference("guide", *guide_id, produced, strict)
            ),
        },
        CanonicalInvocation::SetGrid { grid } => LiftedInvocation {
            command: "set_grid",
            arguments: format!(
                "grid = {{ origin_x = {}; origin_y = {}; spacing_x = {}; spacing_y = {}; subdivisions = {}; }};",
                grid.origin_x, grid.origin_y, grid.spacing_x, grid.spacing_y, grid.subdivisions
            ),
        },
        CanonicalInvocation::DeleteAllGuides => LiftedInvocation {
            command: "delete_all_guides",
            arguments: String::new(),
        },
        CanonicalInvocation::SelectOutputColorGuard {
            profile,
            operation,
            base_revision,
        } => LiftedInvocation {
            command: "select_output_color_guard",
            arguments: format!(
                "profile = {}; operation = {}; base_revision = {base_revision};",
                match profile {
                    crate::OutputColorGuardProfile::Bt709ConservativeYCbCr => {
                        "bt709_conservative_ycbcr"
                    }
                },
                selection_operation_name(*operation)
            ),
        },
        CanonicalInvocation::ApplyGradient { plane_id, gradient } => LiftedInvocation {
            command: "apply_gradient",
            arguments: format!(
                "plane_id = {}; gradient = {};",
                resolve_reference("plane", *plane_id, produced, strict),
                gradient_literal(gradient)
            ),
        },
        CanonicalInvocation::ApplyGeometry { geometry } => LiftedInvocation {
            command: "apply_geometry",
            arguments: geometry_arguments(geometry, produced, strict),
        },
        CanonicalInvocation::ApplyBlur {
            plane_id,
            radius,
            strength_milli,
        } => LiftedInvocation {
            command: "apply_blur",
            arguments: format!(
                "plane_id = {}; radius = {radius}; strength_milli = {strength_milli};",
                resolve_reference("plane", *plane_id, produced, strict)
            ),
        },
        CanonicalInvocation::ApplyAirbrush { plane_id, stroke } => LiftedInvocation {
            command: "apply_airbrush",
            arguments: format!(
                "plane_id = {}; stroke = {};",
                resolve_reference("plane", *plane_id, produced, strict),
                airbrush_stroke_literal(stroke)
            ),
        },
        CanonicalInvocation::ApplyAirbrushGesture { plane_id, gesture } => LiftedInvocation {
            command: "apply_airbrush_gesture",
            arguments: format!(
                "plane_id = {}; gesture = {};",
                resolve_reference("plane", *plane_id, produced, strict),
                airbrush_gesture_literal(gesture)
            ),
        },
        CanonicalInvocation::ApplyStamp { plane_id, stamp } => LiftedInvocation {
            command: "apply_stamp",
            arguments: format!(
                "plane_id = {}; stamp = {};",
                resolve_reference("plane", *plane_id, produced, strict),
                stamp_literal(stamp)
            ),
        },
        CanonicalInvocation::ApplyStampGesture { plane_id, gesture } => LiftedInvocation {
            command: "apply_stamp_gesture",
            arguments: format!(
                "plane_id = {}; gesture = {};",
                resolve_reference("plane", *plane_id, produced, strict),
                stamp_gesture_literal(gesture)
            ),
        },
        CanonicalInvocation::ApplyBlurTool {
            plane_id,
            shape,
            radius,
            strength_milli,
        } => LiftedInvocation {
            command: "apply_blur_tool",
            arguments: format!(
                "plane_id = {}; shape = {}; radius = {radius}; strength_milli = {strength_milli};",
                resolve_reference("plane", *plane_id, produced, strict),
                selection_shape_literal(shape)?
            ),
        },
        CanonicalInvocation::ApplyBlurPressureTrace {
            plane_id,
            samples,
            diameter,
            radius,
            strength_milli,
        } => LiftedInvocation {
            command: "apply_blur_tool",
            arguments: format!(
                "plane_id = {}; shape = trace_brush([{}], {}); radius = {radius}; strength_milli = {strength_milli};",
                resolve_reference("plane", *plane_id, produced, strict),
                samples
                    .iter()
                    .map(|sample| Ok(format!(
                        "selection_sample({}, {}, {})",
                        q16_literal(sample.point.x)?,
                        q16_literal(sample.point.y)?,
                        q16_literal(sample.pressure)?
                    )))
                    .collect::<Result<Vec<_>, InkScriptExportError>>()?
                    .join(", "),
                q16_literal(*diameter)?
            ),
        },
        CanonicalInvocation::ApplyAlphaGradient { plane_id, gradient } => LiftedInvocation {
            command: "apply_alpha_gradient",
            arguments: format!(
                "plane_id = {}; gradient = {};",
                resolve_reference("plane", *plane_id, produced, strict),
                gradient_literal(gradient)
            ),
        },
        CanonicalInvocation::ScopedColorReplace {
            plane_id,
            mode,
            target,
            replacement,
            region,
        } => LiftedInvocation {
            command: "scoped_color_replace",
            arguments: format!(
                "plane_id = {}; mode = {}; target = {}; replacement = {}; region = {};",
                resolve_reference("plane", *plane_id, produced, strict),
                scoped_color_mode_name(*mode),
                pixel_literal(*target),
                pixel_literal(*replacement),
                match region {
                    Some(shape) => selection_shape_literal(shape)?,
                    None => "none".to_owned(),
                }
            ),
        },
        CanonicalInvocation::RestoreSelectedPixels { plane_id, changes } => LiftedInvocation {
            command: "restore_selected_pixels",
            arguments: format!(
                "plane_id = {}; changes = [{}];",
                resolve_reference("plane", *plane_id, produced, strict),
                changes
                    .iter()
                    .map(|change| format!(
                        "{{ x = {}; y = {}; before = {}; after = {}; }}",
                        change.x,
                        change.y,
                        pixel_literal(change.before),
                        pixel_literal(change.after)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
        CanonicalInvocation::ApplySelection {
            shape,
            operation,
            interpretation,
            options,
            target,
        } => LiftedInvocation {
            command: "apply_selection",
            arguments: format!(
                "shape = {}; operation = {}; interpretation = {}; options = {}; target_layer_id = {}; target_plane_id = {};",
                selection_shape_literal(shape)?,
                selection_operation_name(*operation),
                range_interpretation_name(*interpretation),
                selection_options_literal(options),
                resolve_reference("layer", target.layer_id, produced, strict),
                resolve_reference("plane", target.plane_id, produced, strict)
            ),
        },
        CanonicalInvocation::InvertSelection => LiftedInvocation {
            command: "invert_selection",
            arguments: String::new(),
        },
        CanonicalInvocation::ClearSelection => LiftedInvocation {
            command: "clear_selection",
            arguments: String::new(),
        },
        CanonicalInvocation::ResizeSelection { pixels } => LiftedInvocation {
            command: "resize_selection",
            arguments: format!("pixels = {pixels};"),
        },
        CanonicalInvocation::SelectColor {
            color,
            tolerance,
            different,
            operation,
            target,
        } => LiftedInvocation {
            command: "select_color",
            arguments: format!(
                "color = {}; tolerance = {tolerance}; different = {different}; operation = {}; target_layer_id = {}; target_plane_id = {};",
                pixel_literal(*color),
                selection_operation_name(*operation),
                resolve_reference("layer", target.layer_id, produced, strict),
                resolve_reference("plane", target.plane_id, produced, strict)
            ),
        },
        CanonicalInvocation::SaveSelectionMask { name } => LiftedInvocation {
            command: "save_selection_mask",
            arguments: format!("name = {};", string_literal(name)),
        },
        CanonicalInvocation::ApplySavedSelectionMask {
            saved_selection_id,
            operation,
        } => LiftedInvocation {
            command: "apply_saved_selection_mask",
            arguments: format!(
                "saved_selection_id = {}; operation = {};",
                resolve_reference(
                    "saved_selection_mask",
                    saved_selection_id.get(),
                    produced,
                    strict
                ),
                saved_selection_operation_name(*operation)
            ),
        },
        CanonicalInvocation::RenameSavedSelectionMask {
            saved_selection_id,
            name,
        } => LiftedInvocation {
            command: "rename_saved_selection_mask",
            arguments: format!(
                "saved_selection_id = {}; name = {};",
                resolve_reference(
                    "saved_selection_mask",
                    saved_selection_id.get(),
                    produced,
                    strict
                ),
                string_literal(name)
            ),
        },
        CanonicalInvocation::DeleteSavedSelectionMask { saved_selection_id } => LiftedInvocation {
            command: "delete_saved_selection_mask",
            arguments: format!(
                "saved_selection_id = {};",
                resolve_reference(
                    "saved_selection_mask",
                    saved_selection_id.get(),
                    produced,
                    strict
                )
            ),
        },
        CanonicalInvocation::ClearSelectedContent { target } => LiftedInvocation {
            command: "clear_selected_content",
            arguments: format!(
                "target_layer_id = {}; target_plane_id = {};",
                resolve_reference("layer", target.layer_id, produced, strict),
                resolve_reference("plane", target.plane_id, produced, strict)
            ),
        },
        CanonicalInvocation::EditShootingFrame { edit } => LiftedInvocation {
            command: "edit_shooting_frame",
            arguments: format!(
                "edit = {};",
                shooting_frame_edit_literal(*edit, produced, strict)
            ),
        },
        CanonicalInvocation::LightTableSetGlobalOpacity { opacity_milli } => LiftedInvocation {
            command: "light_table_set_global_opacity",
            arguments: format!("opacity_milli = {opacity_milli};"),
        },
        CanonicalInvocation::LightTableCreateSet { name } => LiftedInvocation {
            command: "light_table_create_set",
            arguments: format!("name = {};", string_literal(name)),
        },
        CanonicalInvocation::LightTableDuplicateSet { set_id } => LiftedInvocation {
            command: "light_table_duplicate_set",
            arguments: format!(
                "set_id = {};",
                resolve_reference("light_table_set", *set_id, produced, strict)
            ),
        },
        CanonicalInvocation::LightTableDeleteSet { set_id } => LiftedInvocation {
            command: "light_table_delete_set",
            arguments: format!(
                "set_id = {};",
                resolve_reference("light_table_set", *set_id, produced, strict)
            ),
        },
        CanonicalInvocation::LightTableRenameSet { set_id, name } => LiftedInvocation {
            command: "light_table_rename_set",
            arguments: format!(
                "set_id = {}; name = {};",
                resolve_reference("light_table_set", *set_id, produced, strict),
                string_literal(name)
            ),
        },
        CanonicalInvocation::LightTableReorderSet {
            set_id,
            destination_index,
        } => LiftedInvocation {
            command: "light_table_reorder_set",
            arguments: format!(
                "set_id = {}; destination_index = {destination_index};",
                resolve_reference("light_table_set", *set_id, produced, strict)
            ),
        },
        CanonicalInvocation::LightTableSetActive { set_id } => LiftedInvocation {
            command: "light_table_set_active",
            arguments: format!(
                "set_id = {};",
                resolve_reference("light_table_set", *set_id, produced, strict)
            ),
        },
        CanonicalInvocation::LightTableUpdateItemProperties {
            item_id,
            properties,
        } => LiftedInvocation {
            command: "light_table_update_item_properties",
            arguments: format!(
                "item_id = {}; properties = {};",
                resolve_reference("light_table_item", *item_id, produced, strict),
                light_table_properties_literal(*properties)
            ),
        },
        CanonicalInvocation::LightTableRemoveItem { item_id } => LiftedInvocation {
            command: "light_table_remove_item",
            arguments: format!(
                "item_id = {};",
                resolve_reference("light_table_item", *item_id, produced, strict)
            ),
        },
        CanonicalInvocation::LightTableReorderItem {
            item_id,
            destination_index,
        } => LiftedInvocation {
            command: "light_table_reorder_item",
            arguments: format!(
                "item_id = {}; destination_index = {destination_index};",
                resolve_reference("light_table_item", *item_id, produced, strict)
            ),
        },
        CanonicalInvocation::UpdatePaperFrames { .. }
        | CanonicalInvocation::CreateLayer { .. }
        | CanonicalInvocation::DuplicateLayer { .. }
        | CanonicalInvocation::DeleteLayer { .. }
        | CanonicalInvocation::ReorderLayer { .. }
        | CanonicalInvocation::SetLayerProperties { .. }
        | CanonicalInvocation::CreatePlane { .. }
        | CanonicalInvocation::DuplicatePlane { .. }
        | CanonicalInvocation::DeletePlane { .. }
        | CanonicalInvocation::ReorderPlane { .. }
        | CanonicalInvocation::SetPlaneProperties { .. }
        | CanonicalInvocation::ConvertPlane { .. }
        | CanonicalInvocation::MergePlane { .. }
        | CanonicalInvocation::MergeLayer { .. }
        | CanonicalInvocation::DeleteHiddenLayers
        | CanonicalInvocation::EditTargets { .. }
        | CanonicalInvocation::ApplyFill { .. }
        | CanonicalInvocation::ApplyBoundaryAirbrush { .. }
        | CanonicalInvocation::ApplyDustRemoval { .. }
        | CanonicalInvocation::ApplyFilter { .. }
        | CanonicalInvocation::ReplaceRasterColors { .. }
        | CanonicalInvocation::SeparateRasterColors { .. }
        | CanonicalInvocation::MirrorDocument { .. }
        | CanonicalInvocation::RotateDocument { .. }
        | CanonicalInvocation::ResizeDocument { .. } => {
            return lift_adapter_invocation(invocation, produced, strict);
        }
        CanonicalInvocation::EditPlaneAlpha { .. }
        | CanonicalInvocation::CommitFloating { .. }
        | CanonicalInvocation::LightTableAddItem { .. }
        | CanonicalInvocation::LightTableUpdateItem { .. }
        | CanonicalInvocation::LightTableBulkRegister { .. }
        | CanonicalInvocation::ApplyBatchOperations { .. } => {
            return Err(InkScriptExportError::InvalidSource);
        }
    };
    Ok(direct)
}

fn geometry_arguments(
    geometry: &crate::geometry::CanonicalGeometry,
    produced: &BTreeMap<u64, String>,
    strict: &mut StrictBindings,
) -> String {
    let point = |point: crate::geometry::CanonicalGeometryPoint| {
        format!(
            "point({}, {})",
            fixed_q16_literal(point.x_q16),
            fixed_q16_literal(point.y_q16)
        )
    };
    let segments = geometry
        .segments
        .iter()
        .map(|segment| {
            format!(
                "{{ p0 = {}; p1 = {}; p2 = {}; p3 = {}; width_start = {}; width_end = {}; }}",
                point(segment.p0),
                point(segment.p1),
                point(segment.p2),
                point(segment.p3),
                fixed_q16_literal(segment.width_start_q16),
                fixed_q16_literal(segment.width_end_q16)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let boundary = geometry
        .fill_boundary
        .iter()
        .copied()
        .map(point)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "plane_id = {}; primitive = {}; segments = [{segments}]; fill_boundary = [{boundary}]; outline_color = {}; fill_color = {}; outline_width = {}; cross_section = {}; outline = {}; fill = {}; closed = {};",
        resolve_reference("plane", geometry.plane_id, produced, strict),
        match geometry.primitive {
            crate::GeometryPrimitive::Line => "line",
            crate::GeometryPrimitive::Curve => "curve",
            crate::GeometryPrimitive::Rectangle => "rectangle",
            crate::GeometryPrimitive::Ellipse => "ellipse",
            crate::GeometryPrimitive::Polygon => "polygon",
            crate::GeometryPrimitive::Polyline => "polyline",
        },
        pixel_literal(geometry.outline_color),
        pixel_literal(geometry.fill_color),
        fixed_q16_literal(geometry.outline_width_q16),
        match geometry.cross_section {
            crate::GeometryCrossSection::Round => "round",
            crate::GeometryCrossSection::Square => "square",
        },
        geometry.outline,
        geometry.fill,
        geometry.closed
    )
}

fn shooting_frame_edit_literal(
    edit: crate::ShootingFrameEdit,
    produced: &BTreeMap<u64, String>,
    strict: &mut StrictBindings,
) -> String {
    match edit {
        crate::ShootingFrameEdit::Create(input) => format!(
            "{{ operation = 1; frame_id = none; input = {}; }}",
            shooting_frame_input_literal(input)
        ),
        crate::ShootingFrameEdit::Update { frame_id, input } => format!(
            "{{ operation = 2; frame_id = {}; input = {}; }}",
            resolve_reference("shooting_frame", frame_id, produced, strict),
            shooting_frame_input_literal(input)
        ),
        crate::ShootingFrameEdit::Delete { frame_id } => format!(
            "{{ operation = 3; frame_id = {}; input = none; }}",
            resolve_reference("shooting_frame", frame_id, produced, strict)
        ),
    }
}

fn shooting_frame_input_literal(input: crate::ShootingFrameInput) -> String {
    format!(
        "{{ center_x_milli = {}; center_y_milli = {}; width_milli = {}; height_milli = {}; rotation_turns = {}; anchor = {}; visible = {}; }}",
        input.center_x_milli,
        input.center_y_milli,
        input.width_milli,
        input.height_milli,
        input.rotation_turns,
        shooting_frame_anchor_name(input.anchor),
        input.visible
    )
}

const fn shooting_frame_anchor_name(value: crate::ShootingFrameAnchor) -> &'static str {
    match value {
        crate::ShootingFrameAnchor::TopLeft => "top_left",
        crate::ShootingFrameAnchor::TopRight => "top_right",
        crate::ShootingFrameAnchor::Center => "center",
        crate::ShootingFrameAnchor::BottomLeft => "bottom_left",
        crate::ShootingFrameAnchor::BottomRight => "bottom_right",
    }
}

fn fixed_q16_literal(value: i64) -> String {
    let negative = value < 0;
    let magnitude = value.unsigned_abs();
    let integer = magnitude >> 16;
    let fraction = magnitude & 0xffff;
    if fraction == 0 {
        return format!("{}{}.0", if negative { "-" } else { "" }, integer);
    }
    let decimal = (u128::from(fraction) * 1_000_000_000_u128 + 32_768) / 65_536;
    let mut text = format!("{decimal:09}");
    while text.ends_with('0') {
        text.pop();
    }
    format!("{}{}.{text}", if negative { "-" } else { "" }, integer)
}

fn selection_options_literal(options: &crate::SelectionConstructionOptions) -> String {
    format!(
        "{{ aspect_ratio_q16 = {}; from_center = {}; constrain_rotation_45 = {}; rotation_turns = {}; trace = {{ shape = {}; pressure_size = {}; screen_size = {}; view_zoom = {}; }}; }}",
        options.aspect_ratio_q16,
        options.from_center,
        options.constrain_rotation_45,
        options.rotation_turns,
        match options.trace.shape {
            crate::TraceBrushShape::Round => "round",
            crate::TraceBrushShape::Square => "square",
        },
        options.trace.pressure_size,
        options.trace.screen_size,
        fixed_q16_literal(options.trace.view_zoom_q16)
    )
}

const fn range_interpretation_name(value: crate::RangeInterpretation) -> &'static str {
    match value {
        crate::RangeInterpretation::Normal => "normal",
        crate::RangeInterpretation::Tight => "tight",
        crate::RangeInterpretation::EnclosedInterior => "enclosed_interior",
        crate::RangeInterpretation::Drawing => "drawing",
        crate::RangeInterpretation::Boundary => "boundary",
    }
}

const fn saved_selection_operation_name(value: crate::SavedSelectionOperation) -> &'static str {
    match value {
        crate::SavedSelectionOperation::Replace => "replace",
        crate::SavedSelectionOperation::Add => "add",
        crate::SavedSelectionOperation::Subtract => "subtract",
    }
}

fn q16_literal(value: f32) -> Result<String, InkScriptExportError> {
    crate::primitive::inkscript_batch::q16_literal(value)
        .map_err(|_| InkScriptExportError::InvalidSource)
}

fn selection_shape_literal(shape: &crate::SelectionShape) -> Result<String, InkScriptExportError> {
    crate::primitive::inkscript_batch::selection_shape_literal(shape)
        .map_err(|_| InkScriptExportError::InvalidSource)
}

fn pixel_literal(value: crate::PixelValue) -> String {
    crate::primitive::inkscript_batch::pixel_literal(value)
}

fn rgba16_literal(value: [u16; 4]) -> String {
    crate::primitive::inkscript_batch::rgba16_literal(value)
}

fn effect_samples_literal(samples: &[crate::EffectSample]) -> String {
    samples
        .iter()
        .map(|sample| {
            format!(
                "{{ position = point({}, {}); pressure_milli = {}; }}",
                milli_q16_literal(sample.x_milli),
                milli_q16_literal(sample.y_milli),
                sample.pressure_milli
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn airbrush_stroke_literal(stroke: &crate::AirbrushStroke) -> String {
    format!(
        "{{ center = point({}, {}); radius_milli = {}; hardness_milli = {}; opacity_milli = {}; color = {}; }}",
        milli_q16_literal(stroke.center_x_milli),
        milli_q16_literal(stroke.center_y_milli),
        stroke.radius_milli,
        stroke.hardness_milli,
        stroke.opacity_milli,
        rgba16_literal(stroke.color)
    )
}

fn airbrush_gesture_literal(gesture: &crate::AirbrushGesture) -> String {
    format!(
        "{{ samples = [{}]; radius_milli = {}; hardness_milli = {}; spacing_milli = {}; opacity_milli = {}; fade_milli = {}; pressure_size = {}; pressure_opacity = {}; continuous_dabs = {}; color = {}; }}",
        effect_samples_literal(&gesture.samples),
        gesture.radius_milli,
        gesture.hardness_milli,
        gesture.spacing_milli,
        gesture.opacity_milli,
        gesture.fade_milli,
        gesture.pressure_size,
        gesture.pressure_opacity,
        gesture.continuous_dabs,
        rgba16_literal(gesture.color)
    )
}

fn stamp_literal(stamp: &crate::Stamp) -> String {
    format!(
        "{{ source_x = {}; source_y = {}; destination_x = {}; destination_y = {}; width = {}; height = {}; opacity_milli = {}; }}",
        stamp.source_x,
        stamp.source_y,
        stamp.destination_x,
        stamp.destination_y,
        stamp.width,
        stamp.height,
        stamp.opacity_milli
    )
}

fn stamp_gesture_literal(gesture: &crate::StampGesture) -> String {
    format!(
        "{{ source = point({}, {}); samples = [{}]; radius_milli = {}; hardness_milli = {}; spacing_milli = {}; opacity_milli = {}; shape = {}; pressure_size = {}; pressure_opacity = {}; }}",
        milli_q16_literal(gesture.source_x_milli),
        milli_q16_literal(gesture.source_y_milli),
        effect_samples_literal(&gesture.samples),
        gesture.radius_milli,
        gesture.hardness_milli,
        gesture.spacing_milli,
        gesture.opacity_milli,
        match gesture.shape {
            crate::StampShape::Round => "round",
            crate::StampShape::Square => "square",
        },
        gesture.pressure_size,
        gesture.pressure_opacity
    )
}

const fn scoped_color_mode_name(mode: crate::ScopedColorReplaceMode) -> &'static str {
    match mode {
        crate::ScopedColorReplaceMode::RasterColor => "raster_color",
        crate::ScopedColorReplaceMode::RasterMainLine => "raster_main_line",
    }
}

fn string_literal(value: &str) -> String {
    let mut text = String::from("\"");
    for character in value.chars() {
        match character {
            '\\' => text.push_str("\\\\"),
            '"' => text.push_str("\\\""),
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            '\t' => text.push_str("\\t"),
            '\0' => text.push_str("\\0"),
            character => text.push(character),
        }
    }
    text.push('"');
    text
}

fn gradient_literal(gradient: &crate::Gradient) -> String {
    let stops = gradient
        .stops
        .iter()
        .map(|stop| {
            format!(
                "{{ position_milli = {}; color = rgba16({}, {}, {}, {}); }}",
                stop.position_milli, stop.color[0], stop.color[1], stop.color[2], stop.color[3]
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{{ kind = {}; mode = {}; start = point({}, {}); end = point({}, {}); dither = {}; stops = [{stops}]; }}",
        match gradient.kind {
            crate::GradientKind::Linear => "linear",
            crate::GradientKind::Radial => "radial",
        },
        match gradient.mode {
            crate::GradientMode::Composite => "composite",
            crate::GradientMode::Overwrite => "overwrite",
        },
        milli_q16_literal(gradient.start_x_milli),
        milli_q16_literal(gradient.start_y_milli),
        milli_q16_literal(gradient.end_x_milli),
        milli_q16_literal(gradient.end_y_milli),
        gradient.dither,
    )
}

fn milli_q16_literal(value: i64) -> String {
    let negative = value < 0;
    let magnitude = value.unsigned_abs();
    let integer = magnitude / 1_000;
    let fraction = magnitude % 1_000;
    if fraction == 0 {
        format!("{}{}.0", if negative { "-" } else { "" }, integer)
    } else {
        format!(
            "{}{}.{fraction:03}",
            if negative { "-" } else { "" },
            integer
        )
    }
}

const fn selection_operation_name(operation: crate::SelectionOperation) -> &'static str {
    match operation {
        crate::SelectionOperation::New => "new",
        crate::SelectionOperation::Add => "add",
        crate::SelectionOperation::Subtract => "subtract",
        crate::SelectionOperation::Intersect => "intersect",
    }
}

fn lift_adapter_invocation(
    invocation: &CanonicalInvocation,
    produced: &BTreeMap<u64, String>,
    strict: &mut StrictBindings,
) -> Result<LiftedInvocation, InkScriptExportError> {
    if let Ok((command, binding, arguments)) =
        crate::primitive::inkscript::lift_arguments(invocation)
    {
        let references = binding
            .into_iter()
            .map(|(kind, id)| ("target".to_owned(), kind, id))
            .collect::<Vec<_>>();
        return Ok(LiftedInvocation {
            command,
            arguments: resolve_lifted_references(arguments, &references, produced, strict),
        });
    }

    let mut references = InkScriptRuntimeReferences::default();
    let mut ignored_bindings = String::new();
    if let Ok((command, arguments, _)) = crate::primitive::inkscript_document_tree::lift_arguments(
        invocation,
        &mut ignored_bindings,
        &mut references,
    ) {
        let references = references
            .entries()
            .map(|(name, kind, id)| (name.to_owned(), kind, id))
            .collect::<Vec<_>>();
        return Ok(LiftedInvocation {
            command,
            arguments: resolve_lifted_references(arguments, &references, produced, strict),
        });
    }

    if let Ok((command, layer_id, plane_id, arguments)) =
        crate::primitive::inkscript_batch::lift_arguments(invocation)
    {
        let mut references = Vec::new();
        if let Some(layer_id) = layer_id {
            references.push((
                "target_layer".to_owned(),
                InkScriptEntityKind::Layer,
                layer_id,
            ));
        }
        references.push((
            "target_plane".to_owned(),
            InkScriptEntityKind::Plane,
            plane_id,
        ));
        return Ok(LiftedInvocation {
            command,
            arguments: resolve_lifted_references(arguments, &references, produced, strict),
        });
    }

    let metadata = match invocation {
        CanonicalInvocation::AddGuide { .. }
        | CanonicalInvocation::MoveGuide { .. }
        | CanonicalInvocation::DeleteGuide { .. }
        | CanonicalInvocation::SetGrid { .. }
        | CanonicalInvocation::DeleteAllGuides => {
            MetadataColorGuideInvocation::Document(invocation.clone())
        }
        _ => {
            return Err(InkScriptExportError::UnsupportedPrimitive(
                invocation.primitive_id(),
            ));
        }
    };
    let mut references = InkScriptRuntimeReferences::default();
    let mut ignored_bindings = String::new();
    let (command, arguments, _) = crate::primitive::inkscript_metadata::lift_arguments(
        &metadata,
        &mut ignored_bindings,
        &mut references,
    )
    .map_err(|_| InkScriptExportError::InvalidSource)?;
    let references = references
        .entries()
        .map(|(name, kind, id)| (name.to_owned(), kind, id))
        .collect::<Vec<_>>();
    Ok(LiftedInvocation {
        command,
        arguments: resolve_lifted_references(arguments, &references, produced, strict),
    })
}

fn resolve_lifted_references(
    mut arguments: String,
    references: &[(String, InkScriptEntityKind, u64)],
    produced: &BTreeMap<u64, String>,
    strict: &mut StrictBindings,
) -> String {
    for (source_name, kind, id) in references {
        let replacement = produced.get(id).cloned().unwrap_or_else(|| {
            let key = (kind.name().to_owned(), *id);
            let count = strict.len() + 1;
            let name = strict
                .entry(key)
                .or_insert_with(|| format!("external_{}_{count}", kind.name()));
            format!("${name}")
        });
        arguments = arguments.replace(&format!("${source_name}"), &replacement);
    }
    arguments
}

fn resolve_reference(
    entity: &str,
    persistent_id: u64,
    produced: &BTreeMap<u64, String>,
    strict: &mut StrictBindings,
) -> String {
    produced.get(&persistent_id).cloned().unwrap_or_else(|| {
        let key = (entity.to_owned(), persistent_id);
        let count = strict.len() + 1;
        let name = strict
            .entry(key)
            .or_insert_with(|| format!("external_{entity}_{count}"));
        format!("${name}")
    })
}

fn raster_asset_declaration(
    symbol: &str,
    asset_id: AssetId,
    descriptor: &AssetDescriptor,
    payload: &[u8],
) -> Result<String, InkScriptExportError> {
    if descriptor.kind != AssetKind::CanonicalRaster
        || u64::try_from(payload.len()).ok() != Some(descriptor.logical_payload_length)
    {
        return Err(InkScriptExportError::InvalidSource);
    }
    let pixel_format = match descriptor.pixel_format {
        Some(PixelFormat::BinaryMask8) => "mask8",
        Some(PixelFormat::Grayscale8) => "gray8",
        Some(PixelFormat::Grayscale16) => "gray16",
        Some(PixelFormat::StraightRgba8) => "rgba8",
        Some(PixelFormat::StraightRgba16) => "rgba16",
        Some(PixelFormat::PremultipliedBgra8) | None => {
            return Err(InkScriptExportError::InvalidSource);
        }
    };
    let width = descriptor
        .width
        .ok_or(InkScriptExportError::InvalidSource)?;
    let height = descriptor
        .height
        .ok_or(InkScriptExportError::InvalidSource)?;
    let stride = descriptor
        .canonical_stride
        .ok_or(InkScriptExportError::InvalidSource)?;
    let mut digest = String::with_capacity(64);
    for byte in asset_id.as_bytes() {
        use std::fmt::Write as _;
        write!(digest, "{byte:02x}").map_err(|_| InkScriptExportError::InvalidSource)?;
    }
    Ok(format!(
        "asset {symbol} {{ asset_id = blake3\"{digest}\"; kind = \"canonical_raster\"; descriptor = {{ pixel_format = {pixel_format}; color_space = srgb; alpha = straight; width = {width}; height = {height}; stride = {stride}; element_count = {}; }}; data = base64\"\"\"{}\"\"\"; }};",
        descriptor.logical_element_count,
        encode_base64(payload)
    ))
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(char::from(TABLE[usize::from(first >> 2)]));
        output.push(char::from(
            TABLE[usize::from(((first & 0x03) << 4) | (second >> 4))],
        ));
        output.push(if chunk.len() > 1 {
            char::from(TABLE[usize::from(((second & 0x0f) << 2) | (third >> 6))])
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            char::from(TABLE[usize::from(third & 0x3f)])
        } else {
            '='
        });
    }
    output
}

fn register_results(
    catalog: &super::catalog::InkScriptCatalogView,
    schemas: &[InkScriptCommandSchema],
    command: &str,
    alias: &str,
    output_ids: &[u64],
    output_kinds: &[InkScriptEntityKind],
    produced: &mut BTreeMap<u64, String>,
) -> Result<(), InkScriptExportError> {
    let entry = catalog
        .entry(command)
        .map_err(|_| InkScriptExportError::InvalidSource)?;
    let schema = schemas
        .iter()
        .find(|schema| schema.name() == command)
        .ok_or(InkScriptExportError::InvalidSource)?;
    for result in &entry.results {
        let result_schema = schema
            .results()
            .iter()
            .find(|candidate| candidate.name() == result.name)
            .ok_or(InkScriptExportError::InvalidSource)?;
        match result_schema.cardinality() {
            InkScriptResultCardinality::Scalar => {
                let ordinal = result
                    .output_id_ordinal
                    .ok_or(InkScriptExportError::InvalidSource)?;
                let id = *output_ids
                    .get(usize::from(ordinal))
                    .ok_or(InkScriptExportError::InvalidSource)?;
                if produced
                    .insert(id, format!("${alias}.{}", result.name))
                    .is_some()
                {
                    return Err(InkScriptExportError::InvalidSource);
                }
            }
            InkScriptResultCardinality::OrderedList => {
                let role = result
                    .owner_role
                    .ok_or(InkScriptExportError::InvalidSource)?;
                if output_ids.len() != output_kinds.len() {
                    return Err(InkScriptExportError::InvalidSource);
                }
                for (index, id) in output_ids
                    .iter()
                    .zip(output_kinds)
                    .filter(|(_, kind)| kind.name() == role)
                    .map(|(id, _)| id)
                    .enumerate()
                {
                    if produced
                        .insert(*id, format!("${alias}.{}[{index}]", result.name))
                        .is_some()
                    {
                        return Err(InkScriptExportError::InvalidSource);
                    }
                }
            }
        }
    }
    Ok(())
}

const fn public_portability(value: &InkScriptPortability) -> InkScriptExportPortability {
    match value.class {
        InkScriptPortabilityClass::Portable => InkScriptExportPortability::Portable,
        InkScriptPortabilityClass::RequiresBinding => InkScriptExportPortability::RequiresBinding,
        InkScriptPortabilityClass::StrictSourceOnly => InkScriptExportPortability::StrictSourceOnly,
    }
}

fn poll(cancelled: &mut dyn FnMut() -> bool) -> Result<(), InkScriptExportError> {
    if cancelled() {
        Err(InkScriptExportError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::assets::{ScriptAssetLimits, freeze_inkscript_assets};
    use crate::script::compile::compile_inkscript;
    use crate::script::execute::run_inkscript_on_staged_core;
    use crate::{
        AssetAlphaSemantics, AssetColorSpace, DEFAULT_DPI_MILLI, PrimitiveRequest, RasterAssetInput,
    };
    use inkpod_format::InkScriptRunParameterDecision;

    #[test]
    fn exported_inline_asset_replays_with_exact_retained_identity() {
        let mut source_core = Core::new();
        let document = source_core
            .new_cell_with_uuid(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x24ff)
            .unwrap();
        source_core
            .execute_primitive(PrimitiveRequest::ImportRasterAsset {
                expected_revision: document.document_revision,
                target_plane_id: document.color_plane_id,
                raster: RasterAssetInput {
                    width: 2,
                    height: 1,
                    pixel_format: PixelFormat::StraightRgba8,
                    color_space: Some(AssetColorSpace::Srgb),
                    alpha_semantics: AssetAlphaSemantics::Straight,
                    canonical_stride: 8,
                    pixels: vec![1, 2, 3, 255, 4, 5, 6, 128],
                    expected_id: None,
                },
            })
            .unwrap();
        let event = source_core
            .journal_entries()
            .iter()
            .find_map(|entry| match entry {
                JournalEntry::Commit(commit) => Some(commit.event_id()),
                JournalEntry::HistoryMove(_) | JournalEntry::BranchCut(_) => None,
            })
            .unwrap();
        let mut never_cancel = || false;
        let exported =
            export_inkscript_fragment(&source_core, &[event], &mut never_cancel).unwrap();
        let text = exported
            .text()
            .replacen("inkscript_fragment 2;", "inkscript 2;", 1)
            .replacen("program {", "inputs { current_document; }\nprogram {", 1);
        let text = format!(
            "{text}output {{ policy = duplicate; format = inkpod; folder = \"out\"; cell_folder = false; basename = \"asset\"; start_number = 1; direction = ascending; }}\nexecution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}\n"
        );
        let source = InkScriptSource::new(InkScriptSourceId::new(2401), text.as_bytes()).unwrap();
        let program =
            compile_inkscript(&source, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
        let assets = freeze_inkscript_assets(
            program.model.assets(),
            &mut [],
            ScriptAssetLimits::exact_current(),
            &mut never_cancel,
        )
        .unwrap();
        let expected_asset = source_core
            .journal_entries()
            .iter()
            .find_map(|entry| match entry {
                JournalEntry::Commit(commit) => commit.procedure().asset_ids().first().copied(),
                JournalEntry::HistoryMove(_) | JournalEntry::BranchCut(_) => None,
            })
            .unwrap();
        assert_eq!(assets.asset_id("asset_1"), Some(expected_asset));

        let mut base = Core::new();
        base.new_cell_with_uuid(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x24ff)
            .unwrap();
        let mut replay =
            run_inkscript_on_staged_core(&program, base, Some(&assets), &mut never_cancel).unwrap();
        assert_eq!(
            replay.staged.document_state_digest().unwrap(),
            source_core.document_state_digest().unwrap()
        );
        replay.staged.release_history_cache().unwrap();
        assert_eq!(
            replay
                .staged
                .verify_journal_replay()
                .unwrap()
                .document_state_digest(),
            source_core.document_state_digest().unwrap()
        );
    }
}
