//! Tests for `ReviewStore::append` and `ReviewStore::read_folded` via
//! `Project::reviews()`.
//!
//! Covers:
//! - append + fold returns the last-written record for a key.
//! - Multiple appends for the same key → last write wins.
//! - `status: None` event removes the key from the folded map.
//! - A malformed JSONL line is skipped and surfaces as `ReviewEventParse`
//!   in the warnings vec (line-recoverable).
//! - `reviewer_note` round-trips.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_core::{ReviewStatus, UnitId};
use i18n_harness_project::{InMemoryFs, Project, ProjectError, ProjectFs, ReviewEvent};

const ROOT: &str = "/proj";

fn root() -> &'static Path {
    Path::new(ROOT)
}

const MANIFEST: &str = r#"[project]
name = "store-basics-test"
schema = 1
"#;

fn open_project() -> (Project, Arc<InMemoryFs>) {
    let raw = Arc::new(InMemoryFs::new());
    raw.write_atomic(
        &PathBuf::from(ROOT).join("i18n-harness.toml"),
        MANIFEST.as_bytes(),
    )
    .unwrap();
    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw) as Arc<dyn ProjectFs>;
    let (project, _) = Project::open_with_fs(root(), fs).expect("open");
    (project, raw)
}

fn event(catalog: &str, unit_id: &str, status: Option<ReviewStatus>, hash: &str) -> ReviewEvent {
    ReviewEvent {
        schema: 1,
        ts: "2026-05-24T12:00:00.000000Z".to_owned(),
        catalog: PathBuf::from(catalog),
        unit_id: UnitId::from(unit_id),
        status,
        source_hash: hash.to_owned(),
        reviewer_note: String::new(),
    }
}

#[test]
fn append_then_read_folded_returns_single_record() {
    let (project, _) = open_project();
    let store = project.reviews();

    let ev = event(
        "translations/app_de.ts",
        "Ctx::Hello",
        Some(ReviewStatus::Approved),
        "sha256:aabbcc001122",
    );
    store.append(&ev).expect("append");

    let (map, errs) = store.read_folded().expect("read_folded");
    assert!(errs.is_empty(), "no parse errors expected");
    assert_eq!(map.len(), 1);

    let key = (
        PathBuf::from("translations/app_de.ts"),
        UnitId::from("Ctx::Hello"),
    );
    let record = map.get(&key).expect("record must be present");
    assert_eq!(record.status, ReviewStatus::Approved);
    assert_eq!(record.source_hash_at_review, "sha256:aabbcc001122");
}

#[test]
fn last_write_wins_for_same_key() {
    let (project, _) = open_project();
    let store = project.reviews();

    store
        .append(&event(
            "a.ts",
            "Ctx::X",
            Some(ReviewStatus::MachineTranslated),
            "sha256:111111111111",
        ))
        .expect("1");
    store
        .append(&event(
            "a.ts",
            "Ctx::X",
            Some(ReviewStatus::Reviewed),
            "sha256:222222222222",
        ))
        .expect("2");
    store
        .append(&event(
            "a.ts",
            "Ctx::X",
            Some(ReviewStatus::Approved),
            "sha256:333333333333",
        ))
        .expect("3");

    let (map, errs) = store.read_folded().expect("read_folded");
    assert!(errs.is_empty());
    assert_eq!(map.len(), 1);

    let key = (PathBuf::from("a.ts"), UnitId::from("Ctx::X"));
    let record = map.get(&key).unwrap();
    assert_eq!(record.status, ReviewStatus::Approved);
    assert_eq!(record.source_hash_at_review, "sha256:333333333333");
}

#[test]
fn status_none_clears_the_entry() {
    let (project, _) = open_project();
    let store = project.reviews();

    store
        .append(&event(
            "a.ts",
            "Ctx::X",
            Some(ReviewStatus::Approved),
            "sha256:abc",
        ))
        .expect("append");
    store
        .append(&event("a.ts", "Ctx::X", None, ""))
        .expect("clear event");

    let (map, errs) = store.read_folded().expect("read_folded");
    assert!(errs.is_empty());
    assert!(map.is_empty(), "clear event should remove the key");
}

#[test]
fn multiple_keys_are_independent() {
    let (project, _) = open_project();
    let store = project.reviews();

    store
        .append(&event(
            "a.ts",
            "Ctx::X",
            Some(ReviewStatus::Approved),
            "sha256:aaa",
        ))
        .expect("a");
    store
        .append(&event(
            "a.ts",
            "Ctx::Y",
            Some(ReviewStatus::Reviewed),
            "sha256:bbb",
        ))
        .expect("b");
    store
        .append(&event(
            "b.ts",
            "Ctx::X",
            Some(ReviewStatus::NeedsReview),
            "sha256:ccc",
        ))
        .expect("c");

    let (map, errs) = store.read_folded().expect("read_folded");
    assert!(errs.is_empty());
    assert_eq!(map.len(), 3);
}

#[test]
fn malformed_line_is_skipped_and_reported() {
    let (project, raw_fs) = open_project();

    // Write a valid event, then a bad line, then another valid event directly
    // to the JSONL file (bypassing the store's append to inject the bad line).
    let ev1 = event("a.ts", "Ctx::X", Some(ReviewStatus::Approved), "sha256:aaa");
    let ev2 = event("a.ts", "Ctx::Y", Some(ReviewStatus::Reviewed), "sha256:bbb");

    let good1 = serde_json::to_string(&ev1).unwrap() + "\n";
    let bad = "not-valid-json{{{!\n";
    let good2 = serde_json::to_string(&ev2).unwrap() + "\n";

    let review_path = project.paths().review().to_path_buf();
    let content = format!("{good1}{bad}{good2}");
    raw_fs
        .write_atomic(&review_path, content.as_bytes())
        .expect("write");

    let store = project.reviews();
    let (map, errs) = store.read_folded().expect("read_folded");

    assert_eq!(map.len(), 2, "two good records");
    assert_eq!(errs.len(), 1, "one malformed line");
    assert!(
        matches!(&errs[0], ProjectError::ReviewEventParse { line_no: 2, .. }),
        "error must point to line 2: {errs:?}"
    );
}

#[test]
fn empty_store_returns_empty_map() {
    let (project, _) = open_project();
    let store = project.reviews();

    let (map, errs) = store.read_folded().expect("read_folded");
    assert!(map.is_empty());
    assert!(errs.is_empty());
}

#[test]
fn reviewer_note_round_trips() {
    let (project, _) = open_project();
    let store = project.reviews();

    let mut ev = event("a.ts", "Ctx::X", Some(ReviewStatus::Approved), "sha256:abc");
    ev.reviewer_note = "Confirmed against v3.2 mockup.".to_owned();
    store.append(&ev).expect("append");

    let (map, _) = store.read_folded().expect("read_folded");
    let key = (PathBuf::from("a.ts"), UnitId::from("Ctx::X"));
    let record = map.get(&key).unwrap();
    assert_eq!(
        record.reviewer_note.as_deref(),
        Some("Confirmed against v3.2 mockup.")
    );
}
