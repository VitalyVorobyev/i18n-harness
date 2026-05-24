//! Project manifest, state paths, filesystem abstraction, and translation-memory
//! store for i18n-harness.
//!
//! This crate owns the persistent state of a single translation project:
//!
//! - Reading, validating, and mutating `i18n-harness.toml` with
//!   comment/ordering preserved via `toml_edit`.
//! - The [`ProjectFs`] trait that abstracts filesystem access so the whole
//!   crate is testable against an in-memory backing.
//! - Pure-serde types for the manifest.
//! - Path resolution ([`ProjectPaths`]), locale layering ([`ResolvedLocale`]),
//!   and the live [`Project`] value with its mutation surface.
//! - The correction store (later slices).
//!
//! # What this crate does NOT own
//!
//! - Catalog parsing/serialization (`adapter-qt`, `catalog`).
//! - Backend execution (`backend`).
//! - Gate validation (`gate`).
//! - Tauri command dispatch (`ui/src-tauri`).
//!
//! # Design contract
//!
//! See `docs/m4.1-project-crate-design.md` for the full architecture,
//! extension points, and the slice-by-slice implementation plan.

#![forbid(unsafe_code)]

pub mod discovery;
pub mod error;
pub mod fs;
pub mod locale;
pub mod manifest;
pub mod memory;
pub mod paths;
pub mod project;
pub mod review;

// ── Re-exports ────────────────────────────────────────────────────────────────

pub use error::{ProjectError, ProjectWarning};

pub use fs::{InMemoryFs, ProjectFs, RealFs};

pub use locale::{ResolvedLocale, ResolvedLocaleView};

pub use manifest::{
    BackendBlock, BackendConfig, BackendKind, CatalogEntry, CatalogFormat, GlossaryConfig,
    LocaleConfig, PathsConfig, ProjectManifest, ProjectMeta, PromptsConfig, RegisterOverride,
    SCHEMA_VERSION,
};

// `FormatGuess` is canonically defined in `manifest.rs` and re-exported from
// `discovery` for callers that import it from there. Both paths stay public
// for backward source compatibility.
pub use manifest::FormatGuess;

pub use discovery::{ClassificationConfidence, DraftAlternative, DraftCatalog, DraftManifest};

pub use memory::{
    Correction, CorrectionFilter, CorrectionId, CorrectionProvenance, CorrectionStore,
    CuratedExample, CuratedSet, NewCorrection,
};

pub use paths::ProjectPaths;

pub use project::{CatalogRef, CatalogStatus, Project, ProjectSummary};

pub use review::{ReviewEvent, ReviewRecord, ReviewStore};
