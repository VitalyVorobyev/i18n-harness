//! Byte-subtraction subset tests for the Qt `.ts` adapter.
//!
//! Pins the `render_subset` / `write_subset` contract:
//! - id-exactness: the subset keeps exactly the requested ids;
//! - byte-preservation: each kept message is a verbatim substring of input;
//! - emptied-context pruning: dropping a whole context removes it;
//! - keep-all == byte-identical, parameterized over EVERY fixture;
//! - the subset output itself round-trips byte-stably;
//! - keep-none yields a valid, parseable empty TS.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use i18n_harness_adapter_qt::{extract, render, render_subset};
use i18n_harness_core::UnitId;
use proptest::prelude::*;

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

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || haystack.windows(needle.len()).any(|w| w == needle)
}

fn ids_of(bytes: &[u8]) -> Vec<UnitId> {
    let dir = std::env::temp_dir().join(format!(
        "i18n-subset-ids-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("subset.ts");
    std::fs::write(&path, bytes).unwrap();
    let cat = extract(&path).expect("re-extract subset output");
    let ids = cat.units().iter().map(|u| u.id.clone()).collect();
    std::fs::remove_dir_all(&dir).ok();
    ids
}

// ── (d) keep-all == byte-identical, over EVERY fixture ────────────────────────

#[test]
fn keep_all_is_byte_identical_for_every_fixture() {
    for fixture in all_ts_fixtures() {
        let catalog = extract(&fixture).unwrap_or_else(|e| panic!("extract {fixture:?}: {e}"));
        let keep: HashSet<UnitId> = catalog.units().iter().map(|u| u.id.clone()).collect();
        let out = render_subset(&catalog, &keep);
        let original = std::fs::read(&fixture).expect("re-read fixture");
        assert_eq!(
            out,
            original,
            "keep-all subset must be byte-identical for {}",
            fixture.display(),
        );
    }
}

// ── (a) id-exactness + (b) byte-preservation, over EVERY fixture ──────────────

#[test]
fn subset_keeps_exactly_requested_ids_for_every_fixture() {
    for fixture in all_ts_fixtures() {
        let catalog = extract(&fixture).unwrap_or_else(|e| panic!("extract {fixture:?}: {e}"));
        let all: Vec<UnitId> = catalog.units().iter().map(|u| u.id.clone()).collect();
        if all.is_empty() {
            continue;
        }
        // Keep every other unit by document order.
        let keep: HashSet<UnitId> = all
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 0)
            .map(|(_, id)| id.clone())
            .collect();

        let out = render_subset(&catalog, &keep);
        let got: HashSet<UnitId> = ids_of(&out).into_iter().collect();
        assert_eq!(
            got,
            keep,
            "subset of {} must contain exactly the kept ids",
            fixture.display(),
        );
    }
}

#[test]
fn each_kept_message_is_a_verbatim_substring_for_every_fixture() {
    for fixture in all_ts_fixtures() {
        let catalog = extract(&fixture).unwrap_or_else(|e| panic!("extract {fixture:?}: {e}"));
        let original = std::fs::read(&fixture).expect("re-read");
        let all: Vec<UnitId> = catalog.units().iter().map(|u| u.id.clone()).collect();
        if all.len() < 2 {
            continue;
        }
        // Keep just the first unit.
        let keep: HashSet<UnitId> = std::iter::once(all[0].clone()).collect();
        let out = render_subset(&catalog, &keep);

        // The kept output must itself be a sequence of bytes drawn from the
        // original; concretely, re-extracting the kept unit and confirming its
        // surrounding `<message>` block survived verbatim. We verify by
        // checking the kept message block (from the original) appears in `out`.
        let kept_block = first_message_block(&original);
        assert!(
            contains_subslice(&out, kept_block),
            "kept message block must be a verbatim substring of input for {}",
            fixture.display(),
        );
    }
}

/// Slice the original bytes from the first `<message` to the first
/// `</message>` (inclusive). Used to assert byte-preservation of the kept
/// block without depending on adapter internals.
fn first_message_block(bytes: &[u8]) -> &[u8] {
    let start = find(bytes, b"<message").expect("fixture has a <message>");
    let close = b"</message>";
    let close_at = find(&bytes[start..], close).expect("matching </message>") + start;
    &bytes[start..close_at + close.len()]
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ── (c) emptied-context pruning ───────────────────────────────────────────────

#[test]
fn dropping_a_whole_context_removes_it() {
    let fixture = fixtures_dir().join("subset_mix.ts");
    let catalog = extract(&fixture).expect("extract");
    // Keep only units in the "Toolbar" context; the "StatusBar" and "Legacy"
    // contexts should disappear entirely.
    let keep: HashSet<UnitId> = catalog
        .units()
        .iter()
        .filter(|u| u.id.as_str().starts_with("Toolbar::"))
        .map(|u| u.id.clone())
        .collect();
    assert!(!keep.is_empty(), "fixture must have Toolbar units");

    let out = render_subset(&catalog, &keep);
    let out_str = String::from_utf8(out.clone()).expect("utf8");

    assert!(
        out_str.contains("<name>Toolbar</name>"),
        "kept context must survive:\n{out_str}",
    );
    assert!(
        !out_str.contains("<name>StatusBar</name>"),
        "emptied context StatusBar must be pruned:\n{out_str}",
    );
    assert!(
        !out_str.contains("<name>Legacy</name>"),
        "emptied context Legacy must be pruned:\n{out_str}",
    );
    // No dangling empty `<context>` blocks.
    assert!(
        !out_str.contains("<context>\n</context>"),
        "no empty context blocks may remain:\n{out_str}",
    );

    // Id-exactness on this specific drop.
    let got: HashSet<UnitId> = ids_of(&out).into_iter().collect();
    assert_eq!(got, keep, "subset must keep exactly the Toolbar units");
}

#[test]
fn keeping_one_message_in_a_context_leaves_the_context_and_its_name() {
    let fixture = fixtures_dir().join("subset_mix.ts");
    let catalog = extract(&fixture).expect("extract");
    // Keep exactly the "Ready" unit (in StatusBar). StatusBar must survive
    // with its name; its plural sibling must be gone.
    let ready = catalog
        .units()
        .iter()
        .find(|u| u.source == "Ready")
        .expect("Ready unit");
    let keep: HashSet<UnitId> = std::iter::once(ready.id.clone()).collect();

    let out = render_subset(&catalog, &keep);
    let out_str = String::from_utf8(out.clone()).expect("utf8");

    assert!(out_str.contains("<name>StatusBar</name>"), "{out_str}");
    assert!(
        out_str.contains("Bereit"),
        "kept body must remain:\n{out_str}"
    );
    assert!(
        !out_str.contains("Warnungen"),
        "dropped sibling must be gone:\n{out_str}",
    );
    let got: HashSet<UnitId> = ids_of(&out).into_iter().collect();
    assert_eq!(got, keep);
}

// ── (e) the subset output round-trips byte-stably ─────────────────────────────

#[test]
fn subset_output_round_trips_byte_stably_for_every_fixture() {
    for fixture in all_ts_fixtures() {
        let catalog = extract(&fixture).unwrap_or_else(|e| panic!("extract {fixture:?}: {e}"));
        let all: Vec<UnitId> = catalog.units().iter().map(|u| u.id.clone()).collect();
        if all.is_empty() {
            continue;
        }
        // Keep a non-trivial subset (drop the first unit).
        let keep: HashSet<UnitId> = all.iter().skip(1).cloned().collect();
        let out = render_subset(&catalog, &keep);

        // Write, re-extract, render with no changes: must equal `out`.
        let dir = std::env::temp_dir().join(format!(
            "i18n-subset-rt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("subset.ts");
        std::fs::write(&path, &out).unwrap();
        let catalog2 = extract(&path).expect("re-extract subset");
        let rendered2 = render(&catalog2, &[]).expect("re-render subset");
        assert_eq!(
            out,
            rendered2,
            "subset output of {} must round-trip byte-stably",
            fixture.display(),
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}

// ── (f) keep-none yields a valid, parseable empty TS ──────────────────────────

#[test]
fn keep_none_yields_valid_empty_ts_for_every_fixture() {
    for fixture in all_ts_fixtures() {
        let catalog = extract(&fixture).unwrap_or_else(|e| panic!("extract {fixture:?}: {e}"));
        let keep: HashSet<UnitId> = HashSet::new();
        let out = render_subset(&catalog, &keep);
        let out_str = String::from_utf8(out.clone()).expect("utf8");

        // Header + root preserved.
        assert!(out_str.contains("<!DOCTYPE TS>"), "{out_str}");
        assert!(out_str.contains("<TS "), "{out_str}");
        assert!(out_str.contains("</TS>"), "{out_str}");
        // No contexts left.
        assert!(
            !out_str.contains("<context>"),
            "keep-none must leave no contexts for {}:\n{out_str}",
            fixture.display(),
        );

        // Re-parses to zero units, and the root language is preserved.
        let ids = ids_of(&out);
        assert!(
            ids.is_empty(),
            "keep-none subset of {} must have zero units; got {ids:?}",
            fixture.display(),
        );
    }
}

#[test]
fn keep_none_preserves_root_language() {
    let fixture = fixtures_dir().join("subset_mix.ts");
    let catalog = extract(&fixture).expect("extract");
    let out = render_subset(&catalog, &HashSet::new());
    let dir = std::env::temp_dir().join(format!("i18n-subset-lang-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("empty.ts");
    std::fs::write(&path, &out).unwrap();
    let cat2 = extract(&path).expect("extract empty subset");
    assert_eq!(cat2.language(), Some("de_DE"));
    std::fs::remove_dir_all(&dir).ok();
}

// ── Whitespace-cleanliness: no blank lines accumulate ─────────────────────────

// ── Property: any keep-mask is id-exact and round-trips ───────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// For an arbitrary keep-mask over the showcase fixture's units, the
    /// subset (1) re-parses to exactly the kept ids and (2) itself round-trips
    /// byte-stably (extract → render-unchanged == identity).
    #[test]
    fn subset_is_id_exact_and_round_trips_for_any_mask(mask in proptest::collection::vec(any::<bool>(), 1..=8)) {
        let fixture = fixtures_dir().join("subset_mix.ts");
        let catalog = extract(&fixture).expect("extract");
        let all: Vec<UnitId> = catalog.units().iter().map(|u| u.id.clone()).collect();

        let keep: HashSet<UnitId> = all
            .iter()
            .enumerate()
            .filter(|(i, _)| *mask.get(*i % mask.len()).unwrap_or(&false))
            .map(|(_, id)| id.clone())
            .collect();

        let out = render_subset(&catalog, &keep);

        // (1) id-exactness.
        let got: HashSet<UnitId> = ids_of(&out).into_iter().collect();
        prop_assert_eq!(&got, &keep);

        // (2) the subset output round-trips byte-stably.
        let dir = std::env::temp_dir().join(format!(
            "i18n-subset-prop-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("subset.ts");
        std::fs::write(&path, &out).unwrap();
        let catalog2 = extract(&path).expect("re-extract");
        let rendered2 = render(&catalog2, &[]).expect("re-render");
        std::fs::remove_dir_all(&dir).ok();
        prop_assert_eq!(out, rendered2);
    }
}

#[test]
fn dropping_middle_message_leaves_no_blank_line() {
    let fixture = fixtures_dir().join("subset_mix.ts");
    let catalog = extract(&fixture).expect("extract");
    // Drop the middle Toolbar message ("Open %1"), keep the rest of Toolbar.
    let keep: HashSet<UnitId> = catalog
        .units()
        .iter()
        .filter(|u| u.id.as_str().starts_with("Toolbar::") && u.source != "Open {0}")
        .map(|u| u.id.clone())
        .collect();
    let out = render_subset(&catalog, &keep);
    let out_str = String::from_utf8(out).expect("utf8");
    assert!(
        !out_str.contains("\n\n"),
        "no doubled newlines (blank lines) may appear:\n{out_str}",
    );
    // The inter-message comment belonged to the dropped "Open %1" entry's
    // line region only by adjacency; it is NOT part of the message span, so it
    // survives. Confirm the kept "Save" body is intact.
    assert!(out_str.contains("Speichern"));
}
