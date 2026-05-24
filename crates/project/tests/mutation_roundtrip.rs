//! Round-trip preservation tests for the TOML mutation surface.
//!
//! Critical contract: comments, blank lines, key ordering, and unknown sibling
//! keys survive every mutation. The fixture `mutation_roundtrip.toml` contains
//! all of these; tests apply targeted mutations and verify the survivors.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_project::{
    BackendConfig, BackendKind, CatalogEntry, CatalogFormat, InMemoryFs, LocaleConfig, Project,
    ProjectFs, RegisterOverride,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

const ROOT: &str = "/project";
const FIXTURE: &str = include_str!("fixtures/mutation_roundtrip.toml");

fn root() -> &'static Path {
    Path::new(ROOT)
}

/// Build an in-memory FS pre-populated with the round-trip fixture manifest
/// and stub catalog files (existence check only in M4.1b).
fn fs_with_fixture() -> Arc<InMemoryFs> {
    let fs = Arc::new(InMemoryFs::new());
    let root = PathBuf::from(ROOT);

    fs.write_atomic(&root.join("i18n-harness.toml"), FIXTURE.as_bytes())
        .unwrap();

    // Stub catalog files referenced in the fixture.
    fs.create_dir_all(&root.join("translations")).unwrap();
    fs.write_atomic(&root.join("translations/app_de.ts"), b"<TS></TS>")
        .unwrap();
    fs.write_atomic(
        &root.join("translations/app_es.po"),
        b"msgid \"\"\nmsgstr \"\"",
    )
    .unwrap();

    fs
}

/// Open a project and save it back; return the saved text.
fn open_mutate_save<F>(mutate: F) -> (String, String)
where
    F: FnOnce(&mut Project),
{
    let raw_fs = fs_with_fixture();
    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (mut project, _) = Project::open_with_fs(root(), fs).expect("open");
    mutate(&mut project);
    project.save_manifest().expect("save");

    let saved = raw_fs
        .read_to_string(&PathBuf::from(ROOT).join("i18n-harness.toml"))
        .expect("read saved");
    (FIXTURE.to_owned(), saved)
}

// ── update_locale — only `register` changes ──────────────────────────────────

#[test]
fn update_locale_preserves_comments_and_root_forward_compat_table() {
    let (original, saved) = open_mutate_save(|project| {
        project
            .update_locale(
                "de_DE",
                LocaleConfig {
                    register: Some(RegisterOverride::Neutral),
                    variant: None,
                    length_warn_ratio: None,
                },
            )
            .expect("update_locale");
    });

    // The top-level comment must survive.
    assert!(
        saved.contains("# Top-level comment that must survive all mutations."),
        "top-level comment lost"
    );

    // The forward-compat root table must survive (root has no deny_unknown_fields).
    assert!(
        saved.contains("future_v2_extension"),
        "forward-compat root table 'future_v2_extension' was stripped"
    );
    assert!(
        saved.contains("preserved_value"),
        "forward-compat value 'preserved_value' was stripped"
    );

    // The comment inside the locales block must survive.
    assert!(
        saved.contains("# Locales block with a comment."),
        "locales block comment lost"
    );

    // register changed from "formal" to "neutral".
    assert!(
        saved.contains("neutral"),
        "new register value 'neutral' not found"
    );
    assert!(
        !saved.contains(r#"register = "formal""#),
        "old register value 'formal' survived when it should have changed"
    );

    // The original had "formal"; verify parse sees "neutral" now.
    let (manifest, _) = i18n_harness_project::ProjectManifest::from_toml(&saved).expect("re-parse");
    assert_eq!(
        manifest.locales["de_DE"].register,
        Some(RegisterOverride::Neutral)
    );

    // es_ES is unchanged.
    assert_eq!(
        manifest.locales["es_ES"].register,
        Some(RegisterOverride::Informal)
    );

    // Fixture-level sanity.
    let _ = original;
}

// ── add_catalog — appended, original entries unchanged ───────────────────────

#[test]
fn add_catalog_appends_and_preserves_existing() {
    let raw_fs = fs_with_fixture();
    // Create the new catalog file first.
    raw_fs
        .write_atomic(&PathBuf::from(ROOT).join("translations/app_zh.json"), b"{}")
        .unwrap();

    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (mut project, _) = Project::open_with_fs(root(), fs).expect("open");
    project
        .add_catalog(CatalogEntry {
            path: PathBuf::from("translations/app_zh.json"),
            format: CatalogFormat::IcuJson,
            locale: "zh_Hans".to_owned(),
        })
        .expect("add_catalog");
    project.save_manifest().expect("save");

    let saved = raw_fs
        .read_to_string(&PathBuf::from(ROOT).join("i18n-harness.toml"))
        .expect("read");

    // Top-level comment survived.
    assert!(
        saved.contains("# Top-level comment that must survive all mutations."),
        "top-level comment lost after add_catalog"
    );

    // Original catalogs are still present.
    assert!(
        saved.contains("translations/app_de.ts"),
        "first catalog lost"
    );
    assert!(
        saved.contains("translations/app_es.po"),
        "second catalog lost"
    );

    // New catalog is present.
    assert!(
        saved.contains("translations/app_zh.json"),
        "new catalog not written"
    );
    assert!(saved.contains("icu-json"), "new catalog format not written");

    // Parse and verify 3 catalogs.
    let (manifest, _) = i18n_harness_project::ProjectManifest::from_toml(&saved).expect("re-parse");
    assert_eq!(manifest.catalogs.len(), 3);
    assert_eq!(manifest.catalogs[2].format, CatalogFormat::IcuJson);
}

// ── remove_catalog ────────────────────────────────────────────────────────────

#[test]
fn remove_catalog_leaves_other_catalogs_and_comments() {
    let (original, saved) = open_mutate_save(|project| {
        let removed = project
            .remove_catalog(Path::new("translations/app_de.ts"))
            .expect("remove_catalog");
        assert!(
            removed,
            "remove_catalog should return true when entry existed"
        );
    });

    // First catalog gone.
    assert!(
        !saved.contains("translations/app_de.ts"),
        "removed catalog still present"
    );

    // Second catalog survived.
    assert!(
        saved.contains("translations/app_es.po"),
        "remaining catalog was removed"
    );

    // Comment survived.
    assert!(
        saved.contains("# Top-level comment that must survive all mutations."),
        "top-level comment lost after remove_catalog"
    );

    let (manifest, _) = i18n_harness_project::ProjectManifest::from_toml(&saved).expect("re-parse");
    assert_eq!(manifest.catalogs.len(), 1);
    assert_eq!(
        manifest.catalogs[0].path,
        PathBuf::from("translations/app_es.po")
    );

    let _ = original;
}

#[test]
fn remove_catalog_returns_false_when_not_found() {
    let fs: Arc<dyn ProjectFs> = fs_with_fixture() as Arc<dyn ProjectFs>;
    let (mut project, _) = Project::open_with_fs(root(), fs).expect("open");
    let removed = project
        .remove_catalog(Path::new("translations/nonexistent.ts"))
        .expect("remove_catalog");
    assert!(
        !removed,
        "remove_catalog must return false for missing entry"
    );
}

// ── set_backend ───────────────────────────────────────────────────────────────

#[test]
fn set_backend_updates_fields_and_preserves_comments() {
    let (original, saved) = open_mutate_save(|project| {
        project
            .set_backend(BackendConfig {
                kind: BackendKind::Manual,
                model: None,
                host: None,
                num_ctx: None,
            })
            .expect("set_backend");
    });

    // Comment survived.
    assert!(
        saved.contains("# Top-level comment that must survive all mutations."),
        "top-level comment lost after set_backend"
    );

    // New kind written.
    assert!(saved.contains("manual"), "backend kind 'manual' not found");

    // Old kind gone.
    assert!(
        !saved.contains("ollama"),
        "old backend kind 'ollama' survived"
    );

    let (manifest, _) = i18n_harness_project::ProjectManifest::from_toml(&saved).expect("re-parse");
    assert_eq!(
        manifest.default_backend().unwrap().kind,
        BackendKind::Manual
    );
    assert!(manifest.default_backend().unwrap().model.is_none());

    let _ = original;
}

// ── remove_locale ────────────────────────────────────────────────────────────

#[test]
fn remove_locale_returns_false_when_not_found() {
    let fs: Arc<dyn ProjectFs> = fs_with_fixture() as Arc<dyn ProjectFs>;
    let (mut project, _) = Project::open_with_fs(root(), fs).expect("open");
    let removed = project.remove_locale("fr_FR").expect("remove_locale");
    assert!(!removed);
}

#[test]
fn remove_locale_removes_existing_block() {
    let (_, saved) = open_mutate_save(|project| {
        let removed = project.remove_locale("de_DE").expect("remove_locale");
        assert!(removed, "de_DE should have been removed");
    });

    let (manifest, _) = i18n_harness_project::ProjectManifest::from_toml(&saved).expect("re-parse");
    assert!(!manifest.locales.contains_key("de_DE"));
    assert!(manifest.locales.contains_key("es_ES"));
}

// ── Typed view consistency after each mutation ────────────────────────────────

#[test]
fn manifest_view_reflects_changes_immediately_without_save() {
    let fs: Arc<dyn ProjectFs> = fs_with_fixture() as Arc<dyn ProjectFs>;
    let (mut project, _) = Project::open_with_fs(root(), fs).expect("open");

    assert_eq!(
        project.manifest().locales["de_DE"].register,
        Some(RegisterOverride::Formal)
    );

    project
        .update_locale(
            "de_DE",
            LocaleConfig {
                register: Some(RegisterOverride::Neutral),
                variant: None,
                length_warn_ratio: None,
            },
        )
        .expect("update_locale");

    // In-memory view updated immediately.
    assert_eq!(
        project.manifest().locales["de_DE"].register,
        Some(RegisterOverride::Neutral)
    );
}
