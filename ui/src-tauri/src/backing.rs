//! The in-memory catalog type the desktop shell holds per open file.
//!
//! Every format implements [`FormatDoc`], so the store holds one boxed
//! document regardless of whether the file is Qt, PO, or ICU-JSON.

use std::path::Path;

use i18n_harness_core::Unit;
use i18n_harness_formats::{FormatDoc, FormatRegistry};
use i18n_harness_project::FormatId;

/// One open catalog, any format.
pub(crate) type BackingCatalog = Box<dyn FormatDoc>;

/// Write-back with the shell's `String` error convention.
pub(crate) trait WriteBack {
    /// Render `units` and write them atomically to `out`.
    fn apply(&self, units: &[Unit], out: &Path) -> Result<(), String>;
}

impl WriteBack for BackingCatalog {
    fn apply(&self, units: &[Unit], out: &Path) -> Result<(), String> {
        self.write(units, out)
            .map_err(|e| format!("apply failed: {e}"))
    }
}

/// Open `path` with the manifest-declared `format`.
pub(crate) fn extract_for_format(path: &Path, format: &FormatId) -> Result<BackingCatalog, String> {
    FormatRegistry::builtin()
        .open(path, Some(format.as_str()))
        .map_err(|e| format!("extract failed: {e}"))
}

/// Open `path` with whichever registered format recognizes it.
pub(crate) fn extract_detected(path: &Path) -> Result<BackingCatalog, String> {
    FormatRegistry::builtin()
        .open(path, None)
        .map_err(|e| format!("extract failed: {e}"))
}
