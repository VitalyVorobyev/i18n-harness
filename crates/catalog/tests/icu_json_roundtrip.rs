//! Round-trip integration test: every fixture under `fixtures/icu-json/` must
//! satisfy `apply(extract(f), f.units(), out) == f` byte-for-byte.
//!
//! This is the M4.5 ICU-JSON contract — analogous to M0 (Qt) and M4.4 (PO).
//! Regressions here are immediate rollback.

use std::path::{Path, PathBuf};

use i18n_harness_catalog::{CatalogFormat, IcuJsonFormat};

fn fixtures_dir() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest)
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .join("fixtures")
        .join("icu-json")
}

fn all_json_fixtures() -> Vec<PathBuf> {
    let dir = fixtures_dir();
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| matches!(p.extension().and_then(|s| s.to_str()), Some("json")))
        .collect();
    out.sort();
    out
}

#[test]
fn fixture_dir_is_not_empty() {
    let fixtures = all_json_fixtures();
    assert!(
        !fixtures.is_empty(),
        "fixtures/icu-json/ has no .json files; round-trip suite would silently pass",
    );
}

#[test]
fn extract_then_apply_with_no_changes_is_byte_identical() {
    let fmt = IcuJsonFormat;
    for fixture in all_json_fixtures() {
        let catalog = fmt
            .extract(&fixture)
            .unwrap_or_else(|e| panic!("extract({}): {e}", fixture.display()));
        let tmp = std::env::temp_dir().join(format!(
            "i18n-harness-icu-roundtrip-{}-{}",
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
fn empty_object_extracts_zero_units() {
    let fmt = IcuJsonFormat;
    let fixture = fixtures_dir().join("empty_object.json");
    let catalog = fmt.extract(&fixture).expect("extract empty");
    assert_eq!(catalog.units().len(), 0);
}

#[test]
fn nested_paths_are_dot_joined() {
    let fmt = IcuJsonFormat;
    let fixture = fixtures_dir().join("nested.json");
    let catalog = fmt.extract(&fixture).expect("extract nested");
    let ids: Vec<&str> = catalog.units().iter().map(|u| u.id.as_str()).collect();
    assert!(
        ids.contains(&"app.menu.file.open"),
        "expected `app.menu.file.open`; got {ids:?}",
    );
    assert!(
        ids.contains(&"errors.not_found"),
        "expected `errors.not_found`; got {ids:?}",
    );
}

#[test]
fn extracted_units_are_in_document_order() {
    let fmt = IcuJsonFormat;
    let fixture = fixtures_dir().join("flat.json");
    let catalog = fmt.extract(&fixture).expect("extract flat");
    let ids: Vec<&str> = catalog.units().iter().map(|u| u.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["greeting", "farewell", "menu.file.open", "unicode_ok"],
    );
}

#[test]
fn editing_target_round_trips_through_a_second_apply() {
    let fmt = IcuJsonFormat;
    let fixture = fixtures_dir().join("flat.json");
    let catalog = fmt.extract(&fixture).expect("extract");
    let mut units = catalog.units().to_vec();
    let target = units
        .iter_mut()
        .find(|u| u.id.as_str() == "farewell")
        .expect("farewell unit");
    match &mut target.target {
        i18n_harness_core::Target::Singular { text } => {
            *text = Some("Auf Wiedersehen \"German\"".into());
        }
        _ => panic!("farewell is singular"),
    }
    let tmp =
        std::env::temp_dir().join(format!("i18n-harness-icu-edit-{}.json", std::process::id()));
    fmt.apply(&catalog, &units, &tmp).expect("apply");

    // Re-extract and re-apply with no changes; bytes must be identical
    // (fixed-point).
    let catalog2 = fmt.extract(&tmp).expect("re-extract");
    let tmp2 = tmp.with_extension("json.again");
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

    let rendered = std::str::from_utf8(&first).unwrap();
    assert!(
        rendered.contains(r#""Auf Wiedersehen \"German\"""#),
        "edited target not present (with escaped quotes) in rendered bytes:\n{rendered}",
    );
}

#[test]
fn extract_validates_icu_brace_balance() {
    // A broken file with unbalanced braces should fail at extract time, not
    // at translate time. The placeholder converter is responsible.
    let dir = std::env::temp_dir().join(format!("i18n-harness-icu-broken-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("broken.json");
    std::fs::write(&path, r#"{"bad": "hello {name"}"#).expect("write");
    let fmt = IcuJsonFormat;
    let result = fmt.extract(&path);
    std::fs::remove_file(&path).ok();
    std::fs::remove_dir(&dir).ok();
    assert!(
        result.is_err(),
        "expected unbalanced-brace input to be rejected"
    );
}

#[test]
fn rejects_flattened_id_collision_across_nesting() {
    // Codex P2 on PR #39: `{"a.b": "x", "a": {"b": "y"}}` would project to
    // two units with id `a.b`. The unit-id uniqueness invariant in
    // `Catalog::find_unit_mut` makes one of them unreachable; refuse the
    // input.
    let dir = std::env::temp_dir().join(format!(
        "i18n-harness-icu-id-collide-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("collide.json");
    std::fs::write(&path, r#"{"a.b":"x","a":{"b":"y"}}"#).expect("write");
    let fmt = IcuJsonFormat;
    let result = fmt.extract(&path);
    std::fs::remove_file(&path).ok();
    std::fs::remove_dir(&dir).ok();
    let err = result.expect_err("expected id collision to be rejected");
    let msg = format!("{err}");
    assert!(
        msg.contains("duplicate flattened unit id"),
        "expected duplicate-id message; got: {msg}"
    );
}

#[test]
fn apply_rejects_malformed_icu_in_target() {
    // Codex P1 on PR #39: the writer must validate ICU brace balance on
    // the EDITED target before writing it back; otherwise broken edits
    // (`hello {name` with no closing brace) reach disk and only fail at
    // runtime in the consuming i18n library.
    let fmt = IcuJsonFormat;
    let fixture = fixtures_dir().join("flat.json");
    let catalog = fmt.extract(&fixture).expect("extract");
    let mut units = catalog.units().to_vec();
    let target = units
        .iter_mut()
        .find(|u| u.id.as_str() == "farewell")
        .expect("farewell");
    match &mut target.target {
        i18n_harness_core::Target::Singular { text } => {
            *text = Some("hello {name".into());
        }
        _ => panic!("farewell is singular"),
    }
    let tmp = std::env::temp_dir().join(format!(
        "i18n-harness-icu-bad-apply-{}.json",
        std::process::id()
    ));
    let result = fmt.apply(&catalog, &units, &tmp);
    std::fs::remove_file(&tmp).ok();
    assert!(
        result.is_err(),
        "expected apply with malformed ICU target to fail"
    );
}

#[test]
fn rejects_non_object_root() {
    let dir = std::env::temp_dir().join(format!("i18n-harness-icu-array-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("array.json");
    std::fs::write(&path, "[1,2,3]").expect("write");
    let fmt = IcuJsonFormat;
    let result = fmt.extract(&path);
    std::fs::remove_file(&path).ok();
    std::fs::remove_dir(&dir).ok();
    assert!(result.is_err(), "array root should be rejected");
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
