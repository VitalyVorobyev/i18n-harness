//! Translation commands (project-routed unit translate, glossary-term
//! translate, bulk batch with cancellation).
//!
//! `cancel_translation` is always-on (not cfg-ollama) so the UI can always
//! send a cancel even when the ollama feature is absent at build time.
//! The ollama-gated commands and the `run_batch_worker` body follow below.

use std::path::PathBuf;

use i18n_harness_core::UnitId;

use crate::error;
use crate::state::AppState;

/// Signal cancellation for an in-flight `translate_batch_in_project` job.
///
/// Returns `true` if a job with that id was found (the cancellation flag
/// is now set; the worker will observe it before its next unit), `false`
/// if no such job is running. Idempotent: double-cancel is a no-op.
/// Callers should still wait for the terminal Tauri event — cancellation
/// is cooperative, so the currently-running unit will complete before the
/// worker exits.
///
/// Available regardless of the `ollama` feature so the UI can always cancel.
#[tauri::command]
pub(crate) fn cancel_translation(job_id: String, state: tauri::State<'_, AppState>) -> bool {
    state.jobs.cancel(&job_id)
}

/// Translate one unit through the currently-open project, using the project's
/// glossary, locale config, and default backend. Only the project-catalog store
/// is updated; the singular file-centric `catalog` slot is not touched.
///
/// Requires the catalog to be open in the project store (call
/// `open_catalog_in_project` first). Returns the merged unit plus the gate
/// report.
///
/// Available only when the crate is built with the `ollama` feature.
#[cfg(feature = "ollama")]
#[tauri::command]
pub(crate) fn translate_unit_in_project(
    catalog_path: String,
    unit_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<crate::dto::TranslateResult, String> {
    use i18n_harness_backend::TranslationBackend;

    let abs = PathBuf::from(&catalog_path);
    let id = UnitId::from(unit_id);

    let (locale, glossary, ollama_settings) =
        crate::services::translate::resolve_project_translate_context(&state, &abs)?;
    let backend = crate::services::translate::build_ollama_backend(&ollama_settings)?;
    let backend_name = backend.name().to_string();

    crate::services::translate::translate_one(
        &state.project_catalogs,
        &state.project,
        &abs,
        &id,
        &backend,
        &backend_name,
        locale,
        glossary.as_ref(),
    )
}

/// Propose a translation for one glossary term using the project's default
/// backend.
///
/// Looks up the term by `term_id` (the case-sensitive source key) in the
/// project's glossary, refuses DNT terms and missing terms, then calls the
/// backend with a minimal synthetic unit whose source text is the term's
/// source string.
///
/// Returns the proposed translation string on success. Does not write anything
/// to disk — the caller decides whether to accept and persist the proposal.
///
/// Available only when the crate is built with the `ollama` feature.
#[cfg(feature = "ollama")]
#[tauri::command]
pub(crate) async fn translate_glossary_term(
    state: tauri::State<'_, AppState>,
    project_path: String,
    term_id: String,
    target_locale: String,
) -> Result<String, String> {
    use i18n_harness_locales::Locale;
    use i18n_harness_project::BackendKind;

    // Snapshot the glossary, term, locale, and backend settings under the
    // project lock, then drop the lock before the network call.
    let (term_source, glossary, locale, ollama_settings) = {
        let abs = PathBuf::from(&project_path);
        let project_guard = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        let project = project_guard.as_ref().ok_or_else(error::no_project)?;

        // Validate the caller is referencing the currently-open project.
        if project.paths().root() != abs {
            return Err(format!(
                "project at `{project_path}` is not the currently-open project"
            ));
        }

        let glossary = project
            .glossary()
            .cloned()
            .ok_or_else(|| "no glossary configured for this project".to_string())?;

        let locale = Locale::by_id(&target_locale)
            .ok_or_else(|| format!("unknown locale `{target_locale}`; add it to crates/locales"))?;

        let ollama_settings = if let Some(backend_cfg) = &project.manifest().backends.default {
            if backend_cfg.kind != BackendKind::Ollama {
                return Err(format!(
                    "backend kind {:?} not supported yet",
                    backend_cfg.kind,
                ));
            }
            crate::services::translate::OllamaSettings {
                model: backend_cfg.model.clone(),
                host: backend_cfg.host.clone(),
                num_ctx: backend_cfg.num_ctx,
            }
        } else {
            crate::services::translate::OllamaSettings::default()
        };

        let term_source = crate::services::translate::glossary_term_source(&glossary, &term_id)?;
        (term_source, glossary, locale, ollama_settings)
    };

    let backend = crate::services::translate::build_ollama_backend(&ollama_settings)?;
    crate::services::translate::dispatch_glossary_term_translation(
        &backend,
        &term_source,
        locale,
        &glossary,
    )
}

/// Start a bulk translation of every in-scope unit in a project-stored catalog.
///
/// Resolves locale, glossary, and backend kind exactly like
/// `translate_unit_in_project`, refuses to start when another bulk run is
/// already operating on the same `(catalog, locale)` pair, then spawns a
/// background thread that processes units sequentially and streams progress
/// via Tauri events. Returns immediately; the worker emits the terminal event
/// when it exits.
///
/// Available only when the crate is built with the `ollama` feature.
#[cfg(feature = "ollama")]
#[tauri::command]
pub(crate) fn translate_batch_in_project(
    catalog_path: String,
    scope: crate::dto::BatchScope,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<crate::dto::TranslateBatchStarted, String> {
    use i18n_harness_backend::TranslationBackend;

    let abs = PathBuf::from(&catalog_path);

    // 1. Resolve locale / glossary / backend kind via the shared helper.
    let (locale, glossary, ollama_settings) =
        crate::services::translate::resolve_project_translate_context(&state, &abs)?;
    let locale_id = locale.id.to_string();

    // 2. Collect the units that match `scope` (and exist in the open catalog
    //    store). Vanished/Obsolete are always excluded.
    let unit_ids: Vec<UnitId> = {
        let store = state
            .project_catalogs
            .lock()
            .map_err(error::lock_poisoned("project_catalogs"))?;
        let entry = store.get(&abs).ok_or_else(error::no_catalog_in_project)?;
        entry
            .catalog
            .units()
            .iter()
            .filter(|u| crate::services::translate::unit_matches_scope(u, scope))
            .map(|u| u.id.clone())
            .collect()
    };
    let total = unit_ids.len();

    // 3. Refuse concurrent bulk runs on the same (catalog, locale) pair.
    let active_slot = crate::jobs::BatchSlot::Catalog {
        abs: abs.clone(),
        locale: locale_id.clone(),
    };
    state.active_batches.try_claim(&active_slot)?;

    // 4. Construct the backend on the calling thread so config errors surface
    //    synchronously; if this fails we release the active slot before returning.
    let backend = match crate::services::translate::build_ollama_backend(&ollama_settings) {
        Ok(b) => b,
        Err(e) => {
            // Release the active slot we just claimed.
            state.active_batches.release(&active_slot);
            return Err(e);
        }
    };
    let backend_name = backend.name().to_string();

    // 5. Register the job in the cancellation registry.
    let (job_id, token) = state.jobs.register();

    // 6. Spawn the worker. The worker re-fetches the AppState from the
    //    AppHandle each time it needs a lock; this keeps the thread free of
    //    any borrow from `state` (which is bound to the command lifetime).
    let worker_app = app.clone();
    let worker_glossary = glossary.clone();
    let worker_job_id = job_id.clone();
    let worker_abs = abs.clone();
    let worker_unit_ids = unit_ids;
    // Clone the slot for the spawn-failure cleanup path; the original moves
    // into the worker.
    let spawn_fail_slot = active_slot.clone();
    let worker_active_slot = active_slot;

    std::thread::Builder::new()
        .name(format!("translate-batch-{job_id}"))
        .spawn(move || {
            run_batch_worker(
                worker_app,
                worker_job_id,
                worker_abs,
                worker_unit_ids,
                total,
                backend,
                backend_name,
                locale,
                worker_glossary,
                token,
                worker_active_slot,
            );
        })
        .map_err(|e| {
            // Failed to spawn — undo the registry + active-batches inserts.
            state.jobs.deregister(&job_id);
            state.active_batches.release(&spawn_fail_slot);
            format!("failed to spawn translate-batch worker: {e}")
        })?;

    Ok(crate::dto::TranslateBatchStarted { job_id, total })
}

/// Run the per-unit translate loop on a worker thread, emitting Tauri events
/// for each completed unit and a single terminal event before exiting.
///
/// Cleanup (job deregister + slot release) is handled via a
/// [`ScopedJobRegistration`][crate::jobs::ScopedJobRegistration] guard so it
/// happens on every exit path — including if future code adds an early return
/// — without repeating the two-step cleanup.
#[cfg(feature = "ollama")]
#[allow(clippy::too_many_arguments)]
fn run_batch_worker(
    app: tauri::AppHandle,
    job_id: String,
    abs: PathBuf,
    unit_ids: Vec<UnitId>,
    total: usize,
    backend: i18n_harness_backend::OllamaBackend,
    backend_name: String,
    locale: &'static i18n_harness_locales::Locale,
    glossary: Option<i18n_harness_glossary::Glossary>,
    token: crate::cancellation::CancellationToken,
    active_slot: crate::jobs::BatchSlot,
) {
    use tauri::{Emitter, Manager};

    let state = app.state::<AppState>();
    // Adopt ownership of the already-registered job and already-claimed slot.
    // Drop releases both when this function returns, on every exit path.
    let _guard = crate::jobs::ScopedJobRegistration::adopt(
        &state.jobs,
        &state.active_batches,
        job_id.clone(),
        token.clone(),
        active_slot,
    );
    let mut completed: usize = 0;
    let mut terminal = crate::dto::BatchTerminalPayload {
        completed: 0,
        total,
        cancelled: false,
        failed_reason: None,
    };

    for unit_id in &unit_ids {
        // Cooperative cancellation check between units. The currently-running
        // network call (if any) is not interruptible — it runs to completion.
        if token.is_cancelled() {
            terminal.cancelled = true;
            break;
        }

        let started_payload = crate::dto::BatchUnitStartedPayload {
            unit_id: unit_id.as_str().to_owned(),
            locale: locale.id.to_owned(),
        };
        if let Err(e) = app.emit(&format!("batch-unit-started-{job_id}"), &started_payload) {
            tracing::warn!(job_id = %job_id, error = %e, "batch-unit-started emit failed");
        }

        match crate::services::translate::translate_one(
            &state.project_catalogs,
            &state.project,
            &abs,
            unit_id,
            &backend,
            &backend_name,
            locale,
            glossary.as_ref(),
        ) {
            Ok(result) => {
                completed += 1;
                let payload = crate::dto::BatchProgressPayload {
                    completed,
                    total,
                    flagged: !result.unit.flags.is_empty(),
                    unit: result.unit,
                };
                // Event emission can fail if all webviews are gone (app shutting
                // down); log and keep going. The terminal event will also be a
                // best-effort send.
                if let Err(e) = app.emit(&format!("batch-progress-{job_id}"), &payload) {
                    tracing::warn!(job_id = %job_id, error = %e, "batch-progress emit failed");
                }
            }
            Err(reason) => {
                terminal.failed_reason = Some(reason);
                break;
            }
        }
    }

    terminal.completed = completed;

    let event_name = if terminal.failed_reason.is_some() {
        format!("batch-failed-{job_id}")
    } else {
        format!("batch-completed-{job_id}")
    };
    if let Err(e) = app.emit(&event_name, &terminal) {
        tracing::warn!(job_id = %job_id, error = %e, "batch terminal emit failed");
    }

    // `_guard` drops here — deregisters the job and releases the slot via Drop.
}
