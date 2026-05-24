//! Project manifest, state paths, filesystem abstraction, and translation-memory
//! store for i18n-harness.
//!
//! This crate owns the persistent state of a single translation project:
//!
//! - Reading, validating, and (in later slices) mutating `i18n-harness.toml`
//!   with comment/ordering preserved via `toml_edit`.
//! - The [`ProjectFs`] trait that abstracts filesystem access so the whole
//!   crate is testable against an in-memory backing.
//! - Pure-serde types for the manifest (this slice).
//! - Path resolution, locale layering, discovery, and the correction store
//!   (later slices — see `docs/m4.1-project-crate-design.md`).
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

pub mod error;
pub mod fs;
pub mod manifest;
pub(crate) mod memory;

// ── Re-exports for M4.1a public surface ──────────────────────────────────────

pub use error::{ProjectError, ProjectWarning};

pub use fs::{InMemoryFs, ProjectFs, RealFs};

pub use manifest::{
    BackendBlock, BackendConfig, BackendKind, CatalogEntry, CatalogFormat, FormatGuess,
    GlossaryConfig, LocaleConfig, PathsConfig, ProjectManifest, ProjectMeta, PromptsConfig,
    RegisterOverride, SCHEMA_VERSION,
};

pub use memory::CorrectionId;
