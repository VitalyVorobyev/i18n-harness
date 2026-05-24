//! Content sniffers for catalog format detection.
//!
//! Each sniffer reads up to 64 KiB of file bytes and decides whether the
//! content matches a known catalog format. See design §3.2.
//!
//! The top-level [`sniff`] function applies the extension prefilter, then the
//! appropriate content sniffer, and returns a populated [`DraftCatalog`].

use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use super::{ClassificationConfidence, DraftCatalog, FormatGuess};
use crate::discovery::locale_infer;

/// Result produced by a content sniffer for one format.
pub(crate) struct SniffResult {
    /// Whether the bytes confidently match this format.
    pub(crate) confidence: ClassificationConfidence,
    /// Locale inferred from the content (adapter-derived), if available.
    pub(crate) locale: Option<String>,
    /// Human-readable reason string surfaced verbatim in the UI.
    pub(crate) reason: String,
}

// ── Qt .ts sniffer ────────────────────────────────────────────────────────────

/// Regex to capture the `language` attribute of the `<TS>` root element.
fn qt_language_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"<TS\b[^>]*\blanguage="([^"]+)""#).expect("qt_language_re is valid")
    })
}

/// Patterns that strongly indicate TypeScript source rather than a Qt XML file.
fn ts_js_keywords_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b(?:import|export|const|let|function|class|interface|type)\b")
            .expect("ts_js_keywords_re is valid")
    })
}

/// Sniff bytes as a Qt Linguist `.ts` XML file.
///
/// Returns `Some(SniffResult)` when `<TS` is found in the first 1 KiB after
/// stripping any BOM and XML declaration. Returns `None` when the file is
/// definitively TypeScript source (JS keywords without `<TS`).
pub(crate) fn sniff_qt_ts(bytes: &[u8]) -> Option<SniffResult> {
    let prefix = strip_bom(bytes);
    // Only look at the first 1 KiB for the <TS element.
    let head_len = prefix.len().min(1024);
    let head = std::str::from_utf8(&prefix[..head_len]).unwrap_or("");

    let has_ts_element = head.contains("<TS");
    let has_js_keywords = ts_js_keywords_re().is_match(head);

    if has_ts_element && !has_js_keywords {
        // Confident Qt TS — extract locale if present.
        let full_text = std::str::from_utf8(prefix).unwrap_or(head);
        let locale = qt_language_re()
            .captures(full_text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().replace('-', "_"));

        let reason = if let Some(ref loc) = locale {
            format!("<TS language=\"{loc}\"> seen — Qt Linguist XML")
        } else {
            "<TS root element seen — Qt Linguist XML (no language attribute)".to_owned()
        };

        Some(SniffResult {
            confidence: ClassificationConfidence::High,
            locale,
            reason,
        })
    } else if has_js_keywords && !has_ts_element {
        // Definitely TypeScript source — signal rejection by returning None.
        None
    } else if !has_ts_element {
        // Not Qt TS and no JS keywords — unknown.
        None
    } else {
        // has_ts_element && has_js_keywords — very unusual; treat as Qt TS
        // because the XML element is the stronger signal.
        Some(SniffResult {
            confidence: ClassificationConfidence::Medium,
            locale: None,
            reason: "<TS element seen alongside JS keywords — treating as Qt Linguist XML"
                .to_owned(),
        })
    }
}

// ── PO sniffer ────────────────────────────────────────────────────────────────

/// Regex to capture the `Language:` value from the PO file header string.
fn po_language_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"Language:\s*([a-zA-Z]{2,3}(?:[_-][a-zA-Z]{2,4})?)\s*\\n"#)
            .expect("po_language_re is valid")
    })
}

/// Sniff bytes as a GNU gettext `.po` file.
///
/// Requires the classic header block (`msgid ""` / `msgstr ""` / `Content-Type:`).
/// Returns confidence `High` when a `Language:` header is present, `Medium`
/// otherwise. Returns `None` when none of the PO markers are found.
pub(crate) fn sniff_po(bytes: &[u8]) -> Option<SniffResult> {
    let text = std::str::from_utf8(bytes).unwrap_or("");

    // Must contain msgid/msgstr pairs to be a PO file at all.
    if !text.contains("msgid") || !text.contains("msgstr") {
        return None;
    }

    let has_content_type = text.contains("Content-Type:");
    let has_header_block = text.contains("msgid \"\"\nmsgstr \"\"")
        || text.contains("msgid \"\"\r\nmsgstr \"\"");

    if !has_header_block && !has_content_type {
        // Has msgid/msgstr pairs but no header — still accept as Medium.
        return Some(SniffResult {
            confidence: ClassificationConfidence::Medium,
            locale: None,
            reason: "msgid/msgstr pairs found without header — likely gettext PO".to_owned(),
        });
    }

    // Try to extract locale from Language: header.
    let locale = po_language_re()
        .captures(text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().replace('-', "_"));

    let (confidence, reason) = if let Some(ref loc) = locale {
        (
            ClassificationConfidence::High,
            format!("gettext PO header with Language: {loc}"),
        )
    } else {
        (
            ClassificationConfidence::Medium,
            "gettext PO header block found (no Language: header)".to_owned(),
        )
    };

    Some(SniffResult {
        confidence,
        locale,
        reason,
    })
}

// ── ICU-JSON sniffer ──────────────────────────────────────────────────────────

/// Top-level keys that indicate this JSON is a tool-config file, not a catalog.
const TOOL_CONFIG_SIGNATURES: &[(&str, &str)] = &[
    ("name", "version"),           // package.json
    ("compilerOptions", ""),       // tsconfig.json
    ("devDependencies", ""),       // package.json variant
    ("dependencies", "scripts"),   // package.json variant
];

/// Schema URL fragments that indicate non-i18n schemas.
const NON_I18N_SCHEMA_FRAGMENTS: &[&str] = &[
    "json-schema.org",
    "schemastore.org",
];

/// Sniff bytes as an ICU MessageFormat JSON file.
///
/// Rejects `package.json`, `tsconfig.json`, and JSON files with non-i18n
/// `$schema` values. Accepts flat key→string maps or common i18n-library
/// shapes (Lingui, i18next). Always returns `Medium` confidence — a JSON file
/// cannot be proven to be a catalog without knowing the consuming library.
pub(crate) fn sniff_icu_json(bytes: &[u8]) -> Option<SniffResult> {
    let text = std::str::from_utf8(bytes).unwrap_or("");

    // Must parse as JSON.
    let v: serde_json::Value = serde_json::from_str(text).ok()?;

    let obj = v.as_object()?;

    // Reject if it looks like a tool-config file.
    for (key_a, key_b) in TOOL_CONFIG_SIGNATURES {
        if obj.contains_key(*key_a) && (key_b.is_empty() || obj.contains_key(*key_b)) {
            return None;
        }
    }

    // Reject if $schema is present and not an i18n-known schema.
    if let Some(schema_val) = obj.get("$schema") {
        if let Some(schema_str) = schema_val.as_str() {
            let is_i18n_schema = schema_str.contains("i18n") || schema_str.contains("messageformat");
            let is_tool_schema = NON_I18N_SCHEMA_FRAGMENTS
                .iter()
                .any(|frag| schema_str.contains(frag));
            if is_tool_schema && !is_i18n_schema {
                return None;
            }
        }
    }

    // Try __locale__ key (some ICU-JSON conventions).
    let locale = obj
        .get("__locale__")
        .and_then(|v| v.as_str())
        .map(|s| s.replace('-', "_"));

    // Must have at least one string-valued key to qualify as a catalog.
    let has_string_values = obj.values().any(|v| {
        v.is_string()
            || v.as_object()
                .map(|o| o.contains_key("defaultMessage") || o.contains_key("string"))
                .unwrap_or(false)
    });

    if !has_string_values {
        return None;
    }

    let reason = if locale.is_some() {
        format!(
            "flat JSON map with __locale__ key — ICU-JSON catalog (locale: {})",
            locale.as_deref().unwrap_or("")
        )
    } else {
        "flat JSON key→string map — likely ICU-JSON catalog".to_owned()
    };

    Some(SniffResult {
        confidence: ClassificationConfidence::Medium,
        locale,
        reason,
    })
}

// ── Top-level dispatcher ──────────────────────────────────────────────────────

/// Sniff `bytes` (up to 64 KiB prefix) for `path` and return a [`DraftCatalog`].
///
/// Applies the extension prefilter, then the appropriate content sniffer,
/// then the filename-based locale inference fallback when the sniffer found no
/// locale. Produces `FormatGuess::Unknown` when nothing matched.
pub(crate) fn sniff(path: &Path, bytes: &[u8]) -> DraftCatalog {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    match ext.as_str() {
        "ts" => sniff_ts_path(path, filename, bytes),
        "po" | "pot" => sniff_po_path(filename, bytes),
        "json" => sniff_json_path(filename, bytes),
        _ => DraftCatalog {
            path: path.to_path_buf(),
            format: FormatGuess::Unknown,
            locale: None,
            confidence: ClassificationConfidence::Low,
            reason: format!("extension `.{ext}` is not a known catalog extension"),
            alternatives: vec![],
        },
    }
}

fn sniff_ts_path(path: &Path, filename: &str, bytes: &[u8]) -> DraftCatalog {
    match sniff_qt_ts(bytes) {
        Some(result) => {
            let locale = result.locale.or_else(|| locale_infer::infer_from_filename(filename));
            let confidence = if locale.is_some() {
                result.confidence
            } else {
                // Locale not found — downgrade one level.
                match result.confidence {
                    ClassificationConfidence::High => ClassificationConfidence::Medium,
                    other => other,
                }
            };
            DraftCatalog {
                path: path.to_path_buf(),
                format: FormatGuess::QtTs,
                locale,
                confidence,
                reason: result.reason,
                alternatives: vec![],
            }
        }
        None => {
            // Sniffer rejected it (TypeScript source or truly unknown).
            let is_ts_source = {
                let head = std::str::from_utf8(bytes.get(..1024).unwrap_or(bytes))
                    .unwrap_or("");
                ts_js_keywords_re().is_match(head)
            };
            let reason = if is_ts_source {
                "TypeScript source keywords found — not a Qt Linguist file".to_owned()
            } else {
                "no <TS element found — not a Qt Linguist file".to_owned()
            };
            DraftCatalog {
                path: path.to_path_buf(),
                format: FormatGuess::Unknown,
                locale: None,
                confidence: ClassificationConfidence::Low,
                reason,
                alternatives: vec![],
            }
        }
    }
}

fn sniff_po_path(filename: &str, bytes: &[u8]) -> DraftCatalog {
    // Path is not stored for PO — reconstruct from filename for draft.
    let path = std::path::PathBuf::from(filename);
    match sniff_po(bytes) {
        Some(result) => {
            let locale = result.locale.or_else(|| locale_infer::infer_from_filename(filename));
            DraftCatalog {
                path,
                format: FormatGuess::GettextPo,
                locale,
                confidence: result.confidence,
                reason: result.reason,
                alternatives: vec![],
            }
        }
        None => DraftCatalog {
            path,
            format: FormatGuess::Unknown,
            locale: None,
            confidence: ClassificationConfidence::Low,
            reason: "no gettext msgid/msgstr markers found".to_owned(),
            alternatives: vec![],
        },
    }
}

fn sniff_json_path(filename: &str, bytes: &[u8]) -> DraftCatalog {
    let path = std::path::PathBuf::from(filename);
    match sniff_icu_json(bytes) {
        Some(result) => {
            let locale = result.locale.or_else(|| locale_infer::infer_from_filename(filename));
            DraftCatalog {
                path,
                format: FormatGuess::IcuJson,
                locale,
                confidence: result.confidence,
                reason: result.reason,
                alternatives: vec![],
            }
        }
        None => DraftCatalog {
            path,
            format: FormatGuess::Unknown,
            locale: None,
            confidence: ClassificationConfidence::Low,
            reason: "JSON rejected as non-catalog (tool-config signature or no string values)"
                .to_owned(),
            alternatives: vec![],
        },
    }
}

/// Check whether `bytes` are consistent with `declared_format`.
///
/// Used in `Project::open` to detect format mismatch on user-written manifests.
/// Returns `true` if the content passes the sniffer for the declared format,
/// `false` if it does not.
///
/// This is intentionally permissive for the pass case: we only reject when
/// the sniffer is confident the format is *wrong*, not when it is merely
/// unknown.
pub(crate) fn confirms_format(bytes: &[u8], declared: crate::manifest::CatalogFormat) -> bool {
    use crate::manifest::CatalogFormat;
    match declared {
        CatalogFormat::QtTs => sniff_qt_ts(bytes).is_some(),
        CatalogFormat::GettextPo => sniff_po(bytes).is_some(),
        CatalogFormat::IcuJson => sniff_icu_json(bytes).is_some(),
    }
}

/// Guess the format of `bytes` (used to populate `sniffed` in error messages).
pub(crate) fn guess_format(bytes: &[u8]) -> FormatGuess {
    if sniff_qt_ts(bytes).is_some() {
        FormatGuess::QtTs
    } else if sniff_po(bytes).is_some() {
        FormatGuess::GettextPo
    } else if sniff_icu_json(bytes).is_some() {
        FormatGuess::IcuJson
    } else {
        FormatGuess::Unknown
    }
}

/// Strip a UTF-8 BOM from `bytes` if present.
fn strip_bom(bytes: &[u8]) -> &[u8] {
    if bytes.starts_with(b"\xef\xbb\xbf") {
        &bytes[3..]
    } else {
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Qt TS ─────────────────────────────────────────────────────────────────

    #[test]
    fn qt_ts_basic() {
        let result = sniff_qt_ts(b"<?xml version=\"1.0\"?><TS version=\"2.1\" language=\"de_DE\"></TS>");
        let r = result.unwrap();
        assert_eq!(r.locale.as_deref(), Some("de_DE"));
        assert_eq!(r.confidence, ClassificationConfidence::High);
    }

    #[test]
    fn qt_ts_after_bom() {
        let mut bytes = vec![0xef, 0xbb, 0xbf]; // UTF-8 BOM
        bytes.extend_from_slice(b"<TS language=\"de\"></TS>");
        let result = sniff_qt_ts(&bytes);
        let r = result.unwrap();
        assert_eq!(r.locale.as_deref(), Some("de"));
    }

    #[test]
    fn qt_ts_no_language_attr() {
        let result = sniff_qt_ts(b"<TS></TS>");
        let r = result.unwrap();
        assert!(r.locale.is_none());
        assert_eq!(r.confidence, ClassificationConfidence::High);
    }

    #[test]
    fn typescript_source_rejected() {
        let ts_source = b"import { foo } from 'bar';\nexport const x = 1;";
        assert!(sniff_qt_ts(ts_source).is_none());
    }

    #[test]
    fn random_content_rejected() {
        assert!(sniff_qt_ts(b"just some text").is_none());
    }

    // ── PO ────────────────────────────────────────────────────────────────────

    #[test]
    fn po_with_language_header() {
        let po = b"msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain\\n\"\n\"Language: de_DE\\n\"\n\nmsgid \"hello\"\nmsgstr \"Hallo\"";
        let result = sniff_po(po);
        let r = result.unwrap();
        assert_eq!(r.locale.as_deref(), Some("de_DE"));
        assert_eq!(r.confidence, ClassificationConfidence::High);
    }

    #[test]
    fn po_without_language_header() {
        let po = b"msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain\\n\"\n\nmsgid \"hello\"\nmsgstr \"Hallo\"";
        let result = sniff_po(po);
        let r = result.unwrap();
        assert!(r.locale.is_none());
        assert_eq!(r.confidence, ClassificationConfidence::Medium);
    }

    #[test]
    fn non_po_rejected() {
        assert!(sniff_po(b"<TS></TS>").is_none());
        assert!(sniff_po(b"{\"key\": \"value\"}").is_none());
    }

    // ── ICU-JSON ──────────────────────────────────────────────────────────────

    #[test]
    fn icu_json_flat_map() {
        let json = b"{\"hello\": \"Hello\", \"world\": \"World\"}";
        let result = sniff_icu_json(json);
        assert!(result.is_some());
    }

    #[test]
    fn package_json_rejected() {
        let pkg = b"{\"name\": \"my-app\", \"version\": \"1.0.0\", \"scripts\": {}}";
        assert!(sniff_icu_json(pkg).is_none());
    }

    #[test]
    fn tsconfig_json_rejected() {
        let tsconfig = b"{\"compilerOptions\": {\"target\": \"es2020\"}}";
        assert!(sniff_icu_json(tsconfig).is_none());
    }

    #[test]
    fn json_with_non_i18n_schema_rejected() {
        let json = b"{\"$schema\": \"https://json.schemastore.org/package\", \"name\": \"test\"}";
        assert!(sniff_icu_json(json).is_none());
    }

    #[test]
    fn icu_json_with_locale_key() {
        let json = b"{\"__locale__\": \"fr_FR\", \"hello\": \"Bonjour\"}";
        let result = sniff_icu_json(json).unwrap();
        assert_eq!(result.locale.as_deref(), Some("fr_FR"));
    }

    #[test]
    fn lingui_shape_accepted() {
        let json = b"{\"hello\": {\"defaultMessage\": \"Hello\"}}";
        let result = sniff_icu_json(json);
        assert!(result.is_some());
    }

    // ── confirms_format ───────────────────────────────────────────────────────

    #[test]
    fn confirms_qt_ts() {
        use crate::manifest::CatalogFormat;
        assert!(confirms_format(b"<TS></TS>", CatalogFormat::QtTs));
        assert!(!confirms_format(b"{\"key\": \"val\"}", CatalogFormat::QtTs));
    }

    #[test]
    fn confirms_po() {
        use crate::manifest::CatalogFormat;
        let po = b"msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain\\n\"\n";
        assert!(confirms_format(po, CatalogFormat::GettextPo));
        assert!(!confirms_format(b"<TS></TS>", CatalogFormat::GettextPo));
    }

    #[test]
    fn confirms_icu_json() {
        use crate::manifest::CatalogFormat;
        assert!(confirms_format(b"{\"k\": \"v\"}", CatalogFormat::IcuJson));
        assert!(!confirms_format(b"<TS></TS>", CatalogFormat::IcuJson));
    }
}
