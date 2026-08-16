//! Runtime resolution for schema-checked InkScript entity references.

use inkpod_format::{InkScriptReferenceSegment, InkScriptTypedValue, InkScriptTypedValueKind};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InkScriptEntityKind {
    Layer,
    Plane,
    Guide,
    VectorPath,
    VectorFill,
    Annotation,
    ShootingFrame,
    VanishingPoint,
}

impl InkScriptEntityKind {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "layer" | "layer_ref" => Some(Self::Layer),
            "plane" | "plane_ref" => Some(Self::Plane),
            "guide" | "guide_ref" => Some(Self::Guide),
            "vector_path" | "vector_path_ref" => Some(Self::VectorPath),
            "vector_fill" | "vector_fill_ref" => Some(Self::VectorFill),
            "annotation" | "annotation_ref" => Some(Self::Annotation),
            "shooting_frame" | "shooting_frame_ref" => Some(Self::ShootingFrame),
            "vanishing_point" | "vanishing_point_ref" => Some(Self::VanishingPoint),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedInkScriptReference {
    pub(crate) kind: InkScriptEntityKind,
    pub(crate) persistent_id: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InkScriptReferenceError {
    InvalidReference,
    MissingReference,
    KindMismatch,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct InkScriptRuntimeReferences {
    entries: BTreeMap<String, ResolvedInkScriptReference>,
}

impl InkScriptRuntimeReferences {
    pub(crate) fn insert(
        &mut self,
        key: impl Into<String>,
        kind: InkScriptEntityKind,
        persistent_id: u64,
    ) -> Result<(), InkScriptReferenceError> {
        if persistent_id == 0 {
            return Err(InkScriptReferenceError::InvalidReference);
        }
        self.entries.insert(
            key.into(),
            ResolvedInkScriptReference {
                kind,
                persistent_id,
            },
        );
        Ok(())
    }

    pub(crate) fn resolve(
        &self,
        value: &InkScriptTypedValue,
        expected: InkScriptEntityKind,
    ) -> Result<u64, InkScriptReferenceError> {
        let key = reference_key(value)?;
        let resolved = self
            .entries
            .get(&key)
            .ok_or(InkScriptReferenceError::MissingReference)?;
        if resolved.kind != expected {
            return Err(InkScriptReferenceError::KindMismatch);
        }
        Ok(resolved.persistent_id)
    }

    #[cfg(test)]
    pub(crate) fn entry_mut(&mut self, key: &str) -> Option<&mut ResolvedInkScriptReference> {
        self.entries.get_mut(key)
    }

    #[cfg(test)]
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

fn reference_key(value: &InkScriptTypedValue) -> Result<String, InkScriptReferenceError> {
    let InkScriptTypedValueKind::Reference { root, segments } = value.kind() else {
        return Err(InkScriptReferenceError::InvalidReference);
    };
    match segments.as_slice() {
        [] => Ok(root.clone()),
        [InkScriptReferenceSegment::Index(index)] => Ok(format!("{root}[{index}]")),
        [InkScriptReferenceSegment::Field(field)] => Ok(format!("{root}.{field}")),
        [
            InkScriptReferenceSegment::Field(field),
            InkScriptReferenceSegment::Index(index),
        ] => Ok(format!("{root}.{field}[{index}]")),
        _ => Err(InkScriptReferenceError::InvalidReference),
    }
}
