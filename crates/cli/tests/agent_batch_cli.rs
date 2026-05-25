//! End-to-end CLI tests for `harness export-batch` and `harness import-batch`.
//!
//! These tests exercise the two-phase agent flow without a model. The
//! `export-batch` subcommand produces a folder; the tests hand-write
//! `targets.jsonl` and then run `import-batch` against it, mirroring
//! what a real agent would do.

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
        "i18n-harness-agent-{}-{}-{seq}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    fs::create_dir_all(&dir).expect("create tempdir");
    dir
}

/// Two untranslated singular units. Chosen to be gate-safe when echoed
/// verbatim (no ICU placeholders, no accelerators, short text).
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

/// One already-Finished unit followed by one Untranslated unit.
const WITH_FINISHED_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>App</name>
    <message>
        <source>Done</source>
        <translation>Fertig</translation>
    </message>
    <message>
        <source>Pending</source>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>
"#;

/// Source with a missing ICU brace — echoed verbatim fires a hard gate finding.
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

// ── export_batch_writes_complete_folder ──────────────────────────────────────

#[test]
fn export_batch_writes_complete_folder() {
    let dir = tempdir();
    let ts = dir.join("small.ts");
    let batch_dir = dir.join("batch");
    fs::write(&ts, SMALL_TS).unwrap();

    let out = harness()
        .args(["export-batch", "--locale", "de_DE", "--out"])
        .arg(&batch_dir)
        .arg(&ts)
        .output()
        .expect("spawn");

    assert!(
        out.status.success(),
        "export-batch failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // All five required files must exist.
    for name in &[
        "README.md",
        "prompt.md",
        "units.jsonl",
        "targets.jsonl",
        "meta.json",
    ] {
        let p = batch_dir.join(name);
        assert!(p.exists(), "missing {name} in export folder");
    }

    // units.jsonl must have exactly 2 lines (one per writable unit).
    let units = fs::read_to_string(batch_dir.join("units.jsonl")).unwrap();
    let line_count = units.lines().count();
    assert_eq!(
        line_count, 2,
        "expected 2 units in units.jsonl, got {line_count}"
    );

    // targets.jsonl is initially empty (the agent hasn't run yet).
    let targets = fs::read_to_string(batch_dir.join("targets.jsonl")).unwrap();
    assert!(
        targets.trim().is_empty(),
        "targets.jsonl should be empty initially"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── export_batch_round_trip_byte_identical ───────────────────────────────────

/// M0 invariant on the agent path: the adapter's round-trip contract must hold
/// for files written by `import-batch`.
///
/// Procedure:
/// 1. `export-batch` a fixture Qt `.ts`.
/// 2. Fill `targets.jsonl` with the source verbatim for each unit.
/// 3. `import-batch … --out <new>` to produce a translated catalog.
/// 4. `round-trip <new>` — the written catalog itself must be byte-stable
///    under extract → apply. This is the M0 invariant on the adapter, not
///    a claim that the translated file is byte-identical to the untranslated
///    input (it won't be, since translations are now filled in).
#[test]
fn export_batch_round_trip_byte_identical() {
    let dir = tempdir();
    let ts = dir.join("small.ts");
    let batch_dir = dir.join("batch");
    let out_ts = dir.join("out.ts");
    fs::write(&ts, SMALL_TS).unwrap();

    // Phase 1: export.
    let export_out = harness()
        .args(["export-batch", "--locale", "de_DE", "--out"])
        .arg(&batch_dir)
        .arg(&ts)
        .output()
        .expect("spawn export-batch");
    assert!(
        export_out.status.success(),
        "export-batch failed: {}",
        String::from_utf8_lossy(&export_out.stderr)
    );

    // Fill targets.jsonl: echo the source verbatim for each unit in the
    // order given by units.jsonl (the order Batch::new produces).
    let units_raw = fs::read_to_string(batch_dir.join("units.jsonl")).unwrap();
    let mut targets = String::new();
    for line in units_raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line).expect("parse units.jsonl line");
        let id = v["id"].as_str().expect("id");
        let source = v["source"].as_str().expect("source");
        let is_plural = v.get("plural_arity").is_some();
        if is_plural {
            let arity = v["plural_arity"].as_u64().unwrap() as usize;
            let texts: Vec<&str> = (0..arity).map(|_| source).collect();
            let texts_json = serde_json::to_string(&texts).unwrap();
            targets.push_str(&format!(
                "{{\"id\":\"{id}\",\"kind\":\"plural\",\"texts\":{texts_json}}}\n"
            ));
        } else {
            let escaped = source.replace('"', "\\\"");
            targets.push_str(&format!(
                "{{\"id\":\"{id}\",\"kind\":\"singular\",\"text\":\"{escaped}\"}}\n"
            ));
        }
    }
    fs::write(batch_dir.join("targets.jsonl"), &targets).unwrap();

    // Phase 2: import with --out.
    let import_out = harness()
        .args(["import-batch", "--apply"])
        .arg(&ts)
        .args(["--out"])
        .arg(&out_ts)
        .arg(&batch_dir)
        .output()
        .expect("spawn import-batch");
    assert!(
        import_out.status.success(),
        "import-batch failed: stderr={} stdout={}",
        String::from_utf8_lossy(&import_out.stderr),
        String::from_utf8_lossy(&import_out.stdout),
    );
    assert!(out_ts.exists(), "import-batch must have written --out file");

    // Phase 3: the M0 adapter round-trip check on the written catalog.
    // `harness round-trip` runs extract → apply and asserts byte-identity;
    // this verifies the adapter is not corrupted by the agent write-back path.
    let rt_out = harness()
        .arg("round-trip")
        .arg(&out_ts)
        .output()
        .expect("spawn round-trip");
    assert!(
        rt_out.status.success(),
        "round-trip check failed on import-batch output: stderr={} stdout={}",
        String::from_utf8_lossy(&rt_out.stderr),
        String::from_utf8_lossy(&rt_out.stdout),
    );

    fs::remove_dir_all(&dir).ok();
}

// ── import_batch_missing_targets_errors ──────────────────────────────────────

#[test]
fn import_batch_missing_targets_errors() {
    let dir = tempdir();
    let ts = dir.join("small.ts");
    let batch_dir = dir.join("batch");
    fs::write(&ts, SMALL_TS).unwrap();

    // Export to produce the folder structure.
    harness()
        .args(["export-batch", "--locale", "de_DE", "--out"])
        .arg(&batch_dir)
        .arg(&ts)
        .output()
        .expect("spawn export-batch");

    // targets.jsonl was created empty; it stays empty (no agent ran).
    // import-batch must exit non-zero and print a useful message.
    let out = harness()
        .args(["import-batch", "--apply"])
        .arg(&ts)
        .arg(&batch_dir)
        .output()
        .expect("spawn import-batch");

    assert!(
        !out.status.success(),
        "import-batch should fail when targets.jsonl is empty"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    // The error message should mention "incomplete" or "batch" in some form.
    assert!(
        stderr.contains("incomplete") || stderr.contains("batch") || stderr.contains("expected"),
        "stderr should describe the incomplete batch: {stderr}"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── import_batch_blocks_on_hard_findings ─────────────────────────────────────

/// When the agent's translation fires a hard gate finding, import-batch must
/// exit non-zero and must NOT write the --out file.
#[test]
fn import_batch_blocks_on_hard_findings() {
    let dir = tempdir();
    let ts = dir.join("hard.ts");
    let batch_dir = dir.join("batch");
    let out_ts = dir.join("out.ts");
    fs::write(&ts, HARD_FAIL_TS).unwrap();

    // Export — one writable unit (the malformed source).
    let export_out = harness()
        .args(["export-batch", "--locale", "de_DE", "--out"])
        .arg(&batch_dir)
        .arg(&ts)
        .output()
        .expect("spawn export-batch");
    assert!(
        export_out.status.success(),
        "export-batch failed: {}",
        String::from_utf8_lossy(&export_out.stderr)
    );

    // Read the unit id from units.jsonl so we can craft a valid targets line.
    let units_raw = fs::read_to_string(batch_dir.join("units.jsonl")).unwrap();
    let v: serde_json::Value = serde_json::from_str(units_raw.trim()).expect("parse units.jsonl");
    let id = v["id"].as_str().expect("id");
    let source = v["source"].as_str().expect("source");

    // Echo the malformed source back — this fires IcuParseError (hard).
    let escaped = source.replace('"', "\\\"");
    let targets = format!("{{\"id\":\"{id}\",\"kind\":\"singular\",\"text\":\"{escaped}\"}}\n");
    fs::write(batch_dir.join("targets.jsonl"), targets).unwrap();

    let import_out = harness()
        .args(["import-batch", "--apply"])
        .arg(&ts)
        .args(["--out"])
        .arg(&out_ts)
        .arg(&batch_dir)
        .output()
        .expect("spawn import-batch");

    assert!(
        !import_out.status.success(),
        "import-batch must exit non-zero on a hard finding"
    );
    assert!(
        !out_ts.exists(),
        "--out file must not be written when a hard finding blocks write-back"
    );
    let stderr = String::from_utf8_lossy(&import_out.stderr);
    assert!(
        stderr.contains("hard"),
        "stderr should mention 'hard': {stderr}"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── export_batch_skips_finished_units ────────────────────────────────────────

/// A Finished unit must not appear in units.jsonl — agents must not overwrite
/// accepted translations.
#[test]
fn export_batch_skips_finished_units() {
    let dir = tempdir();
    let ts = dir.join("with_finished.ts");
    let batch_dir = dir.join("batch");
    fs::write(&ts, WITH_FINISHED_TS).unwrap();

    let out = harness()
        .args(["export-batch", "--locale", "de_DE", "--out"])
        .arg(&batch_dir)
        .arg(&ts)
        .output()
        .expect("spawn export-batch");
    assert!(
        out.status.success(),
        "export-batch failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let units_raw = fs::read_to_string(batch_dir.join("units.jsonl")).unwrap();
    let line_count = units_raw.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(
        line_count, 1,
        "only the Untranslated unit should appear in units.jsonl, got {line_count}"
    );

    // The one line must be for "Pending", not "Done".
    let v: serde_json::Value = serde_json::from_str(units_raw.trim()).expect("parse units.jsonl");
    let source = v["source"].as_str().expect("source");
    assert_eq!(
        source, "Pending",
        "finished unit 'Done' must be excluded, got source={source:?}"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── Project-mode helpers ─────────────────────────────────────────────────────

/// A minimal Qt `.ts` for `de_DE` with two untranslated singular units.
const DE_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Main</name>
    <message>
        <source>Open</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <source>Close</source>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>
"#;

/// A minimal Qt `.ts` for `es_ES` with one untranslated singular unit.
const ES_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="es_ES" sourcelanguage="en">
<context>
    <name>Main</name>
    <message>
        <source>Open</source>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>
"#;

/// A minimal PO file accepted by the format sniffer.
const DE_PO: &str = "msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n\"Language: de_DE\\n\"\n\nmsgid \"Hello\"\nmsgstr \"\"\n";

/// Build an `i18n-harness.toml` manifest string registering the given catalogs.
fn make_manifest(catalogs: &[(&str, &str, &str)]) -> String {
    let mut s = "[project]\nname = \"test-proj\"\nschema = 1\n\n".to_owned();
    for (path, fmt, locale) in catalogs {
        s.push_str(&format!(
            "[[catalogs]]\npath = \"{path}\"\nformat = \"{fmt}\"\nlocale = \"{locale}\"\n\n"
        ));
    }
    s
}

/// Write the project fixture to disk.
fn write_project(root: &std::path::Path, manifest: &str, files: &[(&str, &str)]) {
    fs::write(root.join("i18n-harness.toml"), manifest).unwrap();
    for (rel, content) in files {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, content).unwrap();
    }
    fs::create_dir_all(root.join(".i18n-harness")).unwrap();
}

/// Fill `targets.jsonl` in `batch_subdir` by echoing units verbatim.
fn fill_targets_verbatim(batch_subdir: &std::path::Path) {
    let units_raw = fs::read_to_string(batch_subdir.join("units.jsonl")).unwrap();
    let mut targets = String::new();
    for line in units_raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(trimmed).expect("parse units.jsonl");
        let id = v["id"].as_str().expect("id");
        let source = v["source"].as_str().expect("source");
        let escaped = source.replace('"', "\\\"");
        targets.push_str(&format!(
            "{{\"id\":\"{id}\",\"kind\":\"singular\",\"text\":\"{escaped}\"}}\n"
        ));
    }
    fs::write(batch_subdir.join("targets.jsonl"), targets).unwrap();
}

// ── export_batch_project_writes_subfolder_per_matching_catalog ───────────────

#[test]
fn export_batch_project_writes_subfolder_per_matching_catalog() {
    let dir = tempdir();
    let proj = dir.join("proj");
    fs::create_dir_all(&proj).unwrap();

    let manifest = make_manifest(&[("de1.ts", "qt-ts", "de_DE"), ("de2.ts", "qt-ts", "de_DE")]);
    write_project(&proj, &manifest, &[("de1.ts", DE_TS), ("de2.ts", DE_TS)]);

    let batch = dir.join("batch");
    let out = harness()
        .args(["export-batch", "--locale", "de_DE", "--project"])
        .arg(&proj)
        .args(["--out"])
        .arg(&batch)
        .output()
        .expect("spawn export-batch --project");

    assert!(
        out.status.success(),
        "export-batch --project failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(batch.join("README.md").exists(), "missing root README.md");
    assert!(batch.join("meta.json").exists(), "missing root meta.json");

    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(batch.join("meta.json")).unwrap()).unwrap();
    assert_eq!(meta["mode"], "project", "mode must be 'project'");
    assert_eq!(meta["locale_id"], "de_DE");

    let subfolders = meta["subfolders"].as_array().unwrap();
    assert_eq!(subfolders.len(), 2, "expected 2 subfolders");

    for sf in subfolders {
        let sfdir = batch.join(sf.as_str().unwrap());
        assert!(
            sfdir.join("units.jsonl").exists(),
            "missing units.jsonl in {sf}"
        );
        assert!(
            sfdir.join("targets.jsonl").exists(),
            "missing targets.jsonl in {sf}"
        );
        assert!(
            sfdir.join("meta.json").exists(),
            "missing meta.json in {sf}"
        );
        let name = sf.as_str().unwrap();
        assert!(!name.contains('/'), "slug must not contain slashes: {name}");
    }

    fs::remove_dir_all(&dir).ok();
}

// ── export_batch_project_skips_non_matching_locales ──────────────────────────

#[test]
fn export_batch_project_skips_non_matching_locales() {
    let dir = tempdir();
    let proj = dir.join("proj");
    fs::create_dir_all(&proj).unwrap();

    let manifest = make_manifest(&[("de.ts", "qt-ts", "de_DE"), ("es.ts", "qt-ts", "es_ES")]);
    write_project(&proj, &manifest, &[("de.ts", DE_TS), ("es.ts", ES_TS)]);

    let batch = dir.join("batch");
    let out = harness()
        .args(["export-batch", "--locale", "de_DE", "--project"])
        .arg(&proj)
        .args(["--out"])
        .arg(&batch)
        .output()
        .expect("spawn");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(batch.join("meta.json")).unwrap()).unwrap();
    let subfolders = meta["subfolders"].as_array().unwrap();
    assert_eq!(subfolders.len(), 1, "only de_DE catalog should match");

    let sf = batch.join(subfolders[0].as_str().unwrap());
    let sub_meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(sf.join("meta.json")).unwrap()).unwrap();
    let catalog_path = sub_meta["source_catalog_path"].as_str().unwrap();
    assert!(
        catalog_path.ends_with("de.ts"),
        "only de.ts should appear, got {catalog_path}"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── export_batch_project_errors_when_no_catalog_matches ──────────────────────

#[test]
fn export_batch_project_errors_when_no_catalog_matches() {
    let dir = tempdir();
    let proj = dir.join("proj");
    fs::create_dir_all(&proj).unwrap();

    let manifest = make_manifest(&[("de.ts", "qt-ts", "de_DE")]);
    write_project(&proj, &manifest, &[("de.ts", DE_TS)]);

    let batch = dir.join("batch");
    let out = harness()
        .args(["export-batch", "--locale", "fr_FR", "--project"])
        .arg(&proj)
        .args(["--out"])
        .arg(&batch)
        .output()
        .expect("spawn");

    assert!(
        !out.status.success(),
        "expected non-zero exit for missing locale"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("fr_FR"),
        "stderr must mention fr_FR: {stderr}"
    );
    assert!(
        stderr.contains("de_DE"),
        "stderr must list actual locales: {stderr}"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── import_batch_project_round_trip_byte_stable_per_catalog ──────────────────

#[test]
fn import_batch_project_round_trip_byte_stable_per_catalog() {
    let dir = tempdir();
    let proj = dir.join("proj");
    fs::create_dir_all(&proj).unwrap();

    let manifest = make_manifest(&[("de1.ts", "qt-ts", "de_DE"), ("de2.ts", "qt-ts", "de_DE")]);
    write_project(&proj, &manifest, &[("de1.ts", DE_TS), ("de2.ts", DE_TS)]);

    let batch = dir.join("batch");
    let export_out = harness()
        .args(["export-batch", "--locale", "de_DE", "--project"])
        .arg(&proj)
        .args(["--out"])
        .arg(&batch)
        .output()
        .expect("spawn export-batch");
    assert!(
        export_out.status.success(),
        "export-batch failed: {}",
        String::from_utf8_lossy(&export_out.stderr)
    );

    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(batch.join("meta.json")).unwrap()).unwrap();
    for sf in meta["subfolders"].as_array().unwrap() {
        fill_targets_verbatim(&batch.join(sf.as_str().unwrap()));
    }

    let out_dir = dir.join("translated");
    let import_out = harness()
        .args(["import-batch", "--project"])
        .arg(&proj)
        .args(["--out-dir"])
        .arg(&out_dir)
        .arg(&batch)
        .output()
        .expect("spawn import-batch --project");
    assert!(
        import_out.status.success(),
        "import-batch --project failed: stderr={} stdout={}",
        String::from_utf8_lossy(&import_out.stderr),
        String::from_utf8_lossy(&import_out.stdout),
    );

    for sf in meta["subfolders"].as_array().unwrap() {
        let sfdir = batch.join(sf.as_str().unwrap());
        let sub_meta: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(sfdir.join("meta.json")).unwrap()).unwrap();
        let source_cat = sub_meta["source_catalog_path"].as_str().unwrap();
        let manifest_rel = std::path::Path::new(source_cat)
            .strip_prefix(&proj)
            .unwrap_or(std::path::Path::new(source_cat));
        let written = out_dir.join(manifest_rel);
        assert!(
            written.exists(),
            "written catalog missing: {}",
            written.display()
        );

        let rt = harness()
            .arg("round-trip")
            .arg(&written)
            .output()
            .expect("spawn round-trip");
        assert!(
            rt.status.success(),
            "round-trip failed for {}: stderr={}",
            written.display(),
            String::from_utf8_lossy(&rt.stderr),
        );
    }

    fs::remove_dir_all(&dir).ok();
}

// ── import_batch_project_continues_past_hard_findings_in_one_catalog ─────────

#[test]
fn import_batch_project_continues_past_hard_findings_in_one_catalog() {
    const CATALOG_A_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
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
    const CATALOG_B_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Clean</name>
    <message>
        <source>Save</source>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>
"#;

    let dir = tempdir();
    let proj = dir.join("proj");
    fs::create_dir_all(&proj).unwrap();

    let manifest = make_manifest(&[("a.ts", "qt-ts", "de_DE"), ("b.ts", "qt-ts", "de_DE")]);
    write_project(
        &proj,
        &manifest,
        &[("a.ts", CATALOG_A_TS), ("b.ts", CATALOG_B_TS)],
    );

    let batch = dir.join("batch");
    let export_out = harness()
        .args(["export-batch", "--locale", "de_DE", "--project"])
        .arg(&proj)
        .args(["--out"])
        .arg(&batch)
        .output()
        .expect("spawn export-batch");
    assert!(
        export_out.status.success(),
        "export-batch failed: {}",
        String::from_utf8_lossy(&export_out.stderr)
    );

    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(batch.join("meta.json")).unwrap()).unwrap();
    let subfolders = meta["subfolders"].as_array().unwrap();

    let mut sf_a: Option<PathBuf> = None;
    let mut sf_b: Option<PathBuf> = None;
    for sf in subfolders {
        let sfpath = batch.join(sf.as_str().unwrap());
        let sub_meta: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(sfpath.join("meta.json")).unwrap()).unwrap();
        let source = sub_meta["source_catalog_path"].as_str().unwrap();
        if source.ends_with("a.ts") {
            sf_a = Some(sfpath);
        } else if source.ends_with("b.ts") {
            sf_b = Some(sfpath);
        }
    }
    let sf_a = sf_a.expect("subfolder for a.ts");
    let sf_b = sf_b.expect("subfolder for b.ts");

    fill_targets_verbatim(&sf_a);
    fill_targets_verbatim(&sf_b);

    let out_dir = dir.join("translated");
    let import_out = harness()
        .args(["import-batch", "--project"])
        .arg(&proj)
        .args(["--out-dir"])
        .arg(&out_dir)
        .arg(&batch)
        .output()
        .expect("spawn import-batch --project");

    assert!(
        !import_out.status.success(),
        "import-batch should fail when a catalog has hard findings"
    );

    assert!(
        !out_dir.join("a.ts").exists(),
        "catalog A must not be written when it has hard findings"
    );

    assert!(
        out_dir.join("b.ts").exists(),
        "catalog B must be written even though catalog A had hard findings"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── export_batch_project_warns_on_non_qt_catalogs ────────────────────────────

#[test]
fn export_batch_project_warns_on_non_qt_catalogs() {
    let dir = tempdir();
    let proj = dir.join("proj");
    fs::create_dir_all(&proj).unwrap();

    let manifest = make_manifest(&[
        ("de.ts", "qt-ts", "de_DE"),
        ("de.po", "gettext-po", "de_DE"),
    ]);
    write_project(&proj, &manifest, &[("de.ts", DE_TS), ("de.po", DE_PO)]);

    let batch = dir.join("batch");
    let out = harness()
        .args(["export-batch", "--locale", "de_DE", "--project"])
        .arg(&proj)
        .args(["--out"])
        .arg(&batch)
        .output()
        .expect("spawn");

    assert!(
        out.status.success(),
        "export-batch should succeed (PO just warns): stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("warning"),
        "stderr must contain a 'warning' for the PO catalog: {stderr}"
    );

    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(batch.join("meta.json")).unwrap()).unwrap();
    let subfolders = meta["subfolders"].as_array().unwrap();
    assert_eq!(
        subfolders.len(),
        1,
        "only the Qt catalog should get a subfolder"
    );

    let sf = batch.join(subfolders[0].as_str().unwrap());
    let sub_meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(sf.join("meta.json")).unwrap()).unwrap();
    let source = sub_meta["source_catalog_path"].as_str().unwrap();
    assert!(
        source.ends_with("de.ts"),
        "subfolder must be for de.ts, got {source}"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── export_batch_project_slug_collision ──────────────────────────────────────

#[test]
fn export_batch_project_slug_collision() {
    let dir = tempdir();
    let proj = dir.join("proj");
    fs::create_dir_all(&proj).unwrap();

    // `a/b.ts` and `a_b.ts` produce the same slug after slash→underscore.
    let manifest = make_manifest(&[("a/b.ts", "qt-ts", "de_DE"), ("a_b.ts", "qt-ts", "de_DE")]);
    fs::create_dir_all(proj.join("a")).unwrap();
    write_project(&proj, &manifest, &[("a/b.ts", DE_TS), ("a_b.ts", DE_TS)]);

    let batch = dir.join("batch");
    let out = harness()
        .args(["export-batch", "--locale", "de_DE", "--project"])
        .arg(&proj)
        .args(["--out"])
        .arg(&batch)
        .output()
        .expect("spawn");

    assert!(
        out.status.success(),
        "export-batch failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(batch.join("meta.json")).unwrap()).unwrap();
    let subfolders: Vec<&str> = meta["subfolders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(subfolders.len(), 2, "both catalogs must produce subfolders");
    assert_ne!(
        subfolders[0], subfolders[1],
        "colliding paths must produce distinct slugs: {subfolders:?}"
    );
    assert!(
        subfolders.iter().any(|s| s.ends_with("-1")),
        "one slug must have a -1 collision suffix: {subfolders:?}"
    );

    fs::remove_dir_all(&dir).ok();
}
