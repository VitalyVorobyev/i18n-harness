//! Round-trip tests for `Correction` serialisation via `Project`.
//!
//! Covers:
//! - Write a correction, read it back, assert all fields survive.
//! - `CorrectionProvenance::default()` (manual entry) serialises compactly.
//! - `flags_at_correction` round-trips with a non-empty `Vec<Flag>`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_core::{Flag, UnitId};
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

fn make_new_correction(provenance: CorrectionProvenance, flags: Vec<Flag>) -> NewCorrection {
    NewCorrection {
        catalog: PathBuf::from("translations/app_de.ts"),
        locale: "de_DE".to_owned(),
        unit_id: UnitId("ctx::open".to_owned()),
        source: "Open".to_owned(),
        mt_proposal: "Öffnen".to_owned(),
        human_target: "Öffnen".to_owned(),
        provenance,
        flags_at_correction: flags,
    }
}

#[test]
fn roundtrip_all_fields() {
    let (project, _fs) = open_project();

    let provenance = CorrectionProvenance {
        backend: "ollama".to_owned(),
        model: "gemma4:e2b".to_owned(),
        model_version: "sha256:abc123".to_owned(),
        prompt_template_version: "ollama-translate-v2".to_owned(),
        glossary_version: "sha256:def456".to_owned(),
    };
    let id = project
        .record_correction(make_new_correction(provenance, vec![Flag::LengthWarn]))
        .expect("record");

    let all = project
        .list_corrections(CorrectionFilter::default())
        .unwrap();
    assert_eq!(all.len(), 1);
    let got = &all[0];

    assert_eq!(got.schema, 1);
    assert_eq!(got.id, id);
    assert!(!got.ts.is_empty());
    assert_eq!(got.catalog, PathBuf::from("translations/app_de.ts"));
    assert_eq!(got.locale, "de_DE");
    assert_eq!(got.unit_id.0, "ctx::open");
    assert_eq!(got.source, "Open");
    assert_eq!(got.mt_proposal, "Öffnen");
    assert_eq!(got.human_target, "Öffnen");
    assert_eq!(got.provenance.backend, "ollama");
    assert_eq!(got.provenance.model, "gemma4:e2b");
    assert_eq!(got.provenance.model_version, "sha256:abc123");
    assert_eq!(
        got.provenance.prompt_template_version,
        "ollama-translate-v2"
    );
    assert_eq!(got.provenance.glossary_version, "sha256:def456");
    assert_eq!(got.flags_at_correction, vec![Flag::LengthWarn]);
}

#[test]
fn default_provenance_serialises_compactly() {
    let (project, raw_fs) = open_project();

    project
        .record_correction(make_new_correction(CorrectionProvenance::default(), vec![]))
        .expect("record");

    // Read the raw bytes and verify no provenance field keys appear.
    let corrections_path = project.paths().corrections().to_path_buf();
    let raw = raw_fs.read_to_string(&corrections_path).unwrap();
    let line = raw.trim();

    assert!(
        !line.contains("\"backend\""),
        "empty backend should be omitted: {line}"
    );
    assert!(
        !line.contains("\"model\""),
        "empty model should be omitted: {line}"
    );
    assert!(
        !line.contains("\"model_version\""),
        "empty model_version should be omitted: {line}"
    );
    assert!(
        !line.contains("\"flags_at_correction\""),
        "empty flags should be omitted: {line}"
    );

    // Round-trip: the parsed provenance should equal default.
    let all = project
        .list_corrections(CorrectionFilter::default())
        .unwrap();
    assert_eq!(all[0].provenance, CorrectionProvenance::default());
    assert!(all[0].flags_at_correction.is_empty());
}

#[test]
fn flags_roundtrip_with_multiple_flags() {
    let (project, _fs) = open_project();

    let flags = vec![
        Flag::PlaceholderMismatch,
        Flag::LengthWarn,
        Flag::LowConfidence,
    ];
    project
        .record_correction(make_new_correction(
            CorrectionProvenance::default(),
            flags.clone(),
        ))
        .expect("record");

    let all = project
        .list_corrections(CorrectionFilter::default())
        .unwrap();
    assert_eq!(all[0].flags_at_correction, flags);
}

#[test]
fn empty_store_returns_empty_vec() {
    let (project, _fs) = open_project();
    let all = project
        .list_corrections(CorrectionFilter::default())
        .unwrap();
    assert!(all.is_empty());
}

#[test]
fn multiple_corrections_append_and_read_in_order() {
    let (project, _fs) = open_project();

    for i in 0..5usize {
        let c = NewCorrection {
            catalog: PathBuf::from("a.ts"),
            locale: "de_DE".to_owned(),
            unit_id: UnitId(format!("unit_{i}")),
            source: "x".to_owned(),
            mt_proposal: "y".to_owned(),
            human_target: format!("z_{i}"),
            provenance: CorrectionProvenance::default(),
            flags_at_correction: vec![],
        };
        project.record_correction(c).expect("record");
    }

    let all = project
        .list_corrections(CorrectionFilter::default())
        .unwrap();
    assert_eq!(all.len(), 5);
    // Verify each was stored (order is append order).
    for (i, c) in all.iter().enumerate() {
        assert_eq!(c.unit_id.0, format!("unit_{i}"));
    }
}
