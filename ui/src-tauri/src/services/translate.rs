//! Project-mode translate helpers — the testable core behind the Tauri commands.
//!
//! Everything in this module is gated behind `feature = "ollama"` because it
//! depends on [`i18n_harness_backend::OllamaBackend`] and the full outcome
//! pipeline. The file-mode `translate_unit` command and the project-mode
//! commands both call into this module; logic that is common to both lives
//! here so it is tested once and read-once.
//!
//! # Lock ordering (see [`translate_one`])
//!
//! `translate_one` acquires three locks in a precise order. Violating this
//! order causes deadlock. The four-step comment above the function body
//! describes the canonical sequence.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use i18n_harness_core::{Target, Unit, UnitId, UnitState};
use i18n_harness_gate::GateReport;
use i18n_harness_glossary::Glossary;
use i18n_harness_locales::Locale;
use i18n_harness_project::Project;

use crate::dto::BatchScope;
use crate::error;
use crate::state::OpenCatalogEntry;

// ── BatchScope predicate ─────────────────────────────────────────────────────

/// Predicate for `BatchScope` selection. Vanished/Obsolete and Finished are
/// always excluded — see `BatchScope` docs.
#[cfg(feature = "ollama")]
pub(crate) fn unit_matches_scope(unit: &Unit, scope: BatchScope) -> bool {
    match (unit.state, scope) {
        (UnitState::Untranslated, _) => true,
        (UnitState::Proposed, BatchScope::Untranslated) => false,
        (UnitState::Proposed, BatchScope::UntranslatedAndProposed) => true,
        // Vanished / Obsolete / Finished: never.
        _ => false,
    }
}

// ── Context resolution ───────────────────────────────────────────────────────

/// Resolve `(locale, glossary)` for a project-routed translate command.
///
/// Holds the project lock for as short a window as possible: enough to look up
/// the catalog's declared locale, merge it with the project's locale config,
/// validate the declared backend kind, and clone the glossary. Drops the lock
/// before returning. The result is used both by the single-unit translate
/// command and by the bulk-translate worker (M4.2c.2), so the locale/glossary
/// view stays consistent across the two paths.
///
/// Returns the workspace-resolved [`Locale`] (a `&'static` reference held by
/// the `locales` crate, so it crosses the lock boundary trivially) and an
/// owned [`Glossary`] clone.
///
/// # Errors
///
/// - `"no project open"` if the project slot is empty.
/// - `"catalog not open in project"` if `abs` is not declared in the project
///   manifest.
/// - `"unknown locale ..."` if neither the project nor the workspace knows the
///   locale id.
/// - `"backend kind ... not supported yet"` if the project declares a non-Ollama
///   backend (we refuse rather than silently falling back).
#[cfg(feature = "ollama")]
pub(crate) fn resolve_project_translate_context(
    state: &tauri::State<'_, crate::AppState>,
    abs: &Path,
) -> Result<(&'static Locale, Option<Glossary>), String> {
    use i18n_harness_project::BackendKind;

    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;

    let catalog_ref = project
        .catalog(abs)
        .ok_or_else(|| "catalog not open in project".to_string())?;
    let locale_id = &catalog_ref.locale;

    // Three-layer locale merge; fall back to workspace-only if the project
    // doesn't declare this locale id (defensive, not the common path).
    let locale = project
        .locale(locale_id)
        .map(|r| r.workspace_locale())
        .or_else(|| Locale::by_id(locale_id))
        .ok_or_else(|| format!("unknown locale `{locale_id}`; add it to crates/locales"))?;

    if let Some(backend_cfg) = &project.manifest().backends.default {
        if backend_cfg.kind != BackendKind::Ollama {
            return Err(format!(
                "backend kind {:?} not supported yet",
                backend_cfg.kind,
            ));
        }
    }

    let glossary = project.glossary().cloned();
    Ok((locale, glossary))
}

// ── Glossary helpers ─────────────────────────────────────────────────────────

/// Validate a glossary term lookup and return the term's source string.
///
/// Extracts the repeated "find term, check DNT" logic shared by the command
/// and its test helpers. Returns the source string so the caller holds it by
/// value after the glossary borrow ends.
#[cfg(feature = "ollama")]
pub(crate) fn glossary_term_source(glossary: &Glossary, term_id: &str) -> Result<String, String> {
    let term = glossary
        .term(term_id)
        .ok_or_else(|| "term not found".to_string())?;
    if term.do_not_translate {
        return Err("term is do-not-translate".to_string());
    }
    Ok(term_id.to_owned())
}

/// Drive any `TranslationBackend` to translate one glossary term, returning
/// the proposed translation string.
///
/// Builds a minimal synthetic singular unit from `term_source`, calls
/// `backend.translate_batch`, and extracts the singular text from the first
/// outcome. This is the testable inner core of `translate_glossary_term`; the
/// Tauri command wraps it with `AppState` resolution and `OllamaBackend`
/// construction.
///
/// # Why a synthetic Unit
///
/// The `TranslationBackend` trait takes a `&Batch` (a slice of `Unit`s) — it
/// has no glossary-term–specific entry point. Building a minimal unit is
/// cheaper than adding a new trait method and stays within the existing
/// backend contract. The unit id is `"glossary::<term_source>"` so callers and
/// metrics can distinguish glossary-assist calls from catalog-unit calls.
#[cfg(feature = "ollama")]
pub(crate) fn dispatch_glossary_term_translation(
    backend: &dyn i18n_harness_backend::TranslationBackend,
    term_source: &str,
    locale: &Locale,
    glossary: &Glossary,
) -> Result<String, String> {
    use i18n_harness_backend::TranslationOutcome;
    use i18n_harness_core::{Batch, BatchKey};

    let synthetic =
        Unit::untranslated_singular(format!("glossary::{term_source}"), term_source.to_owned());

    let batch = Batch::new(BatchKey::new("glossary", 0), vec![synthetic]);
    let backend_name = backend.name().to_string();

    let outcomes = backend
        .translate_batch(&batch, locale, Some(glossary))
        .map_err(|e| format!("backend `{backend_name}` failed: {e}"))?;

    let outcome = outcomes
        .into_iter()
        .next()
        .ok_or_else(|| format!("backend `{backend_name}` returned no outcomes"))?;

    match outcome {
        TranslationOutcome::Translated { text, .. } => match text {
            i18n_harness_backend::TranslatedText::Singular(s) => Ok(s),
            i18n_harness_backend::TranslatedText::Plural(_) => {
                Err("backend returned plural for a singular glossary term".to_string())
            }
        },
        TranslationOutcome::Skipped { reason } => Err(format!("backend skipped: {reason}")),
        TranslationOutcome::Failed { reason, .. } => Err(format!("backend failed: {reason}")),
    }
}

// ── Core translate helper ────────────────────────────────────────────────────

/// Translate one project-stored unit: read it from the catalog store, call the
/// backend (releasing the catalog lock for the network round-trip), merge the
/// outcome back, optionally mark the unit `NeedsReview` and append a durable
/// review event.
///
/// This is the shared body of both `translate_unit_in_project` (single click)
/// and the per-iteration step of `translate_batch_in_project` (M4.2c.2); the
/// two paths must agree on flag merging, state transitions, review-status
/// side effects, and lock ordering, so they live in one place.
///
/// # Lock ordering
///
/// 1. Acquire `project_catalogs` briefly to clone the source unit; drop.
/// 2. Run the backend call with **no locks held** (network latency must not
///    serialise other Tauri commands).
/// 3. Re-acquire `project_catalogs` to merge the outcome and set `dirty`; drop.
/// 4. If the LLM attached at least one semantic flag, acquire `project` to
///    append a `NeedsReview` event via `set_review_status`.
///
/// # Failure routing
///
/// - `TranslationOutcome::Translated` → merge text, flags, confidence, notes;
///   set `UnitState::Proposed`; optionally set `ReviewStatus::NeedsReview`.
/// - `TranslationOutcome::Skipped` → propagate as `Err("backend skipped: ...")`.
/// - `TranslationOutcome::Failed { MalformedResponse, .. }` → return `Ok` with
///   the original unit and a synthesized `GateReport::backend_malformed_response`.
///   The Inspector renders this inline; no catalog state changes.
/// - `TranslationOutcome::Failed { .. }` → propagate as `Err("backend failed: ...")`.
#[cfg(feature = "ollama")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn translate_one(
    project_catalogs: &Mutex<BTreeMap<std::path::PathBuf, OpenCatalogEntry>>,
    project: &Mutex<Option<Project>>,
    abs: &Path,
    id: &UnitId,
    backend: &i18n_harness_backend::OllamaBackend,
    backend_name: &str,
    locale: &Locale,
    glossary: Option<&Glossary>,
) -> Result<crate::dto::TranslateResult, String> {
    use i18n_harness_backend::{FailureKind, TranslationBackend, TranslationOutcome};
    use i18n_harness_core::{Batch, BatchKey, FlagSet, ReviewStatus};

    // 1. Snapshot the source unit under a brief lock, then drop the lock so
    //    the network call does not hold up other commands.
    let original = {
        let store = project_catalogs
            .lock()
            .map_err(error::lock_poisoned("project_catalogs"))?;
        let entry = store.get(abs).ok_or_else(error::no_catalog_in_project)?;
        let unit = entry
            .catalog
            .units()
            .iter()
            .find(|u| u.id == *id)
            .ok_or_else(|| format!("unit not found: {id}"))?
            .clone();
        if !unit.state.is_writable() {
            return Err(format!(
                "unit {id} is {state:?} — not translatable",
                state = unit.state,
            ));
        }
        unit
    };

    // 2. Network round-trip with no locks held.
    let batch = Batch::new(BatchKey::new("ui", 0), vec![original.clone()]);
    let outcomes = backend
        .translate_batch(&batch, locale, glossary)
        .map_err(|e| format!("backend `{backend_name}` failed: {e}"))?;
    let outcome = outcomes
        .into_iter()
        .next()
        .ok_or_else(|| format!("backend `{backend_name}` returned no outcomes"))?;

    let mut merged = original.clone();
    match outcome {
        TranslationOutcome::Translated {
            text,
            flags,
            confidence,
            flag_notes,
        } => {
            merged.target = match text {
                i18n_harness_backend::TranslatedText::Singular(s) => {
                    Target::Singular { text: Some(s) }
                }
                i18n_harness_backend::TranslatedText::Plural(forms) => Target::Plural {
                    forms: forms.into_iter().map(Some).collect(),
                },
            };
            // M4.3a.1: translate always lands as Proposed; the human
            // explicitly promotes to Finished via save/accept.
            merged.state = UnitState::Proposed;
            let mut flagset = FlagSet::new();
            for f in flags {
                flagset.insert(f);
            }
            merged.flags = flagset;
            merged.confidence = confidence;
            merged.flag_notes = flag_notes;
        }
        TranslationOutcome::Skipped { reason } => {
            return Err(format!("backend skipped: {reason}"));
        }
        TranslationOutcome::Failed {
            reason,
            failure_kind: FailureKind::MalformedResponse,
            ..
        } => {
            // Surface as an inline hard gate finding so the Inspector renders
            // it next to the unit and the user can investigate the prompt.
            // No catalog state changes, no review event.
            let report = GateReport::backend_malformed_response(original.id.clone(), reason);
            return Ok(crate::dto::TranslateResult {
                unit: original,
                report,
            });
        }
        TranslationOutcome::Failed { reason, .. } => {
            return Err(format!("backend failed: {reason}"));
        }
    }

    let report = i18n_harness_gate::validate(&merged, locale, None);

    // M4.6.1: flagged units land in the review queue automatically. Set
    // `merged.review_status` on the in-memory unit BEFORE writing to the
    // catalog slot and BEFORE returning, so the UI sees the queued state
    // immediately rather than only after the next review-map fold.
    let needs_review = !merged.flags.is_empty();
    if needs_review {
        merged.review_status = Some(ReviewStatus::NeedsReview);
    }

    // 3. Merge under the catalog lock, then drop.
    {
        let mut store = project_catalogs
            .lock()
            .map_err(error::lock_poisoned("project_catalogs"))?;
        let entry = store
            .get_mut(abs)
            .ok_or_else(error::no_catalog_in_project)?;
        if let Some(slot) = entry.catalog.find_unit_mut(id) {
            *slot = merged.clone();
        }
        entry.dirty = true;
    }

    // 4. Durable review-event write (with project lock only).
    if needs_review {
        let source_hash = merged.source_hash.clone().unwrap_or_default();
        let project_guard = project.lock().map_err(error::lock_poisoned("project"))?;
        if let Some(project) = project_guard.as_ref() {
            project
                .set_review_status(
                    abs,
                    &merged.id,
                    Some(ReviewStatus::NeedsReview),
                    source_hash,
                    None,
                )
                .map_err(|e| format!("set_review_status failed: {e}"))?;
        }
    }

    Ok(crate::dto::TranslateResult {
        unit: merged,
        report,
    })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(all(test, feature = "ollama"))]
mod glossary_term_translation_tests {
    use super::{dispatch_glossary_term_translation, glossary_term_source};
    use i18n_harness_backend::{ManualBackend, ManualResponse};
    use i18n_harness_glossary::Glossary;
    use i18n_harness_locales::Locale;

    fn make_glossary() -> Glossary {
        let toml = r#"
[meta]
schema_version = 1

[[term]]
source = "Open"
do_not_translate = false
notes = "verb sense"
[term.translations]
de_DE = "Öffnen"

[[term]]
source = "ChromaCheck"
do_not_translate = true
"#;
        let (g, _) = Glossary::from_toml(toml).expect("parse");
        g
    }

    fn de_de() -> &'static Locale {
        Locale::by_id("de_DE").expect("de_DE locale must exist")
    }

    #[test]
    fn found_term_returns_canned_translation() {
        let glossary = make_glossary();
        let backend = ManualBackend::new(|ctx| {
            ManualResponse::Singular(format!("translated:{}", ctx.unit.source))
        });
        let result =
            dispatch_glossary_term_translation(&backend, "Open", de_de(), &glossary).unwrap();
        assert_eq!(result, "translated:Open");
    }

    #[test]
    fn dnt_term_returns_error() {
        let glossary = make_glossary();
        let err = glossary_term_source(&glossary, "ChromaCheck").unwrap_err();
        assert_eq!(err, "term is do-not-translate");
    }

    #[test]
    fn missing_term_returns_error() {
        let glossary = make_glossary();
        let err = glossary_term_source(&glossary, "NonExistent").unwrap_err();
        assert_eq!(err, "term not found");
    }

    #[test]
    fn backend_skip_propagates_as_error() {
        let glossary = make_glossary();
        let backend = ManualBackend::new(|_| ManualResponse::Skip);
        let err =
            dispatch_glossary_term_translation(&backend, "Open", de_de(), &glossary).unwrap_err();
        assert!(
            err.starts_with("backend skipped"),
            "expected 'backend skipped', got: {err}"
        );
    }

    #[test]
    fn backend_fail_propagates_as_error() {
        let glossary = make_glossary();
        let backend = ManualBackend::new(|_| ManualResponse::Fail {
            reason: "model-refused".to_owned(),
            retryable: false,
        });
        let err =
            dispatch_glossary_term_translation(&backend, "Open", de_de(), &glossary).unwrap_err();
        assert!(
            err.contains("model-refused"),
            "expected reason in error, got: {err}"
        );
    }
}

#[cfg(all(test, feature = "ollama"))]
mod batch_scope_tests {
    use super::{BatchScope, unit_matches_scope};
    use i18n_harness_core::{Unit, UnitState};

    fn unit_in(state: UnitState) -> Unit {
        let mut u = Unit::untranslated_singular("u1", "source");
        u.state = state;
        u
    }

    #[test]
    fn vanished_obsolete_and_finished_are_excluded_from_every_scope() {
        for &state in &[
            UnitState::Vanished,
            UnitState::Obsolete,
            UnitState::Finished,
        ] {
            let u = unit_in(state);
            assert!(!unit_matches_scope(&u, BatchScope::Untranslated));
            assert!(!unit_matches_scope(&u, BatchScope::UntranslatedAndProposed));
        }
    }

    #[test]
    fn untranslated_matches_every_scope() {
        let u = unit_in(UnitState::Untranslated);
        assert!(unit_matches_scope(&u, BatchScope::Untranslated));
        assert!(unit_matches_scope(&u, BatchScope::UntranslatedAndProposed));
    }

    #[test]
    fn proposed_matches_only_proposed_scope() {
        let u = unit_in(UnitState::Proposed);
        assert!(!unit_matches_scope(&u, BatchScope::Untranslated));
        assert!(unit_matches_scope(&u, BatchScope::UntranslatedAndProposed));
    }
}
