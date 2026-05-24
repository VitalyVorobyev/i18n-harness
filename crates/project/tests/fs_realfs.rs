//! Parity tests for `RealFs` using `tempfile`.
//!
//! Mirrors the `fs_inmemory` tests to verify that `RealFs` satisfies the
//! same `ProjectFs` contract. Every test creates its own `TempDir` so
//! nothing leaks between runs.

use std::path::Path;

use i18n_harness_project::{ProjectFs, RealFs};

fn fs() -> RealFs {
    RealFs
}

// ── write_atomic then read ────────────────────────────────────────────────────

#[test]
fn write_then_read_returns_same_bytes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.txt");
    let data = b"hello world";
    fs().write_atomic(&path, data).expect("write");
    assert_eq!(fs().read(&path).expect("read"), data);
}

#[test]
fn read_to_string_returns_content() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("str.txt");
    fs().write_atomic(&path, b"hello").expect("write");
    assert_eq!(fs().read_to_string(&path).expect("read"), "hello");
}

#[test]
fn read_nonexistent_returns_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("nonexistent.txt");
    let err = fs().read(&path).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

// ── exists / is_file / is_dir ─────────────────────────────────────────────────

#[test]
fn exists_and_is_file_after_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("file.txt");
    assert!(!fs().exists(&path));
    fs().write_atomic(&path, b"x").expect("write");
    assert!(fs().exists(&path));
    assert!(fs().is_file(&path));
    assert!(!fs().is_dir(&path));
}

#[test]
fn is_dir_on_tempdir_root() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(fs().is_dir(dir.path()));
    assert!(!fs().is_file(dir.path()));
    assert!(fs().exists(dir.path()));
}

#[test]
fn is_dir_after_create_dir_all() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sub = dir.path().join("sub").join("nested");
    assert!(!fs().is_dir(&sub));
    fs().create_dir_all(&sub).expect("mkdir");
    assert!(fs().is_dir(&sub));
}

// ── write_atomic overwrites ───────────────────────────────────────────────────

#[test]
fn write_atomic_overwrites_existing_content() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("overwrite.txt");
    fs().write_atomic(&path, b"first").expect("write 1");
    fs().write_atomic(&path, b"second").expect("write 2");
    assert_eq!(fs().read(&path).expect("read"), b"second");
}

// ── append ────────────────────────────────────────────────────────────────────

#[test]
fn append_creates_file_if_absent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    assert!(!fs().exists(&path));
    fs().append(&path, b"line1\n").expect("append");
    assert!(fs().is_file(&path));
    assert_eq!(fs().read(&path).expect("read"), b"line1\n");
}

#[test]
fn append_adds_to_existing_content() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    fs().append(&path, b"line1\n").expect("first");
    fs().append(&path, b"line2\n").expect("second");
    assert_eq!(fs().read(&path).expect("read"), b"line1\nline2\n");
}

// ── list_dir ──────────────────────────────────────────────────────────────────

#[test]
fn list_dir_returns_sorted_entries() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    fs().write_atomic(&root.join("c.txt"), b"c").expect("c");
    fs().write_atomic(&root.join("a.txt"), b"a").expect("a");
    fs().write_atomic(&root.join("b.txt"), b"b").expect("b");

    let entries = fs().list_dir(root).expect("list");
    let names: Vec<_> = entries.iter().map(|p| p.file_name().unwrap()).collect();
    assert_eq!(names, ["a.txt", "b.txt", "c.txt"]);
}

#[test]
fn list_dir_does_not_recurse() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let sub = root.join("sub");
    fs().create_dir_all(&sub).expect("mkdir");
    fs().write_atomic(&root.join("file.txt"), b"x")
        .expect("file");
    fs().write_atomic(&sub.join("nested.txt"), b"y")
        .expect("nested");

    let entries = fs().list_dir(root).expect("list");
    // Should contain `sub/` and `file.txt` but not `sub/nested.txt`.
    assert_eq!(entries.len(), 2, "entries: {entries:?}");
}

#[test]
fn list_dir_nonexistent_returns_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("nonexistent_dir");
    let err = fs().list_dir(&path).unwrap_err();
    // std::fs::read_dir returns NotFound for missing directories.
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound, "got: {err}");
}

// ── create_dir_all ────────────────────────────────────────────────────────────

#[test]
fn create_dir_all_is_idempotent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sub = dir.path().join("a").join("b");
    fs().create_dir_all(&sub).expect("first");
    fs().create_dir_all(&sub).expect("second — must not error");
    assert!(fs().is_dir(&sub));
}

#[test]
fn create_dir_all_creates_intermediate_dirs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let deep = dir.path().join("a").join("b").join("c").join("d");
    fs().create_dir_all(&deep).expect("mkdir -p");
    assert!(fs().is_dir(&deep));
    assert!(fs().is_dir(deep.parent().unwrap()));
}

// ── load manifest via RealFs ──────────────────────────────────────────────────

#[test]
fn load_manifest_from_real_file() {
    use i18n_harness_project::ProjectManifest;

    let dir = tempfile::tempdir().expect("tempdir");
    let manifest_path = dir.path().join("i18n-harness.toml");
    let content = r#"
[project]
name = "real-test"
schema = 1
"#;
    fs().write_atomic(&manifest_path, content.as_bytes())
        .expect("write");

    let (m, warnings) = ProjectManifest::load(&manifest_path, &fs()).expect("load");
    assert_eq!(m.project.name, "real-test");
    assert!(warnings.is_empty());
}

#[test]
fn load_manifest_nonexistent_returns_io_error() {
    use i18n_harness_project::{ProjectError, ProjectManifest};

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("missing.toml");
    let err = ProjectManifest::load(Path::new(&path), &fs()).unwrap_err();
    assert!(
        matches!(err, ProjectError::Io { .. }),
        "expected Io error for missing file, got: {err:?}"
    );
}
