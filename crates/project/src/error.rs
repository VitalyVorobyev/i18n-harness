//! Error and warning types for the project crate.
//!
//! [`ProjectError`] is the single failure type for all operations in this
//! crate. [`ProjectWarning`] carries non-fatal diagnostics returned alongside
//! a successful result so the caller can surface them to the user.

use std::path::PathBuf;

use crate::manifest::CatalogFormat;
use crate::memory::CorrectionId;

/// Discovery format guess — forward-declared here so `CatalogFormatMismatch`
/// can reference it without a circular dep on the `discovery` module.
///
/// Mirrors `crate::discovery::FormatGuess` but is defined here to let the
/// error module stand alone. The two types are kept in sync by the design
/// contract; only the error module uses this local alias.
pub use crate::manifest::FormatGuess;

/// All failure modes produced by this crate.
///
/// One enum (matching `GlossaryError` / `CoreError` workspace precedent) so
/// the CLI and Tauri layer pattern-match a single type at every call site.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    // ── Manifest ───────────────────────────────────────────────────────────
    /// No `i18n-harness.toml` at the given root. Caller may fall back to
    /// `Project::discover`.
    #[error("no `i18n-harness.toml` at {}", path.display())]
    ManifestMissing {
        /// Directory that was searched.
        path: PathBuf,
    },

    /// Manifest TOML is malformed or fails `deny_unknown_fields` on a
    /// sub-table.
    #[error("parse {}: {source}", path.display())]
    ManifestParse {
        /// Path of the file being parsed.
        path: PathBuf,
        /// Underlying `toml` deserialization error.
        source: toml::de::Error,
    },

    /// Manifest declares a schema version newer than this binary supports.
    /// Caller should upgrade the harness.
    #[error("manifest schema {found} > supported {supported}")]
    UnsupportedSchemaVersion {
        /// Version declared in the manifest.
        found: u32,
        /// Highest version this binary can handle.
        supported: u32,
    },

    /// Manifest is missing a required field such as `project.schema`.
    /// `field` is the dotted key path.
    #[error("manifest missing required field `{field}`")]
    MissingRequiredField {
        /// Dotted key path to the missing field, e.g. `"project.schema"`.
        field: String,
    },

    // ── Catalog ────────────────────────────────────────────────────────────
    /// A `[[catalogs]]` entry points at a file that does not exist on disk.
    /// Encountered in `Project::open` strict mode; discovery does not produce
    /// this variant.
    #[error("catalog not found: {}", path.display())]
    CatalogNotFound {
        /// Absolute path that was expected to exist.
        path: PathBuf,
    },

    /// The file does not match its declared format. The UI should offer to
    /// re-declare the format or remove the entry.
    #[error("catalog {} declared {declared:?} but content looks like {sniffed:?}", path.display())]
    CatalogFormatMismatch {
        /// Absolute path of the mismatched file.
        path: PathBuf,
        /// Format the manifest declared.
        declared: CatalogFormat,
        /// Format the content sniffer detected.
        sniffed: FormatGuess,
    },

    /// Two `[[catalogs]]` entries share the same path.
    #[error("duplicate catalog path: {}", path.display())]
    DuplicateCatalogPath {
        /// The path that appeared twice.
        path: PathBuf,
    },

    // ── Glossary ───────────────────────────────────────────────────────────
    /// Transparent wrapper so glossary load failures propagate without losing
    /// the inner error's `Display` / `source` chain.
    #[error(transparent)]
    Glossary(#[from] i18n_harness_glossary::GlossaryError),

    // ── Corrections / curated ─────────────────────────────────────────────
    /// A JSONL line in `corrections.jsonl` cannot be parsed. The store is
    /// line-recoverable — the caller skips the bad line and surfaces this
    /// warning rather than aborting.
    #[error("malformed correction line {line_no} in {}: {source}", path.display())]
    CorrectionParse {
        /// File that contains the bad line.
        path: PathBuf,
        /// 1-based line number of the bad record.
        line_no: usize,
        /// Underlying JSON error.
        source: serde_json::Error,
    },

    /// The `CorrectionId` passed to `promote_to_curated` is not in
    /// `corrections.jsonl`. Caller should refresh the list and retry.
    #[error("correction id `{id}` not found")]
    CorrectionNotFound {
        /// The id that was looked up.
        id: CorrectionId,
    },

    /// `curated.toml` exists but is malformed.
    #[error("parse curated.toml: {source}")]
    CuratedParse {
        /// Underlying `toml` error.
        source: toml::de::Error,
    },

    // ── Review ────────────────────────────────────────────────────────────
    /// A review-status operation referenced a catalog that is not in the
    /// manifest. Caller: ensure the catalog is registered via `add_catalog`
    /// before recording reviews against it.
    #[error("review op references unknown catalog: {}", path.display())]
    UnknownCatalog {
        /// Manifest-relative or absolute path that did not match a known
        /// `[[catalogs]]` entry.
        path: PathBuf,
    },

    /// A JSONL line in `review.jsonl` cannot be parsed. The store is
    /// line-recoverable — the fold skips the bad line and surfaces this error
    /// rather than aborting.
    #[error("malformed review event line {line_no} in {}: {source}", path.display())]
    ReviewEventParse {
        /// File that contains the bad line.
        path: PathBuf,
        /// 1-based line number of the bad record.
        line_no: usize,
        /// Underlying JSON error.
        source: serde_json::Error,
    },

    // ── I/O ───────────────────────────────────────────────────────────────
    /// Generic filesystem failure. `path` names the file being accessed;
    /// `source` is the OS-level error.
    #[error("io {}: {source}", path.display())]
    Io {
        /// Path involved in the failing operation.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },

    // ── Validation ────────────────────────────────────────────────────────
    /// `[backend.default] kind` is recognized in the enum but not enabled in
    /// this build (feature-gated). The UI can offer to switch backends or
    /// rebuild with the required feature.
    #[error("backend `{kind}` not enabled in this build")]
    BackendNotEnabled {
        /// The backend kind string as it appeared in the manifest.
        kind: String,
    },

    // ── Tuning bundle ─────────────────────────────────────────────────────
    /// Tuning-bundle export was requested but the curated set is empty.
    /// The user must promote at least one correction before exporting.
    #[error("no curated examples; promote some corrections first")]
    NoCuratedExamples,
}

/// Non-fatal diagnostic produced alongside a successful operation result.
///
/// Mirrors `GlossaryWarning`: returned as `Vec<ProjectWarning>` next to the
/// success value so the caller can surface them without aborting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectWarning {
    /// A `[locales.<id>]` block references a locale id not present in the
    /// workspace's `crates/locales` table. Permitted — the manifest may be
    /// ahead of locale registration; the user should fix the typo or register
    /// the locale.
    UnknownLocale {
        /// The locale id as it appears in the manifest.
        locale: String,
    },

    /// A `[[catalogs]]` entry's locale field does not resolve through the
    /// workspace locale table. The catalog is still listed but operations
    /// that require a resolved locale will fail.
    UnknownCatalogLocale {
        /// Manifest-relative path of the catalog.
        path: PathBuf,
        /// The locale id that could not be resolved.
        locale: String,
    },

    /// A glossary warning threaded through from `crates/glossary`.
    Glossary(i18n_harness_glossary::GlossaryWarning),
}

impl std::fmt::Display for ProjectWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownLocale { locale } => {
                write!(
                    f,
                    "manifest references locale `{locale}` which is not in the workspace locales table"
                )
            }
            Self::UnknownCatalogLocale { path, locale } => {
                write!(
                    f,
                    "catalog `{}` declares locale `{locale}` which is not in the workspace locales table",
                    path.display()
                )
            }
            Self::Glossary(w) => write!(f, "{w}"),
        }
    }
}
