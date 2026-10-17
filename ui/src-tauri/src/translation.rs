//! Machine translation from the desktop shell.
//!
//! A run works one string at a time and holds the session lock only to
//! prepare a request and to merge its answer, never while the model works,
//! so the window stays responsive. Each finished string is sent to the
//! frontend as a `translation` event carrying the updated row.

use std::path::{Path, PathBuf};

use i18n_harness_core::UnitId;
use i18n_harness_session::{
    CatalogSummary, InputText, Memo, Progress, Promotion, TranslateScope, UnitView,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::model_settings;
use crate::error::{CommandError, CommandResult};
use crate::state::AppState;

/// Event name of [`TranslationEvent`].
const EVENT: &str = "translation";

/// One progress event of a run.
#[derive(Debug, Clone, Serialize)]
struct TranslationEvent {
    progress: Progress,
    /// The string after the merge (for `unit` events).
    unit: Option<UnitView>,
    /// Its catalog's updated counts (for `unit` events).
    catalog: Option<CatalogSummary>,
}

/// Translate one string now (the "Translate again" action).
#[tauri::command]
pub(crate) async fn translate_one(
    path: String,
    id: String,
    input: Option<InputText>,
    app: AppHandle,
) -> CommandResult<UnitView> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (task, settings) = state.with(|s| {
            let task = s.prepare(
                Path::new(&path),
                &UnitId::from(id.as_str()),
                input.unwrap_or_default(),
            )?;
            Ok((task, model_settings(s)))
        })?;
        let backend = i18n_harness_session::backend(&settings)?;
        let outcome = i18n_harness_session::translate_task(backend.as_ref(), &task)?;
        state.with(|s| {
            let applied = s.apply(&task, outcome, Promotion::Review)?;
            match applied.failure {
                Some(reason) => Err(CommandError::from(
                    i18n_harness_session::SessionError::Backend(
                        i18n_harness_backend::BackendError::Protocol {
                            backend: settings.model.clone().unwrap_or_default(),
                            message: reason,
                        },
                    ),
                )),
                None => Ok(applied.unit),
            }
        })
    })
    .await
    .map_err(|e| CommandError::internal(e.to_string()))?
}

/// Start translating everything in `scope` in the background. Progress
/// arrives as `translation` events; the last one is `finished`.
#[tauri::command]
pub(crate) fn start_translation(
    scope: TranslateScope,
    input: Option<InputText>,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> CommandResult<usize> {
    let (plan, settings) = state.with(|s| Ok((s.plan(&scope)?, model_settings(s))))?;
    // Fail fast (no model selected, unsupported server) before the thread.
    let backend = i18n_harness_session::backend(&settings)?;
    let cancel = state.begin_job()?;
    let total = plan.len();
    let input = input.unwrap_or_default();
    let spawned = std::thread::Builder::new()
        .name("translation".into())
        .spawn(move || {
            let state = app.state::<AppState>();
            run(&app, &state, plan, input, backend.as_ref(), &cancel);
            state.end_job();
        });
    if let Err(e) = spawned {
        state.end_job();
        return Err(CommandError::internal(format!(
            "cannot start translation: {e}"
        )));
    }
    Ok(total)
}

/// Stop the running translation at the next string.
#[tauri::command]
pub(crate) fn cancel_translation(state: tauri::State<'_, AppState>) {
    state.cancel_job();
}

fn run(
    app: &AppHandle,
    state: &AppState,
    plan: Vec<(PathBuf, UnitId)>,
    input: InputText,
    backend: &dyn i18n_harness_backend::TranslationBackend,
    cancel: &i18n_harness_session::CancelToken,
) {
    let emit = |progress: Progress, unit: Option<UnitView>, catalog: Option<CatalogSummary>| {
        let _ = app.emit(
            EVENT,
            TranslationEvent {
                progress,
                unit,
                catalog,
            },
        );
    };
    let total = plan.len();
    emit(Progress::Started { total }, None, None);
    let mut memo = Memo::new();
    let (mut translated, mut failed) = (0, 0);
    for (done, (path, id)) in plan.into_iter().enumerate() {
        if cancel.is_cancelled() {
            emit(
                Progress::Finished {
                    translated,
                    failed,
                    cancelled: true,
                },
                None,
                None,
            );
            return;
        }
        let step = (|| -> CommandResult<(UnitView, CatalogSummary, Option<String>)> {
            let task = state.with(|s| Ok(s.prepare(&path, &id, input)?))?;
            let (outcome, _) = memo.translate(backend, &task)?;
            state.with(|s| {
                let applied = s.apply(&task, outcome, Promotion::Review)?;
                Ok((applied.unit, s.catalog_summary(&path)?, applied.failure))
            })
        })();
        match step {
            Ok((unit, catalog, failure)) => {
                if failure.is_some() {
                    failed += 1;
                } else {
                    translated += 1;
                }
                emit(
                    Progress::Unit {
                        catalog: path.display().to_string(),
                        id: id.0.clone(),
                        done: done + 1,
                        total,
                        translated: failure.is_none(),
                        failure,
                    },
                    Some(unit),
                    Some(catalog),
                );
            }
            Err(error) => {
                // A server-level failure (stopped, unreachable): end the run
                // and say why, instead of failing every remaining string.
                failed += 1;
                emit(
                    Progress::Unit {
                        catalog: path.display().to_string(),
                        id: id.0.clone(),
                        done: done + 1,
                        total,
                        translated: false,
                        failure: Some(serde_json::to_string(&error).unwrap_or_default()),
                    },
                    None,
                    None,
                );
                emit(
                    Progress::Finished {
                        translated,
                        failed,
                        cancelled: true,
                    },
                    None,
                    None,
                );
                return;
            }
        }
    }
    emit(
        Progress::Finished {
            translated,
            failed,
            cancelled: false,
        },
        None,
        None,
    );
}
