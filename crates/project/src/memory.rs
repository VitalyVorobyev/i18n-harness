//! Translation-memory types: correction store, curated set, and all supporting
//! shapes.
//!
//! Two on-disk files cooperate here:
//!
//! - `<state_dir>/corrections.jsonl` — append-only; one [`Correction`] per
//!   line. Written by [`CorrectionStore::append`]; read by
//!   [`CorrectionStore::read_all`] and filtered by
//!   [`CorrectionStore::read_filtered`].
//! - `<state_dir>/curated.toml` — small, round-trip-preserved (via
//!   `toml_edit`); references corrections by [`CorrectionId`] with optional
//!   human notes. Written by `Project::promote_to_curated` /
//!   `Project::un_curate`.
//!
//! See `docs/m4.1-project-crate-design.md` §1.8, §6.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use i18n_harness_core::{Flag, UnitId};

use crate::error::ProjectError;
use crate::fs::ProjectFs;

// ── CorrectionId ──────────────────────────────────────────────────────────────

/// Stable, content-addressed id for one correction record.
///
/// Format: `"corr_<12-hex>"` where the hex is the first 12 chars of a
/// SHA-256 over `(catalog_manifest_path, unit_id, source, mt_proposal,
/// human_target, ts_micros)`. Including `ts_micros` makes the id unique
/// even when the same unit is re-edited immediately after acceptance.
///
/// Provenance fields are **not** included in the hash: two corrections that
/// differ only in which prompt version produced their proposal are the same
/// edit from the human's perspective and hash the same. See design §6.1.
///
/// Stored as a plain string so it round-trips through JSON and TOML without
/// precision loss or quoting issues.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CorrectionId(
    /// The `"corr_<12-hex>"` string.
    pub String,
);

impl CorrectionId {
    /// Construct a `CorrectionId` from the content fields that uniquely identify
    /// an edit. Provenance is deliberately excluded — see design §6.1.
    ///
    /// Returns `"corr_<first-12-hex-chars-of-SHA-256>"`. Fields are separated
    /// by `\0` to avoid ambiguity between `("a", "bc")` and `("ab", "c")`.
    pub(crate) fn from_content_hash(
        catalog: &Path,
        unit_id: &UnitId,
        source: &str,
        mt_proposal: &str,
        human_target: &str,
        ts_micros: i64,
    ) -> Self {
        content_hash(
            catalog,
            unit_id,
            source,
            mt_proposal,
            human_target,
            ts_micros,
        )
    }
}

/// Internal implementation so the hashing logic is not duplicated.
fn content_hash(
    catalog: &Path,
    unit_id: &UnitId,
    source: &str,
    mt_proposal: &str,
    human_target: &str,
    ts_micros: i64,
) -> CorrectionId {
    let mut hasher = Sha256::new();
    hasher.update(catalog.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(unit_id.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(source.as_bytes());
    hasher.update(b"\0");
    hasher.update(mt_proposal.as_bytes());
    hasher.update(b"\0");
    hasher.update(human_target.as_bytes());
    hasher.update(b"\0");
    hasher.update(ts_micros.to_le_bytes());
    let digest = hasher.finalize();
    // Hex-encode and take first 12 chars (6 bytes, 48 bits).
    let hex = format!("{digest:x}");
    CorrectionId(format!("corr_{}", &hex[..12]))
}

impl std::fmt::Display for CorrectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// ── CorrectionProvenance ──────────────────────────────────────────────────────

/// Records the exact backend, model, and prompt context that produced the
/// `mt_proposal` for a [`Correction`].
///
/// All fields are `String` so the JSONL line is forward-compatible — versions
/// are caller-chosen identifiers, not parsed integers. Fields default to empty
/// so manual (non-MT) corrections serialize compactly; the writer populates
/// them when an MT result triggered the edit.
///
/// This matters because the M4.10 tuning loop compares prompt versions:
/// without this, you cannot tell whether prompt v7 + glossary v3 was better or
/// worse than prompt v8 + glossary v3. Historical corrections cannot be
/// backfilled with provenance later, so the schema includes these fields from
/// day one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrectionProvenance {
    /// Backend name as registered by the `TranslationBackend` trait,
    /// e.g. `"ollama"`, `"openai-compatible"`, `"manual"`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub backend: String,
    /// Model identifier as the backend reports it, e.g. `"gemma4:e2b"`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// Free-form model version / revision string when the backend can report
    /// one. Empty when not available.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model_version: String,
    /// Prompt template version identifier, e.g. `"ollama-translate-v2"`. The
    /// Tauri layer or CLI is responsible for passing the active template's
    /// version into the writer.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt_template_version: String,
    /// Hash of the glossary content at correction time, so retroactive glossary
    /// changes don't silently invalidate stored corrections. Format:
    /// `"sha256:<first-12-hex-chars>"`. Empty when no glossary was active.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub glossary_version: String,
}

// ── Correction ────────────────────────────────────────────────────────────────

/// One accepted human edit, stored as a single line in `corrections.jsonl`.
///
/// `schema` is bumped on each breaking change to the line shape. `id` is
/// content-addressed (SHA-256 of the content fields; see design §6.1). `ts` is
/// an RFC 3339 microsecond UTC timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Correction {
    /// Schema version for this line shape. Currently `1`.
    pub schema: u32,
    /// Content-addressed id for this correction. See [`CorrectionId`].
    pub id: CorrectionId,
    /// RFC 3339 microsecond UTC timestamp of when this correction was recorded.
    pub ts: String,
    /// Manifest-relative path to the catalog (so corrections survive a project
    /// move or root rename).
    pub catalog: PathBuf,
    /// Target locale id.
    pub locale: String,
    /// Unit within the catalog.
    pub unit_id: UnitId,
    /// Source text that was being translated.
    pub source: String,
    /// What the model proposed (empty string for a human-from-scratch entry).
    pub mt_proposal: String,
    /// The accepted human translation.
    pub human_target: String,
    /// Provenance of `mt_proposal`. Defaults to all-empty for manual entries,
    /// which serializes compactly without any provenance keys.
    #[serde(default)]
    pub provenance: CorrectionProvenance,
    /// Flags the unit had at correction time. Useful for downstream analysis
    /// (e.g. "which prompts produce the most edits when `length-warn` fires").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags_at_correction: Vec<Flag>,
}

// ── NewCorrection ─────────────────────────────────────────────────────────────

/// Input shape for [`crate::project::Project::record_correction`].
///
/// The `id`, `ts`, and `schema` fields are generated by the store; callers
/// cannot accidentally produce two records with the same id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCorrection {
    /// Manifest-relative path to the catalog.
    pub catalog: PathBuf,
    /// Target locale id.
    pub locale: String,
    /// Unit being corrected.
    pub unit_id: UnitId,
    /// Source text.
    pub source: String,
    /// MT proposal (empty for manual-from-scratch).
    pub mt_proposal: String,
    /// Accepted human translation.
    pub human_target: String,
    /// Provenance of `mt_proposal`.
    pub provenance: CorrectionProvenance,
    /// Flags the unit had at correction time.
    pub flags_at_correction: Vec<Flag>,
}

// ── CorrectionFilter ──────────────────────────────────────────────────────────

/// Filter passed to [`crate::project::Project::list_corrections`].
///
/// An empty filter (all fields `None`) returns every record. Fields narrow the
/// result independently (all active filters must match — AND semantics).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CorrectionFilter {
    /// Restrict to corrections for this manifest-relative catalog path.
    pub catalog: Option<PathBuf>,
    /// Restrict to corrections for this locale id.
    pub locale: Option<String>,
    /// Restrict to corrections for this unit id.
    pub unit_id: Option<UnitId>,
    /// Restrict to corrections at or after this RFC 3339 timestamp (inclusive,
    /// lexicographic comparison — valid because RFC 3339 sorts correctly when
    /// the precision is uniform, which it is here: all timestamps are RFC 3339
    /// with microsecond precision).
    pub since: Option<String>,
}

// ── CorrectionStore ───────────────────────────────────────────────────────────

/// Append-only correction store backed by `<state_dir>/corrections.jsonl`.
///
/// One line per accepted human edit. Cheap to construct — I/O happens only on
/// [`append`](Self::append) and [`read_all`](Self::read_all).
pub struct CorrectionStore {
    path: PathBuf,
    fs: Arc<dyn ProjectFs>,
}

impl CorrectionStore {
    /// Construct a store pointing at `path`. `fs` provides the I/O backing.
    ///
    /// The file need not exist at construction time; it is created lazily on
    /// the first append.
    pub(crate) fn new(path: PathBuf, fs: Arc<dyn ProjectFs>) -> Self {
        Self { path, fs }
    }

    /// Append a single correction record to the JSONL file.
    ///
    /// Each call serializes `correction` to a single-line JSON string
    /// (no embedded newlines — `serde_json` encodes them as `\n`) and appends
    /// it followed by a newline byte.
    ///
    /// # Errors
    ///
    /// - [`ProjectError::Io`] if the filesystem append fails.
    pub fn append(&self, correction: &Correction) -> Result<(), ProjectError> {
        let line =
            serde_json::to_string(correction).expect("Correction serialization is infallible");
        debug_assert!(
            !line.contains('\n'),
            "Correction JSON must not contain literal newlines"
        );
        let mut bytes = line.into_bytes();
        bytes.push(b'\n');
        self.fs
            .append(&self.path, &bytes)
            .map_err(|source| ProjectError::Io {
                path: self.path.clone(),
                source,
            })
    }

    /// Read all correction records from the JSONL file.
    ///
    /// Line-recoverable: malformed lines are returned as
    /// [`ProjectError::CorrectionParse`] warnings in the second element of the
    /// return tuple; well-formed lines accumulate in the first element.
    ///
    /// Returns an empty `Vec` (and no warnings) if the file does not exist yet.
    pub fn read_all(&self) -> Result<(Vec<Correction>, Vec<ProjectError>), ProjectError> {
        if !self.fs.exists(&self.path) {
            return Ok((Vec::new(), Vec::new()));
        }

        let text = self
            .fs
            .read_to_string(&self.path)
            .map_err(|source| ProjectError::Io {
                path: self.path.clone(),
                source,
            })?;

        let mut corrections = Vec::new();
        let mut parse_errors = Vec::new();

        for (idx, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<Correction>(line) {
                Ok(c) => corrections.push(c),
                Err(source) => parse_errors.push(ProjectError::CorrectionParse {
                    path: self.path.clone(),
                    line_no: idx + 1,
                    source,
                }),
            }
        }

        Ok((corrections, parse_errors))
    }

    /// Read all corrections that match `filter`.
    ///
    /// Applies filtering on top of [`read_all`](Self::read_all). Parse errors
    /// from bad lines are included in the second tuple element.
    pub fn read_filtered(
        &self,
        filter: &CorrectionFilter,
    ) -> Result<(Vec<Correction>, Vec<ProjectError>), ProjectError> {
        let (all, errors) = self.read_all()?;
        let filtered = all
            .into_iter()
            .filter(|c| filter_matches(c, filter))
            .collect();
        Ok((filtered, errors))
    }
}

/// Returns `true` if `correction` matches every active field of `filter`.
fn filter_matches(correction: &Correction, filter: &CorrectionFilter) -> bool {
    if let Some(ref cat) = filter.catalog {
        if &correction.catalog != cat {
            return false;
        }
    }
    if let Some(ref loc) = filter.locale {
        if &correction.locale != loc {
            return false;
        }
    }
    if let Some(ref uid) = filter.unit_id {
        if &correction.unit_id != uid {
            return false;
        }
    }
    if let Some(ref since) = filter.since {
        // RFC 3339 with microsecond precision sorts lexicographically.
        if correction.ts.as_str() < since.as_str() {
            return false;
        }
    }
    true
}

// ── CuratedSet ────────────────────────────────────────────────────────────────

/// Read view of `<state_dir>/curated.toml`.
///
/// Curated examples reference corrections by [`CorrectionId`]; the actual
/// text lives in `corrections.jsonl` and is resolved lazily. See design §6.2.
///
/// Loaded and kept in memory by `Project`; reloaded on every
/// `promote_to_curated` / `un_curate`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CuratedSet {
    /// Examples in the order they appear in `curated.toml`.
    examples: Vec<CuratedExample>,
}

impl CuratedSet {
    /// Iterate all curated examples.
    pub fn examples(&self) -> impl Iterator<Item = &CuratedExample> {
        self.examples.iter()
    }

    /// Returns `true` if `id` is in the curated set.
    pub fn contains(&self, id: &CorrectionId) -> bool {
        self.examples.iter().any(|e| &e.id == id)
    }

    /// Return the note for `id`, if any.
    ///
    /// Returns `None` if the id is not in the set, or if it has no note.
    pub fn note(&self, id: &CorrectionId) -> Option<&str> {
        self.examples
            .iter()
            .find(|e| &e.id == id)
            .and_then(|e| e.note.as_deref())
    }

    /// Number of curated examples.
    pub fn len(&self) -> usize {
        self.examples.len()
    }

    /// Returns `true` if the curated set is empty.
    pub fn is_empty(&self) -> bool {
        self.examples.is_empty()
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    /// Parse from a `curated.toml` string.
    ///
    /// The TOML structure is:
    ///
    /// ```toml
    /// schema = 1
    ///
    /// [[example]]
    /// id = "corr_a1b2c3d4e5f6"
    /// note = "Optional human note."
    /// ```
    pub(crate) fn from_toml_str(s: &str) -> Result<Self, ProjectError> {
        // Parse through serde for the typed view.
        let raw: CuratedToml =
            toml::from_str(s).map_err(|source| ProjectError::CuratedParse { source })?;
        let examples = raw
            .example
            .into_iter()
            .map(|e| CuratedExample {
                id: CorrectionId(e.id),
                note: if e.note.as_deref().unwrap_or("").is_empty() {
                    None
                } else {
                    e.note
                },
                correction: None, // resolved later by the caller
            })
            .collect();
        Ok(Self { examples })
    }

    /// Add a new example (used by `promote_to_curated`).
    pub(crate) fn push(&mut self, example: CuratedExample) {
        self.examples.push(example);
    }

    /// Remove the example with `id`. Returns `true` if it was present.
    pub(crate) fn remove(&mut self, id: &CorrectionId) -> bool {
        let before = self.examples.len();
        self.examples.retain(|e| &e.id != id);
        self.examples.len() < before
    }

    /// Resolve the `correction` field of each `CuratedExample` against the
    /// provided correction list.
    pub(crate) fn resolve_corrections(&mut self, corrections: &[Correction]) {
        for ex in &mut self.examples {
            ex.correction = corrections.iter().find(|c| c.id == ex.id).cloned();
        }
    }

    /// Serialise to a `curated.toml` string using `toml_edit` for round-trip
    /// preservation.
    ///
    /// The caller supplies the *existing* document text (to preserve user
    /// comments) plus the current in-memory set. The strategy:
    ///
    /// 1. Parse the existing document (or start from an empty document).
    /// 2. Rebuild `[[example]]` entirely from `self.examples` (the array is
    ///    small; preserving order matters more than preserving per-entry
    ///    comments).
    /// 3. Return the updated document as a string.
    pub(crate) fn to_toml_string(&self, existing_doc: Option<&str>) -> String {
        use std::str::FromStr;
        use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

        let mut doc: DocumentMut = existing_doc
            .and_then(|s| DocumentMut::from_str(s).ok())
            .unwrap_or_default();

        // Ensure schema is present.
        doc.entry("schema")
            .or_insert_with(|| Item::Value(Value::Integer(toml_edit::Formatted::new(1))));

        // Rebuild [[example]] from scratch.
        let mut aot = ArrayOfTables::new();
        for ex in &self.examples {
            let mut t = Table::new();
            t.insert(
                "id",
                Item::Value(Value::String(toml_edit::Formatted::new(ex.id.0.clone()))),
            );
            if let Some(note) = &ex.note {
                t.insert(
                    "note",
                    Item::Value(Value::String(toml_edit::Formatted::new(note.clone()))),
                );
            }
            aot.push(t);
        }
        doc.insert("example", Item::ArrayOfTables(aot));

        doc.to_string()
    }
}

// ── CuratedExample ────────────────────────────────────────────────────────────

/// One entry in the curated set.
///
/// `correction` is `None` when the underlying `corrections.jsonl` entry has
/// been deleted (file rotation, manual edit, project copy without state). The
/// dangling reference is preserved in `curated.toml` so the user can see it
/// and decide whether to remove it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuratedExample {
    /// The correction's content-addressed id.
    pub id: CorrectionId,
    /// Optional human teaching note added when the correction was promoted.
    pub note: Option<String>,
    /// Resolved correction data. `None` for dangling references.
    pub correction: Option<Correction>,
}

// ── TOML serde helpers ────────────────────────────────────────────────────────

/// Raw serde shape for `curated.toml`. Used only for parsing; writing goes
/// through `toml_edit`.
#[derive(Debug, serde::Deserialize)]
struct CuratedToml {
    #[serde(default)]
    pub example: Vec<CuratedExampleRaw>,
}

#[derive(Debug, serde::Deserialize)]
struct CuratedExampleRaw {
    pub id: String,
    #[serde(default)]
    pub note: Option<String>,
}
