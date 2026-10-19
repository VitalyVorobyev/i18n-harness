//! Path resolver for a project's on-disk layout.
//!
//! [`ProjectPaths`] is the **single resolver** for every state file the
//! project crate (and its consumers) ever open. Callers receive resolved
//! absolute paths; they never construct `.i18n-harness/` strings by hand.
//!
//! See `docs/internal/m4.1-project-crate-design.md` §1.7 and §7 for the full
//! path-resolution policy.

use std::path::{Path, PathBuf};

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
    evaluations: PathBuf,
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
        let evaluations = state_dir.join("evaluations.jsonl");
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
            evaluations,
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

    /// Absolute path to `<state_dir>/evaluations.jsonl`.
    pub fn evaluations(&self) -> &Path {
        &self.evaluations
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
