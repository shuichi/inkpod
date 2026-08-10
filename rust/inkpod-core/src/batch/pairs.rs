use super::*;

#[derive(Clone)]
struct CandidateWork {
    old: PixelValue,
    new: PixelValue,
    pixel_count: u64,
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
}

impl Core {
    /// Compares two exact immutable sequence sources and extracts replacement candidates.
    ///
    /// Sources must still exist under the supplied UUID/generation identities and
    /// must have identical dimensions and native pixel formats. Comparison is at
    /// the same document coordinates and includes straight alpha. This read-only
    /// query does not change document, sequence, history, revisions, or dirty state.
    pub fn extract_batch_color_pairs(
        &self,
        old_identity: SequenceSourceIdentity,
        new_identity: SequenceSourceIdentity,
    ) -> Result<BatchPairExtraction, CoreError> {
        if old_identity.document_uuid == 0
            || old_identity.source_generation == 0
            || new_identity.document_uuid == 0
            || new_identity.source_generation == 0
            || old_identity == new_identity
        {
            return Err(CoreError::InvalidArgument(
                "batch pair source identities must be distinct and nonzero",
            ));
        }
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let find = |identity: SequenceSourceIdentity| {
            sequence.cells.iter().find(|cell| {
                cell.document_uuid == identity.document_uuid
                    && cell.source_generation == identity.source_generation
            })
        };
        let old = find(old_identity).ok_or(CoreError::InvalidArgument(
            "old batch pair source identity is stale or missing",
        ))?;
        let new = find(new_identity).ok_or(CoreError::InvalidArgument(
            "new batch pair source identity is stale or missing",
        ))?;
        if old.raster.width() != new.raster.width()
            || old.raster.height() != new.raster.height()
            || old.raster.format() != new.raster.format()
        {
            return Err(CoreError::InvalidArgument(
                "batch pair sources must have identical dimensions and native pixel format",
            ));
        }
        let width = old.raster.width();
        let height = old.raster.height();
        let mut unchanged_pixel_count = 0_u64;
        let mut work = Vec::<CandidateWork>::new();
        for y in 0..height {
            for x in 0..width {
                let old_value = old.raster.pixel(x, y)?;
                let new_value = new.raster.pixel(x, y)?;
                if old_value == new_value {
                    unchanged_pixel_count =
                        unchanged_pixel_count
                            .checked_add(1)
                            .ok_or(CoreError::InvalidArgument(
                                "batch pair unchanged count overflows",
                            ))?;
                    continue;
                }
                if let Some(candidate) = work
                    .iter_mut()
                    .find(|candidate| candidate.old == old_value && candidate.new == new_value)
                {
                    candidate.pixel_count =
                        candidate
                            .pixel_count
                            .checked_add(1)
                            .ok_or(CoreError::InvalidArgument(
                                "batch pair candidate count overflows",
                            ))?;
                    candidate.min_x = candidate.min_x.min(x);
                    candidate.min_y = candidate.min_y.min(y);
                    candidate.max_x = candidate.max_x.max(x);
                    candidate.max_y = candidate.max_y.max(y);
                } else {
                    if work.len() >= MAX_BATCH_COLOR_PAIRS {
                        return Err(CoreError::InvalidArgument(
                            "batch pair candidate count exceeds bounds",
                        ));
                    }
                    work.push(CandidateWork {
                        old: old_value,
                        new: new_value,
                        pixel_count: 1,
                        min_x: x,
                        min_y: y,
                        max_x: x,
                        max_y: y,
                    });
                }
            }
        }

        let mut group_starts = Vec::<PixelValue>::new();
        for candidate in &work {
            if !group_starts.contains(&candidate.old) {
                group_starts.push(candidate.old);
            }
        }
        let ambiguity_count = u32::try_from(
            group_starts
                .iter()
                .filter(|old| {
                    work.iter()
                        .filter(|candidate| candidate.old == **old)
                        .count()
                        > 1
                })
                .count(),
        )
        .map_err(|_| CoreError::InvalidArgument("batch pair ambiguity count overflows"))?;
        let mut candidates = Vec::with_capacity(work.len());
        for old_value in group_starts {
            let ambiguous = work
                .iter()
                .filter(|candidate| candidate.old == old_value)
                .count()
                > 1;
            let mut group: Vec<_> = work
                .iter()
                .filter(|candidate| candidate.old == old_value)
                .cloned()
                .collect();
            group.sort_by(|left, right| {
                right
                    .pixel_count
                    .cmp(&left.pixel_count)
                    .then_with(|| native_pixel_bytes(left.new).cmp(&native_pixel_bytes(right.new)))
            });
            for candidate in group {
                candidates.push(BatchPairCandidate {
                    old: candidate.old,
                    new: candidate.new,
                    pixel_count: candidate.pixel_count,
                    affected_bounds: RectI32 {
                        x: i32::try_from(candidate.min_x).map_err(|_| {
                            CoreError::InvalidArgument("batch pair bounds exceed signed range")
                        })?,
                        y: i32::try_from(candidate.min_y).map_err(|_| {
                            CoreError::InvalidArgument("batch pair bounds exceed signed range")
                        })?,
                        width: i32::try_from(candidate.max_x - candidate.min_x + 1).map_err(
                            |_| CoreError::InvalidArgument("batch pair bounds exceed signed range"),
                        )?,
                        height: i32::try_from(candidate.max_y - candidate.min_y + 1).map_err(
                            |_| CoreError::InvalidArgument("batch pair bounds exceed signed range"),
                        )?,
                    },
                    ambiguous,
                });
            }
        }
        Ok(BatchPairExtraction {
            width,
            height,
            pixel_format: old.raster.format(),
            unchanged_pixel_count,
            ambiguity_count,
            candidates,
        })
    }
}

pub(super) fn resolve_pairs(
    extraction: &BatchPairExtraction,
    resolutions: &[BatchPairResolution],
) -> Result<Vec<BatchColorPair>, CoreError> {
    let mut ambiguous_old = Vec::new();
    for candidate in extraction
        .candidates
        .iter()
        .filter(|candidate| candidate.ambiguous)
    {
        if !ambiguous_old.contains(&candidate.old) {
            ambiguous_old.push(candidate.old);
        }
    }
    if resolutions.len() != ambiguous_old.len() {
        return Err(CoreError::InvalidArgument(
            "every ambiguous batch pair group requires exactly one resolution",
        ));
    }
    for (index, resolution) in resolutions.iter().enumerate() {
        if resolutions[..index]
            .iter()
            .any(|previous| previous.old == resolution.old)
            || !ambiguous_old.contains(&resolution.old)
        {
            return Err(CoreError::InvalidArgument(
                "batch pair resolution is duplicate or unknown",
            ));
        }
        if resolution.selected_new.is_some_and(|selected| {
            !extraction.candidates.iter().any(|candidate| {
                candidate.old == resolution.old && candidate.new == selected && candidate.ambiguous
            })
        }) {
            return Err(CoreError::InvalidArgument(
                "batch pair resolution does not select a candidate",
            ));
        }
    }

    let mut pairs = Vec::new();
    for candidate in &extraction.candidates {
        if candidate.ambiguous {
            let resolution = resolutions
                .iter()
                .find(|resolution| resolution.old == candidate.old)
                .ok_or(CoreError::InvalidArgument(
                    "batch pair ambiguity remains unresolved",
                ))?;
            if resolution.selected_new == Some(candidate.new) {
                pairs.push(BatchColorPair {
                    enabled: true,
                    old: candidate.old,
                    new: candidate.new,
                });
            }
        } else {
            pairs.push(BatchColorPair {
                enabled: true,
                old: candidate.old,
                new: candidate.new,
            });
        }
    }
    Ok(pairs)
}

fn native_pixel_bytes(value: PixelValue) -> Vec<u8> {
    match value {
        PixelValue::Binary(value) | PixelValue::Grayscale8(value) => vec![value],
        PixelValue::Grayscale16(value) => value.to_le_bytes().to_vec(),
        PixelValue::Rgba(value) => value.to_vec(),
        PixelValue::Rgba16(value) => value
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    }
}
