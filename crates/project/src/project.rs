//! The live `Project` value — manifest + resolved paths + catalog index +
//! glossary, with a `toml_edit`-backed mutation surface that preserves user
//! comments and key ordering on every write.
//!
//! See `docs/m4.1-project-crate-design.md` §1.3, §2, §1.4.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use i18n_harness_core::{ReviewStatus, Unit, UnitId};
use i18n_harness_glossary::Glossary;
use i18n_harness_locales::Locale;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::error::{ProjectError, ProjectWarning};
use crate::evaluation::EvaluationStore;
use crate::fs::{ProjectFs, RealFs};
use crate::locale::ResolvedLocale;
use crate::manifest::{
    BackendConfig, BackendKind, CatalogEntry, CatalogFormat, GlossaryConfig, LocaleConfig,
    PathsConfig, ProjectManifest, ProjectMeta, PromptsConfig, ReferenceEntry, RegisterOverride,
    SCHEMA_VERSION,
};
use crate::memory::{
    Correction, CorrectionFilter, CorrectionId, CorrectionStore, CuratedExample, CuratedSet,
    NewCorrection,
};
use crate::paths::ProjectPaths;
use crate::review::{ReviewEvent, ReviewRecord, ReviewStore};

// ── Wire-shape summary types ──────────────────────────────────────────────────

/// Compact summary safe to send over the Tauri IPC bridge.
///
/// Paths are `String` (not `PathBuf`) so the TypeScript layer receives plain
/// strings without conversion. This type is output-only — it is not
/// `Deserialize` because the UI never sends it back.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProjectSummary {
    /// Absolute path to the project root.
    pub root: String,
    /// Human-readable project name.
    pub name: String,
    /// Manifest schema version.
    pub schema: u32,
    /// Locale ids declared in the manifest, in document order.
    pub locales: Vec<String>,
    /// All registered catalogs.
    pub catalogs: Vec<CatalogRef>,
    /// All declared `[[references]]` entries (expert catalogs reused into
    /// same-locale catalogs). Empty when none are declared.
    pub references: Vec<ReferenceRef>,
    /// Absolute path to the glossary file, if declared.
    pub glossary_path: Option<String>,
    /// Default backend config, if declared.
    pub backend: Option<BackendConfig>,
    /// Absolute path to the state directory.
    pub state_dir: String,
}

/// One catalog entry as resolved to absolute paths, for IPC and quick access.
///
/// `absolute_path` and `manifest_path` are both strings (not `PathBuf`) for
/// TS-interop consistency with [`ProjectSummary`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CatalogRef {
    /// Absolute path to the catalog file on disk.
    pub absolute_path: String,
    /// Path as stored in the manifest (project-relative).
    pub manifest_path: String,
    /// Declared format.
    pub format: CatalogFormat,
    /// Locale id this catalog serves.
    pub locale: String,
    /// Health of the catalog entry as determined at open time.
    pub status: CatalogStatus,
}

/// Health status of a catalog entry, determined when the project is opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogStatus {
    /// File exists and format classification matches the declared format.
    Ok,
    /// File is missing on disk. `Project::open` returns `CatalogNotFound` for
    /// this case; this variant is reserved for the discovery / draft path
    /// discovery path where a missing catalog is a soft warning, not a hard error.
    Missing,
    /// File is present but the content sniffer disagrees with the declared
    /// format. Not yet produced (the sniffer is not yet wired in); the
    /// variant is defined so the error enum compiles.
    FormatMismatch,
}

/// One reference entry as resolved to absolute paths.
///
/// Parallel to [`CatalogRef`] but for `[[references]]` entries. Paths are
/// `String` (not `PathBuf`) for TS-interop consistency with [`ProjectSummary`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReferenceRef {
    /// Absolute path to the reference catalog file on disk.
    pub absolute_path: String,
    /// Path as stored in the manifest (project-relative).
    pub manifest_path: String,
    /// Declared format.
    pub format: CatalogFormat,
    /// Locale id this reference serves.
    pub locale: String,
    /// Health of the reference entry as determined at open time.
    pub status: CatalogStatus,
}

// ── Project ───────────────────────────────────────────────────────────────────

/// A loaded, validated project.
///
/// Owns the manifest (read view), the comment-preserving `toml_edit` document
/// (write source of truth), the resolved path set, the catalog index, and the
/// parsed glossary.
///
/// # Invariants
///
/// - `manifest.project.schema == SCHEMA_VERSION` (loader rejects future
///   versions).
/// - Every `CatalogRef` entry was verified to exist on disk at open time.
/// - `paths.state_dir()` exists (created on open if absent).
/// - `glossary` is `Some` iff the manifest declares `[glossary]` and the file
///   loaded successfully.
pub struct Project {
    root: PathBuf,
    fs: Arc<dyn ProjectFs>,
    /// Source of truth for writes; never exposed directly.
    doc: DocumentMut,
    /// Derived from `doc` after every mutation — always a function of `doc`.
    manifest: ProjectManifest,
    glossary: Option<Glossary>,
    paths: ProjectPaths,
    catalogs: Vec<CatalogRef>,
    references: Vec<ReferenceRef>,
    /// Lazy correction store — the file is not opened until first use.
    correction_store: CorrectionStore,
    /// In-memory curated set. Reloaded on every promote/un-curate.
    curated: CuratedSet,
    /// Lazy review store — the file is not opened until first use.
    review_store: ReviewStore,
    /// Lazy evaluation store — the file is not opened until first use.
    evaluation_store: EvaluationStore,
    /// Cached folded review map. Populated on first `review_map()` call;
    /// cleared (set to `None`) after each successful `set_review_status` so
    /// the next read re-folds the file.
    review_map_cache: RefCell<Option<crate::review::ReviewMap>>,
}

impl std::fmt::Debug for Project {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Project")
            .field("root", &self.root)
            .field("manifest", &self.manifest)
            .field("paths", &self.paths)
            .field("catalogs", &self.catalogs)
            .field("references", &self.references)
            .finish_non_exhaustive()
    }
}

// ReviewStore and RefCell<Option<...>> don't implement Debug; the manual impl
// above is needed to avoid the derive issue. Both fields are intentionally
// omitted from the debug output — they are implementation details.

impl Project {
    // ── Open ──────────────────────────────────────────────────────────────────

    /// Load and validate a project rooted at `root` using the real filesystem.
    ///
    /// # Errors
    ///
    /// See `Project::open_with_fs` — same contract.
    pub fn open(root: &Path) -> Result<(Self, Vec<ProjectWarning>), ProjectError> {
        Self::open_with_fs(root, Arc::new(RealFs))
    }

    /// Load and validate a project rooted at `root` using `fs`.
    ///
    /// The testable variant. `fs` is typically [`crate::fs::InMemoryFs`] in
    /// tests and [`crate::fs::RealFs`] in production.
    ///
    /// # Open flow
    ///
    /// 1. Check for `<root>/i18n-harness.toml`; return `ManifestMissing` if
    ///    absent.
    /// 2. Read and parse through `toml_edit::DocumentMut` (comment-preserving)
    ///    and `toml::from_str` (typed read view).
    /// 3. Validate schema version.
    /// 4. Resolve `ProjectPaths`; ensure state dir exists.
    /// 5. Validate and index every `[[catalogs]]` entry.
    /// 6. Emit `ProjectWarning::UnknownLocale` for unresolvable locale ids.
    /// 7. Load glossary if declared; thread `GlossaryWarning`s through.
    ///
    /// # Errors
    ///
    /// - `ManifestMissing` — no `i18n-harness.toml` at `root`.
    /// - `ManifestParse` — malformed TOML or unknown sub-table field.
    /// - `UnsupportedSchemaVersion` — manifest `schema` > `SCHEMA_VERSION`.
    /// - `CatalogNotFound` — a `[[catalogs]]` path does not exist on disk.
    /// - `Glossary` — transparent; glossary file declared but fails to load.
    /// - `Io` — generic filesystem failure.
    pub fn open_with_fs(
        root: &Path,
        fs: Arc<dyn ProjectFs>,
    ) -> Result<(Self, Vec<ProjectWarning>), ProjectError> {
        let manifest_path = root.join("i18n-harness.toml");
        if !fs.exists(&manifest_path) {
            return Err(ProjectError::ManifestMissing {
                path: root.to_path_buf(),
            });
        }

        let text = fs
            .read_to_string(&manifest_path)
            .map_err(|source| ProjectError::Io {
                path: manifest_path.clone(),
                source,
            })?;

        // Parse into comment-preserving document (source of truth for writes).
        // If toml_edit rejects the file, fall through to a toml parse which
        // will produce the same error with a proper `toml::de::Error`.
        let doc = DocumentMut::from_str(&text).map_err(|_| {
            // Re-parse through serde/toml to obtain a well-typed error.
            let toml_err: toml::de::Error = toml::from_str::<toml::Value>(&text)
                .expect_err("toml_edit rejected but toml accepted — should not happen");
            ProjectError::ManifestParse {
                path: manifest_path.clone(),
                source: toml_err,
            }
        })?;

        // Derive the typed read view from the document string.
        let (manifest, _inner_warnings) =
            ProjectManifest::from_toml(&doc.to_string()).map_err(|e| match e {
                ProjectError::ManifestParse { source, .. } => ProjectError::ManifestParse {
                    path: manifest_path.clone(),
                    source,
                },
                other => other,
            })?;

        // Resolve paths.
        let paths = ProjectPaths::new(
            root,
            &manifest.paths,
            manifest.glossary.as_ref().map(|g| g.path.as_path()),
            manifest.prompts.as_ref(),
        );

        // Ensure state dir exists.
        fs.create_dir_all(paths.state_dir())
            .map_err(|source| ProjectError::Io {
                path: paths.state_dir().to_path_buf(),
                source,
            })?;

        let mut warnings: Vec<ProjectWarning> = Vec::new();

        // Validate every catalog entry.
        let catalogs = build_catalog_refs(&manifest, &paths, &*fs, &mut warnings)?;

        // Validate every reference entry (soft — missing or mismatched files
        // emit warnings, not errors, so the project still opens).
        let references = build_reference_refs(&manifest, &paths, &*fs, &mut warnings);

        // Emit warnings for locale ids that don't resolve.
        for locale_id in manifest.locales.keys() {
            if Locale::by_id(locale_id).is_none() {
                warnings.push(ProjectWarning::UnknownLocale {
                    locale: locale_id.clone(),
                });
            }
        }

        // Load glossary if declared.
        let glossary = if let Some(gcfg) = &manifest.glossary {
            let abs = paths.catalog(&gcfg.path); // reuse catalog resolver
            let gpath = root.join(&gcfg.path);
            let gtext = fs
                .read_to_string(&gpath)
                .map_err(|source| ProjectError::Io {
                    path: gpath.clone(),
                    source,
                })?;
            let (g, gwarnings) = Glossary::from_toml(&gtext).map_err(ProjectError::Glossary)?;
            let _ = abs; // path already used for read
            for w in gwarnings {
                warnings.push(ProjectWarning::Glossary(w));
            }
            Some(g)
        } else {
            None
        };

        // Build the correction store (lazy — the file is not opened here).
        let correction_store =
            CorrectionStore::new(paths.corrections().to_path_buf(), Arc::clone(&fs));

        // Load curated.toml if present (empty set if absent or empty file).
        let curated = load_curated(&paths, &*fs, &correction_store)?;

        // Build the review store (lazy — the file is not opened here).
        let review_store = ReviewStore::new(paths.review().to_path_buf(), Arc::clone(&fs));

        // Build the evaluation store (lazy — the file is not opened here).
        let evaluation_store =
            EvaluationStore::new(paths.evaluations().to_path_buf(), Arc::clone(&fs));

        let project = Self {
            root: root.to_path_buf(),
            fs,
            doc,
            manifest,
            glossary,
            paths,
            catalogs,
            references,
            correction_store,
            curated,
            review_store,
            evaluation_store,
            review_map_cache: RefCell::new(None),
        };

        Ok((project, warnings))
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    /// Borrow the validated manifest read view.
    pub fn manifest(&self) -> &ProjectManifest {
        &self.manifest
    }

    /// Borrow the catalog index (no I/O; pre-resolved on open).
    pub fn catalogs(&self) -> &[CatalogRef] {
        &self.catalogs
    }

    /// Look up a catalog by its manifest-relative or absolute path.
    ///
    /// Returns `None` if `path` does not match a registered catalog.
    pub fn catalog(&self, path: &Path) -> Option<&CatalogRef> {
        self.catalogs
            .iter()
            .find(|c| Path::new(&c.manifest_path) == path || Path::new(&c.absolute_path) == path)
    }

    /// Borrow the reference index (no I/O; pre-resolved on open).
    pub fn references(&self) -> &[ReferenceRef] {
        &self.references
    }

    /// All resolved paths under the project.
    pub fn paths(&self) -> &ProjectPaths {
        &self.paths
    }

    /// Borrow the loaded glossary, if any.
    pub fn glossary(&self) -> Option<&Glossary> {
        self.glossary.as_ref()
    }

    /// Resolve a locale id into the merged three-layer view.
    ///
    /// Returns `None` if the workspace does not know the locale id —
    /// `ResolvedLocale` cannot be constructed without a workspace base.
    pub fn locale(&self, id: &str) -> Option<ResolvedLocale> {
        let manifest_cfg = self.manifest.locales.get(id);
        ResolvedLocale::resolve(id, manifest_cfg, self.glossary.as_ref())
    }

    /// Iterate locale ids declared in the manifest in **TOML document order**.
    ///
    /// Document order is preserved across saves. The iteration walks
    /// `doc["locales"]` directly (not the `BTreeMap` in `manifest.locales`,
    /// which is alphabetically sorted).
    pub fn locale_ids(&self) -> impl Iterator<Item = &str> {
        // Walk doc["locales"] key order.
        let locales_item = self.doc.get("locales");
        let keys: Vec<&str> = locales_item
            .and_then(Item::as_table)
            .map(|t| t.iter().map(|(k, _)| k).collect())
            .unwrap_or_default();
        keys.into_iter()
    }

    /// Compact summary safe to send over the Tauri IPC bridge.
    pub fn summary(&self) -> ProjectSummary {
        ProjectSummary {
            root: self.root.display().to_string(),
            name: self.manifest.project.name.clone(),
            schema: self.manifest.project.schema,
            locales: self.locale_ids().map(str::to_owned).collect(),
            catalogs: self.catalogs.clone(),
            references: self.references.clone(),
            glossary_path: self.paths.glossary().map(|p| p.display().to_string()),
            backend: self.manifest.backends.default.clone(),
            state_dir: self.paths.state_dir().display().to_string(),
        }
    }

    // ── Mutation (TOML round-trip preserving) ─────────────────────────────────

    /// Append a new catalog to `[[catalogs]]`.
    ///
    /// Preserves all existing comments and blank lines around the array.
    /// Uses canonical key order: `path`, `format`, `locale`.
    ///
    /// # Errors
    ///
    /// - `DuplicateCatalogPath` if `entry.path` already appears in the
    ///   manifest.
    /// - `CatalogNotFound` if the catalog file does not exist on disk.
    pub fn add_catalog(&mut self, entry: CatalogEntry) -> Result<(), ProjectError> {
        // Duplicate check against existing catalog paths.
        for existing in &self.manifest.catalogs {
            if existing.path == entry.path {
                return Err(ProjectError::DuplicateCatalogPath {
                    path: self.paths.catalog(&entry.path),
                });
            }
        }

        // Existence check.
        let abs = self.paths.catalog(&entry.path);
        if !self.fs.exists(&abs) {
            return Err(ProjectError::CatalogNotFound { path: abs });
        }

        // Build the toml_edit table for the new entry.
        let mut new_table = Table::new();
        new_table.insert(
            "path",
            Item::Value(Value::String(toml_edit::Formatted::new(
                entry.path.display().to_string(),
            ))),
        );
        new_table.insert(
            "format",
            Item::Value(Value::String(toml_edit::Formatted::new(
                catalog_format_str(entry.format).to_owned(),
            ))),
        );
        new_table.insert(
            "locale",
            Item::Value(Value::String(toml_edit::Formatted::new(
                entry.locale.clone(),
            ))),
        );

        // Append to [[catalogs]] array, creating it if absent.
        let doc = &mut self.doc;
        let catalogs_item = doc
            .entry("catalogs")
            .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
        if let Some(aot) = catalogs_item.as_array_of_tables_mut() {
            aot.push(new_table);
        }

        self.sync_manifest()
    }

    /// Remove the catalog whose manifest-relative path matches `path`.
    ///
    /// Idempotent — returns `Ok(false)` if no entry matched, `Ok(true)` if
    /// one was removed.
    pub fn remove_catalog(&mut self, path: &Path) -> Result<bool, ProjectError> {
        let path_str = path.display().to_string();

        let catalogs_item = self.doc.get_mut("catalogs");
        let Some(Item::ArrayOfTables(aot)) = catalogs_item else {
            return Ok(false);
        };

        let before = aot.len();
        // toml_edit ArrayOfTables does not have a retain method; we rebuild by
        // collecting the indices to remove, then removing in reverse order.
        let to_remove: Vec<usize> = aot
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                t.get("path")
                    .and_then(Item::as_str)
                    .map(|p| p == path_str)
                    .unwrap_or(false)
            })
            .map(|(i, _)| i)
            .collect();

        for idx in to_remove.iter().rev() {
            aot.remove(*idx);
        }

        let removed = aot.len() < before;
        if removed {
            self.sync_manifest()?;
        }
        Ok(removed)
    }

    /// Append a new entry to `[[references]]`.
    ///
    /// Preserves all existing comments and blank lines around the array.
    /// Uses canonical key order: `path`, `format`, `locale`.
    ///
    /// # Errors
    ///
    /// - `DuplicateCatalogPath` if `entry.path` already appears in the
    ///   references list.
    /// - `CatalogNotFound` if the reference file does not exist on disk.
    pub fn add_reference(&mut self, entry: ReferenceEntry) -> Result<(), ProjectError> {
        for existing in &self.manifest.references {
            if existing.path == entry.path {
                return Err(ProjectError::DuplicateCatalogPath {
                    path: self.paths.catalog(&entry.path),
                });
            }
        }

        let abs = self.paths.catalog(&entry.path);
        if !self.fs.exists(&abs) {
            return Err(ProjectError::CatalogNotFound { path: abs });
        }

        let mut new_table = Table::new();
        new_table.insert(
            "path",
            Item::Value(Value::String(toml_edit::Formatted::new(
                entry.path.display().to_string(),
            ))),
        );
        new_table.insert(
            "format",
            Item::Value(Value::String(toml_edit::Formatted::new(
                catalog_format_str(entry.format).to_owned(),
            ))),
        );
        new_table.insert(
            "locale",
            Item::Value(Value::String(toml_edit::Formatted::new(
                entry.locale.clone(),
            ))),
        );

        let doc = &mut self.doc;
        let references_item = doc
            .entry("references")
            .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
        if let Some(aot) = references_item.as_array_of_tables_mut() {
            aot.push(new_table);
        }

        self.sync_manifest()
    }

    /// Remove the reference whose manifest-relative path matches `path`.
    ///
    /// Idempotent — returns `Ok(false)` if no entry matched, `Ok(true)` if
    /// one was removed.
    pub fn remove_reference(&mut self, path: &Path) -> Result<bool, ProjectError> {
        let path_str = path.display().to_string();

        let references_item = self.doc.get_mut("references");
        let Some(Item::ArrayOfTables(aot)) = references_item else {
            return Ok(false);
        };

        let before = aot.len();
        let to_remove: Vec<usize> = aot
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                t.get("path")
                    .and_then(Item::as_str)
                    .map(|p| p == path_str)
                    .unwrap_or(false)
            })
            .map(|(i, _)| i)
            .collect();

        for idx in to_remove.iter().rev() {
            aot.remove(*idx);
        }

        let removed = aot.len() < before;
        if removed {
            self.sync_manifest()?;
        }
        Ok(removed)
    }

    /// Upsert `[locales.<id>]`.
    ///
    /// If the locale already exists, sets only the fields present in `config`
    /// and leaves unknown sibling keys untouched (forward-compat). If new,
    /// appends after the last existing `[locales.*]` block.
    pub fn update_locale(&mut self, id: &str, config: LocaleConfig) -> Result<(), ProjectError> {
        // Ensure [locales] table exists.
        let locales = self
            .doc
            .entry("locales")
            .or_insert_with(|| Item::Table(Table::new()));

        if let Some(table) = locales.as_table_mut() {
            let locale_entry = table.entry(id).or_insert_with(|| Item::Table(Table::new()));

            if let Some(locale_table) = locale_entry.as_table_mut() {
                // Set only the fields present in `config`; leave absent fields
                // alone to preserve forward-compat unknown sibling keys.
                if let Some(r) = config.register {
                    locale_table.insert(
                        "register",
                        Item::Value(Value::String(toml_edit::Formatted::new(
                            register_override_str(r).to_owned(),
                        ))),
                    );
                }
                if let Some(v) = &config.variant {
                    locale_table.insert(
                        "variant",
                        Item::Value(Value::String(toml_edit::Formatted::new(v.clone()))),
                    );
                }
                if let Some(ratio) = config.length_warn_ratio {
                    locale_table.insert(
                        "length_warn_ratio",
                        Item::Value(Value::Float(toml_edit::Formatted::new(f64::from(ratio)))),
                    );
                }
            }
        }

        self.sync_manifest()
    }

    /// Remove `[locales.<id>]`.
    ///
    /// Idempotent — returns `Ok(false)` if no such locale block existed.
    pub fn remove_locale(&mut self, id: &str) -> Result<bool, ProjectError> {
        let removed = self
            .doc
            .get_mut("locales")
            .and_then(Item::as_table_mut)
            .map(|t| t.remove(id).is_some())
            .unwrap_or(false);

        if removed {
            self.sync_manifest()?;
        }
        Ok(removed)
    }

    /// Replace every field of `[backend.default]`, creating it if absent.
    /// Preserves unknown sibling keys.
    pub fn set_backend(&mut self, config: BackendConfig) -> Result<(), ProjectError> {
        // Ensure [backend] and [backend.default] exist.
        let backend_item = self
            .doc
            .entry("backend")
            .or_insert_with(|| Item::Table(Table::new()));

        if let Some(backend_table) = backend_item.as_table_mut() {
            let default_item = backend_table
                .entry("default")
                .or_insert_with(|| Item::Table(Table::new()));

            if let Some(t) = default_item.as_table_mut() {
                t.insert(
                    "kind",
                    Item::Value(Value::String(toml_edit::Formatted::new(
                        backend_kind_str(config.kind).to_owned(),
                    ))),
                );
                match config.model {
                    Some(m) => {
                        t.insert(
                            "model",
                            Item::Value(Value::String(toml_edit::Formatted::new(m))),
                        );
                    }
                    None => {
                        t.remove("model");
                    }
                }
                match config.host {
                    Some(h) => {
                        t.insert(
                            "host",
                            Item::Value(Value::String(toml_edit::Formatted::new(h))),
                        );
                    }
                    None => {
                        t.remove("host");
                    }
                }
                match config.num_ctx {
                    Some(n) => {
                        t.insert(
                            "num_ctx",
                            Item::Value(Value::Integer(toml_edit::Formatted::new(i64::from(n)))),
                        );
                    }
                    None => {
                        t.remove("num_ctx");
                    }
                }
            }
        }

        self.sync_manifest()
    }

    /// Replace the `[glossary]` table (creates if absent).
    pub fn set_glossary(&mut self, config: GlossaryConfig) -> Result<(), ProjectError> {
        let item = self
            .doc
            .entry("glossary")
            .or_insert_with(|| Item::Table(Table::new()));

        if let Some(t) = item.as_table_mut() {
            t.insert(
                "path",
                Item::Value(Value::String(toml_edit::Formatted::new(
                    config.path.display().to_string(),
                ))),
            );
        }

        self.sync_manifest()
    }

    /// Replace the `[prompts]` table (creates if absent).
    pub fn set_prompts(&mut self, config: PromptsConfig) -> Result<(), ProjectError> {
        let item = self
            .doc
            .entry("prompts")
            .or_insert_with(|| Item::Table(Table::new()));

        if let Some(t) = item.as_table_mut() {
            t.insert(
                "template_dir",
                Item::Value(Value::String(toml_edit::Formatted::new(
                    config.template_dir.display().to_string(),
                ))),
            );
        }

        self.sync_manifest()
    }

    /// Write the manifest back to `<root>/i18n-harness.toml` atomically.
    ///
    /// Uses `write_atomic` (temp-file + fsync + rename); comments, key order,
    /// and blank lines from the original file survive every prior mutation.
    pub fn save_manifest(&self) -> Result<(), ProjectError> {
        let manifest_path = self.paths.manifest();
        self.fs
            .write_atomic(manifest_path, self.doc.to_string().as_bytes())
            .map_err(|source| ProjectError::Io {
                path: manifest_path.to_path_buf(),
                source,
            })
    }

    // ── Translation memory ────────────────────────────────────────────────────

    /// Borrow the correction store.
    ///
    /// Cheap — the store opens lazily on first append or read. Safe to call
    /// from a hot path.
    pub fn corrections(&self) -> &CorrectionStore {
        &self.correction_store
    }

    /// Append an accepted human edit to `corrections.jsonl`.
    ///
    /// Generates `ts_micros` from `SystemTime::now`, computes the
    /// [`CorrectionId`] via the content hash, builds a [`Correction`] with
    /// `schema = 1`, serialises it as one-line JSON, and appends it.
    ///
    /// # Errors
    ///
    /// - [`ProjectError::Io`] if the append fails.
    pub fn record_correction(&self, c: NewCorrection) -> Result<CorrectionId, ProjectError> {
        let ts_micros = {
            let now = OffsetDateTime::now_utc();
            now.unix_timestamp() * 1_000_000 + i64::from(now.microsecond())
        };

        let id = CorrectionId::from_content_hash(
            &c.catalog,
            &c.unit_id,
            &c.source,
            &c.mt_proposal,
            &c.human_target,
            ts_micros,
        );

        let ts = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| String::from("1970-01-01T00:00:00Z"));

        let correction = Correction {
            schema: 1,
            id: id.clone(),
            ts,
            catalog: c.catalog,
            locale: c.locale,
            unit_id: c.unit_id,
            source: c.source,
            mt_proposal: c.mt_proposal,
            human_target: c.human_target,
            provenance: c.provenance,
            flags_at_correction: c.flags_at_correction,
        };

        self.correction_store.append(&correction)?;
        Ok(id)
    }

    /// List corrections matching `filter`.
    ///
    /// Reads the whole `corrections.jsonl` file (linear scan). Malformed lines
    /// are skipped and their parse errors are silently dropped here; callers
    /// that need the error list can call `self.corrections().read_filtered()`
    /// directly.
    ///
    /// # Errors
    ///
    /// - [`ProjectError::Io`] if the file cannot be read.
    pub fn list_corrections(
        &self,
        filter: CorrectionFilter,
    ) -> Result<Vec<Correction>, ProjectError> {
        let (corrections, _errs) = self.correction_store.read_filtered(&filter)?;
        Ok(corrections)
    }

    /// Promote a correction to the curated set.
    ///
    /// Verifies the id exists in `corrections.jsonl`, then adds a new
    /// `[[example]]` entry to `curated.toml` (round-trip-preserved via
    /// `toml_edit`).
    ///
    /// # Errors
    ///
    /// - [`ProjectError::CorrectionNotFound`] if `id` is not in
    ///   `corrections.jsonl`.
    /// - [`ProjectError::Io`] if the file cannot be written.
    pub fn promote_to_curated(
        &mut self,
        id: CorrectionId,
        note: Option<String>,
    ) -> Result<(), ProjectError> {
        // Verify the id exists.
        let (all, _) = self.correction_store.read_all()?;
        let correction = all
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| ProjectError::CorrectionNotFound { id: id.clone() })?;

        // Read existing curated.toml for round-trip preservation.
        let existing_text = read_curated_text(&self.paths, &*self.fs);

        // Update in-memory set.
        self.curated.push(CuratedExample {
            id: id.clone(),
            note: note.clone().filter(|s| !s.is_empty()),
            correction: Some(correction),
        });

        // Serialise and persist atomically.
        let new_text = self.curated.to_toml_string(existing_text.as_deref());
        self.fs
            .write_atomic(self.paths.curated(), new_text.as_bytes())
            .map_err(|source| ProjectError::Io {
                path: self.paths.curated().to_path_buf(),
                source,
            })
    }

    /// Remove a correction from the curated set.
    ///
    /// Idempotent — returns `false` if the id was not in the curated set,
    /// `true` if it was removed.
    ///
    /// # Errors
    ///
    /// - [`ProjectError::Io`] if the file cannot be written.
    pub fn un_curate(&mut self, id: &CorrectionId) -> Result<bool, ProjectError> {
        if !self.curated.contains(id) {
            return Ok(false);
        }

        let existing_text = read_curated_text(&self.paths, &*self.fs);
        self.curated.remove(id);

        let new_text = self.curated.to_toml_string(existing_text.as_deref());
        self.fs
            .write_atomic(self.paths.curated(), new_text.as_bytes())
            .map_err(|source| ProjectError::Io {
                path: self.paths.curated().to_path_buf(),
                source,
            })?;

        Ok(true)
    }

    /// Borrow the in-memory curated set.
    pub fn curated(&self) -> &CuratedSet {
        &self.curated
    }

    // ── Review status ─────────────────────────────────────────────────────────

    /// Borrow the project's review store.
    ///
    /// Cheap — the store opens lazily on first append or read.
    pub fn reviews(&self) -> &ReviewStore {
        &self.review_store
    }

    /// Borrow the project's evaluation store.
    ///
    /// Cheap — the store opens lazily on first append or read.
    pub fn evaluations(&self) -> &EvaluationStore {
        &self.evaluation_store
    }

    /// Borrow the project's filesystem abstraction.
    ///
    /// Crate-internal; not part of the public API. Used by `tuning_bundle`
    /// to write bundle files without duplicating the I/O error-mapping pattern.
    pub(crate) fn fs(&self) -> &dyn crate::fs::ProjectFs {
        &*self.fs
    }

    /// Record a review-status change.
    ///
    /// Generates `ts` from `SystemTime::now`, builds a [`ReviewEvent`] with
    /// `schema = 1`, serialises it as one-line JSON, and appends it to
    /// `<state_dir>/review.jsonl`. Invalidates the cached fold so the next
    /// `review_map` / `apply_review_state` call sees the new value.
    ///
    /// `status = None` is a deliberate "clear this unit's record" event — the
    /// fold removes the entry from the in-memory map.
    ///
    /// # Errors
    ///
    /// - [`ProjectError::Io`] if the append fails.
    /// - [`ProjectError::UnknownCatalog`] if `catalog` is not in
    ///   `self.catalogs()`.
    pub fn set_review_status(
        &self,
        catalog: &Path,
        unit_id: &UnitId,
        status: Option<ReviewStatus>,
        source_hash_at_review: String,
        reviewer_note: Option<String>,
    ) -> Result<(), ProjectError> {
        // Validate that the catalog is known.
        if self.catalog(catalog).is_none() {
            return Err(ProjectError::UnknownCatalog {
                path: catalog.to_path_buf(),
            });
        }

        let ts = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| String::from("1970-01-01T00:00:00Z"));

        // Determine manifest-relative path for the catalog.
        let catalog_ref = self.catalog(catalog).expect("checked above");
        let catalog_manifest = PathBuf::from(&catalog_ref.manifest_path);

        let event = ReviewEvent {
            schema: 1,
            ts,
            catalog: catalog_manifest,
            unit_id: unit_id.clone(),
            status,
            source_hash: source_hash_at_review,
            reviewer_note: reviewer_note.unwrap_or_default(),
        };

        self.review_store.append(&event)?;

        // Invalidate cache so next read re-folds.
        *self.review_map_cache.borrow_mut() = None;

        Ok(())
    }

    /// Look up the current review status for one unit.
    ///
    /// Returns `None` if the unit has no record in the review store.
    pub fn review_status_of(&self, catalog: &Path, unit_id: &UnitId) -> Option<ReviewRecord> {
        let map = self.review_map();
        // Try manifest-relative path first, then absolute.
        let by_manifest = self
            .catalog(catalog)
            .map(|r| PathBuf::from(&r.manifest_path))
            .and_then(|mp| map.get(&(mp, unit_id.clone())));
        if let Some(r) = by_manifest {
            return Some(r.clone());
        }
        // Fallback: direct key lookup if caller passed manifest-relative directly.
        map.get(&(catalog.to_path_buf(), unit_id.clone())).cloned()
    }

    /// Return the folded review map: every `(catalog, unit_id)` with a current
    /// record.
    ///
    /// Cached; cleared after each `set_review_status` call. The map key uses
    /// manifest-relative catalog paths.
    ///
    /// # Panics
    ///
    /// Panics if the underlying JSONL file is unreadable (I/O error). This
    /// matches the `CuratedSet` precedent; callers that need explicit error
    /// handling can call `self.reviews().read_folded()` directly.
    pub fn review_map(&self) -> std::cell::Ref<'_, crate::review::ReviewMap> {
        // Populate cache if empty.
        {
            let cache = self.review_map_cache.borrow();
            if cache.is_some() {
                drop(cache);
                return std::cell::Ref::map(self.review_map_cache.borrow(), |c| {
                    c.as_ref().expect("just checked")
                });
            }
        }
        // Cache is None; fold the file.
        let (map, _errs) = self
            .review_store
            .read_folded()
            .expect("review.jsonl read failed");
        *self.review_map_cache.borrow_mut() = Some(map);
        std::cell::Ref::map(self.review_map_cache.borrow(), |c| {
            c.as_ref().expect("just populated")
        })
    }

    /// Populate `unit.review_status` and `unit.source_changed_since_review`
    /// for every unit in `units` based on the current fold and each unit's
    /// `source_hash`.
    ///
    /// This is the load-bearing helper for the Tauri layer — every
    /// catalog-open call site routes the extracted units through it before
    /// handing them to the UI.
    ///
    /// The `catalog` path is resolved to a manifest-relative path for the map
    /// lookup. If the catalog is not registered, no units are modified (the map
    /// will simply contain no matching keys).
    pub fn apply_review_state(&self, catalog: &Path, units: &mut [Unit]) {
        let manifest_path = self
            .catalog(catalog)
            .map(|r| PathBuf::from(&r.manifest_path))
            .unwrap_or_else(|| catalog.to_path_buf());

        let map = self.review_map();

        for unit in units.iter_mut() {
            let key = (manifest_path.clone(), unit.id.clone());
            if let Some(record) = map.get(&key) {
                unit.review_status = Some(record.status);
                // Compute source_changed_since_review only when the current
                // extract produced a hash; if source_hash is None (vanished,
                // obsolete, or adapter not yet populating it) we leave the flag
                // false regardless of the stored hash (§6.5 of the design doc).
                if let Some(current) = &unit.source_hash {
                    unit.source_changed_since_review = current != &record.source_hash_at_review;
                }
            }
        }
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    /// Rebuild the typed manifest from `doc` and refresh the catalog index.
    ///
    /// Called after every mutation. Keeps the two views (`doc` and `manifest`)
    /// exactly consistent — the manifest is always a pure function of `doc`.
    fn sync_manifest(&mut self) -> Result<(), ProjectError> {
        let text = self.doc.to_string();
        let (new_manifest, _) = ProjectManifest::from_toml(&text).map_err(|e| match e {
            ProjectError::ManifestParse { source, .. } => ProjectError::ManifestParse {
                path: self.paths.manifest().to_path_buf(),
                source,
            },
            other => other,
        })?;

        self.manifest = new_manifest;

        // Rebuild paths in case glossary or prompts changed.
        self.paths = ProjectPaths::new(
            &self.root,
            &self.manifest.paths,
            self.manifest.glossary.as_ref().map(|g| g.path.as_path()),
            self.manifest.prompts.as_ref(),
        );

        // Rebuild catalog refs from the new manifest (existence is not
        // re-checked on mutation — callers are responsible for that via
        // add_catalog's existence check).
        let mut dummy_warnings = Vec::new();
        self.catalogs =
            build_catalog_refs_no_check(&self.manifest, &self.paths, &mut dummy_warnings);

        // Rebuild reference refs from the new manifest (same no-check policy).
        self.references =
            build_reference_refs_no_check(&self.manifest, &self.paths, &mut dummy_warnings);

        Ok(())
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Load `curated.toml` if it exists, resolve each example against corrections.
///
/// Returns an empty `CuratedSet` if the file does not exist or is empty.
fn load_curated(
    paths: &ProjectPaths,
    fs: &dyn ProjectFs,
    store: &CorrectionStore,
) -> Result<CuratedSet, ProjectError> {
    let curated_path = paths.curated();
    if !fs.exists(curated_path) {
        return Ok(CuratedSet::default());
    }

    let text = fs
        .read_to_string(curated_path)
        .map_err(|source| ProjectError::Io {
            path: curated_path.to_path_buf(),
            source,
        })?;

    if text.trim().is_empty() {
        return Ok(CuratedSet::default());
    }

    let mut curated = CuratedSet::from_toml_str(&text)?;

    // Resolve correction snapshots (best-effort; dangling references stay None).
    if let Ok((corrections, _)) = store.read_all() {
        curated.resolve_corrections(&corrections);
    }

    Ok(curated)
}

/// Read the raw text of `curated.toml` for round-trip preservation, or `None`
/// if the file does not exist.
fn read_curated_text(paths: &ProjectPaths, fs: &dyn ProjectFs) -> Option<String> {
    let path = paths.curated();
    if fs.exists(path) {
        fs.read_to_string(path).ok()
    } else {
        None
    }
}

/// Build the catalog index, checking existence and format on disk (strict open mode).
fn build_catalog_refs(
    manifest: &ProjectManifest,
    paths: &ProjectPaths,
    fs: &dyn ProjectFs,
    warnings: &mut Vec<ProjectWarning>,
) -> Result<Vec<CatalogRef>, ProjectError> {
    use crate::discovery::sniff::{confirms_format, guess_format};

    let mut refs = Vec::with_capacity(manifest.catalogs.len());

    for entry in &manifest.catalogs {
        let abs = paths.catalog(&entry.path);

        if !fs.exists(&abs) {
            return Err(ProjectError::CatalogNotFound { path: abs });
        }

        // Read up to 64 KiB for format sniffing.
        let bytes = fs.read(&abs).map_err(|source| ProjectError::Io {
            path: abs.clone(),
            source,
        })?;
        let prefix = &bytes[..bytes.len().min(64 * 1024)];

        if !confirms_format(prefix, entry.format) {
            let sniffed = guess_format(prefix);
            return Err(ProjectError::CatalogFormatMismatch {
                path: abs,
                declared: entry.format,
                sniffed,
            });
        }

        // Warn if locale id doesn't resolve (but keep the entry).
        if Locale::by_id(&entry.locale).is_none() {
            warnings.push(ProjectWarning::UnknownCatalogLocale {
                path: entry.path.clone(),
                locale: entry.locale.clone(),
            });
        }

        refs.push(CatalogRef {
            absolute_path: abs.display().to_string(),
            manifest_path: entry.path.display().to_string(),
            format: entry.format,
            locale: entry.locale.clone(),
            status: CatalogStatus::Ok,
        });
    }

    Ok(refs)
}

/// Build the catalog index without disk-existence checks (used after mutations).
fn build_catalog_refs_no_check(
    manifest: &ProjectManifest,
    paths: &ProjectPaths,
    _warnings: &mut Vec<ProjectWarning>,
) -> Vec<CatalogRef> {
    manifest
        .catalogs
        .iter()
        .map(|entry| {
            let abs = paths.catalog(&entry.path);
            CatalogRef {
                absolute_path: abs.display().to_string(),
                manifest_path: entry.path.display().to_string(),
                format: entry.format,
                locale: entry.locale.clone(),
                status: CatalogStatus::Ok,
            }
        })
        .collect()
}

/// Build the reference index, validating existence and format on disk.
///
/// Unlike `build_catalog_refs`, a missing or mismatched file emits a
/// [`ProjectWarning`] rather than a hard error — references are read-only
/// supplemental data and a broken path should not prevent the project from
/// opening.
fn build_reference_refs(
    manifest: &ProjectManifest,
    paths: &ProjectPaths,
    fs: &dyn ProjectFs,
    warnings: &mut Vec<ProjectWarning>,
) -> Vec<ReferenceRef> {
    use crate::discovery::sniff::{confirms_format, guess_format};

    let mut refs = Vec::with_capacity(manifest.references.len());

    for entry in &manifest.references {
        let abs = paths.catalog(&entry.path);

        if !fs.exists(&abs) {
            warnings.push(ProjectWarning::ReferenceNotFound {
                path: entry.path.clone(),
            });
            refs.push(ReferenceRef {
                absolute_path: abs.display().to_string(),
                manifest_path: entry.path.display().to_string(),
                format: entry.format,
                locale: entry.locale.clone(),
                status: CatalogStatus::Missing,
            });
            continue;
        }

        let status = match fs.read(&abs) {
            Ok(bytes) => {
                let prefix = &bytes[..bytes.len().min(64 * 1024)];
                if confirms_format(prefix, entry.format) {
                    CatalogStatus::Ok
                } else {
                    let sniffed = guess_format(prefix);
                    warnings.push(ProjectWarning::ReferenceFormatMismatch {
                        path: abs.clone(),
                        declared: entry.format,
                        sniffed,
                    });
                    CatalogStatus::FormatMismatch
                }
            }
            Err(_) => CatalogStatus::Ok, // I/O error on sniff: accept, surface later on read
        };

        refs.push(ReferenceRef {
            absolute_path: abs.display().to_string(),
            manifest_path: entry.path.display().to_string(),
            format: entry.format,
            locale: entry.locale.clone(),
            status,
        });
    }

    refs
}

/// Build the reference index without disk-existence checks (used after mutations).
fn build_reference_refs_no_check(
    manifest: &ProjectManifest,
    paths: &ProjectPaths,
    _warnings: &mut Vec<ProjectWarning>,
) -> Vec<ReferenceRef> {
    manifest
        .references
        .iter()
        .map(|entry| {
            let abs = paths.catalog(&entry.path);
            ReferenceRef {
                absolute_path: abs.display().to_string(),
                manifest_path: entry.path.display().to_string(),
                format: entry.format,
                locale: entry.locale.clone(),
                status: CatalogStatus::Ok,
            }
        })
        .collect()
}

fn catalog_format_str(f: CatalogFormat) -> &'static str {
    match f {
        CatalogFormat::QtTs => "qt-ts",
        CatalogFormat::GettextPo => "gettext-po",
        CatalogFormat::IcuJson => "icu-json",
    }
}

fn backend_kind_str(k: BackendKind) -> &'static str {
    match k {
        BackendKind::Manual => "manual",
        BackendKind::Ollama => "ollama",
        BackendKind::OpenAiCompatible => "open-ai-compatible",
        BackendKind::Agent => "agent",
    }
}

fn register_override_str(r: RegisterOverride) -> &'static str {
    match r {
        RegisterOverride::Formal => "formal",
        RegisterOverride::Informal => "informal",
        RegisterOverride::Neutral => "neutral",
    }
}

// ── Utility re-export so tests can construct ProjectMeta inline ───────────────

#[allow(dead_code)]
pub(crate) fn make_minimal_manifest_toml(name: &str) -> String {
    format!(
        "[project]\nname = \"{name}\"\nschema = {SCHEMA_VERSION}\n",
        name = name
    )
}

/// Construct a fresh `ProjectManifest` from parts (used by test helpers).
#[allow(dead_code)]
pub(crate) fn manifest_from_parts(name: &str) -> ProjectManifest {
    ProjectManifest {
        project: ProjectMeta {
            name: name.to_owned(),
            schema: SCHEMA_VERSION,
        },
        locales: Default::default(),
        catalogs: Default::default(),
        references: Default::default(),
        glossary: None,
        backends: Default::default(),
        prompts: None,
        paths: PathsConfig::default(),
    }
}
