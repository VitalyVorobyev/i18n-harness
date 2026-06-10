//! Tuning-bundle export for the prompt-tuning loop.
//!
//! [`export_tuning_bundle`](Project::export_tuning_bundle) collects the curated
//! example set, the active prompt template, the latest evaluation run, and the
//! project's locale config into a self-contained directory that can be handed
//! to an external tool (e.g. the `tune-i18n-prompt` Claude Code skill) for
//! prompt refinement.
//!
//! See `docs/roadmap-product.md` for the full specification.
//!
//! # Bundle layout
//!
//! ```text
//! .i18n-harness/tuning/<ISO-timestamp>/
//!   examples.jsonl   — one JSON object per resolved curated example
//!   prompt.txt       — verbatim copy of the active prompt template
//!   score.json       — latest EvaluationRun (absent when none exists)
//!   locales.toml     — per-locale LocaleConfig table from the manifest
//!   README.md        — skill contract (copy of skills/tune-i18n-prompt/README.md)
//! ```

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::ProjectError;
use crate::project::Project;

// ── Bundle-example wire type ──────────────────────────────────────────────────

/// One resolved example line in `examples.jsonl`.
///
/// Derived from `CuratedExample` + its backing `Correction`. Fields that are
/// only useful for internal bookkeeping (`id`, `ts`, `catalog`, `unit_id`,
/// `provenance`) are deliberately dropped — the skill only needs the textual
/// content.
#[derive(Debug, Serialize, Deserialize)]
pub struct BundleExample {
    /// Schema version for this line shape. Currently `1`.
    pub schema: u32,
    /// Source text that was being translated.
    pub source: String,
    /// What the model proposed (empty string for a human-from-scratch entry).
    pub mt_proposal: String,
    /// The accepted human translation.
    pub human_target: String,
    /// Target locale id.
    pub locale: String,
    /// Flags the unit had at correction time (kebab-case strings).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    /// Human teaching note added when the correction was promoted to curated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ── Summary returned to callers ───────────────────────────────────────────────

/// Metadata about a successfully exported tuning bundle.
///
/// Returned by [`Project::export_tuning_bundle`] and
/// [`Project::list_tuning_bundles`]. Callers use this to surface a summary in
/// the UI without reading the bundle directory themselves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningBundleSummary {
    /// Absolute path to the bundle directory.
    pub path: String,
    /// Number of resolved examples written to `examples.jsonl`.
    pub examples_count: usize,
    /// Locale ids that appear in at least one example.
    pub locales: Vec<String>,
    /// `true` if `score.json` was written (i.e. a prior evaluation existed).
    pub has_score: bool,
    /// Prompt template version identifier baked into `prompt.txt`.
    pub prompt_template_version: String,
}

// ── Skill README (embedded verbatim) ─────────────────────────────────────────

const SKILL_README: &str = include_str!("../../../skills/tune-i18n-prompt/README.md");

// ── impl Project ─────────────────────────────────────────────────────────────

impl Project {
    /// Export a tuning bundle to a fresh timestamped directory under
    /// `.i18n-harness/tuning/`. Returns metadata about what landed.
    ///
    /// # What is written
    ///
    /// - `examples.jsonl` — one line per resolved curated example. Dangling
    ///   references (curated id with no matching correction in `corrections.jsonl`)
    ///   are silently skipped.
    /// - `prompt.txt` — verbatim copy of the active prompt template
    ///   (`ollama-translate-v2.txt` embedded at compile time).
    /// - `score.json` — the latest `EvaluationRun`. Omitted when no evaluation
    ///   has been run yet; its absence is meaningful ("no baseline score").
    /// - `locales.toml` — the project's per-locale `LocaleConfig` table in TOML.
    /// - `README.md` — verbatim copy of `skills/tune-i18n-prompt/README.md`.
    ///
    /// # Errors
    ///
    /// - [`ProjectError::NoCuratedExamples`] if the curated set is empty.
    /// - [`ProjectError::Io`] if any filesystem write fails.
    ///
    /// A partial bundle directory is left on disk when a write fails partway
    /// through — the caller (Tauri layer) should surface the error to the user
    /// so they can clean up manually.
    pub fn export_tuning_bundle(&self) -> Result<TuningBundleSummary, ProjectError> {
        // Guard: refuse an empty curated set.
        let curated = self.curated();
        if curated.is_empty() {
            return Err(ProjectError::NoCuratedExamples);
        }

        // Resolve corrections for all curated examples in one pass.
        let (corrections, _parse_errors) = self.corrections().read_all()?;
        let correction_map: HashMap<_, _> = corrections.iter().map(|c| (&c.id, c)).collect();

        // Build the example list; skip dangling references silently.
        let mut examples: Vec<BundleExample> = Vec::new();
        let mut seen_locales: BTreeSet<String> = Default::default();

        for curated_example in curated.examples() {
            let Some(correction) = correction_map.get(&curated_example.id) else {
                // Dangling reference — correction was deleted or never written.
                continue;
            };

            let flags: Vec<String> = correction
                .flags_at_correction
                .iter()
                .map(|f| {
                    serde_json::to_value(f)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_else(|| format!("{f:?}"))
                })
                .collect();

            seen_locales.insert(correction.locale.clone());

            examples.push(BundleExample {
                schema: 1,
                source: correction.source.clone(),
                mt_proposal: correction.mt_proposal.clone(),
                human_target: correction.human_target.clone(),
                locale: correction.locale.clone(),
                flags,
                note: curated_example.note.clone(),
            });
        }

        // If every curated reference was dangling, treat it as empty.
        if examples.is_empty() {
            return Err(ProjectError::NoCuratedExamples);
        }

        // Create the timestamped bundle directory.
        let bundle_dir = self.paths().new_tuning_bundle_dir();
        self.fs()
            .create_dir_all(&bundle_dir)
            .map_err(|source| ProjectError::Io {
                path: bundle_dir.clone(),
                source,
            })?;

        // --- examples.jsonl ---
        write_examples_jsonl(self, &bundle_dir, &examples)?;

        // --- prompt.txt ---
        let prompt_text = i18n_harness_backend::ollama_prompt_v2();
        write_file(self, &bundle_dir.join("prompt.txt"), prompt_text.as_bytes())?;

        // --- score.json (omitted if no evaluation has been run) ---
        let has_score = if let Some(run) = self.evaluations().latest()? {
            let json = serde_json::to_string_pretty(&run)
                .expect("EvaluationRun serialization is infallible");
            write_file(self, &bundle_dir.join("score.json"), json.as_bytes())?;
            true
        } else {
            false
        };

        // --- locales.toml ---
        let locales_toml = build_locales_toml(self);
        write_file(
            self,
            &bundle_dir.join("locales.toml"),
            locales_toml.as_bytes(),
        )?;

        // --- README.md ---
        write_file(self, &bundle_dir.join("README.md"), SKILL_README.as_bytes())?;

        Ok(TuningBundleSummary {
            path: bundle_dir.display().to_string(),
            examples_count: examples.len(),
            locales: seen_locales.into_iter().collect(),
            has_score,
            prompt_template_version: i18n_harness_backend::OLLAMA_PROMPT_V2_VERSION.to_owned(),
        })
    }

    /// List previously-exported tuning bundles under `.i18n-harness/tuning/`,
    /// newest-first. Each entry is a lightweight summary derived from reading
    /// `examples.jsonl` within the bundle directory.
    ///
    /// Directories that lack `examples.jsonl` (e.g. partial exports) are
    /// silently skipped. Returns an empty `Vec` if the tuning root does not
    /// exist yet.
    pub fn list_tuning_bundles(&self) -> Result<Vec<TuningBundleSummary>, ProjectError> {
        let tuning_root = self.paths().tuning_root();

        if !self.fs().exists(tuning_root) {
            return Ok(Vec::new());
        }

        let dirs = self
            .fs()
            .list_dir(tuning_root)
            .map_err(|source| ProjectError::Io {
                path: tuning_root.to_path_buf(),
                source,
            })?;

        let mut summaries: Vec<TuningBundleSummary> = Vec::new();

        for dir in dirs {
            if !self.fs().is_dir(&dir) {
                continue;
            }
            let examples_path = dir.join("examples.jsonl");
            if !self.fs().is_file(&examples_path) {
                continue;
            }

            // Parse examples.jsonl to count examples and collect locales.
            let text = match self.fs().read_to_string(&examples_path) {
                Ok(t) => t,
                Err(_) => continue,
            };

            let mut examples_count = 0usize;
            let mut locales: BTreeSet<String> = Default::default();
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(ex) = serde_json::from_str::<BundleExample>(line) {
                    examples_count += 1;
                    locales.insert(ex.locale);
                }
            }

            let has_score = self.fs().is_file(&dir.join("score.json"));

            // The prompt template written into every bundle is the compile-time
            // constant `OLLAMA_PROMPT_V2_VERSION`. The raw template file contains
            // an unsubstituted `{template_version}` placeholder, so we cannot
            // parse the version from `prompt.txt` reliably — use the constant
            // directly.
            let prompt_template_version = i18n_harness_backend::OLLAMA_PROMPT_V2_VERSION.to_owned();

            summaries.push(TuningBundleSummary {
                path: dir.display().to_string(),
                examples_count,
                locales: locales.into_iter().collect(),
                has_score,
                prompt_template_version,
            });
        }

        // Newest-first: bundle dirs are named by ISO timestamp; reverse
        // lexicographic order gives newest first.
        summaries.sort_by(|a, b| b.path.cmp(&a.path));

        Ok(summaries)
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Serialize and write `examples` as JSONL into `<bundle_dir>/examples.jsonl`.
fn write_examples_jsonl(
    project: &Project,
    bundle_dir: &Path,
    examples: &[BundleExample],
) -> Result<(), ProjectError> {
    let path = bundle_dir.join("examples.jsonl");
    let mut content = String::new();
    for ex in examples {
        let line = serde_json::to_string(ex).expect("BundleExample serialization is infallible");
        debug_assert!(
            !line.contains('\n'),
            "BundleExample JSON must not contain literal newlines"
        );
        content.push_str(&line);
        content.push('\n');
    }
    write_file(project, &path, content.as_bytes())
}

/// Write `contents` to `path` via the project's `ProjectFs`.
fn write_file(project: &Project, path: &Path, contents: &[u8]) -> Result<(), ProjectError> {
    project
        .fs()
        .write_atomic(path, contents)
        .map_err(|source| ProjectError::Io {
            path: path.to_path_buf(),
            source,
        })
}

/// Serialize the project's locale configs to a TOML string.
///
/// Format:
/// ```toml
/// [de_DE]
/// register = "formal"
///
/// [fr_FR]
/// register = "neutral"
/// length_warn_ratio = 1.4
/// ```
fn build_locales_toml(project: &Project) -> String {
    use std::fmt::Write as FmtWrite;

    let locales = &project.manifest().locales;
    if locales.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    for (id, cfg) in locales {
        writeln!(out, "[{id}]").ok();
        if let Some(reg) = &cfg.register {
            let reg_str = match reg {
                crate::manifest::RegisterOverride::Formal => "formal",
                crate::manifest::RegisterOverride::Informal => "informal",
                crate::manifest::RegisterOverride::Neutral => "neutral",
            };
            writeln!(out, r#"register = "{reg_str}""#).ok();
        }
        if let Some(variant) = &cfg.variant {
            writeln!(out, r#"variant = "{variant}""#).ok();
        }
        if let Some(ratio) = cfg.length_warn_ratio {
            writeln!(out, "length_warn_ratio = {ratio}").ok();
        }
        writeln!(out).ok();
    }

    out
}
