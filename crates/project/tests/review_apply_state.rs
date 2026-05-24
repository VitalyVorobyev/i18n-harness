//! Tests for `Project::apply_review_state`.
//!
//! Covers:
//! - Matching hash → `source_changed_since_review = false`.
//! - Mismatching hash → `source_changed_since_review = true`.
//! - `Unit.source_hash = None` → `source_changed_since_review = false`
//!   regardless of stored hash.
//! - `review_status` is populated from the stored record.
//! - Units with no record are left untouched.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_core::{ReviewStatus, Unit, UnitId};
use i18n_harness_project::{InMemoryFs, Project, ProjectFs};

const ROOT: &str = "/proj";

fn root() -> &'static Path {
    Path::new(ROOT)
}

const MANIFEST: &str = r#"[project]
name = "apply-review-test"
schema = 1

[[catalogs]]
path = "translations/app_de.ts"
format = "qt-ts"
locale = "de_DE"
"#;

fn open_project() -> (Project, Arc<InMemoryFs>) {
    let raw = Arc::new(InMemoryFs::new());
    let root_path = PathBuf::from(ROOT);
    raw.write_atomic(&root_path.join("i18n-harness.toml"), MANIFEST.as_bytes())
        .unwrap();
    raw.create_dir_all(&root_path.join("translations")).unwrap();
    // Stub catalog file (content not parsed by project crate).
    raw.write_atomic(&root_path.join("translations/app_de.ts"), b"<TS></TS>")
        .unwrap();

    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw) as Arc<dyn ProjectFs>;
    let (project, _) = Project::open_with_fs(root(), fs).expect("open");
    (project, raw)
}

fn make_unit(id: &str, source_hash: Option<&str>) -> Unit {
    let mut u = Unit::untranslated_singular(id, "Source text");
    u.source_hash = source_hash.map(str::to_owned);
    u
}

#[test]
fn matching_hash_clears_changed_flag() {
    let (project, _) = open_project();

    let hash = "sha256:aabbcc001122";
    project
        .set_review_status(
            Path::new("translations/app_de.ts"),
            &UnitId::from("Ctx::Hello"),
            Some(ReviewStatus::Approved),
            hash.to_owned(),
            None,
        )
        .expect("set_review_status");

    let mut units = vec![make_unit("Ctx::Hello", Some(hash))];
    project.apply_review_state(Path::new("translations/app_de.ts"), &mut units);

    assert_eq!(units[0].review_status, Some(ReviewStatus::Approved));
    assert!(
        !units[0].source_changed_since_review,
        "hashes match → flag should be false"
    );
}

#[test]
fn mismatching_hash_sets_changed_flag() {
    let (project, _) = open_project();

    project
        .set_review_status(
            Path::new("translations/app_de.ts"),
            &UnitId::from("Ctx::Hello"),
            Some(ReviewStatus::Approved),
            "sha256:old000000000".to_owned(),
            None,
        )
        .expect("set_review_status");

    let mut units = vec![make_unit("Ctx::Hello", Some("sha256:new000000000"))];
    project.apply_review_state(Path::new("translations/app_de.ts"), &mut units);

    assert_eq!(units[0].review_status, Some(ReviewStatus::Approved));
    assert!(
        units[0].source_changed_since_review,
        "hashes differ → flag should be true"
    );
}

#[test]
fn none_source_hash_never_sets_changed_flag() {
    let (project, _) = open_project();

    // Store a review with a hash.
    project
        .set_review_status(
            Path::new("translations/app_de.ts"),
            &UnitId::from("Ctx::Hello"),
            Some(ReviewStatus::Approved),
            "sha256:some000000000".to_owned(),
            None,
        )
        .expect("set_review_status");

    // Unit has no source_hash (e.g. PO adapter in early M4, or vanished unit).
    let mut units = vec![make_unit("Ctx::Hello", None)];
    project.apply_review_state(Path::new("translations/app_de.ts"), &mut units);

    assert_eq!(units[0].review_status, Some(ReviewStatus::Approved));
    assert!(
        !units[0].source_changed_since_review,
        "no source_hash → changed flag must stay false"
    );
}

#[test]
fn unit_with_no_record_is_untouched() {
    let (project, _) = open_project();

    let mut units = vec![make_unit("Ctx::Unrecorded", Some("sha256:abc"))];
    project.apply_review_state(Path::new("translations/app_de.ts"), &mut units);

    assert!(
        units[0].review_status.is_none(),
        "no record → review_status must stay None"
    );
    assert!(!units[0].source_changed_since_review);
}

#[test]
fn apply_populates_multiple_units() {
    let (project, _) = open_project();

    project
        .set_review_status(
            Path::new("translations/app_de.ts"),
            &UnitId::from("Ctx::A"),
            Some(ReviewStatus::Reviewed),
            "sha256:aaaa00000000".to_owned(),
            None,
        )
        .expect("set A");
    project
        .set_review_status(
            Path::new("translations/app_de.ts"),
            &UnitId::from("Ctx::B"),
            Some(ReviewStatus::Locked),
            "sha256:bbbb00000000".to_owned(),
            None,
        )
        .expect("set B");

    let mut units = vec![
        make_unit("Ctx::A", Some("sha256:aaaa00000000")),
        make_unit("Ctx::B", Some("sha256:xxxx00000000")), // different hash
        make_unit("Ctx::C", Some("sha256:cccc00000000")), // no record
    ];
    project.apply_review_state(Path::new("translations/app_de.ts"), &mut units);

    assert_eq!(units[0].review_status, Some(ReviewStatus::Reviewed));
    assert!(!units[0].source_changed_since_review);

    assert_eq!(units[1].review_status, Some(ReviewStatus::Locked));
    assert!(units[1].source_changed_since_review);

    assert!(units[2].review_status.is_none());
    assert!(!units[2].source_changed_since_review);
}
