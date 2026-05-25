//! Serialisation of the wire-format [`GlossaryPayload`] to the on-disk TOML
//! schema.
//!
//! `payload_to_toml` is the only public surface. The `Wire*` helper structs
//! are private to this module — they exist only to drive serde's `toml`
//! serializer and must not leak into the IPC contract (that lives in
//! `dto::glossary`).

use std::collections::BTreeMap;

use serde::Serialize;

use crate::dto::glossary::GlossaryPayload;

// ── Wire types (private) ─────────────────────────────────────────────────────

#[derive(Serialize)]
struct WireMeta {
    schema_version: u32,
}

#[derive(Serialize)]
struct WireTerm<'a> {
    source: &'a str,
    do_not_translate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<&'a String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    translations: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct WireLocale<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    register: Option<&'a String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    variant: Option<&'a String>,
}

#[derive(Serialize)]
struct Wire<'a> {
    meta: WireMeta,
    #[serde(rename = "term", skip_serializing_if = "Vec::is_empty")]
    terms: Vec<WireTerm<'a>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    locale: BTreeMap<String, WireLocale<'a>>,
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Serialise a [`GlossaryPayload`] to the on-disk TOML schema.
///
/// Kept in this crate so we can drive it from a wire payload without running
/// the validator twice (once on the payload, once on the produced TOML). Field
/// names match `crates/glossary/src/schema.rs`'s `Raw*` shapes.
///
/// # Errors
///
/// Returns an `Err` string when:
/// - A term source appears more than once in `payload.terms`.
/// - A locale id appears more than once in `payload.locale_overrides`.
/// - The TOML serializer itself fails (extremely unlikely for well-formed data).
pub(crate) fn payload_to_toml(payload: &GlossaryPayload) -> Result<String, String> {
    // Locale overrides are keyed by id on disk, so duplicates would
    // silently collapse into the last writer if we let BTreeMap::collect
    // do its thing. Detect them up-front and refuse to write — the UI
    // surfaces the error to the user. Term duplicates are caught by
    // `Glossary::from_toml`'s `DuplicateSource` check downstream, but
    // catching them here too produces a sharper message.
    let mut seen_terms = std::collections::HashSet::new();
    for t in &payload.terms {
        if !seen_terms.insert(&t.source) {
            return Err(format!(
                "duplicate term source `{}` — every source must be unique",
                t.source,
            ));
        }
    }
    let mut locale: BTreeMap<String, WireLocale> = BTreeMap::new();
    for o in &payload.locale_overrides {
        if locale.contains_key(&o.locale) {
            return Err(format!(
                "duplicate locale override for `{}` — each locale appears at most once",
                o.locale,
            ));
        }
        locale.insert(
            o.locale.clone(),
            WireLocale {
                register: o.register.as_ref(),
                variant: o.variant.as_ref(),
            },
        );
    }
    let terms = payload
        .terms
        .iter()
        .map(|t| WireTerm {
            source: &t.source,
            do_not_translate: t.do_not_translate,
            notes: t.notes.as_ref(),
            translations: t.translations.clone(),
        })
        .collect();
    let wire = Wire {
        meta: WireMeta {
            schema_version: payload.schema_version,
        },
        terms,
        locale,
    };
    toml::to_string(&wire).map_err(|e| format!("toml serialize: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::glossary::{LocaleOverrideEntry, TermEntry};

    fn minimal_payload() -> GlossaryPayload {
        GlossaryPayload {
            schema_version: 1,
            terms: vec![TermEntry {
                source: "Hello".to_owned(),
                do_not_translate: false,
                notes: None,
                translations: {
                    let mut m = BTreeMap::new();
                    m.insert("de".to_owned(), "Hallo".to_owned());
                    m
                },
            }],
            locale_overrides: vec![],
        }
    }

    #[test]
    fn payload_round_trips_via_glossary_from_toml() {
        let payload = minimal_payload();
        let toml_str = payload_to_toml(&payload).expect("should serialize");
        // Round-trip through the validator must succeed (UnknownLocale warnings
        // are expected in tests because the locales table is not populated here).
        let (glossary, _warnings) =
            i18n_harness_glossary::Glossary::from_toml(&toml_str).expect("should parse back");
        // The term must have survived the round-trip.
        let term = glossary.term("Hello").expect("term must be present");
        assert_eq!(
            term.translations.get("de").map(String::as_str),
            Some("Hallo")
        );
    }

    #[test]
    fn payload_with_duplicate_term_source_returns_error() {
        let mut payload = minimal_payload();
        // Append a second entry with the same source.
        payload.terms.push(TermEntry {
            source: "Hello".to_owned(),
            do_not_translate: false,
            notes: None,
            translations: BTreeMap::new(),
        });
        let result = payload_to_toml(&payload);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("duplicate term source"),
            "expected 'duplicate term source' in: {msg}"
        );
    }

    #[test]
    fn payload_with_duplicate_locale_override_returns_error() {
        let mut payload = minimal_payload();
        payload.locale_overrides.push(LocaleOverrideEntry {
            locale: "de".to_owned(),
            register: None,
            variant: None,
        });
        payload.locale_overrides.push(LocaleOverrideEntry {
            locale: "de".to_owned(),
            register: Some("formal".to_owned()),
            variant: None,
        });
        let result = payload_to_toml(&payload);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("duplicate locale override"),
            "expected 'duplicate locale override' in: {msg}"
        );
    }
}
