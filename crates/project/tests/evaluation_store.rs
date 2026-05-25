//! Integration tests for [`EvaluationStore`] accessed via [`Project::evaluations`].
//!
//! Mirrors the style of `review_store_basics.rs`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_project::{EvaluationRun, InMemoryFs, Project, ProjectFs};

const ROOT: &str = "/eval-proj";

fn root() -> &'static Path {
    Path::new(ROOT)
}

const MANIFEST: &str = r#"[project]
name = "eval-store-test"
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

fn make_run(ts: &str, score: f32, count: usize) -> EvaluationRun {
    EvaluationRun {
        schema: 1,
        ts: ts.to_owned(),
        prompt_template_version: "ollama-translate-v2".into(),
        overall_score: score,
        per_locale: BTreeMap::new(),
        per_flag_kind: BTreeMap::new(),
        example_count: count,
    }
}

// ── append + list ─────────────────────────────────────────────────────────────

#[test]
fn empty_store_returns_empty_list() {
    let (project, _) = open_project();
    let store = project.evaluations();
    let list = store.list().expect("list");
    assert!(list.is_empty());
}

#[test]
fn empty_store_returns_none_for_latest() {
    let (project, _) = open_project();
    let store = project.evaluations();
    let latest = store.latest().expect("latest");
    assert!(latest.is_none());
}

#[test]
fn append_and_list_round_trip() {
    let (project, _) = open_project();
    let store = project.evaluations();

    let run = make_run("2026-01-01T00:00:00Z", 0.75, 4);
    store.append(&run).expect("append");

    let list = store.list().expect("list");
    assert_eq!(list.len(), 1);
    assert!((list[0].overall_score - 0.75).abs() < f32::EPSILON);
    assert_eq!(list[0].example_count, 4);
}

#[test]
fn multiple_appends_accumulate_in_order() {
    let (project, _) = open_project();
    let store = project.evaluations();

    store
        .append(&make_run("2026-01-01T00:00:00Z", 0.5, 2))
        .expect("1");
    store
        .append(&make_run("2026-01-02T00:00:00Z", 0.8, 2))
        .expect("2");
    store
        .append(&make_run("2026-01-03T00:00:00Z", 0.9, 2))
        .expect("3");

    let list = store.list().expect("list");
    assert_eq!(list.len(), 3);
    assert!((list[0].overall_score - 0.5).abs() < f32::EPSILON);
    assert!((list[1].overall_score - 0.8).abs() < f32::EPSILON);
    assert!((list[2].overall_score - 0.9).abs() < f32::EPSILON);
}

#[test]
fn latest_returns_last_appended() {
    let (project, _) = open_project();
    let store = project.evaluations();

    store
        .append(&make_run("2026-01-01T00:00:00Z", 0.5, 2))
        .expect("1");
    store
        .append(&make_run("2026-01-02T00:00:00Z", 0.95, 2))
        .expect("2");

    let latest = store.latest().expect("latest").expect("some");
    assert!((latest.overall_score - 0.95).abs() < f32::EPSILON);
}

// ── per_locale / per_flag_kind serialization ──────────────────────────────────

#[test]
fn per_locale_and_flag_kind_round_trip() {
    use i18n_harness_project::{FlagScore, LocaleScore};

    let (project, _) = open_project();
    let store = project.evaluations();

    let mut run = make_run("2026-01-01T00:00:00Z", 0.8, 5);
    run.per_locale.insert(
        "de_DE".into(),
        LocaleScore {
            score: 0.75,
            count: 4,
        },
    );
    run.per_locale.insert(
        "fr_FR".into(),
        LocaleScore {
            score: 1.0,
            count: 1,
        },
    );
    run.per_flag_kind.insert(
        "idiom".into(),
        FlagScore {
            score: 0.5,
            count: 2,
        },
    );

    store.append(&run).expect("append");
    let list = store.list().expect("list");
    let loaded = &list[0];

    assert_eq!(loaded.per_locale.len(), 2);
    let de = &loaded.per_locale["de_DE"];
    assert!((de.score - 0.75).abs() < f32::EPSILON);
    assert_eq!(de.count, 4);

    let fs = &loaded.per_flag_kind["idiom"];
    assert!((fs.score - 0.5).abs() < f32::EPSILON);
    assert_eq!(fs.count, 2);
}

// ── malformed line tolerance ──────────────────────────────────────────────────

#[test]
fn malformed_lines_are_silently_skipped() {
    let (project, raw_fs) = open_project();

    let run = make_run("2026-01-01T00:00:00Z", 0.6, 3);
    let good = serde_json::to_string(&run).unwrap();
    let content = format!("{good}\n{{not valid json!}}\n{good}\n");

    let eval_path = project.paths().evaluations().to_path_buf();
    raw_fs
        .write_atomic(&eval_path, content.as_bytes())
        .expect("write");

    let list = project.evaluations().list().expect("list");
    assert_eq!(list.len(), 2, "bad line should be silently skipped");
}

// ── paths accessor ────────────────────────────────────────────────────────────

#[test]
fn paths_evaluations_returns_expected_suffix() {
    let (project, _) = open_project();
    let eval_path = project.paths().evaluations();
    // Should end with "evaluations.jsonl" and be under the state dir.
    assert!(
        eval_path.ends_with("evaluations.jsonl"),
        "unexpected path: {eval_path:?}"
    );
    assert!(
        eval_path.starts_with(project.paths().state_dir()),
        "evaluations must be inside state_dir"
    );
}
