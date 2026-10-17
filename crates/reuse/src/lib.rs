//! Reuse / remainder-split / merge core logic over Qt catalogs.
//!
//! This crate is the deterministic engine behind three translator workflows
//! that move text *between* catalogs without ever calling a model:
//!
//! - **Reuse** ([`reuse_from_references`]) — copy expert translations from one
//!   or more reference catalogs into a base catalog, by exact unit-id match.
//!   Agreement across references decides whether a copy happens; disagreements
//!   surface as conflicts for a human to resolve.
//! - **Split** ([`writable_untranslated_ids`], plus
//!   [`ReuseReport::remaining_ids`]) — identify the leftover untranslated units
//!   so the caller can carve a standalone remainder `.ts` with
//!   `adapter_qt::write_subset`.
//! - **Merge** ([`merge_back`]) — fold a translated remainder back into its
//!   base, with subset and disjointness guards.
//!
//! # Boundaries (what this crate is NOT)
//!
//! It is **pure logic over catalogs and units**. It reads catalogs (via the Qt
//! adapter's `extract`) and returns report structs plus apply-ready unit
//! vectors. It does **not**:
//!
//! - know about project state, manifests, or the review store — those live in
//!   the Tauri layer, which calls this crate and persists its reports;
//! - write catalogs — it returns the extracted [`Catalog`] and the override
//!   `units` so the **caller** runs `adapter_qt::apply` / `write_subset`,
//!   keeping every catalog write behind the adapter (and thus the round-trip
//!   contract);
//! - run a translation backend — moving existing text is deterministic Rust,
//!   per `CLAUDE.md` invariant #2.
//!
//! Conflicts and per-unit provenance live **only** in [`ReuseReport`], never
//! threaded onto `Unit.flags` / `Unit.flag_notes`: those are transient gate
//! output that does not survive a re-extract into the `.ts`. The report is the
//! contract the CLI prints and the Tauri layer persists.
//!
//! # Scope this cut
//!
//! Qt `.ts` only. The reuse/merge *algorithm* is format-agnostic (it works on
//! [`Unit`] and [`UnitId`]), but the catalog read/write is hard-wired to the
//! Qt adapter for now; generalizing to other formats is a later step.
//!
//! [`Catalog`]: i18n_harness_adapter_qt::Catalog
//! [`Unit`]: i18n_harness_core::Unit
//! [`UnitId`]: i18n_harness_core::UnitId

#![forbid(unsafe_code)]

mod error;
mod merge;
mod report;
mod reuse;

pub use error::ReuseError;
pub use merge::{MergeOutcome, merge_back};
pub use report::{
    ConflictCandidate, ConflictText, CopiedDisposition, CopiedUnit, MergeReport, ReferenceConflict,
    ReuseReport,
};
pub use reuse::{ReuseOutcome, reuse_from_references, writable_untranslated_ids};
