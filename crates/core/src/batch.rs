//! [`Batch`] — a contiguous slice of [`crate::Unit`]s with stable ordering
//! and a resume key.
//!
//! Batching is the *unit of work* the backend sees: send `N` units, get
//! `N` units back, persist progress, repeat. This module names the contract
//! but does **not** enforce a batch size — that policy lives in the caller
//! (CLI / backend driver) so we can experiment with `num_ctx`-aware sizing
//! per backend without churning this type.
//!
//! Stable ordering = `(file, unit-id)` lexicographic. The resume key
//! [`BatchKey`] pairs the catalog file's content hash with the batch index;
//! a re-run with the same catalog can pick up at the next batch without
//! re-processing earlier work. The hash itself is computed by the caller and
//! passed in (this crate has no SHA dependency).

use serde::{Deserialize, Serialize};

use crate::unit::Unit;

/// Default batch size used by the CLI when no other policy applies.
///
/// Chosen for Gemma 4 E4B at 128K context with glossary injection: 32 units
/// of ~20 tokens each = ~640 tokens, plus prompt + glossary block = comfortable
/// margin. Override via CLI flag once the M2 quality metric tells us what
/// works.
pub const DEFAULT_BATCH_SIZE: usize = 32;

/// Resume key for a batch: enough to identify the same `(file, batch index)`
/// across runs, refusing to confuse two runs of two different versions of the
/// same file.
///
/// The `file_content_hash` is computed and supplied by the caller. By
/// convention we use SHA-256 hex (lower-case); this struct does not enforce
/// it, so a future change to BLAKE3 (or anything else) is just a caller-side
/// update.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BatchKey {
    /// Hex-encoded content hash of the catalog file the batch came from.
    pub file_content_hash: String,
    /// Zero-based batch index within that file.
    pub batch_index: u32,
}

impl BatchKey {
    /// Construct a key.
    pub fn new(file_content_hash: impl Into<String>, batch_index: u32) -> Self {
        Self {
            file_content_hash: file_content_hash.into(),
            batch_index,
        }
    }
}

/// A contiguous slice of units with stable ordering and a resume key.
///
/// # Invariants
///
/// - [`Self::units`] is in `(file, unit-id)` lexicographic order. The
///   constructor [`Batch::new`] enforces this by sorting; callers that
///   already have sorted input pay one extra `is_sorted` check, no
///   reallocation.
/// - [`Self::key`] identifies this batch within a (file, run) pair.
///
/// # What `Batch` does NOT guarantee
///
/// - Any particular size. Callers slice their universe into batches of
///   whatever size their backend prefers; the constant [`DEFAULT_BATCH_SIZE`]
///   is a default, not a contract.
/// - That all units come from the same catalog file. In practice they do
///   (the resume key references a single file hash), but the type does not
///   enforce it — that lets the M4 export-batch path pack units from
///   multiple files into one off-line batch without redesigning this type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Batch {
    /// Resume key for this batch. See [`BatchKey`].
    pub key: BatchKey,
    /// Units in stable order. See struct-level invariants.
    pub units: Vec<Unit>,
}

impl Batch {
    /// Construct a batch, sorting units into stable `(file, unit-id)` order.
    ///
    /// Sorting is unconditional but cheap (`O(n log n)`) and idempotent;
    /// callers that already have sorted input get an effectively-free check.
    pub fn new(key: BatchKey, mut units: Vec<Unit>) -> Self {
        units.sort_by(|a, b| {
            a.provenance
                .file
                .cmp(&b.provenance.file)
                .then_with(|| a.id.cmp(&b.id))
        });
        Self { key, units }
    }

    /// Number of units in the batch.
    pub fn len(&self) -> usize {
        self.units.len()
    }

    /// True if the batch is empty.
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::{Provenance, Unit};

    fn unit_with(id: &str, file: &str) -> Unit {
        let mut u = Unit::untranslated_singular(id, "x");
        u.provenance = Provenance {
            file: file.to_owned(),
            line: None,
            byte_offset: None,
        };
        u
    }

    #[test]
    fn new_sorts_by_file_then_id() {
        let units = vec![
            unit_with("b", "b.ts"),
            unit_with("a", "b.ts"),
            unit_with("a", "a.ts"),
            unit_with("z", "a.ts"),
        ];
        let b = Batch::new(BatchKey::new("deadbeef", 0), units);
        let ids: Vec<_> = b.units.iter().map(|u| u.id.as_str()).collect();
        // a.ts comes first (a < b), then b.ts; within each, ids sorted.
        assert_eq!(ids, vec!["a", "z", "a", "b"]);
        let files: Vec<_> = b.units.iter().map(|u| u.provenance.file.as_str()).collect();
        assert_eq!(files, vec!["a.ts", "a.ts", "b.ts", "b.ts"]);
    }

    #[test]
    fn key_round_trips_through_serde_json() {
        let k = BatchKey::new("0123abcd", 7);
        let s = serde_json::to_string(&k).unwrap();
        let back: BatchKey = serde_json::from_str(&s).unwrap();
        assert_eq!(k, back);
    }

    #[test]
    fn default_batch_size_is_documented_constant() {
        // Trip-wire: if someone changes the constant, this test points at the
        // doc-comment that explains the rationale.
        assert_eq!(DEFAULT_BATCH_SIZE, 32);
    }
}
