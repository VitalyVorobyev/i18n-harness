//! Filter tests for `Project::list_corrections`.
//!
//! Appends 10 corrections with mixed locales/catalogs/unit_ids via
//! `Project::record_correction`, then verifies filter combinations.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_core::UnitId;
use i18n_harness_project::{
    CorrectionFilter, CorrectionProvenance, InMemoryFs, NewCorrection, Project, ProjectFs,
};

const ROOT: &str = "/project";

fn root() -> &'static Path {
    Path::new(ROOT)
}

const MINIMAL_MANIFEST: &str = r#"[project]
name = "test-project"
schema = 1
"#;

fn open_project() -> Project {
    let raw_fs = Arc::new(InMemoryFs::new());
    raw_fs
        .write_atomic(
            &PathBuf::from(ROOT).join("i18n-harness.toml"),
            MINIMAL_MANIFEST.as_bytes(),
        )
        .unwrap();
    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (project, _) = Project::open_with_fs(root(), fs).expect("open");
    project
}

/// Populate the store using `record_correction`. We cannot control `ts`
/// directly through the public API, so filter-by-since tests use unit_id
/// and locale as the discriminating dimensions instead.
///
/// Pattern:
/// - 0-4: locale="de_DE", catalog="cat_a.ts"
/// - 5-9: locale="fr_FR", catalog="cat_b.po"
fn populate(project: &Project) {
    for i in 0..10usize {
        let (locale, catalog) = if i < 5 {
            ("de_DE", "cat_a.ts")
        } else {
            ("fr_FR", "cat_b.po")
        };
        let c = NewCorrection {
            catalog: PathBuf::from(catalog),
            locale: locale.to_owned(),
            unit_id: UnitId(format!("unit_{i}")),
            source: "src".to_owned(),
            mt_proposal: "mt".to_owned(),
            human_target: format!("ht_{i}"),
            provenance: CorrectionProvenance::default(),
            flags_at_correction: vec![],
        };
        project.record_correction(c).expect("record");
    }
}

#[test]
fn empty_filter_returns_all() {
    let project = open_project();
    populate(&project);
    let all = project
        .list_corrections(CorrectionFilter::default())
        .unwrap();
    assert_eq!(all.len(), 10);
}

#[test]
fn filter_by_locale_de() {
    let project = open_project();
    populate(&project);
    let filter = CorrectionFilter {
        locale: Some("de_DE".to_owned()),
        ..Default::default()
    };
    let result = project.list_corrections(filter).unwrap();
    assert_eq!(result.len(), 5);
    assert!(result.iter().all(|c| c.locale == "de_DE"));
}

#[test]
fn filter_by_locale_fr() {
    let project = open_project();
    populate(&project);
    let filter = CorrectionFilter {
        locale: Some("fr_FR".to_owned()),
        ..Default::default()
    };
    let result = project.list_corrections(filter).unwrap();
    assert_eq!(result.len(), 5);
    assert!(result.iter().all(|c| c.locale == "fr_FR"));
}

#[test]
fn filter_by_catalog() {
    let project = open_project();
    populate(&project);
    let filter = CorrectionFilter {
        catalog: Some(PathBuf::from("cat_b.po")),
        ..Default::default()
    };
    let result = project.list_corrections(filter).unwrap();
    assert_eq!(result.len(), 5);
    assert!(result.iter().all(|c| c.catalog == Path::new("cat_b.po")));
}

#[test]
fn filter_by_unit_id() {
    let project = open_project();
    populate(&project);
    let filter = CorrectionFilter {
        unit_id: Some(UnitId("unit_3".to_owned())),
        ..Default::default()
    };
    let result = project.list_corrections(filter).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].unit_id.0, "unit_3");
}

#[test]
fn filter_locale_and_catalog_combined() {
    let project = open_project();
    populate(&project);
    let filter = CorrectionFilter {
        locale: Some("de_DE".to_owned()),
        catalog: Some(PathBuf::from("cat_a.ts")),
        ..Default::default()
    };
    let result = project.list_corrections(filter).unwrap();
    assert_eq!(result.len(), 5);
}

#[test]
fn filter_locale_and_catalog_disjoint() {
    let project = open_project();
    populate(&project);
    let filter = CorrectionFilter {
        locale: Some("de_DE".to_owned()),
        catalog: Some(PathBuf::from("cat_b.po")),
        ..Default::default()
    };
    let result = project.list_corrections(filter).unwrap();
    assert!(result.is_empty());
}

#[test]
fn filter_no_match_returns_empty() {
    let project = open_project();
    populate(&project);
    let filter = CorrectionFilter {
        locale: Some("ja_JP".to_owned()),
        ..Default::default()
    };
    let result = project.list_corrections(filter).unwrap();
    assert!(result.is_empty());
}

/// Test the `since` filter with a real timestamp boundary.
///
/// We record two corrections with slightly different human_targets to
/// distinguish them, then capture a timestamp boundary between the first and
/// second write, and verify filtering works.
#[test]
fn filter_since_excludes_earlier_records() {
    let project = open_project();

    // Record the first correction.
    let c1 = NewCorrection {
        catalog: PathBuf::from("cat.ts"),
        locale: "de_DE".to_owned(),
        unit_id: UnitId("u1".to_owned()),
        source: "a".to_owned(),
        mt_proposal: String::new(),
        human_target: "b".to_owned(),
        provenance: CorrectionProvenance::default(),
        flags_at_correction: vec![],
    };
    project.record_correction(c1).unwrap();

    // Read all and capture the ts of the first record.
    let first_ts = {
        let all = project
            .list_corrections(CorrectionFilter::default())
            .unwrap();
        all[0].ts.clone()
    };

    // Record 5 more corrections (all after the first).
    for i in 2..7usize {
        let c = NewCorrection {
            catalog: PathBuf::from("cat.ts"),
            locale: "de_DE".to_owned(),
            unit_id: UnitId(format!("u{i}")),
            source: "a".to_owned(),
            mt_proposal: String::new(),
            human_target: format!("b{i}"),
            provenance: CorrectionProvenance::default(),
            flags_at_correction: vec![],
        };
        project.record_correction(c).unwrap();
    }

    // Filter from just after the first correction's ts.
    // Because ts resolution is microseconds and all subsequent corrections
    // have ts >= first_ts, using `since = first_ts` returns all 6.
    let filter_all = CorrectionFilter {
        since: Some(first_ts.clone()),
        ..Default::default()
    };
    let result_all = project.list_corrections(filter_all).unwrap();
    assert_eq!(result_all.len(), 6, "since = first_ts should include all 6");

    // Filter from a future timestamp — expect empty.
    let filter_future = CorrectionFilter {
        since: Some("2099-01-01T00:00:00.000000Z".to_owned()),
        ..Default::default()
    };
    let result_future = project.list_corrections(filter_future).unwrap();
    assert!(result_future.is_empty(), "future since should return empty");

    // Filter from a past timestamp — expect all.
    let filter_past = CorrectionFilter {
        since: Some("2000-01-01T00:00:00.000000Z".to_owned()),
        ..Default::default()
    };
    let result_past = project.list_corrections(filter_past).unwrap();
    assert_eq!(result_past.len(), 6, "past since should return all");
}
