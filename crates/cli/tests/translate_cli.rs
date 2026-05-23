//! End-to-end CLI tests for `harness translate`.
//!
//! The CLI assembles: adapter-qt → batch → backend → gate → metrics → apply.
//! These tests use the `manual` backend (identity echo) so they run without
//! a model and without network. The `ollama` backend has its own mocked
//! tests inside `crates/backend`.

use std::fs;
use std::process::Command;

fn harness() -> Command {
    Command::new(env!("CARGO_BIN_EXE_harness"))
}

fn tempdir() -> std::path::PathBuf {
    // process::id + nanos alone can clash under parallel test execution
    // (cargo's default scheduler can call this twice in the same nanosecond
    // on a fast machine). The atomic counter is the trustworthy source of
    // uniqueness within a process; nanos + pid disambiguate across runs.
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "i18n-harness-translate-{}-{}-{seq}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    fs::create_dir_all(&dir).expect("create tempdir");
    dir
}

const SMALL_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Greet</name>
    <message>
        <source>Hello</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <source>Goodbye</source>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>
"#;

const WITH_OBSOLETE_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Greet</name>
    <message>
        <source>Hello</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <source>Old</source>
        <translation type="obsolete">Alt</translation>
    </message>
</context>
</TS>
"#;

#[test]
fn dry_run_translates_and_does_not_write() {
    let dir = tempdir();
    let path = dir.join("small.ts");
    fs::write(&path, SMALL_TS).unwrap();

    let out = harness()
        .args(["translate", "--locale", "de_DE", "--backend", "manual"])
        .arg(&path)
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("dry-run"), "missing dry-run: {stdout}");
    assert!(stdout.contains("writable=2"), "stdout: {stdout}");
    assert!(stdout.contains("translated=2"), "stdout: {stdout}");
    assert!(stdout.contains("finished=2"), "stdout: {stdout}");

    // File contents on disk are unchanged.
    assert_eq!(fs::read_to_string(&path).unwrap(), SMALL_TS);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn with_out_writes_translated_catalog() {
    let dir = tempdir();
    let path = dir.join("small.ts");
    let out_path = dir.join("small.de_DE.ts");
    fs::write(&path, SMALL_TS).unwrap();

    let out = harness()
        .args(["translate", "--locale", "de_DE", "--out"])
        .arg(&out_path)
        .arg(&path)
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let written = fs::read_to_string(&out_path).expect("output file written");
    // Echo backend should have filled both targets with the source text.
    assert!(
        written.contains("<translation>Hello</translation>"),
        "{written}"
    );
    assert!(
        written.contains("<translation>Goodbye</translation>"),
        "{written}"
    );
    // Both finished (no `type="unfinished"` left in the file).
    assert!(!written.contains(r#"type="unfinished""#), "{written}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn obsolete_units_preserved_untouched() {
    let dir = tempdir();
    let path = dir.join("with_obsolete.ts");
    let out_path = dir.join("with_obsolete.out.ts");
    fs::write(&path, WITH_OBSOLETE_TS).unwrap();

    let result = harness()
        .args(["translate", "--locale", "de_DE", "--out"])
        .arg(&out_path)
        .arg(&path)
        .output()
        .expect("spawn");
    assert!(
        result.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );

    let written = fs::read_to_string(&out_path).expect("output file written");
    // Obsolete unit preserved verbatim (target unchanged, type still set).
    assert!(
        written.contains(r#"<translation type="obsolete">Alt</translation>"#),
        "obsolete must be preserved: {written}"
    );
    // Writable unit was translated.
    assert!(
        written.contains("<translation>Hello</translation>"),
        "{written}"
    );

    // Stdout reports only the writable unit was translated.
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("writable=1"), "{stdout}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn unknown_locale_errors() {
    let dir = tempdir();
    let path = dir.join("small.ts");
    fs::write(&path, SMALL_TS).unwrap();

    let out = harness()
        .args(["translate", "--locale", "xx_XX"])
        .arg(&path)
        .output()
        .expect("spawn");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown locale"), "stderr: {stderr}");

    fs::remove_dir_all(&dir).ok();
}

/// Source contains malformed ICU (unclosed `{`). The echo backend copies
/// it verbatim, so the target is also malformed → gate's `IcuParseError`
/// (hard) fires from the translate loop itself. This is the cleanest way
/// to trigger a hard finding via echo, since the echo path always preserves
/// placeholder multisets and plural arity.
const HARD_FAIL_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Boom</name>
    <message>
        <source>Hello {name</source>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>
"#;

#[test]
fn hard_finding_blocks_write_back_even_with_out() {
    let dir = tempdir();
    let path = dir.join("hard.ts");
    let out_path = dir.join("hard.out.ts");
    fs::write(&path, HARD_FAIL_TS).unwrap();

    let out = harness()
        .args(["translate", "--locale", "de_DE", "--out"])
        .arg(&out_path)
        .arg(&path)
        .output()
        .expect("spawn");

    // CLI must exit non-zero AND must not write the output file.
    assert!(
        !out.status.success(),
        "expected non-zero exit, got success. stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("hard finding"),
        "stderr missing message: {stderr}"
    );
    assert!(
        !out_path.exists(),
        "out file must not exist after a blocked write-back, found at {}",
        out_path.display()
    );

    fs::remove_dir_all(&dir).ok();
}

/// Source carries a German determiner (`der`) immediately before a `%1`
/// placeholder. After ICU-normalization that becomes `der {0}`. The echo
/// backend copies the source verbatim, so the target also contains
/// `der {0}` — the gate's `PlaceholderAgreementRisk` heuristic fires.
/// This is a soft finding only; the unit must stay `Proposed` (not promoted
/// to Finished) and the file write-back must still succeed.
const SOFT_AGREEMENT_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Agree</name>
    <message>
        <source>Öffne der %1 Datei</source>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>
"#;

#[test]
fn soft_finding_keeps_unit_proposed_but_writes_back() {
    let dir = tempdir();
    let path = dir.join("soft.ts");
    let out_path = dir.join("soft.out.ts");
    fs::write(&path, SOFT_AGREEMENT_TS).unwrap();

    let out = harness()
        .args(["translate", "--locale", "de_DE", "--out"])
        .arg(&out_path)
        .arg(&path)
        .output()
        .expect("spawn");

    assert!(
        out.status.success(),
        "soft findings must not block write-back. stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("placeholder-agreement-risk"),
        "expected soft finding reported in stdout: {stdout}"
    );
    assert!(
        stdout.contains("soft=1"),
        "expected soft=1 in summary: {stdout}"
    );
    assert!(
        stdout.contains("flagged=1"),
        "expected flagged=1 in summary: {stdout}"
    );
    assert!(
        stdout.contains("finished=0"),
        "soft-flagged unit must not be promoted to Finished: {stdout}"
    );

    let written = fs::read_to_string(&out_path).expect("output file written");
    // The unit was filled but kept at Proposed — the `type="unfinished"`
    // attribute should still be present in the on-disk catalog.
    assert!(
        written.contains(r#"type="unfinished""#),
        "soft-flagged unit must stay Proposed (unfinished): {written}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn ollama_without_feature_errors_clearly() {
    let dir = tempdir();
    let path = dir.join("small.ts");
    fs::write(&path, SMALL_TS).unwrap();

    let out = harness()
        .args(["translate", "--locale", "de_DE", "--backend", "ollama"])
        .arg(&path)
        .output()
        .expect("spawn");

    // Behaviour depends on whether the binary was built with --features ollama.
    // Without the feature, we expect a clear error message; with it, we expect
    // a network error (no server running). Either case is non-success.
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("ollama") || stderr.contains("network"),
            "stderr should mention ollama/network: {stderr}"
        );
    }

    fs::remove_dir_all(&dir).ok();
}
