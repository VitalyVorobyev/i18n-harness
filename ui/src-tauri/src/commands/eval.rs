//! In-app prompt evaluation commands.
//!
//! `run_evaluation_in_project` is cfg-ollama.
//! `list_evaluation_runs_in_project` is always-on.

use crate::error;
use crate::state::AppState;

/// Evaluate the current prompt over every curated example.
///
/// Resolves the curated set, refuses if it is empty, builds an `OllamaBackend`,
/// registers a job, and spawns a worker thread. The worker translates each
/// example's source text, compares the result to `human_target` (exact-match
/// after `.trim()`), and emits `eval-progress-<job_id>` after each comparison.
/// After the loop it persists the run via `Project::evaluations().append()` and
/// emits `eval-completed-<job_id>`. On mid-batch hard failure it emits
/// `eval-failed-<job_id>` without persisting a partial run.
///
/// Cancellation: `cancel_translation` with this job's id sets the token; the
/// worker observes it between examples and emits `eval-completed-<job_id>` with
/// `cancelled: true` and no run.
///
/// Available only when the crate is built with the `ollama` feature.
#[cfg(feature = "ollama")]
#[tauri::command]
pub(crate) fn run_evaluation_in_project(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<crate::dto::EvaluationStarted, String> {
    use i18n_harness_backend::{OllamaBackend, TranslationBackend, TranslationOutcome};
    use i18n_harness_core::{Batch, BatchKey};
    use i18n_harness_locales::Locale;
    use i18n_harness_project::{Correction, EvaluationRun, ScoreAccumulator, flags_to_strings};

    // 1. Collect everything we need from the project under a brief lock.
    // Each element: (source, human_target, locale_id, flag_strs).
    type EvalExample = (String, String, String, Vec<String>);
    type LocaleEntry = (String, &'static Locale);
    // Capture the project's EvaluationStore here so the worker thread persists
    // to the project that LAUNCHED the eval, even if the global project slot
    // is replaced via open_project / close_project mid-flight (codex P1 on
    // PR #36).
    let (examples_with_corrections, glossary, locale_map, eval_store): (
        Vec<EvalExample>,
        Option<i18n_harness_glossary::Glossary>,
        Vec<LocaleEntry>,
        i18n_harness_project::EvaluationStore,
    ) = {
        let project_guard = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        let project = project_guard.as_ref().ok_or_else(error::no_project)?;

        // Resolve all curated examples that have a backing correction.
        let curated = project.curated();
        if curated.is_empty() {
            return Err("No curated examples yet; promote some corrections first".to_string());
        }

        // Validate backend kind.
        if let Some(backend_cfg) = &project.manifest().backends.default {
            use i18n_harness_project::BackendKind;
            if backend_cfg.kind != BackendKind::Ollama {
                return Err(format!(
                    "backend kind {:?} not supported for evaluation; only ollama is supported",
                    backend_cfg.kind,
                ));
            }
        }

        let mut examples = Vec::new();
        let mut locale_map_build: std::collections::BTreeMap<String, &'static Locale> =
            std::collections::BTreeMap::new();

        // Read all corrections once for resolution.
        let (all_corrections, _) = project
            .corrections()
            .read_all()
            .map_err(|e| e.to_string())?;

        for ex in curated.examples() {
            let correction: Option<&Correction> = all_corrections.iter().find(|c| c.id == ex.id);
            let Some(corr) = correction else {
                // Dangling curated entry — skip silently.
                continue;
            };

            let locale_id = &corr.locale;
            // Resolve Locale lazily; skip if unknown.
            let locale = project
                .locale(locale_id)
                .map(|r| r.workspace_locale())
                .or_else(|| Locale::by_id(locale_id));
            let Some(locale) = locale else {
                tracing::warn!(locale = %locale_id, "run_evaluation: unknown locale, example skipped");
                continue;
            };

            let flag_strs = flags_to_strings(&corr.flags_at_correction);
            examples.push((
                corr.source.clone(),
                corr.human_target.clone(),
                locale_id.clone(),
                flag_strs,
            ));
            locale_map_build.insert(locale_id.clone(), locale);
        }

        if examples.is_empty() {
            return Err(
                "No resolvable curated examples; all entries are dangling or use unknown locales"
                    .to_string(),
            );
        }

        let glossary = project.glossary().cloned();
        let locale_vec: Vec<LocaleEntry> = locale_map_build.into_iter().collect();
        let eval_store = project.evaluations().clone();

        (examples, glossary, locale_vec, eval_store)
    };

    let total = examples_with_corrections.len();

    // 2. Refuse if any evaluation job is already running. The typed eval slot
    //    replaces the old ("__eval__", "__eval__") magic key.
    let eval_slot = crate::jobs::BatchSlot::Eval;
    state.active_batches.try_claim(&eval_slot)?;

    // 3. Construct backend on the calling thread.
    let backend = match OllamaBackend::new() {
        Ok(b) => b,
        Err(e) => {
            state.active_batches.release(&eval_slot);
            return Err(format!("ollama backend construction failed: {e}"));
        }
    };

    // 4. Register the job.
    let (job_id, token) = state.jobs.register();

    // 5. Build a (locale_id → &'static Locale) map for the worker. The worker
    //    needs this but cannot hold the project lock.
    let locale_lookup: std::collections::BTreeMap<String, &'static Locale> =
        locale_map.into_iter().collect();

    // 6. Spawn the worker.
    let worker_app = app.clone();
    let worker_job_id = job_id.clone();
    let worker_eval_slot = eval_slot.clone();
    // Capture the originating project's EvaluationStore so the run is written
    // to it regardless of any open_project / close_project that happens
    // mid-flight (codex P1 on PR #36).
    let worker_eval_store = eval_store;

    std::thread::Builder::new()
        .name(format!("eval-{job_id}"))
        .spawn(move || {
            // flag and locale resolution already done on the calling thread
            use tauri::{Emitter, Manager};

            let app_state = worker_app.state::<AppState>();
            // Adopt ownership of the already-registered job and already-claimed
            // slot. Drop releases both when this closure returns, on every path.
            let _guard = crate::jobs::ScopedJobRegistration::adopt(
                &app_state.jobs,
                &app_state.active_batches,
                worker_job_id.clone(),
                token.clone(),
                worker_eval_slot,
            );
            let mut accumulator = ScoreAccumulator::new();
            let mut completed: usize = 0;
            let mut failed_reason: Option<String> = None;
            let mut cancelled = false;

            'outer: for (source, human_target, locale_id, flag_strs) in
                examples_with_corrections
            {
                // Cooperative cancellation.
                if token.is_cancelled() {
                    cancelled = true;
                    break;
                }

                let locale = match locale_lookup.get(&locale_id) {
                    Some(l) => *l,
                    None => {
                        // Should not happen — we validated above.
                        tracing::warn!(locale = %locale_id, "eval worker: locale disappeared, skipping");
                        continue;
                    }
                };

                // Build a one-unit batch. We only need the source; the target
                // is left blank because we want the LLM's fresh output.
                let unit = i18n_harness_core::Unit::untranslated_singular(source.as_str(), source.as_str());
                let batch = Batch::new(BatchKey::new("eval", 0), vec![unit.clone()]);

                let outcomes = match backend.translate_batch(
                    &batch,
                    locale,
                    glossary.as_ref(),
                ) {
                    Ok(o) => o,
                    Err(e) => {
                        failed_reason = Some(format!("backend failed: {e}"));
                        break 'outer;
                    }
                };

                let outcome = match outcomes.into_iter().next() {
                    Some(o) => o,
                    None => {
                        failed_reason = Some("backend returned no outcomes".to_string());
                        break 'outer;
                    }
                };

                let llm_output = match outcome {
                    TranslationOutcome::Translated { text, .. } => {
                        match text {
                            i18n_harness_backend::TranslatedText::Singular(s) => s,
                            i18n_harness_backend::TranslatedText::Plural(forms) => {
                                forms.into_iter().next().unwrap_or_default()
                            }
                        }
                    }
                    TranslationOutcome::Skipped { reason } => {
                        failed_reason = Some(format!("backend skipped: {reason}"));
                        break 'outer;
                    }
                    TranslationOutcome::Failed { reason, .. } => {
                        failed_reason = Some(format!("backend failed: {reason}"));
                        break 'outer;
                    }
                };

                // Exact-match scoring after trim.
                let score = if llm_output.trim() == human_target.trim() {
                    1.0_f32
                } else {
                    0.0_f32
                };

                accumulator.record(&locale_id, &flag_strs, score);
                completed += 1;

                let progress = crate::dto::EvaluationProgressPayload {
                    job_id: worker_job_id.clone(),
                    completed,
                    total,
                    last_example_locale: locale_id.clone(),
                };
                if let Err(e) =
                    worker_app.emit(&format!("eval-progress-{}", worker_job_id), &progress)
                {
                    tracing::warn!(job_id = %worker_job_id, error = %e, "eval-progress emit failed");
                }
            }

            // Build the terminal payload.
            let run: Option<EvaluationRun> = if failed_reason.is_none() && !cancelled {
                use time::OffsetDateTime;
                use time::format_description::well_known::Rfc3339;
                let ts = OffsetDateTime::now_utc()
                    .format(&Rfc3339)
                    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
                let run = accumulator.finish(ts, "ollama-translate-v2".to_string());

                // Persist the run via the EvaluationStore captured at spawn
                // time — NOT app_state.project, which may have been swapped
                // since this worker started. This guarantees the run lands in
                // the project that launched it.
                if let Err(e) = worker_eval_store.append(&run) {
                    tracing::warn!(
                        job_id = %worker_job_id,
                        error = %e,
                        "failed to persist evaluation run"
                    );
                }
                Some(run)
            } else {
                None
            };

            let terminal = crate::dto::EvaluationTerminalPayload {
                job_id: worker_job_id.clone(),
                cancelled,
                failed_reason: failed_reason.clone(),
                run,
            };

            let event_name = if failed_reason.is_some() {
                format!("eval-failed-{}", worker_job_id)
            } else {
                format!("eval-completed-{}", worker_job_id)
            };
            if let Err(e) = worker_app.emit(&event_name, &terminal) {
                tracing::warn!(job_id = %worker_job_id, error = %e, "eval terminal emit failed");
            }

            // `_guard` drops here — deregisters the job and releases the eval
            // slot via Drop.
        })
        .map_err(|e| {
            state.jobs.deregister(&job_id);
            state.active_batches.release(&eval_slot);
            format!("failed to spawn evaluation worker: {e}")
        })?;

    Ok(crate::dto::EvaluationStarted { job_id, total })
}

/// List all past evaluation runs for the current project, newest-first.
///
/// Returns an empty vec if no project is open or if no runs have been
/// recorded yet.
#[tauri::command]
pub(crate) fn list_evaluation_runs_in_project(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<i18n_harness_project::EvaluationRun>, String> {
    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;
    let mut runs = project.evaluations().list().map_err(|e| e.to_string())?;
    // Return newest-first.
    runs.reverse();
    Ok(runs)
}
