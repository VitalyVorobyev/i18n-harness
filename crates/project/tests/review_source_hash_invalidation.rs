//! Cross-crate integration test: adapter-qt extract → project review → source
//! change → re-extract → invalidation detected.
//!
//! Exercises the full §9.4 slice from the source-hash / review-status design doc:
//!
//! 1. Construct a project over a temp dir with one Qt fixture.
//! 2. Extract through `adapter_qt::extract`; verify units have `source_hash`.
//! 3. `apply_review_state`; verify `review_status = None`, `changed = false`.
//! 4. `set_review_status` for one unit with its current hash.
//! 5. Re-extract, re-apply; verify `Approved` and `changed = false`.
//! 6. Mutate the fixture's `<extracomment>` text (hash changes, unit id stays
//!    the same since Qt ids are context::source[::comment]); re-extract,
//!    re-apply; verify `review_status = Some(Approved)` (unchanged) and
//!    `source_changed_since_review = true`.
//! 7. Call `set_review_status` again with the new `source_hash` (the §6.3
//!    re-approve path); re-apply; verify `source_changed_since_review = false`.

use std::path::Path;

use i18n_harness_adapter_qt::extract as qt_extract;
use i18n_harness_core::ReviewStatus;
use i18n_harness_project::Project;

/// Minimal Qt `.ts` fixture with one unit and an extracomment.
const FIXTURE_INITIAL: &[u8] = br#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>MainWindow</name>
    <message>
        <source>Hello</source>
        <extracomment>Greeting shown at startup.</extracomment>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>
"#;

/// Same fixture with only the extracomment text changed (source and unit id
/// stay the same; the hash changes because extracomment is an input).
const FIXTURE_MUTATED: &[u8] = br#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>MainWindow</name>
    <message>
        <source>Hello</source>
        <extracomment>Greeting shown at startup - UPDATED by developer.</extracomment>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>
"#;

#[test]
fn source_hash_invalidation_full_cycle() {
    // ── Step 1: set up temp dir + project ────────────────────────────────
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let catalog_rel = Path::new("translations/app_de.ts");
    let catalog_abs = root.join(catalog_rel);
    std::fs::create_dir_all(catalog_abs.parent().unwrap()).expect("mkdir");
    std::fs::write(&catalog_abs, FIXTURE_INITIAL).expect("write initial fixture");

    let manifest = format!(
        "[project]\nname = \"hash-invalidation-test\"\nschema = 1\n\n[[catalogs]]\npath = \"{}\"\nformat = \"qt-ts\"\nlocale = \"de_DE\"\n",
        catalog_rel.display()
    );
    std::fs::write(root.join("i18n-harness.toml"), &manifest).expect("write manifest");

    let (project, _warnings) = Project::open(root).expect("open project");

    // ── Step 2: extract and verify source_hash is Some ───────────────────
    let catalog1 = qt_extract(&catalog_abs).expect("initial extract");
    let units1 = catalog1.units();
    assert!(!units1.is_empty());

    let hello = units1
        .iter()
        .find(|u| u.source == "Hello")
        .expect("Hello unit must be present");
    let hash_initial = hello.source_hash.clone().expect("must have source_hash");
    assert!(hash_initial.starts_with("sha256:"), "{hash_initial}");

    // The unit id in the Qt adapter is "context::source[::comment]".
    let unit_id = hello.id.clone();

    // ── Step 3: apply_review_state — no records yet ───────────────────────
    let mut units = units1.to_vec();
    project.apply_review_state(catalog_rel, &mut units);
    let hello = units.iter().find(|u| u.source == "Hello").unwrap();
    assert!(hello.review_status.is_none());
    assert!(!hello.source_changed_since_review);

    // ── Step 4: set_review_status with current hash ───────────────────────
    project
        .set_review_status(
            catalog_rel,
            &unit_id,
            Some(ReviewStatus::Approved),
            hash_initial.clone(),
            None,
        )
        .expect("set_review_status");

    // ── Step 5: re-extract, re-apply → Approved, changed = false ─────────
    let catalog2 = qt_extract(&catalog_abs).expect("re-extract (same file)");
    let mut units2 = catalog2.units().to_vec();
    project.apply_review_state(catalog_rel, &mut units2);
    let hello2 = units2.iter().find(|u| u.source == "Hello").unwrap();
    assert_eq!(hello2.review_status, Some(ReviewStatus::Approved));
    assert!(!hello2.source_changed_since_review);

    // ── Step 6: mutate extracomment → hash changes, unit id stays same ────
    std::fs::write(&catalog_abs, FIXTURE_MUTATED).expect("write mutated fixture");

    let catalog3 = qt_extract(&catalog_abs).expect("extract mutated");
    let units3 = catalog3.units();
    let hello3 = units3
        .iter()
        .find(|u| u.source == "Hello")
        .expect("Hello unit still present after extracomment change");
    let hash_new = hello3.source_hash.clone().expect("must have source_hash");

    // Unit id must be the same (only extracomment changed, not source).
    assert_eq!(
        hello3.id, unit_id,
        "unit id must not change when only extracomment changes"
    );
    assert_ne!(
        hash_initial, hash_new,
        "hash must change when extracomment changes"
    );

    let mut units3_mut = units3.to_vec();
    project.apply_review_state(catalog_rel, &mut units3_mut);
    let hello3 = units3_mut.iter().find(|u| u.source == "Hello").unwrap();
    assert_eq!(
        hello3.review_status,
        Some(ReviewStatus::Approved),
        "status must be preserved"
    );
    assert!(
        hello3.source_changed_since_review,
        "extracomment changed → flag must be true"
    );

    // ── Step 7: re-approve with new hash → changed = false ───────────────
    project
        .set_review_status(
            catalog_rel,
            &unit_id,
            Some(ReviewStatus::Approved),
            hash_new.clone(),
            Some("Re-approved after developer updated extracomment.".to_owned()),
        )
        .expect("re-approve");

    let mut units4 = catalog3.units().to_vec();
    project.apply_review_state(catalog_rel, &mut units4);
    let hello4 = units4.iter().find(|u| u.source == "Hello").unwrap();
    assert_eq!(hello4.review_status, Some(ReviewStatus::Approved));
    assert!(
        !hello4.source_changed_since_review,
        "re-approved → flag must be false"
    );
}
