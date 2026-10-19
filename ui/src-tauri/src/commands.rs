//! Commands that read or change the open session.

use std::path::{Path, PathBuf};

use i18n_harness_locales::Locale;
use i18n_harness_session::{
    CatalogSummary, CheckSettings, FillReport, GlossaryEntry, ModelChoice, ModelSettings,
    SaveReport, Session, SessionError, SharedString, SiblingView, TermUsage, TranslateScope,
    UnitFilter, UnitRow, UnitView,
};
use serde::Serialize;

use crate::error::CommandResult;
use crate::state::AppState;

/// Version information for the About line.
#[derive(Debug, Serialize)]
pub(crate) struct AppInfo {
    version: &'static str,
    cldr: &'static str,
}

#[tauri::command]
pub(crate) fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        cldr: i18n_harness_locales::CLDR_VERSION,
    }
}

/// Where the user guide is published.
const USER_GUIDE_URL: &str = "https://vitalyvorobyev.github.io/i18n-harness/";

/// Open the user guide in the default browser.
#[tauri::command]
pub(crate) fn open_user_guide() -> CommandResult<()> {
    tauri_plugin_opener::open_url(USER_GUIDE_URL, None::<&str>)
        .map_err(|e| crate::error::CommandError::internal(e.to_string()))
}

/// Name of the folder the bundled sample is copied to.
const SAMPLE_FOLDER: &str = "i18n-harness sample";

/// Copy the bundled sample project into the user's Documents folder (their
/// home folder when there is none) and return its path. A copy made earlier
/// is reused as it is, with whatever work was done in it.
#[tauri::command]
pub(crate) fn create_sample(app: tauri::AppHandle) -> CommandResult<String> {
    use tauri::Manager;
    use tauri::path::BaseDirectory;

    let internal = |e: &dyn std::fmt::Display| crate::error::CommandError::internal(e.to_string());
    let paths = app.path();
    let source = paths
        .resolve("sample", BaseDirectory::Resource)
        .map_err(|e| internal(&e))?;
    let parent = paths
        .document_dir()
        .or_else(|_| paths.home_dir())
        .map_err(|e| internal(&e))?;
    let dest = parent.join(SAMPLE_FOLDER);
    if !dest.exists() {
        copy_dir(&source, &dest)
            .map_err(|e| internal(&format_args!("could not create {}: {e}", dest.display())))?;
    }
    Ok(dest.display().to_string())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// A language a user can pick.
#[derive(Debug, Serialize)]
pub(crate) struct Language {
    id: &'static str,
    name: &'static str,
}

#[tauri::command]
pub(crate) fn list_languages() -> Vec<Language> {
    let mut languages: Vec<Language> = Locale::all()
        .map(|l| Language {
            id: l.id,
            name: l.name,
        })
        .collect();
    languages.sort_by(|a, b| a.name.cmp(b.name));
    languages
}

/// An opened folder.
#[derive(Debug, Serialize)]
pub(crate) struct FolderView {
    root: String,
    catalogs: Vec<CatalogSummary>,
    warnings: Vec<String>,
}

#[tauri::command]
pub(crate) async fn open_folder(
    path: String,
    state: tauri::State<'_, AppState>,
) -> CommandResult<FolderView> {
    let session =
        tauri::async_runtime::spawn_blocking(move || Session::open_folder(Path::new(&path)))
            .await
            .map_err(|e| crate::error::CommandError::internal(e.to_string()))??;
    let view = FolderView {
        root: session.root().display().to_string(),
        catalogs: session.catalogs(),
        warnings: session.warnings().to_vec(),
    };
    *state.slot() = Some(session);
    Ok(view)
}

#[tauri::command]
pub(crate) fn close_folder(state: tauri::State<'_, AppState>) {
    state.cancel_job();
    *state.slot() = None;
}

#[tauri::command]
pub(crate) fn list_catalogs(
    state: tauri::State<'_, AppState>,
) -> CommandResult<Vec<CatalogSummary>> {
    state.with(|s| Ok(s.catalogs()))
}

/// The strings of one catalog (`path`) or of every catalog (`None`) that
/// match `filter`.
#[tauri::command]
pub(crate) fn list_rows(
    path: Option<String>,
    filter: UnitFilter,
    state: tauri::State<'_, AppState>,
) -> CommandResult<Vec<UnitRow>> {
    state.with(|s| Ok(s.rows(path.as_deref().map(Path::new), &filter)?))
}

#[tauri::command]
pub(crate) fn get_unit(
    path: String,
    id: String,
    state: tauri::State<'_, AppState>,
) -> CommandResult<UnitView> {
    state.with(|s| Ok(s.unit(Path::new(&path), &id)?))
}

/// The same string in the other files of its language.
#[tauri::command]
pub(crate) fn siblings(
    path: String,
    id: String,
    state: tauri::State<'_, AppState>,
) -> CommandResult<Vec<SiblingView>> {
    state.with(|s| Ok(s.siblings(Path::new(&path), &id)?))
}

/// Every string that occurs in more than one file of its language.
#[tauri::command]
pub(crate) fn shared_strings(
    state: tauri::State<'_, AppState>,
) -> CommandResult<Vec<SharedString>> {
    state.with(|s| Ok(s.shared_strings()))
}

/// Copy a string's translation to the same string in every other file.
#[tauri::command]
pub(crate) fn use_everywhere(
    path: String,
    id: String,
    state: tauri::State<'_, AppState>,
) -> CommandResult<usize> {
    state.with(|s| Ok(s.use_everywhere(Path::new(&path), &id)?))
}

/// Fill strings to do from what other files agree on (`apply` false: only
/// count).
#[tauri::command]
pub(crate) fn fill_from_other_files(
    paths: Vec<PathBuf>,
    apply: bool,
    state: tauri::State<'_, AppState>,
) -> CommandResult<FillReport> {
    state.with(|s| Ok(s.fill_from_other_files(&paths, apply)?))
}

#[tauri::command]
pub(crate) fn get_check_settings(
    locale: String,
    state: tauri::State<'_, AppState>,
) -> CommandResult<CheckSettings> {
    state.with(|s| Ok(s.check_settings(&locale)?))
}

#[tauri::command]
pub(crate) fn set_check_settings(
    locale: String,
    settings: CheckSettings,
    state: tauri::State<'_, AppState>,
) -> CommandResult<CheckSettings> {
    state.with(|s| {
        s.set_check_settings(&locale, settings)?;
        Ok(s.check_settings(&locale)?)
    })
}

#[tauri::command]
pub(crate) fn set_translation(
    path: String,
    id: String,
    forms: Vec<String>,
    done: bool,
    state: tauri::State<'_, AppState>,
) -> CommandResult<UnitView> {
    state.with(|s| Ok(s.set_translation(Path::new(&path), &id, forms, done)?))
}

#[tauri::command]
pub(crate) fn set_done(
    path: String,
    ids: Vec<String>,
    done: bool,
    state: tauri::State<'_, AppState>,
) -> CommandResult<Vec<UnitView>> {
    state.with(|s| Ok(s.set_done(Path::new(&path), &ids, done)?))
}

#[tauri::command]
pub(crate) fn save(
    path: Option<String>,
    state: tauri::State<'_, AppState>,
) -> CommandResult<SaveReport> {
    state.with(|s| Ok(s.save(path.as_deref().map(Path::new))?))
}

#[tauri::command]
pub(crate) fn set_pivot(
    path: String,
    pivot: Option<String>,
    state: tauri::State<'_, AppState>,
) -> CommandResult<CatalogSummary> {
    let path = PathBuf::from(path);
    state.with(|s| {
        s.set_pivot(&path, pivot.as_deref())?;
        Ok(s.catalog_summary(&path)?)
    })
}

#[tauri::command]
pub(crate) fn get_glossary(
    locale: String,
    state: tauri::State<'_, AppState>,
) -> CommandResult<Vec<GlossaryEntry>> {
    state.with(|s| Ok(s.glossary(&locale)))
}

#[tauri::command]
pub(crate) fn set_glossary(
    locale: String,
    entries: Vec<GlossaryEntry>,
    state: tauri::State<'_, AppState>,
) -> CommandResult<Vec<GlossaryEntry>> {
    state.with(|s| {
        s.set_glossary(&locale, entries)?;
        Ok(s.glossary(&locale))
    })
}

/// How the strings of `locale` use each glossary term.
#[tauri::command]
pub(crate) fn glossary_usage(
    locale: String,
    state: tauri::State<'_, AppState>,
) -> CommandResult<Vec<TermUsage>> {
    state.with(|s| Ok(s.glossary_usage(&locale)))
}

/// The terms of a table file (CSV or tab separated), for the glossary
/// editor to merge into its draft.
#[tauri::command]
pub(crate) fn read_glossary_table(path: PathBuf) -> CommandResult<Vec<GlossaryEntry>> {
    let table = std::fs::read_to_string(&path).map_err(|source| SessionError::Io {
        context: format!("cannot read {}", path.display()),
        source,
    })?;
    Ok(i18n_harness_session::glossary_table_entries(&table)?)
}

/// How many strings a translation run over `scope` would cover.
#[tauri::command]
pub(crate) fn plan_size(
    scope: TranslateScope,
    state: tauri::State<'_, AppState>,
) -> CommandResult<usize> {
    state.with(|s| Ok(s.plan(&scope)?.len()))
}

#[tauri::command]
pub(crate) fn get_model_settings(
    state: tauri::State<'_, AppState>,
) -> CommandResult<ModelSettings> {
    state.with(|s| Ok(model_settings(s)))
}

#[tauri::command]
pub(crate) fn set_model_settings(
    settings: ModelSettings,
    state: tauri::State<'_, AppState>,
) -> CommandResult<ModelSettings> {
    state.with(|s| {
        s.project_mut()
            .set_backend(settings.to_config())
            .map_err(i18n_harness_session::SessionError::from)?;
        s.project()
            .save_manifest()
            .map_err(i18n_harness_session::SessionError::from)?;
        Ok(model_settings(s))
    })
}

/// Models the server in `settings` offers. Works without an open folder, so
/// the first screen can show whether a model server is running.
#[tauri::command]
pub(crate) async fn list_models(settings: ModelSettings) -> CommandResult<Vec<ModelChoice>> {
    tauri::async_runtime::spawn_blocking(move || i18n_harness_session::list_models(&settings))
        .await
        .map_err(|e| crate::error::CommandError::internal(e.to_string()))?
        .map_err(Into::into)
}

pub(crate) fn model_settings(session: &Session) -> ModelSettings {
    ModelSettings::from_config(session.project().manifest().backends.default.as_ref())
}
