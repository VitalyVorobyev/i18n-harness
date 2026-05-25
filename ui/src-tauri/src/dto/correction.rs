//! Wire-format types for correction recording and listing commands.

use serde::{Deserialize, Serialize};

/// Provenance of the MT proposal in a correction record, mirroring
/// [`CorrectionProvenance`] with serde derives so it crosses the IPC bridge.
/// All fields default to empty string; the caller fills only what the backend
/// made available.
///
/// [`CorrectionProvenance`]: i18n_harness_project::CorrectionProvenance
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CorrectionProvenanceWire {
    /// Backend name as registered by the `TranslationBackend` trait.
    #[serde(default)]
    pub backend: String,
    /// Model identifier as the backend reports it.
    #[serde(default)]
    pub model: String,
    /// Free-form model version / revision string.
    #[serde(default)]
    pub model_version: String,
    /// Prompt template version identifier.
    #[serde(default)]
    pub prompt_template_version: String,
    /// Hash of the glossary content at correction time.
    #[serde(default)]
    pub glossary_version: String,
}

/// IPC payload for `record_correction_in_project`.
#[derive(Debug, Deserialize)]
pub struct RecordCorrectionRequest {
    /// Absolute or manifest-relative catalog path.
    pub catalog_path: String,
    /// Target locale id.
    pub locale: String,
    /// Unit that was corrected.
    pub unit_id: String,
    /// Source text.
    pub source: String,
    /// MT proposal that was edited (empty for manual-from-scratch).
    pub mt_proposal: String,
    /// The accepted human translation.
    pub human_target: String,
    /// Provenance of `mt_proposal`.
    #[serde(default)]
    pub provenance: CorrectionProvenanceWire,
    /// Flags the unit carried at correction time.
    #[serde(default)]
    pub flags_at_correction: Vec<i18n_harness_core::Flag>,
}

/// IPC response from `record_correction_in_project`.
#[derive(Debug, Serialize)]
pub struct CorrectionIdResponse {
    /// The assigned correction id in `"corr_<12-hex>"` form.
    pub id: String,
}

/// Filter passed to `list_corrections_in_project`. All fields are optional;
/// an all-default filter returns every record (AND semantics for non-None fields).
#[derive(Debug, Default, Deserialize)]
pub struct ListCorrectionsFilter {
    /// Restrict to this catalog (absolute or manifest-relative path).
    #[serde(default)]
    pub catalog_path: Option<String>,
    /// Restrict to this target locale id.
    #[serde(default)]
    pub locale: Option<String>,
    /// Restrict to this unit id.
    #[serde(default)]
    pub unit_id: Option<String>,
    /// If true, restrict to corrections that are in the curated set.
    ///
    /// Note: `CorrectionFilter` has no `curated_only` field; this flag is
    /// honoured by filtering the result list against the project's curated set
    /// after the JSONL scan.
    #[serde(default)]
    pub curated_only: bool,
}

/// IPC payload for `set_review_status_in_project`. Wraps the three fields
/// `Project::set_review_status` accepts beyond catalog + unit.
#[derive(Debug, Deserialize)]
pub struct ReviewStatusInput {
    /// The new review status. `None` clears the unit's record.
    pub status: Option<i18n_harness_core::ReviewStatus>,
    /// Source-hash value from the unit at review time (empty if not available).
    #[serde(default)]
    pub source_hash_at_review: String,
    /// Free-form reviewer note.
    #[serde(default)]
    pub reviewer_note: Option<String>,
}
