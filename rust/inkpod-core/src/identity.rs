//! Internal stable identities and revision tokens.
//!
//! Public Rust APIs, C ABI records, and file DTOs deliberately keep their
//! existing fixed-width integer representation. Core-owned state uses these
//! distinct wrappers so unrelated identity and revision domains cannot be
//! mixed accidentally.

use std::fmt;

macro_rules! numeric_token {
    ($name:ident, $zero:literal) => {
        #[doc = $zero]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[repr(transparent)]
        pub(crate) struct $name(u64);

        impl $name {
            pub(crate) const fn from_raw(value: u64) -> Self {
                Self(value)
            }

            pub(crate) const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

numeric_token!(DocumentId, "A document stable ID. Zero is invalid.");
numeric_token!(LayerId, "A layer stable ID. Zero is invalid.");
numeric_token!(PlaneId, "A plane stable ID. Zero is invalid.");
numeric_token!(GuideId, "A guide stable ID. Zero is invalid.");
numeric_token!(
    LightTableSetId,
    "A light-table set stable ID. Zero is invalid."
);
numeric_token!(
    LightTableItemId,
    "A light-table item stable ID. Zero is invalid."
);
numeric_token!(VectorPathId, "A vector path stable ID. Zero is invalid.");
numeric_token!(VectorFillId, "A vector fill stable ID. Zero is invalid.");
numeric_token!(ViewId, "A secondary-view stable ID. Zero is invalid.");
numeric_token!(
    DocumentRevision,
    "A committed document revision. Zero means no document revision yet."
);
numeric_token!(
    ViewRevision,
    "A view-only revision. Zero is the initial view revision."
);
numeric_token!(
    RenderRevision,
    "A renderer cache or snapshot revision. Zero means no rendered revision."
);
numeric_token!(
    PreviewRevision,
    "A preview-session revision. Zero is invalid."
);

macro_rules! checked_counter {
    ($name:ident) => {
        impl $name {
            pub(crate) const fn checked_next(self) -> Option<Self> {
                match self.0.checked_add(1) {
                    Some(value) => Some(Self(value)),
                    None => None,
                }
            }
        }
    };
}

checked_counter!(ViewId);
checked_counter!(DocumentRevision);
checked_counter!(ViewRevision);
checked_counter!(PreviewRevision);

/// A history state token. Zero is the initial unsaved state.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub(crate) struct HistoryStateId(u64);

impl HistoryStateId {
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

    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for HistoryStateId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl ViewRevision {
    pub(crate) const fn saturating_next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl RenderRevision {
    pub(crate) const fn wrapping_next_nonzero(self) -> Self {
        let next = self.0.wrapping_add(1);
        Self(if next == 0 { 1 } else { next })
    }
}

/// Cursor for the document-wide stable-ID namespace.
///
/// The cursor is not an object ID. Dedicated allocation methods are used so a
/// caller cannot choose an unrelated output type through a generic conversion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(crate) struct StableIdCursor(u64);

impl StableIdCursor {
    pub(crate) const fn first() -> Self {
        Self(1)
    }

    pub(crate) const fn from_next_raw(value: u64) -> Self {
        Self(if value == 0 { 1 } else { value })
    }

    #[cfg(test)]
    pub(crate) const fn next_raw(self) -> u64 {
        self.0
    }

    pub(crate) fn advance_past_raw(&mut self, maximum: u64) {
        self.0 = self.0.max(maximum.saturating_add(1)).max(1);
    }

    fn take_raw(&mut self) -> u64 {
        let value = self.0;
        self.0 = self.0.saturating_add(1).max(1);
        value
    }

    pub(crate) fn take_document(&mut self) -> DocumentId {
        DocumentId::from_raw(self.take_raw())
    }

    pub(crate) fn take_layer(&mut self) -> LayerId {
        LayerId::from_raw(self.take_raw())
    }

    pub(crate) fn take_plane(&mut self) -> PlaneId {
        PlaneId::from_raw(self.take_raw())
    }

    pub(crate) fn take_guide(&mut self) -> GuideId {
        GuideId::from_raw(self.take_raw())
    }

    pub(crate) fn take_light_table_set(&mut self) -> LightTableSetId {
        LightTableSetId::from_raw(self.take_raw())
    }

    pub(crate) fn take_light_table_item(&mut self) -> LightTableItemId {
        LightTableItemId::from_raw(self.take_raw())
    }

    pub(crate) fn take_vector_path(&mut self) -> VectorPathId {
        VectorPathId::from_raw(self.take_raw())
    }

    pub(crate) fn take_vector_fill(&mut self) -> VectorFillId {
        VectorFillId::from_raw(self.take_raw())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_cursor_preserves_one_namespace_while_returning_distinct_types() {
        let mut cursor = StableIdCursor::first();
        let document = cursor.take_document();
        let layer = cursor.take_layer();
        let plane = cursor.take_plane();

        assert_eq!(document.get(), 1);
        assert_eq!(layer.get(), 2);
        assert_eq!(plane.get(), 3);
        assert_eq!(cursor.next_raw(), 4);
    }

    #[test]
    fn typed_counters_keep_zero_and_overflow_policy_explicit() {
        assert_eq!(
            DocumentRevision::from_raw(0).checked_next().unwrap().get(),
            1
        );
        assert!(
            DocumentRevision::from_raw(u64::MAX)
                .checked_next()
                .is_none()
        );
        assert_eq!(
            ViewRevision::from_raw(u64::MAX).saturating_next().get(),
            u64::MAX
        );
    }
}
