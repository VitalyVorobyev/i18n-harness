//! Path resolver for a project's on-disk layout.
//!
//! [`ProjectPaths`] is the **single resolver** for every state file the
//! project crate (and its consumers) ever open. Callers receive resolved
//! absolute paths; they never construct `.i18n-harness/` strings by hand.
//!
//! See `docs/m4.1-project-crate-design.md` §1.7 and §7 for the full
//! path-resolution policy.

use std::path::{Path, PathBuf};

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::manifest::{PathsConfig, PromptsConfig};

/// All on-disk paths a project resolves. Each accessor returns an absolute
/// path. The state directory is `mkdir -p`'d on `Project::open`; individual
/// files are created only on first write.
///
/// # Construction
///
/// Created by `Project::open` / `Project::open_with_fs` — callers access it
/// via [`crate::project::Project::paths`]. There is deliberately no public
/// constructor because valid `ProjectPaths` values require an open project.
#[derive(Debug, Clone)]
pub struct ProjectPaths {
    root: PathBuf,
    manifest: PathBuf,
    glossary: Option<PathBuf>,
    state_dir: PathBuf,
    metrics: PathBuf,
    corrections: PathBuf,
    curated: PathBuf,
    review: PathBuf,
    batches: PathBuf,
    tuning_root: PathBuf,
    prompts_template_dir: Option<PathBuf>,
}

impl ProjectPaths {
    /// Construct from a project root and the parsed manifest sections that
    /// affect path resolution.
    ///
    /// `paths_config` may carry a `state_dir` override; `glossary_path` is
    /// the manifest-relative glossary path if declared; `prompts_config`
    /// carries the template dir if declared.
    ///
    /// All output paths are absolute. Relative overrides in `paths_config`
    /// resolve against `root`.
    /// Construct from a project root and the manifest sections that affect
    /// path resolution. Called by `Project::open_with_fs`; also callable
    /// directly from tests that need a `ProjectPaths` without a full `Project`.
    pub fn new(
        root: &Path,
        paths_config: &PathsConfig,
        glossary_path: Option<&Path>,
        prompts_config: Option<&PromptsConfig>,
    ) -> Self {
        let root = root.to_path_buf();

        let state_dir = match &paths_config.state_dir {
            Some(p) if p.is_absolute() => p.clone(),
            Some(p) => root.join(p),
            None => root.join(".i18n-harness"),
        };

        let manifest = root.join("i18n-harness.toml");
        let glossary = glossary_path.map(|p| root.join(p));
        let metrics = state_dir.join("metrics.jsonl");
        let corrections = state_dir.join("corrections.jsonl");
        let curated = state_dir.join("curated.toml");
        let review = state_dir.join("review.jsonl");
        let batches = state_dir.join("state").join("batches.jsonl");
        let tuning_root = state_dir.join("tuning");
        let prompts_template_dir = prompts_config.map(|pc| root.join(&pc.template_dir));

        Self {
            root,
            manifest,
            glossary,
            state_dir,
            metrics,
            corrections,
            curated,
            review,
            batches,
            tuning_root,
            prompts_template_dir,
        }
    }

    /// The project root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Absolute path to `<root>/i18n-harness.toml`.
    pub fn manifest(&self) -> &Path {
        &self.manifest
    }

    /// Absolute path to the glossary file, if declared in the manifest.
    pub fn glossary(&self) -> Option<&Path> {
        self.glossary.as_deref()
    }

    /// Absolute path to the state directory (`<root>/.i18n-harness/` by
    /// default, or the override from `[paths] state_dir`).
    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    /// Absolute path to `<state_dir>/metrics.jsonl`.
    pub fn metrics(&self) -> &Path {
        &self.metrics
    }

    /// Absolute path to `<state_dir>/corrections.jsonl`.
    pub fn corrections(&self) -> &Path {
        &self.corrections
    }

    /// Absolute path to `<state_dir>/curated.toml`.
    pub fn curated(&self) -> &Path {
        &self.curated
    }

    /// Absolute path to `<state_dir>/review.jsonl`.
    pub fn review(&self) -> &Path {
        &self.review
    }

    /// Absolute path to `<state_dir>/state/batches.jsonl`.
    pub fn batches(&self) -> &Path {
        &self.batches
    }

    /// Absolute path to the `<state_dir>/tuning/` root.
    pub fn tuning_root(&self) -> &Path {
        &self.tuning_root
    }

    /// Generate a fresh timestamped directory for a tuning bundle export.
    ///
    /// Format: `<state_dir>/tuning/<YYYY-MM-DDTHH-MM-SS-ffffff>/` where
    /// colons are replaced with hyphens for cross-platform path safety (as
    /// established by `docs/roadmap-product.md`). Each call produces a unique
    /// directory because the timestamp includes microseconds.
    pub fn new_tuning_bundle_dir(&self) -> PathBuf {
        // Build the timestamp format: colons replaced with hyphens.
        // time's format_description does not allow literal colons in the
        // time portion when targeting path-safe output, so we format using
        // RFC 3339 and post-process.
        let now = OffsetDateTime::now_utc();
        // Format: 2026-05-24T12:34:56.123456Z  (RFC3339 micro-precision)
        let ts_raw = now.format(&Rfc3339).unwrap_or_else(|_| {
            // Fallback: use unix timestamp as string (should never fire).
            format!("{}", now.unix_timestamp())
        });
        // Replace colons and the trailing 'Z' / offset to get a safe dir name.
        // Input looks like: "2026-05-24T12:34:56.123456Z"
        // Target:            "2026-05-24T12-34-56-123456"
        // Replace ':' and '.' (fractional-seconds separator) with '-' for
        // cross-platform path safety; strip the trailing 'Z' / offset first.
        let ts = ts_raw.trim_end_matches('Z').replace([':', '.'], "-");
        self.tuning_root.join(ts)
    }

    /// Resolve a catalog's manifest-relative path to an absolute path.
    ///
    /// `manifest_relative` is the path as stored in `[[catalogs]] path` — it
    /// is relative to the project root in the manifest.
    pub fn catalog(&self, manifest_relative: &Path) -> PathBuf {
        self.root.join(manifest_relative)
    }

    /// Resolve a prompt template path for `locale_id`.
    ///
    /// Returns `<template_dir>/<locale_id>.txt` if `[prompts]` is declared in
    /// the manifest, or `None` otherwise.
    pub fn prompt_template(&self, locale_id: &str) -> Option<PathBuf> {
        self.prompts_template_dir
            .as_ref()
            .map(|dir| dir.join(format!("{locale_id}.txt")))
    }
}
