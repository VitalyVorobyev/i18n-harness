//! Process-wide Tauri state types.
//!
//! Defines [`AppState`] (the shared container managed by
//! `tauri::Builder::manage`) plus the two in-memory catalog slots it holds.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use i18n_harness_adapter_qt::Catalog;
use i18n_harness_glossary::Glossary;
use i18n_harness_project::Project;

use crate::backing::BackingCatalog;
use crate::jobs::{ActiveBatches, JobRegistry};

/// Process-wide state shared across Tauri commands.
///
/// One catalog is open at a time — opening a new one replaces the
/// previous, so memory does not grow unbounded across opens. The
/// catalog carries the preserved source bytes needed for byte-stable
/// round-trip on save, so it lives here rather than crossing the IPC
/// bridge on every command.
///
/// The glossary slot is populated by `load_glossary` or as a
/// side effect of `open_project` when the project declares one. Once set,
/// it is threaded into every `translate_unit` call so MT proposals respect
/// project glossary terms.
///
/// The project slot holds the currently-open project. It coexists
/// with the file-centric catalog slot: opening a project doesn't auto-open
/// any catalog, and opening a stand-alone catalog leaves the project slot
/// untouched.
///
/// The `project_catalogs` slot is the multi-catalog dirty store
/// used when working in project mode. It is keyed by absolute path and
/// populated by `open_catalog_in_project`. The two stores — `catalog`
/// (singular, file-centric) and `project_catalogs` (multi, project-scoped)
/// — are independent. Closing a project clears both.
#[derive(Default)]
pub struct AppState {
    pub(crate) catalog: Mutex<Option<OpenCatalog>>,
    pub(crate) glossary: Mutex<Option<Glossary>>,
    pub(crate) project: Mutex<Option<Project>>,
    pub(crate) project_catalogs: Mutex<BTreeMap<PathBuf, OpenCatalogEntry>>,
    /// In-process registry of cancellable background jobs.
    /// Each `translate_batch_in_project` call registers a new entry; the
    /// worker thread deregisters on exit. Per-catalog/per-locale
    /// exclusion is enforced at the command level via `active_batches`.
    pub(crate) jobs: JobRegistry,
    /// Typed concurrency tracker for bulk translate and evaluation workers.
    /// Replaces the old `Mutex<BTreeSet<(PathBuf, String)>>` and
    /// the `("__eval__", "__eval__")` magic key. See [`ActiveBatches`] for
    /// the claim/release contract.
    pub(crate) active_batches: ActiveBatches,
}

/// An entry in the project-scoped multi-catalog store.
///
/// The `catalog` is the format-erased [`BackingCatalog`] enum so the store
/// can hold Qt and PO (and eventually ICU-JSON) catalogs uniformly. Every
/// command that reads units, finds a unit by id, or saves back to disk
/// goes through the enum's delegating helpers.
pub(crate) struct OpenCatalogEntry {
    pub(crate) catalog: BackingCatalog,
    pub(crate) dirty: bool,
}

/// The currently-open catalog plus the absolute path it was loaded
/// from. The path is the frontend's handle.
pub(crate) struct OpenCatalog {
    pub(crate) path: PathBuf,
    pub(crate) catalog: Catalog,
}
