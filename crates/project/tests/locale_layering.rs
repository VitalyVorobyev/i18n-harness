//! Tests for `ResolvedLocale` — three-layer merge of workspace, manifest, and
//! glossary overrides.
//!
//! For each (workspace_locale, manifest_override, glossary_override) tuple the
//! tests assert the correct effective `register`, `variant`, and
//! `length_warn_ratio`, and verify that `cldr_plural`, `plural_arity`, and
//! `script` are always workspace-immutable.

use i18n_harness_glossary::Glossary;
use i18n_harness_locales::{Register, Script};
use i18n_harness_project::locale::ResolvedLocale;
use i18n_harness_project::manifest::{LocaleConfig, RegisterOverride};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn resolve_no_overrides(id: &str) -> ResolvedLocale {
    ResolvedLocale::resolve(id, None, None).unwrap_or_else(|| panic!("locale {id} must resolve"))
}

fn resolve_with_manifest(id: &str, cfg: LocaleConfig) -> ResolvedLocale {
    ResolvedLocale::resolve(id, Some(&cfg), None)
        .unwrap_or_else(|| panic!("locale {id} must resolve"))
}

fn resolve_with_glossary(id: &str, glossary_toml: &str) -> ResolvedLocale {
    let (g, _) = Glossary::from_toml(glossary_toml).expect("parse glossary");
    ResolvedLocale::resolve(id, None, Some(&g))
        .unwrap_or_else(|| panic!("locale {id} must resolve"))
}

fn resolve_all_layers(id: &str, cfg: LocaleConfig, glossary_toml: &str) -> ResolvedLocale {
    let (g, _) = Glossary::from_toml(glossary_toml).expect("parse glossary");
    ResolvedLocale::resolve(id, Some(&cfg), Some(&g))
        .unwrap_or_else(|| panic!("locale {id} must resolve"))
}

// ── Unknown locale ────────────────────────────────────────────────────────────

#[test]
fn unknown_locale_resolves_to_none() {
    assert!(ResolvedLocale::resolve("xx_XX", None, None).is_none());
}

// ── Layer 1: workspace defaults (no overrides) ────────────────────────────────

#[test]
fn de_de_workspace_defaults() {
    let r = resolve_no_overrides("de_DE");
    assert_eq!(r.id(), "de_DE");
    assert_eq!(r.register(), Register::Formal);
    assert_eq!(r.variant(), "de_DE");
    assert!((r.length_warn_ratio() - 1.4).abs() < f32::EPSILON);
    assert_eq!(r.plural_arity(), 2);
    assert_eq!(r.script(), Script::Latin);
}

#[test]
fn en_workspace_defaults() {
    let r = resolve_no_overrides("en");
    assert_eq!(r.id(), "en");
    assert_eq!(r.register(), Register::Neutral);
    assert!((r.length_warn_ratio() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn zh_hans_workspace_defaults() {
    let r = resolve_no_overrides("zh_Hans");
    assert_eq!(r.plural_arity(), 1);
    assert_eq!(r.script(), Script::Han);
    assert_eq!(r.register(), Register::Neutral);
}

// ── Layer 2: manifest overrides ───────────────────────────────────────────────

#[test]
fn manifest_overrides_register() {
    let cfg = LocaleConfig {
        register: Some(RegisterOverride::Informal),
        variant: None,
        length_warn_ratio: None,
    };
    let r = resolve_with_manifest("de_DE", cfg);
    // Manifest says informal; workspace says formal.
    assert_eq!(r.register(), Register::Informal);
    // Other workspace fields unchanged.
    assert!((r.length_warn_ratio() - 1.4).abs() < f32::EPSILON);
    assert_eq!(r.script(), Script::Latin);
}

#[test]
fn manifest_overrides_variant() {
    let cfg = LocaleConfig {
        register: None,
        variant: Some("de_AT".to_owned()),
        length_warn_ratio: None,
    };
    let r = resolve_with_manifest("de_DE", cfg);
    assert_eq!(r.variant(), "de_AT");
    // Register unchanged.
    assert_eq!(r.register(), Register::Formal);
}

#[test]
fn manifest_overrides_length_warn_ratio() {
    let cfg = LocaleConfig {
        register: None,
        variant: None,
        length_warn_ratio: Some(1.1),
    };
    let r = resolve_with_manifest("de_DE", cfg);
    assert!((r.length_warn_ratio() - 1.1).abs() < f32::EPSILON);
}

#[test]
fn manifest_partial_override_leaves_other_fields_as_workspace() {
    let cfg = LocaleConfig {
        register: Some(RegisterOverride::Neutral),
        variant: None,
        length_warn_ratio: None,
    };
    let r = resolve_with_manifest("de_DE", cfg);
    assert_eq!(r.register(), Register::Neutral);
    // variant and length_warn_ratio come from workspace.
    assert_eq!(r.variant(), "de_DE");
    assert!((r.length_warn_ratio() - 1.4).abs() < f32::EPSILON);
}

// ── Layer 3: glossary overrides ───────────────────────────────────────────────

#[test]
fn glossary_overrides_register() {
    let glossary_toml = r#"
[meta]
schema_version = 1

[locale.de_DE]
register = "informal"
"#;
    let r = resolve_with_glossary("de_DE", glossary_toml);
    assert_eq!(r.register(), Register::Informal);
}

#[test]
fn glossary_overrides_variant() {
    let glossary_toml = r#"
[meta]
schema_version = 1

[locale.de_DE]
variant = "de_AT"
"#;
    let r = resolve_with_glossary("de_DE", glossary_toml);
    assert_eq!(r.variant(), "de_AT");
}

#[test]
fn glossary_cannot_override_length_warn_ratio() {
    // Glossary has no length_warn_ratio field; workspace value should survive.
    let glossary_toml = r#"
[meta]
schema_version = 1

[locale.de_DE]
register = "informal"
"#;
    let r = resolve_with_glossary("de_DE", glossary_toml);
    assert!((r.length_warn_ratio() - 1.4).abs() < f32::EPSILON);
}

// ── Immutable fields ──────────────────────────────────────────────────────────

#[test]
fn cldr_plural_is_workspace_immutable() {
    // Even with all overrides applied, cldr_plural comes from workspace.
    let cfg = LocaleConfig {
        register: Some(RegisterOverride::Informal),
        variant: Some("de_AT".to_owned()),
        length_warn_ratio: Some(1.1),
    };
    let glossary_toml = r#"
[meta]
schema_version = 1

[locale.de_DE]
register = "informal"
variant = "de_AT"
"#;
    let r = resolve_all_layers("de_DE", cfg, glossary_toml);
    // Workspace: [One, Other]
    assert_eq!(r.plural_arity(), 2);
    use i18n_harness_locales::PluralCategory;
    assert_eq!(
        r.cldr_plural(),
        &[PluralCategory::One, PluralCategory::Other]
    );
}

#[test]
fn script_is_workspace_immutable() {
    let cfg = LocaleConfig {
        register: Some(RegisterOverride::Informal),
        variant: None,
        length_warn_ratio: None,
    };
    let r = resolve_with_manifest("de_DE", cfg);
    assert_eq!(r.script(), Script::Latin);
}

// ── Priority: glossary > manifest > workspace ─────────────────────────────────

#[test]
fn glossary_wins_over_manifest_register() {
    let cfg = LocaleConfig {
        register: Some(RegisterOverride::Formal),
        variant: None,
        length_warn_ratio: None,
    };
    let glossary_toml = r#"
[meta]
schema_version = 1

[locale.de_DE]
register = "informal"
"#;
    // Manifest says formal, glossary says informal → informal wins.
    let r = resolve_all_layers("de_DE", cfg, glossary_toml);
    assert_eq!(r.register(), Register::Informal);
}

#[test]
fn glossary_wins_over_manifest_variant() {
    let cfg = LocaleConfig {
        register: None,
        variant: Some("de_AT".to_owned()),
        length_warn_ratio: None,
    };
    let glossary_toml = r#"
[meta]
schema_version = 1

[locale.de_DE]
variant = "de_CH"
"#;
    // Manifest says de_AT, glossary says de_CH → de_CH wins.
    let r = resolve_all_layers("de_DE", cfg, glossary_toml);
    assert_eq!(r.variant(), "de_CH");
}

#[test]
fn manifest_wins_over_workspace_when_no_glossary() {
    let cfg = LocaleConfig {
        register: Some(RegisterOverride::Neutral),
        variant: None,
        length_warn_ratio: Some(1.2),
    };
    let r = resolve_with_manifest("de_DE", cfg);
    assert_eq!(r.register(), Register::Neutral);
    assert!((r.length_warn_ratio() - 1.2).abs() < f32::EPSILON);
}

// ── workspace_locale() caveat ─────────────────────────────────────────────────

#[test]
fn workspace_locale_carries_workspace_register_not_resolved() {
    let cfg = LocaleConfig {
        register: Some(RegisterOverride::Informal),
        variant: None,
        length_warn_ratio: None,
    };
    let r = resolve_with_manifest("de_DE", cfg);
    // resolved register is Informal.
    assert_eq!(r.register(), Register::Informal);
    // workspace_locale() still shows Formal (the workspace value).
    assert_eq!(r.workspace_locale().register, Register::Formal);
}
