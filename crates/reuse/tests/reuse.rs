//! Integration tests for the reuse / split / merge core logic.
//!
//! Each test builds small in-memory `.ts` catalogs on disk (tempdir), then
//! drives `reuse_from_references` / `merge_back` against them and asserts the
//! per-unit contract from the crate docs. The Qt unit id is
//! `"<context>::<source>"`, so two catalogs sharing a context name and source
//! text share the unit id — which is how a reference becomes a candidate for a
//! base unit.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use i18n_harness_adapter_qt::extract;
use i18n_harness_core::{Target, UnitId, UnitState};
use i18n_harness_locales::Locale;
use i18n_harness_reuse::{
    ConflictText, CopiedDisposition, reuse_from_references, writable_untranslated_ids,
};
use tempfile::TempDir;

// ── fixture builders ──────────────────────────────────────────────────────────

/// Write `bytes` to `<dir>/<name>` and return the path.
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

/// A base catalog with three untranslated units in context `Main`, plus one
/// already-finished unit `Help` that reuse must never touch:
///
/// - `Save the document` — references will agree on it,
/// - `Open %1` — references will disagree (conflict),
/// - `Quit the application` — no reference covers it (remaining).
///
/// The non-placeholder sources are long enough that realistic German
/// translations stay within de_DE's 1.4 length-warn ratio, so a clean copy
/// promotes to Finished instead of tripping a soft length finding.
const BASE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Main</name>
    <message>
        <source>Save the document</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <source>Open %1</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <source>Quit the application</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <source>Help</source>
        <translation>Hilfe</translation>
    </message>
</context>
</TS>"#;

// ── tests ─────────────────────────────────────────────────────────────────────

#[test]
fn zero_candidate_unit_stays_untranslated_and_lands_in_remaining() {
    let tmp = TempDir::new().unwrap();
    let base = write_ts(tmp.path(), "base.ts", BASE);
    // A reference that covers nothing the base has.
    let reference = write_ts(
        tmp.path(),
        "ref.ts",
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Other</name>
    <message>
        <source>Unrelated</source>
        <translation>Unbezogen</translation>
    </message>
</context>
</TS>"#,
    );

    let outcome = reuse_from_references(&base, &[reference], de(), None).unwrap();

    // All three writable base units have no candidate → all in remaining.
    let remaining: HashSet<&str> = outcome
        .report
        .remaining_ids
        .iter()
        .map(UnitId::as_str)
        .collect();
    assert!(remaining.contains("Main::Save the document"));
    assert!(remaining.contains("Main::Open %1"));
    assert!(remaining.contains("Main::Quit the application"));
    assert!(outcome.report.copied.is_empty());
    assert!(outcome.report.conflicts.is_empty());

    // The units stay untranslated.
    for u in &outcome.units {
        if u.id == id("Main::Save the document") {
            assert_eq!(u.state, UnitState::Untranslated);
            assert!(u.target.is_empty());
        }
    }
}

#[test]
fn single_candidate_clean_copies_finished() {
    let tmp = TempDir::new().unwrap();
    let base = write_ts(tmp.path(), "base.ts", BASE);
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

    let outcome =
        reuse_from_references(&base, std::slice::from_ref(&reference), de(), None).unwrap();

    assert_eq!(
        outcome.report.copied_finished,
        vec![id("Main::Save the document")],
        "Save must be copied and promoted to Finished",
    );
    assert!(outcome.report.copied_needs_review.is_empty());
    assert_eq!(outcome.report.copied.len(), 1);
    let copied = &outcome.report.copied[0];
    assert_eq!(copied.id, id("Main::Save the document"));
    assert_eq!(copied.disposition, CopiedDisposition::Finished);
    assert_eq!(copied.winning_reference, reference);

    let save = outcome
        .units
        .iter()
        .find(|u| u.id == id("Main::Save the document"))
        .unwrap();
    assert_eq!(save.state, UnitState::Finished);
    assert_eq!(
        save.target,
        Target::Singular {
            text: Some("Dokument speichern".to_string())
        }
    );

    // The pre-finished Help unit is untouched and never enters any bucket.
    assert!(!outcome.report.copied_finished.contains(&id("Main::Help")));
    assert!(!outcome.report.remaining_ids.contains(&id("Main::Help")));
}

#[test]
fn single_candidate_gate_flagged_copies_needs_review() {
    let tmp = TempDir::new().unwrap();
    let base = write_ts(tmp.path(), "base.ts", BASE);
    // The base unit `Open %1` carries a `%1` placeholder. A finished reference
    // whose translation DROPS the placeholder is structurally broken: copying
    // it trips the gate's hard placeholder-mismatch check. The text is still
    // copied; the unit stays Proposed for a human to fix.
    let reference = write_ts(
        tmp.path(),
        "ref.ts",
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Main</name>
    <message>
        <source>Open %1</source>
        <translation>Datei öffnen</translation>
    </message>
</context>
</TS>"#,
    );

    let outcome = reuse_from_references(&base, &[reference], de(), None).unwrap();

    assert!(
        outcome.report.copied_finished.is_empty(),
        "a gate-flagged copy must NOT be promoted to Finished",
    );
    assert_eq!(
        outcome.report.copied_needs_review,
        vec![id("Main::Open %1")],
    );
    assert_eq!(outcome.report.copied.len(), 1);
    assert_eq!(
        outcome.report.copied[0].disposition,
        CopiedDisposition::NeedsReview
    );

    let unit = outcome
        .units
        .iter()
        .find(|u| u.id == id("Main::Open %1"))
        .unwrap();
    assert_eq!(unit.state, UnitState::Proposed, "kept at Proposed");
    assert_eq!(
        unit.target,
        Target::Singular {
            text: Some("Datei öffnen".to_string())
        },
        "the copied text is kept even though the gate flagged it",
    );
}

#[test]
fn two_references_agreeing_copies_once_with_first_provenance() {
    let tmp = TempDir::new().unwrap();
    let base = write_ts(tmp.path(), "base.ts", BASE);
    let ref_a = write_ts(
        tmp.path(),
        "ref_a.ts",
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
    let ref_b = write_ts(
        tmp.path(),
        "ref_b.ts",
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

    // Declaration order: ref_a first, then ref_b.
    let outcome =
        reuse_from_references(&base, &[ref_a.clone(), ref_b.clone()], de(), None).unwrap();

    assert_eq!(
        outcome.report.copied_finished,
        vec![id("Main::Save the document")],
    );
    assert_eq!(outcome.report.copied.len(), 1, "copied once, not twice");
    assert_eq!(
        outcome.report.copied[0].winning_reference, ref_a,
        "provenance is the FIRST reference in declaration order",
    );
    assert!(
        outcome.report.conflicts.is_empty(),
        "agreement is not a conflict",
    );
}

#[test]
fn two_references_disagreeing_is_conflict_not_copied_and_excluded_from_remaining() {
    let tmp = TempDir::new().unwrap();
    let base = write_ts(tmp.path(), "base.ts", BASE);
    let ref_a = write_ts(
        tmp.path(),
        "ref_a.ts",
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Main</name>
    <message>
        <source>Open %1</source>
        <translation>%1 öffnen</translation>
    </message>
</context>
</TS>"#,
    );
    let ref_b = write_ts(
        tmp.path(),
        "ref_b.ts",
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Main</name>
    <message>
        <source>Open %1</source>
        <translation>Öffne %1</translation>
    </message>
</context>
</TS>"#,
    );

    let outcome =
        reuse_from_references(&base, &[ref_a.clone(), ref_b.clone()], de(), None).unwrap();

    // Not copied.
    assert!(outcome.report.copied.is_empty());
    assert!(outcome.report.copied_finished.is_empty());
    assert!(outcome.report.copied_needs_review.is_empty());

    // Recorded as a conflict listing both distinct candidates in declaration
    // order.
    assert_eq!(outcome.report.conflicts.len(), 1);
    let conflict = &outcome.report.conflicts[0];
    assert_eq!(conflict.id, id("Main::Open %1"));
    assert_eq!(conflict.candidates.len(), 2);
    // The adapter normalizes Qt `%1` to ICU `{0}` on extract, so the conflict
    // candidate text is the ICU form — the same form the gate and intermediate
    // JSONL carry.
    assert_eq!(conflict.candidates[0].reference, ref_a);
    assert_eq!(
        conflict.candidates[0].text,
        ConflictText::Singular("{0} öffnen".to_string())
    );
    assert_eq!(conflict.candidates[1].reference, ref_b);
    assert_eq!(
        conflict.candidates[1].text,
        ConflictText::Singular("Öffne {0}".to_string())
    );

    // Excluded from remaining_ids (it has candidates, just needs a human pick).
    assert!(
        !outcome.report.remaining_ids.contains(&id("Main::Open %1")),
        "a conflicted id must NOT feed the split",
    );

    // The base unit is left untranslated.
    let unit = outcome
        .units
        .iter()
        .find(|u| u.id == id("Main::Open %1"))
        .unwrap();
    assert_eq!(unit.state, UnitState::Untranslated);
    assert!(unit.target.is_empty());
}

#[test]
fn three_references_two_agree_one_dissents_folds_voters() {
    let tmp = TempDir::new().unwrap();
    let base = write_ts(tmp.path(), "base.ts", BASE);
    let mk = |name: &str, translation: &str| {
        write_ts(
            tmp.path(),
            name,
            &format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Main</name>
    <message>
        <source>Open %1</source>
        <translation>{translation}</translation>
    </message>
</context>
</TS>"#
            ),
        )
    };
    let ref_a = mk("ref_a.ts", "%1 öffnen");
    let ref_b = mk("ref_b.ts", "Öffne %1");
    let ref_c = mk("ref_c.ts", "%1 öffnen");

    let outcome = reuse_from_references(
        &base,
        &[ref_a.clone(), ref_b.clone(), ref_c.clone()],
        de(),
        None,
    )
    .unwrap();

    assert_eq!(outcome.report.conflicts.len(), 1);
    let conflict = &outcome.report.conflicts[0];
    assert_eq!(conflict.candidates.len(), 2, "two distinct options");
    // First option: "%1 öffnen" from ref_a, also from ref_c.
    assert_eq!(conflict.candidates[0].reference, ref_a);
    assert_eq!(conflict.candidates[0].also_from, vec![ref_c]);
    // Second option: "Öffne %1" from ref_b alone.
    assert_eq!(conflict.candidates[1].reference, ref_b);
    assert!(conflict.candidates[1].also_from.is_empty());
}

#[test]
fn writable_untranslated_ids_excludes_finished_and_filled_proposed() {
    let tmp = TempDir::new().unwrap();
    // Save: untranslated → included. Help: finished → excluded.
    // Draft: proposed WITH content → excluded (it has text, not empty work).
    let path = write_ts(
        tmp.path(),
        "c.ts",
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Main</name>
    <message>
        <source>Save</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <source>Help</source>
        <translation>Hilfe</translation>
    </message>
    <message>
        <source>Draft</source>
        <translation type="unfinished">Entwurf</translation>
    </message>
</context>
</TS>"#,
    );
    let catalog = extract(&path).unwrap();
    let ids = writable_untranslated_ids(&catalog);
    assert!(ids.contains(&id("Main::Save")));
    assert!(!ids.contains(&id("Main::Help")), "finished excluded");
    assert!(
        !ids.contains(&id("Main::Draft")),
        "proposed-with-content excluded",
    );
}
