//! Content sniffer for ICU MessageFormat JSON catalogs.

use std::cell::RefCell;
use std::fmt;

use serde::de::{Deserializer as _, MapAccess, Visitor};
use serde_json::Value;

use crate::format::{Confidence, SniffHit, strip_bom, utf8_prefix};

/// Top-level key pairs that mark a tool-config file rather than a catalog.
const TOOL_CONFIG_SIGNATURES: &[(&str, &str)] = &[
    ("name", "version"),
    ("compilerOptions", ""),
    ("devDependencies", ""),
    ("dependencies", "scripts"),
];

/// Schema URL fragments of non-i18n schemas.
const NON_I18N_SCHEMA_FRAGMENTS: &[&str] = &["json-schema.org", "schemastore.org"];

/// Recognize an ICU-JSON catalog from its first bytes.
///
/// Rejects `package.json`, `tsconfig.json`, and files with a non-i18n
/// `$schema`. Accepts objects with string values (or `defaultMessage`-style
/// objects). Always `Medium`: a JSON file cannot be proven to be a catalog
/// without knowing the library that consumes it.
///
/// The head may be a truncated prefix of a large file, so the top-level
/// entries are read one at a time and reading stops at the first entry that
/// does not parse.
pub(crate) fn sniff(head: &[u8]) -> Option<SniffHit> {
    let text = utf8_prefix(strip_bom(head));
    let entries = top_level_entries(text)?;
    let has = |key: &str| entries.iter().any(|(k, _)| k == key);
    let get = |key: &str| entries.iter().find(|(k, _)| k == key).map(|(_, v)| v);

    for (key_a, key_b) in TOOL_CONFIG_SIGNATURES {
        if has(key_a) && (key_b.is_empty() || has(key_b)) {
            return None;
        }
    }

    if let Some(schema) = get("$schema").and_then(Value::as_str) {
        let is_i18n_schema = schema.contains("i18n") || schema.contains("messageformat");
        let is_tool_schema = NON_I18N_SCHEMA_FRAGMENTS
            .iter()
            .any(|frag| schema.contains(frag));
        if is_tool_schema && !is_i18n_schema {
            return None;
        }
    }

    let language = get("__locale__")
        .and_then(Value::as_str)
        .map(|s| s.replace('-', "_"));

    let has_string_values = entries.iter().any(|(_, v)| {
        v.is_string()
            || v.as_object()
                .is_some_and(|o| o.contains_key("defaultMessage") || o.contains_key("string"))
    });
    if !has_string_values {
        return None;
    }

    let reason = match &language {
        Some(lang) => {
            format!("flat JSON map with __locale__ key — ICU-JSON catalog (locale: {lang})")
        }
        None => "flat JSON key→string map — likely ICU-JSON catalog".to_owned(),
    };
    Some(SniffHit {
        confidence: Confidence::Medium,
        language,
        reason,
    })
}

/// The complete top-level `(key, value)` entries at the start of a JSON
/// object. `None` when `text` does not start with an object.
fn top_level_entries(text: &str) -> Option<Vec<(String, Value)>> {
    if !text.trim_start().starts_with('{') {
        return None;
    }
    let entries = RefCell::new(Vec::new());
    let mut de = serde_json::Deserializer::from_str(text);
    // A truncated head ends with an error; the entries read before it stand.
    let _ = de.deserialize_map(EntryCollector(&entries));
    Some(entries.into_inner())
}

struct EntryCollector<'a>(&'a RefCell<Vec<(String, Value)>>);

impl<'de> Visitor<'de> for EntryCollector<'_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some((key, value)) = map.next_entry::<String, Value>()? {
            self.0.borrow_mut().push((key, value));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_map_is_accepted() {
        let hit = sniff(br#"{"greeting": "Hello {name}"}"#).expect("catalog");
        assert_eq!(hit.confidence, Confidence::Medium);
    }

    #[test]
    fn locale_key_is_reported() {
        let hit = sniff(br#"{"__locale__": "de-DE", "a": "b"}"#).expect("catalog");
        assert_eq!(hit.language.as_deref(), Some("de_DE"));
    }

    #[test]
    fn package_json_is_rejected() {
        assert!(sniff(br#"{"name": "app", "version": "1.0.0"}"#).is_none());
    }

    #[test]
    fn tsconfig_is_rejected() {
        assert!(sniff(br#"{"compilerOptions": {"strict": true}}"#).is_none());
    }

    #[test]
    fn non_object_is_rejected() {
        assert!(sniff(b"[1, 2, 3]").is_none());
        assert!(sniff(b"not json").is_none());
    }

    #[test]
    fn truncated_head_of_a_large_catalog_is_accepted() {
        let mut text = String::from("{");
        for i in 0..5_000 {
            text.push_str(&format!("\"key.{i}\": \"Message number {i}\","));
        }
        text.push_str("\"last\": \"end\"}");
        let head = &text.as_bytes()[..64 * 1024];
        assert!(sniff(head).is_some(), "a 64 KiB prefix must still sniff");
    }
}
