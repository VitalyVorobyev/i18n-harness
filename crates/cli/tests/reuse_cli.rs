//! End-to-end CLI tests for `harness reuse`, `harness split-remainder`, and
//! `harness merge`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn harness() -> Command {
    Command::new(env!("CARGO_BIN_EXE_harness"))
}

fn tempdir() -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "i18n-harness-reuse-{}-{}-{seq}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    fs::create_dir_all(&dir).expect("create tempdir");
    dir
}

// ── Fixtures ─────────────────────────────────────────────────────────────────

/// Two untranslated units in de_DE.
const BASE_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>App</name>
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

/// Reference with both units translated (Finished).
const REF_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>App</name>
    <message>
        <source>Hello</source>
        <translation>Hallo</translation>
    </message>
    <message>
        <source>Goodbye</source>
        <translation>Auf Wiedersehen</translation>
    </message>
</context>
</TS>
"#;

/// Reference that only translates "Hello". "Goodbye" is missing.
const REF_PARTIAL_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>App</name>
    <message>
        <source>Hello</source>
        <translation>Hallo</translation>
    </message>
</context>
</TS>
"#;

/// Second reference that disagrees on "Hello" (conflict scenario).
const REF_CONFLICT_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>App</name>
    <message>
        <source>Hello</source>
        <translation>Guten Tag</translation>
    </message>
</context>
</TS>
"#;

// ── reuse ad-hoc: shared id copied, non-shared stays untranslated ────────────

#[test]
fn reuse_adhoc_copies_shared_id_leaves_unshared_untranslated() {
    let dir = tempdir();
    let base = dir.join("base.ts");
    let reference = dir.join("ref.ts");
    let out = dir.join("out.ts");
    fs::write(&base, BASE_TS).unwrap();
    fs::write(&reference, REF_PARTIAL_TS).unwrap();

    let result = harness()
        .args(["reuse", "--locale", "de_DE", "--reference"])
        .arg(&reference)
        .args(["--out"])
        .arg(&out)
        .arg(&base)
        .output()
        .expect("spawn reuse");

    assert!(
        result.status.success(),
        "reuse failed: stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(out.exists(), "--out file must exist after reuse");

    // Verify round-trip stability of the output.
    let rt = harness()
        .arg("round-trip")
        .arg(&out)
        .output()
        .expect("spawn round-trip");
    assert!(
        rt.status.success(),
        "round-trip failed: {}",
        String::from_utf8_lossy(&rt.stderr)
    );

    // The stdout report should mention 1 copied_finished (Hello) and 1 remaining (Goodbye).
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("copied_finished=1"),
        "expected copied_finished=1 in report: {stdout}"
    );
    assert!(
        stdout.contains("remaining=1"),
        "expected remaining=1 in report: {stdout}"
    );

    // The output .ts should contain "Hallo" (copied) and no "Auf Wiedersehen" (not in reference).
    let content = fs::read_to_string(&out).unwrap();
    assert!(
        content.contains("Hallo"),
        "copied translation must appear in output"
    );
    assert!(
        !content.contains("Auf Wiedersehen"),
        "non-reference unit must not get a translation"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── reuse ad-hoc: conflict — two references disagree ────────────────────────

#[test]
fn reuse_adhoc_conflict_reported_unit_left_untranslated() {
    let dir = tempdir();
    let base = dir.join("base.ts");
    let ref1 = dir.join("ref1.ts");
    let ref2 = dir.join("ref2.ts");
    let out = dir.join("out.ts");
    fs::write(&base, BASE_TS).unwrap();
    // ref1 says "Hallo", ref2 says "Guten Tag" — disagreement on Hello.
    fs::write(&ref1, REF_PARTIAL_TS).unwrap();
    fs::write(&ref2, REF_CONFLICT_TS).unwrap();

    let result = harness()
        .args(["reuse", "--locale", "de_DE", "--reference"])
        .arg(&ref1)
        .args(["--reference"])
        .arg(&ref2)
        .args(["--out"])
        .arg(&out)
        .arg(&base)
        .output()
        .expect("spawn reuse");

    // Conflicts are informational; exit must still be 0.
    assert!(
        result.status.success(),
        "reuse must exit 0 even with conflicts: stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );

    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("conflicts=1"),
        "expected conflicts=1 in report: {stdout}"
    );
    // Neither reference translation should appear in the output for Hello
    // (left untranslated for human resolution).
    let content = fs::read_to_string(&out).unwrap();
    assert!(
        !content.contains("Hallo"),
        "conflicted unit must not have any translation copied in"
    );
    assert!(
        !content.contains("Guten Tag"),
        "conflicted unit must not have any translation copied in"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── reuse ad-hoc: all units copied from a complete reference ─────────────────

#[test]
fn reuse_adhoc_all_units_copied_from_complete_reference() {
    let dir = tempdir();
    let base = dir.join("base.ts");
    let reference = dir.join("ref.ts");
    let out = dir.join("out.ts");
    fs::write(&base, BASE_TS).unwrap();
    fs::write(&reference, REF_TS).unwrap();

    let result = harness()
        .args(["reuse", "--locale", "de_DE", "--reference"])
        .arg(&reference)
        .args(["--out"])
        .arg(&out)
        .arg(&base)
        .output()
        .expect("spawn reuse");

    assert!(
        result.status.success(),
        "reuse failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("remaining=0"),
        "expected remaining=0 in report: {stdout}"
    );
    assert!(
        stdout.contains("conflicts=0"),
        "expected conflicts=0 in report: {stdout}"
    );

    let content = fs::read_to_string(&out).unwrap();
    assert!(content.contains("Hallo"), "Hello must be translated");
    assert!(
        content.contains("Auf Wiedersehen"),
        "Goodbye must be translated"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── split-remainder: output is exactly writable-untranslated units ───────────

#[test]
fn split_remainder_writes_writable_untranslated_units() {
    let dir = tempdir();

    // Base with one Finished unit and two Untranslated units.
    let base_with_finished: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>App</name>
    <message>
        <source>Done</source>
        <translation>Fertig</translation>
    </message>
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

    let base = dir.join("base.ts");
    let out = dir.join("remainder.ts");
    fs::write(&base, base_with_finished).unwrap();

    let result = harness()
        .arg("split-remainder")
        .arg(&base)
        .args(["--out"])
        .arg(&out)
        .output()
        .expect("spawn split-remainder");

    assert!(
        result.status.success(),
        "split-remainder failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(out.exists(), "remainder file must be written");

    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("2"),
        "stdout should report 2 units: {stdout}"
    );

    // The remainder must re-extract cleanly (round-trip stable).
    let rt = harness()
        .arg("round-trip")
        .arg(&out)
        .output()
        .expect("spawn round-trip");
    assert!(
        rt.status.success(),
        "round-trip failed on remainder: {}",
        String::from_utf8_lossy(&rt.stderr)
    );

    // The Finished unit must not appear in the remainder.
    let content = fs::read_to_string(&out).unwrap();
    assert!(
        !content.contains("Fertig"),
        "Finished unit must not appear in remainder"
    );
    assert!(
        content.contains("Hello"),
        "untranslated unit must appear in remainder"
    );
    assert!(
        content.contains("Goodbye"),
        "untranslated unit must appear in remainder"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── merge: base + translated remainder → complete merged file ───────────────

/// Full workflow: reuse fills part of base, split-remainder carves the leftover,
/// we simulate a translator filling the remainder, then merge folds it back.
#[test]
fn merge_produces_complete_catalog_from_disjoint_halves() {
    let dir = tempdir();

    // Base: Hello + Goodbye, both untranslated.
    let base = dir.join("base.ts");
    fs::write(&base, BASE_TS).unwrap();

    // Reference translates only Hello → after reuse, Hello is Finished and
    // Goodbye remains untranslated.
    let reference = dir.join("ref_partial.ts");
    fs::write(&reference, REF_PARTIAL_TS).unwrap();

    let reused = dir.join("reused.ts");
    let reuse_out = harness()
        .args(["reuse", "--locale", "de_DE", "--reference"])
        .arg(&reference)
        .args(["--out"])
        .arg(&reused)
        .arg(&base)
        .output()
        .expect("spawn reuse");
    assert!(
        reuse_out.status.success(),
        "reuse failed: {}",
        String::from_utf8_lossy(&reuse_out.stderr)
    );

    // Split: carve the leftover (Goodbye) into remainder.ts.
    let remainder = dir.join("remainder.ts");
    let split_out = harness()
        .arg("split-remainder")
        .arg(&reused)
        .args(["--out"])
        .arg(&remainder)
        .output()
        .expect("spawn split-remainder");
    assert!(
        split_out.status.success(),
        "split-remainder failed: {}",
        String::from_utf8_lossy(&split_out.stderr)
    );

    // Simulate translator: overwrite the remainder with a translated version.
    let translated_remainder: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>App</name>
    <message>
        <source>Goodbye</source>
        <translation>Auf Wiedersehen</translation>
    </message>
</context>
</TS>
"#;
    fs::write(&remainder, translated_remainder).unwrap();

    // Merge: fold remainder back into the reused base.
    let merged = dir.join("merged.ts");
    let merge_out = harness()
        .arg("merge")
        .arg(&reused)
        .args(["--with"])
        .arg(&remainder)
        .args(["--out"])
        .arg(&merged)
        .output()
        .expect("spawn merge");
    assert!(
        merge_out.status.success(),
        "merge failed: stderr={} stdout={}",
        String::from_utf8_lossy(&merge_out.stderr),
        String::from_utf8_lossy(&merge_out.stdout),
    );

    let stdout = String::from_utf8_lossy(&merge_out.stdout);
    assert!(
        stdout.contains("merged=1"),
        "expected merged=1 in report: {stdout}"
    );
    assert!(
        stdout.contains("merged_complete=1"),
        "expected merged_complete=1 in report: {stdout}"
    );

    // The merged file must contain both translations.
    let content = fs::read_to_string(&merged).unwrap();
    assert!(content.contains("Hallo"), "Hello must be in merged file");
    assert!(
        content.contains("Auf Wiedersehen"),
        "Goodbye must be in merged file"
    );

    // Round-trip stability of the merged file.
    let rt = harness()
        .arg("round-trip")
        .arg(&merged)
        .output()
        .expect("spawn round-trip");
    assert!(
        rt.status.success(),
        "round-trip failed on merged file: {}",
        String::from_utf8_lossy(&rt.stderr)
    );

    fs::remove_dir_all(&dir).ok();
}

// ── merge: overlap → non-zero exit ──────────────────────────────────────────

#[test]
fn merge_overlap_exits_nonzero() {
    let dir = tempdir();

    // Both base and remainder claim to have "Hello" as Finished.
    let base_finished: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>App</name>
    <message>
        <source>Hello</source>
        <translation>Hallo</translation>
    </message>
</context>
</TS>
"#;

    // Remainder also has Hello as Finished — overlapping with the base.
    let remainder_overlap: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>App</name>
    <message>
        <source>Hello</source>
        <translation>Servus</translation>
    </message>
</context>
</TS>
"#;

    let base = dir.join("base.ts");
    let remainder = dir.join("remainder.ts");
    let merged = dir.join("merged.ts");
    fs::write(&base, base_finished).unwrap();
    fs::write(&remainder, remainder_overlap).unwrap();

    let result = harness()
        .arg("merge")
        .arg(&base)
        .args(["--with"])
        .arg(&remainder)
        .args(["--out"])
        .arg(&merged)
        .output()
        .expect("spawn merge");

    assert!(
        !result.status.success(),
        "merge must exit non-zero on overlap"
    );
    assert!(
        !merged.exists(),
        "--out must not be written when merge aborts"
    );

    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("finished in both")
            || stderr.contains("overlap")
            || stderr.contains("disjoint"),
        "stderr must describe the overlap: {stderr}"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── merge: stray ids → non-zero exit ────────────────────────────────────────

#[test]
fn merge_stray_ids_exits_nonzero() {
    let dir = tempdir();

    // Base has only "Hello". Remainder has "Hello" + "Goodbye" — stray.
    let base_hello: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>App</name>
    <message>
        <source>Hello</source>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>
"#;
    let remainder_stray: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>App</name>
    <message>
        <source>Hello</source>
        <translation>Hallo</translation>
    </message>
    <message>
        <source>Goodbye</source>
        <translation>Auf Wiedersehen</translation>
    </message>
</context>
</TS>
"#;

    let base = dir.join("base.ts");
    let remainder = dir.join("remainder.ts");
    let merged = dir.join("merged.ts");
    fs::write(&base, base_hello).unwrap();
    fs::write(&remainder, remainder_stray).unwrap();

    let result = harness()
        .arg("merge")
        .arg(&base)
        .args(["--with"])
        .arg(&remainder)
        .args(["--out"])
        .arg(&merged)
        .output()
        .expect("spawn merge");

    assert!(
        !result.status.success(),
        "merge must exit non-zero on stray ids"
    );
    assert!(
        !merged.exists(),
        "--out must not be written when merge aborts"
    );

    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("not present in base") || stderr.contains("stray"),
        "stderr must describe the stray ids: {stderr}"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── reuse project mode: bases resolved from manifest, references from manifest ──

#[test]
fn reuse_project_mode_uses_manifest_references() {
    let dir = tempdir();
    let proj = dir.join("proj");
    fs::create_dir_all(&proj).unwrap();

    // Two base catalogs for de_DE, one reference for de_DE.
    fs::write(proj.join("base1.ts"), BASE_TS).unwrap();
    fs::write(proj.join("base2.ts"), BASE_TS).unwrap();
    fs::write(proj.join("ref.ts"), REF_TS).unwrap();
    fs::create_dir_all(proj.join(".i18n-harness")).unwrap();

    let manifest = r#"
[project]
name = "test"
schema = 1

[[catalogs]]
path = "base1.ts"
format = "qt-ts"
locale = "de_DE"

[[catalogs]]
path = "base2.ts"
format = "qt-ts"
locale = "de_DE"

[[references]]
path = "ref.ts"
format = "qt-ts"
locale = "de_DE"
"#;
    fs::write(proj.join("i18n-harness.toml"), manifest).unwrap();

    let result = harness()
        .args(["reuse", "--locale", "de_DE", "--project"])
        .arg(&proj)
        .output()
        .expect("spawn reuse --project");

    assert!(
        result.status.success(),
        "reuse --project failed: stderr={} stdout={}",
        String::from_utf8_lossy(&result.stderr),
        String::from_utf8_lossy(&result.stdout),
    );

    // Both base catalogs should have been reused in place.
    for name in &["base1.ts", "base2.ts"] {
        let content = fs::read_to_string(proj.join(name)).unwrap();
        assert!(
            content.contains("Hallo"),
            "{name} must contain the reused translation"
        );
        assert!(
            content.contains("Auf Wiedersehen"),
            "{name} must contain the reused translation"
        );

        // Each written file must be byte-stable.
        let rt = harness()
            .arg("round-trip")
            .arg(proj.join(name))
            .output()
            .expect("spawn round-trip");
        assert!(
            rt.status.success(),
            "round-trip failed for {name}: {}",
            String::from_utf8_lossy(&rt.stderr)
        );
    }

    fs::remove_dir_all(&dir).ok();
}
