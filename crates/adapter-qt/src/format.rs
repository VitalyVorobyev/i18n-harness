//! [`QtTsFormat`] — Qt Linguist `.ts` behind the shared
//! [`CatalogFormat`] / [`FormatDoc`] extension point.

use std::collections::HashSet;
use std::path::Path;

use i18n_harness_catalog::{CatalogError, CatalogFormat, FormatDoc, PlaceholderError, SniffHit};
use i18n_harness_core::{Unit, UnitId};

use crate::catalog::Catalog;
use crate::error::{ApplyError, ExtractError};

/// The Qt Linguist `.ts` format plugin.
#[derive(Debug, Clone, Copy, Default)]
pub struct QtTsFormat;

/// The shared instance [`FormatDoc::format`] hands out for `.ts` documents.
pub static QT_TS_FORMAT: QtTsFormat = QtTsFormat;

impl CatalogFormat for QtTsFormat {
    fn id(&self) -> &'static str {
        "qt-ts"
    }

    fn display_name(&self) -> &'static str {
        "Qt Linguist"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ts"]
    }

    fn sniff(&self, _path: &Path, head: &[u8]) -> Option<SniffHit> {
        crate::sniff::sniff(head)
    }

    fn open(&self, path: &Path) -> Result<Box<dyn FormatDoc>, CatalogError> {
        Ok(Box::new(
            crate::parse::extract(path).map_err(extract_error)?,
        ))
    }

    fn placeholders_to_icu(&self, native: &str) -> Result<String, PlaceholderError> {
        Ok(crate::placeholder::to_icu(native))
    }

    fn placeholders_from_icu(&self, icu: &str) -> Result<String, PlaceholderError> {
        Ok(crate::placeholder::from_icu(icu))
    }
}

impl FormatDoc for Catalog {
    fn format(&self) -> &'static dyn CatalogFormat {
        &QT_TS_FORMAT
    }

    fn path(&self) -> &Path {
        &self.source_path
    }

    fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    fn units(&self) -> &[Unit] {
        &self.units
    }

    fn units_mut(&mut self) -> &mut [Unit] {
        &mut self.units
    }

    fn source_bytes(&self) -> &[u8] {
        &self.source_bytes
    }

    /// `lupdate` writes every plural message of a file with the target
    /// language's form count, so any plural message is evidence for all.
    fn plural_forms(&self) -> Option<u32> {
        self.edit_points
            .iter()
            .map(|ep| ep.numerus_bodies.len())
            .filter(|&n| n > 0)
            .max()
            .and_then(|n| u32::try_from(n).ok())
    }

    fn render(&self, units: &[Unit]) -> Result<Vec<u8>, CatalogError> {
        crate::write::render(self, units).map_err(|e| apply_error(&self.source_path, e))
    }

    fn render_subset(&self, keep: &HashSet<UnitId>) -> Result<Vec<u8>, CatalogError> {
        Ok(crate::subset::render_subset(self, keep))
    }
}

fn extract_error(err: ExtractError) -> CatalogError {
    match err {
        ExtractError::Io { path, source } => CatalogError::Io {
            op: "read",
            path,
            source,
        },
        ExtractError::Xml { path, source } => CatalogError::Parse {
            path,
            reason: source.to_string(),
        },
        ExtractError::NotTsFile { path } => CatalogError::Parse {
            path,
            reason: "missing <TS> root element".to_owned(),
        },
    }
}

fn apply_error(path: &Path, err: ApplyError) -> CatalogError {
    match err {
        ApplyError::Io { path, source } => CatalogError::Io {
            op: "write",
            path,
            source,
        },
        other => CatalogError::Apply {
            path: path.to_path_buf(),
            reason: other.to_string(),
        },
    }
}
