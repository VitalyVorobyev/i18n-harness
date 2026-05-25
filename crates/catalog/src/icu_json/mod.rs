//! ICU MessageFormat JSON [`CatalogFormat`] implementation.
//!
//! The react-intl / format.js / i18next / Lingui ecosystem convention: one
//! JSON file per locale, top-level object whose leaf string values are ICU
//! MessageFormat. Nested objects are flattened into `dot.joined.paths` for
//! the unit id.
//!
//! # Round-trip contract
//!
//! For every fixture under `fixtures/icu-json/`, `apply(extract(f), units, out)`
//! with `units == extract(f).units()` is byte-identical to the input. See
//! `crates/catalog/tests/icu_json_roundtrip.rs` for the executable form.
//!
//! # Placeholder normalization
//!
//! ICU-JSON already stores ICU MessageFormat strings; the converter is
//! identity-with-validation. Both directions verify that the braces balance
//! (catching the common hand-edit corruption mode) and pass the bytes
//! through unchanged. Full ICU grammar validation lives in the gate.
//!
//! # Single locale per file
//!
//! ICU-JSON catalogs are single-locale by convention — the file name
//! (`en.json`, `de.json`, …) or the manifest entry carries the locale; the
//! file itself has no per-leaf locale tag. The extracted [`Catalog`]'s
//! `language()` is therefore always `None`; the project layer resolves the
//! locale from the manifest's `[[catalogs]]` entry.

use std::path::Path;

use crate::catalog::Catalog;
use crate::error::{CatalogError, PlaceholderError};
use crate::format::CatalogFormat;

mod parse;
mod placeholder;
mod write;

pub(crate) use parse::ExtractStateIcuJson;

/// The ICU-JSON format plugin.
#[derive(Debug, Clone, Copy, Default)]
pub struct IcuJsonFormat;

impl CatalogFormat for IcuJsonFormat {
    fn id(&self) -> &'static str {
        "icu-json"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["json"]
    }

    fn extract(&self, path: &Path) -> Result<Catalog, CatalogError> {
        parse::extract(path)
    }

    fn apply(
        &self,
        catalog: &Catalog,
        units: &[i18n_harness_core::Unit],
        out: &Path,
    ) -> Result<(), CatalogError> {
        write::apply(catalog, units, out)
    }

    fn placeholders_to_icu(&self, native: &str) -> Result<String, PlaceholderError> {
        placeholder::to_icu(native)
    }

    fn placeholders_from_icu(&self, icu: &str) -> Result<String, PlaceholderError> {
        placeholder::from_icu(icu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_and_extensions_match_convention() {
        let f = IcuJsonFormat;
        assert_eq!(f.id(), "icu-json");
        assert_eq!(f.extensions(), &["json"]);
    }

    #[test]
    fn placeholder_round_trip_is_identity() {
        let f = IcuJsonFormat;
        let s = "Hi, {name}! You have {count, plural, one {1 message} other {# messages}}.";
        let icu = f.placeholders_to_icu(s).unwrap();
        let back = f.placeholders_from_icu(&icu).unwrap();
        assert_eq!(back, s);
    }
}
