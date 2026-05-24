//! Discovery heuristic for projects without a manifest.
//!
//! [`Project::discover_with_fs`] walks a directory tree, classifies every
//! candidate file, and returns a [`DraftManifest`] for user confirmation.
//! The draft is **never silently persisted** — the caller (CLI or UI) presents
//! it for review before calling [`Project::create_from_draft`].
//!
//! See `docs/m4.1-project-crate-design.md` §3 for the full heuristic spec.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{ProjectError, ProjectWarning};
use crate::fs::{ProjectFs, RealFs};
use crate::manifest::{CatalogEntry, CatalogFormat, GlossaryConfig, LocaleConfig};
use crate::project::Project;

pub(crate) mod locale_infer;
pub(crate) mod sniff;
pub(crate) mod walk;

// Re-export `FormatGuess` from here as the canonical location.
// `manifest.rs` will `pub use discovery::FormatGuess` for backward compat.
pub use crate::manifest::FormatGuess;

// ── Public draft types ────────────────────────────────────────────────────────

/// Output of [`Project::discover`] — a manifest the user reviews before it is
/// written to disk.
///
/// All fields are best-guesses; the UI / CLI presents them for confirmation
/// before calling [`Project::create_from_draft`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DraftManifest {
    /// Absolute path to the project root.
    pub root: PathBuf,
    /// Best-guess project name (the root directory's leaf name).
    pub name: String,
    /// One `LocaleConfig` per distinct locale id observed across all catalogs.
    /// Includes unknown locale ids (user can fix or register them).
    pub locales: BTreeMap<String, LocaleConfig>,
    /// Classified catalog files, sorted by path.
    pub catalogs: Vec<DraftCatalog>,
    /// Detected `glossary.toml` at the project root, if present.
    pub glossary: Option<GlossaryConfig>,
    /// Always `None` — the UI prompts for the backend (design §3.6).
    pub backend: Option<crate::manifest::BackendConfig>,
}

/// One candidate catalog as classified by the discovery heuristic.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DraftCatalog {
    /// Absolute path to the candidate file.
    pub path: PathBuf,
    /// Best-guess format. UI shows alternatives when confidence is below `High`.
    pub format: FormatGuess,
    /// Inferred locale id. `None` when neither adapter-derived nor filename
    /// inference produced a result.
    pub locale: Option<String>,
    /// Confidence level of the classification.
    pub confidence: ClassificationConfidence,
    /// Human-readable reason shown verbatim in the UI for transparency.
    pub reason: String,
    /// Alternative (format, locale) interpretations. Empty when `confidence == High`.
    pub alternatives: Vec<DraftAlternative>,
}

/// An alternative classification for a [`DraftCatalog`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DraftAlternative {
    /// Alternative format guess.
    pub format: FormatGuess,
    /// Alternative locale guess.
    pub locale: Option<String>,
    /// Reason the alternative fired.
    pub reason: String,
}

/// Confidence level for a format / locale classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClassificationConfidence {
    /// Extension and content sniff agree, and locale was found via a primary
    /// adapter-derived rule.
    High,
    /// Extension matched but content sniff is ambiguous, or locale was inferred
    /// via filename-only fallback.
    Medium,
    /// Best-effort guess; user should confirm.
    Low,
}

// ── Project::discover / discover_with_fs / create_from_draft ─────────────────

impl Project {
    /// Discover a project from a directory that has no `i18n-harness.toml`.
    ///
    /// Uses the real filesystem. Returns a [`DraftManifest`] for user
    /// confirmation; never writes anything to disk.
    ///
    /// # Errors
    ///
    /// Returns `ProjectError::Io` if `root` is not readable.
    pub fn discover(root: &Path) -> Result<DraftManifest, ProjectError> {
        Self::discover_with_fs(root, Arc::new(RealFs))
    }

    /// Discover a project using a custom filesystem implementation.
    ///
    /// The testable variant; the production path calls [`Self::discover`].
    ///
    /// # Discovery flow
    ///
    /// 1. Walk from `root`, honoring the skip-list and max-depth = 8.
    /// 2. For each candidate file, read up to 64 KiB and sniff.
    /// 3. Merge locale inference (adapter-derived, then filename fallback).
    /// 4. Synthesize one `LocaleConfig` per distinct locale id (§3.4).
    /// 5. Check for `glossary.toml` at root (§3.5).
    /// 6. Return `DraftManifest`.
    ///
    /// # Errors
    ///
    /// Returns `ProjectError::Io` for unreadable directories.
    pub fn discover_with_fs(
        root: &Path,
        fs: Arc<dyn ProjectFs>,
    ) -> Result<DraftManifest, ProjectError> {
        const MAX_READ: usize = 64 * 1024; // 64 KiB sniff cap

        let candidates = walk::walk_candidates(root, &*fs);

        let mut draft_catalogs: Vec<DraftCatalog> = Vec::new();

        for abs_path in candidates {
            let bytes = match fs.read(&abs_path) {
                Ok(b) => b,
                Err(_) => continue, // unreadable — skip silently
            };
            let prefix = &bytes[..bytes.len().min(MAX_READ)];

            // sniff::sniff returns a DraftCatalog with path = filename only;
            // we fix the path to the absolute path here.
            let mut dc = sniff::sniff(&abs_path, prefix);
            dc.path = abs_path.clone();

            // Only include files that have a recognized format.
            if dc.format != FormatGuess::Unknown {
                draft_catalogs.push(dc);
            }
        }

        // Sort for deterministic output.
        draft_catalogs.sort_by(|a, b| a.path.cmp(&b.path));

        // Synthesize locale blocks from observed locales.
        let mut locales: BTreeMap<String, LocaleConfig> = BTreeMap::new();
        for dc in &draft_catalogs {
            if let Some(ref id) = dc.locale {
                locales.entry(id.clone()).or_default();
            }
        }

        // Glossary inference (§3.5).
        let glossary_path = root.join("glossary.toml");
        let glossary = if fs.exists(&glossary_path) {
            Some(GlossaryConfig {
                path: PathBuf::from("glossary.toml"),
            })
        } else {
            None
        };

        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());

        Ok(DraftManifest {
            root: root.to_path_buf(),
            name,
            locales,
            catalogs: draft_catalogs,
            glossary,
            backend: None,
        })
    }

    /// Create a new project from a [`DraftManifest`] confirmed by the user.
    ///
    /// 1. Builds a [`crate::manifest::ProjectManifest`] from the draft (skipping
    ///    `FormatGuess::Unknown` entries).
    /// 2. Serializes it and writes to `<root>/i18n-harness.toml`.
    /// 3. Opens the result via [`Project::open_with_fs`].
    ///
    /// This is the path the UI's "Create from this folder" button takes.
    ///
    /// # Errors
    ///
    /// - `ProjectError::Io` if the manifest cannot be written.
    /// - Any error from [`Project::open_with_fs`] if the resulting manifest
    ///   fails validation.
    pub fn create_from_draft(
        root: &Path,
        draft: DraftManifest,
    ) -> Result<(Self, Vec<ProjectWarning>), ProjectError> {
        Self::create_from_draft_with_fs(root, draft, Arc::new(RealFs))
    }

    /// Testable variant of [`Self::create_from_draft`].
    pub fn create_from_draft_with_fs(
        root: &Path,
        draft: DraftManifest,
        fs: Arc<dyn ProjectFs>,
    ) -> Result<(Self, Vec<ProjectWarning>), ProjectError> {
        use crate::manifest::{
            BackendBlock, PathsConfig, ProjectManifest, ProjectMeta, SCHEMA_VERSION,
        };

        // Build catalog entries from non-Unknown draft catalogs.
        let catalogs: Vec<CatalogEntry> = draft
            .catalogs
            .iter()
            .filter(|dc| dc.format != FormatGuess::Unknown)
            .filter_map(|dc| {
                let format = match dc.format {
                    FormatGuess::QtTs => CatalogFormat::QtTs,
                    FormatGuess::GettextPo => CatalogFormat::GettextPo,
                    FormatGuess::IcuJson => CatalogFormat::IcuJson,
                    FormatGuess::Unknown => return None,
                };
                let locale = dc.locale.clone().unwrap_or_default();
                // Store path as relative to root.
                let rel = dc
                    .path
                    .strip_prefix(root)
                    .unwrap_or(&dc.path)
                    .to_path_buf();
                Some(CatalogEntry {
                    path: rel,
                    format,
                    locale,
                })
            })
            .collect();

        let manifest = ProjectManifest {
            project: ProjectMeta {
                name: draft.name,
                schema: SCHEMA_VERSION,
            },
            locales: draft.locales,
            catalogs,
            glossary: draft.glossary,
            backends: BackendBlock {
                default: draft.backend,
            },
            prompts: None,
            paths: PathsConfig::default(),
        };

        let toml_text = toml::to_string_pretty(&manifest).map_err(|e| {
            // toml serialization errors map to an Io error for simplicity;
            // they indicate a programming error, not user input.
            ProjectError::Io {
                path: root.join("i18n-harness.toml"),
                source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
            }
        })?;

        // Ensure root exists.
        fs.create_dir_all(root)
            .map_err(|source| ProjectError::Io {
                path: root.to_path_buf(),
                source,
            })?;

        let manifest_path = root.join("i18n-harness.toml");
        fs.write_atomic(&manifest_path, toml_text.as_bytes())
            .map_err(|source| ProjectError::Io {
                path: manifest_path.clone(),
                source,
            })?;

        Project::open_with_fs(root, fs)
    }
}
