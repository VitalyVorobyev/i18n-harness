//! On-disk handoff format for the two-phase agent translation flow.
//!
//! `export-batch` calls [`write_export`] to produce a folder an external
//! agent (Claude Code, Copilot, Codex) fills in. `import-batch` calls
//! [`read_targets`] to consume the agent's output and feed it into
//! [`crate::ManualBackend`].
//!
//! # Folder layout
//!
//! ```text
//! <batch-dir>/
//! ├── README.md      — instructions for the human / agent
//! ├── prompt.md      — system prompt rendered by export-batch
//! ├── units.jsonl    — source units (one [`ExportedUnit`] per line)
//! ├── targets.jsonl  — filled in by the agent; consumed by import-batch
//! └── meta.json      — audit record; not consumed by import-batch
//! ```
//!
//! # Format stability
//!
//! `units.jsonl` and `targets.jsonl` are the stable on-disk contract
//! the CLI subcommands and external agents consume. Field names are
//! snake_case; the `kind` tag in `targets.jsonl` mirrors
//! [`crate::ManualResponse`]. The `meta.json` `format_version` field
//! gates schema bumps.

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use i18n_harness_core::{Batch, FlagSet, Placeholder};
use i18n_harness_glossary::Glossary;
use i18n_harness_locales::Locale;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ManualResponse;

// ── ExportedUnit ────────────────────────────────────────────────────────────

/// A curated view of one [`i18n_harness_core::Unit`] for external agents.
///
/// Contains only the fields an agent needs to produce a translation:
/// the stable id, the ICU-normalised source text, plural arity, placeholder
/// metadata, and any flags already attached. Internal fields (`target`,
/// `provenance`, `state`, `source_hash`, `review_status`, `confidence`,
/// `source_changed_since_review`) are intentionally absent — agents must
/// not touch structure, only text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportedUnit {
    /// Stable identifier within the catalog. Opaque to the agent; must
    /// be echoed verbatim in `targets.jsonl`.
    pub id: String,
    /// Source text in ICU MessageFormat. The agent translates this string.
    pub source: String,
    /// CLDR plural-category arity for plural units; `None` for singular.
    /// When `Some(n)`, the `targets.jsonl` entry must supply `n` forms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plural_arity: Option<u32>,
    /// Placeholder tokens occurring in the source. Agents must preserve
    /// every token verbatim in the translation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub placeholders: Vec<Placeholder>,
    /// Flags already attached to this unit (e.g. from a prior gate run).
    /// Informational only; agents should not act on them beyond noting
    /// ambiguous or low-confidence contexts.
    #[serde(default)]
    pub flags_so_far: FlagSet,
}

// ── TargetLine (internal wire shape for targets.jsonl) ─────────────────────

/// Wire representation of one line in `targets.jsonl`.
///
/// The `kind` tag mirrors [`ManualResponse`]:
/// - `"singular"` → `ManualResponse::Singular`
/// - `"plural"`   → `ManualResponse::Plural`
/// - `"skip"`     → `ManualResponse::Skip`
/// - `"fail"`     → `ManualResponse::Fail`
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TargetLine {
    Singular {
        id: String,
        text: String,
    },
    Plural {
        id: String,
        texts: Vec<String>,
    },
    Skip {
        id: String,
    },
    Fail {
        id: String,
        reason: String,
        retryable: bool,
    },
}

impl TargetLine {
    fn id(&self) -> &str {
        match self {
            Self::Singular { id, .. }
            | Self::Plural { id, .. }
            | Self::Skip { id }
            | Self::Fail { id, .. } => id,
        }
    }

    fn into_response(self) -> ManualResponse {
        match self {
            Self::Singular { text, .. } => ManualResponse::Singular(text),
            Self::Plural { texts, .. } => ManualResponse::Plural(texts),
            Self::Skip { .. } => ManualResponse::Skip,
            Self::Fail {
                reason, retryable, ..
            } => ManualResponse::Fail { reason, retryable },
        }
    }
}

// ── AgentBatchError ─────────────────────────────────────────────────────────

/// Errors produced by [`write_export`] and [`read_targets`].
#[derive(Debug, Error)]
pub enum AgentBatchError {
    /// Filesystem operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// A line in `targets.jsonl` is not valid JSON (or not a JSON object).
    #[error("JSON parse error on line {line}: {source}")]
    Json {
        /// 1-based line number in `targets.jsonl`.
        line: usize,
        /// Underlying JSON parse error.
        #[source]
        source: serde_json::Error,
    },

    /// `targets.jsonl` has fewer lines than `units.jsonl`.
    #[error("batch incomplete: expected {expected} responses, got {got}")]
    IncompleteBatch {
        /// Number of lines in `units.jsonl`.
        expected: usize,
        /// Number of valid lines parsed from `targets.jsonl`.
        got: usize,
    },

    /// The id on line `line` of `targets.jsonl` is present in `units.jsonl`
    /// but at a different position. Agents must not reorder lines.
    #[error("id order mismatch on line {line}: expected id `{expected}`, got `{got}`")]
    OrderMismatch {
        /// 1-based line number in `targets.jsonl`.
        line: usize,
        /// Id expected at this position (from `units.jsonl`).
        expected: String,
        /// Id actually present on this line.
        got: String,
    },

    /// The id on line `line` of `targets.jsonl` does not appear anywhere in
    /// `units.jsonl`.
    #[error("unknown unit id on line {line}: `{id}`")]
    MismatchedIds {
        /// 1-based line number in `targets.jsonl`.
        line: usize,
        /// The unknown id found on this line.
        id: String,
    },

    /// A `targets.jsonl` line has a `kind` value that is not one of
    /// `singular`, `plural`, `skip`, or `fail`.
    #[error("unknown target kind `{kind}` on line {line}")]
    UnknownTargetKind {
        /// 1-based line number in `targets.jsonl`.
        line: usize,
        /// The unrecognised `kind` string.
        kind: String,
    },

    /// A `targets.jsonl` line is structurally valid JSON but does not conform
    /// to the expected shape (e.g. missing `text` for a `singular` entry).
    #[error("malformed target on line {line}: {msg}")]
    MalformedTarget {
        /// 1-based line number in `targets.jsonl`.
        line: usize,
        /// What is wrong with this line.
        msg: String,
    },
}

// ── MetaJson (internal, audit only) ─────────────────────────────────────────

#[derive(Serialize)]
struct MetaJson<'a> {
    format_version: &'static str,
    locale_id: &'a str,
    glossary_path: Option<&'a str>,
    source_catalog_path: &'a str,
    started_at_unix_secs: u64,
    unit_count: usize,
}

// ── write_export ────────────────────────────────────────────────────────────

/// Write an export folder for the two-phase agent flow.
///
/// Each file is written to a `.tmp` sibling first and then renamed into
/// place, so a partially-written export is never silently consumed by
/// `import-batch`.
///
/// # Parameters
///
/// - `dir` — destination directory. Created if it does not exist.
/// - `batch` — the units to export.
/// - `locale` — target locale (used in README and meta.json).
/// - `glossary` — optional glossary. Its path is recorded in meta.json
///   for audit; the glossary content is NOT inlined into the folder
///   (the prompt passed by the caller already contains any glossary
///   context the agent needs).
/// - `glossary_path` — filesystem path of the glossary, if any. Used
///   only in meta.json.
/// - `source_catalog_path` — filesystem path of the catalog the batch
///   was extracted from. Used only in meta.json.
/// - `prompt` — rendered system prompt; written to `prompt.md` verbatim.
pub fn write_export(
    dir: &Path,
    batch: &Batch,
    locale: &Locale,
    _glossary: Option<&Glossary>,
    glossary_path: Option<&Path>,
    source_catalog_path: &Path,
    prompt: &str,
) -> Result<(), AgentBatchError> {
    fs::create_dir_all(dir)?;

    let unit_count = batch.units.len();
    let locale_id = locale.id;
    let gloss_path_str = glossary_path.map(|p| p.to_string_lossy().into_owned());
    let catalog_path_str = source_catalog_path.to_string_lossy().into_owned();
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // units.jsonl
    let exported: Vec<ExportedUnit> = batch
        .units
        .iter()
        .map(|u| ExportedUnit {
            id: u.id.as_str().to_owned(),
            source: u.source.clone(),
            plural_arity: u.plural_arity,
            placeholders: u.placeholders.clone(),
            flags_so_far: u.flags.clone(),
        })
        .collect();
    write_atomic(dir, "units.jsonl", |w| {
        for eu in &exported {
            serde_json::to_writer(&mut *w, eu).map_err(|e| io::Error::other(e.to_string()))?;
            w.write_all(b"\n")?;
        }
        Ok(())
    })?;

    // prompt.md
    write_atomic(dir, "prompt.md", |w| w.write_all(prompt.as_bytes()))?;

    // README.md
    let readme = render_readme(locale_id, unit_count);
    write_atomic(dir, "README.md", |w| w.write_all(readme.as_bytes()))?;

    // meta.json
    let meta = MetaJson {
        format_version: "1",
        locale_id,
        glossary_path: gloss_path_str.as_deref(),
        source_catalog_path: &catalog_path_str,
        started_at_unix_secs: started_at,
        unit_count,
    };
    write_atomic(dir, "meta.json", |w| {
        serde_json::to_writer_pretty(&mut *w, &meta)
            .map_err(|e| io::Error::other(e.to_string()))?;
        w.write_all(b"\n")
    })?;

    Ok(())
}

fn write_atomic<F>(dir: &Path, name: &str, fill: F) -> io::Result<()>
where
    F: FnOnce(&mut fs::File) -> io::Result<()>,
{
    let final_path = dir.join(name);
    let tmp_path = dir.join(format!("{name}.tmp"));
    {
        let mut f = fs::File::create(&tmp_path)?;
        fill(&mut f)?;
        f.sync_all()?;
    }
    fs::rename(&tmp_path, &final_path)
}

fn render_readme(locale_id: &str, unit_count: usize) -> String {
    format!(
        r#"# Translation batch — {locale_id}

## What is this folder?

This folder contains a translation batch produced by `harness export-batch`.
It contains {unit_count} unit(s) to translate into **{locale_id}**.

## Files

| File | Purpose |
|---|---|
| `units.jsonl` | Source units — one JSON object per line. Read-only. |
| `prompt.md` | System prompt — read this before translating. |
| `targets.jsonl` | **Your output file.** Write one JSON line per unit. |
| `meta.json` | Audit record. Ignore during translation. |

## How to produce `targets.jsonl`

Write exactly **{unit_count} line(s)** to `targets.jsonl` — one per unit,
in the **same order** as `units.jsonl`. Each line must be a valid JSON object.

### Singular unit

```json
{{"id": "<unit-id>", "kind": "singular", "text": "<translated text>"}}
```

### Plural unit

```json
{{"id": "<unit-id>", "kind": "plural", "texts": ["<form-1>", "<form-2>"]}}
```

The `texts` array must have exactly as many entries as the unit's
`plural_arity`. Forms appear in canonical CLDR order for the target locale
(e.g. for German: `[one, other]`).

### Skip a unit

Use `skip` when the unit should not be translated (e.g. it is already
correct, or it is format-only). The unit is left untranslated; this does
not count as a failure.

```json
{{"id": "<unit-id>", "kind": "skip"}}
```

### Fail a unit

Use `fail` when translation is impossible (e.g. the source is ambiguous
beyond recovery, or the agent is certain it cannot produce a correct result).

```json
{{"id": "<unit-id>", "kind": "fail", "reason": "<kebab-case-reason>", "retryable": false}}
```

Set `"retryable": true` only when a retry with the same prompt might
succeed (e.g. a transient model error). For permanent failures, use `false`.

## Placeholder rules

- Placeholders such as `{{0}}`, `{{name}}`, `{{count}}` must appear in
  your translation verbatim, with identical spelling and braces.
- Do **not** rename, add, or drop any placeholder.
- ICU plural forms (`{{count, plural, one {{# item}} other {{# items}}}}`)
  must not be modified structurally — only translate the literal words.

## Done signal

`import-batch` considers the batch complete when `targets.jsonl` contains
exactly {unit_count} line(s). Fewer lines → `IncompleteBatch` error.
There is no sentinel line or done-marker file.
"#
    )
}

// ── read_targets ─────────────────────────────────────────────────────────────

/// Parse `targets.jsonl` in `dir` and return [`ManualResponse`]s in the same
/// order as `units.jsonl`.
///
/// # Validation
///
/// 1. Reads `units.jsonl` to obtain the canonical unit order.
/// 2. Reads every line of `targets.jsonl`.
/// 3. Rejects files where line count < unit count ([`AgentBatchError::IncompleteBatch`]).
/// 4. Validates that each line's `id` matches the expected position in
///    `units.jsonl` ([`AgentBatchError::OrderMismatch`] when the id exists but
///    is in the wrong slot; [`AgentBatchError::MismatchedIds`] when it does not
///    appear at all).
///
/// The downstream `(batch, outcomes)` contract depends on positional alignment,
/// not on id lookup — a reordered `targets.jsonl` is a bug the agent must fix.
pub fn read_targets(dir: &Path) -> Result<Vec<ManualResponse>, AgentBatchError> {
    let unit_ids = read_unit_ids(dir)?;
    let expected = unit_ids.len();

    let targets_path = dir.join("targets.jsonl");
    let file = fs::File::open(&targets_path)?;
    let reader = BufReader::new(file);

    let mut responses = Vec::with_capacity(expected);
    for (idx, raw) in reader.lines().enumerate() {
        let line_num = idx + 1;
        let raw = raw?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parsed: Result<TargetLine, _> = serde_json::from_str(trimmed);
        let target_line = match parsed {
            Ok(t) => t,
            Err(e) => {
                // Try to distinguish "unknown kind" from "malformed JSON".
                // Parse as a plain object to fish out the `kind` field.
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if let Some(kind) = v.get("kind").and_then(|k| k.as_str()) {
                        match kind {
                            "singular" | "plural" | "skip" | "fail" => {
                                return Err(AgentBatchError::MalformedTarget {
                                    line: line_num,
                                    msg: e.to_string(),
                                });
                            }
                            other => {
                                return Err(AgentBatchError::UnknownTargetKind {
                                    line: line_num,
                                    kind: other.to_owned(),
                                });
                            }
                        }
                    }
                }
                return Err(AgentBatchError::Json {
                    line: line_num,
                    source: e,
                });
            }
        };

        let pos = responses.len();
        if pos >= expected {
            // Extra lines are allowed (agent may add a trailing newline);
            // we stop consuming after we have enough responses.
            break;
        }

        let got_id = target_line.id().to_owned();
        let expected_id = &unit_ids[pos];
        if &got_id != expected_id {
            // Distinguish "id exists but in wrong position" from "id unknown".
            if unit_ids.contains(&got_id) {
                return Err(AgentBatchError::OrderMismatch {
                    line: line_num,
                    expected: expected_id.clone(),
                    got: got_id,
                });
            } else {
                return Err(AgentBatchError::MismatchedIds {
                    line: line_num,
                    id: got_id,
                });
            }
        }

        responses.push(target_line.into_response());
    }

    if responses.len() < expected {
        return Err(AgentBatchError::IncompleteBatch {
            expected,
            got: responses.len(),
        });
    }

    Ok(responses)
}

/// Read `units.jsonl` and return the unit ids in file order.
fn read_unit_ids(dir: &Path) -> Result<Vec<String>, AgentBatchError> {
    let path = dir.join("units.jsonl");
    let file = fs::File::open(&path)?;
    let reader = BufReader::new(file);
    let mut ids = Vec::new();
    for (idx, raw) in reader.lines().enumerate() {
        let line_num = idx + 1;
        let raw = raw?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let eu: ExportedUnit =
            serde_json::from_str(trimmed).map_err(|e| AgentBatchError::Json {
                line: line_num,
                source: e,
            })?;
        ids.push(eu.id);
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use i18n_harness_core::{Batch, BatchKey, Target, Unit};
    use i18n_harness_locales::Locale;
    use std::io::Write;
    use tempfile::TempDir;

    fn de_de() -> &'static Locale {
        Locale::by_id("de_DE").expect("de_DE")
    }

    fn singular_unit(id: &str, source: &str) -> Unit {
        Unit::untranslated_singular(id, source)
    }

    fn plural_unit(id: &str, source: &str) -> Unit {
        let mut u = Unit::untranslated_singular(id, source);
        u.plural_arity = Some(2);
        u.target = Target::Plural {
            forms: vec![None, None],
        };
        u
    }

    fn make_batch(units: Vec<Unit>) -> Batch {
        Batch::new(BatchKey::new("test-hash", 0), units)
    }

    // ── round-trip property test ────────────────────────────────────────────

    #[test]
    fn round_trip_singular_and_plural_via_manual_backend() {
        use crate::{ManualBackend, TranslatedText, TranslationBackend, TranslationOutcome};
        use std::collections::HashMap;

        let locale = de_de();
        let units = vec![
            singular_unit("app.greeting", "Hello"),
            singular_unit("app.title", "World"),
            plural_unit("app.items", "item"),
        ];
        let batch = make_batch(units);
        let tmp = TempDir::new().unwrap();

        write_export(
            tmp.path(),
            &batch,
            locale,
            None,
            None,
            Path::new("/fixtures/test.ts"),
            "Translate to German.",
        )
        .expect("write_export");

        // Fill targets.jsonl with identity responses.
        let targets_path = tmp.path().join("targets.jsonl");
        let mut f = fs::File::create(&targets_path).unwrap();
        for unit in &batch.units {
            let line = if unit.plural_arity.is_some() {
                let arity = locale.plural_arity() as usize;
                let texts: Vec<String> = (0..arity).map(|_| unit.source.clone()).collect();
                let texts_json = serde_json::to_string(&texts).unwrap();
                format!(
                    "{{\"id\":\"{}\",\"kind\":\"plural\",\"texts\":{}}}\n",
                    unit.id, texts_json
                )
            } else {
                format!(
                    "{{\"id\":\"{}\",\"kind\":\"singular\",\"text\":\"{}\"}}\n",
                    unit.id, unit.source
                )
            };
            f.write_all(line.as_bytes()).unwrap();
        }
        drop(f);

        let responses = read_targets(tmp.path()).expect("read_targets");
        assert_eq!(responses.len(), batch.units.len());

        // Drive ManualBackend with the parsed responses.
        let response_map: HashMap<String, ManualResponse> = batch
            .units
            .iter()
            .map(|u| u.id.as_str().to_owned())
            .zip(responses.iter().cloned())
            .collect();

        let backend = ManualBackend::named("agent", false, |ctx| {
            response_map[ctx.unit.id.as_str()].clone()
        });
        let outcomes = backend.translate_batch(&batch, locale, None).unwrap();
        assert_eq!(outcomes.len(), batch.units.len());

        for (outcome, unit) in outcomes.iter().zip(&batch.units) {
            match outcome {
                TranslationOutcome::Translated {
                    text: TranslatedText::Singular(s),
                    ..
                } => {
                    assert!(unit.plural_arity.is_none(), "expected singular unit");
                    assert_eq!(s, &unit.source);
                }
                TranslationOutcome::Translated {
                    text: TranslatedText::Plural(forms),
                    ..
                } => {
                    assert!(unit.plural_arity.is_some(), "expected plural unit");
                    for form in forms {
                        assert_eq!(form, &unit.source);
                    }
                }
                other => panic!("expected Translated, got {other:?}"),
            }
        }
    }

    // ── error cases ─────────────────────────────────────────────────────────

    fn setup_export(tmp: &TempDir, units: Vec<Unit>) -> Batch {
        let locale = de_de();
        let batch = make_batch(units);
        write_export(
            tmp.path(),
            &batch,
            locale,
            None,
            None,
            Path::new("/fixtures/test.ts"),
            "prompt",
        )
        .unwrap();
        batch
    }

    #[test]
    fn incomplete_targets_yields_incomplete_batch_error() {
        let tmp = TempDir::new().unwrap();
        setup_export(&tmp, vec![singular_unit("a", "A"), singular_unit("b", "B")]);

        // Write only one response.
        let mut f = fs::File::create(tmp.path().join("targets.jsonl")).unwrap();
        writeln!(f, r#"{{"id":"a","kind":"singular","text":"A"}}"#).unwrap();
        drop(f);

        let err = read_targets(tmp.path()).unwrap_err();
        assert!(
            matches!(
                err,
                AgentBatchError::IncompleteBatch {
                    expected: 2,
                    got: 1
                }
            ),
            "got {err}"
        );
    }

    #[test]
    fn reordered_ids_yield_order_mismatch_error() {
        let tmp = TempDir::new().unwrap();
        setup_export(&tmp, vec![singular_unit("a", "A"), singular_unit("b", "B")]);

        let mut f = fs::File::create(tmp.path().join("targets.jsonl")).unwrap();
        // ids in reversed order
        writeln!(f, r#"{{"id":"b","kind":"singular","text":"B"}}"#).unwrap();
        writeln!(f, r#"{{"id":"a","kind":"singular","text":"A"}}"#).unwrap();
        drop(f);

        let err = read_targets(tmp.path()).unwrap_err();
        assert!(
            matches!(err, AgentBatchError::OrderMismatch { line: 1, .. }),
            "got {err}"
        );
    }

    #[test]
    fn unknown_id_yields_mismatched_ids_error() {
        let tmp = TempDir::new().unwrap();
        setup_export(&tmp, vec![singular_unit("a", "A")]);

        let mut f = fs::File::create(tmp.path().join("targets.jsonl")).unwrap();
        writeln!(
            f,
            r#"{{"id":"DOES_NOT_EXIST","kind":"singular","text":"X"}}"#
        )
        .unwrap();
        drop(f);

        let err = read_targets(tmp.path()).unwrap_err();
        assert!(
            matches!(err, AgentBatchError::MismatchedIds { line: 1, .. }),
            "got {err}"
        );
    }

    #[test]
    fn unknown_kind_yields_unknown_target_kind_error() {
        let tmp = TempDir::new().unwrap();
        setup_export(&tmp, vec![singular_unit("a", "A")]);

        let mut f = fs::File::create(tmp.path().join("targets.jsonl")).unwrap();
        writeln!(f, r#"{{"id":"a","kind":"translate","text":"X"}}"#).unwrap();
        drop(f);

        let err = read_targets(tmp.path()).unwrap_err();
        assert!(
            matches!(
                err,
                AgentBatchError::UnknownTargetKind {
                    line: 1,
                    kind: ref k
                } if k == "translate"
            ),
            "got {err}"
        );
    }

    #[test]
    fn malformed_json_yields_json_error() {
        let tmp = TempDir::new().unwrap();
        setup_export(&tmp, vec![singular_unit("a", "A")]);

        let mut f = fs::File::create(tmp.path().join("targets.jsonl")).unwrap();
        writeln!(f, "not json at all").unwrap();
        drop(f);

        let err = read_targets(tmp.path()).unwrap_err();
        assert!(
            matches!(err, AgentBatchError::Json { line: 1, .. }),
            "got {err}"
        );
    }

    #[test]
    fn missing_text_field_for_singular_yields_malformed_target() {
        let tmp = TempDir::new().unwrap();
        setup_export(&tmp, vec![singular_unit("a", "A")]);

        let mut f = fs::File::create(tmp.path().join("targets.jsonl")).unwrap();
        // `singular` kind requires `text` field
        writeln!(f, r#"{{"id":"a","kind":"singular"}}"#).unwrap();
        drop(f);

        let err = read_targets(tmp.path()).unwrap_err();
        assert!(
            matches!(err, AgentBatchError::MalformedTarget { line: 1, .. }),
            "got {err}"
        );
    }

    // ── curated fixture test ─────────────────────────────────────────────────

    #[test]
    fn curated_fixture_parses_and_runs_through_manual_backend() {
        use crate::{ManualBackend, TranslatedText, TranslationBackend, TranslationOutcome};
        use std::collections::HashMap;

        let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("fixtures/agent-batch/qt-de_DE");

        let locale = de_de();

        // Read unit ids from fixture units.jsonl.
        let unit_ids = read_unit_ids(&fixture_dir).expect("read unit ids from fixture");
        assert_eq!(unit_ids, vec!["app.greeting", "app.items"]);

        // Parse targets.jsonl.
        let responses = read_targets(&fixture_dir).expect("read_targets from fixture");
        assert_eq!(responses.len(), 2);

        // Build a minimal batch matching the fixture.
        let units = vec![
            {
                let mut u = singular_unit("app.greeting", "Hello, {0}!");
                u.placeholders = vec![Placeholder::positional(0, 7)];
                u
            },
            plural_unit("app.items", "{count, plural, one {# item} other {# items}}"),
        ];
        let batch = make_batch(units);

        let response_map: HashMap<String, ManualResponse> = batch
            .units
            .iter()
            .map(|u| u.id.as_str().to_owned())
            .zip(responses.iter().cloned())
            .collect();

        let backend = ManualBackend::named("agent", false, |ctx| {
            response_map[ctx.unit.id.as_str()].clone()
        });
        let outcomes = backend.translate_batch(&batch, locale, None).unwrap();
        assert_eq!(outcomes.len(), 2);

        // First unit: singular.
        assert!(
            matches!(
                &outcomes[0],
                TranslationOutcome::Translated {
                    text: TranslatedText::Singular(s),
                    ..
                } if s == "Hallo, {0}!"
            ),
            "got {:?}",
            outcomes[0]
        );

        // Second unit: plural with 2 forms.
        match &outcomes[1] {
            TranslationOutcome::Translated {
                text: TranslatedText::Plural(forms),
                ..
            } => {
                assert_eq!(forms.len(), 2);
                assert_eq!(forms[0], "# Eintrag");
                assert_eq!(forms[1], "# Einträge");
            }
            other => panic!("expected Translated plural, got {other:?}"),
        }
    }

    // ── ExportedUnit serde ───────────────────────────────────────────────────

    #[test]
    fn exported_unit_round_trips_through_json() {
        let eu = ExportedUnit {
            id: "app.greeting".to_owned(),
            source: "Hello, {0}!".to_owned(),
            plural_arity: None,
            placeholders: vec![Placeholder::positional(0, 7)],
            flags_so_far: FlagSet::new(),
        };
        let json = serde_json::to_string(&eu).unwrap();
        let back: ExportedUnit = serde_json::from_str(&json).unwrap();
        assert_eq!(eu, back);
    }

    #[test]
    fn exported_unit_omits_optional_fields_when_empty() {
        let eu = ExportedUnit {
            id: "x".to_owned(),
            source: "y".to_owned(),
            plural_arity: None,
            placeholders: vec![],
            flags_so_far: FlagSet::new(),
        };
        let json = serde_json::to_string(&eu).unwrap();
        assert!(!json.contains("plural_arity"), "{json}");
        assert!(!json.contains("placeholders"), "{json}");
    }
}
