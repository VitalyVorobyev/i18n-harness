//! Tauri desktop shell for the i18n-harness.
//!
//! This crate is a *thin* wrapper over the library API. Translation,
//! gate, and adapter logic live in the workspace's pure-Rust crates and
//! remain testable as a headless library. Commands here marshal
//! arguments, invoke the library, and serialize results back to the
//! JavaScript layer. No business logic that does not fit on a single
//! screen of glue belongs here.

mod backing;
mod cancellation;
mod error;
mod jobs;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use i18n_harness_adapter_qt::Catalog;
use i18n_harness_core::{Target, Unit, UnitId, UnitState};

use backing::{BackingCatalog, extract_for_format};
#[cfg(feature = "ollama")]
use i18n_harness_gate::GateReport;
use i18n_harness_glossary::Glossary;
use i18n_harness_locales::Locale;
use i18n_harness_project::{
    BackendConfig, CatalogEntry, CatalogRef, Correction, CorrectionId, CorrectionProvenance,
    DraftManifest, GlossaryConfig, LocaleConfig, NewCorrection, Project, ProjectSummary,
    PromptsConfig,
};
use serde::{Deserialize, Serialize};

/// Process-wide state shared across Tauri commands.
///
/// One catalog is open at a time — opening a new one replaces the
/// previous, so memory does not grow unbounded across opens. The
/// catalog carries the preserved source bytes needed for byte-stable
/// round-trip on save, so it lives here rather than crossing the IPC
/// bridge on every command.
///
/// The glossary slot is populated by `load_glossary` or, in M4.2a, as a
/// side effect of `open_project` when the project declares one. Once set,
/// it is threaded into every `translate_unit` call so MT proposals respect
/// project glossary terms.
///
/// The project slot (M4.2a) holds the currently-open project. It coexists
/// with the file-centric catalog slot: opening a project doesn't auto-open
/// any catalog, and opening a stand-alone catalog leaves the project slot
/// untouched.
///
/// The `project_catalogs` slot (M4.2b) is the multi-catalog dirty store
/// used when working in project mode. It is keyed by absolute path and
/// populated by `open_catalog_in_project`. The two stores — `catalog`
/// (singular, file-centric) and `project_catalogs` (multi, project-scoped)
/// — are independent. Closing a project clears both.
#[derive(Default)]
pub struct AppState {
    catalog: Mutex<Option<OpenCatalog>>,
    glossary: Mutex<Option<Glossary>>,
    project: Mutex<Option<Project>>,
    project_catalogs: Mutex<std::collections::BTreeMap<PathBuf, OpenCatalogEntry>>,
    /// In-process registry of cancellable background jobs (M4.2c.2).
    /// Each `translate_batch_in_project` call registers a new entry; the
    /// worker thread deregisters on exit. Per-catalog/per-locale
    /// exclusion is enforced at the command level via `active_batches`.
    jobs: jobs::JobRegistry,
    /// `(absolute catalog path, locale id)` pairs that have a bulk
    /// translate in flight (M4.2c.2). Inserted by
    /// `translate_batch_in_project` before spawning the worker, removed
    /// by the worker's exit path. The pair is the granularity we refuse
    /// concurrent runs on — two workers writing to the same catalog
    /// would race on the merge step.
    active_batches: Mutex<std::collections::BTreeSet<(PathBuf, String)>>,
}

/// An entry in the project-scoped multi-catalog store.
///
/// The `catalog` is the format-erased [`BackingCatalog`] enum so the store
/// can hold Qt and PO (and eventually ICU-JSON) catalogs uniformly. Every
/// command that reads units, finds a unit by id, or saves back to disk
/// goes through the enum's delegating helpers.
struct OpenCatalogEntry {
    catalog: BackingCatalog,
    dirty: bool,
}

/// The currently-open catalog plus the absolute path it was loaded
/// from. The path is the frontend's handle.
struct OpenCatalog {
    path: PathBuf,
    catalog: Catalog,
}

/// Wire-format response from the `open_catalog` Tauri command.
#[derive(Debug, Serialize)]
pub struct CatalogResponse {
    /// Absolute path the catalog was read from; also the handle for
    /// follow-up commands.
    pub path: String,
    /// Number of units in the catalog (including non-writable ones).
    pub unit_count: usize,
    /// Target language as declared in the `.ts` root element. `None`
    /// if the catalog did not specify one — `translate_unit` will fail
    /// in that case until the locale is set explicitly.
    pub language: Option<String>,
    /// The units themselves, in document order. Serializes through
    /// [`Unit`]'s own serde derive — no flattening or projection here.
    pub units: Vec<Unit>,
}

/// Wire-format response from `save_catalog`.
#[derive(Debug, Serialize)]
pub struct SaveSummary {
    /// Absolute path the catalog was written to.
    pub path: String,
    /// Total units written (writable + preserved).
    pub unit_count: usize,
}

/// One edit to a unit's target, mirroring [`Target`] for singular and
/// plural cases. The frontend sends this when the user types into the
/// target editor; the command merges it into the in-memory unit.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TargetEdit {
    /// Replace the singular target's text. `None` empties it.
    Singular {
        /// New value, or `None` to clear the target.
        text: Option<String>,
    },
    /// Replace one form of a plural target. `form_index` is the CLDR
    /// position; `text` is the new value (`None` empties that form).
    Plural {
        /// CLDR-ordered position of the form to write (`0..arity`).
        form_index: u32,
        /// New value for that form, or `None` to clear it.
        text: Option<String>,
    },
}

/// Result of `translate_unit`: the updated unit plus the gate report
/// the harness ran on it.
#[cfg(feature = "ollama")]
#[derive(Debug, Serialize)]
pub struct TranslateResult {
    /// The unit after merging the backend's output and running the
    /// gate. Its `state` reflects the gate outcome.
    pub unit: Unit,
    /// The gate report. Findings drive the UI's inline review.
    pub report: GateReport,
}

/// Wire-format locale entry returned by `list_locales` — the UI uses
/// this to build column headers in the glossary editor and to label
/// register overrides.
#[derive(Debug, Serialize)]
pub struct LocaleInfo {
    /// CLDR-style id (`en`, `de_DE`, `es_ES`, `zh_Hans`).
    pub id: String,
    /// Default register declared in the locales table
    /// (`"formal"` | `"informal"` | `"neutral"`). Glossary overrides may
    /// override per project.
    pub register: &'static str,
    /// Script family — useful for grouping or icon picks
    /// (`"Latin"`, `"Han"`, …).
    pub script: String,
    /// CLDR plural arity. Useful as a tooltip in the editor.
    pub plural_arity: u32,
}

/// One glossary term in the wire format. Mirrors
/// `i18n_harness_glossary::Term` plus the source key, since the JS
/// layer prefers a flat list to a map.
#[derive(Debug, Serialize, Deserialize)]
pub struct TermEntry {
    /// Source string (case-sensitive natural key).
    pub source: String,
    /// `true` if the term must never be translated.
    #[serde(default)]
    pub do_not_translate: bool,
    /// Free-form notes (sense disambiguation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Translations keyed by locale id.
    #[serde(default)]
    pub translations: BTreeMap<String, String>,
}

/// One `[locale.<id>]` override in wire form.
#[derive(Debug, Serialize, Deserialize)]
pub struct LocaleOverrideEntry {
    /// Locale id (`de_DE`, `es_ES`, …).
    pub locale: String,
    /// `"formal"` | `"informal"` | `"neutral"`, or `None` to leave
    /// the workspace default in place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub register: Option<String>,
    /// Variant tag override; rarely set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

/// Editable glossary payload exchanged between the UI and the Rust
/// layer. The shape mirrors the on-disk TOML schema; `save_glossary`
/// validates by round-tripping through `Glossary::from_toml` before
/// writing.
#[derive(Debug, Serialize, Deserialize)]
pub struct GlossaryPayload {
    /// Schema version (`1` today). The save command refuses higher
    /// values to keep forward compatibility deliberate.
    pub schema_version: u32,
    /// Terms in alphabetical order by source.
    pub terms: Vec<TermEntry>,
    /// Per-locale register / variant overrides.
    pub locale_overrides: Vec<LocaleOverrideEntry>,
}

/// Response from `load_glossary` — the parsed payload plus any
/// non-fatal warnings the loader surfaced (unknown locale ids, terms
/// with empty translation tables).
#[derive(Debug, Serialize)]
pub struct GlossaryLoadResponse {
    /// Absolute path the glossary was read from.
    pub path: String,
    /// Editable payload — what the UI binds against.
    pub payload: GlossaryPayload,
    /// Human-readable warning strings. Empty when the glossary
    /// validates cleanly.
    pub warnings: Vec<String>,
}

/// Response from `save_glossary` — the path written plus the
/// validator's warnings (so the UI can surface them without re-loading).
#[derive(Debug, Serialize)]
pub struct GlossarySaveResponse {
    /// Path the glossary was written to.
    pub path: String,
    /// Warnings the validator surfaced before write.
    pub warnings: Vec<String>,
}

/// One row from `.i18n-harness/metrics.jsonl`, re-shaped for the wire.
///
/// We don't depend on `i18n_harness_gate::Event` directly — its serde
/// flattened tag would force the UI to deal with two shapes. Here we
/// surface a flat record the TypeScript layer can render without a
/// custom serde dance.
#[derive(Debug, Serialize, Deserialize)]
pub struct MetricEvent {
    /// On-disk schema version (canonical metrics schema, not the
    /// in-memory `Unit` schema).
    pub schema: u32,
    /// RFC3339 timestamp (`…Z`, UTC, microsecond precision).
    pub ts: String,
    /// Backend that produced the unit (`"manual"`, `"ollama"`, …).
    pub backend: String,
    /// Target locale (`"de_DE"`, …).
    pub locale: String,
    /// Event kind discriminator (`"gate-reject"`, `"soft-warning"`,
    /// `"human-edit"`, `"retry"`).
    pub event: String,
    /// Unit id the event pertains to.
    pub unit_id: String,
    /// Lower-case kebab flag name (`"length-warn"`, …) or empty for
    /// non-flag events.
    pub rule: String,
    /// Per-rule detail payload. We pass it through opaquely; the UI
    /// renders rule-specific summaries.
    pub detail: serde_json::Value,
}

/// Wire response from `load_metrics`.
#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    /// Absolute path read.
    pub path: String,
    /// Successfully-parsed events, in file order.
    pub events: Vec<MetricEvent>,
    /// Lines that could not be parsed as JSON or were missing fields.
    /// Surfaced to the UI so a corrupted run is visible, not silent.
    pub error_count: usize,
    /// Total non-blank lines read (`events.len() + error_count`).
    pub line_count: usize,
}

/// Return the package version baked at compile time.
///
/// Smoke-test command: confirms the IPC bridge is wired correctly
/// before any catalog has been opened.
#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Open a Qt `.ts` catalog at `path` and stash it in [`AppState`].
///
/// Returns the units for the frontend to render. The catalog itself
/// (including the byte buffer needed for byte-stable round-trip) is
/// kept server-side; the frontend identifies it by path on follow-up
/// commands. Opening a new catalog replaces the previously-open one.
#[tauri::command]
fn open_catalog(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<CatalogResponse, String> {
    let abs = PathBuf::from(&path);
    let catalog =
        i18n_harness_adapter_qt::extract(&abs).map_err(|e| format!("extract failed: {e}"))?;
    let response = build_catalog_response(&abs, &catalog);
    let mut current = state
        .catalog
        .lock()
        .map_err(error::lock_poisoned("catalog"))?;
    *current = Some(OpenCatalog { path: abs, catalog });
    Ok(response)
}

/// Write a target into the currently-open catalog's in-memory unit.
///
/// Promotes a writable unit's state from `Untranslated` to `Proposed`
/// the first time any text is set. Refuses to touch `Vanished` /
/// `Obsolete` units (the harness must never modify those).
#[tauri::command]
fn update_unit_target(
    unit_id: String,
    edit: TargetEdit,
    state: tauri::State<'_, AppState>,
) -> Result<Unit, String> {
    let mut current = state
        .catalog
        .lock()
        .map_err(error::lock_poisoned("catalog"))?;
    let open = current.as_mut().ok_or_else(error::no_catalog)?;
    let id = UnitId::from(unit_id);
    let unit = open
        .catalog
        .find_unit_mut(&id)
        .ok_or_else(|| format!("unit not found: {id}"))?;
    if !unit.state.is_ui_editable() {
        return Err(format!(
            "unit {id} is {state:?} — vanished/obsolete units are not editable",
            state = unit.state,
        ));
    }
    match (&mut unit.target, edit) {
        (Target::Singular { text }, TargetEdit::Singular { text: new }) => *text = new,
        (Target::Plural { forms }, TargetEdit::Plural { form_index, text }) => {
            let i = form_index as usize;
            if i >= forms.len() {
                return Err(format!(
                    "plural form index {i} out of range (have {})",
                    forms.len()
                ));
            }
            forms[i] = text;
        }
        (Target::Singular { .. }, TargetEdit::Plural { .. }) => {
            return Err("cannot apply plural edit to singular unit".into());
        }
        (Target::Plural { .. }, TargetEdit::Singular { .. }) => {
            return Err("cannot apply singular edit to plural unit".into());
        }
    }
    // State auto-transitions on edit:
    //   Untranslated          → Proposed      once any text is set
    //   Proposed | Finished   → Untranslated  once the target is fully cleared
    //   Finished              → Proposed      when text changes (human reconsidering)
    match unit.state {
        UnitState::Untranslated if !unit.target.is_empty() => {
            unit.state = UnitState::Proposed;
        }
        UnitState::Proposed | UnitState::Finished if unit.target.is_empty() => {
            unit.state = UnitState::Untranslated;
            unit.flags = Default::default();
        }
        UnitState::Finished if !unit.target.is_empty() => {
            // M4.3a.1: editing a Finished unit reverts it to Proposed — the
            // human is actively reconsidering the finalized translation, so
            // it should re-enter the review loop.
            unit.state = UnitState::Proposed;
        }
        _ => {}
    }
    Ok(unit.clone())
}

/// Persist the in-memory catalog to disk via the byte-stable adapter.
///
/// If `out_path` is `None`, writes back to the path the catalog was
/// opened from. The original bytes (modulo edited unit bodies) come
/// through verbatim — the M0 contract.
#[tauri::command]
fn save_catalog(
    out_path: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<SaveSummary, String> {
    let current = state
        .catalog
        .lock()
        .map_err(error::lock_poisoned("catalog"))?;
    let open = current.as_ref().ok_or_else(error::no_catalog)?;
    let target = out_path
        .map(PathBuf::from)
        .unwrap_or_else(|| open.path.clone());
    let units = open.catalog.units().to_vec();
    i18n_harness_adapter_qt::apply(&open.catalog, &units, &target)
        .map_err(|e| format!("apply failed: {e}"))?;
    Ok(SaveSummary {
        path: target.to_string_lossy().into_owned(),
        unit_count: units.len(),
    })
}

/// List the workspace locales (`en`, `de_DE`, `es_ES`, `zh_Hans`).
/// The UI uses this for column headers in the glossary editor and to
/// render the locale badge on the catalog view.
#[tauri::command]
fn list_locales() -> Vec<LocaleInfo> {
    Locale::all()
        .map(|l| LocaleInfo {
            id: l.id.to_string(),
            register: match l.register {
                i18n_harness_locales::Register::Formal => "formal",
                i18n_harness_locales::Register::Informal => "informal",
                i18n_harness_locales::Register::Neutral => "neutral",
            },
            script: format!("{:?}", l.script),
            plural_arity: l.plural_arity(),
        })
        .collect()
}

/// Load a glossary `.toml` from `path`. Returns the editable payload
/// and any non-fatal warnings (unknown locale, empty translations).
///
/// Side effect: stashes the parsed glossary in [`AppState`] so the
/// next `translate_unit` call passes it to the backend. Loading a new
/// glossary replaces the previous one.
#[tauri::command]
fn load_glossary(
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

/// Read a `metrics.jsonl` file (as produced by
/// `i18n_harness_gate::metrics::FileSink`). Each line is one event; we
/// parse leniently — malformed lines are counted but do not abort the
/// load, so a corrupted run still surfaces the events that came
/// before it.
#[tauri::command]
fn load_metrics(path: String) -> Result<MetricsResponse, String> {
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

/// Validate the payload by round-tripping through `Glossary::from_toml`
/// and then write the resulting (deterministic, alphabetical) TOML to
/// `path`. Refuses to write if validation fails.
#[tauri::command]
fn save_glossary(path: String, payload: GlossaryPayload) -> Result<GlossarySaveResponse, String> {
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

/// Drop all in-memory edits and re-read the catalog from disk. The
/// UI's "Discard changes" / revert action.
#[tauri::command]
fn discard_changes(state: tauri::State<'_, AppState>) -> Result<CatalogResponse, String> {
    let mut current = state
        .catalog
        .lock()
        .map_err(error::lock_poisoned("catalog"))?;
    let open = current.as_mut().ok_or_else(error::no_catalog)?;
    let fresh =
        i18n_harness_adapter_qt::extract(&open.path).map_err(|e| format!("extract failed: {e}"))?;
    let response = build_catalog_response(&open.path, &fresh);
    open.catalog = fresh;
    Ok(response)
}

/// Translate one unit via the configured backend, then run the gate.
///
/// Available only when the crate is built with the `ollama` feature
/// (the default). Returns the merged unit + gate report; the UI uses
/// findings inline.
#[cfg(feature = "ollama")]
#[tauri::command]
fn translate_unit(
    unit_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<TranslateResult, String> {
    use i18n_harness_backend::{
        FailureKind, OllamaBackend, TranslationBackend, TranslationOutcome,
    };
    use i18n_harness_core::{Batch, BatchKey, FlagSet};

    let mut current = state
        .catalog
        .lock()
        .map_err(error::lock_poisoned("catalog"))?;
    let open = current.as_mut().ok_or_else(error::no_catalog)?;
    let language = open
        .catalog
        .language()
        .ok_or_else(|| "catalog has no <TS language=…>; cannot translate".to_string())?;
    let locale = Locale::by_id(language)
        .ok_or_else(|| format!("unknown locale `{language}`; add it to crates/locales"))?;
    let id = UnitId::from(unit_id);
    let original = open
        .catalog
        .find_unit_mut(&id)
        .ok_or_else(|| format!("unit not found: {id}"))?
        .clone();
    if !original.state.is_writable() {
        return Err(format!(
            "unit {id} is {state:?} — not translatable",
            state = original.state,
        ));
    }

    let batch = Batch::new(BatchKey::new("ui", 0), vec![original.clone()]);
    let backend =
        OllamaBackend::new().map_err(|e| format!("ollama backend construction failed: {e}"))?;
    let backend_name = backend.name().to_string();
    // Clone the glossary under the lock so we drop the guard before any
    // network call. Glossary owns small TOML-derived BTreeMaps; the
    // clone is cheap and avoids holding two locks at once.
    let glossary = state
        .glossary
        .lock()
        .map_err(error::lock_poisoned("glossary"))?
        .clone();
    let outcomes = backend
        .translate_batch(&batch, locale, glossary.as_ref())
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
            // explicitly promotes to Finished via save/accept. Auto-
            // promoting hid model output behind a "done" badge before
            // the translator could review.
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
            // Surface as an inline hard gate finding rather than an Err.
            // The legacy `translate_unit` has no project handle, so we
            // do not touch review status here.
            let report = GateReport::backend_malformed_response(original.id.clone(), reason);
            return Ok(TranslateResult {
                unit: original,
                report,
            });
        }
        TranslationOutcome::Failed { reason, .. } => {
            return Err(format!("backend failed: {reason}"));
        }
    }

    let report = i18n_harness_gate::validate(&merged, locale, None);

    // Persist the merged unit back into the catalog.
    if let Some(slot) = open.catalog.find_unit_mut(&id) {
        *slot = merged.clone();
    }

    Ok(TranslateResult {
        unit: merged,
        report,
    })
}

/// Serialise a [`GlossaryPayload`] to the on-disk TOML schema. Kept
/// in this crate so we can drive it from a wire payload without
/// running the validator twice (once on the payload, once on the
/// produced TOML). Field names match `crates/glossary/src/schema.rs`'s
/// `Raw*` shapes.
fn payload_to_toml(payload: &GlossaryPayload) -> Result<String, String> {
    #[derive(Serialize)]
    struct WireMeta {
        schema_version: u32,
    }
    #[derive(Serialize)]
    struct WireTerm<'a> {
        source: &'a str,
        do_not_translate: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        notes: Option<&'a String>,
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        translations: BTreeMap<String, String>,
    }
    #[derive(Serialize)]
    struct WireLocale<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        register: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        variant: Option<&'a String>,
    }
    #[derive(Serialize)]
    struct Wire<'a> {
        meta: WireMeta,
        #[serde(rename = "term", skip_serializing_if = "Vec::is_empty")]
        terms: Vec<WireTerm<'a>>,
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        locale: BTreeMap<String, WireLocale<'a>>,
    }

    // Locale overrides are keyed by id on disk, so duplicates would
    // silently collapse into the last writer if we let BTreeMap::collect
    // do its thing. Detect them up-front and refuse to write — the UI
    // surfaces the error to the user. Term duplicates are caught by
    // `Glossary::from_toml`'s `DuplicateSource` check downstream, but
    // catching them here too produces a sharper message.
    let mut seen_terms = std::collections::HashSet::new();
    for t in &payload.terms {
        if !seen_terms.insert(&t.source) {
            return Err(format!(
                "duplicate term source `{}` — every source must be unique",
                t.source,
            ));
        }
    }
    let mut locale: BTreeMap<String, WireLocale> = BTreeMap::new();
    for o in &payload.locale_overrides {
        if locale.contains_key(&o.locale) {
            return Err(format!(
                "duplicate locale override for `{}` — each locale appears at most once",
                o.locale,
            ));
        }
        locale.insert(
            o.locale.clone(),
            WireLocale {
                register: o.register.as_ref(),
                variant: o.variant.as_ref(),
            },
        );
    }
    let terms = payload
        .terms
        .iter()
        .map(|t| WireTerm {
            source: &t.source,
            do_not_translate: t.do_not_translate,
            notes: t.notes.as_ref(),
            translations: t.translations.clone(),
        })
        .collect();
    let wire = Wire {
        meta: WireMeta {
            schema_version: payload.schema_version,
        },
        terms,
        locale,
    };
    toml::to_string(&wire).map_err(|e| format!("toml serialize: {e}"))
}

// ── M4.2c wire shapes ────────────────────────────────────────────────────────

/// Provenance of the MT proposal in a correction record, mirroring
/// [`CorrectionProvenance`] with serde derives so it crosses the IPC bridge.
/// All fields default to empty string; the caller fills only what the backend
/// made available.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CorrectionProvenanceWire {
    /// Backend name as registered by the `TranslationBackend` trait.
    #[serde(default)]
    pub backend: String,
    /// Model identifier as the backend reports it.
    #[serde(default)]
    pub model: String,
    /// Free-form model version / revision string.
    #[serde(default)]
    pub model_version: String,
    /// Prompt template version identifier.
    #[serde(default)]
    pub prompt_template_version: String,
    /// Hash of the glossary content at correction time.
    #[serde(default)]
    pub glossary_version: String,
}

/// IPC payload for `record_correction_in_project`.
#[derive(Debug, Deserialize)]
pub struct RecordCorrectionRequest {
    /// Absolute or manifest-relative catalog path.
    pub catalog_path: String,
    /// Target locale id.
    pub locale: String,
    /// Unit that was corrected.
    pub unit_id: String,
    /// Source text.
    pub source: String,
    /// MT proposal that was edited (empty for manual-from-scratch).
    pub mt_proposal: String,
    /// The accepted human translation.
    pub human_target: String,
    /// Provenance of `mt_proposal`.
    #[serde(default)]
    pub provenance: CorrectionProvenanceWire,
    /// Flags the unit carried at correction time.
    #[serde(default)]
    pub flags_at_correction: Vec<i18n_harness_core::Flag>,
}

/// IPC response from `record_correction_in_project`.
#[derive(Debug, Serialize)]
pub struct CorrectionIdResponse {
    /// The assigned correction id in `"corr_<12-hex>"` form.
    pub id: String,
}

/// Filter passed to `list_corrections_in_project`. All fields are optional;
/// an all-default filter returns every record (AND semantics for non-None fields).
#[derive(Debug, Default, Deserialize)]
pub struct ListCorrectionsFilter {
    /// Restrict to this catalog (absolute or manifest-relative path).
    #[serde(default)]
    pub catalog_path: Option<String>,
    /// Restrict to this target locale id.
    #[serde(default)]
    pub locale: Option<String>,
    /// Restrict to this unit id.
    #[serde(default)]
    pub unit_id: Option<String>,
    /// If true, restrict to corrections that are in the curated set.
    ///
    /// Note: `CorrectionFilter` has no `curated_only` field; this flag is
    /// honoured by filtering the result list against the project's curated set
    /// after the JSONL scan.
    #[serde(default)]
    pub curated_only: bool,
}

/// IPC payload for `set_review_status_in_project`. Wraps the three fields
/// `Project::set_review_status` accepts beyond catalog + unit.
#[derive(Debug, Deserialize)]
pub struct ReviewStatusInput {
    /// The new review status. `None` clears the unit's record.
    pub status: Option<i18n_harness_core::ReviewStatus>,
    /// Source-hash value from the unit at review time (empty if not available).
    #[serde(default)]
    pub source_hash_at_review: String,
    /// Free-form reviewer note.
    #[serde(default)]
    pub reviewer_note: Option<String>,
}

// ── Project commands (M4.2a) ─────────────────────────────────────────────────

/// Wire response for `open_project` / `create_project`. Carries the summary
/// the UI binds against plus any non-fatal warnings (unknown locale ids,
/// glossary parse warnings). Hard failures come back as `Err(String)`.
#[derive(Debug, Serialize)]
pub struct ProjectOpenResponse {
    /// Compact project summary safe to send across the IPC bridge.
    pub summary: ProjectSummary,
    /// Human-readable warning strings (`UnknownLocale`, `Glossary(...)`).
    /// Empty when the project loads cleanly.
    pub warnings: Vec<String>,
}

/// Open an existing project rooted at `root`.
///
/// Stashes the project in [`AppState`]; replaces any previously-open project
/// and clears the stand-alone catalog slot (the UI's old single-file session
/// is closed when a project takes over). As a side effect, if the project
/// declares a glossary, that glossary is also pinned in the glossary slot so
/// existing `translate_unit` calls benefit immediately.
#[tauri::command]
fn open_project(
    root: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    open_project_impl(Path::new(&root), &state)
}

/// State-only impl of [`open_project`] — same semantics, takes a borrowed
/// [`AppState`] so integration tests can drive the Tauri command surface
/// without spinning up a real Tauri runtime. The command wrapper above is
/// a one-liner that calls into this helper.
pub(crate) fn open_project_impl(
    root: &Path,
    state: &AppState,
) -> Result<ProjectOpenResponse, String> {
    let (project, warnings) = Project::open(root).map_err(|e| e.to_string())?;
    let summary = project.summary();
    let glossary_for_slot = project.glossary().cloned();

    {
        let mut current = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        *current = Some(project);
    }
    {
        let mut g = state
            .glossary
            .lock()
            .map_err(error::lock_poisoned("glossary"))?;
        *g = glossary_for_slot;
    }
    {
        let mut c = state
            .catalog
            .lock()
            .map_err(error::lock_poisoned("catalog"))?;
        *c = None;
    }

    Ok(ProjectOpenResponse {
        summary,
        warnings: warnings.into_iter().map(|w| w.to_string()).collect(),
    })
}

/// Discover a project from a directory that has no manifest yet.
///
/// Returns the draft for the UI to confirm; never writes anything. The UI
/// follows up with `create_project` once the user has reviewed the draft.
#[tauri::command]
fn discover_project(root: String) -> Result<DraftManifest, String> {
    let root_path = PathBuf::from(&root);
    Project::discover(&root_path).map_err(|e| e.to_string())
}

/// Write `<root>/i18n-harness.toml` from `draft` and open the result.
///
/// Side effects mirror `open_project`: stashes the project, pre-populates the
/// glossary slot when declared, and clears the stand-alone catalog slot.
#[tauri::command]
fn create_project(
    root: String,
    draft: DraftManifest,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let root_path = PathBuf::from(&root);
    let (project, warnings) =
        Project::create_from_draft(&root_path, draft).map_err(|e| e.to_string())?;
    let summary = project.summary();
    let glossary_for_slot = project.glossary().cloned();

    {
        let mut current = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        *current = Some(project);
    }
    {
        let mut g = state
            .glossary
            .lock()
            .map_err(error::lock_poisoned("glossary"))?;
        *g = glossary_for_slot;
    }
    {
        let mut c = state
            .catalog
            .lock()
            .map_err(error::lock_poisoned("catalog"))?;
        *c = None;
    }

    Ok(ProjectOpenResponse {
        summary,
        warnings: warnings.into_iter().map(|w| w.to_string()).collect(),
    })
}

/// Drop the currently-open project. The stand-alone catalog slot, the
/// project-scoped multi-catalog store, and the glossary slot are also
/// cleared so the next "open file" starts from a clean slate.
///
/// Locks are acquired and released one at a time to avoid any lock-order
/// issue.
#[tauri::command]
fn close_project(state: tauri::State<'_, AppState>) -> Result<(), String> {
    *state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))? = None;
    *state
        .glossary
        .lock()
        .map_err(error::lock_poisoned("glossary"))? = None;
    *state
        .catalog
        .lock()
        .map_err(error::lock_poisoned("catalog"))? = None;
    state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?
        .clear();
    Ok(())
}

/// Return the currently-open project's summary, or `None` if no project is
/// open. The UI calls this on launch to rehydrate (when persistence lands)
/// or to detect whether the home screen should be shown.
#[tauri::command]
fn current_project_summary(
    state: tauri::State<'_, AppState>,
) -> Result<Option<ProjectSummary>, String> {
    let current = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    Ok(current.as_ref().map(Project::summary))
}

/// List catalogs declared in the currently-open project.
///
/// Errors with `"no project open"` when the project slot is empty — the UI
/// should gate this command behind a successful `open_project`.
#[tauri::command]
fn list_catalogs(state: tauri::State<'_, AppState>) -> Result<Vec<CatalogRef>, String> {
    let current = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = current.as_ref().ok_or_else(error::no_project)?;
    Ok(project.catalogs().to_vec())
}

/// Persist the manifest's in-memory `toml_edit` document to disk.
///
/// Mutations applied through `Project::add_catalog`, `update_locale`,
/// `set_backend`, etc. update the document in memory; this command writes
/// the document atomically. Settings-tab edits in the UI will call this
/// after each batch of mutations.
#[tauri::command]
fn save_manifest(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let current = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = current.as_ref().ok_or_else(error::no_project)?;
    project.save_manifest().map_err(|e| e.to_string())
}

// ── Project-scoped per-catalog commands (M4.2b) ───────────────────────────────

/// Open a catalog that belongs to the currently-open project.
///
/// The catalog must be declared in the project manifest — either by its
/// manifest-relative path (e.g. `"translations/app_de.ts"`) or by its
/// resolved absolute path. The extracted units have their `review_status`
/// and `source_changed_since_review` fields populated by
/// `Project::apply_review_state` before they are returned. The catalog is
/// stashed in the per-project multi-catalog store keyed by absolute path;
/// subsequent edit/save commands use that key.
///
/// Errors if no project is open, or if `catalog_path` does not match any
/// declared catalog.
#[tauri::command]
fn open_catalog_in_project(
    catalog_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<CatalogResponse, String> {
    open_catalog_in_project_impl(&catalog_path, &state)
}

/// State-only impl of [`open_catalog_in_project`] — same semantics, takes a
/// borrowed [`AppState`] so integration tests can drive the open + edit + save
/// flow without spinning up a real Tauri runtime.
pub(crate) fn open_catalog_in_project_impl(
    catalog_path: &str,
    state: &AppState,
) -> Result<CatalogResponse, String> {
    let path = PathBuf::from(catalog_path);

    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;

    let catalog_ref = project
        .catalog(&path)
        .ok_or_else(|| format!("catalog not in project: {catalog_path}"))?;
    let abs = PathBuf::from(&catalog_ref.absolute_path);
    let format = catalog_ref.format;

    let mut catalog = extract_for_format(&abs, format)?;
    project.apply_review_state(&abs, catalog.units_mut());

    let response = build_catalog_response_backing(&abs, &catalog);
    drop(project_guard);

    state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?
        .insert(
            abs,
            OpenCatalogEntry {
                catalog,
                dirty: false,
            },
        );

    Ok(response)
}

/// Write a target edit into a catalog that is open in the project store.
///
/// Mirrors the state-transition rules of `update_unit_target`: promotes
/// `Untranslated → Proposed` on first text, demotes `Proposed/Finished →
/// Untranslated` when the target is fully cleared, and refuses edits on
/// `Vanished`/`Obsolete` units. Sets `dirty = true` on the entry on any
/// successful mutation.
///
/// Errors if the catalog is not in the project store or the unit is not
/// found / not writable.
#[tauri::command]
fn update_unit_target_in_project(
    catalog_path: String,
    unit_id: String,
    edit: TargetEdit,
    state: tauri::State<'_, AppState>,
) -> Result<Unit, String> {
    update_unit_target_in_project_impl(&catalog_path, &unit_id, edit, &state)
}

/// State-only impl of [`update_unit_target_in_project`] — same semantics, takes
/// a borrowed [`AppState`] so integration tests can drive the open + edit +
/// save flow without spinning up a real Tauri runtime.
pub(crate) fn update_unit_target_in_project_impl(
    catalog_path: &str,
    unit_id: &str,
    edit: TargetEdit,
    state: &AppState,
) -> Result<Unit, String> {
    let abs = PathBuf::from(catalog_path);
    let mut store = state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?;
    let entry = store
        .get_mut(&abs)
        .ok_or_else(error::no_catalog_in_project)?;
    let id = UnitId::from(unit_id.to_owned());
    let unit = entry
        .catalog
        .find_unit_mut(&id)
        .ok_or_else(|| format!("unit not found: {id}"))?;

    if !unit.state.is_ui_editable() {
        return Err(format!(
            "unit {id} is {state:?} — vanished/obsolete units are not editable",
            state = unit.state,
        ));
    }
    match (&mut unit.target, edit) {
        (Target::Singular { text }, TargetEdit::Singular { text: new }) => *text = new,
        (Target::Plural { forms }, TargetEdit::Plural { form_index, text }) => {
            let i = form_index as usize;
            if i >= forms.len() {
                return Err(format!(
                    "plural form index {i} out of range (have {})",
                    forms.len()
                ));
            }
            forms[i] = text;
        }
        (Target::Singular { .. }, TargetEdit::Plural { .. }) => {
            return Err("cannot apply plural edit to singular unit".into());
        }
        (Target::Plural { .. }, TargetEdit::Singular { .. }) => {
            return Err("cannot apply singular edit to plural unit".into());
        }
    }
    match unit.state {
        UnitState::Untranslated if !unit.target.is_empty() => {
            unit.state = UnitState::Proposed;
        }
        UnitState::Proposed | UnitState::Finished if unit.target.is_empty() => {
            unit.state = UnitState::Untranslated;
            unit.flags = Default::default();
        }
        UnitState::Finished if !unit.target.is_empty() => {
            // M4.3a.1: editing a Finished unit reverts it to Proposed — the
            // human is actively reconsidering the finalized translation, so
            // it should re-enter the review loop.
            unit.state = UnitState::Proposed;
        }
        _ => {}
    }
    let result = unit.clone();
    entry.dirty = true;
    Ok(result)
}

/// Write a single project-catalog back to disk using the byte-stable adapter.
///
/// Clears `dirty` on success. Errors if the catalog is not in the project
/// store.
#[tauri::command]
fn save_catalog_in_project(
    catalog_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<SaveSummary, String> {
    let abs = PathBuf::from(&catalog_path);
    let mut store = state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?;
    let entry = store
        .get_mut(&abs)
        .ok_or_else(error::no_catalog_in_project)?;
    let units = entry.catalog.units().to_vec();
    entry.catalog.apply(&units, &abs)?;
    entry.dirty = false;
    Ok(SaveSummary {
        path: abs.to_string_lossy().into_owned(),
        unit_count: units.len(),
    })
}

/// Wire response from `save_all_dirty`. Carries both the catalogs that were
/// successfully written and (when the run stopped early) the path + reason of
/// the first failure. Using a single response shape — rather than
/// `Result<Vec<SaveSummary>, String>` — means the UI never loses the list of
/// already-saved catalogs when one apply mid-batch fails, so it can refresh
/// the right dirty pills without an extra IPC round trip.
#[derive(Debug, Serialize)]
pub struct SaveAllDirtyResponse {
    /// Catalogs written, in BTreeMap iteration order (absolute path order).
    /// Their `dirty` flag has been cleared in the in-memory store.
    pub saved: Vec<SaveSummary>,
    /// Absolute path of the catalog whose `apply` failed, if any. Catalogs
    /// after this entry in the iteration order were not attempted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_path: Option<String>,
    /// Human-readable failure reason, matched 1:1 with `failed_path`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_reason: Option<String>,
}

/// Write every dirty catalog in the project store back to disk.
///
/// Catalogs are written in BTreeMap iteration order (absolute path order),
/// which is deterministic. On the first apply failure the function stops and
/// returns the catalogs written so far plus `failed_path` / `failed_reason`
/// describing the failure. Successfully-written entries have their `dirty`
/// flag cleared regardless of whether a later entry failed.
#[tauri::command]
fn save_all_dirty(state: tauri::State<'_, AppState>) -> Result<SaveAllDirtyResponse, String> {
    save_all_dirty_impl(&state)
}

/// State-only impl of [`save_all_dirty`] — same semantics, takes a borrowed
/// [`AppState`] so integration tests can drive the open + edit + save flow
/// without spinning up a real Tauri runtime.
pub(crate) fn save_all_dirty_impl(state: &AppState) -> Result<SaveAllDirtyResponse, String> {
    let mut store = state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?;
    let mut saved: Vec<SaveSummary> = Vec::new();
    for (abs, entry) in store.iter_mut() {
        if !entry.dirty {
            continue;
        }
        let units = entry.catalog.units().to_vec();
        match entry.catalog.apply(&units, abs) {
            Ok(()) => {
                entry.dirty = false;
                saved.push(SaveSummary {
                    path: abs.to_string_lossy().into_owned(),
                    unit_count: units.len(),
                });
            }
            Err(e) => {
                return Ok(SaveAllDirtyResponse {
                    saved,
                    failed_path: Some(abs.to_string_lossy().into_owned()),
                    failed_reason: Some(e),
                });
            }
        }
    }
    Ok(SaveAllDirtyResponse {
        saved,
        failed_path: None,
        failed_reason: None,
    })
}

/// Re-read a catalog from disk and fold the current review state back in.
///
/// Replaces the in-memory entry in the project store and clears `dirty`.
/// Returns the same shape as `open_catalog_in_project`. Errors if the
/// catalog is not in the project store or if no project is open.
#[tauri::command]
fn discard_changes_in_project(
    catalog_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<CatalogResponse, String> {
    let abs = PathBuf::from(&catalog_path);

    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;

    // Confirm the catalog is in the store before doing I/O, and capture its
    // format so we dispatch to the right reader.
    let format = {
        let store = state
            .project_catalogs
            .lock()
            .map_err(error::lock_poisoned("project_catalogs"))?;
        if !store.contains_key(&abs) {
            return Err(error::no_catalog_in_project());
        }
        project
            .catalog(&abs)
            .ok_or_else(|| "catalog not registered in project".to_string())?
            .format
    };

    let mut catalog = extract_for_format(&abs, format)?;
    project.apply_review_state(&abs, catalog.units_mut());

    let response = build_catalog_response_backing(&abs, &catalog);
    drop(project_guard);

    state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?
        .insert(
            abs,
            OpenCatalogEntry {
                catalog,
                dirty: false,
            },
        );

    Ok(response)
}

/// Return the absolute paths of all catalogs currently open in the project
/// store, in BTreeMap order (i.e. lexicographic absolute-path order).
///
/// The UI uses this to render per-catalog dirty-state pills. Returns an
/// empty vec when no catalogs have been opened via `open_catalog_in_project`.
#[tauri::command]
fn list_open_catalogs(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    let store = state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?;
    Ok(store
        .keys()
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

/// Return whether the named catalog has unsaved edits.
///
/// Returns `false` if the catalog is not in the project store (treat
/// unknown = clean from the UI's perspective).
#[tauri::command]
fn is_catalog_dirty(
    catalog_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    is_catalog_dirty_impl(&catalog_path, &state)
}

/// State-only impl of [`is_catalog_dirty`] — same semantics, takes a borrowed
/// [`AppState`] so integration tests can drive the open + edit + save flow
/// without spinning up a real Tauri runtime.
pub(crate) fn is_catalog_dirty_impl(catalog_path: &str, state: &AppState) -> Result<bool, String> {
    let abs = PathBuf::from(catalog_path);
    let store = state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?;
    Ok(store.get(&abs).is_some_and(|e| e.dirty))
}

// ── M4.2c.1 — project-routed translate, corrections, review status ────────────

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
fn translate_unit_in_project(
    catalog_path: String,
    unit_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<TranslateResult, String> {
    use i18n_harness_backend::{OllamaBackend, TranslationBackend};

    let abs = PathBuf::from(&catalog_path);
    let id = UnitId::from(unit_id);

    let (locale, glossary) = resolve_project_translate_context(&state, &abs)?;
    let backend =
        OllamaBackend::new().map_err(|e| format!("ollama backend construction failed: {e}"))?;
    let backend_name = backend.name().to_string();

    translate_one(
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
/// # Errors
///
/// - `"no project open"` — open a project first.
/// - `"no glossary configured for this project"` — the project manifest has no
///   `[glossary]` block (no glossary path declared or the file is absent).
/// - `"term not found"` — no term with `source == term_id` in the glossary.
/// - `"term is do-not-translate"` — the term's `dnt` flag is set; translating
///   it would contradict the glossary contract.
/// - backend / locale errors forwarded from the translate machinery.
///
/// Available only when the crate is built with the `ollama` feature.
#[cfg(feature = "ollama")]
#[tauri::command]
async fn translate_glossary_term(
    state: tauri::State<'_, AppState>,
    project_path: String,
    term_id: String,
    target_locale: String,
) -> Result<String, String> {
    use i18n_harness_backend::OllamaBackend;

    // Snapshot the glossary and validate the term under the project lock, then
    // drop the lock before the network call.
    let (term_source, glossary, locale) = {
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

        let term_source = glossary_term_source(&glossary, &term_id)?;
        (term_source, glossary, locale)
    };

    let backend =
        OllamaBackend::new().map_err(|e| format!("ollama backend construction failed: {e}"))?;
    dispatch_glossary_term_translation(&backend, &term_source, locale, &glossary)
}

/// Validate a glossary term lookup and return the term's source string.
///
/// Extracts the repeated "find term, check DNT" logic shared by the command
/// and its test helpers. Returns the source string so the caller holds it by
/// value after the glossary borrow ends.
#[cfg(feature = "ollama")]
fn glossary_term_source(glossary: &Glossary, term_id: &str) -> Result<String, String> {
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
fn dispatch_glossary_term_translation(
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
fn resolve_project_translate_context(
    state: &tauri::State<'_, AppState>,
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
fn translate_one(
    project_catalogs: &Mutex<BTreeMap<PathBuf, OpenCatalogEntry>>,
    project: &Mutex<Option<Project>>,
    abs: &Path,
    id: &UnitId,
    backend: &i18n_harness_backend::OllamaBackend,
    backend_name: &str,
    locale: &Locale,
    glossary: Option<&Glossary>,
) -> Result<TranslateResult, String> {
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
            return Ok(TranslateResult {
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

    Ok(TranslateResult {
        unit: merged,
        report,
    })
}

// ── M4.2c.2 — bulk translate with cancellation ───────────────────────────────

/// Selection of units to operate on in a bulk-translate run.
///
/// Vanished/Obsolete units are always excluded regardless of scope — the
/// harness must never overwrite catalog entries the source no longer
/// references. `Finished` units are also always excluded; a "re-translate
/// everything including human-accepted" variant is a meaningful policy
/// decision that belongs to the M4.8 UI design, not this primitive, and
/// would require pre-demoting Finished → Proposed to honour
/// [`UnitState::is_writable`]. Adding a new scope variant later is
/// backward-compatible.
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

/// Start a bulk translation of every in-scope unit in a project-stored catalog.
///
/// Resolves locale, glossary, and backend kind exactly like
/// `translate_unit_in_project`, refuses to start when another bulk run is
/// already operating on the same `(catalog, locale)` pair, then spawns a
/// background thread that processes units sequentially and streams progress
/// via Tauri events. Returns immediately; the worker emits the terminal event
/// when it exits.
///
/// Events emitted (subscribe before the response lands; the worker only starts
/// once this function returns):
/// - `batch-unit-started-<job_id>` just before each backend call, carrying
///   [`BatchUnitStartedPayload`]. Lets the UI light a per-cell spinner.
/// - `batch-progress-<job_id>` after each successful unit, carrying
///   [`BatchProgressPayload`].
/// - `batch-completed-<job_id>` once on clean exit OR cancellation, carrying
///   [`BatchTerminalPayload`] with `cancelled = false` or `cancelled = true`.
/// - `batch-failed-<job_id>` once on hard mid-batch failure, carrying
///   [`BatchTerminalPayload`] with `failed_reason = Some(...)`.
///
/// # Errors
///
/// - `"catalog not open in project"` — call `open_catalog_in_project` first.
/// - `"unknown locale ..."` / `"backend kind ... not supported yet"` — see
///   `translate_unit_in_project`.
/// - `"a translation is already running for this catalog/locale"` — another
///   `translate_batch_in_project` call is in flight for the same pair. Wait
///   for it or cancel it first.
///
/// Available only when the crate is built with the `ollama` feature.
#[cfg(feature = "ollama")]
#[tauri::command]
fn translate_batch_in_project(
    catalog_path: String,
    scope: BatchScope,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<TranslateBatchStarted, String> {
    use i18n_harness_backend::{OllamaBackend, TranslationBackend};

    let abs = PathBuf::from(&catalog_path);

    // 1. Resolve locale / glossary / backend kind via the shared helper.
    let (locale, glossary) = resolve_project_translate_context(&state, &abs)?;
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
            .filter(|u| unit_matches_scope(u, scope))
            .map(|u| u.id.clone())
            .collect()
    };
    let total = unit_ids.len();

    // 3. Refuse concurrent bulk runs on the same (catalog, locale) pair.
    let active_key = (abs.clone(), locale_id.clone());
    {
        let mut active = state
            .active_batches
            .lock()
            .map_err(error::lock_poisoned("active_batches"))?;
        if active.contains(&active_key) {
            return Err("a translation is already running for this catalog/locale".to_string());
        }
        active.insert(active_key.clone());
    }

    // 4. Construct the backend on the calling thread so config errors surface
    //    synchronously; if this fails we release the active-key slot before
    //    returning.
    let backend = match OllamaBackend::new() {
        Ok(b) => b,
        Err(e) => {
            // Release the active slot we just claimed.
            if let Ok(mut active) = state.active_batches.lock() {
                active.remove(&active_key);
            }
            return Err(format!("ollama backend construction failed: {e}"));
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
    let worker_active_key = active_key;

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
                worker_active_key,
            );
        })
        .map_err(|e| {
            // Failed to spawn — undo the registry + active-batches inserts.
            state.jobs.deregister(&job_id);
            if let Ok(mut active) = state.active_batches.lock() {
                active.remove(&(abs.clone(), locale_id.clone()));
            }
            format!("failed to spawn translate-batch worker: {e}")
        })?;

    Ok(TranslateBatchStarted { job_id, total })
}

/// Predicate for `BatchScope` selection. Vanished/Obsolete and Finished are
/// always excluded — see `BatchScope` docs.
#[cfg(feature = "ollama")]
fn unit_matches_scope(unit: &Unit, scope: BatchScope) -> bool {
    match (unit.state, scope) {
        (UnitState::Untranslated, _) => true,
        (UnitState::Proposed, BatchScope::Untranslated) => false,
        (UnitState::Proposed, BatchScope::UntranslatedAndProposed) => true,
        // Vanished / Obsolete / Finished: never.
        _ => false,
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

/// Run the per-unit translate loop on a worker thread, emitting Tauri events
/// for each completed unit and a single terminal event before exiting.
///
/// Always deregisters the job and clears the `active_batches` slot before
/// returning, regardless of outcome. The terminal event is emitted exactly
/// once.
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
    locale: &'static Locale,
    glossary: Option<Glossary>,
    token: cancellation::CancellationToken,
    active_key: (PathBuf, String),
) {
    use tauri::{Emitter, Manager};

    let state = app.state::<AppState>();
    let mut completed: usize = 0;
    let mut terminal: BatchTerminalPayload = BatchTerminalPayload {
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

        let started_payload = BatchUnitStartedPayload {
            unit_id: unit_id.as_str().to_owned(),
            locale: locale.id.to_owned(),
        };
        if let Err(e) = app.emit(&format!("batch-unit-started-{job_id}"), &started_payload) {
            tracing::warn!(job_id = %job_id, error = %e, "batch-unit-started emit failed");
        }

        match translate_one(
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
                let payload = BatchProgressPayload {
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

    // Cleanup: deregister the job and clear the active-batches slot. Always
    // executed on every exit path.
    state.jobs.deregister(&job_id);
    if let Ok(mut active) = state.active_batches.lock() {
        active.remove(&active_key);
    }
}

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
fn cancel_translation(job_id: String, state: tauri::State<'_, AppState>) -> bool {
    state.jobs.cancel(&job_id)
}

/// Record an accepted human edit in the project's `corrections.jsonl`.
///
/// `catalog_path` may be absolute or manifest-relative; the command resolves
/// it against the project's catalog index and errors with
/// `"catalog not registered in project"` if no match is found. Returns the
/// content-addressed correction id.
#[tauri::command]
fn record_correction_in_project(
    req: RecordCorrectionRequest,
    state: tauri::State<'_, AppState>,
) -> Result<CorrectionIdResponse, String> {
    use i18n_harness_project::CorrectionFilter;

    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;

    let (_, manifest_relative) = resolve_catalog_path(project, &req.catalog_path)?;

    let new_corr = NewCorrection {
        catalog: manifest_relative,
        locale: req.locale,
        unit_id: UnitId::from(req.unit_id),
        source: req.source,
        mt_proposal: req.mt_proposal,
        human_target: req.human_target,
        provenance: CorrectionProvenance {
            backend: req.provenance.backend,
            model: req.provenance.model,
            model_version: req.provenance.model_version,
            prompt_template_version: req.provenance.prompt_template_version,
            glossary_version: req.provenance.glossary_version,
        },
        flags_at_correction: req.flags_at_correction,
    };
    let _ = CorrectionFilter::default(); // suppress unused import warning
    let id = project
        .record_correction(new_corr)
        .map_err(|e| e.to_string())?;

    Ok(CorrectionIdResponse { id: id.to_string() })
}

/// List corrections stored in the project, optionally filtered.
///
/// `catalog_path` in the filter (if provided) may be absolute or
/// manifest-relative; it is resolved before the scan. The `curated_only` flag
/// post-filters to corrections that appear in the curated set.
#[tauri::command]
fn list_corrections_in_project(
    filter: ListCorrectionsFilter,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Correction>, String> {
    use i18n_harness_project::CorrectionFilter;

    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;

    let catalog_manifest = filter
        .catalog_path
        .as_deref()
        .map(|p| resolve_catalog_path(project, p).map(|(_, rel)| rel))
        .transpose()?;

    let cf = CorrectionFilter {
        catalog: catalog_manifest,
        locale: filter.locale,
        unit_id: filter.unit_id.map(UnitId::from),
        since: None,
    };

    let mut corrections = project.list_corrections(cf).map_err(|e| e.to_string())?;

    if filter.curated_only {
        let curated = project.curated();
        corrections.retain(|c| curated.contains(&c.id));
    }

    Ok(corrections)
}

/// Promote a correction to the project's curated set.
///
/// `id` must be a `"corr_<12-hex>"` string previously returned by
/// `record_correction_in_project`. Returns `"invalid correction id"` if the
/// string cannot be parsed.
#[tauri::command]
fn promote_correction_to_curated(
    id: String,
    note: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let corr_id = parse_correction_id(&id)?;
    let mut project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_mut().ok_or_else(error::no_project)?;
    project
        .promote_to_curated(corr_id, note)
        .map_err(|e| e.to_string())
}

/// Remove a correction from the project's curated set.
///
/// Idempotent: returns `false` if the id was not in the curated set, `true`
/// if it was removed. Returns `"invalid correction id"` if `id` cannot be
/// parsed.
#[tauri::command]
fn un_curate_correction(id: String, state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let corr_id = parse_correction_id(&id)?;
    let mut project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_mut().ok_or_else(error::no_project)?;
    project.un_curate(&corr_id).map_err(|e| e.to_string())
}

/// Return all entries in the project's curated set.
///
/// Each `CuratedExample` carries its `id`, an optional `note`, and the
/// resolved `correction` data (the full `Correction` record from
/// `corrections.jsonl`). When the underlying correction has been deleted
/// (file rotation, manual edit, project copy without state), the `correction`
/// field is `None` — the dangling entry is still returned so the UI can show
/// it and let the user remove it via `un_curate_correction`.
#[tauri::command]
fn list_curated_in_project(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<i18n_harness_project::CuratedExample>, String> {
    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;
    Ok(project.curated().examples().cloned().collect())
}

/// Record a review-status change for one unit.
///
/// Appends an event to `review.jsonl` via the project's store. As a side
/// effect, the in-memory unit (if the catalog is currently open in the project
/// store) has its `review_status` field set immediately so the UI reflects the
/// change without re-opening the catalog.
///
/// Review state lives in `review.jsonl`, not in the catalog file, so the
/// catalog's `dirty` flag is not set.
#[tauri::command]
fn set_review_status_in_project(
    catalog_path: String,
    unit_id: String,
    input: ReviewStatusInput,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let abs = PathBuf::from(&catalog_path);

    {
        let project_guard = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        let project = project_guard.as_ref().ok_or_else(error::no_project)?;
        let uid = UnitId::from(unit_id.clone());
        project
            .set_review_status(
                &abs,
                &uid,
                input.status,
                input.source_hash_at_review,
                input.reviewer_note,
            )
            .map_err(|e| e.to_string())?;
    }

    // Update the in-memory unit if the catalog is open; silently skip if not.
    if let Ok(mut store) = state.project_catalogs.lock() {
        if let Some(entry) = store.get_mut(&abs) {
            let uid = UnitId::from(unit_id);
            if let Some(unit) = entry.catalog.find_unit_mut(&uid) {
                unit.review_status = input.status;
            }
        }
    }

    Ok(())
}

// ── M4.7 — Project-wide review queue scan ────────────────────────────────────

/// One unit that requires human attention in the project-wide review queue.
///
/// A unit qualifies if `review_status == NeedsReview` OR `flags` is non-empty.
/// Both conditions are surfaced because flags are themselves a "human attention"
/// signal even before an explicit `NeedsReview` event has been recorded.
#[derive(Debug, Serialize)]
pub struct ReviewQueueItem {
    /// Absolute path to the catalog file on disk.
    pub catalog_path: String,
    /// Manifest-relative path for display in the table.
    pub catalog_manifest_path: String,
    /// Target locale id (e.g. `"de_DE"`).
    pub locale: String,
    /// The unit's id string.
    pub unit_id: String,
    /// Source text, truncated to 120 chars at a word boundary where possible.
    pub source_preview: String,
    /// Target text, truncated to 120 chars; empty string when untranslated.
    pub target_preview: String,
    /// Kebab-case flag names; empty when only `NeedsReview` triggered inclusion.
    pub flags: Vec<String>,
    /// Kebab-case `ReviewStatus` variant, or `None` when not set.
    pub review_status: Option<String>,
    /// Kebab-case unit state (`"untranslated"`, `"proposed"`, `"finished"`, …).
    pub state: String,
}

/// Aggregated result of a project-wide review-queue scan.
#[derive(Debug, Serialize)]
pub struct ReviewQueueResponse {
    /// Total units that need review across all catalogs.
    pub total_count: usize,
    /// Per-catalog unit count, keyed by absolute catalog path.
    pub by_catalog: BTreeMap<String, usize>,
    /// All items, sorted by catalog path then unit id.
    pub items: Vec<ReviewQueueItem>,
}

/// Scan every catalog in the open project for units that need human review.
///
/// A unit qualifies if:
/// - `unit.review_status == NeedsReview`, OR
/// - `unit.flags` is non-empty (any flag — gate-produced or model-supplied).
///
/// Catalogs that have already been opened in `project_catalogs` are scanned
/// from the in-memory store. Catalogs that are NOT yet open are extracted via
/// the Qt adapter and folded with `Project::apply_review_state`, then cached
/// into the store with `dirty: false` — the same side-effect as
/// `open_catalog_in_project`.
///
/// Only `qt-ts` catalogs are supported today. Non-`qt-ts` catalogs are
/// skipped with a `tracing::warn!` and will be wired in M4.4/M4.5.
#[tauri::command]
fn scan_project_review_state(
    state: tauri::State<'_, AppState>,
) -> Result<ReviewQueueResponse, String> {
    use i18n_harness_core::ReviewStatus;

    // Collect the list of catalog refs from the project while holding the
    // project lock; drop the lock before any I/O so we do not hold it across
    // extract calls.
    let catalog_refs: Vec<i18n_harness_project::CatalogRef> = {
        let project_guard = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        let project = project_guard.as_ref().ok_or_else(error::no_project)?;
        project.catalogs().to_vec()
    };

    // For each catalog, ensure it is in the project_catalogs store.
    // If it is already open, skip the I/O; otherwise extract + apply and insert.
    for catalog_ref in &catalog_refs {
        // M4.4 wired gettext-po and M4.5 added icu-json; all manifest
        // formats are now handled through `extract_for_format`. The guard
        // remains in case future formats land in the manifest enum before
        // their wiring is finished — surface the gap loudly rather than
        // failing later in extract.
        if !matches!(
            catalog_ref.format,
            i18n_harness_project::CatalogFormat::QtTs
                | i18n_harness_project::CatalogFormat::GettextPo
                | i18n_harness_project::CatalogFormat::IcuJson,
        ) {
            tracing::warn!(
                path = %catalog_ref.manifest_path,
                format = ?catalog_ref.format,
                "scan_project_review_state: format not yet wired"
            );
            continue;
        }

        let abs = std::path::PathBuf::from(&catalog_ref.absolute_path);

        // Check whether already cached.
        let already_open = {
            let store = state
                .project_catalogs
                .lock()
                .map_err(error::lock_poisoned("project_catalogs"))?;
            store.contains_key(&abs)
        };

        if !already_open {
            // Extract from disk via the format-aware backing dispatcher.
            let mut catalog = extract_for_format(&abs, catalog_ref.format)
                .map_err(|e| format!("{} ({}): {e}", catalog_ref.manifest_path, "extract"))?;

            // Fold review state in.
            {
                let project_guard = state
                    .project
                    .lock()
                    .map_err(error::lock_poisoned("project"))?;
                if let Some(project) = project_guard.as_ref() {
                    project.apply_review_state(&abs, catalog.units_mut());
                }
            }

            // Insert into the store.
            state
                .project_catalogs
                .lock()
                .map_err(error::lock_poisoned("project_catalogs"))?
                .insert(
                    abs.clone(),
                    OpenCatalogEntry {
                        catalog,
                        dirty: false,
                    },
                );
        }
    }

    // Now scan all open catalogs and collect review items.
    let store = state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?;

    // Build a manifest-path lookup by absolute path.
    let manifest_path_of: std::collections::HashMap<String, String> = catalog_refs
        .iter()
        .map(|r| (r.absolute_path.clone(), r.manifest_path.clone()))
        .collect();
    let locale_of: std::collections::HashMap<String, String> = catalog_refs
        .iter()
        .map(|r| (r.absolute_path.clone(), r.locale.clone()))
        .collect();

    let mut items: Vec<ReviewQueueItem> = Vec::new();
    let mut by_catalog: BTreeMap<String, usize> = BTreeMap::new();

    // Iterate in BTreeMap order (= absolute path order) for deterministic output.
    for (abs, entry) in store.iter() {
        let abs_str = abs.to_string_lossy().into_owned();
        let manifest_path = manifest_path_of
            .get(&abs_str)
            .cloned()
            .unwrap_or_else(|| abs_str.clone());
        let locale = locale_of.get(&abs_str).cloned().unwrap_or_default();

        let mut catalog_count: usize = 0;

        for unit in entry.catalog.units() {
            // Serialize flags via serde to get the kebab-case strings that the
            // `#[serde(rename_all = "kebab-case")]` attribute on `Flag` produces.
            // `format!("{:?}")` would give PascalCase debug output instead.
            let flags: Vec<String> = unit
                .flags
                .iter()
                .filter_map(|f| {
                    serde_json::to_value(f)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                })
                .collect();

            let needs_review =
                unit.review_status == Some(ReviewStatus::NeedsReview) || !flags.is_empty();
            if !needs_review {
                continue;
            }

            let source_preview = truncate_preview(&unit.source, 120);
            let target_preview = extract_target_text(&unit.target, 120);

            // Serialize review_status via serde for the kebab-case string.
            let review_status_str: Option<String> = unit.review_status.and_then(|s| {
                serde_json::to_value(s)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
            });

            // Serialize state via serde for the kebab-case string.
            let state_str: String = serde_json::to_value(unit.state)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_else(|| "unknown".to_string());

            items.push(ReviewQueueItem {
                catalog_path: abs_str.clone(),
                catalog_manifest_path: manifest_path.clone(),
                locale: locale.clone(),
                unit_id: unit.id.to_string(),
                source_preview,
                target_preview,
                flags,
                review_status: review_status_str,
                state: state_str.to_string(),
            });
            catalog_count += 1;
        }

        if catalog_count > 0 {
            by_catalog.insert(abs_str, catalog_count);
        }
    }

    // Sort: by catalog_path then unit_id (both strings, BTreeMap already gave
    // catalog order; within each catalog units came in document order — resort
    // by unit_id for stable output).
    items.sort_by(|a, b| {
        a.catalog_path
            .cmp(&b.catalog_path)
            .then_with(|| a.unit_id.cmp(&b.unit_id))
    });

    let total_count = items.len();

    Ok(ReviewQueueResponse {
        total_count,
        by_catalog,
        items,
    })
}

/// Truncate `s` to at most `max_chars` characters, preferring a word boundary.
///
/// `cutoff` and `search_start` are byte indices derived from `char_indices`,
/// so they always land on UTF-8 char boundaries — slicing the string between
/// them never panics on multi-byte text. (Codex P1 on PR #33: a previous
/// implementation derived `search_start` via byte subtraction, which crashed
/// `scan_project_review_state` on long non-ASCII previews.)
fn truncate_preview(s: &str, max_chars: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= max_chars {
        return s;
    }
    let cutoff = s
        .char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    // Try to break at a word boundary within the last 20 chars of the limit.
    let search_start_char = max_chars.saturating_sub(20);
    let search_start = s
        .char_indices()
        .nth(search_start_char)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let best = s[search_start..cutoff]
        .rfind(' ')
        .map(|off| search_start + off)
        .unwrap_or(cutoff);
    format!("{}…", s[..best].trim_end())
}

#[cfg(test)]
mod truncate_preview_tests {
    use super::truncate_preview;

    #[test]
    fn ascii_under_limit_returns_unchanged() {
        assert_eq!(truncate_preview("hello world", 100), "hello world");
    }

    #[test]
    fn ascii_over_limit_breaks_at_word() {
        let out = truncate_preview("the quick brown fox jumps over the lazy dog", 20);
        assert!(out.ends_with('…'));
        assert!(out.len() <= 25);
    }

    #[test]
    fn multibyte_text_does_not_panic_anywhere_near_cutoff() {
        // Long Cyrillic/CJK strings where every char is 2-3 bytes —
        // the previous byte-subtraction crashed somewhere in here.
        let cyr: String = "Здравствуйте мир ".repeat(20);
        let _ = truncate_preview(&cyr, 50);
        let cjk: String = "你好世界今天天气真好".repeat(20);
        let _ = truncate_preview(&cjk, 50);
        let mixed = format!("hello {} world {}", cyr, cjk);
        let _ = truncate_preview(&mixed, 50);
    }

    #[test]
    fn newlines_collapse_to_spaces() {
        let out = truncate_preview("line one\nline two\nline three\nline four", 20);
        assert!(!out.contains('\n'));
    }
}

/// Extract a plain-text preview from a `Target`, truncated to `max_chars`.
fn extract_target_text(target: &i18n_harness_core::Target, max_chars: usize) -> String {
    match target {
        i18n_harness_core::Target::Singular { text: Some(t) } => truncate_preview(t, max_chars),
        i18n_harness_core::Target::Plural { forms } => {
            // Use the first non-None form as the preview.
            forms
                .iter()
                .flatten()
                .next()
                .map(|t| truncate_preview(t, max_chars))
                .unwrap_or_default()
        }
        _ => String::new(),
    }
}

// ── M4.6.2 — Accept (clear flags, mark Reviewed) ────────────────────────────

/// Accept a unit as reviewed: clear its flags and flag notes, then append a
/// `Reviewed` event to `review.jsonl`.
///
/// The unit's `state` is left unchanged (per M4.3a.1: the human controls the
/// `Proposed → Finished` transition via Save/accept flows). The catalog is
/// marked dirty because clearing flags is a meaningful edit that the next Save
/// will persist.
///
/// Lock ordering: acquire `project_catalogs` first for the mutation, drop it,
/// then acquire `project` for the durable review write. This mirrors the
/// pattern established in `translate_unit_in_project`.
#[tauri::command]
fn accept_unit_in_project(
    catalog_path: String,
    unit_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Unit, String> {
    use i18n_harness_core::{FlagSet, ReviewStatus};
    use std::collections::BTreeMap;

    let abs = PathBuf::from(&catalog_path);
    let uid = UnitId::from(unit_id.clone());

    // Read the unit's source_hash without mutating, so we can do the durable
    // write first. Codex P2: clearing flags before the fallible review.jsonl
    // append would leave the in-memory state mutated on append failure, and
    // a later Save would persist a cleared-flags state with no matching
    // review event.
    let source_hash = {
        let store = state
            .project_catalogs
            .lock()
            .map_err(error::lock_poisoned("project_catalogs"))?;
        let entry = store
            .get(&abs)
            .ok_or_else(|| "catalog not open in project".to_string())?;
        let unit = entry
            .catalog
            .units()
            .iter()
            .find(|u| u.id == uid)
            .ok_or_else(|| format!("unit not found: {unit_id}"))?;
        unit.source_hash.clone().unwrap_or_default()
    };
    // project_catalogs lock is now dropped.

    // Durable write first. If this fails, the in-memory state is untouched
    // and the caller can retry safely.
    {
        let project_guard = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        let project = project_guard.as_ref().ok_or_else(error::no_project)?;
        project
            .set_review_status(&abs, &uid, Some(ReviewStatus::Reviewed), source_hash, None)
            .map_err(|e| format!("set_review_status failed: {e}"))?;
    }

    // Durable write succeeded — now mutate the in-memory unit and return it.
    let mut store = state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?;
    let entry = store
        .get_mut(&abs)
        .ok_or_else(|| "catalog not open in project".to_string())?;
    let unit = entry
        .catalog
        .find_unit_mut(&uid)
        .ok_or_else(|| format!("unit not found: {unit_id}"))?;

    unit.flags = FlagSet::new();
    unit.flag_notes = BTreeMap::new();
    unit.review_status = Some(ReviewStatus::Reviewed);
    // Accept also transitions Proposed → Finished. The HANDOFF redesign
    // treats Accept as the manual finish action; the gate's hard-flag guard
    // (caller-side) already prevents accepting an unsound unit. Other states
    // are left alone (Finished stays Finished; Untranslated stays
    // Untranslated, though the caller should not invoke this in that case).
    if matches!(unit.state, UnitState::Proposed) {
        unit.state = UnitState::Finished;
    }
    let merged = unit.clone();
    entry.dirty = true;

    Ok(merged)
}

// ── M4.3c — Settings view mutation commands ───────────────────────────────────

/// Add a new catalog entry to the project manifest and persist it.
///
/// Errors if the path already exists in the manifest (`DuplicateCatalogPath`)
/// or the file is not found on disk (`CatalogNotFound`). Returns a fresh
/// `ProjectOpenResponse` so the UI can re-render without an extra round trip.
#[tauri::command]
fn add_catalog_to_project(
    entry: CatalogEntry,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let mut guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = guard.as_mut().ok_or_else(error::no_project)?;
    project.add_catalog(entry).map_err(|e| e.to_string())?;
    project.save_manifest().map_err(|e| e.to_string())?;
    let summary = project.summary();
    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}

/// Remove the catalog at `path` (manifest-relative) from the project manifest
/// and persist it.
///
/// Also evicts the catalog from the in-memory `project_catalogs` store so
/// the UI cannot navigate to a catalog that no longer exists in the manifest.
/// Idempotent — returns successfully even if no entry matched (removed = false
/// is not surfaced to the UI; the fresh summary is sufficient).
#[tauri::command]
fn remove_catalog_from_project(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let manifest_path = std::path::PathBuf::from(&path);

    // Acquire project lock, mutate, and release before taking project_catalogs.
    let (summary, abs_path) = {
        let mut guard = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        let project = guard.as_mut().ok_or_else(error::no_project)?;

        // Resolve the absolute path before the mutation for the catalog eviction
        // step below — after removal the catalog() lookup would return None.
        let abs = project
            .catalog(&manifest_path)
            .map(|r| std::path::PathBuf::from(&r.absolute_path));

        project
            .remove_catalog(&manifest_path)
            .map_err(|e| e.to_string())?;
        project.save_manifest().map_err(|e| e.to_string())?;
        let summary = project.summary();
        (summary, abs)
    };

    // Evict from the open-catalog store (best-effort; no error if absent).
    if let Some(abs) = abs_path {
        if let Ok(mut store) = state.project_catalogs.lock() {
            store.remove(&abs);
        }
    }

    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}

/// Upsert a locale config block in the project manifest and persist it.
///
/// Creates the `[locales.<id>]` block if absent; updates only the fields
/// present in `config`, leaving unknown sibling keys untouched (forward-compat).
#[tauri::command]
fn update_locale_in_project(
    id: String,
    config: LocaleConfig,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let mut guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = guard.as_mut().ok_or_else(error::no_project)?;
    project
        .update_locale(&id, config)
        .map_err(|e| e.to_string())?;
    project.save_manifest().map_err(|e| e.to_string())?;
    let summary = project.summary();
    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}

/// Remove a locale config block from the project manifest and persist it.
///
/// Idempotent — no error if the block did not exist.
#[tauri::command]
fn remove_locale_from_project(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let mut guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = guard.as_mut().ok_or_else(error::no_project)?;
    project.remove_locale(&id).map_err(|e| e.to_string())?;
    project.save_manifest().map_err(|e| e.to_string())?;
    let summary = project.summary();
    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}

/// Replace the `[backend.default]` block in the project manifest and persist it.
///
/// Creates the block if absent. Preserves unknown sibling keys.
#[tauri::command]
fn set_backend_in_project(
    config: BackendConfig,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let mut guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = guard.as_mut().ok_or_else(error::no_project)?;
    project.set_backend(config).map_err(|e| e.to_string())?;
    project.save_manifest().map_err(|e| e.to_string())?;
    let summary = project.summary();
    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}

/// Replace the `[glossary]` block in the project manifest and persist it.
///
/// Creates the block if absent.
#[tauri::command]
fn set_glossary_in_project(
    config: GlossaryConfig,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let mut guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = guard.as_mut().ok_or_else(error::no_project)?;
    project.set_glossary(config).map_err(|e| e.to_string())?;
    project.save_manifest().map_err(|e| e.to_string())?;
    let summary = project.summary();
    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}

/// Replace the `[prompts]` block in the project manifest and persist it.
///
/// Creates the block if absent.
#[tauri::command]
fn set_prompts_in_project(
    config: PromptsConfig,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectOpenResponse, String> {
    let mut guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = guard.as_mut().ok_or_else(error::no_project)?;
    project.set_prompts(config).map_err(|e| e.to_string())?;
    project.save_manifest().map_err(|e| e.to_string())?;
    let summary = project.summary();
    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
}

// ── M4.9 — In-app prompt evaluation ──────────────────────────────────────────

/// Synchronous response from `run_evaluation_in_project`. The worker thread
/// runs in the background; the frontend subscribes to events keyed by `job_id`.
#[cfg(feature = "ollama")]
#[derive(Debug, Serialize)]
pub struct EvaluationStarted {
    /// Opaque process-unique job id (UUID v4 hex, no hyphens).
    pub job_id: String,
    /// Number of curated examples the worker will evaluate.
    pub total: usize,
}

/// Per-example progress event, emitted on `eval-progress-<job_id>` after each
/// completed backend call in the evaluation worker.
#[cfg(feature = "ollama")]
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
#[cfg(feature = "ollama")]
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
/// Returns `"No curated examples yet; promote some corrections first"` when the
/// curated set is empty, and `"an evaluation is already running"` when another
/// run is active (single global evaluation at a time).
///
/// Available only when the crate is built with the `ollama` feature.
#[cfg(feature = "ollama")]
#[tauri::command]
fn run_evaluation_in_project(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<EvaluationStarted, String> {
    use i18n_harness_backend::{OllamaBackend, TranslationBackend, TranslationOutcome};
    use i18n_harness_core::{Batch, BatchKey};
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
        Option<Glossary>,
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

    // 2. Refuse if any evaluation job is already running. We use a dedicated
    //    active_batches key format so the eval slot is separate from per-catalog
    //    batch slots.
    let eval_key = (PathBuf::from("__eval__"), "__eval__".to_string());
    {
        let mut active = state
            .active_batches
            .lock()
            .map_err(error::lock_poisoned("active_batches"))?;
        if active.contains(&eval_key) {
            return Err("an evaluation is already running".to_string());
        }
        active.insert(eval_key.clone());
    }

    // 3. Construct backend on the calling thread.
    let backend = match OllamaBackend::new() {
        Ok(b) => b,
        Err(e) => {
            if let Ok(mut active) = state.active_batches.lock() {
                active.remove(&eval_key);
            }
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
    let worker_eval_key = eval_key.clone();
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

                let progress = EvaluationProgressPayload {
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

            let terminal = EvaluationTerminalPayload {
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

            // Cleanup.
            app_state.jobs.deregister(&worker_job_id);
            if let Ok(mut active) = app_state.active_batches.lock() {
                active.remove(&worker_eval_key);
            }
        })
        .map_err(|e| {
            state.jobs.deregister(&job_id);
            if let Ok(mut active) = state.active_batches.lock() {
                active.remove(&eval_key);
            }
            format!("failed to spawn evaluation worker: {e}")
        })?;

    Ok(EvaluationStarted { job_id, total })
}

/// List all past evaluation runs for the current project, newest-first.
///
/// Returns an empty vec if no project is open or if no runs have been
/// recorded yet.
#[tauri::command]
fn list_evaluation_runs_in_project(
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

// ── M4.10 — Tuning bundle export ──────────────────────────────────────────────

/// Wire-format response from `export_tuning_bundle_in_project`.
///
/// Mirrors [`i18n_harness_project::TuningBundleSummary`] with all path fields
/// as `String` for TypeScript interop.
#[derive(Debug, Serialize)]
pub struct ExportTuningBundleResponse {
    /// Absolute path to the exported bundle directory.
    pub path: String,
    /// Number of resolved examples written to `examples.jsonl`.
    pub examples_count: usize,
    /// Locale ids that appear in at least one example.
    pub locales: Vec<String>,
    /// `true` if `score.json` was written (prior evaluation existed).
    pub has_score: bool,
    /// Prompt template version identifier baked into `prompt.txt`.
    pub prompt_template_version: String,
}

/// Export a tuning bundle to `.i18n-harness/tuning/<timestamp>/`.
///
/// Calls `Project::export_tuning_bundle` under a brief project lock. The
/// bundle contains the curated example set, the active prompt template,
/// the latest evaluation run (if one exists), the locale config, and a copy
/// of the skill README. Returns a summary of what was written.
///
/// Errors:
/// - `"no project open"` when no project is loaded.
/// - `"no curated examples; promote some corrections first"` when the curated
///   set is empty.
/// - I/O error messages for filesystem failures.
#[tauri::command]
fn export_tuning_bundle_in_project(
    state: tauri::State<'_, AppState>,
) -> Result<ExportTuningBundleResponse, String> {
    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;
    let summary = project.export_tuning_bundle().map_err(|e| e.to_string())?;
    Ok(ExportTuningBundleResponse {
        path: summary.path,
        examples_count: summary.examples_count,
        locales: summary.locales,
        has_score: summary.has_score,
        prompt_template_version: summary.prompt_template_version,
    })
}

/// List previously-exported tuning bundles for the open project, newest-first.
///
/// Reads the `.i18n-harness/tuning/` directory and returns one summary per
/// bundle subdirectory that contains a valid `examples.jsonl`. Returns an
/// empty array when no bundles have been exported or no project is open.
#[tauri::command]
fn list_tuning_bundles_in_project(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ExportTuningBundleResponse>, String> {
    let project_guard = state
        .project
        .lock()
        .map_err(error::lock_poisoned("project"))?;
    let project = project_guard.as_ref().ok_or_else(error::no_project)?;
    let bundles = project.list_tuning_bundles().map_err(|e| e.to_string())?;
    Ok(bundles
        .into_iter()
        .map(|s| ExportTuningBundleResponse {
            path: s.path,
            examples_count: s.examples_count,
            locales: s.locales,
            has_score: s.has_score,
            prompt_template_version: s.prompt_template_version,
        })
        .collect())
}

// ── Internal helpers ─────────────────────────────────────────────────────────

fn build_catalog_response(path: &std::path::Path, catalog: &Catalog) -> CatalogResponse {
    CatalogResponse {
        path: path.to_string_lossy().into_owned(),
        unit_count: catalog.units().len(),
        language: catalog.language().map(str::to_owned),
        units: catalog.units().to_vec(),
    }
}

fn build_catalog_response_backing(
    path: &std::path::Path,
    catalog: &BackingCatalog,
) -> CatalogResponse {
    CatalogResponse {
        path: path.to_string_lossy().into_owned(),
        unit_count: catalog.units().len(),
        language: catalog.language().map(str::to_owned),
        units: catalog.units().to_vec(),
    }
}

/// Return `(absolute_path, manifest_relative_path)` for a catalog path that
/// may be absolute or manifest-relative. Errors with
/// `"catalog not registered in project"` if no registered catalog matches.
fn resolve_catalog_path(project: &Project, input: &str) -> Result<(PathBuf, PathBuf), String> {
    let input_path = PathBuf::from(input);
    let catalog_ref = project
        .catalog(&input_path)
        .ok_or_else(|| "catalog not registered in project".to_string())?;
    let absolute = PathBuf::from(&catalog_ref.absolute_path);
    let manifest_relative = PathBuf::from(&catalog_ref.manifest_path);
    Ok((absolute, manifest_relative))
}

/// Parse a `"corr_<12-hex>"` string into a [`CorrectionId`].
///
/// Returns a generic `"invalid correction id"` on failure to avoid leaking
/// internal format details to the caller.
fn parse_correction_id(s: &str) -> Result<CorrectionId, String> {
    if s.starts_with("corr_") && s.len() == 17 && s[5..].chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(CorrectionId(s.to_owned()))
    } else {
        Err("invalid correction id".to_string())
    }
}

/// Write `content` (UTF-8 text) to `path`, creating or overwriting the file.
///
/// This is a pure filesystem thin-wrapper used by the frontend "Save .md"
/// flow in ProofreadView — no business logic here.
#[tauri::command]
fn write_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content.as_bytes()).map_err(|e| format!("write failed: {e}"))
}

/// Entry point invoked from `main.rs` (and from the mobile entry point
/// macro when the crate is built for iOS/Android).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default());

    #[cfg(feature = "ollama")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        app_version,
        open_catalog,
        update_unit_target,
        save_catalog,
        discard_changes,
        translate_unit,
        list_locales,
        load_glossary,
        save_glossary,
        load_metrics,
        open_project,
        discover_project,
        create_project,
        close_project,
        current_project_summary,
        list_catalogs,
        save_manifest,
        open_catalog_in_project,
        update_unit_target_in_project,
        save_catalog_in_project,
        save_all_dirty,
        discard_changes_in_project,
        list_open_catalogs,
        is_catalog_dirty,
        translate_unit_in_project,
        translate_glossary_term,
        translate_batch_in_project,
        cancel_translation,
        record_correction_in_project,
        list_corrections_in_project,
        promote_correction_to_curated,
        un_curate_correction,
        list_curated_in_project,
        set_review_status_in_project,
        accept_unit_in_project,
        add_catalog_to_project,
        remove_catalog_from_project,
        update_locale_in_project,
        remove_locale_from_project,
        set_backend_in_project,
        set_glossary_in_project,
        set_prompts_in_project,
        scan_project_review_state,
        run_evaluation_in_project,
        list_evaluation_runs_in_project,
        export_tuning_bundle_in_project,
        list_tuning_bundles_in_project,
        write_text_file,
    ]);

    #[cfg(not(feature = "ollama"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        app_version,
        open_catalog,
        update_unit_target,
        save_catalog,
        discard_changes,
        list_locales,
        load_glossary,
        save_glossary,
        load_metrics,
        open_project,
        discover_project,
        create_project,
        close_project,
        current_project_summary,
        list_catalogs,
        save_manifest,
        open_catalog_in_project,
        update_unit_target_in_project,
        save_catalog_in_project,
        save_all_dirty,
        discard_changes_in_project,
        list_open_catalogs,
        is_catalog_dirty,
        cancel_translation,
        record_correction_in_project,
        list_corrections_in_project,
        promote_correction_to_curated,
        un_curate_correction,
        list_curated_in_project,
        set_review_status_in_project,
        accept_unit_in_project,
        add_catalog_to_project,
        remove_catalog_from_project,
        update_locale_in_project,
        remove_locale_from_project,
        set_backend_in_project,
        set_glossary_in_project,
        set_prompts_in_project,
        scan_project_review_state,
        list_evaluation_runs_in_project,
        export_tuning_bundle_in_project,
        list_tuning_bundles_in_project,
        write_text_file,
    ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ── Tests: translate_glossary_term helpers ────────────────────────────────────

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

// ── Tests: BatchUnitStartedPayload structure ──────────────────────────────────

#[cfg(all(test, feature = "ollama"))]
mod batch_unit_started_tests {
    use super::BatchUnitStartedPayload;

    #[test]
    fn payload_fields_round_trip_through_serde() {
        let p = BatchUnitStartedPayload {
            unit_id: "u::hello".to_owned(),
            locale: "de_DE".to_owned(),
        };
        let json = serde_json::to_string(&p).expect("serialize");
        let back: serde_json::Value = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back["unit_id"], "u::hello");
        assert_eq!(back["locale"], "de_DE");
    }

    // The loop in `run_batch_worker` emits one `batch-unit-started-<id>` per
    // iteration (one per unit_id in the batch). This structural test checks that
    // the payload count equals the number of units: it drives the payload
    // construction path in isolation so clippy/miri can also exercise it.
    #[test]
    fn one_started_payload_constructed_per_unit() {
        let unit_ids = ["u1", "u2", "u3"];
        let locale_id = "de_DE";
        let payloads: Vec<BatchUnitStartedPayload> = unit_ids
            .iter()
            .map(|uid| BatchUnitStartedPayload {
                unit_id: (*uid).to_owned(),
                locale: locale_id.to_owned(),
            })
            .collect();
        assert_eq!(payloads.len(), unit_ids.len());
        for (p, expected_id) in payloads.iter().zip(&unit_ids) {
            assert_eq!(&p.unit_id, expected_id);
            assert_eq!(p.locale, locale_id);
        }
    }
}

// ── Tests: end-to-end open → edit → save → reopen contract ─────────────────────
//
// These tests bypass the Tauri runtime and drive the *_impl helpers directly.
// They exist because translator-facing "Save all" complaints traced back to
// the question: when a Proposed edit is applied to an Untranslated unit,
// does the bytes-on-disk → reopen → in-memory state round-trip preserve the
// edit? The single test that follows answers that question without any
// frontend wiring in scope; if it ever turns red, the bug is in Rust.
#[cfg(test)]
mod save_roundtrip_integration_tests {
    use super::{
        AppState, TargetEdit, is_catalog_dirty_impl, open_catalog_in_project_impl,
        open_project_impl, save_all_dirty_impl, update_unit_target_in_project_impl,
    };
    use i18n_harness_core::{Target, UnitState};
    use std::fs;
    use tempfile::TempDir;

    /// Minimal Qt `.ts` fixture: one unfinished singular unit. Exactly the
    /// shape that Linguist + the harness's extract path produce on first run.
    /// Body bytes are stable across runs.
    const TS_BEFORE: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n<TS version=\"2.1\" language=\"de_DE\" sourcelanguage=\"en\">\n<context>\n    <name>MainWindow</name>\n    <message>\n        <location filename=\"src/mainwindow.cpp\" line=\"42\"/>\n        <source>Hello</source>\n        <translation type=\"unfinished\"></translation>\n    </message>\n</context>\n</TS>\n";

    const MANIFEST: &str = r#"[project]
name = "save-roundtrip-test"
schema = 1

[locales.de_DE]
register = "neutral"

[[catalogs]]
path = "translations/app_de.ts"
format = "qt-ts"
locale = "de_DE"
"#;

    fn write_project(dir: &TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = dir.path().to_path_buf();
        let manifest_path = root.join("i18n-harness.toml");
        fs::write(&manifest_path, MANIFEST).expect("write manifest");
        let translations_dir = root.join("translations");
        fs::create_dir_all(&translations_dir).expect("create translations dir");
        let ts_path = translations_dir.join("app_de.ts");
        fs::write(&ts_path, TS_BEFORE).expect("write .ts");
        (root, ts_path)
    }

    /// The forensic test the maintainer asked for. Open a fresh project, edit
    /// one unit, save all, then drop the AppState (simulating an app restart)
    /// and reopen. The edit must survive every step:
    ///
    ///   - in-memory state after edit: `state == Proposed`, dirty == true.
    ///   - in-memory state after save: dirty == false.
    ///   - on-disk file after save: contains `Hallo Welt` and still
    ///     carries `type="unfinished"` (write contract for Proposed).
    ///   - in-memory state after reopen (fresh AppState): the parser must
    ///     promote `type="unfinished" + non-empty body` back to Proposed,
    ///     so the unit comes back with `state == Proposed` and the
    ///     translated text intact.
    ///
    /// If this test fails, the bug is on the Rust side. If it passes, the
    /// "Save all loses changes" report is necessarily a frontend issue
    /// (the textarea draft never made it into the IPC layer, or the IPC
    /// race lost the edit).
    #[test]
    fn edit_save_reopen_preserves_proposed_translation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (root, ts_path) = write_project(&dir);

        // Phase 1 — open project, open catalog, confirm Untranslated.
        let state = AppState::default();
        let open_resp = open_project_impl(&root, &state).expect("open project");
        // Use the path the project resolved — on macOS the temp dir lives at
        // /var/folders/... which symlinks to /private/var/folders/..., and the
        // canonicalized form on disk does not match the manifest-resolved form.
        // The project's own absolute_path is the IPC handle the UI uses too.
        let abs_path = open_resp
            .summary
            .catalogs
            .iter()
            .find(|c| {
                std::path::Path::new(&c.absolute_path).file_name()
                    == Some(ts_path.file_name().unwrap())
            })
            .expect("project must know about the catalog")
            .absolute_path
            .clone();
        let opened = open_catalog_in_project_impl(&abs_path, &state).expect("open catalog");
        assert_eq!(opened.unit_count, 1, "fixture has one unit");
        let unit_id = opened.units[0].id.to_string();
        assert_eq!(
            opened.units[0].state,
            UnitState::Untranslated,
            "before edit: state must be Untranslated"
        );

        // Phase 2 — apply an edit; state machine promotes to Proposed.
        let edited = update_unit_target_in_project_impl(
            &abs_path,
            &unit_id,
            TargetEdit::Singular {
                text: Some("Hallo Welt".to_owned()),
            },
            &state,
        )
        .expect("update edit");
        assert_eq!(
            edited.state,
            UnitState::Proposed,
            "after edit: state must be Proposed"
        );
        match &edited.target {
            Target::Singular { text } => assert_eq!(
                text.as_deref(),
                Some("Hallo Welt"),
                "after edit: target text must be the new value"
            ),
            Target::Plural { .. } => panic!("expected singular target"),
        }
        assert!(
            is_catalog_dirty_impl(&abs_path, &state).expect("dirty check"),
            "after edit: catalog must be marked dirty"
        );

        // Phase 3 — save all dirty; expect one saved entry, no failure.
        let save = save_all_dirty_impl(&state).expect("save all dirty");
        assert_eq!(save.saved.len(), 1, "save_all_dirty: one saved entry");
        assert_eq!(
            save.saved[0].path, abs_path,
            "save_all_dirty: path must match"
        );
        assert!(save.failed_path.is_none(), "save_all_dirty: no failure");
        assert!(
            !is_catalog_dirty_impl(&abs_path, &state).expect("dirty check post-save"),
            "post-save: dirty flag must be cleared"
        );

        // Phase 4 — read bytes off disk; assert write contract.
        let bytes_after = fs::read_to_string(&abs_path).expect("read .ts after save");
        assert!(
            bytes_after.contains("Hallo Welt"),
            "on-disk: must contain the edited text\n----\n{bytes_after}\n----"
        );
        assert!(
            bytes_after.contains("type=\"unfinished\""),
            "on-disk: must retain type=\"unfinished\" (Proposed write contract)\n----\n{bytes_after}\n----"
        );

        // Phase 5 — drop AppState (simulate app restart), reopen, verify
        // the edit survives the parse path.
        drop(state);
        let state2 = AppState::default();
        open_project_impl(&root, &state2).expect("reopen project");
        let reopened = open_catalog_in_project_impl(&abs_path, &state2).expect("reopen catalog");
        assert_eq!(reopened.unit_count, 1, "after reopen: still one unit");
        let unit = &reopened.units[0];
        assert_eq!(
            unit.state,
            UnitState::Proposed,
            "after reopen: state must round-trip as Proposed (parse promotion)"
        );
        match &unit.target {
            Target::Singular { text } => assert_eq!(
                text.as_deref(),
                Some("Hallo Welt"),
                "after reopen: target text must round-trip intact"
            ),
            Target::Plural { .. } => panic!("expected singular target after reopen"),
        }
        assert!(
            !is_catalog_dirty_impl(&abs_path, &state2).expect("dirty after reopen"),
            "after reopen: catalog must be clean"
        );
    }
}
