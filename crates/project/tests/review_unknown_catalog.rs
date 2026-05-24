//! Tests for `Project::set_review_status` when the catalog is not registered.
//!
//! Covers:
//! - Passing an unregistered catalog path returns `ProjectError::UnknownCatalog`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_core::{ReviewStatus, UnitId};
use i18n_harness_project::{InMemoryFs, Project, ProjectError, ProjectFs};

const ROOT: &str = "/proj";

fn root() -> &'static Path {
    Path::new(ROOT)
}

const MANIFEST: &str = r#"[project]
name = "unknown-catalog-test"
schema = 1

[[catalogs]]
path = "translations/app_de.ts"
format = "qt-ts"
locale = "de_DE"
"#;

fn open_project() -> Project {
    let raw = Arc::new(InMemoryFs::new());
    let root_path = PathBuf::from(ROOT);
    raw.write_atomic(&root_path.join("i18n-harness.toml"), MANIFEST.as_bytes())
        .unwrap();
    raw.create_dir_all(&root_path.join("translations")).unwrap();
    raw.write_atomic(&root_path.join("translations/app_de.ts"), b"<TS></TS>")
        .unwrap();

    let fs: Arc<dyn ProjectFs> = raw;
    let (project, _) = Project::open_with_fs(root(), fs).expect("open");
    project
}

#[test]
fn unregistered_catalog_returns_unknown_catalog_error() {
    let project = open_project();

    let err = project
        .set_review_status(
            Path::new("translations/not_registered.ts"),
            &UnitId::from("Ctx::X"),
            Some(ReviewStatus::Approved),
            "sha256:abc".to_owned(),
            None,
        )
        .unwrap_err();

    assert!(
        matches!(err, ProjectError::UnknownCatalog { .. }),
        "expected UnknownCatalog, got {err:?}"
    );
}

#[test]
fn registered_catalog_succeeds() {
    let project = open_project();

    // Should not error.
    project
        .set_review_status(
            Path::new("translations/app_de.ts"),
            &UnitId::from("Ctx::Hello"),
            Some(ReviewStatus::Approved),
            "sha256:aabbcc".to_owned(),
            None,
        )
        .expect("registered catalog must succeed");
}

#[test]
fn absolute_path_to_registered_catalog_succeeds() {
    let project = open_project();

    // Absolute path resolution: `catalog()` accepts absolute paths too.
    let abs = PathBuf::from(ROOT).join("translations/app_de.ts");
    project
        .set_review_status(
            &abs,
            &UnitId::from("Ctx::Hello"),
            Some(ReviewStatus::Approved),
            "sha256:aabbcc".to_owned(),
            None,
        )
        .expect("absolute path to registered catalog must succeed");
}
