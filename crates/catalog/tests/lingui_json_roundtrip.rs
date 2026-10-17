//! Lingui JSON catalogs: entries of `message` / `translation`; only the
//! translation is ever rewritten, and untouched files round-trip exactly.

use std::path::Path;

use i18n_harness_catalog::{CatalogFormat, IcuJsonFormat, LinguiJsonFormat};
use i18n_harness_core::{Target, UnitState};

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/lingui-json/de.json")
}

#[test]
fn round_trips_byte_identically() {
    let doc = LinguiJsonFormat.open(&fixture()).unwrap();
    assert_eq!(doc.format().id(), "lingui-json");
    assert!(doc.render(doc.units()).unwrap() == doc.source_bytes());
}

#[test]
fn units_take_the_message_as_source_and_the_translation_as_target() {
    let doc = LinguiJsonFormat.open(&fixture()).unwrap();
    let ids: Vec<&str> = doc.units().iter().map(|u| u.id.0.as_str()).collect();
    assert_eq!(
        ids,
        [
            "Hello, {name}!",
            "inbox.count",
            "Uploaded {percent}%",
            "header.title"
        ]
    );
    let hello = &doc.units()[0];
    assert_eq!(hello.source, "Hello, {name}!");
    assert_eq!(
        hello.target,
        Target::Singular {
            text: Some("Hallo, {name}!".into())
        }
    );
    let upload = &doc.units()[2];
    assert_eq!(upload.state, UnitState::Untranslated);
    // No `message`: the explicit id stands in for the source.
    assert_eq!(doc.units()[3].source, "header.title");
}

#[test]
fn only_the_translation_value_is_rewritten() {
    let mut doc = LinguiJsonFormat.open(&fixture()).unwrap();
    let unit = &mut doc.units_mut()[2];
    unit.target = Target::Singular {
        text: Some("{percent}% hochgeladen".into()),
    };
    unit.state = UnitState::Finished;
    let written = String::from_utf8(doc.render(doc.units()).unwrap()).unwrap();
    assert!(
        written.contains(r#""translation": "{percent}% hochgeladen""#),
        "{written}"
    );
    assert!(written.contains(r#""message": "Uploaded {percent}%""#));
    let original = std::fs::read_to_string(fixture()).unwrap();
    assert_eq!(written.lines().count(), original.lines().count());
}

#[test]
fn sniffing_tells_lingui_json_from_flat_icu_json() {
    let head = std::fs::read(fixture()).unwrap();
    assert!(LinguiJsonFormat.sniff(&fixture(), &head).is_some());
    assert!(IcuJsonFormat.sniff(&fixture(), &head).is_none());
    let flat = br#"{"greeting": "Hallo {name}"}"#;
    assert!(LinguiJsonFormat.sniff(&fixture(), flat).is_none());
}
