//! Unit-style tests for the filename-based locale inference regex.
//!
//! These mirror the examples from design §3.3 and verify the normalization
//! of hyphens to underscores.

// The `locale_infer` module is `pub(crate)` — access through the public API
// of the discovery module by calling `Project::discover_with_fs` with a single
// file and inspecting the locale field, OR by directly testing via `#[cfg(test)]`
// in the module. Since the function is `pub(crate)` we test it indirectly through
// a discover call with a file whose name embeds a locale.

use std::path::PathBuf;
use std::sync::Arc;

use i18n_harness_project::{InMemoryFs, Project, ProjectFs};

const ROOT: &str = "/proj";

fn root() -> &'static std::path::Path {
    std::path::Path::new(ROOT)
}

fn discover_single(filename: &str, content: &[u8]) -> Option<String> {
    let fs = Arc::new(InMemoryFs::new());
    let path = PathBuf::from(ROOT).join(filename);
    fs.write_atomic(&path, content).unwrap();
    let draft = Project::discover_with_fs(root(), fs as Arc<dyn ProjectFs>).unwrap();
    draft.catalogs.first().and_then(|dc| dc.locale.clone())
}

// ── Spec examples from §3.3 ───────────────────────────────────────────────────

#[test]
fn app_de_ts_gives_de() {
    // Qt TS with language attr takes priority over filename inference.
    let locale =
        discover_single("app_de.ts", b"<?xml version=\"1.0\"?><TS language=\"de\"></TS>");
    assert_eq!(locale.as_deref(), Some("de"));
}

#[test]
fn app_de_de_ts_gives_de_de() {
    let locale =
        discover_single("app-de_DE.ts", b"<?xml version=\"1.0\"?><TS language=\"de_DE\"></TS>");
    assert_eq!(locale.as_deref(), Some("de_DE"));
}

#[test]
fn messages_zh_hans_json_gives_zh_hans() {
    // ICU-JSON with only filename inference (no __locale__ key).
    let locale = discover_single(
        "messages.zh_Hans.json",
        b"{\"hello\": \"Hello\", \"world\": \"World\"}",
    );
    assert_eq!(locale.as_deref(), Some("zh_Hans"));
}

#[test]
fn de_po_gives_de_from_header() {
    let po = b"msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain\\n\"\n\"Language: de\\n\"\n\nmsgid \"hi\"\nmsgstr \"Hallo\"";
    let locale = discover_single("de.po", po);
    assert_eq!(locale.as_deref(), Some("de"));
}

#[test]
fn de_po_gives_de_from_filename_when_no_header() {
    // A PO with pairs but no Language: header — locale from filename.
    let po = b"msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain\\n\"\n\nmsgid \"hi\"\nmsgstr \"Hallo\"";
    let locale = discover_single("de.po", po);
    assert_eq!(locale.as_deref(), Some("de"));
}

#[test]
fn random_ts_no_locale() {
    // TypeScript source — rejected entirely; no catalog produced.
    let ts_source = b"import { foo } from 'bar';\nexport const x: string = 'hello';";
    let draft = {
        let fs = Arc::new(InMemoryFs::new());
        let path = PathBuf::from(ROOT).join("random.ts");
        fs.write_atomic(&path, ts_source).unwrap();
        Project::discover_with_fs(root(), fs as Arc<dyn ProjectFs>).unwrap()
    };
    // TypeScript source should produce no recognized catalogs.
    assert!(
        draft.catalogs.is_empty(),
        "TypeScript source should not be classified as a catalog: {:?}",
        draft.catalogs
    );
}

// ── Hyphen normalization ───────────────────────────────────────────────────────

#[test]
fn hyphenated_locale_normalized_in_filename() {
    // File has no <TS language> attr — falls back to filename inference.
    // Hyphen in `de-AT` should normalize to underscore.
    let locale = discover_single("app-de-AT.ts", b"<TS></TS>");
    // The Qt sniffer (no language attr) + filename inference should find de_AT.
    assert_eq!(locale.as_deref(), Some("de_AT"));
}
