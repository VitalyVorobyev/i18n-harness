//! Core types for `i18n-harness`: [`Unit`], [`Intermediate`], [`Batch`],
//! [`Flag`], placeholder metadata, and shared error types.
//!
//! These types are the **contract** between every other crate in the workspace.
//! Adapters produce `Unit`s, backends consume and fill them, the gate validates
//! them, and the intermediate JSONL representation is how units flow between
//! processes (export-batch / import-batch). Nothing in this crate names
//! a specific catalog format, locale, or translation engine — those concerns
//! belong to adapters, locale records, and backend implementations
//! respectively (see `docs/initial_design.md` §1).
//!
//! # Stability
//!
//! The on-disk [`Intermediate`] line shape carries a [`SCHEMA_VERSION`] field
//! so future format changes can be migrated rather than silently misread. Any
//! breaking change to the line shape requires bumping that constant *and*
//! providing a migration path; see the `Intermediate` doc comment.
//!
//! # What this crate guarantees
//!
//! - Stable, serializable types for units, placeholders, flags, batches.
//! - Lossless round-trip of an `Intermediate` line to JSON and back (one
//!   property test in this crate covers it).
//! - No I/O, no format parsing, no algorithms. Other crates do that work.
//!
//! # What this crate does NOT guarantee
//!
//! - That a `Unit` produced from a Qt `.ts` file can round-trip back through
//!   the Qt adapter. That guarantee lives in `adapter-qt`, where the catalog
//!   carries enough preserved fragments to do so.
//! - That a target string is well-formed ICU, has the right placeholders, or
//!   satisfies a locale's plural arity. Those are gate concerns.

#![forbid(unsafe_code)]

mod batch;
mod error;
mod flag;
mod intermediate;
mod placeholder;
mod review;
mod unit;

pub use batch::{Batch, BatchKey, DEFAULT_BATCH_SIZE};
pub use error::{CoreError, IntermediateError};
pub use flag::{Flag, FlagSet, FlagSeverity};
pub use intermediate::{Intermediate, IntermediateLine, SCHEMA_VERSION};
pub use placeholder::{IcuForm, Placeholder, PlaceholderKind};
pub use review::ReviewStatus;
pub use unit::{Provenance, Target, Unit, UnitId, UnitState, compute_source_hash};
