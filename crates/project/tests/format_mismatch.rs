//! Tests that `Project::open` returns `CatalogFormatMismatch` when the file
//! content does not match the declared format.

use std::path::PathBuf;
use std::sync::Arc;

use i18n_harness_project::{InMemoryFs, Project, ProjectError, ProjectFs};

const ROOT: &str = "/project";

fn make_fs(manifest_toml: &str, catalog_rel: &str, catalog_bytes: &[u8]) -> Arc<dyn ProjectFs> {
    let fs = Arc::new(InMemoryFs::new());
    let root = PathBuf::from(ROOT);
    fs.write_atomic(&root.join("i18n-harness.toml"), manifest_toml.as_bytes())
        .unwrap();
    if let Some(parent) = root.join(catalog_rel).parent() {
        fs.create_dir_all(parent).unwrap();
    }
    fs.write_atomic(&root.join(catalog_rel), catalog_bytes)
        .unwrap();
    fs as Arc<dyn ProjectFs>
}

fn manifest_with(catalog_rel: &str, format: &str) -> String {
    format!(
        r#"[project]
name = "test"
schema = 1

[[catalogs]]
path = "{catalog_rel}"
format = "{format}"
locale = "de_DE"
"#
    )
}

// ── qt-ts declared but content is JSON ───────────────────────────────────────

#[test]
fn qt_ts_declared_but_json_content_returns_mismatch() {
    let manifest = manifest_with("translations/de.ts", "qt-ts");
    let json_bytes = b"{\"hello\": \"Hallo\"}";
    let fs = make_fs(&manifest, "translations/de.ts", json_bytes);

    let err = Project::open_with_fs(std::path::Path::new(ROOT), fs).unwrap_err();
    assert!(
        matches!(
            err,
            ProjectError::CatalogFormatMismatch {
                declared: i18n_harness_project::CatalogFormat::QtTs,
                ..
            }
        ),
        "expected CatalogFormatMismatch(QtTs), got {err:?}"
    );
}

// ── gettext-po declared but content is Qt XML ─────────────────────────────────

#[test]
fn gettext_po_declared_but_qt_content_returns_mismatch() {
    let manifest = manifest_with("translations/de.po", "gettext-po");
    let qt_bytes = b"<?xml version=\"1.0\"?><TS language=\"de_DE\"></TS>";
    let fs = make_fs(&manifest, "translations/de.po", qt_bytes);

    let err = Project::open_with_fs(std::path::Path::new(ROOT), fs).unwrap_err();
    assert!(
        matches!(
            err,
            ProjectError::CatalogFormatMismatch {
                declared: i18n_harness_project::CatalogFormat::GettextPo,
                ..
            }
        ),
        "expected CatalogFormatMismatch(GettextPo), got {err:?}"
    );
}

// ── icu-json declared but content is PO ──────────────────────────────────────

#[test]
fn icu_json_declared_but_po_content_returns_mismatch() {
    let manifest = manifest_with("translations/de.json", "icu-json");
    let po_bytes =
        b"msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain\\n\"\n\"Language: de_DE\\n\"\n";
    let fs = make_fs(&manifest, "translations/de.json", po_bytes);

    let err = Project::open_with_fs(std::path::Path::new(ROOT), fs).unwrap_err();
    assert!(
        matches!(
            err,
            ProjectError::CatalogFormatMismatch {
                declared: i18n_harness_project::CatalogFormat::IcuJson,
                ..
            }
        ),
        "expected CatalogFormatMismatch(IcuJson), got {err:?}"
    );
}

// ── Correct content passes ────────────────────────────────────────────────────

#[test]
fn correct_format_does_not_return_mismatch() {
    let manifest = manifest_with("translations/de.ts", "qt-ts");
    let qt_bytes = b"<?xml version=\"1.0\"?><TS language=\"de_DE\"></TS>";
    let fs = make_fs(&manifest, "translations/de.ts", qt_bytes);

    let result = Project::open_with_fs(std::path::Path::new(ROOT), fs);
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}
