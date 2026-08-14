//! Single validation, canonicalization, execution, and atomic publish boundary.

use super::*;
use crate::document::ensure_editable_plane;
use crate::primitive::digest::{
    advance_canonical_document_state_cache, canonical_document_state_cache, color_bytes,
    decode_color,
};
use crate::primitive::raster::{apply as apply_raster_stroke, canonicalize as canonicalize_stroke};
use crate::*;

const METADATA_PRIMITIVE_SCHEMA_VERSION: u16 = 1;
const RASTER_STROKE_PRIMITIVE_SCHEMA_VERSION: u16 = 3;
const IMPORT_RASTER_ASSET_PRIMITIVE_SCHEMA_VERSION: u16 = 1;
const MAX_INLINE_PROCEDURE_PAYLOAD_BYTES: usize = 4 * 1_024 * 1_024;

pub(super) const fn current_primitive_schema_version(primitive_id: PrimitiveId) -> Option<u16> {
    if primitive_id.get() == PrimitiveId::SET_MAIN_LINE_COLOR.get()
        || primitive_id.get() == PrimitiveId::REPLACE_PALETTE.get()
        || primitive_id.get() == PrimitiveId::REPLACE_COLOR_CHART.get()
    {
        Some(METADATA_PRIMITIVE_SCHEMA_VERSION)
    } else if primitive_id.get() == PrimitiveId::APPLY_RASTER_STROKE.get() {
        Some(RASTER_STROKE_PRIMITIVE_SCHEMA_VERSION)
    } else if primitive_id.get() == PrimitiveId::IMPORT_RASTER_ASSET.get() {
        Some(IMPORT_RASTER_ASSET_PRIMITIVE_SCHEMA_VERSION)
    } else {
        super::invocation::schema_version(primitive_id)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CachePolicy {
    Preserve,
    InvalidateAll,
    RasterRevision,
}

struct PrimitiveTransaction {
    working: CellDocument,
    next_stable_id: StableIdCursor,
    output_ids: Vec<u64>,
}

impl PrimitiveTransaction {
    fn begin(core: &Core) -> Result<Self, CoreError> {
        let working = core.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        Ok(Self {
            working,
            next_stable_id: core.next_id,
            output_ids: Vec::new(),
        })
    }
}

struct CanonicalizedRequest {
    primitive: CanonicalPrimitive,
    primitive_id: PrimitiveId,
    input_ids: Vec<u64>,
    asset_ids: Vec<AssetId>,
    arguments: Vec<u8>,
    staged_assets: Option<asset::AssetStore>,
}

impl CanonicalizedRequest {
    fn execution_payload(&self) -> &[u8] {
        match &self.primitive {
            CanonicalPrimitive::ApplyRasterStroke(arguments) => &arguments.payload,
            CanonicalPrimitive::SetMainLineColor(_)
            | CanonicalPrimitive::ReplacePalette(_)
            | CanonicalPrimitive::ReplaceColorChart { .. }
            | CanonicalPrimitive::ImportRasterAsset { .. } => &[],
        }
    }

    fn procedure_payload(&self) -> &[u8] {
        if self.asset_ids.is_empty() {
            self.execution_payload()
        } else {
            &[]
        }
    }
}

struct AppliedPrimitive {
    history: HistoryChange,
    cache_policy: CachePolicy,
}

impl Core {
    /// Validates, canonicalizes, executes, and explicitly commits one primitive.
    ///
    /// The request is evaluated against a private working document. Invalid,
    /// stale, overflowing, or semantic no-op work publishes no document state,
    /// procedure/state ID, history, revision, dirty state, or render-cache
    /// change. Validation may cold-fill the private canonical-digest memo, which
    /// is not semantic or renderer-visible state.
    ///
    /// `request` owns all scalar and variable arguments for the duration of this
    /// call; it contains no borrowed frontend buffer, callback, or external path.
    /// The caller must hold the Core's single-writer authority. The synchronous
    /// operation has no cancellation callback and returns only after commit,
    /// semantic no-op, or an explicit [`CoreError`]. Invalid input is reported
    /// without panicking or partially consuming document/procedure/state IDs.
    pub fn execute_primitive(
        &mut self,
        request: PrimitiveRequest,
    ) -> Result<PrimitiveOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if request.expected_revision() != self.document_revision.get() {
            return Err(CoreError::InvalidState(
                "primitive request revision is stale",
            ));
        }
        let canonical = self.canonicalize_primitive(request)?;
        self.execute_canonical(canonical, None)
    }

    /// Replays one canonical procedure through the same working-state executor.
    ///
    /// Replay requires the exact next Procedure/State IDs, current replay epoch,
    /// base state, pre-state digest, schema, input/output roles, payload, and
    /// post-state digest. Any mismatch leaves the live Core unchanged.
    pub fn replay_procedure(
        &mut self,
        procedure: &CanonicalProcedure,
    ) -> Result<PrimitiveOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if procedure.replay_epoch != ReplayEpoch::CURRENT {
            return Err(CoreError::InvalidArgument(
                "procedure replay epoch is unsupported",
            ));
        }
        if current_primitive_schema_version(procedure.primitive_id)
            != Some(procedure.primitive_schema_version)
        {
            return Err(CoreError::InvalidArgument(
                "procedure primitive schema version is unsupported",
            ));
        }
        if procedure.procedure_id != self.next_procedure {
            return Err(CoreError::InvalidState(
                "procedure ID is not the next expected value",
            ));
        }
        if procedure.base_state_id.get() != self.current_state.get() {
            return Err(CoreError::InvalidState("procedure base state is stale"));
        }
        if procedure.committed_state_id.get() != self.next_state.get() {
            return Err(CoreError::InvalidState(
                "procedure committed state ID is not the next expected value",
            ));
        }
        if canonical_payload_digest(&procedure.canonical_payload)?
            != procedure.canonical_payload_digest
        {
            return Err(CoreError::InvalidArgument(
                "procedure inline payload digest does not match its bytes",
            ));
        }
        if self.document_state_digest()? != procedure.pre_state_digest {
            return Err(CoreError::InvalidState(
                "procedure pre-state digest does not match current state",
            ));
        }
        if let Some(runtime) = procedure.runtime_invocation.clone() {
            return self.replay_runtime_invocation(procedure, &runtime);
        }
        if !procedure.output_ids.is_empty() {
            return Err(CoreError::InvalidArgument(
                "primitive schema does not emit output object IDs",
            ));
        }
        let canonical = decode_procedure(procedure, &self.assets)?;
        self.execute_canonical(canonical, Some(procedure))
    }

    /// Computes the BLAKE3-256 digest of canonical semantic document state.
    ///
    /// Session revision, history, paths, views, transient previews, allocation
    /// layout, and renderer caches do not contribute to the digest.
    pub fn document_state_digest(&self) -> Result<DocumentStateDigest, CoreError> {
        self.ensure_canonical_state_cache_current()?;
        self.canonical_state_cache
            .borrow()
            .as_ref()
            .map(|cache| cache.digest())
            .ok_or(CoreError::InvalidState("canonical state cache is missing"))
    }

    pub(super) fn ensure_canonical_state_cache_current(&self) -> Result<(), CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let mut slot = self.canonical_state_cache.borrow_mut();
        if slot
            .as_ref()
            .is_none_or(|cache| cache.revision() != self.document_revision)
        {
            *slot = Some(canonical_document_state_cache(
                document,
                self.document_revision,
            )?);
        }
        Ok(())
    }

    pub(crate) fn execute_canonical_stroke(
        &mut self,
        expected_revision: u64,
        arguments: CanonicalStrokeArguments,
    ) -> Result<PrimitiveOutcome, CoreError> {
        if expected_revision != self.document_revision.get() {
            return Err(CoreError::InvalidState(
                "primitive request revision is stale",
            ));
        }
        let canonical = self.canonicalized_stroke_request(arguments)?;
        self.execute_canonical(canonical, None)
    }

    fn canonicalize_primitive(
        &self,
        request: PrimitiveRequest,
    ) -> Result<CanonicalizedRequest, CoreError> {
        match request {
            PrimitiveRequest::SetMainLineColor { color, .. } => {
                if color.rgba16().is_none() {
                    return Err(CoreError::InvalidArgument(
                        "main-line base color must be RGBA",
                    ));
                }
                let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
                let main_line = document.plane_for_role(ActivePlane::MainLine)?;
                ensure_editable_plane(document, main_line.id)?;
                if !matches!(
                    main_line.raster.format(),
                    PixelFormat::BinaryMask8 | PixelFormat::Grayscale8 | PixelFormat::Grayscale16
                ) {
                    return Err(CoreError::InvalidState(
                        "main-line base color requires a binary or grayscale main plane",
                    ));
                }
                Ok(CanonicalizedRequest {
                    primitive: CanonicalPrimitive::SetMainLineColor(color),
                    primitive_id: PrimitiveId::SET_MAIN_LINE_COLOR,
                    input_ids: Vec::new(),
                    asset_ids: Vec::new(),
                    arguments: color_bytes(color)?,
                    staged_assets: None,
                })
            }
            PrimitiveRequest::ReplacePalette { colors, .. } => {
                let arguments = encode_palette(&colors)?;
                Ok(CanonicalizedRequest {
                    primitive: CanonicalPrimitive::ReplacePalette(colors),
                    primitive_id: PrimitiveId::REPLACE_PALETTE,
                    input_ids: Vec::new(),
                    asset_ids: Vec::new(),
                    arguments,
                    staged_assets: None,
                })
            }
            PrimitiveRequest::ReplaceColorChart {
                entries, locked, ..
            } => {
                let arguments = encode_color_chart(&entries, locked)?;
                Ok(CanonicalizedRequest {
                    primitive: CanonicalPrimitive::ReplaceColorChart { entries, locked },
                    primitive_id: PrimitiveId::REPLACE_COLOR_CHART,
                    input_ids: Vec::new(),
                    asset_ids: Vec::new(),
                    arguments,
                    staged_assets: None,
                })
            }
            PrimitiveRequest::ApplyRasterStroke {
                target_plane_id,
                stroke,
                ..
            } => {
                let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
                let arguments = canonicalize_stroke(
                    &stroke,
                    &self.view,
                    document.width,
                    document.height,
                    target_plane_id,
                )?;
                self.canonicalized_stroke_request(arguments)
            }
            PrimitiveRequest::ImportRasterAsset {
                target_plane_id,
                raster,
                ..
            } => {
                let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
                let target_id = PlaneId::from_raw(target_plane_id);
                let target = document
                    .plane_by_id(target_id)
                    .ok_or(CoreError::InvalidArgument(
                        "import target plane ID does not exist",
                    ))?;
                ensure_editable_plane(document, target_id)?;
                if raster.width != document.width
                    || raster.height != document.height
                    || raster.pixel_format != target.raster.format()
                {
                    return Err(CoreError::InvalidArgument(
                        "import raster must exactly match destination dimensions and format",
                    ));
                }
                let mut staged_assets = self.assets.clone();
                let asset = staged_assets.ingest_raster(raster)?;
                Ok(CanonicalizedRequest {
                    primitive: CanonicalPrimitive::ImportRasterAsset {
                        target_plane_id,
                        asset_id: asset.id(),
                    },
                    primitive_id: PrimitiveId::IMPORT_RASTER_ASSET,
                    input_ids: vec![target_plane_id],
                    asset_ids: vec![asset.id()],
                    arguments: target_plane_id.to_le_bytes().to_vec(),
                    staged_assets: Some(staged_assets),
                })
            }
        }
    }

    fn canonicalized_stroke_request(
        &self,
        arguments: CanonicalStrokeArguments,
    ) -> Result<CanonicalizedRequest, CoreError> {
        let canonical_arguments = encode_stroke_arguments(&arguments)?;
        let target_plane_id = arguments.target_plane_id;
        let (asset_ids, staged_assets) =
            if arguments.payload.len() > MAX_INLINE_PROCEDURE_PAYLOAD_BYTES {
                let samples = crate::primitive::raster::decode_payload(&arguments.payload)?;
                let mut staged_assets = self.assets.clone();
                let asset = staged_assets.ingest_stream(CanonicalStreamInput {
                    kind: AssetKind::CanonicalSampleStream,
                    element_count: samples.len() as u64,
                    payload: arguments.payload.clone(),
                    expected_id: None,
                })?;
                (vec![asset.id()], Some(staged_assets))
            } else {
                (Vec::new(), None)
            };
        Ok(CanonicalizedRequest {
            primitive: CanonicalPrimitive::ApplyRasterStroke(arguments),
            primitive_id: PrimitiveId::APPLY_RASTER_STROKE,
            input_ids: vec![target_plane_id],
            asset_ids,
            arguments: canonical_arguments,
            staged_assets,
        })
    }

    fn execute_canonical(
        &mut self,
        canonical: CanonicalizedRequest,
        replay: Option<&CanonicalProcedure>,
    ) -> Result<PrimitiveOutcome, CoreError> {
        self.ensure_canonical_state_cache_current()?;
        let pre_state_digest = self
            .canonical_state_cache
            .borrow()
            .as_ref()
            .map(|cache| cache.digest())
            .ok_or(CoreError::InvalidState("canonical state cache is missing"))?;
        let mut transaction = PrimitiveTransaction::begin(self)?;
        let staging_revision = self
            .document_revision
            .checked_next()
            .unwrap_or(self.document_revision);
        let execution_assets = canonical.staged_assets.as_ref().unwrap_or(&self.assets);
        let applied = apply_primitive(
            &mut transaction.working,
            &canonical.primitive,
            execution_assets,
            staging_revision.get(),
        )?;
        let Some(applied) = applied else {
            if replay.is_some() {
                return Err(CoreError::InvalidState(
                    "committed procedure replays as a semantic no-op",
                ));
            }
            return Ok(PrimitiveOutcome::no_op(self.noop_outcome()));
        };

        let revision = self.next_document_revision()?;
        let next_state = self.next_state;
        let following_state = self
            .next_state
            .checked_next()
            .ok_or(CoreError::InvalidState("history state overflow"))?;
        let procedure_id = self.next_procedure;
        let following_procedure = self
            .next_procedure
            .checked_next()
            .ok_or(CoreError::InvalidState("procedure ID overflow"))?;
        let post_state_cache = {
            let cache = self.canonical_state_cache.borrow();
            let previous = cache
                .as_ref()
                .ok_or(CoreError::InvalidState("canonical state cache is missing"))?;
            advance_canonical_document_state_cache(
                &transaction.working,
                self.document_revision,
                revision,
                previous,
                &applied.history,
            )?
        };
        let post_state_digest = post_state_cache.digest();

        if let Some(expected) = replay {
            if expected.primitive_id != canonical.primitive_id
                || expected.input_ids != canonical.input_ids
                || expected.output_ids != transaction.output_ids
                || expected.asset_ids != canonical.asset_ids
                || expected.canonical_arguments != canonical.arguments
                || expected.canonical_payload != canonical.procedure_payload()
            {
                return Err(CoreError::InvalidArgument(
                    "procedure canonical fields do not match its primitive schema",
                ));
            }
            if expected.post_state_digest != post_state_digest {
                return Err(CoreError::InvalidState(
                    "procedure post-state digest does not match replay result",
                ));
            }
        }

        let payload = canonical.procedure_payload().to_vec();
        let payload_digest = canonical_payload_digest(&payload)?;
        let CanonicalizedRequest {
            primitive: _,
            primitive_id,
            input_ids,
            asset_ids,
            arguments,
            staged_assets,
        } = canonical;
        let primitive_schema_version = current_primitive_schema_version(primitive_id).ok_or(
            CoreError::InvalidArgument("primitive ID is not in the catalog"),
        )?;
        let procedure = Arc::new(CanonicalProcedure {
            procedure_id,
            primitive_id,
            primitive_schema_version,
            replay_epoch: ReplayEpoch::CURRENT,
            base_state_id: self.current_state,
            committed_state_id: next_state,
            input_ids,
            output_ids: transaction.output_ids,
            asset_ids,
            canonical_arguments: arguments,
            canonical_payload: payload,
            canonical_payload_digest: payload_digest,
            pre_state_digest,
            post_state_digest,
            runtime_invocation: None,
        });
        let journal_plan = self.prepare_canonical_commit(Arc::clone(&procedure))?;

        // Reserve every fallible runtime allocation before the single publish
        // boundary so document/history/journal/counters advance together.
        self.history
            .try_reserve(1)
            .map_err(|_| CoreError::InvalidState("history allocation failed"))?;
        let staged_assets =
            self.prepare_asset_store_for_commit(staged_assets, &transaction.working, &procedure)?;
        let editor = self.stage_reconciled_editor_target(&transaction.working, None, None)?;

        self.history.truncate(self.history_cursor);
        let history_entry = HistoryEntry {
            kind: applied.history.kind(),
            change: Some(applied.history),
            before_state: self.current_state,
            after_state: next_state,
            procedure: Arc::clone(&procedure),
            branch_id: journal_plan.branch_id(),
        };
        self.document = Some(transaction.working);
        self.document_revision = revision;
        self.next_id = transaction.next_stable_id;
        if let Some(assets) = staged_assets {
            self.assets = assets;
        }
        match applied.cache_policy {
            CachePolicy::Preserve | CachePolicy::RasterRevision => {}
            CachePolicy::InvalidateAll => self.render_cache.clear(),
        }
        self.history.push(history_entry);
        self.history_cursor = self.history.len();
        self.current_state = next_state;
        self.publish_canonical_commit(journal_plan);
        self.next_state = following_state;
        self.next_procedure = following_procedure;
        *self.canonical_state_cache.get_mut() = Some(post_state_cache);
        self.publish_editor_session(editor);
        let dispatch = DispatchOutcome {
            revision: revision.get(),
            accepted_commands: 1,
        };
        Ok(PrimitiveOutcome::committed(dispatch, procedure))
    }
}

fn apply_primitive(
    working: &mut CellDocument,
    primitive: &CanonicalPrimitive,
    assets: &asset::AssetStore,
    revision: u64,
) -> Result<Option<AppliedPrimitive>, CoreError> {
    match primitive {
        CanonicalPrimitive::SetMainLineColor(color) => {
            if working.main_line_color == *color {
                return Ok(None);
            }
            let old = working.main_line_color;
            working.main_line_color = *color;
            Ok(Some(AppliedPrimitive {
                history: HistoryChange::MainLineColor {
                    before: old,
                    after: *color,
                },
                cache_policy: CachePolicy::InvalidateAll,
            }))
        }
        CanonicalPrimitive::ReplacePalette(colors) => {
            let mut palette = Palette::default();
            for color in colors {
                palette.push(*color)?;
            }
            if working.palette == palette {
                return Ok(None);
            }
            let old = working.palette.clone();
            working.palette = palette.clone();
            Ok(Some(AppliedPrimitive {
                history: HistoryChange::Palette {
                    before: old,
                    after: palette,
                },
                cache_policy: CachePolicy::Preserve,
            }))
        }
        CanonicalPrimitive::ReplaceColorChart { entries, locked } => {
            let chart = ColorChart::validated(entries.clone(), *locked)?;
            if working.color_chart == chart {
                return Ok(None);
            }
            let old = working.color_chart.clone();
            working.color_chart = chart.clone();
            Ok(Some(AppliedPrimitive {
                history: HistoryChange::ColorChart {
                    before: old,
                    after: chart,
                },
                cache_policy: CachePolicy::Preserve,
            }))
        }
        CanonicalPrimitive::ApplyRasterStroke(arguments) => {
            let changes = apply_raster_stroke(working, arguments, revision)?;
            if changes.is_empty() {
                return Ok(None);
            }
            let plane_id = PlaneId::from_raw(arguments.target_plane_id);
            Ok(Some(AppliedPrimitive {
                history: HistoryChange::Pixels { plane_id, changes },
                cache_policy: CachePolicy::RasterRevision,
            }))
        }
        CanonicalPrimitive::ImportRasterAsset {
            target_plane_id,
            asset_id,
        } => {
            let target_id = PlaneId::from_raw(*target_plane_id);
            ensure_editable_plane(working, target_id)?;
            let target = working
                .plane_by_id(target_id)
                .ok_or(CoreError::InvalidState("import target plane disappeared"))?;
            let asset = assets.get(*asset_id).ok_or(CoreError::InvalidState(
                "import procedure raster asset is not registered",
            ))?;
            let raster = asset.raster().ok_or(CoreError::InvalidArgument(
                "import procedure asset is not a raster",
            ))?;
            if raster.width() != working.width
                || raster.height() != working.height
                || raster.format() != target.raster.format()
            {
                return Err(CoreError::InvalidArgument(
                    "import procedure asset does not match its destination",
                ));
            }
            if target.raster == **raster {
                return Ok(None);
            }
            let before = working.clone();
            working
                .plane_by_id_mut(target_id)
                .ok_or(CoreError::InvalidState("import target plane disappeared"))?
                .raster = (**raster).clone();
            Ok(Some(AppliedPrimitive {
                history: HistoryChange::Document {
                    before: Box::new(before),
                    after: Box::new(working.clone()),
                },
                cache_policy: CachePolicy::InvalidateAll,
            }))
        }
    }
}

fn encode_palette(colors: &[PixelValue]) -> Result<Vec<u8>, CoreError> {
    let mut palette = Palette::default();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(
        &u64::try_from(colors.len())
            .map_err(|_| CoreError::InvalidArgument("palette entry count overflows"))?
            .to_le_bytes(),
    );
    for color in colors {
        palette.push(*color)?;
        let color = color_bytes(*color)?;
        bytes.extend_from_slice(&(color.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&color);
    }
    Ok(bytes)
}

pub(super) fn decode_palette(bytes: &[u8]) -> Result<Vec<PixelValue>, CoreError> {
    if bytes.len() < 8 {
        return Err(CoreError::InvalidArgument(
            "canonical palette is missing its count",
        ));
    }
    let count = u64::from_le_bytes(bytes[0..8].try_into().expect("fixed-width slice"));
    let count = usize::try_from(count)
        .map_err(|_| CoreError::InvalidArgument("canonical palette count overflows"))?;
    let mut cursor = 8_usize;
    let mut colors = Vec::with_capacity(count.min(4_096));
    for _ in 0..count {
        let end = cursor.checked_add(8).ok_or(CoreError::InvalidArgument(
            "canonical palette length overflows",
        ))?;
        let length_bytes = bytes.get(cursor..end).ok_or(CoreError::InvalidArgument(
            "canonical palette element length is truncated",
        ))?;
        let length = usize::try_from(u64::from_le_bytes(
            length_bytes.try_into().expect("fixed-width slice"),
        ))
        .map_err(|_| CoreError::InvalidArgument("canonical palette element is too large"))?;
        cursor = end;
        let end = cursor
            .checked_add(length)
            .ok_or(CoreError::InvalidArgument(
                "canonical palette element overflows",
            ))?;
        colors.push(decode_color(bytes.get(cursor..end).ok_or(
            CoreError::InvalidArgument("canonical palette element is truncated"),
        )?)?);
        cursor = end;
    }
    if cursor != bytes.len() {
        return Err(CoreError::InvalidArgument(
            "canonical palette has trailing bytes",
        ));
    }
    encode_palette(&colors)?;
    Ok(colors)
}

fn encode_color_chart(entries: &[ColorChartEntry], locked: bool) -> Result<Vec<u8>, CoreError> {
    crate::color_chart::validate_entries(entries)?;
    let mut bytes = Vec::new();
    bytes.push(u8::from(locked));
    bytes.extend_from_slice(
        &u32::try_from(entries.len())
            .map_err(|_| CoreError::InvalidArgument("Color chart count overflows"))?
            .to_le_bytes(),
    );
    for entry in entries {
        let color = color_bytes(entry.color)?;
        bytes.extend_from_slice(&(color.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&color);
        bytes.extend_from_slice(
            &u32::try_from(entry.name.len())
                .map_err(|_| CoreError::InvalidArgument("Color chart name overflows"))?
                .to_le_bytes(),
        );
        bytes.extend_from_slice(entry.name.as_bytes());
    }
    Ok(bytes)
}

pub(super) fn decode_color_chart(bytes: &[u8]) -> Result<(Vec<ColorChartEntry>, bool), CoreError> {
    let (&locked, rest) = bytes.split_first().ok_or(CoreError::InvalidArgument(
        "canonical Color chart is truncated",
    ))?;
    let locked = match locked {
        0 => false,
        1 => true,
        _ => {
            return Err(CoreError::InvalidArgument(
                "canonical Color chart lock is invalid",
            ));
        }
    };
    let count_bytes = rest.get(..4).ok_or(CoreError::InvalidArgument(
        "canonical Color chart count is truncated",
    ))?;
    let count = u32::from_le_bytes(count_bytes.try_into().expect("fixed-width slice")) as usize;
    if count > MAX_APPLICATION_COLORS {
        return Err(CoreError::InvalidArgument(
            "canonical Color chart count exceeds the supported maximum",
        ));
    }
    let mut cursor = 4_usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let color_length = read_chart_u32(rest, &mut cursor, "Color chart color length")? as usize;
        let color_end = cursor
            .checked_add(color_length)
            .ok_or(CoreError::InvalidArgument(
                "canonical Color chart color length overflows",
            ))?;
        let color = decode_color(rest.get(cursor..color_end).ok_or(
            CoreError::InvalidArgument("canonical Color chart color is truncated"),
        )?)?;
        cursor = color_end;
        let name_length = read_chart_u32(rest, &mut cursor, "Color chart name length")? as usize;
        let name_end = cursor
            .checked_add(name_length)
            .ok_or(CoreError::InvalidArgument(
                "canonical Color chart name length overflows",
            ))?;
        let name = std::str::from_utf8(rest.get(cursor..name_end).ok_or(
            CoreError::InvalidArgument("canonical Color chart name is truncated"),
        )?)
        .map_err(|_| CoreError::InvalidArgument("canonical Color chart name is not UTF-8"))?
        .to_owned();
        cursor = name_end;
        entries.push(ColorChartEntry { color, name });
    }
    if cursor != rest.len() {
        return Err(CoreError::InvalidArgument(
            "canonical Color chart has trailing bytes",
        ));
    }
    encode_color_chart(&entries, locked)?;
    Ok((entries, locked))
}

fn read_chart_u32(
    bytes: &[u8],
    cursor: &mut usize,
    message: &'static str,
) -> Result<u32, CoreError> {
    let end = cursor
        .checked_add(4)
        .ok_or(CoreError::InvalidArgument(message))?;
    let value = u32::from_le_bytes(
        bytes
            .get(*cursor..end)
            .ok_or(CoreError::InvalidArgument(message))?
            .try_into()
            .expect("fixed-width slice"),
    );
    *cursor = end;
    Ok(value)
}

fn encode_stroke_arguments(arguments: &CanonicalStrokeArguments) -> Result<Vec<u8>, CoreError> {
    if arguments.target_plane_id == 0 {
        return Err(CoreError::InvalidArgument(
            "stroke target plane ID must be nonzero",
        ));
    }
    let color = color_bytes(arguments.color)?;
    let mut bytes = Vec::with_capacity(8 + 4 + color.len() + 8 + 4 + 2 + 4 + 2);
    bytes.extend_from_slice(&arguments.target_plane_id.to_le_bytes());
    bytes.extend_from_slice(&arguments.tool_code.to_le_bytes());
    bytes.extend_from_slice(&color);
    bytes.extend_from_slice(&arguments.diameter_q16.to_le_bytes());
    bytes.extend_from_slice(&arguments.shape_code.to_le_bytes());
    bytes.extend_from_slice(&arguments.smoothing.to_le_bytes());
    bytes.extend_from_slice(&arguments.start_color_code.to_le_bytes());
    bytes.push(u8::from(arguments.auto_erase));
    bytes.push(u8::from(arguments.pressure_size));
    Ok(bytes)
}

fn decode_procedure(
    procedure: &CanonicalProcedure,
    assets: &asset::AssetStore,
) -> Result<CanonicalizedRequest, CoreError> {
    if !procedure.output_ids.is_empty() {
        return Err(CoreError::InvalidArgument(
            "primitive schema does not permit output IDs",
        ));
    }
    match procedure.primitive_id {
        PrimitiveId::SET_MAIN_LINE_COLOR => {
            if !procedure.input_ids.is_empty()
                || !procedure.asset_ids.is_empty()
                || !procedure.canonical_payload.is_empty()
            {
                return Err(CoreError::InvalidArgument(
                    "main-line color procedure has unexpected roles or payload",
                ));
            }
            let color = decode_color(&procedure.canonical_arguments)?;
            Ok(CanonicalizedRequest {
                primitive: CanonicalPrimitive::SetMainLineColor(color),
                primitive_id: procedure.primitive_id,
                input_ids: Vec::new(),
                asset_ids: Vec::new(),
                arguments: procedure.canonical_arguments.clone(),
                staged_assets: None,
            })
        }
        PrimitiveId::REPLACE_PALETTE => {
            if !procedure.input_ids.is_empty()
                || !procedure.asset_ids.is_empty()
                || !procedure.canonical_payload.is_empty()
            {
                return Err(CoreError::InvalidArgument(
                    "palette procedure has unexpected roles or payload",
                ));
            }
            let colors = decode_palette(&procedure.canonical_arguments)?;
            Ok(CanonicalizedRequest {
                primitive: CanonicalPrimitive::ReplacePalette(colors),
                primitive_id: procedure.primitive_id,
                input_ids: Vec::new(),
                asset_ids: Vec::new(),
                arguments: procedure.canonical_arguments.clone(),
                staged_assets: None,
            })
        }
        PrimitiveId::REPLACE_COLOR_CHART => {
            if !procedure.input_ids.is_empty()
                || !procedure.asset_ids.is_empty()
                || !procedure.canonical_payload.is_empty()
            {
                return Err(CoreError::InvalidArgument(
                    "Color chart procedure has unexpected roles or payload",
                ));
            }
            let (entries, locked) = decode_color_chart(&procedure.canonical_arguments)?;
            Ok(CanonicalizedRequest {
                primitive: CanonicalPrimitive::ReplaceColorChart { entries, locked },
                primitive_id: procedure.primitive_id,
                input_ids: Vec::new(),
                asset_ids: Vec::new(),
                arguments: procedure.canonical_arguments.clone(),
                staged_assets: None,
            })
        }
        PrimitiveId::APPLY_RASTER_STROKE => decode_stroke_procedure(procedure, assets),
        PrimitiveId::IMPORT_RASTER_ASSET => decode_import_raster_procedure(procedure, assets),
        _ => Err(CoreError::InvalidArgument(
            "primitive ID is not in the catalog",
        )),
    }
}

pub(crate) fn validate_persisted_procedure(
    procedure: &CanonicalProcedure,
    assets: &asset::AssetStore,
) -> Result<(), CoreError> {
    if procedure.replay_epoch() != ReplayEpoch::CURRENT
        || current_primitive_schema_version(procedure.primitive_id())
            != Some(procedure.primitive_schema_version())
        || canonical_payload_digest(procedure.canonical_payload())?
            != *procedure.canonical_payload_digest()
    {
        return Err(CoreError::Format(
            "persisted procedure replay contract is invalid".to_owned(),
        ));
    }
    if procedure.runtime_invocation.is_none() {
        let _ = decode_procedure(procedure, assets)?;
    }
    Ok(())
}

fn decode_stroke_procedure(
    procedure: &CanonicalProcedure,
    assets: &asset::AssetStore,
) -> Result<CanonicalizedRequest, CoreError> {
    if procedure.input_ids.len() != 1 || !matches!(procedure.canonical_arguments.len(), 37 | 41) {
        return Err(CoreError::InvalidArgument(
            "raster stroke procedure has invalid canonical roles or arguments",
        ));
    }
    let bytes = &procedure.canonical_arguments;
    let target_plane_id = u64::from_le_bytes(bytes[0..8].try_into().expect("fixed-width slice"));
    if target_plane_id == 0 || target_plane_id != procedure.input_ids[0] {
        return Err(CoreError::InvalidArgument(
            "raster stroke target argument does not match its input role",
        ));
    }
    let tool_code = u32::from_le_bytes(bytes[8..12].try_into().expect("fixed-width slice"));
    let color_length = match bytes[12] {
        1 => 5,
        2 => 9,
        _ => {
            return Err(CoreError::InvalidArgument(
                "raster stroke color tag is unknown",
            ));
        }
    };
    let diameter_start = 12 + color_length;
    let shape_start = diameter_start + 8;
    let smoothing_start = shape_start + 4;
    let start_color_start = smoothing_start + 2;
    let flags_start = start_color_start + 4;
    if bytes.len() != flags_start + 2
        || !matches!(bytes[flags_start], 0 | 1)
        || !matches!(bytes[flags_start + 1], 0 | 1)
    {
        return Err(CoreError::InvalidArgument(
            "raster stroke arguments are not canonical",
        ));
    }
    let color = decode_color(&bytes[12..diameter_start])?;
    let diameter_q16 = i64::from_le_bytes(
        bytes[diameter_start..shape_start]
            .try_into()
            .expect("fixed-width slice"),
    );
    let shape_code = u32::from_le_bytes(
        bytes[shape_start..smoothing_start]
            .try_into()
            .expect("fixed-width slice"),
    );
    let smoothing = u16::from_le_bytes(
        bytes[smoothing_start..start_color_start]
            .try_into()
            .expect("fixed-width slice"),
    );
    let start_color_code = u32::from_le_bytes(
        bytes[start_color_start..flags_start]
            .try_into()
            .expect("fixed-width slice"),
    );
    let (payload, asset_element_count) = match (
        procedure.asset_ids.as_slice(),
        procedure.canonical_payload.is_empty(),
    ) {
        ([], false) => (procedure.canonical_payload.clone(), None),
        ([asset_id], true) => {
            let asset = assets.get(*asset_id).ok_or(CoreError::InvalidState(
                "stroke sample asset is not registered",
            ))?;
            if asset.descriptor().kind != AssetKind::CanonicalSampleStream {
                return Err(CoreError::InvalidArgument(
                    "stroke procedure asset is not a canonical sample stream",
                ));
            }
            (
                asset.payload().to_vec(),
                Some(asset.descriptor().logical_element_count),
            )
        }
        _ => {
            return Err(CoreError::InvalidArgument(
                "stroke procedure must use exactly one inline or asset payload",
            ));
        }
    };
    let arguments = CanonicalStrokeArguments {
        target_plane_id,
        tool_code,
        color,
        diameter_q16,
        shape_code,
        smoothing,
        start_color_code,
        auto_erase: bytes[flags_start] != 0,
        pressure_size: bytes[flags_start + 1] != 0,
        payload,
    };
    let samples = crate::primitive::raster::decode_payload(&arguments.payload)?;
    if asset_element_count.is_some_and(|count| count != samples.len() as u64) {
        return Err(CoreError::InvalidArgument(
            "stroke sample asset element count does not match its payload",
        ));
    }
    let encoded = encode_stroke_arguments(&arguments)?;
    Ok(CanonicalizedRequest {
        primitive: CanonicalPrimitive::ApplyRasterStroke(arguments),
        primitive_id: procedure.primitive_id,
        input_ids: procedure.input_ids.clone(),
        asset_ids: procedure.asset_ids.clone(),
        arguments: encoded,
        staged_assets: None,
    })
}

fn decode_import_raster_procedure(
    procedure: &CanonicalProcedure,
    assets: &asset::AssetStore,
) -> Result<CanonicalizedRequest, CoreError> {
    if procedure.input_ids.len() != 1
        || procedure.asset_ids.len() != 1
        || procedure.canonical_arguments.len() != 8
        || !procedure.canonical_payload.is_empty()
    {
        return Err(CoreError::InvalidArgument(
            "raster import procedure has invalid canonical roles or payload",
        ));
    }
    let target_plane_id = u64::from_le_bytes(
        procedure.canonical_arguments[..8]
            .try_into()
            .expect("fixed-width slice"),
    );
    if target_plane_id == 0 || procedure.input_ids[0] != target_plane_id {
        return Err(CoreError::InvalidArgument(
            "raster import target argument does not match its input role",
        ));
    }
    let asset_id = procedure.asset_ids[0];
    let asset = assets.get(asset_id).ok_or(CoreError::InvalidState(
        "raster import asset is not registered",
    ))?;
    if asset.descriptor().kind != AssetKind::CanonicalRaster || asset.raster().is_none() {
        return Err(CoreError::InvalidArgument(
            "raster import procedure references a non-raster asset",
        ));
    }
    Ok(CanonicalizedRequest {
        primitive: CanonicalPrimitive::ImportRasterAsset {
            target_plane_id,
            asset_id,
        },
        primitive_id: procedure.primitive_id,
        input_ids: procedure.input_ids.clone(),
        asset_ids: procedure.asset_ids.clone(),
        arguments: procedure.canonical_arguments.clone(),
        staged_assets: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initialized_core(uuid: u128) -> Core {
        let mut core = Core::new();
        core.new_cell_with_uuid(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, uuid)
            .unwrap();
        core
    }

    fn edit_pixel(core: &mut Core, x: f32, y: f32, tool: PaintTool, color: [u8; 4]) {
        let document = core.document_info().unwrap();
        core.execute_primitive(PrimitiveRequest::ApplyRasterStroke {
            expected_revision: document.document_revision,
            target_plane_id: document.color_plane_id,
            stroke: Stroke {
                tool,
                plane: ActivePlane::Color,
                color,
                diameter: 1.0,
                shape: BrushShape::Round,
                smoothing: 0,
                start_color: StartColorPredicate::Any,
                auto_erase: false,
                pressure_size: false,
                coordinate_space: CoordinateSpace::Document,
                samples: vec![StrokeSample {
                    x,
                    y,
                    pressure: 1.0,
                }],
            },
        })
        .unwrap();
    }

    fn paint_pixel(core: &mut Core, x: f32, y: f32, color: [u8; 4]) {
        edit_pixel(core, x, y, PaintTool::Pencil, color);
    }

    fn assert_hot_digest_matches_cold(core: &Core) {
        let hot = core.document_state_digest().unwrap();
        let cold = crate::primitive::canonical_document_state(core.document.as_ref().unwrap())
            .unwrap()
            .1;
        assert_eq!(hot, cold);
    }

    #[test]
    fn hot_raster_digest_reads_only_the_changed_tile_and_revision_mismatch_rebuilds() {
        let mut core = Core::new();
        core.new_cell_with_uuid(4_096, 4_096, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x17)
            .unwrap();
        paint_pixel(&mut core, 1.0, 1.0, [10, 20, 30, 255]);
        assert_hot_digest_matches_cold(&core);
        paint_pixel(&mut core, 65.0, 1.0, [40, 50, 60, 255]);
        assert_hot_digest_matches_cold(&core);

        paint_pixel(&mut core, 1.0, 1.0, [70, 80, 90, 255]);
        assert_hot_digest_matches_cold(&core);
        let cache = core.canonical_state_cache.borrow();
        assert_eq!(cache.as_ref().unwrap().tile_payload_reads(), 1);
        drop(cache);

        edit_pixel(&mut core, 65.0, 1.0, PaintTool::Eraser, [0; 4]);
        assert_hot_digest_matches_cold(&core);
        let cache = core.canonical_state_cache.borrow();
        assert_eq!(cache.as_ref().unwrap().tile_payload_reads(), 1);
        drop(cache);

        paint_pixel(&mut core, 65.0, 1.0, [40, 50, 60, 255]);
        assert_hot_digest_matches_cold(&core);
        let cache = core.canonical_state_cache.borrow();
        assert_eq!(cache.as_ref().unwrap().tile_payload_reads(), 1);
        drop(cache);

        core.document_revision = core.document_revision.checked_next().unwrap();
        core.document_state_digest().unwrap();
        let cache = core.canonical_state_cache.borrow();
        assert_eq!(cache.as_ref().unwrap().tile_payload_reads(), 2);
    }

    #[test]
    fn palette_encoding_retains_exact_depth_and_order() {
        let colors = vec![
            PixelValue::Rgba([1, 2, 3, 4]),
            PixelValue::Rgba16([5, 6, 7, 8]),
        ];
        assert_eq!(
            decode_palette(&encode_palette(&colors).unwrap()).unwrap(),
            colors
        );
    }

    #[test]
    fn persistent_counter_overflow_does_not_publish_working_state() {
        let mut state_boundary = initialized_core(0x10);
        state_boundary.current_state = StateId::from_raw(MAX_PERSISTENT_NUMERIC_ID - 2);
        state_boundary.next_state = StateId::from_raw(MAX_PERSISTENT_NUMERIC_ID - 1);
        let outcome = state_boundary
            .execute_primitive(PrimitiveRequest::SetMainLineColor {
                expected_revision: state_boundary.document_revision.get(),
                color: PixelValue::Rgba([1, 1, 1, 255]),
            })
            .unwrap();
        assert_eq!(
            outcome.procedure().unwrap().committed_state_id().get(),
            MAX_PERSISTENT_NUMERIC_ID - 1
        );
        assert_eq!(
            state_boundary.current_state.get(),
            MAX_PERSISTENT_NUMERIC_ID - 1
        );
        assert_eq!(state_boundary.next_state.get(), MAX_PERSISTENT_NUMERIC_ID);

        let mut procedure_boundary = initialized_core(0x14);
        procedure_boundary.next_procedure =
            ProcedureId::from_raw(crate::journal::MAX_JOURNAL_COMMITS);
        let outcome = procedure_boundary
            .execute_primitive(PrimitiveRequest::SetMainLineColor {
                expected_revision: procedure_boundary.document_revision.get(),
                color: PixelValue::Rgba([2, 2, 2, 255]),
            })
            .unwrap();
        assert_eq!(
            outcome.procedure().unwrap().procedure_id().get(),
            crate::journal::MAX_JOURNAL_COMMITS
        );
        assert_eq!(
            procedure_boundary.next_procedure.get(),
            crate::journal::MAX_JOURNAL_COMMITS + 1
        );

        let mut state_overflow = initialized_core(0x11);
        state_overflow.current_state = StateId::from_raw(MAX_PERSISTENT_NUMERIC_ID - 1);
        state_overflow.next_state = StateId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
        let before_document = state_overflow.document.clone();
        let before_revision = state_overflow.document_revision;
        let before_procedure = state_overflow.next_procedure;
        assert!(matches!(
            state_overflow.execute_primitive(PrimitiveRequest::SetMainLineColor {
                expected_revision: before_revision.get(),
                color: PixelValue::Rgba([1, 2, 3, 255]),
            }),
            Err(CoreError::InvalidState("history state overflow"))
        ));
        assert_eq!(state_overflow.document, before_document);
        assert_eq!(state_overflow.document_revision, before_revision);
        assert_eq!(state_overflow.next_procedure, before_procedure);
        assert!(state_overflow.history.is_empty());

        let mut procedure_overflow = initialized_core(0x12);
        procedure_overflow.next_procedure = ProcedureId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
        let before_document = procedure_overflow.document.clone();
        let before_revision = procedure_overflow.document_revision;
        let before_state = procedure_overflow.next_state;
        assert!(matches!(
            procedure_overflow.execute_primitive(PrimitiveRequest::SetMainLineColor {
                expected_revision: before_revision.get(),
                color: PixelValue::Rgba([4, 5, 6, 255]),
            }),
            Err(CoreError::InvalidState("procedure ID overflow"))
        ));
        assert_eq!(procedure_overflow.document, before_document);
        assert_eq!(procedure_overflow.document_revision, before_revision);
        assert_eq!(procedure_overflow.next_state, before_state);
        assert!(procedure_overflow.history.is_empty());
    }

    #[test]
    fn replay_rejects_exhausted_persistent_counters_atomically() {
        let mut source = initialized_core(0x16);
        let procedure = source
            .execute_primitive(PrimitiveRequest::SetMainLineColor {
                expected_revision: source.document_revision.get(),
                color: PixelValue::Rgba([12, 34, 56, 255]),
            })
            .unwrap()
            .procedure()
            .unwrap()
            .clone();

        let mut state_exhausted = initialized_core(0x16);
        state_exhausted.current_state = StateId::from_raw(MAX_PERSISTENT_NUMERIC_ID - 1);
        state_exhausted.next_state = StateId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
        let mut state_procedure = procedure.clone();
        state_procedure.base_state_id = StateId::from_raw(MAX_PERSISTENT_NUMERIC_ID - 1);
        state_procedure.committed_state_id = StateId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
        let state_before = state_exhausted.document.clone();
        let state_revision = state_exhausted.document_revision;
        let state_next_procedure = state_exhausted.next_procedure;
        assert!(matches!(
            state_exhausted.replay_procedure(&state_procedure),
            Err(CoreError::InvalidState("history state overflow"))
        ));
        assert_eq!(state_exhausted.document, state_before);
        assert_eq!(state_exhausted.document_revision, state_revision);
        assert_eq!(state_exhausted.next_state.get(), MAX_PERSISTENT_NUMERIC_ID);
        assert_eq!(state_exhausted.next_procedure, state_next_procedure);
        assert!(state_exhausted.history.is_empty());

        let mut procedure_exhausted = initialized_core(0x16);
        procedure_exhausted.next_procedure = ProcedureId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
        let mut procedure_at_limit = procedure;
        procedure_at_limit.procedure_id = ProcedureId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
        let procedure_before = procedure_exhausted.document.clone();
        let procedure_revision = procedure_exhausted.document_revision;
        let procedure_next_state = procedure_exhausted.next_state;
        assert!(matches!(
            procedure_exhausted.replay_procedure(&procedure_at_limit),
            Err(CoreError::InvalidState("procedure ID overflow"))
        ));
        assert_eq!(procedure_exhausted.document, procedure_before);
        assert_eq!(procedure_exhausted.document_revision, procedure_revision);
        assert_eq!(procedure_exhausted.next_state, procedure_next_state);
        assert_eq!(
            procedure_exhausted.next_procedure.get(),
            MAX_PERSISTENT_NUMERIC_ID
        );
        assert!(procedure_exhausted.history.is_empty());
    }

    #[test]
    fn document_revision_overflow_allows_no_ops_but_rejects_changes_atomically() {
        let mut core = initialized_core(0x15);
        let _ = core.build_snapshot();
        core.document_revision = DocumentRevision::from_raw(u64::MAX);
        let before_document = core.document.clone();
        let before_digest = core.document_state_digest().unwrap();
        let before_cache = core.render_cache.clone();
        let before_render_revision = core.next_render_tile_revision;
        let before_state = core.current_state;
        let before_next_state = core.next_state;
        let before_procedure = core.next_procedure;
        let before_id = core.next_id;
        let before_savepoint = core.savepoint;

        let main_line = core
            .execute_primitive(PrimitiveRequest::SetMainLineColor {
                expected_revision: u64::MAX,
                color: PixelValue::Rgba([0, 0, 0, 255]),
            })
            .unwrap();
        assert!(main_line.procedure().is_none());
        assert_eq!(main_line.dispatch().revision(), u64::MAX);

        let palette = core
            .execute_primitive(PrimitiveRequest::ReplacePalette {
                expected_revision: u64::MAX,
                colors: Vec::new(),
            })
            .unwrap();
        assert!(palette.procedure().is_none());
        assert_eq!(palette.dispatch().revision(), u64::MAX);

        let target_plane_id = core.document.as_ref().unwrap().primary_ids().2.get();
        let stroke = core
            .execute_primitive(PrimitiveRequest::ApplyRasterStroke {
                expected_revision: u64::MAX,
                target_plane_id,
                stroke: Stroke {
                    tool: PaintTool::Eraser,
                    plane: ActivePlane::Color,
                    color: [1, 2, 3, 255],
                    diameter: 1.0,
                    shape: BrushShape::Round,
                    smoothing: 0,
                    start_color: StartColorPredicate::Any,
                    auto_erase: false,
                    pressure_size: false,
                    coordinate_space: CoordinateSpace::Document,
                    samples: vec![StrokeSample {
                        x: 1.0,
                        y: 1.0,
                        pressure: 1.0,
                    }],
                },
            })
            .unwrap();
        assert!(stroke.procedure().is_none());
        assert_eq!(stroke.dispatch().revision(), u64::MAX);

        assert!(matches!(
            core.execute_primitive(PrimitiveRequest::SetMainLineColor {
                expected_revision: u64::MAX,
                color: PixelValue::Rgba([1, 2, 3, 255]),
            }),
            Err(CoreError::InvalidState("document revision overflow"))
        ));
        assert_eq!(core.document, before_document);
        assert_eq!(core.document_state_digest().unwrap(), before_digest);
        assert_eq!(core.render_cache, before_cache);
        assert_eq!(core.next_render_tile_revision, before_render_revision);
        assert_eq!(core.document_revision.get(), u64::MAX);
        assert_eq!(core.current_state, before_state);
        assert_eq!(core.next_state, before_next_state);
        assert_eq!(core.next_procedure, before_procedure);
        assert_eq!(core.next_id, before_id);
        assert_eq!(core.savepoint, before_savepoint);
        assert!(core.history.is_empty());
        assert_eq!(core.history_cursor, 0);
    }

    #[test]
    fn replay_rejects_forged_output_ids_and_payload_digest_atomically() {
        let mut source = initialized_core(0x13);
        let revision = source.document_revision.get();
        let procedure = source
            .execute_primitive(PrimitiveRequest::SetMainLineColor {
                expected_revision: revision,
                color: PixelValue::Rgba([7, 8, 9, 255]),
            })
            .unwrap()
            .procedure()
            .unwrap()
            .clone();

        let mut target = initialized_core(0x13);
        let before = target.document.clone();
        let mut forged_output = procedure.clone();
        forged_output.output_ids.push(99);
        assert!(matches!(
            target.replay_procedure(&forged_output),
            Err(CoreError::InvalidArgument(_))
        ));
        assert_eq!(target.document, before);
        assert!(target.history.is_empty());

        let mut forged_payload_digest = procedure;
        forged_payload_digest.canonical_payload_digest[0] = 1;
        assert!(matches!(
            target.replay_procedure(&forged_payload_digest),
            Err(CoreError::InvalidArgument(_))
        ));
        assert_eq!(target.document, before);
        assert!(target.history.is_empty());
    }

    #[test]
    fn stroke_arguments_encode_target_and_exact_color_depth() {
        let mut exact_source = initialized_core(0x18);
        let exact_document = exact_source.document.as_ref().unwrap();
        let exact_target = exact_document.primary_ids().2.get();
        let exact_color = PixelValue::Rgba16([0x0123, 0x4567, 0x89ab, 0xcdef]);
        let stroke = Stroke {
            tool: PaintTool::Brush,
            plane: ActivePlane::Color,
            color: [9, 8, 7, 6],
            diameter: 1.0,
            shape: BrushShape::Round,
            smoothing: 0,
            start_color: StartColorPredicate::Any,
            auto_erase: true,
            pressure_size: false,
            coordinate_space: CoordinateSpace::Document,
            samples: vec![StrokeSample {
                x: 1.0,
                y: 1.0,
                pressure: 1.0,
            }],
        };
        let exact_arguments = crate::primitive::raster::canonicalize_exact(
            &stroke,
            exact_color,
            65_536,
            &exact_source.view,
            exact_document.width,
            exact_document.height,
            exact_target,
        )
        .unwrap();
        let exact_outcome = exact_source
            .execute_canonical_stroke(exact_source.document_revision.get(), exact_arguments)
            .unwrap();
        let exact_procedure = exact_outcome.procedure().unwrap();
        let exact_bytes = exact_procedure.canonical_arguments();
        assert_eq!(exact_procedure.primitive_schema_version(), 3);
        assert_eq!(exact_procedure.replay_epoch(), ReplayEpoch::CURRENT);
        assert_eq!(exact_bytes.len(), 41);
        assert_eq!(
            u64::from_le_bytes(exact_bytes[0..8].try_into().unwrap()),
            exact_target
        );
        assert_eq!(
            u32::from_le_bytes(exact_bytes[8..12].try_into().unwrap()),
            2
        );
        assert_eq!(exact_bytes[12], 2);
        assert_eq!(
            &exact_bytes[13..21],
            &[0x23, 0x01, 0x67, 0x45, 0xab, 0x89, 0xef, 0xcd]
        );
        assert_eq!(
            i64::from_le_bytes(exact_bytes[21..29].try_into().unwrap()),
            65_536
        );
        assert_eq!(&exact_bytes[29..33], &1_u32.to_le_bytes());
        assert_eq!(&exact_bytes[33..35], &0_u16.to_le_bytes());
        assert_eq!(&exact_bytes[35..39], &0_u32.to_le_bytes());
        assert_eq!(&exact_bytes[39..41], &[1, 0]);

        let mut exact_replay = initialized_core(0x18);
        exact_replay.replay_procedure(exact_procedure).unwrap();
        assert_eq!(
            exact_source.document_state_digest().unwrap(),
            exact_replay.document_state_digest().unwrap()
        );

        let mut stale_schema = exact_procedure.clone();
        stale_schema.primitive_schema_version = 1;
        let mut rejected_replay = initialized_core(0x18);
        let rejected_document = rejected_replay.document.clone();
        assert!(matches!(
            rejected_replay.replay_procedure(&stale_schema),
            Err(CoreError::InvalidArgument(
                "procedure primitive schema version is unsupported"
            ))
        ));
        assert_eq!(rejected_replay.document, rejected_document);
        assert!(rejected_replay.history.is_empty());

        let mut legacy = initialized_core(0x19);
        let legacy_document = legacy.document_info().unwrap();
        let legacy_outcome = legacy
            .execute_primitive(PrimitiveRequest::ApplyRasterStroke {
                expected_revision: legacy_document.document_revision,
                target_plane_id: legacy_document.color_plane_id,
                stroke: Stroke {
                    auto_erase: false,
                    pressure_size: true,
                    ..stroke
                },
            })
            .unwrap();
        let legacy_bytes = legacy_outcome.procedure().unwrap().canonical_arguments();
        assert_eq!(legacy_bytes.len(), 37);
        assert_eq!(
            u64::from_le_bytes(legacy_bytes[0..8].try_into().unwrap()),
            legacy_document.color_plane_id
        );
        assert_eq!(&legacy_bytes[8..12], &2_u32.to_le_bytes());
        assert_eq!(&legacy_bytes[12..17], &[1, 9, 8, 7, 6]);
        assert_eq!(
            i64::from_le_bytes(legacy_bytes[17..25].try_into().unwrap()),
            65_536
        );
        assert_eq!(&legacy_bytes[25..29], &1_u32.to_le_bytes());
        assert_eq!(&legacy_bytes[29..31], &0_u16.to_le_bytes());
        assert_eq!(&legacy_bytes[31..35], &0_u32.to_le_bytes());
        assert_eq!(&legacy_bytes[35..37], &[0, 1]);
    }

    #[test]
    fn primitive_schema_catalog_is_per_primitive_and_exact_current() {
        assert_eq!(
            current_primitive_schema_version(PrimitiveId::SET_MAIN_LINE_COLOR),
            Some(1)
        );
        assert_eq!(
            current_primitive_schema_version(PrimitiveId::REPLACE_PALETTE),
            Some(1)
        );
        assert_eq!(
            current_primitive_schema_version(PrimitiveId::APPLY_RASTER_STROKE),
            Some(3)
        );

        let mut main_line = initialized_core(0x20);
        let main_line_procedure = main_line
            .execute_primitive(PrimitiveRequest::SetMainLineColor {
                expected_revision: main_line.document_revision.get(),
                color: PixelValue::Rgba([1, 2, 3, 255]),
            })
            .unwrap()
            .procedure()
            .unwrap()
            .clone();
        assert_eq!(main_line_procedure.primitive_schema_version(), 1);

        let mut palette = initialized_core(0x21);
        let palette_procedure = palette
            .execute_primitive(PrimitiveRequest::ReplacePalette {
                expected_revision: palette.document_revision.get(),
                colors: vec![PixelValue::Rgba16([1, 2, 3, 4])],
            })
            .unwrap()
            .procedure()
            .unwrap()
            .clone();
        assert_eq!(palette_procedure.primitive_schema_version(), 1);

        for mut wrong in [main_line_procedure, palette_procedure] {
            wrong.primitive_schema_version = 2;
            let mut target = initialized_core(match wrong.primitive_id {
                PrimitiveId::SET_MAIN_LINE_COLOR => 0x20,
                PrimitiveId::REPLACE_PALETTE => 0x21,
                _ => unreachable!("the test covers only metadata primitives"),
            });
            assert!(matches!(
                target.replay_procedure(&wrong),
                Err(CoreError::InvalidArgument(
                    "procedure primitive schema version is unsupported"
                ))
            ));
            assert!(target.history.is_empty());
        }
    }
}
