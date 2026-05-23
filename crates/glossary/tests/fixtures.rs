//! Integration tests for the glossary fixture directory.
//!
//! These tests:
//! - Load `fixtures/good.toml` end-to-end and assert the in-memory shape.
//! - Load each `fixtures/bad_*.toml` and assert it errors with the right
//!   variant. The bad-fixture set is the documented failure modes; if a new
//!   one lands, a fixture and an assertion go together.

use std::path::PathBuf;

use i18n_harness_glossary::{Glossary, GlossaryError, Register};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

#[test]
fn good_fixture_loads_with_expected_shape() {
    let (g, warnings) = Glossary::load(fixture("good.toml")).expect("load");
    assert!(
        warnings.is_empty(),
        "expected no warnings, got {warnings:?}"
    );
    assert_eq!(g.len(), 3);
    let open = g.term("Open").expect("Open term present");
    assert!(!open.do_not_translate);
    assert_eq!(
        open.translations.get("de_DE").map(String::as_str),
        Some("Öffnen")
    );
    let chroma = g.term("ChromaCheck").expect("ChromaCheck term present");
    assert!(chroma.do_not_translate);
    assert_eq!(g.register_for("de_DE"), Some(Register::Formal));
    assert_eq!(g.variant_for("de_DE"), Some("de_DE"));

    let dnt: Vec<_> = g.do_not_translate().collect();
    assert_eq!(dnt, vec!["ChromaCheck"]);

    let de: Vec<_> = g.terms_for("de_DE").collect();
    // Alphabetical by source: Open then Save (ChromaCheck is DNT, skipped).
    assert_eq!(de, vec![("Open", "Öffnen"), ("Save", "Speichern")]);
}

#[test]
fn round_trip_through_disk_is_identity() {
    let (g1, _) = Glossary::load(fixture("good.toml")).unwrap();
    let serialized = g1.to_toml().expect("serialize");
    let (g2, _) = Glossary::from_toml(&serialized).expect("re-parse");
    assert_eq!(g1, g2);
}

#[test]
fn missing_schema_version_is_rejected() {
    let err = Glossary::load(fixture("bad_missing_schema_version.toml")).expect_err("must fail");
    assert!(
        matches!(err, GlossaryError::MissingSchemaVersion),
        "got {err:?}",
    );
}

#[test]
fn duplicate_source_is_rejected() {
    let err = Glossary::load(fixture("bad_duplicate_source.toml")).expect_err("must fail");
    assert!(
        matches!(&err, GlossaryError::DuplicateSource { term_source } if term_source == "Open"),
        "got {err:?}",
    );
}

#[test]
fn invalid_register_is_rejected() {
    let err = Glossary::load(fixture("bad_register_value.toml")).expect_err("must fail");
    assert!(
        matches!(&err, GlossaryError::InvalidRegister { locale, value }
            if locale == "de_DE" && value == "casual"),
        "got {err:?}",
    );
}

#[test]
fn nonexistent_file_yields_io_error() {
    let err = Glossary::load(fixture("does-not-exist.toml")).expect_err("must fail");
    assert!(matches!(err, GlossaryError::Io { .. }), "got {err:?}");
}
