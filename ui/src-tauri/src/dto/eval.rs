//! Wire-format types for the in-app prompt evaluation commands.
//!
//! All types in this module are gated behind `feature = "ollama"`.

use serde::Serialize;

/// Synchronous response from `run_evaluation_in_project`. The worker thread
/// runs in the background; the frontend subscribes to events keyed by `job_id`.
#[derive(Debug, Serialize)]
pub struct EvaluationStarted {
    /// Opaque process-unique job id (UUID v4 hex, no hyphens).
    pub job_id: String,
    /// Number of curated examples the worker will evaluate.
    pub total: usize,
}

/// Per-example progress event, emitted on `eval-progress-<job_id>` after each
/// completed backend call in the evaluation worker.
#[derive(Debug, Serialize, Clone)]
pub struct EvaluationProgressPayload {
    /// Job id for correlation.
    pub job_id: String,
    /// Examples evaluated so far (1-indexed).
    pub completed: usize,
    /// Total examples the worker started with.
    pub total: usize,
    /// Target locale of the just-evaluated example.
    pub last_example_locale: String,
}

/// Terminal event payload, emitted exactly once on `eval-completed-<job_id>`
/// (success or cancellation) or `eval-failed-<job_id>` (hard failure). The
/// worker does **not** persist partial runs; `run` is `None` unless the
/// evaluation completed successfully.
#[derive(Debug, Serialize, Clone)]
pub struct EvaluationTerminalPayload {
    /// Job id for correlation.
    pub job_id: String,
    /// `true` if the worker observed cancellation before completing all examples.
    pub cancelled: bool,
    /// Hard-failure reason; `None` on success or cancellation.
    pub failed_reason: Option<String>,
    /// The completed evaluation run, present only on successful completion.
    pub run: Option<i18n_harness_project::EvaluationRun>,
}
