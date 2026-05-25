//! Error types for [`crate::CatalogFormat`] implementations.
//!
//! Separated from individual format modules so that callers can match on a
//! single error enum regardless of which format produced it.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Failures that can occur while converting placeholders between a format's
/// native syntax and ICU MessageFormat.
///
/// These bubble up from [`crate::CatalogFormat::placeholders_to_icu`] and
/// [`crate::CatalogFormat::placeholders_from_icu`] and (wrapped in
/// [`CatalogError::PlaceholderConversion`]) from `extract` / `apply` paths
/// that invoke the converter.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PlaceholderError {
    /// The source string contains a placeholder token the converter does not
    /// know how to handle (e.g. gettext `%n` glibc-format, width specifiers
    /// like `%-10s`).
    ///
    /// **When this fires.** The catalog format ships a deliberately
    /// restricted converter and refuses to silently drop or rewrite
    /// constructs it cannot round-trip. The translator can edit the unit by
    /// hand if they need the exotic form.
    #[error("unsupported placeholder syntax: {0}")]
    UnsupportedSyntax(String),

    /// The ICU string references a named placeholder that the format cannot
    /// project back into its native syntax.
    ///
    /// **When this fires.** Inverse conversion (ICU → native) is only valid
    /// for placeholders the format originally produced. If the translator (or
    /// the backend) introduced a new named placeholder, the inverse converter
    /// surfaces it here instead of guessing.
    #[error("unknown named placeholder in target: {0}")]
    UnknownNamedPlaceholder(String),
}

/// Failures that can occur in [`crate::CatalogFormat::extract`] /
/// [`crate::CatalogFormat::apply`].
#[derive(Debug, Error)]
pub enum CatalogError {
    /// The file could not be read or written.
    ///
    /// **When this fires.** Filesystem-level: permission denied, missing
    /// file, full disk. The catalog state on disk is unchanged on a failed
    /// `apply` because implementations write to a temp path and rename only
    /// on success.
    #[error("failed to {op} {path}: {source}")]
    Io {
        /// The operation that failed (`"read"`, `"write"`, etc.).
        op: &'static str,
        /// The path the harness tried to touch.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// The catalog text could not be parsed against the format's grammar.
    ///
    /// **When this fires.** The file does not satisfy the format's structural
    /// rules — invalid PO header, malformed JSON, mis-nested XML, non-UTF-8
    /// bytes, etc. Carries a human-readable diagnostic intended for the UI
    /// (line/column when the format parser can produce one).
    #[error("parse error in {path}: {reason}")]
    Parse {
        /// Path the harness was reading.
        path: PathBuf,
        /// Human-readable diagnostic with line/column where available.
        reason: String,
    },

    /// `apply` could not splice the unit back into the catalog bytes.
    ///
    /// **When this fires.** A unit handed to `apply` cannot be matched
    /// against the catalog (e.g. its id no longer exists because the source
    /// changed between extract and apply), or the format-specific writer
    /// encountered an unexpected shape.
    #[error("apply failed for {path}: {reason}")]
    Apply {
        /// Output path the harness tried to write.
        path: PathBuf,
        /// Human-readable diagnostic.
        reason: String,
    },

    /// The format identifier passed to [`crate::open_catalog_by_format`] is
    /// not one of the formats this crate knows about.
    ///
    /// **When this fires.** A new `CatalogFormat` enum variant was added in
    /// the project manifest but the dispatcher was not updated. This is a
    /// programming error — surface it loudly.
    #[error("unsupported catalog format: {0}")]
    UnsupportedFormat(String),

    /// A placeholder converter failed inside an extract / apply path.
    #[error("placeholder conversion failed: {0}")]
    PlaceholderConversion(#[from] PlaceholderError),
}
