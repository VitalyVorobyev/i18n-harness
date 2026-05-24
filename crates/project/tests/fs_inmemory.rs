//! Parity tests for `InMemoryFs`.
//!
//! Verifies: write-then-read returns same bytes, `exists`/`is_file`/`is_dir`
//! consistency, `write_atomic` overwrites rather than appends, `append`
//! creates-if-absent and appends-if-present, `list_dir` returns sorted
//! results, `create_dir_all` is idempotent.

use std::path::Path;

use i18n_harness_project::{InMemoryFs, ProjectFs};

fn fs() -> InMemoryFs {
    InMemoryFs::new()
}

// ── write_atomic then read ────────────────────────────────────────────────────

#[test]
fn write_then_read_returns_same_bytes() {
    let fs = fs();
    let path = Path::new("/tmp/test.txt");
    let data = b"hello world";
    fs.write_atomic(path, data).expect("write");
    assert_eq!(fs.read(path).expect("read"), data);
}

#[test]
fn read_to_string_returns_utf8_content() {
    let fs = fs();
    let path = Path::new("/tmp/str.txt");
    fs.write_atomic(path, b"hello \xc3\xbc").expect("write");
    assert_eq!(fs.read_to_string(path).expect("read"), "hello ü");
}

#[test]
fn read_nonexistent_returns_not_found() {
    let fs = fs();
    let err = fs.read(Path::new("/nonexistent/file.txt")).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

// ── exists / is_file / is_dir ─────────────────────────────────────────────────

#[test]
fn exists_and_is_file_after_write() {
    let fs = fs();
    let path = Path::new("/tmp/file.txt");
    assert!(!fs.exists(path));
    assert!(!fs.is_file(path));
    fs.write_atomic(path, b"data").expect("write");
    assert!(fs.exists(path));
    assert!(fs.is_file(path));
    assert!(!fs.is_dir(path));
}

#[test]
fn is_dir_after_create_dir_all() {
    let fs = fs();
    let dir = Path::new("/tmp/mydir");
    assert!(!fs.is_dir(dir));
    fs.create_dir_all(dir).expect("mkdir");
    assert!(fs.is_dir(dir));
    assert!(fs.exists(dir));
    assert!(!fs.is_file(dir));
}

#[test]
fn ancestors_are_visible_as_dirs_after_write() {
    let fs = fs();
    // Writing a file should make its parent visible as a directory.
    let path = Path::new("/a/b/c/file.txt");
    fs.write_atomic(path, b"x").expect("write");
    assert!(fs.is_dir(Path::new("/a/b/c")));
    assert!(fs.is_dir(Path::new("/a/b")));
    assert!(fs.is_dir(Path::new("/a")));
}

// ── write_atomic is overwriting, not appending ────────────────────────────────

#[test]
fn write_atomic_overwrites_existing_content() {
    let fs = fs();
    let path = Path::new("/tmp/overwrite.txt");
    fs.write_atomic(path, b"first").expect("write 1");
    fs.write_atomic(path, b"second").expect("write 2");
    assert_eq!(fs.read(path).expect("read"), b"second");
}

// ── append creates-if-absent, appends-if-present ──────────────────────────────

#[test]
fn append_creates_file_if_absent() {
    let fs = fs();
    let path = Path::new("/tmp/log.jsonl");
    assert!(!fs.exists(path));
    fs.append(path, b"line1\n").expect("append");
    assert!(fs.is_file(path));
    assert_eq!(fs.read(path).expect("read"), b"line1\n");
}

#[test]
fn append_adds_to_existing_content() {
    let fs = fs();
    let path = Path::new("/tmp/log.jsonl");
    fs.append(path, b"line1\n").expect("first");
    fs.append(path, b"line2\n").expect("second");
    assert_eq!(fs.read(path).expect("read"), b"line1\nline2\n");
}

#[test]
fn append_after_write_atomic_extends() {
    let fs = fs();
    let path = Path::new("/tmp/mixed.txt");
    fs.write_atomic(path, b"base").expect("write");
    fs.append(path, b"-ext").expect("append");
    assert_eq!(fs.read(path).expect("read"), b"base-ext");
}

// ── list_dir returns sorted entries ──────────────────────────────────────────

#[test]
fn list_dir_returns_sorted_entries() {
    let fs = fs();
    let dir = Path::new("/tmp/dir");
    fs.create_dir_all(dir).expect("mkdir");
    fs.write_atomic(&dir.join("c.txt"), b"c").expect("c");
    fs.write_atomic(&dir.join("a.txt"), b"a").expect("a");
    fs.write_atomic(&dir.join("b.txt"), b"b").expect("b");

    let entries = fs.list_dir(dir).expect("list");
    let names: Vec<_> = entries.iter().map(|p| p.file_name().unwrap()).collect();
    assert_eq!(names, ["a.txt", "b.txt", "c.txt"]);
}

#[test]
fn list_dir_does_not_recurse() {
    let fs = fs();
    let dir = Path::new("/tmp/parent");
    let child = dir.join("child");
    fs.create_dir_all(&child).expect("mkdir child");
    fs.write_atomic(&dir.join("file.txt"), b"x").expect("write");
    fs.write_atomic(&child.join("nested.txt"), b"y")
        .expect("nested");

    let entries = fs.list_dir(dir).expect("list");
    // Should contain `child/` and `file.txt` but not `child/nested.txt`.
    assert_eq!(entries.len(), 2, "entries: {entries:?}");
}

#[test]
fn list_dir_nonexistent_returns_not_found() {
    let fs = fs();
    let err = fs.list_dir(Path::new("/nonexistent")).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

// ── create_dir_all is idempotent ──────────────────────────────────────────────

#[test]
fn create_dir_all_is_idempotent() {
    let fs = fs();
    let dir = Path::new("/tmp/idempotent/nested");
    fs.create_dir_all(dir).expect("first");
    fs.create_dir_all(dir).expect("second — must not error");
    assert!(fs.is_dir(dir));
}

#[test]
fn create_dir_all_creates_intermediate_dirs() {
    let fs = fs();
    let deep = Path::new("/a/b/c/d");
    fs.create_dir_all(deep).expect("mkdir -p");
    assert!(fs.is_dir(deep));
    assert!(fs.is_dir(Path::new("/a/b/c")));
    assert!(fs.is_dir(Path::new("/a/b")));
    assert!(fs.is_dir(Path::new("/a")));
}
