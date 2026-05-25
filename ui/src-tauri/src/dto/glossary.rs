//! Wire-format types for the glossary editor commands.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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
