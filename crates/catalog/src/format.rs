//! [`CatalogFormat`] — the stable extension point that PO, ICU-JSON, and any
//! future non-Qt catalog serializer implements.
//!
//! # Why a trait
//!
//! `CLAUDE.md` invariant #1: "Languages, translation engines, and catalog
//! formats are data or plugins behind stable extension points." The trait is
//! the plugin surface for the third axis. PO and ICU-JSON implement it; Qt
//! has its own crate (`adapter-qt`) because its XML parser dominates and is
//! orthogonal to the line-oriented work the other formats need.
//!
//! # Contract
//!
//! Implementations MUST satisfy the round-trip property:
//!
//! > For every catalog file `f` in the format's fixture corpus,
//! > `apply(extract(f), extract(f).units(), out)` produces bytes identical
//! > to `f` on disk.
//!
//! This is the load-bearing round-trip contract. New fixtures get
//! added to the round-trip suite; they never get carved out.
//!
//! # What this trait does NOT guarantee
//!
//! - That `extract` and `apply` are async-safe. Implementations are
//!   synchronous, blocking I/O; backends that need async should wrap the
//!   calls in `spawn_blocking`.
//! - That placeholder converters handle every possible printf-style or
//!   ICU construct. Each implementation documents its supported subset and
//!   refuses (with [`crate::PlaceholderError::UnsupportedSyntax`]) anything
//!   it cannot round-trip.

use std::path::Path;

use crate::catalog::Catalog;
use crate::error::{CatalogError, PlaceholderError};

/// A catalog format plugin.
///
/// One impl per format. The implementation is normally a unit struct (no
/// state) — all per-file state lives in [`Catalog`].
pub trait CatalogFormat: std::fmt::Debug {
    /// Stable identifier matching the manifest's `[[catalogs]] format =
    /// "..."` field. Used by [`crate::open_catalog_by_format`] to dispatch.
    fn id(&self) -> &'static str;

    /// File extensions the format claims (without the leading dot). Used by
    /// project discovery to suggest a default format for an untyped file.
    fn extensions(&self) -> &'static [&'static str];

    /// Parse the catalog file at `path` into a [`Catalog`].
    ///
    /// The returned catalog's [`Unit::source`](i18n_harness_core::Unit::source)
    /// strings carry **ICU-normalized** placeholders. The format's original
    /// placeholder spelling is preserved inside the catalog's source bytes so
    /// `apply` can write it back unchanged.
    ///
    /// # Errors
    ///
    /// - [`CatalogError::Io`] on filesystem errors.
    /// - [`CatalogError::Parse`] when the file does not satisfy the format's
    ///   grammar (carrying line/column when the parser can extract them).
    /// - [`CatalogError::PlaceholderConversion`] when the file contains a
    ///   placeholder construct the converter cannot represent.
    fn extract(&self, path: &Path) -> Result<Catalog, CatalogError>;

    /// Render `units` back into the catalog format at `out`.
    ///
    /// Implementations splice the new target text into `catalog.source_bytes`
    /// and write to a temporary path before renaming over `out`, so a failure
    /// mid-write leaves the previous file intact.
    ///
    /// The expected invariant: when `units` equals `catalog.units()` (no
    /// edits), the bytes written to `out` are identical to
    /// `catalog.source_bytes()`. The format's fixture round-trip suite is the
    /// executable form of this guarantee.
    ///
    /// # Errors
    ///
    /// - [`CatalogError::Io`] on filesystem errors.
    /// - [`CatalogError::Apply`] when a unit cannot be matched against the
    ///   catalog (its id no longer exists) or the splice would produce
    ///   malformed output.
    /// - [`CatalogError::PlaceholderConversion`] when the inverse converter
    ///   refuses a placeholder the model or translator introduced.
    fn apply(
        &self,
        catalog: &Catalog,
        units: &[i18n_harness_core::Unit],
        out: &Path,
    ) -> Result<(), CatalogError>;

    /// Convert placeholders FROM the format's native syntax TO ICU
    /// MessageFormat.
    ///
    /// Exposed on the trait (in addition to being called internally during
    /// `extract`) so the prompt pipeline can run any user-supplied text
    /// through the same converter the catalog uses.
    fn placeholders_to_icu(&self, native: &str) -> Result<String, PlaceholderError>;

    /// Inverse of [`Self::placeholders_to_icu`]. Used by `apply` and by the
    /// translator UI when showing the format-native preview of an
    /// ICU-form target.
    fn placeholders_from_icu(&self, icu: &str) -> Result<String, PlaceholderError>;
}
