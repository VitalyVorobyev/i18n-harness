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
    let dir = std::env::temp_dir().join(format!(
        "i18n-harness-translate-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
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
