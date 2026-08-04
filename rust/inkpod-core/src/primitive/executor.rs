//! Single validation, canonicalization, execution, and atomic publish boundary.

use super::*;
use crate::document::ensure_editable_plane;
use crate::primitive::digest::{color_bytes, decode_color};
use crate::primitive::raster::{apply as apply_raster_stroke, canonicalize as canonicalize_stroke};
use crate::*;

const PRIMITIVE_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CachePolicy {
    Preserve,
    InvalidateAll,
    RasterRevision,
}

struct PrimitiveTransaction {
    before: CellDocument,
    working: CellDocument,
    next_stable_id: StableIdCursor,
    output_ids: Vec<u64>,
}

impl PrimitiveTransaction {
    fn begin(core: &Core) -> Result<Self, CoreError> {
        let before = core.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        Ok(Self {
            working: before.clone(),
            before,
            next_stable_id: core.next_id,
            output_ids: Vec::new(),
        })
    }
}

struct CanonicalizedRequest {
    primitive: CanonicalPrimitive,
    primitive_id: PrimitiveId,
    input_ids: Vec<u64>,
    arguments: Vec<u8>,
    payload: Vec<u8>,
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
    /// procedure/state ID, history, revision, dirty state, or cache change.
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
        if procedure.primitive_schema_version != PRIMITIVE_SCHEMA_VERSION {
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
        if !procedure.output_ids.is_empty() {
            return Err(CoreError::InvalidArgument(
                "primitive schema does not emit output object IDs",
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
        let canonical = decode_procedure(procedure)?;
        self.execute_canonical(canonical, Some(procedure))
    }

    /// Computes the BLAKE3-256 digest of canonical semantic document state.
    ///
    /// Session revision, history, paths, views, transient previews, allocation
    /// layout, and renderer caches do not contribute to the digest.
    pub fn document_state_digest(&self) -> Result<DocumentStateDigest, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        canonical_document_state(document).map(|(_, digest)| digest)
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
        let payload = arguments.payload.clone();
        let canonical_arguments = encode_stroke_arguments(&arguments);
        let target_plane_id = arguments.target_plane_id;
        self.execute_canonical(
            CanonicalizedRequest {
                primitive: CanonicalPrimitive::ApplyRasterStroke(arguments),
                primitive_id: PrimitiveId::APPLY_RASTER_STROKE,
                input_ids: vec![target_plane_id],
                arguments: canonical_arguments,
                payload,
            },
            None,
        )
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
                    arguments: color_bytes(color)?,
                    payload: Vec::new(),
                })
            }
            PrimitiveRequest::ReplacePalette { colors, .. } => {
                let arguments = encode_palette(&colors)?;
                Ok(CanonicalizedRequest {
                    primitive: CanonicalPrimitive::ReplacePalette(colors),
                    primitive_id: PrimitiveId::REPLACE_PALETTE,
                    input_ids: Vec::new(),
                    arguments,
                    payload: Vec::new(),
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
                let payload = arguments.payload.clone();
                let encoded_arguments = encode_stroke_arguments(&arguments);
                Ok(CanonicalizedRequest {
                    primitive: CanonicalPrimitive::ApplyRasterStroke(arguments),
                    primitive_id: PrimitiveId::APPLY_RASTER_STROKE,
                    input_ids: vec![target_plane_id],
                    arguments: encoded_arguments,
                    payload,
                })
            }
        }
    }

    fn execute_canonical(
        &mut self,
        canonical: CanonicalizedRequest,
        replay: Option<&CanonicalProcedure>,
    ) -> Result<PrimitiveOutcome, CoreError> {
        let pre_state_digest = self.document_state_digest()?;
        let mut transaction = PrimitiveTransaction::begin(self)?;
        let staging_revision = self
            .document_revision
            .checked_next()
            .unwrap_or(self.document_revision);
        let applied = apply_primitive(
            &mut transaction.working,
            &transaction.before,
            &canonical.primitive,
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
        let (_, post_state_digest) = canonical_document_state(&transaction.working)?;

        if let Some(expected) = replay {
            if expected.primitive_id != canonical.primitive_id
                || expected.input_ids != canonical.input_ids
                || expected.output_ids != transaction.output_ids
                || expected.canonical_arguments != canonical.arguments
                || expected.canonical_payload != canonical.payload
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

        let payload_digest = canonical_payload_digest(&canonical.payload)?;
        let procedure = CanonicalProcedure {
            procedure_id,
            primitive_id: canonical.primitive_id,
            primitive_schema_version: PRIMITIVE_SCHEMA_VERSION,
            replay_epoch: ReplayEpoch::CURRENT,
            base_state_id: StateId::from_raw(self.current_state.get()),
            committed_state_id: StateId::from_raw(next_state.get()),
            input_ids: canonical.input_ids,
            output_ids: transaction.output_ids,
            canonical_arguments: canonical.arguments,
            canonical_payload: canonical.payload,
            canonical_payload_digest: payload_digest,
            pre_state_digest,
            post_state_digest,
        };

        // `commit_history_change` cannot report `Vec::push` allocation failure.
        // Reserve before the publish point so every recoverable capacity error
        // still leaves the live document and all counters untouched.
        self.history
            .try_reserve(1)
            .map_err(|_| CoreError::InvalidState("history allocation failed"))?;

        self.document = Some(transaction.working);
        self.document_revision = revision;
        self.next_state = following_state;
        self.next_procedure = following_procedure;
        self.next_id = transaction.next_stable_id;
        match applied.cache_policy {
            CachePolicy::Preserve | CachePolicy::RasterRevision => {}
            CachePolicy::InvalidateAll => self.render_cache.clear(),
        }
        self.commit_history_change(applied.history, next_state);
        let dispatch = DispatchOutcome {
            revision: revision.get(),
            accepted_commands: 1,
        };
        Ok(PrimitiveOutcome::committed(dispatch, procedure))
    }
}

fn apply_primitive(
    working: &mut CellDocument,
    before: &CellDocument,
    primitive: &CanonicalPrimitive,
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
        CanonicalPrimitive::ApplyRasterStroke(arguments) => {
            let changes = apply_raster_stroke(working, arguments, revision)?;
            if changes.is_empty() {
                return Ok(None);
            }
            let plane_id = PlaneId::from_raw(arguments.target_plane_id);
            working.active_plane_id = plane_id;
            Ok(Some(AppliedPrimitive {
                history: HistoryChange::Pixels { plane_id, changes },
                cache_policy: CachePolicy::RasterRevision,
            }))
        }
    }
    .map(|applied| if before == working { None } else { applied })
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

fn decode_palette(bytes: &[u8]) -> Result<Vec<PixelValue>, CoreError> {
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

fn encode_stroke_arguments(arguments: &CanonicalStrokeArguments) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(19);
    bytes.extend_from_slice(&arguments.tool_code.to_le_bytes());
    bytes.push(1);
    bytes.extend_from_slice(&arguments.color);
    bytes.extend_from_slice(&arguments.diameter_q16.to_le_bytes());
    bytes.push(u8::from(arguments.auto_erase));
    bytes.push(u8::from(arguments.pressure_size));
    bytes
}

fn decode_procedure(procedure: &CanonicalProcedure) -> Result<CanonicalizedRequest, CoreError> {
    if !procedure.output_ids.is_empty() {
        return Err(CoreError::InvalidArgument(
            "primitive schema does not permit output IDs",
        ));
    }
    match procedure.primitive_id {
        PrimitiveId::SET_MAIN_LINE_COLOR => {
            if !procedure.input_ids.is_empty() || !procedure.canonical_payload.is_empty() {
                return Err(CoreError::InvalidArgument(
                    "main-line color procedure has unexpected roles or payload",
                ));
            }
            let color = decode_color(&procedure.canonical_arguments)?;
            Ok(CanonicalizedRequest {
                primitive: CanonicalPrimitive::SetMainLineColor(color),
                primitive_id: procedure.primitive_id,
                input_ids: Vec::new(),
                arguments: procedure.canonical_arguments.clone(),
                payload: Vec::new(),
            })
        }
        PrimitiveId::REPLACE_PALETTE => {
            if !procedure.input_ids.is_empty() || !procedure.canonical_payload.is_empty() {
                return Err(CoreError::InvalidArgument(
                    "palette procedure has unexpected roles or payload",
                ));
            }
            let colors = decode_palette(&procedure.canonical_arguments)?;
            Ok(CanonicalizedRequest {
                primitive: CanonicalPrimitive::ReplacePalette(colors),
                primitive_id: procedure.primitive_id,
                input_ids: Vec::new(),
                arguments: procedure.canonical_arguments.clone(),
                payload: Vec::new(),
            })
        }
        PrimitiveId::APPLY_RASTER_STROKE => decode_stroke_procedure(procedure),
        _ => Err(CoreError::InvalidArgument(
            "primitive ID is not in the catalog",
        )),
    }
}

fn decode_stroke_procedure(
    procedure: &CanonicalProcedure,
) -> Result<CanonicalizedRequest, CoreError> {
    if procedure.input_ids.len() != 1 || procedure.canonical_arguments.len() != 19 {
        return Err(CoreError::InvalidArgument(
            "raster stroke procedure has invalid canonical roles or arguments",
        ));
    }
    let bytes = &procedure.canonical_arguments;
    let tool_code = u32::from_le_bytes(bytes[0..4].try_into().expect("fixed-width slice"));
    if bytes[4] != 1 || !matches!(bytes[17], 0 | 1) || !matches!(bytes[18], 0 | 1) {
        return Err(CoreError::InvalidArgument(
            "raster stroke arguments are not canonical",
        ));
    }
    let color = [bytes[5], bytes[6], bytes[7], bytes[8]];
    let diameter_q16 = i64::from_le_bytes(bytes[9..17].try_into().expect("fixed-width slice"));
    let arguments = CanonicalStrokeArguments {
        target_plane_id: procedure.input_ids[0],
        tool_code,
        color,
        diameter_q16,
        auto_erase: bytes[17] != 0,
        pressure_size: bytes[18] != 0,
        payload: procedure.canonical_payload.clone(),
    };
    crate::primitive::raster::decode_payload(&arguments.payload)?;
    let encoded = encode_stroke_arguments(&arguments);
    Ok(CanonicalizedRequest {
        primitive: CanonicalPrimitive::ApplyRasterStroke(arguments),
        primitive_id: procedure.primitive_id,
        input_ids: procedure.input_ids.clone(),
        arguments: encoded,
        payload: procedure.canonical_payload.clone(),
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
        state_boundary.current_state = HistoryStateId::from_raw(MAX_PERSISTENT_NUMERIC_ID - 2);
        state_boundary.next_state = HistoryStateId::from_raw(MAX_PERSISTENT_NUMERIC_ID - 1);
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
        procedure_boundary.next_procedure = ProcedureId::from_raw(MAX_PERSISTENT_NUMERIC_ID - 1);
        let outcome = procedure_boundary
            .execute_primitive(PrimitiveRequest::SetMainLineColor {
                expected_revision: procedure_boundary.document_revision.get(),
                color: PixelValue::Rgba([2, 2, 2, 255]),
            })
            .unwrap();
        assert_eq!(
            outcome.procedure().unwrap().procedure_id().get(),
            MAX_PERSISTENT_NUMERIC_ID - 1
        );
        assert_eq!(
            procedure_boundary.next_procedure.get(),
            MAX_PERSISTENT_NUMERIC_ID
        );

        let mut state_overflow = initialized_core(0x11);
        state_overflow.current_state = HistoryStateId::from_raw(MAX_PERSISTENT_NUMERIC_ID - 1);
        state_overflow.next_state = HistoryStateId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
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
        state_exhausted.current_state = HistoryStateId::from_raw(MAX_PERSISTENT_NUMERIC_ID - 1);
        state_exhausted.next_state = HistoryStateId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
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
}
