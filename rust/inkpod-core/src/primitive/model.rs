//! Stable public types for canonical primitive execution and replay.

use crate::{DispatchOutcome, PixelValue, Stroke};
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
    /// Primitive ID for main-line display color replacement.
    pub const SET_MAIN_LINE_COLOR: Self = Self(0x0003_0001);
    /// Primitive ID for ordered palette replacement.
    pub const REPLACE_PALETTE: Self = Self(0x0003_0002);
    /// Primitive ID for one bounded raster stroke transaction.
    pub const APPLY_RASTER_STROKE: Self = Self(0x0005_0001);
}

impl ProcedureId {
    #[cfg(test)]
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
    pub const CURRENT: Self = Self(2);
}

/// A BLAKE3-256 digest of canonical semantic document-state schema-2 bytes.
///
/// The digest uses the `org.inkpod.digest.document-state.v2` derive-key domain;
/// every nested canonical document-state frame carries schema version 2.
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
    /// Applies one bounded raster stroke to an exact stable plane ID.
    ApplyRasterStroke {
        /// Document revision observed by the request producer.
        expected_revision: u64,
        /// Stable target Plane ID resolved before canonicalization.
        target_plane_id: u64,
        /// Owned stroke settings and samples.
        stroke: Stroke,
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
            | Self::ApplyRasterStroke {
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
    pub(crate) canonical_arguments: Vec<u8>,
    pub(crate) canonical_payload: Vec<u8>,
    pub(crate) canonical_payload_digest: [u8; 32],
    pub(crate) pre_state_digest: DocumentStateDigest,
    pub(crate) post_state_digest: DocumentStateDigest,
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
    ApplyRasterStroke(CanonicalStrokeArguments),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CanonicalStrokeArguments {
    pub(crate) target_plane_id: u64,
    pub(crate) tool_code: u32,
    pub(crate) color: [u8; 4],
    pub(crate) diameter_q16: i64,
    pub(crate) auto_erase: bool,
    pub(crate) pressure_size: bool,
    pub(crate) payload: Vec<u8>,
}
