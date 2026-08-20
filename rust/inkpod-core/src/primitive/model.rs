//! Stable public types for canonical primitive execution and replay.

use crate::{AssetId, ColorChartEntry, DispatchOutcome, PixelValue, RasterAssetInput, Stroke};
use std::sync::Arc;

macro_rules! public_id {
    ($name:ident, $inner:ty, $docs:literal) => {
        #[doc = $docs]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[repr(transparent)]
        pub struct $name($inner);

        impl $name {
            /// Returns the fixed-width numeric representation.
            #[must_use]
            pub const fn get(self) -> $inner {
                self.0
            }
        }
    };
}

public_id!(
    PrimitiveId,
    u32,
    "A stable nonzero identifier in the built-in primitive catalog."
);
public_id!(
    ProcedureId,
    u64,
    "A monotonically allocated identifier for one committed canonical procedure."
);
public_id!(
    StateId,
    u64,
    "A nonzero persistent identifier for Genesis or one committed semantic state in a document namespace. IDs remain unique until that document is replaced."
);
public_id!(
    ReplayEpoch,
    u32,
    "The closed replay-semantics epoch required by a canonical procedure."
);

impl PrimitiveId {
    pub(crate) const fn from_raw(value: u32) -> Self {
        Self(value)
    }
    /// Primitive ID for paper/frame metadata replacement.
    pub const UPDATE_PAPER_FRAMES: Self = Self(0x0001_0001);
    /// Primitive ID for creating one typed layer.
    pub const CREATE_LAYER: Self = Self(0x0002_0001);
    /// Primitive ID for duplicating one typed layer.
    pub const DUPLICATE_LAYER: Self = Self(0x0002_0002);
    /// Primitive ID for deleting one typed layer.
    pub const DELETE_LAYER: Self = Self(0x0002_0003);
    /// Primitive ID for reordering one typed layer.
    pub const REORDER_LAYER: Self = Self(0x0002_0004);
    /// Primitive ID for replacing one layer's properties.
    pub const SET_LAYER_PROPERTIES: Self = Self(0x0002_0005);
    /// Primitive ID for creating one typed plane.
    pub const CREATE_PLANE: Self = Self(0x0002_0011);
    /// Primitive ID for duplicating one typed plane.
    pub const DUPLICATE_PLANE: Self = Self(0x0002_0012);
    /// Primitive ID for deleting one typed plane.
    pub const DELETE_PLANE: Self = Self(0x0002_0013);
    /// Primitive ID for reordering one typed plane.
    pub const REORDER_PLANE: Self = Self(0x0002_0014);
    /// Primitive ID for replacing one plane's properties.
    pub const SET_PLANE_PROPERTIES: Self = Self(0x0002_0015);
    /// Primitive ID for converting one raster plane.
    pub const CONVERT_PLANE: Self = Self(0x0002_0016);
    /// Primitive ID for merging one raster plane into its lower sibling.
    pub const MERGE_PLANE: Self = Self(0x0002_0017);
    /// Primitive ID for converting one coloring layer.
    pub const CONVERT_LAYER: Self = Self(0x0002_0021);
    /// Primitive ID for merging one layer into its lower sibling.
    pub const MERGE_LAYER: Self = Self(0x0002_0022);
    /// Primitive ID for deleting every hidden layer as one atomic topology edit.
    pub const DELETE_HIDDEN_LAYERS: Self = Self(0x0002_0023);
    /// Primitive ID for one grouped layer/plane edit-target command.
    pub const EDIT_TARGETS: Self = Self(0x0002_0030);
    /// Primitive ID for one typed angled shooting-frame object edit.
    pub const EDIT_SHOOTING_FRAME: Self = Self(0x0002_0050);
    /// Primitive ID for one atomic bounded vanishing-point edit batch.
    pub const EDIT_VANISHING_POINTS: Self = Self(0x0002_0060);
    /// Primitive ID for main-line display color replacement.
    pub const SET_MAIN_LINE_COLOR: Self = Self(0x0003_0001);
    /// Primitive ID for ordered palette replacement.
    pub const REPLACE_PALETTE: Self = Self(0x0003_0002);
    /// Primitive ID for ordered named Color chart and lock replacement.
    pub const REPLACE_COLOR_CHART: Self = Self(0x0003_0003);
    /// Primitive ID for one bounded raster stroke transaction.
    pub const APPLY_RASTER_STROKE: Self = Self(0x0005_0001);
    /// Primitive ID for one bounded raster fill transaction.
    pub const APPLY_FILL: Self = Self(0x0005_0002);
    /// Primitive ID for one canonical raster geometry transaction.
    pub const APPLY_GEOMETRY: Self = Self(0x0005_0003);
    /// Primitive ID for applying a raster gradient.
    pub const APPLY_GRADIENT: Self = Self(0x0005_0010);
    /// Primitive ID for applying boundary-aware airbrush processing.
    pub const APPLY_BOUNDARY_AIRBRUSH: Self = Self(0x0005_0011);
    /// Primitive ID for applying a bounded raster blur.
    pub const APPLY_BLUR: Self = Self(0x0005_0012);
    /// Primitive ID for applying one raster airbrush dab.
    pub const APPLY_AIRBRUSH: Self = Self(0x0005_0013);
    /// Primitive ID for applying one canonical airbrush gesture.
    pub const APPLY_AIRBRUSH_GESTURE: Self = Self(0x0005_0014);
    /// Primitive ID for applying one raster stamp.
    pub const APPLY_STAMP: Self = Self(0x0005_0015);
    /// Primitive ID for applying one canonical stamp gesture.
    pub const APPLY_STAMP_GESTURE: Self = Self(0x0005_0016);
    /// Primitive ID for applying a shape-bounded blur tool operation.
    pub const APPLY_BLUR_TOOL: Self = Self(0x0005_0017);
    /// Primitive ID for applying bounded dust removal.
    pub const APPLY_DUST_REMOVAL: Self = Self(0x0005_0018);
    /// Primitive ID for replacing a raster plane's alpha from a mask.
    pub const EDIT_PLANE_ALPHA: Self = Self(0x0005_0019);
    /// Primitive ID for applying a gradient to raster alpha.
    pub const APPLY_ALPHA_GRADIENT: Self = Self(0x0005_001a);
    /// Primitive ID for committing a filter operation.
    pub const APPLY_FILTER: Self = Self(0x0005_0020);
    /// Primitive ID for creating an adjustment layer.
    pub const CREATE_ADJUSTMENT_LAYER: Self = Self(0x0005_0030);
    /// Primitive ID for replacing an adjustment layer's parameters.
    pub const UPDATE_ADJUSTMENT_LAYER: Self = Self(0x0005_0031);
    /// Primitive ID for exact bounded raster color replacement.
    pub const REPLACE_RASTER_COLORS: Self = Self(0x0005_0040);
    /// Primitive ID for separating bounded raster colors.
    pub const SEPARATE_RASTER_COLORS: Self = Self(0x0005_0041);
    /// Primitive ID for restoring selected raster pixels from an ingested source.
    pub const RESTORE_SELECTED_PIXELS: Self = Self(0x0005_0042);
    /// Primitive ID for exact region-scoped raster color replacement.
    pub const SCOPED_COLOR_REPLACE: Self = Self(0x0005_0043);
    /// Primitive ID for replacing one existing raster plane from an immutable asset.
    pub const IMPORT_RASTER_ASSET: Self = Self(0x0009_0001);
    /// Primitive ID for adding one document guide.
    pub const ADD_GUIDE: Self = Self(0x0004_0001);
    /// Primitive ID for moving one document guide.
    pub const MOVE_GUIDE: Self = Self(0x0004_0002);
    /// Primitive ID for deleting one document guide.
    pub const DELETE_GUIDE: Self = Self(0x0004_0003);
    /// Primitive ID for replacing the document grid.
    pub const SET_GRID: Self = Self(0x0004_0010);
    /// Primitive ID for deleting every document guide as one atomic edit.
    pub const DELETE_ALL_GUIDES: Self = Self(0x0004_0011);
    /// Primitive ID for combining one selection shape.
    pub const APPLY_SELECTION: Self = Self(0x0006_0001);
    /// Primitive ID for inverting the selection mask.
    pub const INVERT_SELECTION: Self = Self(0x0006_0002);
    /// Primitive ID for clearing the selection mask.
    pub const CLEAR_SELECTION: Self = Self(0x0006_0003);
    /// Primitive ID for expanding or shrinking the selection mask.
    pub const RESIZE_SELECTION: Self = Self(0x0006_0004);
    /// Primitive ID for selecting pixels by exact-depth color.
    pub const SELECT_COLOR: Self = Self(0x0006_0005);
    /// Primitive ID for selecting visible composite pixels outside an output-color guard.
    pub const SELECT_OUTPUT_COLOR_GUARD: Self = Self(0x0006_0006);
    /// Primitive ID for converting the selection mask into a layer.
    pub const SELECTION_TO_LAYER: Self = Self(0x0006_0010);
    /// Primitive ID for combining a selection layer into the active mask.
    pub const SELECTION_FROM_LAYER: Self = Self(0x0006_0011);
    /// Primitive ID for clearing selected content on a captured target.
    pub const CLEAR_SELECTED_CONTENT: Self = Self(0x0006_0020);
    /// Primitive ID for committing one typed floating selection.
    pub const COMMIT_FLOATING: Self = Self(0x0006_0021);
    /// Primitive ID for mirroring all document data.
    pub const MIRROR_DOCUMENT: Self = Self(0x0007_0001);
    /// Primitive ID for rotating all document data by a right angle.
    pub const ROTATE_DOCUMENT: Self = Self(0x0007_0002);
    /// Primitive ID for resizing document data and frame metadata.
    pub const RESIZE_DOCUMENT: Self = Self(0x0007_0003);
    /// Primitive ID for replacing Light Table global opacity.
    pub const LIGHT_TABLE_SET_GLOBAL_OPACITY: Self = Self(0x000a_0001);
    /// Primitive ID for creating a Light Table set.
    pub const LIGHT_TABLE_CREATE_SET: Self = Self(0x000a_0002);
    /// Primitive ID for duplicating a Light Table set.
    pub const LIGHT_TABLE_DUPLICATE_SET: Self = Self(0x000a_0003);
    /// Primitive ID for deleting a Light Table set.
    pub const LIGHT_TABLE_DELETE_SET: Self = Self(0x000a_0004);
    /// Primitive ID for renaming a Light Table set.
    pub const LIGHT_TABLE_RENAME_SET: Self = Self(0x000a_0005);
    /// Primitive ID for reordering a Light Table set.
    pub const LIGHT_TABLE_REORDER_SET: Self = Self(0x000a_0006);
    /// Primitive ID for selecting the active Light Table set.
    pub const LIGHT_TABLE_SET_ACTIVE: Self = Self(0x000a_0007);
    /// Primitive ID for adding a Light Table item.
    pub const LIGHT_TABLE_ADD_ITEM: Self = Self(0x000a_0010);
    /// Primitive ID for replacing Light Table item properties.
    pub const LIGHT_TABLE_UPDATE_ITEM_PROPERTIES: Self = Self(0x000a_0011);
    /// Primitive ID for replacing a Light Table item's immutable source.
    pub const LIGHT_TABLE_UPDATE_ITEM: Self = Self(0x000a_0012);
    /// Primitive ID for removing a Light Table item.
    pub const LIGHT_TABLE_REMOVE_ITEM: Self = Self(0x000a_0013);
    /// Primitive ID for reordering a Light Table item.
    pub const LIGHT_TABLE_REORDER_ITEM: Self = Self(0x000a_0014);
    /// Primitive ID for swapping Light Table content with the active plane.
    pub const LIGHT_TABLE_SWAP_WITH_ACTIVE: Self = Self(0x000a_0015);
    /// Primitive ID for inserting one resolved natural-sequence Light Table block.
    pub const LIGHT_TABLE_BULK_REGISTER: Self = Self(0x000a_0016);
}

impl ProcedureId {
    pub(crate) const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub(crate) const fn first() -> Self {
        Self(1)
    }

    pub(crate) const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) if value <= crate::MAX_PERSISTENT_NUMERIC_ID => Some(Self(value)),
            None => None,
            Some(_) => None,
        }
    }
}

impl StateId {
    /// State ID assigned to a document's Genesis state.
    pub const GENESIS: Self = Self(1);

    pub(crate) const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub(crate) const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) if value <= crate::MAX_PERSISTENT_NUMERIC_ID => Some(Self(value)),
            None => None,
            Some(_) => None,
        }
    }
}

impl ReplayEpoch {
    /// Replay epoch used by every built-in primitive in this Core version.
    pub const CURRENT: Self = Self(24);
}

/// Exact current top-level procedure-authoritative native format version.
///
/// The build, reader, writer, and replay contract all use this value. Earlier
/// and later top-level versions are rejected without migration.
pub const PROCEDURE_FORMAT_VERSION: u32 = 27;

/// Version of the canonical scalar, rounding, alpha, and geometry contract.
pub const CANONICAL_NUMERIC_VERSION: u32 = 1;

/// Immutable build contract for canonical procedure replay.
///
/// The catalog digest covers every stable built-in primitive ID, its exact
/// schema version, canonical name, argument-schema digest, semantics revision,
/// work-formula ID, and replay policy. Querying this value is side-effect free and does not
/// require a document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayContract {
    pub(crate) replay_epoch: ReplayEpoch,
    pub(crate) procedure_format_version: u32,
    pub(crate) canonical_numeric_version: u32,
    pub(crate) primitive_count: u32,
    pub(crate) primitive_catalog_digest: [u8; 32],
}

impl ReplayContract {
    /// Returns the exact replay-semantics epoch accepted by this build.
    #[must_use]
    pub const fn replay_epoch(self) -> ReplayEpoch {
        self.replay_epoch
    }

    /// Returns the exact current top-level native format version.
    #[must_use]
    pub const fn procedure_format_version(self) -> u32 {
        self.procedure_format_version
    }

    /// Returns the canonical numeric-contract version.
    #[must_use]
    pub const fn canonical_numeric_version(self) -> u32 {
        self.canonical_numeric_version
    }

    /// Returns the number of stable entries covered by the catalog digest.
    #[must_use]
    pub const fn primitive_count(self) -> u32 {
        self.primitive_count
    }

    /// Borrows the BLAKE3-256 primitive-catalog digest.
    #[must_use]
    pub const fn primitive_catalog_digest(&self) -> &[u8; 32] {
        &self.primitive_catalog_digest
    }
}

/// A BLAKE3-256 digest of canonical semantic document-state schema-10 bytes.
///
/// The compact root and semantic metadata frames use schema version 10 in the
/// `org.inkpod.digest.document-state.v8` derive-key domain. Raster payloads
/// enter that root through separately domain-separated, content-addressed tile
/// and raster commitments, so the digest is independent of edit order and
/// allocation history without requiring unchanged tile bytes to be rehashed
/// after an edit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct DocumentStateDigest([u8; 32]);

impl DocumentStateDigest {
    pub(crate) const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrows the 32 digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// A validated frontend request for one of the canonical document primitives.
///
/// `expected_revision` is session-local and rejects stale UI work. It is never
/// stored in the resulting procedure; replay uses persistent [`StateId`] and
/// the pre-state digest instead.
#[derive(Clone, Debug, PartialEq)]
pub enum PrimitiveRequest {
    /// Replaces the document's exact-depth main-line display color.
    SetMainLineColor {
        /// Document revision observed by the request producer.
        expected_revision: u64,
        /// Straight-alpha RGBA8 or RGBA16 color.
        color: PixelValue,
    },
    /// Replaces all palette entries while retaining their supplied order.
    ReplacePalette {
        /// Document revision observed by the request producer.
        expected_revision: u64,
        /// Owned exact-depth RGBA8/RGBA16 palette entries.
        colors: Vec<PixelValue>,
    },
    /// Replaces the independent named Color chart and its edit lock.
    ReplaceColorChart {
        /// Document revision observed by the request producer.
        expected_revision: u64,
        /// Owned exact-depth, named Color chart entries.
        entries: Vec<ColorChartEntry>,
        /// Whether subsequent document-changing chart commands are rejected.
        locked: bool,
    },
    /// Applies one bounded raster stroke to an exact stable plane ID.
    ApplyRasterStroke {
        /// Document revision observed by the request producer.
        expected_revision: u64,
        /// Stable target Plane ID resolved before canonicalization.
        target_plane_id: u64,
        /// Owned stroke settings and samples.
        stroke: Stroke,
    },
    /// Replaces one existing editable plane from canonical immutable raster bytes.
    ImportRasterAsset {
        /// Document revision observed by the request producer.
        expected_revision: u64,
        /// Stable destination Plane ID resolved before canonicalization.
        target_plane_id: u64,
        /// Owned canonical raster descriptor and bytes. External paths and encoded
        /// source bytes are deliberately absent from this semantic request.
        raster: RasterAssetInput,
    },
}

impl PrimitiveRequest {
    pub(crate) const fn expected_revision(&self) -> u64 {
        match self {
            Self::SetMainLineColor {
                expected_revision, ..
            }
            | Self::ReplacePalette {
                expected_revision, ..
            }
            | Self::ReplaceColorChart {
                expected_revision, ..
            }
            | Self::ApplyRasterStroke {
                expected_revision, ..
            }
            | Self::ImportRasterAsset {
                expected_revision, ..
            } => *expected_revision,
        }
    }
}

/// The canonical, caller-lifetime-independent record of one committed primitive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalProcedure {
    pub(crate) procedure_id: ProcedureId,
    pub(crate) primitive_id: PrimitiveId,
    pub(crate) primitive_schema_version: u16,
    pub(crate) replay_epoch: ReplayEpoch,
    pub(crate) base_state_id: StateId,
    pub(crate) committed_state_id: StateId,
    pub(crate) input_ids: Vec<u64>,
    pub(crate) output_ids: Vec<u64>,
    pub(crate) asset_ids: Vec<AssetId>,
    pub(crate) canonical_arguments: Vec<u8>,
    pub(crate) canonical_payload: Vec<u8>,
    pub(crate) canonical_payload_digest: [u8; 32],
    pub(crate) pre_state_digest: DocumentStateDigest,
    pub(crate) post_state_digest: DocumentStateDigest,
    pub(crate) runtime_invocation: Option<super::invocation::RuntimeInvocation>,
}

impl CanonicalProcedure {
    /// Returns this committed procedure's monotonic ID.
    #[must_use]
    pub const fn procedure_id(&self) -> ProcedureId {
        self.procedure_id
    }

    /// Returns the stable built-in primitive ID.
    #[must_use]
    pub const fn primitive_id(&self) -> PrimitiveId {
        self.primitive_id
    }

    /// Returns the primitive argument/payload schema version.
    #[must_use]
    pub const fn primitive_schema_version(&self) -> u16 {
        self.primitive_schema_version
    }

    /// Returns the replay epoch required by this procedure.
    #[must_use]
    pub const fn replay_epoch(&self) -> ReplayEpoch {
        self.replay_epoch
    }

    /// Returns the persistent state on which the procedure depends.
    #[must_use]
    pub const fn base_state_id(&self) -> StateId {
        self.base_state_id
    }

    /// Returns the persistent state created by the procedure.
    #[must_use]
    pub const fn committed_state_id(&self) -> StateId {
        self.committed_state_id
    }

    /// Borrows stable input object IDs in schema role order.
    #[must_use]
    pub fn input_ids(&self) -> &[u64] {
        &self.input_ids
    }

    /// Borrows transaction-allocated output object IDs in schema role order.
    #[must_use]
    pub fn output_ids(&self) -> &[u64] {
        &self.output_ids
    }

    /// Borrows immutable asset IDs in primitive-schema role order.
    #[must_use]
    pub fn asset_ids(&self) -> &[AssetId] {
        &self.asset_ids
    }

    /// Borrows the canonical fixed-width argument bytes.
    #[must_use]
    pub fn canonical_arguments(&self) -> &[u8] {
        &self.canonical_arguments
    }

    /// Borrows the bounded inline canonical payload.
    #[must_use]
    pub fn canonical_payload(&self) -> &[u8] {
        &self.canonical_payload
    }

    /// Returns the domain-separated digest of a nonempty inline payload.
    ///
    /// The closed empty-payload representation is thirty-two zero bytes.
    #[must_use]
    pub const fn canonical_payload_digest(&self) -> &[u8; 32] {
        &self.canonical_payload_digest
    }

    /// Returns the semantic digest required before replay.
    #[must_use]
    pub const fn pre_state_digest(&self) -> DocumentStateDigest {
        self.pre_state_digest
    }

    /// Returns the semantic digest that replay must produce.
    #[must_use]
    pub const fn post_state_digest(&self) -> DocumentStateDigest {
        self.post_state_digest
    }
}

/// Result of canonical primitive execution or replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrimitiveOutcome {
    pub(crate) dispatch: DispatchOutcome,
    pub(crate) procedure: Option<Arc<CanonicalProcedure>>,
}

impl PrimitiveOutcome {
    pub(crate) const fn no_op(dispatch: DispatchOutcome) -> Self {
        Self {
            dispatch,
            procedure: None,
        }
    }

    pub(crate) const fn committed(
        dispatch: DispatchOutcome,
        procedure: Arc<CanonicalProcedure>,
    ) -> Self {
        Self {
            dispatch,
            procedure: Some(procedure),
        }
    }

    /// Returns revision and accepted-command metadata.
    #[must_use]
    pub const fn dispatch(&self) -> DispatchOutcome {
        self.dispatch
    }

    /// Borrows the committed canonical procedure, or `None` for a semantic no-op.
    #[must_use]
    pub fn procedure(&self) -> Option<&CanonicalProcedure> {
        self.procedure.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CanonicalPrimitive {
    SetMainLineColor(PixelValue),
    ReplacePalette(Vec<PixelValue>),
    ReplaceColorChart {
        entries: Vec<ColorChartEntry>,
        locked: bool,
    },
    ApplyRasterStroke(CanonicalStrokeArguments),
    ImportRasterAsset {
        target_plane_id: u64,
        asset_id: AssetId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CanonicalStrokeArguments {
    pub(crate) target_plane_id: u64,
    pub(crate) tool_code: u32,
    pub(crate) color: PixelValue,
    pub(crate) diameter_q16: i64,
    pub(crate) shape_code: u32,
    pub(crate) smoothing: u16,
    pub(crate) start_color_code: u32,
    pub(crate) auto_erase: bool,
    pub(crate) pressure_size: bool,
    pub(crate) payload: Vec<u8>,
}
