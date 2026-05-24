//! Translation-memory types for the project crate (M4.1a stub).
//!
//! This module holds only the types required by [`crate::error::ProjectError`]
//! in M4.1a. The full `CorrectionStore`, `CuratedSet`, and the I/O operations
//! are implemented in slice d. Keeping this stub here avoids a forward
//! reference cycle: `error.rs` names `CorrectionId`, and `lib.rs` re-exports
//! it, so the type must exist even though the store logic is not yet present.

use serde::{Deserialize, Serialize};

/// Stable, content-addressed id for one correction record.
///
/// Format: `"corr_<12-hex>"` where the hex is the first 12 chars of a
/// SHA-256 over `(catalog_manifest_path, unit_id, source, mt_proposal,
/// human_target, ts_micros)`. Including `ts_micros` makes the id unique
/// even when the same unit is re-edited immediately after acceptance.
///
/// Stored as a plain string so it round-trips through JSON and TOML without
/// precision loss or quoting issues.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CorrectionId(
    /// The `"corr_<12-hex>"` string.
    pub String,
);

impl std::fmt::Display for CorrectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
