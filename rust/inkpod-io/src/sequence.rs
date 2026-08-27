use crate::{IoError, IoManager, IoResult, JobContext, JobPhase};
use inkpod_format::CommonRasterFormat;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const MAXIMUM_SEQUENCE_IMAGES: usize = 1_000;
const MAXIMUM_DIRECTORY_ENTRIES: usize = 1_000_000;

/// Deterministically selected natural-order neighbors around the seed, which is
/// always present. Numeric width and raster extension are not part of the pattern.
/// `truncated` makes the 1000-item cap visible.
#[derive(Clone, Debug)]
pub struct SequenceDiscovery {
    pub paths: Vec<PathBuf>,
    pub seed_index: usize,
    pub truncated: bool,
}

#[derive(Clone, Eq, PartialEq)]
struct SequenceKey {
    digits: String,
    name: String,
    path: PathBuf,
}

impl Ord for SequenceKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.digits
            .len()
            .cmp(&other.digits.len())
            .then_with(|| self.digits.cmp(&other.digits))
            .then_with(|| self.name.cmp(&other.name))
            .then_with(|| self.path.cmp(&other.path))
    }
}

impl PartialOrd for SequenceKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct Pattern<'a> {
    prefix: &'a str,
    digits: &'a str,
    suffix: &'a str,
}

fn pattern(stem: &str) -> Option<Pattern<'_>> {
    let bytes = stem.as_bytes();
    let last = bytes.iter().rposition(u8::is_ascii_digit)?;
    let mut first = last;
    while first > 0 && bytes[first - 1].is_ascii_digit() {
        first -= 1;
    }
    Some(Pattern {
        prefix: &stem[..first],
        digits: &stem[first..=last],
        suffix: &stem[last + 1..],
    })
}

impl IoManager {
    /// Synchronously enumerates one directory with bounded retained memory. Call
    /// this from an I/O job, never from a UI event handler. PNG/TIFF/TGA/BMP may be
    /// mixed. Prefix/suffix comparison is case-insensitive, the last ASCII digit
    /// run is numeric, and digit width does not define a separate sequence.
    pub fn discover_sequence(
        &self,
        seed: &Path,
        context: &JobContext,
    ) -> IoResult<SequenceDiscovery> {
        self.check_running(context)?;
        context.set_phase(JobPhase::Enumerating);
        let seed = std::fs::canonicalize(seed)?;
        if !seed.is_file() {
            return Err(IoError::InvalidInput("sequence seed is not a file"));
        }
        let stem = seed
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or(IoError::InvalidInput("sequence file name is not UTF-8"))?;
        seed.extension()
            .and_then(|extension| extension.to_str())
            .and_then(CommonRasterFormat::from_extension)
            .ok_or(IoError::InvalidInput(
                "sequence image format is unsupported",
            ))?;
        let Some(seed_pattern) = pattern(stem) else {
            return Ok(SequenceDiscovery {
                paths: vec![seed],
                seed_index: 0,
                truncated: false,
            });
        };
        let parent = seed
            .parent()
            .ok_or(IoError::InvalidInput("sequence directory is missing"))?;
        let mut before = BTreeSet::new();
        let mut after = BTreeSet::new();
        let seed_digits = seed_pattern.digits.trim_start_matches('0');
        let seed_key = SequenceKey {
            digits: if seed_digits.is_empty() {
                "0".to_owned()
            } else {
                seed_digits.to_owned()
            },
            name: seed
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_lowercase(),
            path: seed.clone(),
        };
        let mut seed_seen = false;
        let mut matched = 0_u64;
        for (index, entry) in std::fs::read_dir(parent)?.enumerate() {
            context.check_cancelled()?;
            if index >= MAXIMUM_DIRECTORY_ENTRIES {
                return Err(IoError::LimitExceeded(
                    "sequence directory enumeration exceeds its entry limit",
                ));
            }
            let entry = entry?;
            let path = entry.path();
            if path
                .extension()
                .and_then(|extension| extension.to_str())
                .and_then(CommonRasterFormat::from_extension)
                .is_none()
            {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(candidate) = pattern(name) else {
                continue;
            };
            if candidate.prefix.to_lowercase() != seed_pattern.prefix.to_lowercase()
                || candidate.suffix.to_lowercase() != seed_pattern.suffix.to_lowercase()
                || !entry.file_type()?.is_file()
            {
                continue;
            }
            let trimmed = candidate.digits.trim_start_matches('0');
            let key = SequenceKey {
                digits: if trimmed.is_empty() {
                    "0".to_owned()
                } else {
                    trimmed.to_owned()
                },
                name: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("")
                    .to_lowercase(),
                path: path.clone(),
            };
            matched += 1;
            match key.cmp(&seed_key) {
                Ordering::Equal => seed_seen = true,
                Ordering::Less => {
                    before.insert(key);
                    if before.len() >= MAXIMUM_SEQUENCE_IMAGES {
                        before.pop_first();
                    }
                }
                Ordering::Greater => {
                    after.insert(key);
                    if after.len() >= MAXIMUM_SEQUENCE_IMAGES {
                        after.pop_last();
                    }
                }
            }
        }
        if !seed_seen {
            return Err(IoError::ChangedDuringRead);
        }
        let before_initial = before.len().min((MAXIMUM_SEQUENCE_IMAGES - 1) / 2);
        let after_count = after
            .len()
            .min(MAXIMUM_SEQUENCE_IMAGES - 1 - before_initial);
        let before_count = before.len().min(MAXIMUM_SEQUENCE_IMAGES - 1 - after_count);
        let before_skip = before.len() - before_count;
        let paths: Vec<_> = before
            .into_iter()
            .skip(before_skip)
            .chain(std::iter::once(seed_key))
            .chain(after.into_iter().take(after_count))
            .map(|entry| entry.path)
            .collect();
        let seed_index = paths
            .iter()
            .position(|path| path == &seed)
            .ok_or(IoError::ChangedDuringRead)?;
        context.update(|progress| progress.discovered = paths.len() as u64);
        Ok(SequenceDiscovery {
            paths,
            seed_index,
            truncated: matched > MAXIMUM_SEQUENCE_IMAGES as u64,
        })
    }

    /// Lists regular files non-recursively, without retaining more than the
    /// explicit bound. Unsupported file formats are left to the caller's policy.
    pub fn list_files(
        &self,
        directory: &Path,
        maximum: usize,
        context: &JobContext,
    ) -> IoResult<Vec<PathBuf>> {
        self.check_running(context)?;
        if maximum == 0 || maximum > 1_000_000 {
            return Err(IoError::InvalidInput("directory file bound is invalid"));
        }
        let mut paths = Vec::new();
        for (index, entry) in std::fs::read_dir(directory)?.enumerate() {
            context.check_cancelled()?;
            if index >= MAXIMUM_DIRECTORY_ENTRIES {
                return Err(IoError::LimitExceeded(
                    "directory enumeration exceeds its entry limit",
                ));
            }
            let entry = entry?;
            if entry.file_type()?.is_file() {
                if paths.len() >= maximum {
                    return Err(IoError::LimitExceeded(
                        "directory file count exceeds its bound",
                    ));
                }
                paths.push(entry.path());
            }
        }
        paths.sort();
        Ok(paths)
    }
}
