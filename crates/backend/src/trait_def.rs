//! The [`TranslationBackend`] trait itself.
//!
//! Kept in its own file (rather than inlined into `lib.rs`) so the doc
//! comment can be the canonical contract reference: when a new backend
//! lands, the implementer reads this file end-to-end.

use i18n_harness_core::Batch;
use i18n_harness_glossary::Glossary;
use i18n_harness_locales::Locale;

use crate::error::BackendError;
use crate::outcome::TranslationOutcome;

/// The translation engine extension point.
///
/// Implementations: built into this crate, [`crate::manual::ManualBackend`]
/// (always); the `ollama` HTTP backend (M2 follow-up, behind the
/// `ollama` feature); `openai-compatible` (M4, behind `openai-compatible`).
/// Third-party crates implement this trait the same way.
///
/// # The contract
///
/// 1. **Batch in, batch out, same length, same order.**
///    `translate_batch(&self, batch, ..)` returns a `Vec` whose length
///    equals `batch.units.len()`. The outcome at index `i` corresponds
///    to `batch.units[i]`. Producing a different length, dropping a unit,
///    or shuffling the order is a backend bug.
/// 2. **Text only, no structural mutation.** The return type is
///    `Vec<TranslationOutcome>`, which carries text and per-unit reason
///    strings only. Backends do **not** receive `&mut Unit` and have no
///    way to mutate `id`, `placeholders`, `plural_arity`, `provenance`,
///    or `state`. Those structural fields are owned by the caller; the
///    backend produces text that the caller merges into its own units.
///    Combined with the gate, this means a malicious or buggy backend
///    cannot corrupt catalog structure.
/// 3. **Failure model is bimodal.**
///    - Whole-batch failure: return `Err(BackendError)`. Examples:
///      network down, model unloaded, auth failed. The caller may retry
///      the whole batch or abort.
///    - Per-unit failure: return [`TranslationOutcome::Failed`] with a
///      `reason` and `retryable` flag. Examples: model returned malformed
///      text for that slot, slot took too long, model refused. The caller
///      decides what to do per unit.
/// 4. **No I/O assumptions.** The trait does not require HTTP, files, or
///    network. The [`crate::manual::ManualBackend`] reads from a
///    closure provided by the caller. A future on-device backend could
///    talk to an in-process FFI engine. The trait does not name a
///    transport.
/// 5. **Sync.** No `async`, no `tokio`. Backends that need HTTP bring
///    their own minimal sync client (e.g., `ureq`).
/// 6. **No global state.** All inputs are passed in (`batch`, `locale`,
///    `glossary`). Backends may hold endpoint configuration on `self`;
///    they do not consult environment variables or files at translation
///    time (configure that at construction).
///
/// # `name` and `is_deterministic`
///
/// `name` is recorded verbatim as the `backend` label on every metrics
/// event. By convention: lower-case kebab (`"manual"`, `"ollama"`,
/// `"openai-compatible"`, `"echo"`). Third-party backends should use a
/// short, stable, machine-readable identifier; the user only sees it in
/// metrics.
///
/// `is_deterministic` is `true` if calling `translate_batch` twice with
/// the same inputs produces the same outputs. LLMs return `false`; the
/// manual backend's determinism depends on whether the closure is pure.
/// The metrics view uses this to decide whether to retry a unit (no
/// point retrying a deterministic Failed outcome).
///
/// # What the trait does NOT guarantee
///
/// - That the produced text is valid ICU MessageFormat — the gate
///   enforces that downstream.
/// - That the produced text respects glossary terms — the gate may
///   surface a soft flag.
/// - That timeouts are handled — backends with network calls implement
///   their own timeout strategy and surface failures via
///   [`BackendError::Network`] or [`TranslationOutcome::Failed`].
pub trait TranslationBackend {
    /// Short, stable, machine-readable identifier for this backend.
    /// Recorded as the `backend` label on metrics events.
    fn name(&self) -> &str;

    /// `true` if `translate_batch` is a pure function of its inputs.
    /// Used by the CLI's retry policy.
    fn is_deterministic(&self) -> bool;

    /// Translate every unit in `batch`, in batch order.
    ///
    /// # Returns
    ///
    /// On success, a `Vec` of length `batch.units.len()`. The outcome at
    /// index `i` corresponds to `batch.units[i]`. The caller merges the
    /// outcomes into its own owned `Unit` vector.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] only on whole-batch failure (network
    /// down, model unloaded, malformed response, auth, configuration).
    /// Per-unit failures are surfaced inside
    /// [`TranslationOutcome::Failed`] within the success-case vector.
    fn translate_batch(
        &self,
        batch: &Batch,
        locale: &Locale,
        glossary: Option<&Glossary>,
    ) -> Result<Vec<TranslationOutcome>, BackendError>;
}
