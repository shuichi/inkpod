use crate::*;

impl Core {
    /// Returns legacy single-stroke bindings in command-ID order.
    pub fn shortcut_bindings(&self) -> Vec<ShortcutBinding> {
        self.shortcuts
            .iter()
            .filter_map(|(command_id, strokes)| {
                (strokes.len() == 1).then_some(ShortcutBinding {
                    command_id: *command_id,
                    stroke: strokes[0],
                })
            })
            .collect()
    }

    /// Assigns a validated single-stroke shortcut to one command.
    ///
    /// Conflicting bindings are removed. Shortcut state is independent of document
    /// revision, history, dirty state, and native document persistence.
    pub fn rebind_shortcut(&mut self, binding: ShortcutBinding) -> Result<(), CoreError> {
        self.rebind_shortcut_sequence(ShortcutSequenceBinding {
            command_id: binding.command_id,
            strokes: vec![binding.stroke],
        })
    }

    /// Resolves one validated stroke to an exact command, if present.
    pub fn resolve_shortcut(&self, stroke: ShortcutStroke) -> Result<Option<u32>, CoreError> {
        match self.resolve_shortcut_sequence(&[stroke])? {
            ShortcutSequenceMatch::Exact(command_id) => Ok(Some(command_id)),
            ShortcutSequenceMatch::None | ShortcutSequenceMatch::Prefix => Ok(None),
        }
    }

    /// Restores current shortcuts from the configured defaults.
    pub fn reset_shortcuts(&mut self) {
        self.shortcuts.clone_from(&self.shortcut_defaults);
    }

    /// Returns owned shortcut sequences in command-ID order.
    pub fn shortcut_sequences(&self) -> Vec<ShortcutSequenceBinding> {
        self.shortcuts
            .iter()
            .map(|(command_id, strokes)| ShortcutSequenceBinding {
                command_id: *command_id,
                strokes: strokes.clone(),
            })
            .collect()
    }

    /// Atomically replaces both shortcut defaults and current bindings.
    ///
    /// Sequences must be non-empty, bounded, unique, and prefix-free.
    pub fn set_shortcut_defaults(
        &mut self,
        bindings: &[ShortcutSequenceBinding],
    ) -> Result<(), CoreError> {
        let replacement = validate_shortcut_sequences(bindings)?;
        self.shortcut_defaults = replacement.clone();
        self.shortcuts = replacement;
        Ok(())
    }

    /// Atomically replaces current shortcut sequences while retaining defaults.
    pub fn replace_shortcut_sequences(
        &mut self,
        bindings: &[ShortcutSequenceBinding],
    ) -> Result<(), CoreError> {
        self.shortcuts = validate_shortcut_sequences(bindings)?;
        Ok(())
    }

    /// Assigns one prefix-free shortcut sequence to a command.
    ///
    /// Existing conflicting sequences are removed after the new binding validates.
    pub fn rebind_shortcut_sequence(
        &mut self,
        binding: ShortcutSequenceBinding,
    ) -> Result<(), CoreError> {
        validate_shortcut_sequence(&binding)?;
        if self.shortcuts.len() >= MAX_SHORTCUTS
            && !self.shortcuts.contains_key(&binding.command_id)
        {
            return Err(CoreError::InvalidState("shortcut limit reached"));
        }
        self.shortcuts.retain(|command, candidate| {
            *command == binding.command_id
                || !shortcut_sequences_conflict(candidate, &binding.strokes)
        });
        self.shortcuts.insert(binding.command_id, binding.strokes);
        Ok(())
    }

    /// Classifies validated entered strokes as no match, prefix, or exact match.
    pub fn resolve_shortcut_sequence(
        &self,
        strokes: &[ShortcutStroke],
    ) -> Result<ShortcutSequenceMatch, CoreError> {
        validate_shortcut_strokes(strokes)?;
        if let Some(command_id) = self.shortcuts.iter().find_map(|(command_id, candidate)| {
            (candidate.as_slice() == strokes).then_some(*command_id)
        }) {
            return Ok(ShortcutSequenceMatch::Exact(command_id));
        }
        if self
            .shortcuts
            .values()
            .any(|candidate| candidate.starts_with(strokes))
        {
            Ok(ShortcutSequenceMatch::Prefix)
        } else {
            Ok(ShortcutSequenceMatch::None)
        }
    }
}

fn validate_shortcut_sequences(
    bindings: &[ShortcutSequenceBinding],
) -> Result<BTreeMap<u32, Vec<ShortcutStroke>>, CoreError> {
    if bindings.len() > MAX_SHORTCUTS {
        return Err(CoreError::InvalidArgument("too many shortcut bindings"));
    }
    let mut replacement = BTreeMap::new();
    for binding in bindings {
        validate_shortcut_sequence(binding)?;
        if replacement
            .insert(binding.command_id, binding.strokes.clone())
            .is_some()
        {
            return Err(CoreError::InvalidArgument("duplicate shortcut command"));
        }
    }
    let sequences = replacement.values().collect::<Vec<_>>();
    for (index, sequence) in sequences.iter().enumerate() {
        if sequences[index + 1..]
            .iter()
            .any(|candidate| shortcut_sequences_conflict(sequence, candidate))
        {
            return Err(CoreError::InvalidArgument("shortcut sequences conflict"));
        }
    }
    Ok(replacement)
}

fn validate_shortcut_sequence(binding: &ShortcutSequenceBinding) -> Result<(), CoreError> {
    if binding.command_id == 0 {
        return Err(CoreError::InvalidArgument("shortcut command is invalid"));
    }
    validate_shortcut_strokes(&binding.strokes)
}

fn validate_shortcut_strokes(strokes: &[ShortcutStroke]) -> Result<(), CoreError> {
    if strokes.is_empty() || strokes.len() > MAX_SHORTCUT_STROKES {
        return Err(CoreError::InvalidArgument(
            "shortcut stroke count is invalid",
        ));
    }
    if strokes
        .iter()
        .any(|stroke| {
            matches!(stroke.key, ShortcutKey::Named(ShortcutNamedKey::Function(value)) if !(1..=24).contains(&value))
                || stroke.modifiers & !SHORTCUT_MODIFIER_MASK != 0
        })
    {
        return Err(CoreError::InvalidArgument("shortcut stroke is invalid"));
    }
    Ok(())
}

fn shortcut_sequences_conflict(left: &[ShortcutStroke], right: &[ShortcutStroke]) -> bool {
    left.starts_with(right) || right.starts_with(left)
}
pub(crate) fn default_shortcuts() -> BTreeMap<u32, Vec<ShortcutStroke>> {
    [
        ShortcutBinding {
            command_id: 1,
            stroke: ShortcutStroke {
                key: ShortcutKey::UnicodeScalar('Z'),
                modifiers: SHORTCUT_MODIFIER_PRIMARY,
            },
        },
        ShortcutBinding {
            command_id: 2,
            stroke: ShortcutStroke {
                key: ShortcutKey::UnicodeScalar('Y'),
                modifiers: SHORTCUT_MODIFIER_PRIMARY,
            },
        },
        ShortcutBinding {
            command_id: 3,
            stroke: ShortcutStroke {
                key: ShortcutKey::UnicodeScalar('C'),
                modifiers: SHORTCUT_MODIFIER_PRIMARY,
            },
        },
        ShortcutBinding {
            command_id: 4,
            stroke: ShortcutStroke {
                key: ShortcutKey::UnicodeScalar('V'),
                modifiers: SHORTCUT_MODIFIER_PRIMARY,
            },
        },
    ]
    .into_iter()
    .map(|binding| (binding.command_id, vec![binding.stroke]))
    .collect()
}
