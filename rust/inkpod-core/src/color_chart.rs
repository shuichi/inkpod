//! Document-owned Color chart and immutable generated-result previews.

use crate::animation::flatten_document;
use crate::{
    Core, CoreError, DispatchOutcome, MAX_APPLICATION_COLORS, MAX_COLOR_CHART_NAME_BYTES,
    PixelValue, PrimitiveRequest,
};
use std::collections::{BTreeMap, BTreeSet};

/// Number of entries presented on one Color chart page.
pub const COLOR_CHART_PAGE_SIZE: u32 = 20;

/// One exact-depth, named Color chart entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColorChartEntry {
    /// Straight-alpha RGBA8 or RGBA16 color.
    pub color: PixelValue,
    /// Non-empty UTF-8 display name, bounded by [`MAX_COLOR_CHART_NAME_BYTES`].
    pub name: String,
}

/// Document-owned ordered Color chart content and its edit lock.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ColorChart {
    entries: Vec<ColorChartEntry>,
    locked: bool,
}

impl ColorChart {
    pub(crate) fn validated(
        entries: Vec<ColorChartEntry>,
        locked: bool,
    ) -> Result<Self, CoreError> {
        validate_entries(&entries)?;
        Ok(Self { entries, locked })
    }

    /// Borrows ordered entries for the lifetime of this chart borrow.
    #[must_use]
    pub fn entries(&self) -> &[ColorChartEntry] {
        &self.entries
    }

    /// Reports whether document-changing Color chart commands are locked.
    #[must_use]
    pub const fn locked(&self) -> bool {
        self.locked
    }
}

/// One preview candidate with deterministic visible-composite frequency.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColorChartPreviewEntry {
    /// Quantized straight-alpha RGBA8 candidate.
    pub color: PixelValue,
    /// Name that Apply will commit.
    pub name: String,
    /// Number of visible composite pixels mapped to this candidate.
    pub frequency: u64,
}

/// Bounded comparison between the current chart and generated candidates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorChartPreviewSummary {
    /// Number of distinct visible candidates, capped at 4097 when the bounded
    /// representative set overflows.
    pub source_unique_colors: u64,
    /// Existing exact-depth colors retained by the generated result.
    pub retained_colors: u32,
    /// Generated colors absent from the current chart.
    pub added_colors: u32,
    /// Existing colors absent from the generated result.
    pub removed_colors: u32,
    /// Whether the generated result exceeds the requested maximum or the
    /// application-wide representative bound.
    pub exceeds_maximum: bool,
}

/// Immutable, revision-bound generated-result comparison preview.
///
/// Dropping this value is Cancel and never changes Core state. Apply succeeds
/// only while its base document revision is current and the chart is unlocked.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColorChartPreview {
    document_uuid: u128,
    base_document_revision: u64,
    entries: Vec<ColorChartPreviewEntry>,
    chart_entries: Vec<ColorChartEntry>,
    summary: ColorChartPreviewSummary,
}

impl ColorChartPreview {
    /// Returns the document revision whose visible composite was sampled.
    #[must_use]
    pub const fn base_document_revision(&self) -> u64 {
        self.base_document_revision
    }

    /// Borrows bounded preview candidates and their frequencies.
    #[must_use]
    pub fn entries(&self) -> &[ColorChartPreviewEntry] {
        &self.entries
    }

    /// Borrows the exact ordered entries Apply will commit.
    #[must_use]
    pub fn chart_entries(&self) -> &[ColorChartEntry] {
        &self.chart_entries
    }

    /// Returns the deterministic comparison summary.
    #[must_use]
    pub const fn summary(&self) -> ColorChartPreviewSummary {
        self.summary
    }
}

impl Core {
    /// Borrows the independent document Color chart.
    pub fn color_chart(&self) -> Result<&ColorChart, CoreError> {
        Ok(&self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .color_chart)
    }

    /// Replaces all Color chart entries and lock state as one canonical edit.
    ///
    /// Identical input is a no-op. Invalid names, colors, or bounds fail without
    /// advancing document revision, history, journal, dirty state, or IDs.
    pub fn replace_color_chart(
        &mut self,
        entries: &[ColorChartEntry],
        locked: bool,
    ) -> Result<DispatchOutcome, CoreError> {
        let expected_revision = self.document_revision.get();
        self.execute_primitive(PrimitiveRequest::ReplaceColorChart {
            expected_revision,
            entries: entries.to_vec(),
            locked,
        })
        .map(|outcome| outcome.dispatch())
    }

    /// Generates a bounded immutable comparison from the current visible composite.
    ///
    /// Each call starts from the same committed base document rather than a prior
    /// preview. The query never changes document/editor revisions, history, dirty
    /// state, savepoints, IDs, or caches visible to callers.
    pub fn preview_color_chart_generation(
        &self,
        maximum_colors: usize,
        quantization_bits: u8,
    ) -> Result<ColorChartPreview, CoreError> {
        self.preview_color_chart_generation_with_cancel(
            maximum_colors,
            quantization_bits,
            |_, _| true,
        )
    }

    /// Generates a comparison while reporting row progress and observing cancellation.
    ///
    /// `continue_work` receives completed and total source rows. Returning false
    /// yields [`CoreError::Cancelled`] and publishes no state. The callback is
    /// never retained or invoked while mutating live document state.
    pub fn preview_color_chart_generation_with_cancel<F>(
        &self,
        maximum_colors: usize,
        quantization_bits: u8,
        mut continue_work: F,
    ) -> Result<ColorChartPreview, CoreError>
    where
        F: FnMut(u64, u64) -> bool,
    {
        if maximum_colors == 0 || maximum_colors > MAX_APPLICATION_COLORS {
            return Err(CoreError::InvalidArgument(
                "generated Color chart maximum is invalid",
            ));
        }
        if quantization_bits > 7 {
            return Err(CoreError::InvalidArgument(
                "Color chart quantization must retain at least one bit",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let flattened =
            flatten_document(document, &self.assets, self.document_revision.get().max(1))?;
        let mask = u8::MAX << quantization_bits;
        let mut frequencies = BTreeMap::<[u8; 4], u64>::new();
        let mut representative_overflow = false;
        for y in 0..flattened.height() {
            if !continue_work(u64::from(y), u64::from(flattened.height())) {
                return Err(CoreError::Cancelled);
            }
            for x in 0..flattened.width() {
                let PixelValue::Rgba(mut rgba) = flattened.pixel(x, y)? else {
                    return Err(CoreError::InvalidState(
                        "flattened Color chart source is not RGBA8",
                    ));
                };
                if rgba[3] == 0 {
                    continue;
                }
                for channel in &mut rgba {
                    *channel &= mask;
                }
                if let Some(frequency) = frequencies.get_mut(&rgba) {
                    *frequency = frequency
                        .checked_add(1)
                        .ok_or(CoreError::InvalidState("Color chart frequency overflows"))?;
                    continue;
                }
                if frequencies.len() < MAX_APPLICATION_COLORS {
                    frequencies.insert(rgba, 1);
                } else {
                    representative_overflow = true;
                    let largest = *frequencies
                        .last_key_value()
                        .ok_or(CoreError::InvalidState(
                            "Color chart representative set is empty",
                        ))?
                        .0;
                    if rgba < largest {
                        frequencies.remove(&largest);
                        frequencies.insert(rgba, 1);
                    }
                }
            }
        }
        if !continue_work(u64::from(flattened.height()), u64::from(flattened.height())) {
            return Err(CoreError::Cancelled);
        }

        let current = &document.color_chart;
        let candidate_colors = frequencies
            .keys()
            .copied()
            .map(PixelValue::Rgba)
            .collect::<Vec<_>>();
        let candidate_keys = candidate_colors
            .iter()
            .copied()
            .map(color_key)
            .collect::<BTreeSet<_>>();
        let current_keys = current
            .entries()
            .iter()
            .map(|entry| color_key(entry.color))
            .collect::<BTreeSet<_>>();
        let retained = candidate_keys.intersection(&current_keys).count();
        let added = candidate_keys.difference(&current_keys).count();
        let removed = current_keys.difference(&candidate_keys).count();
        let mut chart_entries = Vec::with_capacity(candidate_colors.len());
        let mut entries = Vec::with_capacity(candidate_colors.len());
        for (index, color) in candidate_colors.into_iter().enumerate() {
            let name = current
                .entries()
                .iter()
                .find(|entry| entry.color == color)
                .map(|entry| entry.name.clone())
                .unwrap_or_else(|| format!("Color {}", index + 1));
            let PixelValue::Rgba(rgba) = color else {
                unreachable!("generated colors are RGBA8")
            };
            let frequency = frequencies[&rgba];
            chart_entries.push(ColorChartEntry {
                color,
                name: name.clone(),
            });
            entries.push(ColorChartPreviewEntry {
                color,
                name,
                frequency,
            });
        }
        let unique = frequencies.len() as u64 + u64::from(representative_overflow);
        Ok(ColorChartPreview {
            document_uuid: document.uuid,
            base_document_revision: self.document_revision.get(),
            entries,
            chart_entries,
            summary: ColorChartPreviewSummary {
                source_unique_colors: unique,
                retained_colors: retained as u32,
                added_colors: added as u32,
                removed_colors: removed as u32,
                exceeds_maximum: representative_overflow || frequencies.len() > maximum_colors,
            },
        })
    }

    /// Applies one current, non-overflowing generated preview as one canonical edit.
    pub fn apply_color_chart_preview(
        &mut self,
        preview: &ColorChartPreview,
    ) -> Result<DispatchOutcome, CoreError> {
        if self.document.as_ref().ok_or(CoreError::NoDocument)?.uuid != preview.document_uuid {
            return Err(CoreError::InvalidState(
                "color chart preview belongs to another document",
            ));
        }
        if preview.base_document_revision != self.document_revision.get() {
            return Err(CoreError::InvalidState(
                "color chart preview revision is stale",
            ));
        }
        if preview.summary.exceeds_maximum {
            return Err(CoreError::InvalidState(
                "color chart preview exceeds the configured maximum",
            ));
        }
        if self.color_chart()?.locked() {
            return Err(CoreError::InvalidState("color chart is locked"));
        }
        self.replace_color_chart(&preview.chart_entries, false)
    }
}

pub(crate) fn validate_entries(entries: &[ColorChartEntry]) -> Result<(), CoreError> {
    if entries.len() > MAX_APPLICATION_COLORS {
        return Err(CoreError::InvalidArgument(
            "Color chart entry count exceeds the supported maximum",
        ));
    }
    for entry in entries {
        if entry.color.rgba16().is_none() {
            return Err(CoreError::InvalidArgument(
                "Color chart entries must be RGBA8 or RGBA16",
            ));
        }
        if entry.name.is_empty() || entry.name.len() > MAX_COLOR_CHART_NAME_BYTES {
            return Err(CoreError::InvalidArgument(
                "Color chart entry name is outside the supported bounds",
            ));
        }
    }
    Ok(())
}

pub(crate) fn color_key(color: PixelValue) -> Vec<u8> {
    match color {
        PixelValue::Rgba(channels) => {
            let mut key = vec![8];
            key.extend_from_slice(&channels);
            key
        }
        PixelValue::Rgba16(channels) => {
            let mut key = vec![16];
            key.extend(channels.into_iter().flat_map(u16::to_le_bytes));
            key
        }
        PixelValue::Binary(_) | PixelValue::Grayscale8(_) | PixelValue::Grayscale16(_) => {
            Vec::new()
        }
    }
}
