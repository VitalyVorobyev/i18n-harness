//! Shared helpers used by multiple command modules.

use std::path::Path;

use i18n_harness_adapter_qt::Catalog;

use crate::backing::BackingCatalog;
use crate::dto::CatalogResponse;

pub(crate) fn build_catalog_response(path: &Path, catalog: &Catalog) -> CatalogResponse {
    CatalogResponse {
        path: path.to_string_lossy().into_owned(),
        unit_count: catalog.units().len(),
        language: catalog.language().map(str::to_owned),
        units: catalog.units().to_vec(),
    }
}

pub(crate) fn build_catalog_response_backing(
    path: &Path,
    catalog: &BackingCatalog,
) -> CatalogResponse {
    CatalogResponse {
        path: path.to_string_lossy().into_owned(),
        unit_count: catalog.units().len(),
        language: catalog.language().map(str::to_owned),
        units: catalog.units().to_vec(),
    }
}
