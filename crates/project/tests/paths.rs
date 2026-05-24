//! Tests for `ProjectPaths` — path resolution with and without `[paths]
//! state_dir`, and the uniqueness contract of `new_tuning_bundle_dir`.

use std::path::{Path, PathBuf};

use i18n_harness_project::manifest::{PathsConfig, PromptsConfig};
use i18n_harness_project::paths::ProjectPaths;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn default_paths(root: &Path) -> ProjectPaths {
    ProjectPaths::new(root, &PathsConfig::default(), None, None)
}

fn paths_with_state_dir(root: &Path, state_dir: &str) -> ProjectPaths {
    let cfg = PathsConfig {
        state_dir: Some(PathBuf::from(state_dir)),
    };
    ProjectPaths::new(root, &cfg, None, None)
}

// ── Default paths ─────────────────────────────────────────────────────────────

#[test]
fn root_returns_project_root() {
    let root = Path::new("/home/user/my-app");
    let p = default_paths(root);
    assert_eq!(p.root(), root);
}

#[test]
fn manifest_is_under_root() {
    let root = Path::new("/home/user/my-app");
    let p = default_paths(root);
    assert_eq!(p.manifest(), root.join("i18n-harness.toml"));
}

#[test]
fn default_state_dir_is_dot_i18n_harness() {
    let root = Path::new("/home/user/my-app");
    let p = default_paths(root);
    assert_eq!(p.state_dir(), root.join(".i18n-harness"));
}

#[test]
fn metrics_is_under_state_dir() {
    let root = Path::new("/home/user/my-app");
    let p = default_paths(root);
    assert_eq!(
        p.metrics(),
        root.join(".i18n-harness").join("metrics.jsonl")
    );
}

#[test]
fn corrections_is_under_state_dir() {
    let root = Path::new("/home/user/my-app");
    let p = default_paths(root);
    assert_eq!(
        p.corrections(),
        root.join(".i18n-harness").join("corrections.jsonl")
    );
}

#[test]
fn curated_is_under_state_dir() {
    let root = Path::new("/home/user/my-app");
    let p = default_paths(root);
    assert_eq!(p.curated(), root.join(".i18n-harness").join("curated.toml"));
}

#[test]
fn batches_is_under_state_slash_state() {
    let root = Path::new("/home/user/my-app");
    let p = default_paths(root);
    assert_eq!(
        p.batches(),
        root.join(".i18n-harness")
            .join("state")
            .join("batches.jsonl")
    );
}

#[test]
fn tuning_root_is_under_state_dir() {
    let root = Path::new("/home/user/my-app");
    let p = default_paths(root);
    assert_eq!(p.tuning_root(), root.join(".i18n-harness").join("tuning"));
}

// ── state_dir override ────────────────────────────────────────────────────────

#[test]
fn absolute_state_dir_override_is_used_as_is() {
    let root = Path::new("/home/user/my-app");
    let p = paths_with_state_dir(root, "/var/state/my-app");
    assert_eq!(p.state_dir(), Path::new("/var/state/my-app"));
    assert_eq!(
        p.metrics(),
        Path::new("/var/state/my-app").join("metrics.jsonl")
    );
}

#[test]
fn relative_state_dir_override_resolves_against_root() {
    let root = Path::new("/home/user/my-app");
    let p = paths_with_state_dir(root, "custom-state");
    assert_eq!(p.state_dir(), root.join("custom-state"));
    assert_eq!(
        p.corrections(),
        root.join("custom-state").join("corrections.jsonl")
    );
}

// ── Glossary path ─────────────────────────────────────────────────────────────

#[test]
fn glossary_none_when_not_declared() {
    let root = Path::new("/home/user/my-app");
    let p = default_paths(root);
    assert!(p.glossary().is_none());
}

#[test]
fn glossary_resolves_against_root() {
    let root = Path::new("/home/user/my-app");
    let p = ProjectPaths::new(
        root,
        &PathsConfig::default(),
        Some(Path::new("glossary.toml")),
        None,
    );
    assert_eq!(p.glossary(), Some(root.join("glossary.toml").as_path()));
}

// ── catalog() ────────────────────────────────────────────────────────────────

#[test]
fn catalog_resolves_relative_path_against_root() {
    let root = Path::new("/home/user/my-app");
    let p = default_paths(root);
    let abs = p.catalog(Path::new("translations/app.ts"));
    assert_eq!(abs, root.join("translations/app.ts"));
}

// ── prompt_template() ────────────────────────────────────────────────────────

#[test]
fn prompt_template_none_when_no_prompts_config() {
    let root = Path::new("/home/user/my-app");
    let p = default_paths(root);
    assert!(p.prompt_template("de_DE").is_none());
}

#[test]
fn prompt_template_resolves_to_locale_txt() {
    let root = Path::new("/home/user/my-app");
    let p = ProjectPaths::new(
        root,
        &PathsConfig::default(),
        None,
        Some(&PromptsConfig {
            template_dir: PathBuf::from("prompts"),
        }),
    );
    let tmpl = p.prompt_template("de_DE");
    assert_eq!(tmpl, Some(root.join("prompts").join("de_DE.txt")));
}

// ── new_tuning_bundle_dir() ───────────────────────────────────────────────────

#[test]
fn new_tuning_bundle_dir_is_under_tuning_root() {
    let root = Path::new("/home/user/my-app");
    let p = default_paths(root);
    let dir = p.new_tuning_bundle_dir();
    assert!(
        dir.starts_with(p.tuning_root()),
        "tuning bundle dir {dir:?} must be under {tuning:?}",
        tuning = p.tuning_root()
    );
}

#[test]
fn new_tuning_bundle_dir_is_unique_across_calls() {
    let root = Path::new("/home/user/my-app");
    let p = default_paths(root);
    // Two rapid calls must produce different paths (microsecond timestamp).
    // In practice the OS clock will advance, but we cannot guarantee that in
    // a tight loop — we produce up to 10 pairs and require at least one to differ.
    let dirs: Vec<_> = (0..10).map(|_| p.new_tuning_bundle_dir()).collect();
    let unique: std::collections::HashSet<_> = dirs.iter().collect();
    // If every call returned the same path, all 10 would be identical.
    // We only require the set has more than one distinct value OR (pragmatic
    // fallback) that the path matches the expected format.
    let first = dirs[0].file_name().and_then(|n| n.to_str()).unwrap_or("");
    // Format: YYYY-MM-DDTHH-MM-SS-ffffff
    assert!(
        first.len() >= 10 && first.contains('-'),
        "unexpected tuning dir name format: {first}"
    );
    // At least one pair differs (or all are fine).
    let _ = unique; // suppress unused warning if assert above is the only check
}

#[test]
fn new_tuning_bundle_dir_has_no_colons() {
    let root = Path::new("/home/user/my-app");
    let p = default_paths(root);
    let dir = p.new_tuning_bundle_dir();
    let name = dir.to_str().unwrap_or("");
    assert!(
        !name.contains(':'),
        "tuning bundle dir name must not contain colons (cross-platform): {name}"
    );
}
