//! Tauri-side adapter over the two catalog backing implementations.
//!
//! Background. The translator-facing crate splits catalog handling between
//! `i18n_harness_adapter_qt::Catalog` (the M0 XML round-tripper) and
//! `i18n_harness_catalog::Catalog` (the M4.4 PO / M4.5 ICU-JSON
//! serializer). Their data model is the same — both expose a slice of
//! [`i18n_harness_core::Unit`], both byte-stably round-trip — but their
//! types are distinct because the underlying parsers are unrelated. The
//! Tauri store therefore needs ONE type it can hold per catalog regardless
//! of format.
//!
//! [`BackingCatalog`] is that type. Every store entry holds one variant
//! plus the manifest-declared format so `apply` can dispatch back to the
//! correct serializer.

use std::path::Path;

use i18n_harness_core::{Unit, UnitId};
use i18n_harness_project::CatalogFormat as ProjectFormat;

/// One catalog held by the Tauri store. Either the Qt adapter's
/// XML-round-tripping `Catalog` or the catalog crate's generic
/// (PO or ICU-JSON) format-neutral `Catalog`.
pub(crate) enum BackingCatalog {
    /// Qt `.ts` catalog handled by the `adapter-qt` crate.
    Qt(i18n_harness_adapter_qt::Catalog),
    /// Generic catalog (PO or ICU-JSON) handled by `i18n-harness-catalog`.
    /// The format id is carried alongside so `apply` can dispatch to the
    /// same serializer that produced the extract state.
    Generic {
        /// Manifest-declared format the catalog was opened with.
        format: ProjectFormat,
        /// Parsed catalog state.
        catalog: i18n_harness_catalog::Catalog,
    },
}

impl BackingCatalog {
    /// Borrow the units in document order.
    pub(crate) fn units(&self) -> &[Unit] {
        match self {
            Self::Qt(c) => c.units(),
            Self::Generic { catalog, .. } => catalog.units(),
        }
    }

    /// Mutable slice of units (for review-state folding).
    pub(crate) fn units_mut(&mut self) -> &mut [Unit] {
        match self {
            Self::Qt(c) => c.units_mut(),
            Self::Generic { catalog, .. } => catalog.units_mut(),
        }
    }

    /// Find a unit by id and return a mutable borrow.
    pub(crate) fn find_unit_mut(&mut self, id: &UnitId) -> Option<&mut Unit> {
        match self {
            Self::Qt(c) => c.find_unit_mut(id),
            Self::Generic { catalog, .. } => catalog.find_unit_mut(id),
        }
    }

    /// Language declared in the catalog header, if any.
    pub(crate) fn language(&self) -> Option<&str> {
        match self {
            Self::Qt(c) => c.language(),
            Self::Generic { catalog, .. } => catalog.language(),
        }
    }

    /// Apply the in-memory units back to disk via the appropriate writer.
    ///
    /// For Generic catalogs the format that originally produced the extract
    /// state is reused — dispatching by extension or by some other heuristic
    /// risks mismatching the extract state's enum variant and producing
    /// "extract state is not …" errors at apply time.
    pub(crate) fn apply(&self, units: &[Unit], out: &Path) -> Result<(), String> {
        match self {
            Self::Qt(c) => i18n_harness_adapter_qt::apply(c, units, out)
                .map_err(|e| format!("apply failed: {e}")),
            Self::Generic { format, catalog } => {
                let fmt = format_for(*format)?;
                fmt.apply(catalog, units, out)
                    .map_err(|e| format!("apply failed: {e}"))
            }
        }
    }
}

/// Resolve a `ProjectFormat` (manifest enum) to the matching catalog-crate
/// trait object. Refuses Qt — that path lives in `adapter-qt`.
fn format_for(
    format: ProjectFormat,
) -> Result<Box<dyn i18n_harness_catalog::CatalogFormat>, String> {
    match format {
        ProjectFormat::GettextPo => i18n_harness_catalog::format_by_id("gettext-po"),
        ProjectFormat::IcuJson => i18n_harness_catalog::format_by_id("icu-json"),
        ProjectFormat::QtTs => {
            return Err("internal: qt-ts must not reach the Generic apply path".to_owned());
        }
    }
    .map_err(|e| format!("format dispatch failed: {e}"))
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
                .map(|catalog| BackingCatalog::Generic { format, catalog })
                .map_err(|e| format!("extract failed: {e}"))
        }
        ProjectFormat::IcuJson => i18n_harness_catalog::open_catalog_by_format(path, "icu-json")
            .map(|catalog| BackingCatalog::Generic { format, catalog })
            .map_err(|e| format!("extract failed: {e}")),
    }
}
