//! Error types shared by `core` and re-used by other crates when they need to
//! signal a `core`-level failure.

use thiserror::Error;

/// Generic error envelope for the core crate.
///
/// Other crates wrap this with their own error enums; we do not try to be the
/// one error type for the whole workspace.
#[derive(Debug, Error)]
pub enum CoreError {
    /// An [`IntermediateError`] surfaced while (de)serializing a JSONL line.
    ///
    /// Caller can either skip the line (logging it) or abort the batch,
    /// depending on whether they have a write-back commitment yet.
    #[error(transparent)]
    Intermediate(#[from] IntermediateError),
}

/// Errors that can occur while (de)serializing an [`crate::Intermediate`]
/// line.
#[derive(Debug, Error)]
pub enum IntermediateError {
    /// The line is not valid JSON.
    ///
    /// Occurs when reading a corrupted or truncated intermediate file.
    /// Caller should treat the line as lost — the catalog is the source of
    /// truth and can be re-extracted.
    #[error("intermediate line is not valid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),

    /// The line's `schema_version` is from a future version this binary does
    /// not know how to read.
    ///
    /// Occurs when an intermediate file was produced by a newer
    /// `i18n-harness` than the one currently running. Caller should re-extract
    /// from the catalog with the current binary; do not attempt to apply.
    #[error(
        "intermediate line has unsupported schema_version {found} (this binary supports up to {supported})"
    )]
    UnsupportedSchemaVersion {
        /// Schema version found on the line.
        found: u32,
        /// Highest schema version this binary knows how to read.
        supported: u32,
    },
}
