//! React-side adapter — thin dispatcher over the catalog serializers.
//!
//! The React i18n ecosystem (react-intl, format.js, i18next, Lingui) does
//! not define a single catalog file format; it picks one of the
//! file-format families the consuming app prefers. This crate is the public
//! anchor those callers import; it owns no parser of its own and routes by
//! file extension to the actual implementation in
//! [`i18n_harness_catalog`].
//!
//! # Why a separate crate
//!
//! The choice of file format is a property of the consuming application, not
//! of the harness. Putting React-specific dispatch behind a stable crate
//! lets a downstream user write `i18n_harness_adapter_react::extract(path)`
//! without caring which serializer ran underneath. Adding a new React-side
//! format (Lingui's native JSON dialect, etc.) is one match arm here plus
//! one new module in `i18n-harness-catalog`.
//!
//! # Supported formats
//!
//! - `.json` → [`IcuJsonFormat`] (react-intl / format.js / i18next / Lingui
//!   string maps).
//!
//! Future slices may add `.po` here too (some React apps use gettext via
//! `node-gettext` bindings), but the v1 dispatcher keeps a narrow surface.

#![forbid(unsafe_code)]

use std::path::Path;

use i18n_harness_catalog::{Catalog, CatalogError, CatalogFormat, IcuJsonFormat};
use i18n_harness_core::Unit;

/// Open a React-ecosystem catalog file by sniffing its extension.
///
/// # Errors
///
/// - [`CatalogError::UnsupportedFormat`] when the file's extension does not
///   match any format wired up here.
/// - Any error returned by the underlying format's `extract` (I/O,
///   parse, placeholder conversion).
pub fn extract(path: &Path) -> Result<Catalog, CatalogError> {
    let fmt = format_for_path(path)?;
    fmt.extract(path)
}

/// Write `units` back to `path` using the format inferred from the path's
/// extension. The `catalog` argument carries the original source bytes and
/// per-format edit state required for byte-stable round-trip.
///
/// # Errors
///
/// - [`CatalogError::UnsupportedFormat`] when the file's extension does not
///   match any format wired up here.
/// - Any error returned by the underlying format's `apply` (I/O, splice,
///   placeholder conversion).
pub fn apply(catalog: &Catalog, units: &[Unit], path: &Path) -> Result<(), CatalogError> {
    let fmt = format_for_path(path)?;
    fmt.apply(catalog, units, path)
}

/// Resolve a path to a [`CatalogFormat`] implementation by file extension.
///
/// Exposed so the Tauri layer (and any other dispatch site) can verify the
/// extension matches a known format before extracting. Returns a boxed
/// trait object so the caller does not need to know the concrete type.
///
/// # Errors
///
/// - [`CatalogError::UnsupportedFormat`] when the extension does not match
///   any format in this crate's dispatch table.
pub fn format_for_path(path: &Path) -> Result<Box<dyn CatalogFormat>, CatalogError> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "json" => Ok(Box::new(IcuJsonFormat)),
        other => Err(CatalogError::UnsupportedFormat(format!(
            "adapter-react does not handle `.{other}` files (path: {})",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn json_extension_routes_to_icu_json() {
        let fmt = format_for_path(Path::new("messages.json")).expect("json");
        assert_eq!(fmt.id(), "icu-json");
    }

    #[test]
    fn unknown_extension_is_rejected() {
        let err = format_for_path(Path::new("messages.po")).unwrap_err();
        assert!(matches!(err, CatalogError::UnsupportedFormat(_)));
    }

    #[test]
    fn extract_then_apply_with_no_changes_is_byte_identical() {
        // End-to-end smoke test through the dispatcher; the
        // format-specific round-trip suite lives in
        // `crates/catalog/tests/icu_json_roundtrip.rs`.
        let fixture = workspace_root()
            .join("fixtures")
            .join("icu-json")
            .join("flat.json");
        let catalog = extract(&fixture).expect("extract via adapter-react");
        let tmp =
            std::env::temp_dir().join(format!("i18n-harness-react-rt-{}.json", std::process::id()));
        apply(&catalog, catalog.units(), &tmp).expect("apply via adapter-react");
        let rendered = std::fs::read(&tmp).expect("read tmp");
        let original = std::fs::read(&fixture).expect("read fixture");
        std::fs::remove_file(&tmp).ok();
        assert_eq!(rendered, original, "round-trip via adapter-react diverged");
    }

    fn workspace_root() -> PathBuf {
        // adapter-react/Cargo.toml → adapter-react → crates → root
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root")
            .to_path_buf()
    }
}
