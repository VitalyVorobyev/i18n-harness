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
use crate::format::{CatalogFormat, FormatDoc, SniffHit};

mod parse;
pub(crate) mod placeholder;
mod sniff;
pub(crate) mod write;

pub(crate) use parse::ExtractStateIcuJson;

/// The ICU-JSON format plugin.
#[derive(Debug, Clone, Copy, Default)]
pub struct IcuJsonFormat;

impl CatalogFormat for IcuJsonFormat {
    fn id(&self) -> &'static str {
        "icu-json"
    }

    fn display_name(&self) -> &'static str {
        "ICU MessageFormat JSON"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["json"]
    }

    fn sniff(&self, _path: &Path, head: &[u8]) -> Option<SniffHit> {
        sniff::sniff(head)
    }

    fn open(&self, path: &Path) -> Result<Box<dyn FormatDoc>, CatalogError> {
        Ok(Box::new(self.extract(path)?))
    }

    fn placeholders_to_icu(&self, native: &str) -> Result<String, PlaceholderError> {
        placeholder::to_icu(native)
    }

    fn placeholders_from_icu(&self, icu: &str) -> Result<String, PlaceholderError> {
        placeholder::from_icu(icu)
    }
}

/// The shared instance [`FormatDoc::format`] hands out for ICU MessageFormat JSON documents.
pub(crate) static ICU_JSON_FORMAT: IcuJsonFormat = IcuJsonFormat;

impl IcuJsonFormat {
    /// Parse the catalog at `path`. Same as [`CatalogFormat::open`], but
    /// returns the concrete [`Catalog`].
    ///
    /// # Errors
    ///
    /// See [`CatalogFormat::open`].
    pub fn extract(&self, path: &Path) -> Result<Catalog, CatalogError> {
        parse::extract(path)
    }

    /// Render `units` into `catalog`'s bytes and write them atomically to
    /// `out`. Same as [`FormatDoc::write`] on the concrete [`Catalog`].
    ///
    /// # Errors
    ///
    /// See [`FormatDoc::write`].
    pub fn apply(
        &self,
        catalog: &Catalog,
        units: &[i18n_harness_core::Unit],
        out: &Path,
    ) -> Result<(), CatalogError> {
        catalog.write(units, out)
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
