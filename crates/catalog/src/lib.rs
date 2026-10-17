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
pub use format::{CatalogFormat, Confidence, FormatDoc, SniffHit, strip_bom, utf8_prefix};
pub use icu_json::{IcuJsonFormat, LinguiJsonFormat};
pub use po::{LinguiPoFormat, PoFormat, reconcile_plural_arity};

/// The formats this crate implements, in registry order.
///
/// Qt `.ts` is not here: it lives in `adapter-qt`, which implements the same
/// traits. The full registry (every built-in format) is assembled by the
/// `i18n-harness-formats` crate.
pub fn builtin_formats() -> [&'static dyn CatalogFormat; 4] {
    [
        &po::LINGUI_PO_FORMAT,
        &po::PO_FORMAT,
        &icu_json::LINGUI_JSON_FORMAT,
        &icu_json::ICU_JSON_FORMAT,
    ]
}

/// Look up one of this crate's formats by its manifest id.
///
/// # Errors
///
/// [`CatalogError::UnsupportedFormat`] for an id this crate does not
/// implement (including `"qt-ts"`, which lives in `adapter-qt`).
pub fn format_by_id(format: &str) -> Result<&'static dyn CatalogFormat, CatalogError> {
    builtin_formats()
        .into_iter()
        .find(|f| f.id() == format)
        .ok_or_else(|| CatalogError::UnsupportedFormat(format.to_owned()))
}

/// Open a catalog file using one of this crate's formats, by manifest id.
///
/// # Errors
///
/// - [`CatalogError::UnsupportedFormat`] when `format` is not implemented by
///   this crate.
/// - Any error returned by the underlying format's parser.
pub fn open_catalog_by_format(path: &Path, format: &str) -> Result<Catalog, CatalogError> {
    match format_by_id(format)?.id() {
        "gettext-po" => PoFormat.extract(path),
        "lingui-po" => po::parse::extract(path, po::PoDialect::Lingui),
        "lingui-json" => icu_json::parse::extract_lingui(path),
        _ => IcuJsonFormat.extract(path),
    }
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
