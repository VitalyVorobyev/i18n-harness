//! Round-trip integration test: every fixture under `fixtures/po/` must
//! satisfy `apply(extract(f), f.units(), out) == f` byte-for-byte.
//!
//! This is the PO round-trip contract — analogous to the Qt round-trip contract.
//! Regressions here are immediate rollback.

use std::path::{Path, PathBuf};

use i18n_harness_catalog::{CatalogFormat, PoFormat};

fn fixtures_dir() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest)
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .join("fixtures")
        .join("po")
}

fn all_po_fixtures() -> Vec<PathBuf> {
    let dir = fixtures_dir();
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|s| s.to_str()),
                Some("po") | Some("pot")
            )
        })
        .collect();
    out.sort();
    out
}

#[test]
fn fixture_dir_is_not_empty() {
    let fixtures = all_po_fixtures();
    assert!(
        !fixtures.is_empty(),
        "fixtures/po/ has no .po files; round-trip suite would silently pass",
    );
}

#[test]
fn extract_then_apply_with_no_changes_is_byte_identical() {
    let fmt = PoFormat;
    for fixture in all_po_fixtures() {
        let catalog = fmt
            .extract(&fixture)
            .unwrap_or_else(|e| panic!("extract({}): {e}", fixture.display()));
        let tmp = std::env::temp_dir().join(format!(
            "i18n-harness-po-roundtrip-{}-{}",
            std::process::id(),
            fixture
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("fixture")
        ));
        fmt.apply(&catalog, catalog.units(), &tmp)
            .unwrap_or_else(|e| panic!("apply({}): {e}", fixture.display()));
        let rendered = std::fs::read(&tmp).expect("read tmp output");
        let original = std::fs::read(&fixture)
            .unwrap_or_else(|e| panic!("re-read {}: {e}", fixture.display()));
        std::fs::remove_file(&tmp).ok();
        if rendered != original {
            let diff = byte_diff_summary(&original, &rendered);
            panic!("round-trip differs for {}:\n{}", fixture.display(), diff,);
        }
    }
}

#[test]
fn extract_normalizes_placeholders_to_icu() {
    let fmt = PoFormat;
    let mixed = fixtures_dir().join("mixed_placeholders.po");
    let catalog = fmt.extract(&mixed).expect("extract mixed");
    let sources: Vec<&str> = catalog.units().iter().map(|u| u.source.as_str()).collect();
    assert!(
        sources.iter().any(|s| s.contains("{0}")),
        "expected at least one source with {{0}}; got {sources:?}",
    );
    assert!(
        sources
            .iter()
            .any(|s| s.contains("{user}") || s.contains("{count}")),
        "expected at least one named placeholder; got {sources:?}",
    );
}

#[test]
fn editing_target_round_trips_through_a_second_apply() {
    let fmt = PoFormat;
    let fixture = fixtures_dir().join("singular_basic.po");
    let catalog = fmt.extract(&fixture).expect("extract");
    let mut units = catalog.units().to_vec();
    let target = units
        .iter_mut()
        .find(|u| u.source == "Open")
        .expect("Open unit");
    match &mut target.target {
        i18n_harness_core::Target::Singular { text } => {
            *text = Some("Öffnen (changed)".into());
        }
        _ => panic!("Open is singular"),
    }
    let tmp = std::env::temp_dir().join(format!("i18n-harness-po-edit-{}.po", std::process::id()));
    fmt.apply(&catalog, &units, &tmp).expect("apply");

    // Re-extract and re-apply with no changes; bytes must be identical.
    let catalog2 = fmt.extract(&tmp).expect("re-extract");
    let tmp2 = tmp.with_extension("po.again");
    fmt.apply(&catalog2, catalog2.units(), &tmp2)
        .expect("re-apply");
    let first = std::fs::read(&tmp).unwrap();
    let second = std::fs::read(&tmp2).unwrap();
    std::fs::remove_file(&tmp).ok();
    std::fs::remove_file(&tmp2).ok();
    assert_eq!(
        first, second,
        "second round-trip diverged (fixed-point violation)"
    );

    // Also: the new translation must appear escaped in the bytes.
    let rendered = std::str::from_utf8(&first).unwrap();
    assert!(
        rendered.contains("Öffnen (changed)"),
        "edited target not present in rendered bytes:\n{rendered}",
    );
    let _ = catalog.units();
}

#[test]
fn plural_entries_extract_with_correct_arity() {
    let fmt = PoFormat;
    let fixture = fixtures_dir().join("plurals_polish.po");
    let catalog = fmt.extract(&fixture).expect("extract polish");
    let plural = catalog
        .units()
        .iter()
        .find(|u| matches!(u.target, i18n_harness_core::Target::Plural { .. }))
        .expect("a plural unit");
    if let i18n_harness_core::Target::Plural { forms } = &plural.target {
        assert_eq!(forms.len(), 3, "Polish plural has 3 forms");
    }
}

#[test]
fn msgctxt_disambiguates_entries() {
    let fmt = PoFormat;
    let fixture = fixtures_dir().join("msgctxt.po");
    let catalog = fmt.extract(&fixture).expect("extract msgctxt fixture");
    // Two entries share msgid "Open" but differ by msgctxt.
    let opens: Vec<&i18n_harness_core::Unit> = catalog
        .units()
        .iter()
        .filter(|u| u.source == "Open")
        .collect();
    assert_eq!(opens.len(), 2, "msgctxt fixture must have two Open entries");
    assert_ne!(opens[0].id, opens[1].id, "msgctxt must disambiguate ids");
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
