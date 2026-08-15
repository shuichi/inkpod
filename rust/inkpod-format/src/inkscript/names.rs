use std::collections::BTreeSet;

use super::schema::{InkScriptSemanticErrorCode, is_identifier};

/// Deterministic generated-symbol allocator.
///
/// It uses only the supplied names, occurrence order, and the smallest available decimal suffix;
/// hash order, locale, clock, and process entropy are not observed.
#[derive(Clone, Debug, Default)]
pub struct InkScriptGeneratedNames {
    used: BTreeSet<String>,
}

impl InkScriptGeneratedNames {
    /// Creates an allocator and rejects an invalid existing identifier.
    pub fn new<'a>(
        existing: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, InkScriptSemanticErrorCode> {
        let mut result = Self::default();
        for name in existing {
            if !is_identifier(name) {
                return Err(InkScriptSemanticErrorCode::InvalidGeneratedName);
            }
            result.used.insert(name.to_owned());
        }
        Ok(result)
    }

    /// Allocates `<stem>_1`, `<stem>_2`, ... using the smallest unreserved suffix.
    pub fn next_numbered(&mut self, stem: &str) -> Result<String, InkScriptSemanticErrorCode> {
        if !is_identifier(stem) {
            return Err(InkScriptSemanticErrorCode::InvalidGeneratedName);
        }
        self.suffixed(stem, 1)
    }

    /// Reserves `name` when free, otherwise allocates `name_2`, `name_3`, ... .
    pub fn reserve_or_rename(&mut self, name: &str) -> Result<String, InkScriptSemanticErrorCode> {
        if !is_identifier(name) {
            return Err(InkScriptSemanticErrorCode::InvalidGeneratedName);
        }
        if self.used.insert(name.to_owned()) {
            return Ok(name.to_owned());
        }
        self.suffixed(name, 2)
    }

    fn suffixed(
        &mut self,
        stem: &str,
        mut suffix: u32,
    ) -> Result<String, InkScriptSemanticErrorCode> {
        loop {
            let candidate = format!("{stem}_{suffix}");
            if !is_identifier(&candidate) {
                return Err(InkScriptSemanticErrorCode::InvalidGeneratedName);
            }
            if self.used.insert(candidate.clone()) {
                return Ok(candidate);
            }
            suffix = suffix
                .checked_add(1)
                .ok_or(InkScriptSemanticErrorCode::InvalidGeneratedName)?;
        }
    }
}
