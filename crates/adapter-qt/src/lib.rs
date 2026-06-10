//! Qt Linguist `.ts` adapter — the reference adapter.
//!
//! Provides three operations:
//!
//! - [`extract`] — read a `.ts` file into a [`Catalog`] of [`Unit`]s plus the
//!   preserved bytes needed for byte-stable round-trip.
//! - [`apply`] — write a (possibly modified) catalog back to disk, preserving
//!   every byte we did not deliberately change.
//! - [`render_subset`] / [`write_subset`] — produce a valid `.ts` containing
//!   only a chosen subset of units, by byte-subtraction. Kept messages come
//!   out byte-identical; emptied contexts are pruned. The inverse discipline
//!   of [`apply`].
//! - [`placeholder`] — the Qt ↔ ICU placeholder normalizer (its own module
//!   with a property-tested round trip).
//!
//! # Round-trip contract
//!
//! For any file in `fixtures/qt/`, `apply(extract(f), &units, out)` with
//! `units` unchanged from `extract(f).units()` produces a file whose bytes
//! are **identical** to the input. This is the round-trip contract; the integration
//! test `roundtrip_byte_identical` in `tests/` is the executable form.
//!
//! # Design choice: original bytes + edit list
//!
//! We do not reserialize the document from a parsed DOM. We retain the
//! source bytes inside [`Catalog::source_bytes`] and identify the
//! *byte ranges* of every editable element (`<translation>`, every
//! `<numerusform>`, and the `type=` attribute on `<translation>`). `apply`
//! walks the original bytes and substitutes those ranges only where the
//! corresponding unit actually changed; non-target states (`vanished`,
//! `obsolete`) are skipped entirely.
//!
//! This costs us:
//!
//! - Two passes per `apply` (one to compute edits, one to splice). On
//!   anything smaller than a 100 MB catalog this is in the noise.
//! - Some duplication of state between the `Unit` view and the byte view.
//!   That duplication is the price of byte fidelity; the alternative — a
//!   round-tripping DOM — would require us to reimplement every Qt
//!   formatting quirk we have not yet seen.
//!
//! It buys us: the round-trip is **provably** safe for everything we did
//! not touch, because we literally do not touch those bytes.
//!
//! # What this crate guarantees
//!
//! - Byte-stable round-trip on every fixture.
//! - `unfinished` → `finished` transition happens only on units the harness
//!   actually filled (target non-empty, state was writable).
//! - `vanished` and `obsolete` units are never modified.
//! - Placeholder normalization round-trips: `from_icu(to_icu(s)) == s` on
//!   the corpus the property test generates.
//!
//! # What this crate does NOT guarantee
//!
//! - That `.ts` files using exotic constructs we have not added a fixture
//!   for (e.g. `<userdata>`, message-level CDATA, BOMs) round-trip — those
//!   will be covered as fixtures land.
//! - That the placeholder converter handles every possible `printf`-style
//!   token (Qt itself does not document an exhaustive list); see
//!   [`placeholder`] module docs for the supported set.

#![forbid(unsafe_code)]

mod catalog;
mod error;
mod parse;
pub mod placeholder;
mod subset;
mod write;

pub use catalog::Catalog;
pub use error::{ApplyError, ExtractError};
pub use parse::extract;
pub use subset::{render_subset, write_subset};
pub use write::{apply, render};

pub use placeholder::{from_icu, to_icu};

// Re-export the core unit type so callers don't need to depend on
// `i18n-harness-core` explicitly to consume the API.
pub use i18n_harness_core::Unit;
