//! Tests for `ProjectManifest` parsing and validation.
//!
//! Covers: minimal valid manifest, missing `[project] schema`, unsupported
//! schema versions, unknown fields at root (accepted) and in sub-tables
//! (rejected), all variants of every enum type, and `PathsConfig` with and
//! without `state_dir`.

use std::path::PathBuf;

use i18n_harness_project::{
    BackendKind, CatalogFormat, ProjectError, ProjectManifest, RegisterOverride, SCHEMA_VERSION,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn minimal() -> &'static str {
    r#"
[project]
name = "my-app"
schema = 1
"#
}

// ── Minimal valid manifest ────────────────────────────────────────────────────

#[test]
fn minimal_valid_manifest_parses() {
    let (m, warnings) = ProjectManifest::from_toml(minimal()).expect("parse");
    assert_eq!(m.project.name, "my-app");
    assert_eq!(m.project.schema, 1);
    assert!(m.locales.is_empty());
    assert!(m.catalogs.is_empty());
    assert!(m.glossary.is_none());
    assert!(m.backends.default.is_none());
    assert!(m.prompts.is_none());
    assert!(m.paths.state_dir.is_none());
    assert!(warnings.is_empty());
}

#[test]
fn schema_version_accessor_matches_field() {
    let (m, _) = ProjectManifest::from_toml(minimal()).expect("parse");
    assert_eq!(m.schema_version(), SCHEMA_VERSION);
}

#[test]
fn default_backend_is_none_on_minimal() {
    let (m, _) = ProjectManifest::from_toml(minimal()).expect("parse");
    assert!(m.default_backend().is_none());
}

// ── Missing `[project] schema` ────────────────────────────────────────────────

#[test]
fn missing_schema_field_gives_precise_error() {
    let toml = r#"
[project]
name = "my-app"
"#;
    let err = ProjectManifest::from_toml(toml).unwrap_err();
    assert!(
        matches!(
            err,
            ProjectError::MissingRequiredField { ref field } if field == "project.schema"
        ),
        "got: {err:?}"
    );
}

// ── Unsupported schema versions ───────────────────────────────────────────────

#[test]
fn schema_zero_is_rejected() {
    let toml = r#"
[project]
name = "my-app"
schema = 0
"#;
    let err = ProjectManifest::from_toml(toml).unwrap_err();
    assert!(
        matches!(
            err,
            ProjectError::UnsupportedSchemaVersion {
                found: 0,
                supported: 1
            }
        ),
        "got: {err:?}"
    );
}

#[test]
fn schema_two_is_rejected() {
    let toml = r#"
[project]
name = "my-app"
schema = 2
"#;
    let err = ProjectManifest::from_toml(toml).unwrap_err();
    assert!(
        matches!(
            err,
            ProjectError::UnsupportedSchemaVersion {
                found: 2,
                supported: 1
            }
        ),
        "got: {err:?}"
    );
}

#[test]
fn schema_large_is_rejected() {
    let toml = r#"
[project]
name = "my-app"
schema = 999
"#;
    let err = ProjectManifest::from_toml(toml).unwrap_err();
    assert!(
        matches!(
            err,
            ProjectError::UnsupportedSchemaVersion {
                found: 999,
                supported: 1
            }
        ),
        "got: {err:?}"
    );
}

// ── Unknown fields ────────────────────────────────────────────────────────────

#[test]
fn unknown_field_at_root_is_accepted() {
    // The root manifest does NOT use deny_unknown_fields (design §2.4).
    let toml = r#"
[project]
name = "my-app"
schema = 1

[future_v2_table]
some_key = "value"
"#;
    let result = ProjectManifest::from_toml(toml);
    assert!(
        result.is_ok(),
        "unknown root-level table should be accepted, got: {result:?}"
    );
}

#[test]
fn unknown_field_in_locale_sub_table_is_rejected() {
    // Sub-tables use deny_unknown_fields (design §2.4).
    let toml = r#"
[project]
name = "my-app"
schema = 1

[locales.de_DE]
register = "formal"
typo_field = "oops"
"#;
    let err = ProjectManifest::from_toml(toml).unwrap_err();
    assert!(
        matches!(err, ProjectError::ManifestParse { .. }),
        "expected ManifestParse for unknown sub-table field, got: {err:?}"
    );
}

#[test]
fn unknown_field_in_catalog_entry_is_rejected() {
    let toml = r#"
[project]
name = "my-app"
schema = 1

[[catalogs]]
path = "translations/app.ts"
format = "qt-ts"
locale = "de_DE"
extra_field = "oops"
"#;
    let err = ProjectManifest::from_toml(toml).unwrap_err();
    assert!(
        matches!(err, ProjectError::ManifestParse { .. }),
        "expected ManifestParse for unknown catalog field, got: {err:?}"
    );
}

// ── All manifest fields populated ─────────────────────────────────────────────

#[test]
fn fully_populated_manifest_parses() {
    let toml = r#"
[project]
name = "full-app"
schema = 1

[locales.de_DE]
register = "formal"
variant = "de_DE"
length_warn_ratio = 1.3

[locales.es_ES]
register = "informal"

[[catalogs]]
path = "translations/app_de.ts"
format = "qt-ts"
locale = "de_DE"

[[catalogs]]
path = "translations/app_es.po"
format = "gettext-po"
locale = "es_ES"

[[catalogs]]
path = "translations/app_de.json"
format = "icu-json"
locale = "de_DE"

[glossary]
path = "glossary.toml"

[backend]
[backend.default]
kind = "ollama"
model = "gemma3:27b"
host = "http://localhost:11434"
num_ctx = 8192

[prompts]
template_dir = "templates"

[paths]
state_dir = ".i18n-harness"
"#;
    let (m, warnings) = ProjectManifest::from_toml(toml).expect("parse");
    assert!(warnings.is_empty());
    assert_eq!(m.project.name, "full-app");
    assert_eq!(m.locales.len(), 2);
    assert_eq!(m.catalogs.len(), 3);
    assert!(m.glossary.is_some());
    assert_eq!(
        m.glossary.as_ref().unwrap().path,
        PathBuf::from("glossary.toml")
    );
    let backend = m.default_backend().expect("backend");
    assert_eq!(backend.kind, BackendKind::Ollama);
    assert_eq!(backend.model.as_deref(), Some("gemma3:27b"));
    assert_eq!(backend.host.as_deref(), Some("http://localhost:11434"));
    assert_eq!(backend.num_ctx, Some(8192));
    assert!(m.prompts.is_some());
    assert!(m.paths.state_dir.is_some());
}

// ── BackendKind variants ──────────────────────────────────────────────────────

#[test]
fn backend_kind_manual_parses() {
    let (m, _) = parse_with_backend("manual");
    assert_eq!(m.default_backend().unwrap().kind, BackendKind::Manual);
}

#[test]
fn backend_kind_ollama_parses() {
    let (m, _) = parse_with_backend("ollama");
    assert_eq!(m.default_backend().unwrap().kind, BackendKind::Ollama);
}

#[test]
fn backend_kind_openai_compatible_parses() {
    let (m, _) = parse_with_backend("open-ai-compatible");
    assert_eq!(
        m.default_backend().unwrap().kind,
        BackendKind::OpenAiCompatible
    );
}

#[test]
fn backend_kind_agent_parses() {
    let (m, _) = parse_with_backend("agent");
    assert_eq!(m.default_backend().unwrap().kind, BackendKind::Agent);
}

fn parse_with_backend(kind: &str) -> (ProjectManifest, Vec<i18n_harness_project::ProjectWarning>) {
    let toml = format!(
        r#"
[project]
name = "test"
schema = 1

[backend.default]
kind = "{kind}"
"#
    );
    ProjectManifest::from_toml(&toml).expect("parse")
}

// ── CatalogFormat variants ────────────────────────────────────────────────────

#[test]
fn catalog_format_qt_ts_parses() {
    assert_eq!(parse_catalog_format("qt-ts"), CatalogFormat::QtTs);
}

#[test]
fn catalog_format_gettext_po_parses() {
    assert_eq!(parse_catalog_format("gettext-po"), CatalogFormat::GettextPo);
}

#[test]
fn catalog_format_icu_json_parses() {
    assert_eq!(parse_catalog_format("icu-json"), CatalogFormat::IcuJson);
}

fn parse_catalog_format(fmt: &str) -> CatalogFormat {
    let toml = format!(
        r#"
[project]
name = "test"
schema = 1

[[catalogs]]
path = "file.ts"
format = "{fmt}"
locale = "de_DE"
"#
    );
    let (m, _) = ProjectManifest::from_toml(&toml).expect("parse");
    m.catalogs[0].format
}

// ── RegisterOverride variants ─────────────────────────────────────────────────

#[test]
fn register_override_formal_parses() {
    assert_eq!(parse_register("formal"), RegisterOverride::Formal);
}

#[test]
fn register_override_informal_parses() {
    assert_eq!(parse_register("informal"), RegisterOverride::Informal);
}

#[test]
fn register_override_neutral_parses() {
    assert_eq!(parse_register("neutral"), RegisterOverride::Neutral);
}

fn parse_register(reg: &str) -> RegisterOverride {
    let toml = format!(
        r#"
[project]
name = "test"
schema = 1

[locales.de_DE]
register = "{reg}"
"#
    );
    let (m, _) = ProjectManifest::from_toml(&toml).expect("parse");
    m.locales["de_DE"].register.expect("register")
}

// ── PathsConfig ───────────────────────────────────────────────────────────────

#[test]
fn paths_config_without_state_dir() {
    let (m, _) = ProjectManifest::from_toml(minimal()).expect("parse");
    assert!(m.paths.state_dir.is_none());
}

#[test]
fn paths_config_with_state_dir() {
    let toml = r#"
[project]
name = "test"
schema = 1

[paths]
state_dir = "/tmp/my-state"
"#;
    let (m, _) = ProjectManifest::from_toml(toml).expect("parse");
    assert_eq!(m.paths.state_dir, Some(PathBuf::from("/tmp/my-state")));
}
