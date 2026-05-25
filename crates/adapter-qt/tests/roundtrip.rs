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
fn extract_reads_language_from_ts_root() {
    let cases = [
        ("showcase.ts", "de_DE"),
        ("es_ES.ts", "es_ES"),
        ("zh_Hans.ts", "zh_Hans"),
    ];
    for (name, expected) in cases {
        let fixture = fixtures_dir().join(name);
        let catalog = extract(&fixture).expect("extract");
        assert_eq!(
            catalog.language(),
            Some(expected),
            "{name} should declare language={expected}",
        );
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
    // The "Verbatim block" unit has `type="unfinished"` plus a non-empty
    // CDATA body, which the parse contract maps to `Proposed` (mirror of
    // the writer's `Proposed → keep type="unfinished"` behaviour).
    assert!(
        states.contains(&UnitState::Proposed),
        "showcase should include a proposed unit (unfinished + non-empty body); got {states:?}",
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

// ── Source-hash tests ─────────────────────────────────────────────────────────

/// Every active (non-vanished, non-obsolete) unit in the showcase has a hash.
#[test]
fn active_units_have_source_hash() {
    use i18n_harness_core::UnitState;
    let fixture = fixtures_dir().join("showcase.ts");
    let catalog = extract(&fixture).expect("extract");
    for unit in catalog.units() {
        if matches!(unit.state, UnitState::Vanished | UnitState::Obsolete) {
            assert!(
                unit.source_hash.is_none(),
                "vanished/obsolete unit {:?} should have no hash",
                unit.id
            );
        } else {
            assert!(
                unit.source_hash.is_some(),
                "active unit {:?} should have a source_hash",
                unit.id
            );
        }
    }
}

/// Vanished and obsolete units must have `source_hash = None`.
#[test]
fn vanished_unit_has_no_source_hash() {
    use i18n_harness_core::UnitState;
    let fixture = fixtures_dir().join("showcase.ts");
    let catalog = extract(&fixture).expect("extract");
    let vanished = catalog
        .units()
        .iter()
        .find(|u| u.state == UnitState::Vanished)
        .expect("showcase must have a vanished unit");
    assert!(
        vanished.source_hash.is_none(),
        "vanished unit must have source_hash = None"
    );
}

#[test]
fn obsolete_unit_has_no_source_hash() {
    use i18n_harness_core::UnitState;
    let fixture = fixtures_dir().join("showcase.ts");
    let catalog = extract(&fixture).expect("extract");
    let obsolete = catalog
        .units()
        .iter()
        .find(|u| u.state == UnitState::Obsolete)
        .expect("showcase must have an obsolete unit");
    assert!(
        obsolete.source_hash.is_none(),
        "obsolete unit must have source_hash = None"
    );
}

/// A unit with an extracomment produces a hash that differs from the same
/// unit without an extracomment.
#[test]
fn extracomment_changes_hash() {
    use i18n_harness_core::compute_source_hash;
    let hash_with = compute_source_hash("Save", "", "Button label in the main toolbar.", false);
    let hash_without = compute_source_hash("Save", "", "", false);
    assert_ne!(
        hash_with, hash_without,
        "hash with extracomment must differ from hash without"
    );
}

/// The extracomment fixture's "Save" unit gets a hash matching the expected
/// value (validates the parser is capturing extracomment text).
#[test]
fn extracomment_fixture_hash_matches_expected() {
    use i18n_harness_core::compute_source_hash;
    let fixture = fixtures_dir().join("extracomment.ts");
    let catalog = extract(&fixture).expect("extract extracomment fixture");
    let save_unit = catalog
        .units()
        .iter()
        .find(|u| u.source == "Save")
        .expect("Save unit must be present");
    let expected = compute_source_hash("Save", "", "Button label in the main toolbar.", false);
    assert_eq!(
        save_unit.source_hash.as_deref(),
        Some(expected.as_str()),
        "Save unit hash must include extracomment text"
    );
}

/// Two `<extracomment>` blocks are concatenated with `\n` before hashing.
#[test]
fn multi_extracomment_concatenated_with_newline() {
    use i18n_harness_core::compute_source_hash;
    let fixture = fixtures_dir().join("extracomment.ts");
    let catalog = extract(&fixture).expect("extract extracomment fixture");
    let open_unit = catalog
        .units()
        .iter()
        .find(|u| u.source == "Open")
        .expect("Open unit must be present");
    let expected = compute_source_hash(
        "Open",
        "",
        "First developer note.\nSecond developer note.",
        false,
    );
    assert_eq!(
        open_unit.source_hash.as_deref(),
        Some(expected.as_str()),
        "Open unit hash must reflect concatenated extracomment blocks"
    );
}

/// A plural unit and a singular unit with the same source text have different
/// hashes (the `plural` flag distinguishes them).
#[test]
fn plural_hash_differs_from_singular() {
    use i18n_harness_core::compute_source_hash;
    let fixture = fixtures_dir().join("extracomment.ts");
    let catalog = extract(&fixture).expect("extract");
    // "%n item(s)" is plural in the fixture; "Save" is singular.
    // The source text differs, so compare via compute_source_hash directly.
    let singular_hash = compute_source_hash("msg", "", "", false);
    let plural_hash = compute_source_hash("msg", "", "", true);
    assert_ne!(
        singular_hash, plural_hash,
        "plural and singular hash of same text must differ"
    );

    // Also verify from the fixture: the plural unit's hash reflects plural=true.
    let plural_unit = catalog
        .units()
        .iter()
        .find(|u| u.source.contains("{count}"))
        .expect("plural unit with {count} source must be present");
    let expected = compute_source_hash(&plural_unit.source, "", "Shown in the status bar.", true);
    assert_eq!(
        plural_unit.source_hash.as_deref(),
        Some(expected.as_str()),
        "plural unit hash must encode plural=true"
    );
}

/// The extracomment fixture vanished/obsolete units must have `source_hash = None`.
#[test]
fn extracomment_fixture_vanished_obsolete_no_hash() {
    use i18n_harness_core::UnitState;
    let fixture = fixtures_dir().join("extracomment.ts");
    let catalog = extract(&fixture).expect("extract");
    for unit in catalog.units() {
        if matches!(unit.state, UnitState::Vanished | UnitState::Obsolete) {
            assert!(
                unit.source_hash.is_none(),
                "vanished/obsolete unit {:?} must have source_hash = None",
                unit.id
            );
        }
    }
}

// ── Proposed-state mapping tests ──────────────────────────────────────────────
//
// The parse contract must mirror the write contract: the writer keeps
// `type="unfinished"` on `Proposed` units (gate-flagged, awaiting human
// review). The parser must therefore read `type="unfinished"` plus a
// non-empty body as `Proposed`, not `Untranslated`. Without this symmetry,
// the UI's UntranslatedDraftEditor renders such units as a blank textarea
// and the user perceives total data loss across a close/reopen cycle.
//
// Empty bodies stay `Untranslated`; vanished/obsolete are unaffected.

/// Singular `<translation type="unfinished">non-empty</translation>` must
/// parse as `Proposed` with the body preserved in `Target::Singular`.
#[test]
fn unfinished_with_nonempty_singular_body_parses_as_proposed() {
    use i18n_harness_core::{Target, UnitState};
    let dir = std::env::temp_dir().join(format!(
        "i18n-proposed-singular-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("proposed.ts");
    // Two singular cases: one with an empty body (must stay Untranslated)
    // and one with a non-empty body (must promote to Proposed).
    let src = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Dlg</name>
    <message>
        <source>Hello</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <source>Save</source>
        <translation type="unfinished">Speichern</translation>
    </message>
</context>
</TS>
"#;
    std::fs::write(&path, src).unwrap();
    let catalog = extract(&path).expect("extract");
    let hello = catalog
        .units()
        .iter()
        .find(|u| u.source == "Hello")
        .expect("Hello unit");
    assert_eq!(
        hello.state,
        UnitState::Untranslated,
        "empty body must stay Untranslated; got {:?}",
        hello.state,
    );
    assert!(
        matches!(hello.target, Target::Singular { text: None }),
        "empty body must surface as Target::Singular {{ text: None }}; got {:?}",
        hello.target,
    );

    let save = catalog
        .units()
        .iter()
        .find(|u| u.source == "Save")
        .expect("Save unit");
    assert_eq!(
        save.state,
        UnitState::Proposed,
        "type=\"unfinished\" + non-empty body must parse as Proposed; got {:?}",
        save.state,
    );
    match &save.target {
        Target::Singular { text: Some(t) } => {
            assert_eq!(t, "Speichern");
        }
        other => panic!("expected Target::Singular {{ Some(\"Speichern\") }}, got {other:?}"),
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// Plural `<translation type="unfinished">` whose forms have any non-empty
/// body must parse as `Proposed` with each form preserved.
#[test]
fn unfinished_with_nonempty_plural_form_parses_as_proposed() {
    use i18n_harness_core::{Target, UnitState};
    let dir = std::env::temp_dir().join(format!(
        "i18n-proposed-plural-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("proposed_plural.ts");
    // One plural with empty forms (stays Untranslated), one with one
    // non-empty form (promotes to Proposed).
    let src = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>P</name>
    <message numerus="yes">
        <source>%n empty(s)</source>
        <translation type="unfinished">
            <numerusform></numerusform>
            <numerusform></numerusform>
        </translation>
    </message>
    <message numerus="yes">
        <source>%n item(s)</source>
        <translation type="unfinished">
            <numerusform>%n Eintrag</numerusform>
            <numerusform>%n Einträge</numerusform>
        </translation>
    </message>
</context>
</TS>
"#;
    std::fs::write(&path, src).unwrap();
    let catalog = extract(&path).expect("extract");

    let empty = catalog
        .units()
        .iter()
        .find(|u| u.source.contains("empty"))
        .expect("empty plural unit");
    assert_eq!(
        empty.state,
        UnitState::Untranslated,
        "all-empty plural forms must stay Untranslated; got {:?}",
        empty.state,
    );
    match &empty.target {
        Target::Plural { forms } => {
            assert!(
                forms.iter().all(|f| f.is_none()),
                "empty plural forms must each be None; got {forms:?}",
            );
        }
        other => panic!("expected Plural, got {other:?}"),
    }

    let filled = catalog
        .units()
        .iter()
        .find(|u| u.source.contains("item"))
        .expect("filled plural unit");
    assert_eq!(
        filled.state,
        UnitState::Proposed,
        "type=\"unfinished\" + non-empty plural form must parse as Proposed; got {:?}",
        filled.state,
    );
    match &filled.target {
        Target::Plural { forms } => {
            assert_eq!(forms.len(), 2);
            // Forms carry over verbatim (with %n normalised to {count} by
            // the placeholder converter).
            assert_eq!(forms[0].as_deref(), Some("{count} Eintrag"));
            assert_eq!(forms[1].as_deref(), Some("{count} Einträge"));
        }
        other => panic!("expected Plural, got {other:?}"),
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// The CDATA-bearing "Verbatim block" unit in `showcase.ts` is the existing
/// real-world case: `type="unfinished"` with non-empty CDATA body. It must
/// parse as `Proposed`, not `Untranslated`.
#[test]
fn showcase_verbatim_block_with_cdata_parses_as_proposed() {
    use i18n_harness_core::{Target, UnitState};
    let fixture = fixtures_dir().join("showcase.ts");
    let catalog = extract(&fixture).expect("extract showcase");
    let verbatim = catalog
        .units()
        .iter()
        .find(|u| u.source == "Verbatim block")
        .expect("Verbatim block unit");
    assert_eq!(
        verbatim.state,
        UnitState::Proposed,
        "CDATA body with type=\"unfinished\" must be Proposed; got {:?}",
        verbatim.state,
    );
    match &verbatim.target {
        Target::Singular { text: Some(t) } => {
            // CDATA content is preserved verbatim — the `&` and `<` here are
            // *literal* characters (CDATA suppresses XML interpretation).
            assert_eq!(t, "A & B < C");
        }
        other => panic!("expected Target::Singular with text, got {other:?}"),
    }
}

/// Symmetry contract: parsing a file that contains `Proposed` units (i.e.
/// `type="unfinished"` + non-empty body), then re-rendering with those
/// units unchanged, must produce byte-identical output. This is the path
/// "user closes the app and reopens it" exercises, and it must not corrupt
/// the file.
#[test]
fn proposed_roundtrip_is_byte_identical() {
    let dir = std::env::temp_dir().join(format!(
        "i18n-proposed-rt-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("proposed_rt.ts");
    let src = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>RT</name>
    <message>
        <source>Save</source>
        <translation type="unfinished">Speichern</translation>
    </message>
    <message numerus="yes">
        <source>%n item(s)</source>
        <translation type="unfinished">
            <numerusform>%n Eintrag</numerusform>
            <numerusform>%n Einträge</numerusform>
        </translation>
    </message>
</context>
</TS>
"#;
    std::fs::write(&path, src).unwrap();
    let catalog = extract(&path).expect("extract");
    // Re-render with zero changes to the units; bytes must match.
    let rendered = render(&catalog, &[]).expect("render");
    let original = std::fs::read(&path).expect("re-read");
    assert_eq!(
        rendered, original,
        "Proposed unit round-trip must be byte-identical",
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// Regression test for the "Save All silently no-ops" failure mode.
///
/// The buggy pattern (the one the Tauri shell uses):
///   1. `extract(path)` → `Catalog` with mutable `units` and pristine bytes.
///   2. Caller mutates `catalog.units_mut()[i].target` in place.
///   3. Caller passes `catalog.units().to_vec()` back as the `units` arg
///      to `apply`/`render`.
///
/// Before the fix, `plan_edits_for_unit` short-circuited on
/// `candidate.target == original.target` where `original` was the same
/// in-place-mutated unit — so the comparison was trivially true and the
/// edit never landed. The render output equaled the source bytes.
/// `apply` wrote the source bytes back. Disk contents were unchanged.
///
/// This test pins the contract: in-place mutation MUST round-trip
/// through `render` as a real edit, byte-visible in the output.
#[test]
fn in_place_mutation_then_render_emits_the_edit() {
    use i18n_harness_core::Target;

    let dir = std::env::temp_dir().join(format!(
        "i18n-inplace-rt-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("inplace.ts");
    let src = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n<TS version=\"2.1\" language=\"de_DE\" sourcelanguage=\"en\">\n<context>\n    <name>MainWindow</name>\n    <message>\n        <source>Hello</source>\n        <translation type=\"unfinished\"></translation>\n    </message>\n</context>\n</TS>\n";
    std::fs::write(&path, src).unwrap();
    let mut catalog = extract(&path).expect("extract");

    // The Tauri shell's update_unit_target_in_project takes a mutable handle
    // and writes the target in place. Mirror that pattern exactly.
    {
        let unit = catalog
            .units_mut()
            .iter_mut()
            .find(|u| u.source == "Hello")
            .expect("Hello unit");
        unit.target = Target::Singular {
            text: Some("Hallo Welt".into()),
        };
        unit.state = i18n_harness_core::UnitState::Proposed;
    }

    // Snapshot then hand the slice to render — same pattern as
    // `entry.catalog.units().to_vec()` in the Tauri save path.
    let units = catalog.units().to_vec();
    let rendered = render(&catalog, &units).expect("render");
    let rendered_str = std::str::from_utf8(&rendered).expect("utf8");

    assert!(
        rendered_str.contains("Hallo Welt"),
        "render output must contain the in-place edit; got:\n{rendered_str}"
    );
    assert!(
        rendered_str.contains("type=\"unfinished\""),
        "Proposed write contract: type=\"unfinished\" must be retained; got:\n{rendered_str}"
    );
    std::fs::remove_dir_all(&dir).ok();
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
