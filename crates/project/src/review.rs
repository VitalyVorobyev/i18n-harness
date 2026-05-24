//! Review-status persistence: [`ReviewEvent`], [`ReviewRecord`], [`ReviewStore`].
//!
//! Review state is stored in `<state_dir>/review.jsonl` as an append-only log.
//! [`ReviewStore::read_folded`] folds the log last-write-wins per
//! `(catalog, unit_id)` pair. [`ReviewStore::append`] adds a new event.
//!
//! This module mirrors the pattern of `crates/project/src/memory.rs`
//! (`CorrectionStore`). See `docs/m4.1.5-source-hash-review-status-design.md`
//! §5 for the design rationale.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use i18n_harness_core::{ReviewStatus, UnitId};

use crate::error::ProjectError;
use crate::fs::ProjectFs;

/// Folded review-status map: `(manifest-relative catalog path, unit id)` →
/// `ReviewRecord`. Returned by [`ReviewStore::read_folded`].
pub type ReviewMap = BTreeMap<(PathBuf, UnitId), ReviewRecord>;

// ── ReviewEvent ───────────────────────────────────────────────────────────────

/// One review-status change event. Append-only; last write per
/// `(catalog, unit_id)` wins on fold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewEvent {
    /// Schema version for this line shape. Currently `1`.
    pub schema: u32,
    /// RFC 3339 microsecond UTC timestamp of the status change.
    pub ts: String,
    /// Manifest-relative path to the catalog (so events survive a project move
    /// or root rename — same convention as `Correction`).
    pub catalog: PathBuf,
    /// Unit within the catalog.
    pub unit_id: UnitId,
    /// The new status. `None` records a deliberate "clear this unit's record"
    /// event; the fold treats `null` as "remove this unit from the in-memory
    /// map."
    pub status: Option<ReviewStatus>,
    /// The unit's `source_hash` at the moment the reviewer approved it. This
    /// is the value the load-time check compares the current extract's hash
    /// against. Empty string when the unit's adapter produces no hash.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_hash: String,
    /// Free-form reviewer note attached to this status change. Empty when not
    /// provided.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reviewer_note: String,
}

// ── ReviewRecord ──────────────────────────────────────────────────────────────

/// The current (folded) review state of one `(catalog, unit_id)` pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRecord {
    /// The most recently persisted review status.
    pub status: ReviewStatus,
    /// The `source_hash` that was current when the reviewer last acted on this
    /// unit. Compared against the live extract's `source_hash` to detect stale
    /// approvals.
    pub source_hash_at_review: String,
    /// Timestamp of the last status change (RFC 3339).
    pub ts: String,
    /// Reviewer note from the last event that provided one, if any.
    pub reviewer_note: Option<String>,
}

// ── ReviewStore ───────────────────────────────────────────────────────────────

/// Append-only review-status store backed by `<state_dir>/review.jsonl`.
///
/// One line per status change. Cheap to construct — I/O happens only on
/// [`append`](Self::append) and [`read_folded`](Self::read_folded).
pub struct ReviewStore {
    path: PathBuf,
    fs: Arc<dyn ProjectFs>,
}

impl ReviewStore {
    /// Construct a store pointing at `path`. `fs` provides the I/O backing.
    ///
    /// The file need not exist at construction time; it is created lazily on
    /// the first append.
    pub(crate) fn new(path: PathBuf, fs: Arc<dyn ProjectFs>) -> Self {
        Self { path, fs }
    }

    /// Append a single review event to the JSONL file.
    ///
    /// Each call serializes `event` to a single-line JSON string and appends it
    /// followed by a newline byte.
    ///
    /// # Errors
    ///
    /// - [`ProjectError::Io`] if the filesystem append fails.
    pub fn append(&self, event: &ReviewEvent) -> Result<(), ProjectError> {
        let line = serde_json::to_string(event).expect("ReviewEvent serialization is infallible");
        debug_assert!(
            !line.contains('\n'),
            "ReviewEvent JSON must not contain literal newlines"
        );
        let mut bytes = line.into_bytes();
        bytes.push(b'\n');
        self.fs
            .append(&self.path, &bytes)
            .map_err(|source| ProjectError::Io {
                path: self.path.clone(),
                source,
            })
    }

    /// Fold the append-only log into the current `(catalog, unit_id) →
    /// ReviewRecord` view.
    ///
    /// Reads every line; the last well-formed event for a given key wins.
    /// `Option::None` status events remove the key from the result.
    ///
    /// Malformed lines are reported in the warnings vec; the store is
    /// line-recoverable.
    ///
    /// Returns an empty map (and no warnings) if the file does not exist yet.
    pub fn read_folded(&self) -> Result<(ReviewMap, Vec<ProjectError>), ProjectError> {
        if !self.fs.exists(&self.path) {
            return Ok((BTreeMap::new(), Vec::new()));
        }

        let text = self
            .fs
            .read_to_string(&self.path)
            .map_err(|source| ProjectError::Io {
                path: self.path.clone(),
                source,
            })?;

        let mut map: ReviewMap = BTreeMap::new();
        let mut parse_errors: Vec<ProjectError> = Vec::new();

        for (idx, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<ReviewEvent>(line) {
                Ok(event) => {
                    let key = (event.catalog.clone(), event.unit_id.clone());
                    match event.status {
                        Some(status) => {
                            map.insert(
                                key,
                                ReviewRecord {
                                    status,
                                    source_hash_at_review: event.source_hash,
                                    ts: event.ts,
                                    reviewer_note: if event.reviewer_note.is_empty() {
                                        None
                                    } else {
                                        Some(event.reviewer_note)
                                    },
                                },
                            );
                        }
                        None => {
                            // Deliberate "clear" event.
                            map.remove(&key);
                        }
                    }
                }
                Err(source) => parse_errors.push(ProjectError::ReviewEventParse {
                    path: self.path.clone(),
                    line_no: idx + 1,
                    source,
                }),
            }
        }

        Ok((map, parse_errors))
    }
}
