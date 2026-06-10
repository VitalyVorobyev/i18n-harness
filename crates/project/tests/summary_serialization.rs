//! Tests for `ProjectSummary` serialization to JSON.
//!
//! Verifies that the field shape is what the Tauri command surface expects:
//! paths as strings, not `PathBuf`; `CatalogStatus` as kebab-case strings;
//! `BackendKind` serialized correctly; `Serialize` but not `Deserialize`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_project::{
    BackendConfig, BackendKind, CatalogEntry, CatalogFormat, InMemoryFs, Project, ProjectFs,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

const ROOT: &str = "/project";

fn fs_minimal() -> Arc<dyn ProjectFs> {
    let toml = r#"
[project]
name = "summary-test"
schema = 1

[locales.de_DE]
register = "formal"

[[catalogs]]
path = "translations/app.ts"
format = "qt-ts"
locale = "de_DE"

[backend.default]
kind = "ollama"
model = "gemma3:27b"
"#;
    let fs = Arc::new(InMemoryFs::new());
    let root = PathBuf::from(ROOT);
    fs.write_atomic(&root.join("i18n-harness.toml"), toml.as_bytes())
        .unwrap();
    fs.create_dir_all(&root.join("translations")).unwrap();
    fs.write_atomic(&root.join("translations/app.ts"), b"<TS></TS>")
        .unwrap();
    fs as Arc<dyn ProjectFs>
}

// ── Field shape ───────────────────────────────────────────────────────────────

#[test]
fn summary_paths_are_strings_not_pathbuf() {
    let (project, _) = Project::open_with_fs(Path::new(ROOT), fs_minimal()).expect("open");
    let summary = project.summary();

    // `root`, `state_dir`, `glossary_path` are String — verify via serde_json
    let json = serde_json::to_string(&summary).expect("serialize");
    let v: serde_json::Value = serde_json::from_str(&json).expect("deserialize json");

    assert!(v["root"].is_string(), "root must be a JSON string");
    assert!(
        v["state_dir"].is_string(),
        "state_dir must be a JSON string"
    );
    assert!(v["name"].is_string(), "name must be a JSON string");
}

#[test]
fn summary_catalog_absolute_path_is_string() {
    let (project, _) = Project::open_with_fs(Path::new(ROOT), fs_minimal()).expect("open");
    let summary = project.summary();
    let json = serde_json::to_string(&summary).expect("serialize");
    let v: serde_json::Value = serde_json::from_str(&json).expect("deserialize json");

    let cat = &v["catalogs"][0];
    assert!(
        cat["absolute_path"].is_string(),
        "absolute_path must be a string"
    );
    assert!(
        cat["manifest_path"].is_string(),
        "manifest_path must be a string"
    );
}

#[test]
fn catalog_status_serializes_as_kebab_case() {
    let (project, _) = Project::open_with_fs(Path::new(ROOT), fs_minimal()).expect("open");
    let summary = project.summary();
    let json = serde_json::to_string(&summary).expect("serialize");
    let v: serde_json::Value = serde_json::from_str(&json).expect("deserialize json");
    let status = v["catalogs"][0]["status"].as_str().expect("status string");
    assert_eq!(status, "ok", "CatalogStatus::Ok must serialize as 'ok'");
}

#[test]
fn backend_kind_serializes_in_summary() {
    let (project, _) = Project::open_with_fs(Path::new(ROOT), fs_minimal()).expect("open");
    let summary = project.summary();
    let json = serde_json::to_string(&summary).expect("serialize");
    let v: serde_json::Value = serde_json::from_str(&json).expect("deserialize json");
    let kind = v["backend"]["kind"].as_str().expect("backend kind string");
    assert_eq!(kind, "ollama");
}

#[test]
fn glossary_path_is_null_when_absent() {
    let (project, _) = Project::open_with_fs(Path::new(ROOT), fs_minimal()).expect("open");
    let summary = project.summary();
    let json = serde_json::to_string(&summary).expect("serialize");
    let v: serde_json::Value = serde_json::from_str(&json).expect("deserialize json");
    // No glossary in the minimal manifest.
    assert!(
        v["glossary_path"].is_null(),
        "glossary_path must be null when absent"
    );
}

// ── Round-trip through JSON ───────────────────────────────────────────────────

#[test]
fn summary_serializes_all_top_level_fields() {
    let (project, _) = Project::open_with_fs(Path::new(ROOT), fs_minimal()).expect("open");
    let summary = project.summary();
    let json = serde_json::to_string(&summary).expect("serialize");
    let v: serde_json::Value = serde_json::from_str(&json).expect("parse json");

    // All expected top-level keys present.
    for key in &[
        "root",
        "name",
        "schema",
        "locales",
        "catalogs",
        "state_dir",
        "backend",
    ] {
        assert!(
            v.get(*key).is_some(),
            "expected key '{key}' in ProjectSummary JSON"
        );
    }
    assert_eq!(v["name"].as_str().unwrap(), "summary-test");
    assert_eq!(v["schema"].as_u64().unwrap(), 1);
    assert_eq!(v["locales"].as_array().unwrap().len(), 1);
    assert_eq!(v["catalogs"].as_array().unwrap().len(), 1);
}

// ── ProjectSummary is NOT Deserialize ────────────────────────────────────────

// The type contract says ProjectSummary is output-only (no Deserialize).
// We can't test for the *absence* of a trait via runtime code, but we verify
// the concrete behavior: serde_json::from_str into ProjectSummary must not
// compile. We enforce this via a `compile_fail` doc-test in the source if
// desired, but for now we document the intent here.
//
// Rationale: the TypeScript side never sends a ProjectSummary back; Tauri
// commands that need to *receive* catalog data use narrower types.

#[test]
fn summary_clone_works() {
    let (project, _) = Project::open_with_fs(Path::new(ROOT), fs_minimal()).expect("open");
    let summary = project.summary();
    let cloned = summary.clone();
    assert_eq!(summary.name, cloned.name);
    assert_eq!(summary.catalogs.len(), cloned.catalogs.len());
}

// ── CatalogRef fields ─────────────────────────────────────────────────────────

#[test]
fn catalog_ref_manifest_path_is_relative() {
    let (project, _) = Project::open_with_fs(Path::new(ROOT), fs_minimal()).expect("open");
    let cat = &project.catalogs()[0];
    // manifest_path is the project-relative form from the manifest.
    assert_eq!(cat.manifest_path, "translations/app.ts");
    // absolute_path is rooted at the project root.
    assert!(
        cat.absolute_path.starts_with(ROOT),
        "absolute_path must start with project root"
    );
}

// ── set_backend round-trip through summary ────────────────────────────────────

#[test]
fn set_backend_then_summary_reflects_new_backend() {
    let (mut project, _) = Project::open_with_fs(Path::new(ROOT), fs_minimal()).expect("open");

    project
        .set_backend(BackendConfig {
            kind: BackendKind::Manual,
            model: None,
            host: None,
            num_ctx: None,
        })
        .expect("set_backend");

    let summary = project.summary();
    assert_eq!(summary.backend.as_ref().unwrap().kind, BackendKind::Manual);

    let json = serde_json::to_string(&summary).expect("serialize");
    let v: serde_json::Value = serde_json::from_str(&json).expect("parse json");
    assert_eq!(v["backend"]["kind"].as_str().unwrap(), "manual");
}

// ── add_catalog round-trip through summary ────────────────────────────────────

#[test]
fn add_catalog_then_summary_reflects_new_catalog() {
    let raw_fs = Arc::new(InMemoryFs::new());
    let toml = r#"
[project]
name = "summary-test"
schema = 1

[locales.de_DE]
register = "formal"

[[catalogs]]
path = "translations/app.ts"
format = "qt-ts"
locale = "de_DE"

[backend.default]
kind = "ollama"
model = "gemma3:27b"
"#;
    let root = PathBuf::from(ROOT);
    raw_fs
        .write_atomic(&root.join("i18n-harness.toml"), toml.as_bytes())
        .unwrap();
    raw_fs.create_dir_all(&root.join("translations")).unwrap();
    raw_fs
        .write_atomic(&root.join("translations/app.ts"), b"<TS></TS>")
        .unwrap();
    // Create the new file.
    raw_fs
        .write_atomic(&root.join("translations/app2.ts"), b"<TS></TS>")
        .unwrap();

    let fs: Arc<dyn ProjectFs> = Arc::clone(&raw_fs) as Arc<dyn ProjectFs>;
    let (mut project, _) = Project::open_with_fs(Path::new(ROOT), fs).expect("open");
    project
        .add_catalog(CatalogEntry {
            path: PathBuf::from("translations/app2.ts"),
            format: CatalogFormat::QtTs,
            locale: "es_ES".to_owned(),
        })
        .expect("add_catalog");

    let summary = project.summary();
    assert_eq!(summary.catalogs.len(), 2);

    let json = serde_json::to_string(&summary).expect("serialize");
    let v: serde_json::Value = serde_json::from_str(&json).expect("parse json");
    assert_eq!(v["catalogs"].as_array().unwrap().len(), 2);
}
