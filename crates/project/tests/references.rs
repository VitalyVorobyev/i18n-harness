//! Tests for the `[[references]]` manifest table — `Project::references()`,
//! `add_reference`, `remove_reference`, and warning emission on open.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_project::{
    CatalogFormat, CatalogStatus, InMemoryFs, Project, ProjectError, ProjectFs, ProjectWarning,
    ReferenceEntry,
};

const ROOT: &str = "/project";

fn root() -> &'static Path {
    Path::new(ROOT)
}

// ── FS helpers ────────────────────────────────────────────────────────────────

fn base_fs() -> Arc<InMemoryFs> {
    let fs = Arc::new(InMemoryFs::new());
    let root = PathBuf::from(ROOT);
    let manifest = r#"
[project]
name = "ref-test"
schema = 1
"#;
    fs.write_atomic(&root.join("i18n-harness.toml"), manifest.as_bytes())
        .unwrap();
    fs
}

fn fs_with_references() -> Arc<InMemoryFs> {
    let fs = Arc::new(InMemoryFs::new());
    let root = PathBuf::from(ROOT);
    let manifest = r#"
[project]
name = "ref-test"
schema = 1

[[references]]
path = "expert/app_de.ts"
format = "qt-ts"
locale = "de_DE"

[[references]]
path = "expert/app_es.po"
format = "gettext-po"
locale = "es_ES"
"#;
    fs.write_atomic(&root.join("i18n-harness.toml"), manifest.as_bytes())
        .unwrap();
    fs.create_dir_all(&root.join("expert")).unwrap();
    fs.write_atomic(
        &root.join("expert/app_de.ts"),
        b"<?xml version=\"1.0\"?><TS language=\"de_DE\"></TS>",
    )
    .unwrap();
    fs.write_atomic(&root.join("expert/app_es.po"), b"msgid \"\"\nmsgstr \"\"")
        .unwrap();
    fs
}

// ── references() accessor ─────────────────────────────────────────────────────

#[test]
fn references_returns_resolved_absolute_paths() {
    let raw_fs = fs_with_references();
    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (project, warnings) = Project::open_with_fs(root(), fs).expect("open");

    let refs = project.references();
    assert_eq!(refs.len(), 2);

    assert_eq!(refs[0].manifest_path, "expert/app_de.ts");
    assert_eq!(refs[0].absolute_path, format!("{ROOT}/expert/app_de.ts"));
    assert_eq!(refs[0].format, CatalogFormat::QtTs);
    assert_eq!(refs[0].locale, "de_DE");
    assert_eq!(refs[0].status, CatalogStatus::Ok);

    assert_eq!(refs[1].format, CatalogFormat::GettextPo);
    assert_eq!(refs[1].locale, "es_ES");
    assert_eq!(refs[1].status, CatalogStatus::Ok);

    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
}

#[test]
fn no_references_section_yields_empty_slice() {
    let raw_fs = base_fs();
    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (project, _) = Project::open_with_fs(root(), fs).expect("open");
    assert!(project.references().is_empty());
}

// ── Missing reference emits warning, not error ────────────────────────────────

#[test]
fn missing_reference_file_emits_warning_not_error() {
    let raw_fs = Arc::new(InMemoryFs::new());
    let manifest = r#"
[project]
name = "ref-test"
schema = 1

[[references]]
path = "expert/missing.ts"
format = "qt-ts"
locale = "de_DE"
"#;
    raw_fs
        .write_atomic(
            &PathBuf::from(ROOT).join("i18n-harness.toml"),
            manifest.as_bytes(),
        )
        .unwrap();
    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;

    let (project, warnings) = Project::open_with_fs(Path::new(ROOT), fs).expect("open");

    assert!(
        warnings.iter().any(|w| matches!(
            w,
            ProjectWarning::ReferenceNotFound { path } if path == Path::new("expert/missing.ts")
        )),
        "expected ReferenceNotFound warning, got {warnings:?}"
    );

    let refs = project.references();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].status, CatalogStatus::Missing);
}

// ── add_reference ─────────────────────────────────────────────────────────────

#[test]
fn add_reference_then_reopen_yields_entry() {
    let raw_fs = base_fs();
    let rootp = PathBuf::from(ROOT);
    raw_fs
        .write_atomic(
            &rootp.join("expert/app_de.ts"),
            b"<?xml version=\"1.0\"?><TS language=\"de_DE\"></TS>",
        )
        .unwrap();
    raw_fs.create_dir_all(&rootp.join("expert")).unwrap();

    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (mut project, _) = Project::open_with_fs(root(), fs).expect("open");

    project
        .add_reference(ReferenceEntry {
            path: PathBuf::from("expert/app_de.ts"),
            format: CatalogFormat::QtTs,
            locale: "de_DE".to_owned(),
        })
        .expect("add_reference");

    project.save_manifest().expect("save");

    // Re-open and verify the entry persisted.
    let fs2: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (project2, _) = Project::open_with_fs(root(), fs2).expect("reopen");
    let refs = project2.references();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].manifest_path, "expert/app_de.ts");
    assert_eq!(refs[0].format, CatalogFormat::QtTs);
    assert_eq!(refs[0].locale, "de_DE");
}

#[test]
fn add_reference_in_memory_view_updates_immediately() {
    let raw_fs = base_fs();
    let rootp = PathBuf::from(ROOT);
    raw_fs.create_dir_all(&rootp.join("expert")).unwrap();
    raw_fs
        .write_atomic(
            &rootp.join("expert/app_de.ts"),
            b"<?xml version=\"1.0\"?><TS language=\"de_DE\"></TS>",
        )
        .unwrap();

    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (mut project, _) = Project::open_with_fs(root(), fs).expect("open");
    assert!(project.references().is_empty());

    project
        .add_reference(ReferenceEntry {
            path: PathBuf::from("expert/app_de.ts"),
            format: CatalogFormat::QtTs,
            locale: "de_DE".to_owned(),
        })
        .expect("add_reference");

    // In-memory view updated immediately, no save needed.
    assert_eq!(project.references().len(), 1);
}

#[test]
fn add_reference_rejects_duplicate_path() {
    let raw_fs = fs_with_references();
    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (mut project, _) = Project::open_with_fs(root(), fs).expect("open");

    let err = project
        .add_reference(ReferenceEntry {
            path: PathBuf::from("expert/app_de.ts"),
            format: CatalogFormat::QtTs,
            locale: "de_DE".to_owned(),
        })
        .unwrap_err();

    assert!(
        matches!(err, ProjectError::DuplicateCatalogPath { .. }),
        "expected DuplicateCatalogPath, got {err:?}"
    );
}

#[test]
fn add_reference_rejects_missing_file() {
    let raw_fs = base_fs();
    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (mut project, _) = Project::open_with_fs(root(), fs).expect("open");

    let err = project
        .add_reference(ReferenceEntry {
            path: PathBuf::from("expert/nonexistent.ts"),
            format: CatalogFormat::QtTs,
            locale: "de_DE".to_owned(),
        })
        .unwrap_err();

    assert!(
        matches!(err, ProjectError::CatalogNotFound { .. }),
        "expected CatalogNotFound, got {err:?}"
    );
}

// ── remove_reference ──────────────────────────────────────────────────────────

#[test]
fn remove_reference_deletes_entry() {
    let raw_fs = fs_with_references();
    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (mut project, _) = Project::open_with_fs(root(), fs).expect("open");
    assert_eq!(project.references().len(), 2);

    let removed = project
        .remove_reference(Path::new("expert/app_de.ts"))
        .expect("remove_reference");
    assert!(removed, "expected true when entry exists");
    assert_eq!(project.references().len(), 1);
    assert_eq!(project.references()[0].manifest_path, "expert/app_es.po");
}

#[test]
fn remove_reference_returns_false_when_not_found() {
    let raw_fs = fs_with_references();
    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (mut project, _) = Project::open_with_fs(root(), fs).expect("open");

    let removed = project
        .remove_reference(Path::new("expert/nonexistent.ts"))
        .expect("remove_reference");
    assert!(!removed, "expected false when entry absent");
}

#[test]
fn remove_reference_then_save_and_reopen_yields_reduced_set() {
    let raw_fs = fs_with_references();

    {
        let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
        let (mut project, _) = Project::open_with_fs(root(), fs).expect("open");
        project
            .remove_reference(Path::new("expert/app_de.ts"))
            .expect("remove");
        project.save_manifest().expect("save");
    }

    let fs2: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (project2, _) = Project::open_with_fs(root(), fs2).expect("reopen");
    let refs = project2.references();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].manifest_path, "expert/app_es.po");
}

// ── Comment / ordering preservation ──────────────────────────────────────────

#[test]
fn add_reference_preserves_existing_comments() {
    let raw_fs = Arc::new(InMemoryFs::new());
    let rootp = PathBuf::from(ROOT);
    let manifest = "# Top-level comment.\n\n[project]\nname = \"ref-test\"\nschema = 1\n";
    raw_fs
        .write_atomic(&rootp.join("i18n-harness.toml"), manifest.as_bytes())
        .unwrap();
    raw_fs
        .write_atomic(
            &rootp.join("expert/app_de.ts"),
            b"<?xml version=\"1.0\"?><TS language=\"de_DE\"></TS>",
        )
        .unwrap();
    raw_fs.create_dir_all(&rootp.join("expert")).unwrap();

    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (mut project, _) = Project::open_with_fs(root(), fs).expect("open");
    project
        .add_reference(ReferenceEntry {
            path: PathBuf::from("expert/app_de.ts"),
            format: CatalogFormat::QtTs,
            locale: "de_DE".to_owned(),
        })
        .expect("add_reference");
    project.save_manifest().expect("save");

    let saved = raw_fs
        .read_to_string(&PathBuf::from(ROOT).join("i18n-harness.toml"))
        .expect("read");
    assert!(
        saved.contains("# Top-level comment."),
        "top-level comment lost after add_reference"
    );
    assert!(saved.contains("expert/app_de.ts"), "reference path missing");
}
