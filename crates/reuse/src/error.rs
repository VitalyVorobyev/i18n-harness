//! Error type for the reuse / split / merge operations.

use std::path::PathBuf;

use i18n_harness_adapter_qt::ExtractError;
use thiserror::Error;

/// Errors that can occur while reusing translations from references, or
/// merging a translated remainder back into its base.
///
/// Every variant names the offending file or unit ids so the CLI and the
/// Tauri layer can present an actionable message without re-deriving context.
#[derive(Debug, Error)]
pub enum ReuseError {
    /// A catalog (base, reference, or remainder) could not be extracted.
    ///
    /// Occurs on I/O failure, malformed XML, or a non-`.ts` file. The wrapped
    /// [`ExtractError`] carries the underlying cause; `path` records which
    /// catalog the harness was reading so the caller can point the user at it.
    /// The caller cannot recover automatically — the catalog must be fixed or
    /// re-pointed.
    #[error("failed to extract {path}: {source}")]
    Extract {
        /// Path the harness tried to extract.
        path: PathBuf,
        /// Underlying adapter extract error.
        #[source]
        source: ExtractError,
    },

    /// `merge_back` was handed a remainder containing unit ids that do not
    /// exist in the base catalog.
    ///
    /// Occurs when the remainder file is not a true subset of the base — for
    /// instance it was split from a *different* base, or the base was edited
    /// (units renamed/removed) after the split. The harness refuses to apply
    /// stray units rather than guess. The caller should re-run the split from
    /// the current base. The listed ids are the stray ones, sorted.
    #[error(
        "remainder contains {n} unit id(s) not present in base {base}: {ids}",
        n = .ids.len(),
        ids = fmt_ids(.ids),
    )]
    MergeStrayIds {
        /// Base catalog path.
        base: PathBuf,
        /// Stray unit ids, sorted for a stable message.
        ids: Vec<String>,
    },

    /// `merge_back` detected unit ids that are `Finished` (complete target) in
    /// **both** the base and the remainder.
    ///
    /// The split contract is that the base half keeps the already-finished
    /// units while the remainder half carries only the leftover untranslated
    /// units; the two are disjoint by construction. An overlap means the two
    /// files were not produced as halves of one original (or one was edited
    /// after the split), and merging would force the harness to pick a winner
    /// silently. It refuses instead. The caller should re-derive the halves.
    /// The listed ids are the overlapping ones, sorted.
    #[error(
        "{n} unit id(s) are finished in both base {base} and remainder; \
         the files are not disjoint halves: {ids}",
        n = .ids.len(),
        ids = fmt_ids(.ids),
    )]
    MergeOverlap {
        /// Base catalog path.
        base: PathBuf,
        /// Overlapping unit ids, sorted for a stable message.
        ids: Vec<String>,
    },

    /// A locale id could not be resolved against the workspace locale table.
    ///
    /// Occurs when `reuse_from_references` is handed a `&Locale` that is fine,
    /// but a downstream lookup (currently none) fails, or a caller-facing
    /// helper resolves a locale id string. Retained as a variant so the public
    /// surface does not need to grow when locale resolution moves into this
    /// crate. The caller should pass a known locale id.
    #[error("unknown locale: {0}")]
    UnknownLocale(String),
}

/// Render an id list for an error message. Each id is wrapped in backticks so
/// ids containing commas or spaces stay legible; the separator is `, `.
fn fmt_ids(ids: &[String]) -> String {
    ids.iter()
        .map(|id| format!("`{id}`"))
        .collect::<Vec<_>>()
        .join(", ")
}
