//! Commands for glossary and metrics loading/saving.

use std::collections::BTreeMap;
use std::path::PathBuf;

use i18n_harness_glossary::Glossary;
use i18n_harness_project::GlossaryConfig;

use crate::commands::project_lifecycle::open_project_impl;
use crate::dto::{
    GlossaryLoadResponse, GlossaryPayload, GlossarySaveResponse, LocaleOverrideEntry, MetricEvent,
    MetricsResponse, ProjectOpenResponse, TermEntry,
};
use crate::error;
use crate::services::glossary_io::payload_to_toml;
use crate::state::AppState;

/// Load a glossary `.toml` from `path`. Returns the editable payload
/// and any non-fatal warnings (unknown locale, empty translations).
///
/// Side effect: stashes the parsed glossary in [`AppState`] so the
/// next `translate_unit` call passes it to the backend. Loading a new
/// glossary replaces the previous one.
#[tauri::command]
pub(crate) fn load_glossary(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<GlossaryLoadResponse, String> {
    let abs = PathBuf::from(&path);
    let (glossary, warnings) = Glossary::load(&abs).map_err(|e| format!("load failed: {e}"))?;
    let terms = glossary
        .terms()
        .map(|(src, t)| TermEntry {
            source: src.to_string(),
            do_not_translate: t.do_not_translate,
            notes: t.notes.clone(),
            translations: t.translations.clone(),
        })
        .collect();
    let locale_overrides = glossary
        .overrides()
        .map(|(id, ov)| LocaleOverrideEntry {
            locale: id.to_string(),
            register: ov.register.map(|r| r.as_str().to_owned()),
            variant: ov.variant.clone(),
        })
        .collect();
    let response = GlossaryLoadResponse {
        path: abs.to_string_lossy().into_owned(),
        payload: GlossaryPayload {
            schema_version: glossary.schema_version(),
            terms,
            locale_overrides,
        },
        warnings: warnings.into_iter().map(|w| w.to_string()).collect(),
    };
    *state
        .glossary
        .lock()
        .map_err(error::lock_poisoned("glossary"))? = Some(glossary);
    Ok(response)
}

/// Validate the payload by round-tripping through `Glossary::from_toml`
/// and then write the resulting (deterministic, alphabetical) TOML to
/// `path`. Refuses to write if validation fails.
#[tauri::command]
pub(crate) fn save_glossary(
    path: String,
    payload: GlossaryPayload,
) -> Result<GlossarySaveResponse, String> {
    let abs = PathBuf::from(&path);
    let toml_string = payload_to_toml(&payload)?;
    let (_, warnings) = i18n_harness_glossary::Glossary::from_toml(&toml_string)
        .map_err(|e| format!("validation failed: {e}"))?;
    std::fs::write(&abs, &toml_string).map_err(|e| format!("write failed: {e}"))?;
    Ok(GlossarySaveResponse {
        path: abs.to_string_lossy().into_owned(),
        warnings: warnings.into_iter().map(|w| w.to_string()).collect(),
    })
}

/// Map a parsed [`Glossary`] back into the editable wire [`GlossaryPayload`].
/// Mirrors the projection in [`load_glossary`] so the create/add-term flow can
/// round-trip a term through the same validated TOML serializer.
fn glossary_to_payload(glossary: &Glossary) -> GlossaryPayload {
    let terms = glossary
        .terms()
        .map(|(src, t)| TermEntry {
            source: src.to_string(),
            do_not_translate: t.do_not_translate,
            notes: t.notes.clone(),
            translations: t.translations.clone(),
        })
        .collect();
    let locale_overrides = glossary
        .overrides()
        .map(|(id, ov)| LocaleOverrideEntry {
            locale: id.to_string(),
            register: ov.register.map(|r| r.as_str().to_owned()),
            variant: ov.variant.clone(),
        })
        .collect();
    GlossaryPayload {
        schema_version: glossary.schema_version(),
        terms,
        locale_overrides,
    }
}

/// Create a `glossary.toml` at the project root and register it in the manifest.
///
/// Refuses if the project already declares a glossary. Writes a minimal valid
/// glossary (schema version, no terms), adds the `[glossary]` block to
/// `i18n-harness.toml`, then re-opens the project so the parsed glossary is live
/// for the reuse and translate paths and the glossary slot is pinned. Returns
/// the refreshed [`ProjectOpenResponse`].
#[tauri::command]
pub(crate) fn create_project_glossary(
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    // Resolve the root + reject if a glossary already exists, under a brief lock.
    let root: PathBuf = {
        let guard = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        let project = guard.as_ref().ok_or_else(error::no_project)?;
        if project.glossary().is_some() {
            return Err("project already has a glossary".to_string());
        }
        PathBuf::from(&project.summary().root)
    };

    // Write the minimal glossary file (validated through the same round-trip as
    // save_glossary) before touching the manifest, so a declared-but-missing
    // glossary can never be persisted.
    let rel = PathBuf::from("glossary.toml");
    let abs = root.join(&rel);
    if abs.exists() {
        return Err(format!(
            "{} already exists; declare it in Settings instead",
            abs.display()
        ));
    }
    let payload = GlossaryPayload {
        schema_version: 1,
        terms: vec![],
        locale_overrides: vec![],
    };
    let toml_string = payload_to_toml(&payload)?;
    i18n_harness_glossary::Glossary::from_toml(&toml_string)
        .map_err(|e| format!("validation failed: {e}"))?;
    std::fs::write(&abs, &toml_string).map_err(|e| format!("write failed: {e}"))?;

    // Register in the manifest, then re-open to refresh every consumer.
    {
        let mut guard = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        let project = guard.as_mut().ok_or_else(error::no_project)?;
        project
            .set_glossary(GlossaryConfig { path: rel })
            .map_err(|e| e.to_string())?;
        project.save_manifest().map_err(|e| e.to_string())?;
    }

    open_project_impl(&root, &state)
}

/// Insert or merge a single term into the project glossary from a translation
/// surface (e.g. the unit editor's "Add to glossary").
///
/// Loads the project's current glossary, sets `translations[locale] =
/// translation` (preserving other locales) for an existing `source` or appends
/// a new term, writes the file back through the validated TOML round-trip, then
/// re-opens the project so the change is live. Errors if the project has no
/// glossary yet (the UI should create one first).
#[tauri::command]
pub(crate) fn add_glossary_term(
    source: String,
    locale: String,
    translation: Option<String>,
    do_not_translate: bool,
    notes: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let source = source.trim().to_string();
    if source.is_empty() {
        return Err("term source must not be empty".to_string());
    }

    // Snapshot root + current glossary path + payload under a brief lock.
    let (root, glossary_path, mut payload): (PathBuf, PathBuf, GlossaryPayload) = {
        let guard = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        let project = guard.as_ref().ok_or_else(error::no_project)?;
        let glossary = project
            .glossary()
            .ok_or_else(|| "project has no glossary; create one first".to_string())?;
        let summary = project.summary();
        let gpath = summary
            .glossary_path
            .clone()
            .ok_or_else(|| "glossary path not resolved".to_string())?;
        (
            PathBuf::from(&summary.root),
            PathBuf::from(gpath),
            glossary_to_payload(glossary),
        )
    };

    // Merge the term into the payload.
    match payload.terms.iter_mut().find(|t| t.source == source) {
        Some(existing) => {
            existing.do_not_translate = do_not_translate;
            if notes.is_some() {
                existing.notes = notes;
            }
            if let Some(t) = translation {
                existing.translations.insert(locale, t);
            }
        }
        None => {
            let mut translations = BTreeMap::new();
            if let Some(t) = translation {
                translations.insert(locale, t);
            }
            payload.terms.push(TermEntry {
                source,
                do_not_translate,
                notes,
                translations,
            });
        }
    }
    // Keep the on-disk order deterministic (alphabetical by source), matching
    // save_glossary's contract.
    payload.terms.sort_by(|a, b| a.source.cmp(&b.source));

    // Validate + write through the same round-trip as save_glossary.
    let toml_string = payload_to_toml(&payload)?;
    i18n_harness_glossary::Glossary::from_toml(&toml_string)
        .map_err(|e| format!("validation failed: {e}"))?;
    std::fs::write(&glossary_path, &toml_string).map_err(|e| format!("write failed: {e}"))?;

    open_project_impl(&root, &state)
}

/// Read a `metrics.jsonl` file (as produced by
/// `i18n_harness_gate::metrics::FileSink`). Each line is one event; we
/// parse leniently — malformed lines are counted but do not abort the
/// load, so a corrupted run still surfaces the events that came
/// before it.
#[tauri::command]
pub(crate) fn load_metrics(path: String) -> Result<MetricsResponse, String> {
    let abs = PathBuf::from(&path);
    // Read raw bytes, not a String. A single non-UTF-8 byte anywhere
    // in the file would make `read_to_string` abort the whole command,
    // turning a recoverable "bad line" scenario into a hard load
    // failure. With raw bytes we split on '\n' and try UTF-8 decode
    // per line, so a partially-corrupted run still surfaces the
    // events that came before the corruption.
    let bytes = std::fs::read(&abs).map_err(|e| format!("read failed: {e}"))?;
    let mut events: Vec<MetricEvent> = Vec::new();
    let mut error_count: usize = 0;
    let mut line_count: usize = 0;
    for raw in bytes.split(|b| *b == b'\n') {
        let trimmed = raw.trim_ascii();
        if trimmed.is_empty() {
            continue;
        }
        line_count += 1;
        let Ok(line) = std::str::from_utf8(trimmed) else {
            error_count += 1;
            continue;
        };
        match serde_json::from_str::<MetricEvent>(line) {
            Ok(event) => events.push(event),
            Err(_) => {
                error_count += 1;
            }
        }
    }
    Ok(MetricsResponse {
        path: abs.to_string_lossy().into_owned(),
        events,
        error_count,
        line_count,
    })
}
