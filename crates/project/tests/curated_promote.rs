//! Tests for `Project::promote_to_curated`, `Project::un_curate`, and
//! `CuratedSet` membership.
//!
//! Also tests the dangling-reference case: promote, delete the underlying
//! correction from the JSONL file, reload, verify `CuratedExample.correction == None`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_core::UnitId;
use i18n_harness_project::{
    CorrectionId, CorrectionProvenance, InMemoryFs, NewCorrection, Project, ProjectFs,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

const ROOT: &str = "/project";

fn root() -> &'static Path {
    Path::new(ROOT)
}

/// Minimal manifest — no catalogs (none needed for correction tests).
const MINIMAL_MANIFEST: &str = r#"[project]
name = "test-project"
schema = 1
"#;

fn open_project() -> (Project, Arc<InMemoryFs>) {
    let raw_fs = Arc::new(InMemoryFs::new());
    raw_fs
        .write_atomic(
            &PathBuf::from(ROOT).join("i18n-harness.toml"),
            MINIMAL_MANIFEST.as_bytes(),
        )
        .unwrap();
    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (project, _) = Project::open_with_fs(root(), fs).expect("open");
    (project, raw_fs)
}

fn new_correction(catalog: &str, unit: &str, src: &str, ht: &str) -> NewCorrection {
    NewCorrection {
        catalog: PathBuf::from(catalog),
        locale: "de_DE".to_owned(),
        unit_id: UnitId(unit.to_owned()),
        source: src.to_owned(),
        mt_proposal: String::new(),
        human_target: ht.to_owned(),
        provenance: CorrectionProvenance::default(),
        flags_at_correction: vec![],
    }
}

// ── promote_to_curated ────────────────────────────────────────────────────────

#[test]
fn promote_adds_to_curated_set() {
    let (mut project, _fs) = open_project();

    let id = project
        .record_correction(new_correction("cat.ts", "unit_1", "Open", "Öffnen"))
        .expect("record");

    project
        .promote_to_curated(id.clone(), Some("Use Öffnen always.".to_owned()))
        .expect("promote");

    assert!(
        project.curated().contains(&id),
        "id should be in curated set"
    );
    assert_eq!(
        project.curated().note(&id),
        Some("Use Öffnen always."),
        "note should be preserved"
    );
    assert_eq!(project.curated().len(), 1);
}

#[test]
fn promote_with_no_note() {
    let (mut project, _fs) = open_project();
    let id = project
        .record_correction(new_correction("cat.ts", "unit_1", "Close", "Schließen"))
        .expect("record");

    project
        .promote_to_curated(id.clone(), None)
        .expect("promote");

    assert!(project.curated().contains(&id));
    assert_eq!(project.curated().note(&id), None);
}

#[test]
fn promote_nonexistent_id_returns_error() {
    let (mut project, _fs) = open_project();
    let fake_id = CorrectionId("corr_000000000000".to_owned());
    let err = project
        .promote_to_curated(fake_id, None)
        .expect_err("should fail for unknown id");

    assert!(
        matches!(
            err,
            i18n_harness_project::ProjectError::CorrectionNotFound { .. }
        ),
        "expected CorrectionNotFound, got: {err}"
    );
}

// ── un_curate ─────────────────────────────────────────────────────────────────

#[test]
fn un_curate_removes_entry_and_returns_true() {
    let (mut project, _fs) = open_project();
    let id = project
        .record_correction(new_correction("cat.ts", "u1", "x", "y"))
        .expect("record");
    project
        .promote_to_curated(id.clone(), None)
        .expect("promote");

    let removed = project.un_curate(&id).expect("un_curate");
    assert!(removed, "un_curate should return true when entry existed");
    assert!(!project.curated().contains(&id));
    assert!(project.curated().is_empty());
}

#[test]
fn un_curate_nonexistent_id_returns_false() {
    let (mut project, _fs) = open_project();
    let fake_id = CorrectionId("corr_000000000000".to_owned());
    let removed = project.un_curate(&fake_id).expect("un_curate");
    assert!(!removed, "un_curate should return false for unknown id");
}

#[test]
fn un_curate_is_idempotent() {
    let (mut project, _fs) = open_project();
    let id = project
        .record_correction(new_correction("cat.ts", "u1", "a", "b"))
        .expect("record");
    project
        .promote_to_curated(id.clone(), None)
        .expect("promote");

    let first = project.un_curate(&id).expect("first un_curate");
    let second = project.un_curate(&id).expect("second un_curate");
    assert!(first, "first un_curate should return true");
    assert!(!second, "second un_curate should return false");
}

// ── Multiple promote/un-curate ────────────────────────────────────────────────

#[test]
fn promote_multiple_then_un_curate_one() {
    let (mut project, _fs) = open_project();

    let id1 = project
        .record_correction(new_correction("cat.ts", "u1", "a", "b"))
        .expect("record 1");
    let id2 = project
        .record_correction(new_correction("cat.ts", "u2", "c", "d"))
        .expect("record 2");

    project
        .promote_to_curated(id1.clone(), Some("Note 1".to_owned()))
        .expect("promote 1");
    project
        .promote_to_curated(id2.clone(), Some("Note 2".to_owned()))
        .expect("promote 2");
    assert_eq!(project.curated().len(), 2);

    project.un_curate(&id1).expect("un_curate 1");
    assert_eq!(project.curated().len(), 1);
    assert!(!project.curated().contains(&id1));
    assert!(project.curated().contains(&id2));
    assert_eq!(project.curated().note(&id2), Some("Note 2"));
}

// ── Dangling reference ────────────────────────────────────────────────────────

#[test]
fn dangling_reference_when_correction_deleted() {
    let (mut project, raw_fs) = open_project();

    let id = project
        .record_correction(new_correction("cat.ts", "u1", "Open", "Öffnen"))
        .expect("record");
    project
        .promote_to_curated(id.clone(), Some("Keep this.".to_owned()))
        .expect("promote");

    // Overwrite corrections.jsonl to remove the correction (simulating deletion
    // or file rotation). We write a completely empty file.
    let corrections_path = project.paths().corrections().to_path_buf();
    raw_fs.write_atomic(&corrections_path, b"").unwrap();

    // Re-open the project — curated.toml still references the id, but the
    // underlying correction no longer exists.
    let fs2: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (project2, _) = Project::open_with_fs(root(), fs2).expect("reopen");

    // The curated set still has the entry (dangling reference is preserved).
    assert!(
        project2.curated().contains(&id),
        "dangling reference should survive reload"
    );

    // But the resolved correction should be None.
    let example = project2
        .curated()
        .examples()
        .find(|e| e.id == id)
        .expect("example should exist");
    assert!(
        example.correction.is_none(),
        "correction should be None for dangling reference"
    );
}
