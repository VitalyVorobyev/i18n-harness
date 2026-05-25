//! Tauri desktop shell for the i18n-harness.
//!
//! This crate is a *thin* wrapper over the library API. Translation,
//! gate, and adapter logic live in the workspace's pure-Rust crates and
//! remain testable as a headless library. Commands here marshal
//! arguments, invoke the library, and serialize results back to the
//! JavaScript layer. No business logic that does not fit on a single
//! screen of glue belongs here.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use i18n_harness_adapter_qt::Catalog;
use i18n_harness_core::{Target, Unit, UnitId, UnitState};
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
}

/// An entry in the project-scoped multi-catalog store.
struct OpenCatalogEntry {
    catalog: Catalog,
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
    let mut current = state.catalog.lock().map_err(lock_poisoned)?;
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
    let mut current = state.catalog.lock().map_err(lock_poisoned)?;
    let open = current.as_mut().ok_or_else(no_catalog)?;
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
    let current = state.catalog.lock().map_err(lock_poisoned)?;
    let open = current.as_ref().ok_or_else(no_catalog)?;
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
    *state.glossary.lock().map_err(glossary_lock_poisoned)? = Some(glossary);
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
    let mut current = state.catalog.lock().map_err(lock_poisoned)?;
    let open = current.as_mut().ok_or_else(no_catalog)?;
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

    let mut current = state.catalog.lock().map_err(lock_poisoned)?;
    let open = current.as_mut().ok_or_else(no_catalog)?;
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
        .map_err(glossary_lock_poisoned)?
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
    let root_path = PathBuf::from(&root);
    let (project, warnings) = Project::open(&root_path).map_err(|e| e.to_string())?;
    let summary = project.summary();
    let glossary_for_slot = project.glossary().cloned();

    {
        let mut current = state.project.lock().map_err(project_lock_poisoned)?;
        *current = Some(project);
    }
    {
        let mut g = state.glossary.lock().map_err(glossary_lock_poisoned)?;
        *g = glossary_for_slot;
    }
    {
        let mut c = state.catalog.lock().map_err(lock_poisoned)?;
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
        let mut current = state.project.lock().map_err(project_lock_poisoned)?;
        *current = Some(project);
    }
    {
        let mut g = state.glossary.lock().map_err(glossary_lock_poisoned)?;
        *g = glossary_for_slot;
    }
    {
        let mut c = state.catalog.lock().map_err(lock_poisoned)?;
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
    *state.project.lock().map_err(project_lock_poisoned)? = None;
    *state.glossary.lock().map_err(glossary_lock_poisoned)? = None;
    *state.catalog.lock().map_err(lock_poisoned)? = None;
    state
        .project_catalogs
        .lock()
        .map_err(project_catalogs_lock_poisoned)?
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
    let current = state.project.lock().map_err(project_lock_poisoned)?;
    Ok(current.as_ref().map(Project::summary))
}

/// List catalogs declared in the currently-open project.
///
/// Errors with `"no project open"` when the project slot is empty — the UI
/// should gate this command behind a successful `open_project`.
#[tauri::command]
fn list_catalogs(state: tauri::State<'_, AppState>) -> Result<Vec<CatalogRef>, String> {
    let current = state.project.lock().map_err(project_lock_poisoned)?;
    let project = current.as_ref().ok_or_else(no_project)?;
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
    let current = state.project.lock().map_err(project_lock_poisoned)?;
    let project = current.as_ref().ok_or_else(no_project)?;
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
    let path = PathBuf::from(&catalog_path);

    let project_guard = state.project.lock().map_err(project_lock_poisoned)?;
    let project = project_guard.as_ref().ok_or_else(no_project)?;

    let catalog_ref = project
        .catalog(&path)
        .ok_or_else(|| format!("catalog not in project: {catalog_path}"))?;
    let abs = PathBuf::from(&catalog_ref.absolute_path);

    let mut catalog =
        i18n_harness_adapter_qt::extract(&abs).map_err(|e| format!("extract failed: {e}"))?;
    project.apply_review_state(&abs, catalog.units_mut());

    let response = build_catalog_response(&abs, &catalog);
    drop(project_guard);

    state
        .project_catalogs
        .lock()
        .map_err(project_catalogs_lock_poisoned)?
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
    let abs = PathBuf::from(&catalog_path);
    let mut store = state
        .project_catalogs
        .lock()
        .map_err(project_catalogs_lock_poisoned)?;
    let entry = store.get_mut(&abs).ok_or_else(no_catalog_in_project)?;
    let id = UnitId::from(unit_id);
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
        .map_err(project_catalogs_lock_poisoned)?;
    let entry = store.get_mut(&abs).ok_or_else(no_catalog_in_project)?;
    let units = entry.catalog.units().to_vec();
    i18n_harness_adapter_qt::apply(&entry.catalog, &units, &abs)
        .map_err(|e| format!("apply failed: {e}"))?;
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
    let mut store = state
        .project_catalogs
        .lock()
        .map_err(project_catalogs_lock_poisoned)?;
    let mut saved: Vec<SaveSummary> = Vec::new();
    for (abs, entry) in store.iter_mut() {
        if !entry.dirty {
            continue;
        }
        let units = entry.catalog.units().to_vec();
        match i18n_harness_adapter_qt::apply(&entry.catalog, &units, abs) {
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
                    failed_reason: Some(format!("apply failed: {e}")),
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

    let project_guard = state.project.lock().map_err(project_lock_poisoned)?;
    let project = project_guard.as_ref().ok_or_else(no_project)?;

    // Confirm the catalog is in the store before doing I/O.
    {
        let store = state
            .project_catalogs
            .lock()
            .map_err(project_catalogs_lock_poisoned)?;
        if !store.contains_key(&abs) {
            return Err(no_catalog_in_project());
        }
    }

    let mut catalog =
        i18n_harness_adapter_qt::extract(&abs).map_err(|e| format!("extract failed: {e}"))?;
    project.apply_review_state(&abs, catalog.units_mut());

    let response = build_catalog_response(&abs, &catalog);
    drop(project_guard);

    state
        .project_catalogs
        .lock()
        .map_err(project_catalogs_lock_poisoned)?
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
        .map_err(project_catalogs_lock_poisoned)?;
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
    let abs = PathBuf::from(&catalog_path);
    let store = state
        .project_catalogs
        .lock()
        .map_err(project_catalogs_lock_poisoned)?;
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
    use i18n_harness_backend::{
        FailureKind, OllamaBackend, TranslationBackend, TranslationOutcome,
    };
    use i18n_harness_core::{Batch, BatchKey, FlagSet, ReviewStatus};
    use i18n_harness_project::BackendKind;

    let abs = PathBuf::from(&catalog_path);

    // Resolve locale and backend config from the project slot first.
    let (locale, glossary) = {
        let project_guard = state.project.lock().map_err(project_lock_poisoned)?;
        let project = project_guard.as_ref().ok_or_else(no_project)?;

        // Find the catalog in the project to determine the locale id.
        let catalog_ref = project
            .catalog(&abs)
            .ok_or_else(|| "catalog not open in project".to_string())?;
        let locale_id = &catalog_ref.locale;

        // Three-layer locale merge; fall back to workspace-only if the
        // project doesn't declare this locale id (defensive, not the common path).
        let locale = project
            .locale(locale_id)
            .map(|r| r.workspace_locale())
            .or_else(|| Locale::by_id(locale_id))
            .ok_or_else(|| format!("unknown locale `{locale_id}`; add it to crates/locales"))?;

        // Validate the backend config: if the project declares a non-Ollama
        // backend, we refuse rather than silently falling back.
        if let Some(backend_cfg) = &project.manifest().backends.default {
            if backend_cfg.kind != BackendKind::Ollama {
                return Err(format!(
                    "backend kind {:?} not supported yet",
                    backend_cfg.kind,
                ));
            }
        }

        let glossary = project.glossary().cloned();
        (locale, glossary)
    };

    let mut store = state
        .project_catalogs
        .lock()
        .map_err(project_catalogs_lock_poisoned)?;
    let entry = store.get_mut(&abs).ok_or_else(no_catalog_in_project)?;

    let id = UnitId::from(unit_id);
    let original = entry
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
            // Surface as an inline hard gate finding rather than an Err
            // so the Inspector renders it next to the unit and the user
            // can investigate the prompt. We leave the unit unchanged
            // (no target write, no flag merge, no review-status update)
            // and the catalog stays clean for this slot.
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
    // immediately rather than only after the next review-map fold. The
    // durable `review.jsonl` append happens after we drop the
    // project_catalogs lock, to keep the lock-ordering convention
    // (project before project_catalogs is the convention; we already
    // released project before acquiring project_catalogs and the durable
    // write re-takes project after dropping project_catalogs).
    let needs_review = !merged.flags.is_empty();
    if needs_review {
        merged.review_status = Some(ReviewStatus::NeedsReview);
    }

    if let Some(slot) = entry.catalog.find_unit_mut(&id) {
        *slot = merged.clone();
    }
    entry.dirty = true;

    if needs_review {
        let source_hash = merged.source_hash.clone().unwrap_or_default();
        drop(store);
        let project_guard = state.project.lock().map_err(project_lock_poisoned)?;
        if let Some(project) = project_guard.as_ref() {
            // A failure here would leave the in-memory unit reporting
            // NeedsReview while review.jsonl is unaware of the change.
            // We surface the error to the caller; the next translate will
            // recompute and re-attempt. Empty-flag units never reach this
            // branch (see `needs_review` above), so the No-op-Save-All
            // path is unchanged.
            project
                .set_review_status(
                    &abs,
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

    let project_guard = state.project.lock().map_err(project_lock_poisoned)?;
    let project = project_guard.as_ref().ok_or_else(no_project)?;

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

    let project_guard = state.project.lock().map_err(project_lock_poisoned)?;
    let project = project_guard.as_ref().ok_or_else(no_project)?;

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
    let mut project_guard = state.project.lock().map_err(project_lock_poisoned)?;
    let project = project_guard.as_mut().ok_or_else(no_project)?;
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
    let mut project_guard = state.project.lock().map_err(project_lock_poisoned)?;
    let project = project_guard.as_mut().ok_or_else(no_project)?;
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
    let project_guard = state.project.lock().map_err(project_lock_poisoned)?;
    let project = project_guard.as_ref().ok_or_else(no_project)?;
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
        let project_guard = state.project.lock().map_err(project_lock_poisoned)?;
        let project = project_guard.as_ref().ok_or_else(no_project)?;
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
        let project_guard = state.project.lock().map_err(project_lock_poisoned)?;
        let project = project_guard.as_ref().ok_or_else(no_project)?;
        project.catalogs().to_vec()
    };

    // For each catalog, ensure it is in the project_catalogs store.
    // If it is already open, skip the I/O; otherwise extract + apply and insert.
    for catalog_ref in &catalog_refs {
        // Only qt-ts is supported today.
        if catalog_ref.format != i18n_harness_project::CatalogFormat::QtTs {
            tracing::warn!(
                path = %catalog_ref.manifest_path,
                format = ?catalog_ref.format,
                "scan_project_review_state: non-qt-ts catalog skipped (M4.4/M4.5 will wire PO/ICU-JSON)"
            );
            continue;
        }

        let abs = std::path::PathBuf::from(&catalog_ref.absolute_path);

        // Check whether already cached.
        let already_open = {
            let store = state
                .project_catalogs
                .lock()
                .map_err(project_catalogs_lock_poisoned)?;
            store.contains_key(&abs)
        };

        if !already_open {
            // Extract from disk.
            let mut catalog = i18n_harness_adapter_qt::extract(&abs)
                .map_err(|e| format!("extract failed for {}: {e}", catalog_ref.manifest_path))?;

            // Fold review state in.
            {
                let project_guard = state.project.lock().map_err(project_lock_poisoned)?;
                if let Some(project) = project_guard.as_ref() {
                    project.apply_review_state(&abs, catalog.units_mut());
                }
            }

            // Insert into the store.
            state
                .project_catalogs
                .lock()
                .map_err(project_catalogs_lock_poisoned)?
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
        .map_err(project_catalogs_lock_poisoned)?;

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
            .map_err(project_catalogs_lock_poisoned)?;
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
        let project_guard = state.project.lock().map_err(project_lock_poisoned)?;
        let project = project_guard.as_ref().ok_or_else(no_project)?;
        project
            .set_review_status(&abs, &uid, Some(ReviewStatus::Reviewed), source_hash, None)
            .map_err(|e| format!("set_review_status failed: {e}"))?;
    }

    // Durable write succeeded — now mutate the in-memory unit and return it.
    let mut store = state
        .project_catalogs
        .lock()
        .map_err(project_catalogs_lock_poisoned)?;
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
    let mut guard = state.project.lock().map_err(project_lock_poisoned)?;
    let project = guard.as_mut().ok_or_else(no_project)?;
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
        let mut guard = state.project.lock().map_err(project_lock_poisoned)?;
        let project = guard.as_mut().ok_or_else(no_project)?;

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
    let mut guard = state.project.lock().map_err(project_lock_poisoned)?;
    let project = guard.as_mut().ok_or_else(no_project)?;
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
    let mut guard = state.project.lock().map_err(project_lock_poisoned)?;
    let project = guard.as_mut().ok_or_else(no_project)?;
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
    let mut guard = state.project.lock().map_err(project_lock_poisoned)?;
    let project = guard.as_mut().ok_or_else(no_project)?;
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
    let mut guard = state.project.lock().map_err(project_lock_poisoned)?;
    let project = guard.as_mut().ok_or_else(no_project)?;
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
    let mut guard = state.project.lock().map_err(project_lock_poisoned)?;
    let project = guard.as_mut().ok_or_else(no_project)?;
    project.set_prompts(config).map_err(|e| e.to_string())?;
    project.save_manifest().map_err(|e| e.to_string())?;
    let summary = project.summary();
    Ok(ProjectOpenResponse {
        summary,
        warnings: vec![],
    })
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

fn lock_poisoned(
    _: std::sync::PoisonError<std::sync::MutexGuard<'_, Option<OpenCatalog>>>,
) -> String {
    "catalog state lock poisoned".to_string()
}

fn glossary_lock_poisoned(
    _: std::sync::PoisonError<std::sync::MutexGuard<'_, Option<Glossary>>>,
) -> String {
    "glossary state lock poisoned".to_string()
}

fn project_lock_poisoned(
    _: std::sync::PoisonError<std::sync::MutexGuard<'_, Option<Project>>>,
) -> String {
    "project state lock poisoned".to_string()
}

fn project_catalogs_lock_poisoned(
    _: std::sync::PoisonError<
        std::sync::MutexGuard<'_, std::collections::BTreeMap<PathBuf, OpenCatalogEntry>>,
    >,
) -> String {
    "project_catalogs state lock poisoned".to_string()
}

fn no_catalog() -> String {
    "no catalog open".to_string()
}

fn no_project() -> String {
    "no project open".to_string()
}

fn no_catalog_in_project() -> String {
    "catalog not open in project".to_string()
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
    ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
