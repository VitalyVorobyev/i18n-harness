//! Wire-format types for tuning bundle export/list commands.

use serde::Serialize;

/// Wire-format response from `export_tuning_bundle_in_project`.
///
/// Mirrors [`i18n_harness_project::TuningBundleSummary`] with all path fields
/// as `String` for TypeScript interop.
#[derive(Debug, Serialize)]
pub struct ExportTuningBundleResponse {
    /// Absolute path to the exported bundle directory.
    pub path: String,
    /// Number of resolved examples written to `examples.jsonl`.
    pub examples_count: usize,
    /// Locale ids that appear in at least one example.
    pub locales: Vec<String>,
    /// `true` if `score.json` was written (prior evaluation existed).
    pub has_score: bool,
    /// Prompt template version identifier baked into `prompt.txt`.
    pub prompt_template_version: String,
}
