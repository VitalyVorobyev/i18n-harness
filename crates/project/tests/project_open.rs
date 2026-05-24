//! Integration tests for `Project::open_with_fs` — happy path, error paths,
//! and warning emission.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_project::{
    CatalogStatus, InMemoryFs, Project, ProjectError, ProjectFs, ProjectWarning,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

const ROOT: &str = "/project";

fn root() -> &'static Path {
    Path::new(ROOT)
}

/// Build a minimal in-memory FS with the provided manifest text.
fn fs_with_manifest(manifest_toml: &str) -> Arc<dyn ProjectFs> {
    let fs = Arc::new(InMemoryFs::new());
    fs.write_atomic(
        &PathBuf::from(ROOT).join("i18n-harness.toml"),
        manifest_toml.as_bytes(),
    )
    .unwrap();
    fs as Arc<dyn ProjectFs>
}

/// Basic manifest + glossary + two catalog files.
fn fs_full() -> Arc<dyn ProjectFs> {
    let manifest = include_str!("fixtures/project_open_basic.toml");
    let glossary = include_str!("fixtures/glossary_basic.toml");

    let fs = Arc::new(InMemoryFs::new());
    let root = PathBuf::from(ROOT);

    fs.write_atomic(&root.join("i18n-harness.toml"), manifest.as_bytes())
        .unwrap();
    fs.write_atomic(&root.join("glossary.toml"), glossary.as_bytes())
        .unwrap();

    // Create stub catalog files (content not parsed in M4.1b).
    fs.create_dir_all(&root.join("translations")).unwrap();
    fs.write_atomic(&root.join("translations/app_de.ts"), b"<TS></TS>")
        .unwrap();
    fs.write_atomic(
        &root.join("translations/app_es.po"),
        b"msgid \"\"\nmsgstr \"\"",
    )
    .unwrap();

    fs as Arc<dyn ProjectFs>
}

// ── Happy path ────────────────────────────────────────────────────────────────

#[test]
fn open_full_project_succeeds() {
    let fs = fs_full();
    let (project, warnings) = Project::open_with_fs(root(), fs).expect("open");

    // Manifest is readable.
    let m = project.manifest();
    assert_eq!(m.project.name, "my-app");
    assert_eq!(m.project.schema, 1);

    // Two catalogs, both Ok.
    let cats = project.catalogs();
    assert_eq!(cats.len(), 2);
    assert!(cats.iter().all(|c| c.status == CatalogStatus::Ok));

    // Glossary loaded.
    assert!(project.glossary().is_some());

    // State dir was created.
    assert!(project.paths().state_dir().starts_with(PathBuf::from(ROOT)));

    // No errors in happy path; may have glossary locale warnings (es_ES
    // informal override is fine since es_ES is in the workspace table).
    let hard_errors: Vec<_> = warnings
        .iter()
        .filter(|w| !matches!(w, ProjectWarning::Glossary(_)))
        .collect();
    assert!(
        hard_errors.is_empty(),
        "unexpected non-glossary warnings: {hard_errors:?}"
    );
}

#[test]
fn locale_ids_order_preserved() {
    let fs = fs_full();
    let (project, _) = Project::open_with_fs(root(), fs).expect("open");
    let ids: Vec<&str> = project.locale_ids().collect();
    // Manifest declares de_DE before es_ES.
    assert_eq!(ids, vec!["de_DE", "es_ES"]);
}

#[test]
fn summary_reflects_manifest() {
    let fs = fs_full();
    let (project, _) = Project::open_with_fs(root(), fs).expect("open");
    let summary = project.summary();
    assert_eq!(summary.name, "my-app");
    assert_eq!(summary.schema, 1);
    assert_eq!(summary.locales, vec!["de_DE", "es_ES"]);
    assert_eq!(summary.catalogs.len(), 2);
    assert!(summary.glossary_path.is_some());
    assert!(summary.backend.is_some());
}

#[test]
fn state_dir_exists_after_open() {
    let fs = fs_full();
    let (project, _) = Project::open_with_fs(root(), fs).expect("open");
    // state_dir was created by open; it has an absolute path.
    assert!(project.paths().state_dir().is_absolute());
}

// ── Error paths ───────────────────────────────────────────────────────────────

#[test]
fn manifest_missing_returns_error() {
    let fs = Arc::new(InMemoryFs::new());
    let err = Project::open_with_fs(root(), fs).unwrap_err();
    assert!(
        matches!(err, ProjectError::ManifestMissing { .. }),
        "expected ManifestMissing, got {err:?}"
    );
}

#[test]
fn manifest_malformed_returns_parse_error() {
    let fs = fs_with_manifest("this is not valid toml @@@@");
    let err = Project::open_with_fs(root(), fs).unwrap_err();
    assert!(
        matches!(err, ProjectError::ManifestParse { .. }),
        "expected ManifestParse, got {err:?}"
    );
}

#[test]
fn unsupported_schema_version_returns_error() {
    let toml = "[project]\nname = \"x\"\nschema = 999\n";
    let fs = fs_with_manifest(toml);
    let err = Project::open_with_fs(root(), fs).unwrap_err();
    assert!(
        matches!(
            err,
            ProjectError::UnsupportedSchemaVersion { found: 999, .. }
        ),
        "expected UnsupportedSchemaVersion, got {err:?}"
    );
}

#[test]
fn catalog_not_found_returns_error() {
    let toml = r#"
[project]
name = "x"
schema = 1

[[catalogs]]
path = "translations/missing.ts"
format = "qt-ts"
locale = "de_DE"
"#;
    let fs = fs_with_manifest(toml);
    let err = Project::open_with_fs(root(), fs).unwrap_err();
    assert!(
        matches!(err, ProjectError::CatalogNotFound { .. }),
        "expected CatalogNotFound, got {err:?}"
    );
}

// ── Warning emission ──────────────────────────────────────────────────────────

#[test]
fn unknown_locale_in_manifest_emits_warning_not_error() {
    let toml = r#"
[project]
name = "x"
schema = 1

[locales.xx_XX]
register = "formal"
"#;
    let fs = fs_with_manifest(toml);
    // Should succeed despite unknown locale.
    let (project, warnings) = Project::open_with_fs(root(), fs).expect("open");
    assert!(
        warnings.iter().any(|w| matches!(
            w,
            ProjectWarning::UnknownLocale { locale } if locale == "xx_XX"
        )),
        "expected UnknownLocale warning, got {warnings:?}"
    );
    // locale() returns None for unknown id.
    assert!(project.locale("xx_XX").is_none());
}

#[test]
fn unknown_catalog_locale_emits_warning_not_error() {
    let toml = r#"
[project]
name = "x"
schema = 1

[[catalogs]]
path = "cat.ts"
format = "qt-ts"
locale = "xx_XX"
"#;
    let inner = Arc::new(InMemoryFs::new());
    inner
        .write_atomic(
            &PathBuf::from(ROOT).join("i18n-harness.toml"),
            toml.as_bytes(),
        )
        .unwrap();
    // Create the catalog file so we don't get CatalogNotFound.
    inner
        .write_atomic(&PathBuf::from(ROOT).join("cat.ts"), b"<TS></TS>")
        .unwrap();
    let fs: Arc<dyn ProjectFs> = inner;

    let (_project, warnings) = Project::open_with_fs(root(), fs).expect("open");
    assert!(
        warnings.iter().any(|w| matches!(
            w,
            ProjectWarning::UnknownCatalogLocale { locale, .. } if locale == "xx_XX"
        )),
        "expected UnknownCatalogLocale warning, got {warnings:?}"
    );
}

// ── Locale resolution ─────────────────────────────────────────────────────────

#[test]
fn known_locale_resolves_with_merged_overrides() {
    let fs = fs_full();
    let (project, _) = Project::open_with_fs(root(), fs).expect("open");

    let de = project.locale("de_DE").expect("de_DE must resolve");
    // Manifest says formal; workspace default for de_DE is also formal.
    assert_eq!(de.register(), i18n_harness_locales::Register::Formal);
    // Workspace-immutable fields are correct.
    assert_eq!(de.plural_arity(), 2);
    assert_eq!(de.script(), i18n_harness_locales::Script::Latin);

    let es = project.locale("es_ES").expect("es_ES must resolve");
    // Manifest says informal; glossary also says informal → informal wins.
    assert_eq!(es.register(), i18n_harness_locales::Register::Informal);
}
