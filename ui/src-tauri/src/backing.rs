//! Tauri-side adapter over the two catalog backing implementations.
//!
//! Background. The translator-facing crate splits catalog handling between
//! `i18n_harness_adapter_qt::Catalog` (the M0 XML round-tripper) and
//! `i18n_harness_catalog::Catalog` (the M4.4 PO / future ICU-JSON
//! serializer). Their data model is the same — both expose a slice of
//! [`i18n_harness_core::Unit`], both byte-stably round-trip — but their
//! types are distinct because the underlying parsers are unrelated. The
//! Tauri store therefore needs ONE type it can hold per catalog regardless
//! of format.
//!
//! [`BackingCatalog`] is that type. Every store entry holds one variant;
//! every command that needs to read units, find a unit by id, or save back
//! to disk goes through the delegating helpers.
//!
//! Wiring a third format is one more arm on the enum plus pattern matches
//! in `extract` / `apply` — and that's it.

use std::path::Path;

use i18n_harness_core::{Unit, UnitId};
use i18n_harness_project::CatalogFormat as ProjectFormat;

/// One catalog held by the Tauri store. Either the Qt adapter's
/// XML-round-tripping `Catalog` or the catalog crate's generic
/// (currently PO-only; ICU-JSON in M4.5) format-neutral `Catalog`.
pub(crate) enum BackingCatalog {
    /// Qt `.ts` catalog handled by the `adapter-qt` crate.
    Qt(i18n_harness_adapter_qt::Catalog),
    /// Generic catalog (PO today; ICU-JSON later) handled by
    /// `i18n-harness-catalog`.
    Generic(i18n_harness_catalog::Catalog),
}

impl BackingCatalog {
    /// Borrow the units in document order.
    pub(crate) fn units(&self) -> &[Unit] {
        match self {
            Self::Qt(c) => c.units(),
            Self::Generic(c) => c.units(),
        }
    }

    /// Mutable slice of units (for review-state folding).
    pub(crate) fn units_mut(&mut self) -> &mut [Unit] {
        match self {
            Self::Qt(c) => c.units_mut(),
            Self::Generic(c) => c.units_mut(),
        }
    }

    /// Find a unit by id and return a mutable borrow.
    pub(crate) fn find_unit_mut(&mut self, id: &UnitId) -> Option<&mut Unit> {
        match self {
            Self::Qt(c) => c.find_unit_mut(id),
            Self::Generic(c) => c.find_unit_mut(id),
        }
    }

    /// Language declared in the catalog header, if any.
    pub(crate) fn language(&self) -> Option<&str> {
        match self {
            Self::Qt(c) => c.language(),
            Self::Generic(c) => c.language(),
        }
    }

    /// Apply the in-memory units back to disk via the appropriate writer.
    pub(crate) fn apply(&self, units: &[Unit], out: &Path) -> Result<(), String> {
        match self {
            Self::Qt(c) => i18n_harness_adapter_qt::apply(c, units, out)
                .map_err(|e| format!("apply failed: {e}")),
            Self::Generic(c) => {
                use i18n_harness_catalog::{CatalogFormat as _, PoFormat};
                PoFormat
                    .apply(c, units, out)
                    .map_err(|e| format!("apply failed: {e}"))
            }
        }
    }
}

/// Open a catalog by its manifest-declared format.
///
/// Qt goes through `adapter-qt`; everything else goes through the
/// `i18n-harness-catalog` dispatcher. Errors carry the source format name
/// so the UI can show a useful message.
pub(crate) fn extract_for_format(
    path: &Path,
    format: ProjectFormat,
) -> Result<BackingCatalog, String> {
    match format {
        ProjectFormat::QtTs => i18n_harness_adapter_qt::extract(path)
            .map(BackingCatalog::Qt)
            .map_err(|e| format!("extract failed: {e}")),
        ProjectFormat::GettextPo => {
            i18n_harness_catalog::open_catalog_by_format(path, "gettext-po")
                .map(BackingCatalog::Generic)
                .map_err(|e| format!("extract failed: {e}"))
        }
        ProjectFormat::IcuJson => {
            Err("icu-json catalog format is not implemented yet (M4.5)".to_string())
        }
    }
}
