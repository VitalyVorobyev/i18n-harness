//! End-to-end tests for `Project::create_from_draft`.
//!
//! Build a `DraftManifest` programmatically, persist it via
//! `create_from_draft_with_fs`, then verify the resulting project loads
//! cleanly with matching catalogs/locales/glossary.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_project::{
    BackendConfig, BackendKind, ClassificationConfidence, DraftCatalog, DraftManifest, FormatGuess,
    GlossaryConfig, InMemoryFs, LocaleConfig, Project, ProjectFs,
};

const ROOT: &str = "/myapp";

fn root() -> &'static Path {
    Path::new(ROOT)
}

fn fs_with_catalog_files() -> Arc<InMemoryFs> {
    let fs = Arc::new(InMemoryFs::new());
    let root = PathBuf::from(ROOT);
    fs.create_dir_all(&root.join("translations")).unwrap();
    fs.write_atomic(
        &root.join("translations/app_de.ts"),
        br#"<?xml version="1.0"?><TS version="2.1" language="de_DE"></TS>"#,
    )
    .unwrap();
    fs.write_atomic(
        &root.join("translations/app_es.ts"),
        br#"<?xml version="1.0"?><TS version="2.1" language="es_ES"></TS>"#,
    )
    .unwrap();
    fs
}

fn basic_draft() -> DraftManifest {
    let mut locales = BTreeMap::new();
    locales.insert("de_DE".to_owned(), LocaleConfig::default());
    locales.insert("es_ES".to_owned(), LocaleConfig::default());

    DraftManifest {
        root: PathBuf::from(ROOT),
        name: "myapp".to_owned(),
        locales,
        catalogs: vec![
            DraftCatalog {
                path: PathBuf::from(ROOT).join("translations/app_de.ts"),
                format: FormatGuess::QtTs,
                locale: Some("de_DE".to_owned()),
                confidence: ClassificationConfidence::High,
                reason: "test fixture".to_owned(),
                alternatives: vec![],
            },
            DraftCatalog {
                path: PathBuf::from(ROOT).join("translations/app_es.ts"),
                format: FormatGuess::QtTs,
                locale: Some("es_ES".to_owned()),
                confidence: ClassificationConfidence::High,
                reason: "test fixture".to_owned(),
                alternatives: vec![],
            },
        ],
        glossary: None,
        backend: None,
    }
}

#[test]
fn round_trip_writes_manifest_and_loads_project() {
    let fs = fs_with_catalog_files();
    let fs_arc: Arc<dyn ProjectFs> = Arc::clone(&fs) as Arc<dyn ProjectFs>;
    let draft = basic_draft();

    let (project, warnings) =
        Project::create_from_draft_with_fs(root(), draft, fs_arc).expect("create_from_draft");

    assert_eq!(warnings.len(), 0, "unexpected warnings: {warnings:?}");
    assert_eq!(project.manifest().project.name, "myapp");
    assert_eq!(project.catalogs().len(), 2);
    assert!(project.glossary().is_none());

    // Manifest landed on disk.
    let manifest_text = fs
        .read_to_string(&PathBuf::from(ROOT).join("i18n-harness.toml"))
        .expect("manifest file written");
    assert!(manifest_text.contains("[project]"));
    assert!(manifest_text.contains("name = \"myapp\""));
    assert!(manifest_text.contains("schema = 1"));
    assert!(manifest_text.contains("de_DE"));
    assert!(manifest_text.contains("es_ES"));
}

#[test]
fn draft_with_glossary_persists_glossary_config() {
    let fs = fs_with_catalog_files();
    fs.write_atomic(
        &PathBuf::from(ROOT).join("glossary.toml"),
        b"[meta]\nschema_version = 1\n",
    )
    .unwrap();

    let fs_arc: Arc<dyn ProjectFs> = Arc::clone(&fs) as Arc<dyn ProjectFs>;
    let mut draft = basic_draft();
    draft.glossary = Some(GlossaryConfig {
        path: PathBuf::from("glossary.toml"),
    });

    let (project, _) =
        Project::create_from_draft_with_fs(root(), draft, fs_arc).expect("create_from_draft");

    assert!(project.glossary().is_some(), "glossary should have loaded");
    assert!(
        project
            .manifest()
            .glossary
            .as_ref()
            .map(|g| g.path.as_path() == Path::new("glossary.toml"))
            .unwrap_or(false)
    );
}

#[test]
fn draft_with_backend_persists_default_backend() {
    let fs = fs_with_catalog_files();
    let fs_arc: Arc<dyn ProjectFs> = Arc::clone(&fs) as Arc<dyn ProjectFs>;
    let mut draft = basic_draft();
    draft.backend = Some(BackendConfig {
        kind: BackendKind::Ollama,
        model: Some("gemma4:e2b".to_owned()),
        host: Some("http://localhost:11434".to_owned()),
        num_ctx: Some(8192),
    });

    let (project, _) =
        Project::create_from_draft_with_fs(root(), draft, fs_arc).expect("create_from_draft");

    let backend = project
        .manifest()
        .default_backend()
        .expect("backend should be present");
    assert_eq!(backend.kind, BackendKind::Ollama);
    assert_eq!(backend.model.as_deref(), Some("gemma4:e2b"));
}

#[test]
fn draft_skips_unknown_format_catalogs() {
    let fs = fs_with_catalog_files();
    let fs_arc: Arc<dyn ProjectFs> = Arc::clone(&fs) as Arc<dyn ProjectFs>;

    let mut draft = basic_draft();
    // Add an Unknown-format catalog; it should be skipped when the manifest is written.
    draft.catalogs.push(DraftCatalog {
        path: PathBuf::from(ROOT).join("translations/mystery.bin"),
        format: FormatGuess::Unknown,
        locale: None,
        confidence: ClassificationConfidence::Low,
        reason: "unrecognized format".to_owned(),
        alternatives: vec![],
    });

    let (project, _) =
        Project::create_from_draft_with_fs(root(), draft, fs_arc).expect("create_from_draft");

    // Only the two QtTs catalogs end up in the manifest.
    assert_eq!(project.catalogs().len(), 2);
    assert!(
        project
            .catalogs()
            .iter()
            .all(|c| c.format == i18n_harness_project::CatalogFormat::QtTs)
    );
}
