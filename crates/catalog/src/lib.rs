//! `CatalogFormat` trait and non-Qt serializers (PO; ICU-JSON).
//!
//! See `docs/initial_design.md` §6 and `CLAUDE.md` invariant #1 — catalog
//! formats are plugins behind a stable extension point. This crate hosts the
//! trait surface and every implementation except Qt (which lives in
//! `adapter-qt` because its XML parser dominates).
//!
//! # What lives here
//!
//! - [`CatalogFormat`] — the trait every non-Qt format implements.
//! - [`Catalog`] — the format-neutral output of `extract`.
//! - [`CatalogError`] / [`PlaceholderError`] — the unified error types.
//! - [`PoFormat`] — the gettext PO implementation.
//! - [`IcuJsonFormat`] — the ICU MessageFormat JSON implementation.
//! - [`open_catalog_by_format`] — small dispatcher that the Tauri layer (and
//!   the CLI) call to read a catalog whose format the manifest already named.
//!
//! # What this crate does NOT do
//!
//! - It does not host the Qt adapter. Qt has its own crate; the dispatcher
//!   here refuses `qt-ts` so callers know to route Qt through `adapter-qt`.
//!   Folding Qt under this trait is a future slice once PO and ICU-JSON have
//!   stabilized the trait shape.
//! - It does not consult locale records or the gate. The trait is a pure
//!   reader/writer; gate validation runs on the units the trait produces,
//!   not inside the trait.

#![forbid(unsafe_code)]

use std::path::Path;

mod catalog;
mod error;
mod format;
pub mod icu_json;
pub mod po;

pub use catalog::Catalog;
pub use error::{CatalogError, PlaceholderError};
pub use format::CatalogFormat;
pub use icu_json::IcuJsonFormat;
pub use po::{PoFormat, reconcile_plural_arity};

/// Dispatch table: format-id → boxed implementation.
///
/// Kept tiny (a `match` arm) so adding a new format means one new line.
/// The caller passes the manifest-declared format string; the dispatcher
/// returns the corresponding implementation OR
/// [`CatalogError::UnsupportedFormat`] for `qt-ts` (which lives elsewhere)
/// and for unknown ids.
///
/// # Errors
///
/// - [`CatalogError::UnsupportedFormat`] when `format` is `"qt-ts"` (route
///   to `adapter-qt`) or an unknown id.
pub fn format_by_id(format: &str) -> Result<Box<dyn CatalogFormat>, CatalogError> {
    match format {
        "gettext-po" => Ok(Box::new(PoFormat)),
        "icu-json" => Ok(Box::new(IcuJsonFormat)),
        "qt-ts" => Err(CatalogError::UnsupportedFormat(
            "qt-ts is handled by the adapter-qt crate, not by i18n-harness-catalog".to_owned(),
        )),
        other => Err(CatalogError::UnsupportedFormat(other.to_owned())),
    }
}

/// Open a catalog file using the format named by the manifest.
///
/// Convenience over [`format_by_id`] followed by
/// [`CatalogFormat::extract`]. The Tauri layer uses this in
/// `open_catalog_in_project` to dispatch by format string.
///
/// # Errors
///
/// - [`CatalogError::UnsupportedFormat`] when `format` is `"qt-ts"` or
///   unknown.
/// - Any error returned by the underlying format's `extract`.
pub fn open_catalog_by_format(path: &Path, format: &str) -> Result<Catalog, CatalogError> {
    format_by_id(format)?.extract(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_by_id_routes_po() {
        let fmt = format_by_id("gettext-po").expect("po");
        assert_eq!(fmt.id(), "gettext-po");
        assert_eq!(fmt.extensions(), &["po", "pot"]);
    }

    #[test]
    fn format_by_id_routes_icu_json() {
        let fmt = format_by_id("icu-json").expect("icu-json");
        assert_eq!(fmt.id(), "icu-json");
        assert_eq!(fmt.extensions(), &["json"]);
    }

    #[test]
    fn format_by_id_refuses_qt_ts() {
        let err = format_by_id("qt-ts").unwrap_err();
        assert!(matches!(err, CatalogError::UnsupportedFormat(_)));
    }

    #[test]
    fn format_by_id_refuses_unknown() {
        let err = format_by_id("xliff").unwrap_err();
        assert!(matches!(err, CatalogError::UnsupportedFormat(s) if s == "xliff"));
    }
}
