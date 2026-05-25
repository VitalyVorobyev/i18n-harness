//! GNU gettext `.po` / `.pot` [`CatalogFormat`] implementation.
//!
//! # Round-trip contract
//!
//! For every fixture under `fixtures/po/`, `apply(extract(f), units, out)`
//! with `units == extract(f).units()` is byte-identical to the input. See
//! `crates/catalog/tests/po_roundtrip.rs` for the executable form.
//!
//! # Placeholder normalization
//!
//! Source-side gettext placeholders (`%s`, `%d`, `%(name)s`, `%1$s` …) are
//! normalized to ICU `{0}` / `{name}` on extract; the writer recovers the
//! original conversion specifier from the per-unit placeholder table
//! captured during extract. The conversion table and the rejected syntax
//! list live in the (private) `placeholder` submodule.
//!
//! # Plural-form reconciliation
//!
//! PO's `Plural-Forms:` header declares `nplurals=N`. We parse N and compare
//! it against the locale's CLDR arity (when the caller resolves the locale).
//! A mismatch is logged but not corrected — the PO header is the
//! authoritative declaration for the *existing* `msgstr\[N\]` blocks. When
//! the harness writes a new plural unit (creating new `msgstr\[N\]` blocks),
//! it uses the locale's CLDR arity. This rule lives in
//! [`reconcile_plural_arity`].

use std::path::Path;

use tracing::warn;

use crate::catalog::Catalog;
use crate::error::{CatalogError, PlaceholderError};
use crate::format::CatalogFormat;

pub(crate) mod parse;
pub(crate) mod placeholder;
mod write;

pub(crate) use parse::ExtractStatePo;

/// The PO format plugin.
#[derive(Debug, Clone, Copy, Default)]
pub struct PoFormat;

impl CatalogFormat for PoFormat {
    fn id(&self) -> &'static str {
        "gettext-po"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["po", "pot"]
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

/// Compare the PO header's `nplurals=N` declaration with the locale's CLDR
/// arity. Returns the arity the harness should use when WRITING new plural
/// blocks for this catalog.
///
/// # Policy
///
/// - When both are present and equal: trust both; return the agreed value.
/// - When both are present and differ: emit a `tracing::warn!` (the PO
///   header may have been hand-written or generated against an older CLDR
///   release) and return the locale's CLDR arity for new writes. Existing
///   `msgstr[N]` blocks are unchanged; the PO header is not rewritten —
///   the user's choice stays authoritative for what is already on disk.
/// - When only one is present: trust it.
/// - When neither is present: return `None`. The caller (typically the
///   project layer) decides whether to refuse the catalog or default.
///
/// # Trade-off
///
/// Silently rewriting the PO header on mismatch would surprise users who
/// deliberately pinned an older `Plural-Forms:`. Refusing the catalog
/// outright would block translation of every PO with a stale header.
/// Warning + preferring the locale for *new* writes only is the least
/// surprising middle ground: the user keeps their declared arity until they
/// explicitly add a plural form.
pub fn reconcile_plural_arity(po_header: Option<u32>, locale_arity: Option<u32>) -> Option<u32> {
    match (po_header, locale_arity) {
        (Some(po), Some(loc)) if po == loc => Some(po),
        (Some(po), Some(loc)) => {
            warn!(
                po_header = po,
                locale_arity = loc,
                "PO header `nplurals` disagrees with locale CLDR arity; \
                 keeping existing msgstr[N] blocks intact, using locale arity for new writes"
            );
            Some(loc)
        }
        (Some(po), None) => Some(po),
        (None, Some(loc)) => Some(loc),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconcile_returns_agreed_value() {
        assert_eq!(reconcile_plural_arity(Some(2), Some(2)), Some(2));
    }

    #[test]
    fn reconcile_prefers_locale_on_mismatch() {
        assert_eq!(reconcile_plural_arity(Some(2), Some(3)), Some(3));
    }

    #[test]
    fn reconcile_falls_back_to_either_when_one_missing() {
        assert_eq!(reconcile_plural_arity(Some(2), None), Some(2));
        assert_eq!(reconcile_plural_arity(None, Some(3)), Some(3));
        assert_eq!(reconcile_plural_arity(None, None), None);
    }
}
