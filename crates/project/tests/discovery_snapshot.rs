//! Snapshot tests for `Project::discover_with_fs`.
//!
//! Each scenario sets up an `InMemoryFs` fixture tree at `/project`, runs
//! discovery, and snapshots the resulting `DraftManifest` as JSON via
//! `insta::assert_json_snapshot!`. Snapshots live under
//! `tests/snapshots/discovery_snapshot__<scenario>.snap`.
//!
//! Scenarios mirror design `docs/m4.1-project-crate-design.md` §11.
//! The walker's depth, skip-list, and per-format sniff behavior are all
//! locked here as observable contracts.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_project::{InMemoryFs, Project, ProjectFs};

const ROOT: &str = "/project";

fn root() -> &'static Path {
    Path::new(ROOT)
}

fn empty_fs() -> Arc<InMemoryFs> {
    Arc::new(InMemoryFs::new())
}

fn write(fs: &InMemoryFs, rel: &str, bytes: &[u8]) {
    let path = PathBuf::from(ROOT).join(rel);
    if let Some(parent) = path.parent() {
        fs.create_dir_all(parent).expect("create_dir_all");
    }
    fs.write_atomic(&path, bytes).expect("write_atomic");
}

fn discover(fs: Arc<InMemoryFs>) -> i18n_harness_project::DraftManifest {
    let fs_arc: Arc<dyn ProjectFs> = Arc::clone(&fs) as Arc<dyn ProjectFs>;
    Project::discover_with_fs(root(), fs_arc).expect("discover")
}

// ── Scenarios ────────────────────────────────────────────────────────────────

#[test]
fn qt_only() {
    let fs = empty_fs();
    write(
        &fs,
        "translations/app_de.ts",
        br#"<?xml version="1.0"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE"><context></context></TS>"#,
    );
    write(
        &fs,
        "translations/app_fr.ts",
        br#"<?xml version="1.0"?>
<!DOCTYPE TS>
<TS version="2.1" language="fr_FR"><context></context></TS>"#,
    );

    insta::assert_json_snapshot!(discover(fs));
}

#[test]
fn po_only() {
    let fs = empty_fs();
    write(
        &fs,
        "locale/de.po",
        br#"msgid ""
msgstr ""
"Content-Type: text/plain; charset=UTF-8\n"
"Language: de_DE\n"

msgid "Hello"
msgstr "Hallo"
"#,
    );
    write(
        &fs,
        "locale/fr.po",
        br#"msgid ""
msgstr ""
"Content-Type: text/plain; charset=UTF-8\n"
"Language: fr_FR\n"

msgid "Hello"
msgstr "Bonjour"
"#,
    );

    insta::assert_json_snapshot!(discover(fs));
}

#[test]
fn icu_json_only() {
    let fs = empty_fs();
    write(&fs, "i18n/en.json", br#"{"hello":"Hello","bye":"Bye"}"#);
    write(
        &fs,
        "i18n/de.json",
        r#"{"hello":"Hallo","bye":"Tschüss"}"#.as_bytes(),
    );
    write(
        &fs,
        "i18n/fr.json",
        br#"{"hello":"Bonjour","bye":"Au revoir"}"#,
    );

    insta::assert_json_snapshot!(discover(fs));
}

#[test]
fn mixed() {
    let fs = empty_fs();
    write(
        &fs,
        "qt/app_de.ts",
        br#"<?xml version="1.0"?><TS version="2.1" language="de_DE"></TS>"#,
    );
    write(
        &fs,
        "po/de.po",
        br#"msgid ""
msgstr ""
"Content-Type: text/plain; charset=UTF-8\n"
"Language: de_DE\n"
"#,
    );
    write(&fs, "json/de.json", br#"{"hello":"Hallo"}"#);

    insta::assert_json_snapshot!(discover(fs));
}

#[test]
fn misclassified_typescript() {
    let fs = empty_fs();
    write(
        &fs,
        "src/index.ts",
        br#"import { foo } from 'bar';
export const greeting = 'Hello';
function main() { return greeting; }
"#,
    );

    let draft = discover(fs);
    assert!(
        draft.catalogs.is_empty(),
        "TypeScript file was misclassified as a catalog: {:?}",
        draft.catalogs
    );
    insta::assert_json_snapshot!(draft);
}

#[test]
fn ambiguous_json() {
    let fs = empty_fs();
    write(
        &fs,
        "package.json",
        br#"{"name":"myapp","version":"1.0.0","dependencies":{}}"#,
    );
    write(
        &fs,
        "tsconfig.json",
        br#"{"compilerOptions":{"target":"es2020"}}"#,
    );
    write(&fs, "i18n/en.json", br#"{"hello":"Hello"}"#);

    let draft = discover(fs);
    let catalog_paths: Vec<String> = draft
        .catalogs
        .iter()
        .map(|c| c.path.display().to_string())
        .collect();
    assert!(
        !catalog_paths.iter().any(|p| p.ends_with("package.json")),
        "package.json should not be classified as an i18n catalog"
    );
    assert!(
        !catalog_paths.iter().any(|p| p.ends_with("tsconfig.json")),
        "tsconfig.json should not be classified as an i18n catalog"
    );
    insta::assert_json_snapshot!(draft);
}

#[test]
fn no_catalogs() {
    let fs = empty_fs();
    write(&fs, "README.md", b"# my project\n");
    write(&fs, "src/main.rs", b"fn main() {}\n");

    let draft = discover(fs);
    assert!(draft.catalogs.is_empty());
    assert!(draft.locales.is_empty());
    insta::assert_json_snapshot!(draft);
}

#[test]
fn nested() {
    let fs = empty_fs();
    write(
        &fs,
        "a.ts",
        br#"<?xml version="1.0"?><TS version="2.1" language="de_DE"></TS>"#,
    );
    write(
        &fs,
        "d1/d2/d3/d.ts",
        br#"<?xml version="1.0"?><TS version="2.1" language="fr_FR"></TS>"#,
    );
    write(
        &fs,
        "d1/d2/d3/d4/d5/d6/d7/deep.ts",
        br#"<?xml version="1.0"?><TS version="2.1" language="es_ES"></TS>"#,
    );
    write(
        &fs,
        "d1/d2/d3/d4/d5/d6/d7/d8/too_deep.ts",
        br#"<?xml version="1.0"?><TS version="2.1" language="zh_Hans"></TS>"#,
    );

    insta::assert_json_snapshot!(discover(fs));
}

#[test]
fn with_glossary() {
    let fs = empty_fs();
    write(
        &fs,
        "glossary.toml",
        r#"[meta]
schema_version = 1

[[term]]
source = "Open"
do_not_translate = false
translations = { de_DE = "Öffnen" }
"#
        .as_bytes(),
    );
    write(
        &fs,
        "translations/app_de.ts",
        br#"<?xml version="1.0"?><TS version="2.1" language="de_DE"></TS>"#,
    );

    insta::assert_json_snapshot!(discover(fs));
}
