//! Lingui PO: ICU messages inside a PO file. Every fixture round-trips
//! byte-identically, `{…}` stays ICU, and `%` stays a literal character.

use std::path::{Path, PathBuf};

use i18n_harness_catalog::{CatalogFormat, LinguiPoFormat, PoFormat};
use i18n_harness_core::{Target, UnitState};

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/lingui-po");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "po"))
        .collect();
    out.sort();
    assert!(!out.is_empty(), "no Lingui fixtures");
    out
}

#[test]
fn every_fixture_round_trips_byte_identically() {
    for fixture in fixtures() {
        let doc = LinguiPoFormat.open(&fixture).unwrap();
        assert_eq!(doc.format().id(), "lingui-po");
        assert!(
            doc.render(doc.units()).unwrap() == doc.source_bytes(),
            "{}",
            fixture.display()
        );
    }
}

#[test]
fn sniffing_prefers_lingui_over_plain_gettext() {
    let path = fixtures().into_iter().next().unwrap();
    let head = std::fs::read(&path).unwrap();
    let lingui = LinguiPoFormat.sniff(&path, &head).unwrap();
    let gettext = PoFormat.sniff(&path, &head).unwrap();
    assert!(lingui.confidence > gettext.confidence);
    assert_eq!(lingui.language.as_deref(), Some("de"));
}

#[test]
fn messages_stay_icu_and_percent_stays_literal() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/lingui-po/de.po");
    let mut doc = LinguiPoFormat.open(&path).unwrap();
    let sources: Vec<&str> = doc.units().iter().map(|u| u.source.as_str()).collect();
    assert!(sources.contains(&"Hello, {name}!"));
    assert!(sources.contains(&"Uploaded {percent}%"));
    assert!(
        sources.iter().any(|s| s.starts_with("{count, plural,")),
        "{sources:?}"
    );

    // Translate the empty one; `%` and `{percent}` are written as they are.
    let unit = doc
        .units_mut()
        .iter_mut()
        .find(|u| u.source == "Uploaded {percent}%")
        .unwrap();
    unit.target = Target::Singular {
        text: Some("{percent}% hochgeladen".into()),
    };
    unit.state = UnitState::Finished;
    let written = String::from_utf8(doc.render(doc.units()).unwrap()).unwrap();
    assert!(
        written.contains("msgstr \"{percent}% hochgeladen\""),
        "{written}"
    );
    assert!(
        !written.contains("%%"),
        "a literal % must not be escaped: {written}"
    );
}
