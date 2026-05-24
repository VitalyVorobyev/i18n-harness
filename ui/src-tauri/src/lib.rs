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
use i18n_harness_project::{CatalogRef, DraftManifest, Project, ProjectSummary};
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
    if !unit.state.is_writable() {
        return Err(format!(
            "unit {id} is {state:?} — vanished/obsolete units are not writable",
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
    //   Untranslated → Proposed   once any text is set
    //   Proposed     → Untranslated  once the target is fully cleared
    // The model-translate path and any explicit Finished promotion live
    // on top of these; gate-clean translations override to Finished.
    match unit.state {
        UnitState::Untranslated if !unit.target.is_empty() => {
            unit.state = UnitState::Proposed;
        }
        UnitState::Proposed | UnitState::Finished if unit.target.is_empty() => {
            unit.state = UnitState::Untranslated;
            unit.flags = Default::default();
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
    use i18n_harness_backend::{OllamaBackend, TranslationBackend, TranslationOutcome};
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
        TranslationOutcome::Translated { text, flags } => {
            merged.target = match text {
                i18n_harness_backend::TranslatedText::Singular(s) => {
                    Target::Singular { text: Some(s) }
                }
                i18n_harness_backend::TranslatedText::Plural(forms) => Target::Plural {
                    forms: forms.into_iter().map(Some).collect(),
                },
            };
            merged.state = UnitState::Proposed;
            let mut flagset = FlagSet::new();
            for f in flags {
                flagset.insert(f);
            }
            merged.flags = flagset;
        }
        TranslationOutcome::Skipped { reason } => {
            return Err(format!("backend skipped: {reason}"));
        }
        TranslationOutcome::Failed { reason, .. } => {
            return Err(format!("backend failed: {reason}"));
        }
    }

    let report = i18n_harness_gate::validate(&merged, locale, None);
    if report.is_clean() && merged.target.is_complete() {
        merged.state = UnitState::Finished;
    }

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

    if !unit.state.is_writable() {
        return Err(format!(
            "unit {id} is {state:?} — vanished/obsolete units are not writable",
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

/// Write every dirty catalog in the project store back to disk.
///
/// Catalogs are written in BTreeMap iteration order (absolute path order),
/// which is deterministic. On the first apply failure the function stops and
/// returns the partial list of successes plus an `Err` describing the failing
/// path. Successfully-written entries have their `dirty` flag cleared before
/// the failure is surfaced.
#[tauri::command]
fn save_all_dirty(state: tauri::State<'_, AppState>) -> Result<Vec<SaveSummary>, String> {
    let mut store = state
        .project_catalogs
        .lock()
        .map_err(project_catalogs_lock_poisoned)?;
    let mut summaries: Vec<SaveSummary> = Vec::new();
    for (abs, entry) in store.iter_mut() {
        if !entry.dirty {
            continue;
        }
        let units = entry.catalog.units().to_vec();
        i18n_harness_adapter_qt::apply(&entry.catalog, &units, abs)
            .map_err(|e| format!("apply failed for {}: {e}", abs.display()))?;
        entry.dirty = false;
        summaries.push(SaveSummary {
            path: abs.to_string_lossy().into_owned(),
            unit_count: units.len(),
        });
    }
    Ok(summaries)
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
    ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
