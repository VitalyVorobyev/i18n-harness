//! Round-trip integration test: every fixture under `fixtures/qt/` must
//! satisfy `apply(extract(f), f.units(), out) == f` byte-for-byte.
//!
//! This is the M0 contract. If a fixture is added that does not round-trip,
//! the test fails — do not relax the assertion; either fix the adapter or
//! document the limitation in `fixtures/README.md` and add a hand-crafted
//! variant that excludes the offending construct.

use std::path::{Path, PathBuf};

use i18n_harness_adapter_qt::{extract, render};

fn fixtures_dir() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest)
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .join("fixtures")
        .join("qt")
}

fn all_ts_fixtures() -> Vec<PathBuf> {
    let dir = fixtures_dir();
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("ts"))
        .collect();
    out.sort();
    out
}

#[test]
fn fixture_dir_is_not_empty() {
    let fixtures = all_ts_fixtures();
    assert!(
        !fixtures.is_empty(),
        "fixtures/qt/ has no .ts files; round-trip suite would silently pass",
    );
}

#[test]
fn extract_then_render_with_no_changes_is_byte_identical() {
    for fixture in all_ts_fixtures() {
        let catalog =
            extract(&fixture).unwrap_or_else(|e| panic!("extract({}): {e}", fixture.display()));
        let rendered =
            render(&catalog, &[]).unwrap_or_else(|e| panic!("render({}): {e}", fixture.display()));
        let original = std::fs::read(&fixture)
            .unwrap_or_else(|e| panic!("re-read {}: {e}", fixture.display()));
        if rendered != original {
            let diff = byte_diff_summary(&original, &rendered);
            panic!("round-trip differs for {}:\n{}", fixture.display(), diff,);
        }
    }
}

#[test]
fn extract_recognises_message_states() {
    use i18n_harness_core::UnitState;
    let fixture = fixtures_dir().join("showcase.ts");
    let catalog = extract(&fixture).expect("extract showcase");
    let states: Vec<UnitState> = catalog.units().iter().map(|u| u.state).collect();
    assert!(
        states.contains(&UnitState::Untranslated),
        "showcase should include untranslated units; got {states:?}",
    );
    assert!(
        states.contains(&UnitState::Vanished),
        "showcase should include a vanished unit; got {states:?}",
    );
    assert!(
        states.contains(&UnitState::Obsolete),
        "showcase should include an obsolete unit; got {states:?}",
    );
}

#[test]
fn vanished_and_obsolete_are_never_modified_even_when_target_changes() {
    use i18n_harness_core::{Target, UnitState};
    let fixture = fixtures_dir().join("showcase.ts");
    let catalog = extract(&fixture).expect("extract");
    let mut units = catalog.units().to_vec();
    // Attempt to mutate every unit, regardless of state.
    for u in &mut units {
        u.target = match &u.target {
            Target::Singular { .. } => Target::Singular {
                text: Some("MUTATED".into()),
            },
            Target::Plural { forms } => Target::Plural {
                forms: vec![Some("MUTATED".into()); forms.len()],
            },
        };
        u.state = UnitState::Finished;
    }
    let rendered = render(&catalog, &units).expect("render");

    // The original vanished and obsolete bodies must survive verbatim.
    let original = std::fs::read(&fixture).expect("re-read");
    let vanished_marker = b"<translation type=\"vanished\">Veralteter Importpfad</translation>";
    let obsolete_marker = b"<translation type=\"obsolete\">Alter Startbildschirmtext</translation>";
    assert!(
        contains_subslice(&original, vanished_marker),
        "fixture sanity"
    );
    assert!(
        contains_subslice(&original, obsolete_marker),
        "fixture sanity"
    );
    assert!(
        contains_subslice(&rendered, vanished_marker),
        "vanished unit was modified",
    );
    assert!(
        contains_subslice(&rendered, obsolete_marker),
        "obsolete unit was modified",
    );
}

#[test]
fn filling_an_untranslated_unit_promotes_state_to_finished() {
    use i18n_harness_core::Target;
    let fixture = fixtures_dir().join("showcase.ts");
    let catalog = extract(&fixture).expect("extract");
    let mut units = catalog.units().to_vec();
    // Find the "Hello" unit and fill it.
    let hello = units
        .iter_mut()
        .find(|u| u.source == "Hello")
        .expect("Hello unit");
    hello.target = Target::Singular {
        text: Some("Hallo".into()),
    };

    let rendered = render(&catalog, &units).expect("render");
    let rendered_str = std::str::from_utf8(&rendered).expect("utf8");
    assert!(
        rendered_str.contains("<translation>Hallo</translation>"),
        "expected <translation>Hallo</translation> in:\n{rendered_str}",
    );
    assert!(
        !rendered_str.contains("type=\"unfinished\">Hallo"),
        "type=\"unfinished\" should be stripped when promoting to finished",
    );
}

#[test]
fn round_trip_after_applying_changes_is_itself_round_trippable() {
    use i18n_harness_core::Target;
    let fixture = fixtures_dir().join("showcase.ts");
    let catalog = extract(&fixture).expect("extract");
    let mut units = catalog.units().to_vec();
    for u in &mut units {
        if let Target::Singular { text } = &u.target
            && text.is_none()
        {
            u.target = Target::Singular {
                text: Some("X".into()),
            };
        }
    }
    let rendered = render(&catalog, &units).expect("render");

    // Write to a temp file, re-extract, render with no changes, expect
    // byte identity to the previous render.
    let tmp = std::env::temp_dir().join("i18n_harness_qt_roundtrip.ts");
    std::fs::write(&tmp, &rendered).expect("write tmp");
    let catalog2 = extract(&tmp).expect("re-extract");
    let rendered2 = render(&catalog2, &[]).expect("re-render");
    assert_eq!(
        rendered, rendered2,
        "second round-trip diverged; fixed-point property violated",
    );
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn extract_normalizes_placeholders_to_icu() {
    let fixture = fixtures_dir().join("showcase.ts");
    let catalog = extract(&fixture).expect("extract showcase");
    let sources: Vec<&str> = catalog.units().iter().map(|u| u.source.as_str()).collect();
    assert!(
        sources
            .iter()
            .any(|s| s.contains("{0}") && s.contains("{1}")),
        "expected at least one source with {{0}} and {{1}} from %1/%2; got {sources:?}",
    );
    assert!(
        sources.iter().any(|s| s.contains("{count}")),
        "expected at least one source with {{count}} from %n; got {sources:?}",
    );
}

/// XML entity references in `<translation>` body bodies must round-trip
/// through `Unit::target` correctly. The singular body parser previously
/// dropped `&lt;`/`&gt;`/`&amp;` GeneralRef events, so a target containing
/// `&lt;b&gt;Speichern&lt;/b&gt;` came out as `bSpeichern/b` (no brackets).
/// The plural body parser uses a different code path (full byte slice +
/// `unescape_xml`) and was always correct; this test pins both shapes.
#[test]
fn xml_entity_references_in_translation_body_are_decoded() {
    let dir = std::env::temp_dir().join(format!("i18n-translation-decode-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("entity.ts");
    let src = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>X</name>
    <message>
        <source>Click &lt;b&gt;Save&lt;/b&gt;.</source>
        <translation>Klicken Sie auf &lt;b&gt;Speichern&lt;/b&gt;.</translation>
    </message>
</context>
</TS>
"#;
    std::fs::write(&path, src).unwrap();
    let cat = extract(&path).expect("extract");
    let unit = &cat.units()[0];
    assert_eq!(unit.source, "Click <b>Save</b>.");
    match &unit.target {
        i18n_harness_core::Target::Singular { text: Some(t) } => {
            assert_eq!(t, "Klicken Sie auf <b>Speichern</b>.");
        }
        other => panic!("expected Singular Some, got {other:?}"),
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// XML entity references in `<source>` and `<comment>` bodies (`&amp;`,
/// `&lt;`, `&gt;`, `&quot;`, `&apos;`) must be decoded into their literal
/// characters in `Unit::source` — otherwise the backend prompt loses the
/// accelerator marker (`&File` → `File`) and the gate cannot fire its
/// accel-mismatch rule. Discovered by the first Gemma 4 measurement run.
#[test]
fn xml_entity_references_in_source_are_decoded() {
    let fixture = fixtures_dir().join("showcase.ts");
    let cat = extract(&fixture).expect("extract");

    let file_unit = cat
        .units()
        .iter()
        .find(|u| u.id.as_str() == "MainWindow::&File::menu")
        .unwrap_or_else(|| {
            let ids: Vec<&str> = cat.units().iter().map(|u| u.id.as_str()).collect();
            panic!("expected `MainWindow::&File::menu` in unit ids: {ids:?}")
        });
    assert_eq!(
        file_unit.source, "&File",
        "source must contain the literal `&` accelerator marker, not just `File`",
    );

    let html_unit = cat
        .units()
        .iter()
        .find(|u| u.source.contains("Click"))
        .expect("expected a unit whose source starts with `Click`");
    assert_eq!(
        html_unit.source, "Click <b>Save</b> to continue.",
        "source must preserve `<b>` / `</b>` (encoded as `&lt;` / `&gt;` in XML)",
    );
}

fn byte_diff_summary(a: &[u8], b: &[u8]) -> String {
    let common = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let line = a[..common].iter().filter(|&&b| b == b'\n').count() + 1;
    let col = common
        - a[..common]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |p| p + 1);
    let snippet_len = 80;
    let a_snip = String::from_utf8_lossy(&a[common..a.len().min(common + snippet_len)]).to_string();
    let b_snip = String::from_utf8_lossy(&b[common..b.len().min(common + snippet_len)]).to_string();
    format!(
        "  diverge at byte {common} (line {line}, col {col})\n  original len: {}\n  rendered len: {}\n  original: {:?}\n  rendered: {:?}",
        a.len(),
        b.len(),
        a_snip,
        b_snip,
    )
}
