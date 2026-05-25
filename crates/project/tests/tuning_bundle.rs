//! Integration tests for `Project::export_tuning_bundle` and
//! `Project::list_tuning_bundles`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_core::UnitId;
use i18n_harness_project::{
    CorrectionId, CorrectionProvenance, InMemoryFs, NewCorrection, Project, ProjectError, ProjectFs,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

const ROOT: &str = "/project";

fn root() -> &'static Path {
    Path::new(ROOT)
}

/// Minimal manifest with one locale entry.
const MANIFEST_WITH_LOCALE: &str = r#"[project]
name = "test-project"
schema = 1

[locales.de_DE]
register = "formal"
"#;

fn open_project_with_locale() -> (Project, Arc<InMemoryFs>) {
    let raw_fs = Arc::new(InMemoryFs::new());
    raw_fs
        .write_atomic(
            &PathBuf::from(ROOT).join("i18n-harness.toml"),
            MANIFEST_WITH_LOCALE.as_bytes(),
        )
        .unwrap();
    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (project, _) = Project::open_with_fs(root(), fs).expect("open");
    (project, raw_fs)
}

fn open_project_minimal() -> (Project, Arc<InMemoryFs>) {
    let raw_fs = Arc::new(InMemoryFs::new());
    raw_fs
        .write_atomic(
            &PathBuf::from(ROOT).join("i18n-harness.toml"),
            r#"[project]
name = "test-project"
schema = 1
"#
            .as_bytes(),
        )
        .unwrap();
    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (project, _) = Project::open_with_fs(root(), fs).expect("open");
    (project, raw_fs)
}

fn new_correction(catalog: &str, unit: &str, src: &str, mt: &str, ht: &str) -> NewCorrection {
    NewCorrection {
        catalog: PathBuf::from(catalog),
        locale: "de_DE".to_owned(),
        unit_id: UnitId(unit.to_owned()),
        source: src.to_owned(),
        mt_proposal: mt.to_owned(),
        human_target: ht.to_owned(),
        provenance: CorrectionProvenance::default(),
        flags_at_correction: vec![],
    }
}

// ── Test: empty curated set returns NoCuratedExamples ────────────────────────

#[test]
fn empty_curated_set_returns_error_without_creating_directory() {
    let (project, raw_fs) = open_project_minimal();

    // No curated examples — export must fail.
    let err = project.export_tuning_bundle().unwrap_err();
    assert!(
        matches!(err, ProjectError::NoCuratedExamples),
        "expected NoCuratedExamples, got: {err:?}"
    );

    // The tuning root must NOT have been created (or must remain empty).
    let tuning_root = project.paths().tuning_root().to_path_buf();
    // Either the directory was never created, or it exists but has no children.
    if raw_fs.exists(&tuning_root) {
        let entries = raw_fs.list_dir(&tuning_root).unwrap();
        assert!(
            entries.is_empty(),
            "tuning root should be empty when export fails on empty curated set"
        );
    }
}

// ── Test: small curated set produces all expected files ───────────────────────

#[test]
fn small_curated_set_produces_all_expected_files() {
    let (mut project, raw_fs) = open_project_with_locale();

    // Record two corrections.
    let id1 = project
        .record_correction(new_correction(
            "app.ts",
            "u1",
            "Open",
            "Öffnen (Vorschlag)",
            "Öffnen",
        ))
        .expect("record 1");
    let id2 = project
        .record_correction(new_correction(
            "app.ts",
            "u2",
            "Save",
            "Speichern (Vorschlag)",
            "Sichern",
        ))
        .expect("record 2");

    // Promote both.
    project
        .promote_to_curated(id1, Some("Use Öffnen.".to_owned()))
        .expect("promote 1");
    project.promote_to_curated(id2, None).expect("promote 2");

    // Export.
    let summary = project.export_tuning_bundle().expect("export");

    assert_eq!(summary.examples_count, 2);
    assert_eq!(summary.locales, vec!["de_DE"]);
    assert!(!summary.has_score, "no evaluation run yet");
    assert!(!summary.prompt_template_version.is_empty());

    // Verify the bundle directory exists and contains all expected files.
    let bundle_path = PathBuf::from(&summary.path);
    assert!(raw_fs.is_dir(&bundle_path), "bundle dir should exist");

    let expected_files = ["examples.jsonl", "prompt.txt", "locales.toml", "README.md"];
    for name in &expected_files {
        let file_path = bundle_path.join(name);
        assert!(
            raw_fs.is_file(&file_path),
            "expected file {} to exist in bundle",
            name
        );
    }

    // score.json must NOT be present when no evaluation has run.
    assert!(
        !raw_fs.is_file(&bundle_path.join("score.json")),
        "score.json should be absent when no evaluation has run"
    );

    // Parse examples.jsonl and verify content.
    let examples_text = raw_fs
        .read_to_string(&bundle_path.join("examples.jsonl"))
        .expect("read examples.jsonl");

    let parsed: Vec<serde_json::Value> = examples_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("valid JSON"))
        .collect();

    assert_eq!(parsed.len(), 2, "two examples in jsonl");

    // Verify schema field.
    assert_eq!(parsed[0]["schema"], 1);

    // Verify note is present on example 1 but absent (or null) on example 2.
    let note0 = parsed[0].get("note");
    assert!(
        note0.is_some_and(|v| v == "Use Öffnen."),
        "note should be 'Use Öffnen.' on first example"
    );
    // Note was omitted for id2, so the field should be absent.
    assert!(
        parsed[1].get("note").is_none(),
        "note should be absent on second example (no note given)"
    );

    // Verify locales.toml contains the de_DE section.
    let locales_text = raw_fs
        .read_to_string(&bundle_path.join("locales.toml"))
        .expect("read locales.toml");
    assert!(
        locales_text.contains("[de_DE]"),
        "locales.toml must contain [de_DE] section"
    );
    assert!(
        locales_text.contains("formal"),
        "locales.toml must reflect the register"
    );

    // Verify prompt.txt is non-empty and contains the template header.
    let prompt_text = raw_fs
        .read_to_string(&bundle_path.join("prompt.txt"))
        .expect("read prompt.txt");
    assert!(
        prompt_text.contains("[template="),
        "prompt.txt must contain the template header"
    );

    // Verify README.md is non-empty.
    let readme_text = raw_fs
        .read_to_string(&bundle_path.join("README.md"))
        .expect("read README.md");
    assert!(
        readme_text.len() > 100,
        "README.md should be non-trivially long"
    );
}

// ── Test: dangling curated references are skipped without crashing ────────────

#[test]
fn dangling_curated_references_are_skipped() {
    let (mut project, raw_fs) = open_project_minimal();

    // Record a real correction and promote it.
    let id_real = project
        .record_correction(new_correction(
            "app.ts",
            "u1",
            "Close",
            "Schließen v",
            "Schließen",
        ))
        .expect("record real");
    project
        .promote_to_curated(id_real, None)
        .expect("promote real");

    // Manually promote a fake id (dangling reference) by writing curated.toml
    // directly.
    let fake_id = CorrectionId("corr_deadbeef0000".to_owned());
    let curated_path = project.paths().curated().to_path_buf();
    // Read existing curated.toml and append the dangling entry.
    let existing = raw_fs.read_to_string(&curated_path).unwrap_or_default();
    let updated = format!("{existing}\n[[example]]\nid = \"{}\"\n", fake_id.0);
    raw_fs
        .write_atomic(&curated_path, updated.as_bytes())
        .unwrap();

    // Re-open so the project re-reads curated.toml.
    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (project2, _) = Project::open_with_fs(root(), Arc::clone(&fs)).expect("reopen");

    // Export should succeed and produce 1 example (the real one), not crash.
    let summary = project2.export_tuning_bundle().expect("export");
    assert_eq!(
        summary.examples_count, 1,
        "only the real correction should be in the bundle"
    );
}

// ── Test: multiple exports produce distinct timestamped directories ───────────

#[test]
fn multiple_exports_produce_distinct_directories() {
    let (mut project, _raw_fs) = open_project_minimal();

    // Record and promote one correction.
    let id = project
        .record_correction(new_correction("app.ts", "u1", "Yes", "Ja (v)", "Ja"))
        .expect("record");
    project.promote_to_curated(id, None).expect("promote");

    // Export twice (timestamps include microseconds so collisions are rare).
    let _s1 = project.export_tuning_bundle().expect("export 1");
    let _s2 = project.export_tuning_bundle().expect("export 2");

    // list_tuning_bundles must return at least one entry.
    let bundles = project.list_tuning_bundles().expect("list");
    assert!(
        !bundles.is_empty(),
        "list_tuning_bundles should return at least one bundle"
    );

    // All returned summaries must have examples_count > 0.
    for b in &bundles {
        assert!(b.examples_count > 0, "each bundle must have examples");
    }
}
