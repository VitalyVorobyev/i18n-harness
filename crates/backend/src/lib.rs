//! [`TranslationBackend`] trait and reference implementations.
//!
//! See `docs/initial_design.md` §5 and `CLAUDE.md` invariant #2. The
//! translation backend is the **one and only** place an LLM (or any other
//! engine) participates in the pipeline. Everything else — parsing,
//! validation, write-back — is deterministic Rust.
//!
//! # The pipeline shape
//!
//! ```text
//!   adapter.extract        → Vec<Unit>
//!   batched(...)           → core::Batch
//!   backend.translate_batch → Vec<TranslationOutcome>    [LLM here]
//!   merge(batch, outcomes) → Vec<Unit> (target filled)
//!   gate.validate_batch    → Vec<GateReport>
//!   adapter.apply          → catalog file (byte-stable)
//! ```
//!
//! # Why the trait returns [`TranslationOutcome`] and not [`i18n_harness_core::Unit`]
//!
//! The backend produces **text only**. Structural fields (`id`,
//! `placeholders`, `plural_arity`, `provenance`, `state`) are owned by the
//! adapter and the caller, and a backend that tries to edit them is a bug.
//! Rather than rely on documentation to enforce that, the trait surface
//! makes it *impossible* to express: a [`TranslationOutcome`] contains
//! either a string (success), a reason for skipping, or a reason for
//! failing. It does not carry a `Unit`. The caller threads the outcome
//! back into its own `Unit` vector, preserving id/placeholders/etc.
//!
//! This also means that if a backend turns malicious or buggy, the worst
//! it can do is produce *worse text* — the structural integrity of the
//! catalog is preserved by the trait surface, not by validation alone.
//!
//! # Failure model (whole-batch vs per-unit)
//!
//! - Whole-batch failures ([`BackendError`]): the backend could not even
//!   attempt the batch. Network is down, model is unloaded, auth failed,
//!   request was malformed. The caller retries the whole batch or aborts.
//! - Per-unit failures ([`TranslationOutcome::Failed`]): the backend
//!   attempted the unit and produced something the caller cannot use.
//!   Model gave up, JSON parse failed for that slot, model returned text
//!   that does not match the requested shape (singular/plural). The
//!   caller retries the failed slots, flags them for human review, or
//!   leaves them untranslated.
//! - Per-unit skip ([`TranslationOutcome::Skipped`]): the backend
//!   deliberately declined a unit (e.g., manual backend skipped on user
//!   request). Treated as "leave the unit untranslated"; not a hard
//!   failure.
//!
//! # Why sync (no `async`, no `tokio`)
//!
//! Per `docs/implementation_plan.md` §13 decision #5, the trait stays
//! synchronous. Backends that need HTTP bring their own minimal sync
//! client (e.g., `ureq`); we do not pull `tokio` into the workspace. The
//! CLI driver translates one batch at a time and writes metrics; the
//! desktop UI does its own threading on top of the sync trait.
//!
//! # Backends in this crate
//!
//! - [`manual`] — no model. The caller provides a closure that produces
//!   translations (typically reading from a JSONL file in the two-phase
//!   CLI flow). Useful for tests and for the agent path. Always built.
//! - `ollama` — local HTTP backend; lands behind the `ollama` feature in
//!   the follow-up implementer pass. The trait shape was designed so this
//!   addition is mechanical.

#![forbid(unsafe_code)]

mod context;
mod error;
mod outcome;
pub mod prompt;
mod trait_def;

#[cfg(feature = "manual")]
pub mod manual;

#[cfg(feature = "ollama")]
pub mod ollama;

pub mod agent_batch;

pub use agent_batch::{AgentBatchError, ExportedUnit, read_targets, read_unit_ids, write_export};
pub use context::PromptContext;
pub use error::BackendError;
pub use outcome::{FailureKind, TranslatedText, TranslationOutcome};
pub use trait_def::TranslationBackend;

#[cfg(feature = "manual")]
pub use manual::{ManualBackend, ManualResponse};

#[cfg(feature = "ollama")]
pub use ollama::{OllamaBackend, PromptResponseShape};

/// Return the embedded Ollama v2 prompt template verbatim.
///
/// Embedded via `include_str!` at compile time from
/// `prompts/ollama-translate-v2.txt`. Callers (e.g. the project crate's
/// tuning-bundle export) copy this into a bundle without having to know the
/// on-disk path.
pub fn ollama_prompt_v2() -> &'static str {
    include_str!("../prompts/ollama-translate-v2.txt")
}

/// Version identifier for the embedded Ollama v2 prompt template.
///
/// Matches the `[template=...]` line in the template body. Used by
/// `TuningBundleSummary::prompt_template_version` so the bundle is
/// self-documenting without callers having to parse the template text.
pub const OLLAMA_PROMPT_V2_VERSION: &str = "ollama-translate-v2";
