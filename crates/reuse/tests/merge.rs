//! Merge-back and full reuse → split → fill → merge round-trip tests.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use i18n_harness_adapter_qt::{Catalog, apply, extract, write_subset};
use i18n_harness_core::{Target, Unit, UnitId, UnitState};
use i18n_harness_locales::Locale;
use i18n_harness_reuse::{ReuseError, merge_back, reuse_from_references};
use tempfile::TempDir;

fn write_ts(dir: &Path, name: &str, bytes: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write fixture");
    path
}

fn de() -> &'static Locale {
    Locale::by_id("de_DE").expect("de_DE locale")
}

fn id(s: &str) -> UnitId {
    UnitId::from(s)
}

/// Fill the given ids of `catalog` with finished singular targets from `fills`
/// (id → text), then apply to `out`. Returns nothing; panics on adapter error.
fn fill_and_apply(catalog: &Catalog, fills: &[(&str, &str)], out: &Path) {
    let mut units: Vec<Unit> = catalog.units().to_vec();
    for u in &mut units {
        if let Some((_, text)) = fills.iter().find(|(fid, _)| id(fid) == u.id) {
            u.target = Target::Singular {
                text: Some((*text).to_string()),
            };
            u.state = UnitState::Finished;
        }
    }
    apply(catalog, &units, out).expect("apply fills");
}

// Sources are deliberately long enough that realistic German translations
// stay within de_DE's 1.4 length-warn ratio, so a clean reuse copy promotes to
// Finished rather than tripping a soft length finding.
const ORIGINAL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Main</name>
    <message>
        <source>Save the document</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <source>Open the menu</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <source>Quit the application</source>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>"#;

// ── merge_back guards ─────────────────────────────────────────────────────────

#[test]
fn merge_back_rejects_stray_ids() {
    let tmp = TempDir::new().unwrap();
    let base = write_ts(tmp.path(), "base.ts", ORIGINAL);
    // Remainder has an id (`Main::Nonexistent`) that the base does not.
    let remainder = write_ts(
        tmp.path(),
        "remainder.ts",
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Main</name>
    <message>
        <source>Nonexistent</source>
        <translation>Existiert nicht</translation>
    </message>
</context>
</TS>"#,
    );

    let err = merge_back(&base, &remainder).unwrap_err();
    match err {
        ReuseError::MergeStrayIds { ids, .. } => {
            assert_eq!(ids, vec!["Main::Nonexistent".to_string()]);
        }
        other => panic!("expected MergeStrayIds, got {other:?}"),
    }
    // The message names the stray id.
    let msg = merge_back(&base, &remainder).unwrap_err().to_string();
    assert!(msg.contains("Main::Nonexistent"), "message: {msg}");
}

#[test]
fn merge_back_rejects_overlap_of_finished_units() {
    let tmp = TempDir::new().unwrap();
    // Base where `Save the document` is already finished.
    let base = write_ts(
        tmp.path(),
        "base.ts",
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Main</name>
    <message>
        <source>Save the document</source>
        <translation>Dokument speichern</translation>
    </message>
    <message>
        <source>Open the menu</source>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>"#,
    );
    // Remainder ALSO has `Save the document` finished — overlap.
    let remainder = write_ts(
        tmp.path(),
        "remainder.ts",
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Main</name>
    <message>
        <source>Save the document</source>
        <translation>Datei sichern</translation>
    </message>
</context>
</TS>"#,
    );

    let err = merge_back(&base, &remainder).unwrap_err();
    match err {
        ReuseError::MergeOverlap { ids, .. } => {
            assert_eq!(ids, vec!["Main::Save the document".to_string()]);
        }
        other => panic!("expected MergeOverlap, got {other:?}"),
    }
}

#[test]
fn merge_back_returns_only_filled_units() {
    let tmp = TempDir::new().unwrap();
    let base = write_ts(tmp.path(), "base.ts", ORIGINAL);
    // Remainder: "Save the document" filled, "Open the menu" left empty.
    let remainder = write_ts(
        tmp.path(),
        "remainder.ts",
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Main</name>
    <message>
        <source>Save the document</source>
        <translation>Dokument speichern</translation>
    </message>
    <message>
        <source>Open the menu</source>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>"#,
    );

    let outcome = merge_back(&base, &remainder).unwrap();
    assert_eq!(outcome.report.merged, 1, "only the filled unit merges");
    assert_eq!(outcome.report.merged_complete, 1);
    assert_eq!(outcome.units.len(), 1);
    assert_eq!(outcome.units[0].id, id("Main::Save the document"));
}

// ── end-to-end: reuse → split → fill → merge → apply ──────────────────────────

#[test]
fn end_to_end_reuse_split_fill_merge_equals_applying_all_translations() {
    let tmp = TempDir::new().unwrap();
    let original = write_ts(tmp.path(), "original.ts", ORIGINAL);

    // A reference that can fill `Save the document` (and only that).
    let reference = write_ts(
        tmp.path(),
        "ref.ts",
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Main</name>
    <message>
        <source>Save the document</source>
        <translation>Dokument speichern</translation>
    </message>
</context>
</TS>"#,
    );

    // 1) Reuse from the reference, then apply → base_after_reuse.ts.
    let reuse = reuse_from_references(&original, &[reference], de(), None).unwrap();
    assert_eq!(
        reuse.report.copied_finished,
        vec![id("Main::Save the document")],
    );
    let base_after_reuse = tmp.path().join("base_after_reuse.ts");
    apply(&reuse.base, &reuse.units, &base_after_reuse).expect("apply reuse");

    // 2) Split: write a subset of just the remaining (untranslated) ids.
    let remaining: HashSet<UnitId> = reuse.report.remaining_ids.iter().cloned().collect();
    assert_eq!(
        remaining,
        HashSet::from([id("Main::Open the menu"), id("Main::Quit the application")]),
        "Save was reused, Open and Quit remain",
    );
    let base_cat = extract(&base_after_reuse).unwrap();
    let remainder = tmp.path().join("remainder.ts");
    write_subset(&base_cat, &remaining, &remainder).expect("write subset");

    // The remainder contains exactly the remaining units, untranslated.
    let remainder_cat = extract(&remainder).unwrap();
    let remainder_ids: HashSet<UnitId> =
        remainder_cat.units().iter().map(|u| u.id.clone()).collect();
    assert_eq!(remainder_ids, remaining);

    // 3) Fill the remainder (a translator does this).
    let remainder_filled = tmp.path().join("remainder_filled.ts");
    fill_and_apply(
        &remainder_cat,
        &[
            ("Main::Open the menu", "Menü öffnen"),
            ("Main::Quit the application", "Anwendung beenden"),
        ],
        &remainder_filled,
    );

    // 4) Merge the filled remainder back into the base, then apply → final.ts.
    let merge = merge_back(&base_after_reuse, &remainder_filled).unwrap();
    assert_eq!(merge.report.merged, 2);
    let final_path = tmp.path().join("final.ts");
    apply(&merge.base, &merge.units, &final_path).expect("apply merge");

    // 5) Expected: apply ALL translations directly onto the original.
    let original_cat = extract(&original).unwrap();
    let expected = tmp.path().join("expected.ts");
    fill_and_apply(
        &original_cat,
        &[
            ("Main::Save the document", "Dokument speichern"),
            ("Main::Open the menu", "Menü öffnen"),
            ("Main::Quit the application", "Anwendung beenden"),
        ],
        &expected,
    );

    let final_bytes = std::fs::read(&final_path).unwrap();
    let expected_bytes = std::fs::read(&expected).unwrap();
    assert_eq!(
        final_bytes, expected_bytes,
        "reuse→split→fill→merge→apply must byte-equal applying every translation to the original",
    );
}
