//! Smoke test for the v1 prompt-template file shipped with this crate.
//!
//! The template body itself is checked into `prompts/`; this test loads
//! it, renders against a representative [`PromptContext`], and asserts
//! the rendered prompt:
//! - Has the template version line baked in (so a metrics consumer can
//!   correlate model outputs with the template revision).
//! - Substitutes every documented v1 token.
//! - Does NOT contain any `{?...}` unknown-token markers (the prompt
//!   uses only documented tokens).
//!
//! When the implementer adds the Ollama template, they add an analogous
//! test for it next to this one.

use std::path::PathBuf;

use i18n_harness_backend::PromptContext;
use i18n_harness_backend::prompt::PromptTemplate;
use i18n_harness_core::{FlagSet, Unit};
use i18n_harness_glossary::{Glossary, Register};
use i18n_harness_locales::Locale;

fn template_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("prompts")
        .join(name)
}

fn load(name: &str) -> PromptTemplate {
    let body = std::fs::read_to_string(template_path(name)).expect("read template");
    // Convention: file name `<backend>-<purpose>-vN.txt`; version is the
    // `v<N>` segment.
    let version = name
        .rsplit_once('-')
        .and_then(|(_, ver_ext)| ver_ext.split('.').next())
        .unwrap_or("v?")
        .to_owned();
    PromptTemplate::new(body, version)
}

#[test]
fn manual_translate_v1_renders_against_realistic_context() {
    let tpl = load("manual-translate-v1.txt");

    let toml = r#"
[meta]
schema_version = 1

[[term]]
source = "Open"
[term.translations]
de_DE = "Öffnen"

[[term]]
source = "Save"
[term.translations]
de_DE = "Speichern"

[[term]]
source = "ChromaCheck"
do_not_translate = true

[locale.de_DE]
register = "formal"
"#;
    let (g, _) = Glossary::from_toml(toml).unwrap();

    let unit = Unit::untranslated_singular("greet::1", "Open the requested document");
    let locale = Locale::by_id("de_DE").unwrap();
    let flags = FlagSet::new();
    let ctx = PromptContext::new(&unit, locale, Register::Formal, Some(&g), &flags);
    let rendered = tpl.render(&ctx);

    assert!(
        rendered.contains("[template=v1]"),
        "template version line missing: {rendered}"
    );
    assert!(rendered.contains("Translate the following user-interface string into de_DE."));
    assert!(rendered.contains("Use the formal register."));
    assert!(rendered.contains("Open -> Öffnen"));
    assert!(rendered.contains("Save -> Speichern"));
    assert!(rendered.contains("- ChromaCheck"));
    assert!(rendered.contains("Source:\nOpen the requested document"));
    assert!(
        !rendered.contains("{?"),
        "rendered prompt contains unknown-token marker, all v1 tokens must be documented: {rendered}"
    );
}

#[test]
fn manual_translate_v1_uses_none_placeholder_when_glossary_empty() {
    let tpl = load("manual-translate-v1.txt");
    let unit = Unit::untranslated_singular("x", "Hello");
    let locale = Locale::by_id("de_DE").unwrap();
    let flags = FlagSet::new();
    let ctx = PromptContext::new(&unit, locale, Register::Formal, None, &flags);
    let rendered = tpl.render(&ctx);
    assert!(
        rendered.contains("(none)"),
        "(none) marker missing: {rendered}"
    );
}
