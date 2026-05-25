//! Wire-format types for bulk-translate and single-unit translate commands.
//!
//! All types in this module are gated behind `feature = "ollama"` because
//! the translate commands require the Ollama backend.

use i18n_harness_core::Unit;
use i18n_harness_gate::GateReport;
use serde::{Deserialize, Serialize};

/// Result of `translate_unit`: the updated unit plus the gate report
/// the harness ran on it.
#[derive(Debug, Serialize)]
pub struct TranslateResult {
    /// The unit after merging the backend's output and running the
    /// gate. Its `state` reflects the gate outcome.
    pub unit: Unit,
    /// The gate report. Findings drive the UI's inline review.
    pub report: GateReport,
}

/// Scope selector for `translate_batch_in_project`. Controls which units
/// the bulk worker attempts.
///
/// The harness must never overwrite catalog entries the source no longer
/// references. `Finished` units are also always excluded; a "re-translate
/// everything including human-accepted" variant is a meaningful policy
/// decision that belongs to the M4.8 UI design, not this primitive, and
/// would require pre-demoting Finished → Proposed to honour
/// [`UnitState::is_writable`]. Adding a new scope variant later is
/// backward-compatible.
///
/// [`UnitState::is_writable`]: i18n_harness_core::UnitState::is_writable
#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum BatchScope {
    /// Only units whose state is `Untranslated` — fill in the gaps.
    Untranslated,
    /// `Untranslated` plus `Proposed` — re-translate everything not yet
    /// human-confirmed.
    UntranslatedAndProposed,
}

/// Started-bulk-translate response, returned immediately after the worker
/// thread is spawned. The frontend uses `job_id` for the follow-up
/// `cancel_translation` call and to subscribe to `batch-progress-<id>` /
/// `batch-completed-<id>` / `batch-failed-<id>` Tauri events.
#[derive(Debug, Serialize, Clone)]
pub struct TranslateBatchStarted {
    /// Opaque process-unique job id (UUID v4 hex, no hyphens).
    pub job_id: String,
    /// Number of units the worker will attempt at start time. The catalog
    /// state could change while the worker runs (a human edit promoting a
    /// unit out of scope, say), but the total in the progress events is
    /// pinned to this number — partial completion is reported as
    /// `completed / total`.
    pub total: usize,
}

/// Pre-unit event payload, emitted as `batch-unit-started-<job_id>` just before
/// the backend call begins for each unit. Lets the frontend light up a per-cell
/// spinner without waiting for the full round-trip to complete.
#[derive(Debug, Serialize, Clone)]
pub struct BatchUnitStartedPayload {
    /// Id of the unit about to be translated.
    pub unit_id: String,
    /// Target locale for this translation call.
    pub locale: String,
}

/// Per-unit progress event payload, emitted as `batch-progress-<job_id>` after
/// each completed network round-trip.
#[derive(Debug, Serialize, Clone)]
pub struct BatchProgressPayload {
    /// Units processed so far (1-indexed: the first emit has `completed = 1`).
    pub completed: usize,
    /// Total units the worker started with.
    pub total: usize,
    /// The just-translated unit (post-merge). The UI patches this into its
    /// in-memory cache without an extra round trip.
    pub unit: Unit,
    /// Shortcut for the UI: `true` if the gate or LLM attached one or more
    /// flags. Equivalent to `!unit.flags.is_empty()`; pre-computed so the UI
    /// doesn't need to inspect the FlagSet.
    pub flagged: bool,
}

/// Terminal event payload, emitted exactly once on `batch-completed-<job_id>`
/// (clean exit or cancelled) or `batch-failed-<job_id>` (hard failure).
#[derive(Debug, Serialize, Clone)]
pub struct BatchTerminalPayload {
    /// Units processed when the worker stopped. For success: equals `total`.
    /// For cancellation: count of fully-merged units before the cancel was
    /// observed. For failure: count before the failing unit.
    pub completed: usize,
    /// Total units the worker started with.
    pub total: usize,
    /// `true` if the worker stopped because cancellation was observed.
    /// Mutually exclusive with `failed_reason.is_some()`.
    pub cancelled: bool,
    /// Hard-failure reason. `None` on clean completion or cancellation;
    /// `Some` only when a mid-batch backend error stopped the run.
    pub failed_reason: Option<String>,
}
