//! Round-trip preservation for `curated.toml`.
//!
//! After promoting a new id into a `curated.toml` that already has user
//! comments and existing entries, those comments and entries survive.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_core::UnitId;
use i18n_harness_project::{CorrectionProvenance, InMemoryFs, NewCorrection, Project, ProjectFs};

const ROOT: &str = "/project";

const MINIMAL_MANIFEST: &str = r#"[project]
name = "test-project"
schema = 1
"#;

/// A pre-existing `curated.toml` with user comments and one existing entry.
const EXISTING_CURATED: &str = r#"schema = 1

# This comment was added by the human translator.
# It explains why this example was curated.

[[example]]
id = "corr_aabbccddeeff"
note = "We always say 'Öffnen', not 'Eröffnen', for the file menu action."

# Another comment about the second entry.
[[example]]
id = "corr_112233445566"
note = "Prefer 'Abbrechen' over 'Stornieren'."
"#;

fn open_project_with_existing_curated() -> (Project, Arc<InMemoryFs>) {
    let raw_fs = Arc::new(InMemoryFs::new());
    let root = PathBuf::from(ROOT);

    raw_fs
        .write_atomic(&root.join("i18n-harness.toml"), MINIMAL_MANIFEST.as_bytes())
        .unwrap();

    // Pre-populate the state dir with an existing curated.toml.
    let state_dir = root.join(".i18n-harness");
    raw_fs.create_dir_all(&state_dir).unwrap();
    raw_fs
        .write_atomic(&state_dir.join("curated.toml"), EXISTING_CURATED.as_bytes())
        .unwrap();

    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (project, _) = Project::open_with_fs(Path::new(ROOT), fs).expect("open");
    (project, raw_fs)
}

fn new_correction(unit: &str, ht: &str) -> NewCorrection {
    NewCorrection {
        catalog: PathBuf::from("cat.ts"),
        locale: "de_DE".to_owned(),
        unit_id: UnitId(unit.to_owned()),
        source: "src".to_owned(),
        mt_proposal: String::new(),
        human_target: ht.to_owned(),
        provenance: CorrectionProvenance::default(),
        flags_at_correction: vec![],
    }
}

#[test]
fn existing_entries_survive_new_promote() {
    let (mut project, raw_fs) = open_project_with_existing_curated();

    // The existing curated set should have 2 entries (dangling, but that's ok).
    assert_eq!(
        project.curated().len(),
        2,
        "should have loaded 2 existing entries"
    );

    // Record a new correction and promote it.
    let id = project
        .record_correction(new_correction("u_new", "Neu"))
        .expect("record");
    project
        .promote_to_curated(id.clone(), Some("New entry note.".to_owned()))
        .expect("promote");

    // Verify in-memory state.
    assert_eq!(project.curated().len(), 3);
    assert!(project.curated().contains(&id));

    // Read back the saved curated.toml and verify the old entries are present.
    let saved = raw_fs
        .read_to_string(&PathBuf::from(ROOT).join(".i18n-harness/curated.toml"))
        .expect("read curated.toml");

    assert!(
        saved.contains("corr_aabbccddeeff"),
        "first existing id must survive"
    );
    assert!(
        saved.contains("corr_112233445566"),
        "second existing id must survive"
    );
    assert!(saved.contains(&id.0), "new id must be present: {}", id.0);

    // The user's notes for the existing entries must survive.
    assert!(saved.contains("Öffnen"), "first existing note must survive");
    assert!(
        saved.contains("Abbrechen"),
        "second existing note must survive"
    );
    assert!(
        saved.contains("New entry note."),
        "new note must be written"
    );
}

#[test]
fn curated_toml_parses_after_roundtrip() {
    let (mut project, raw_fs) = open_project_with_existing_curated();

    let id = project
        .record_correction(new_correction("u2", "Neu2"))
        .expect("record");
    project
        .promote_to_curated(id.clone(), None)
        .expect("promote");

    let saved = raw_fs
        .read_to_string(&PathBuf::from(ROOT).join(".i18n-harness/curated.toml"))
        .expect("read curated.toml");

    // The saved TOML must parse cleanly.
    let parsed: toml::Value = toml::from_str(&saved).expect("curated.toml must be valid TOML");
    let examples = parsed["example"].as_array().expect("example array");
    assert_eq!(examples.len(), 3, "must have 3 entries after promote");
}

#[test]
fn un_curate_removes_entry_file_survives_other_entries() {
    let (mut project, raw_fs) = open_project_with_existing_curated();

    // Un-curate the first existing entry (corr_aabbccddeeff).
    let first_id = i18n_harness_project::CorrectionId("corr_aabbccddeeff".to_owned());
    let removed = project.un_curate(&first_id).expect("un_curate");
    assert!(removed);

    let saved = raw_fs
        .read_to_string(&PathBuf::from(ROOT).join(".i18n-harness/curated.toml"))
        .expect("read curated.toml");

    assert!(
        !saved.contains("corr_aabbccddeeff"),
        "first id should be removed"
    );
    assert!(
        saved.contains("corr_112233445566"),
        "second id should survive"
    );
}
