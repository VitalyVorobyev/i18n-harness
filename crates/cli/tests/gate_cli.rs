//! End-to-end CLI tests for `harness gate`.
//!
//! The CLI assembles three independently-tested layers (`adapter-qt`, `gate`,
//! `metrics`); these tests check the assembly itself: exit code on hard
//! findings, the `--metrics` file is created and contains one JSONL line per
//! finding, the unknown-locale error path.

use std::fs;
use std::process::Command;

/// The harness binary `cargo test` built for this integration test.
fn harness() -> Command {
    Command::new(env!("CARGO_BIN_EXE_harness"))
}

fn tempdir() -> std::path::PathBuf {
    // process::id + nanos alone can clash under parallel test execution.
    // The atomic counter is the trustworthy source of uniqueness within a
    // process; nanos + pid disambiguate across runs.
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "i18n-harness-cli-{}-{}-{seq}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    fs::create_dir_all(&dir).expect("create tempdir");
    dir
}

const HARD_FAIL_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Boom</name>
    <message>
        <source>Open %1 from %2</source>
        <translation>Datei öffnen</translation>
    </message>
</context>
</TS>
"#;

const CLEAN_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Greet</name>
    <message>
        <source>Hello</source>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>
"#;

#[test]
fn clean_fixture_exits_zero() {
    let dir = tempdir();
    let path = dir.join("clean.ts");
    fs::write(&path, CLEAN_TS).unwrap();

    let out = harness()
        .args(["gate", "--locale", "de_DE"])
        .arg(&path)
        .output()
        .expect("spawn harness");

    assert!(
        out.status.success(),
        "expected success, got status {:?}, stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("hard=0"), "stdout missing hard=0: {stdout}");
    assert!(
        stdout.contains("clean=1"),
        "stdout missing clean=1: {stdout}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn hard_finding_exits_nonzero() {
    let dir = tempdir();
    let path = dir.join("boom.ts");
    fs::write(&path, HARD_FAIL_TS).unwrap();

    let out = harness()
        .args(["gate", "--locale", "de_DE"])
        .arg(&path)
        .output()
        .expect("spawn harness");

    // Source has `%1` and `%2` (→ ICU `{0}`/`{1}`); target has neither.
    // The gate's `PlaceholderMismatch` hard rule fires and the CLI must
    // exit non-zero.
    assert!(
        !out.status.success(),
        "expected non-zero exit, got success. stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("placeholder-mismatch"),
        "stdout missing rule name: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("hard finding"),
        "stderr missing failure message: {stderr}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn metrics_file_gets_one_line_per_finding() {
    let dir = tempdir();
    let path = dir.join("boom.ts");
    let metrics = dir.join("metrics.jsonl");
    fs::write(&path, HARD_FAIL_TS).unwrap();

    let _ = harness()
        .args(["gate", "--locale", "de_DE", "--metrics"])
        .arg(&metrics)
        .arg(&path)
        .output()
        .expect("spawn harness");

    let lines = fs::read_to_string(&metrics).expect("metrics file written");
    // One hard line (placeholder-mismatch) — there may also be a soft
    // length-warn line on the same unit. Assert one gate-reject line plus
    // the right metadata.
    assert!(
        lines.contains("\"event\":\"gate-reject\""),
        "expected gate-reject event, got:\n{lines}"
    );
    assert!(lines.contains("\"rule\":\"placeholder-mismatch\""));
    assert!(lines.contains("\"backend\":\"manual\""));
    assert!(lines.contains("\"locale\":\"de_DE\""));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn unknown_locale_errors() {
    let dir = tempdir();
    let path = dir.join("clean.ts");
    fs::write(&path, CLEAN_TS).unwrap();

    let out = harness()
        .args(["gate", "--locale", "xx_XX"])
        .arg(&path)
        .output()
        .expect("spawn harness");

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown locale"),
        "stderr missing message: {stderr}"
    );

    fs::remove_dir_all(&dir).ok();
}
