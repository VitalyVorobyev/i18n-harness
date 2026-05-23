//! Property test: `Glossary::from_toml(g.to_toml())` is the identity.
//!
//! We generate small but representative glossaries and assert that
//! serializing then re-parsing returns an equal value. The corpus is bounded
//! deliberately small — `toml`'s round-trip is well-tested, what we are
//! testing here is that *our* schema-bridge code in `schema::serialize` and
//! `schema::validate` agrees on every field.

use std::collections::BTreeMap;

use i18n_harness_glossary::{Glossary, Register};
use proptest::prelude::*;

// Glossaries are built up by hand here (rather than via `proptest_derive`)
// because the `Glossary` type does not expose its constructors publicly —
// the on-disk shape is the only authoring path. We generate a TOML string,
// parse it, and assert round-trip on the parsed value.

fn ident_strategy() -> impl Strategy<Value = String> {
    // ASCII identifier-ish: source strings can be anything in practice, but
    // for round-trip we want characters that survive TOML escaping cleanly.
    proptest::string::string_regex("[A-Za-z][A-Za-z0-9_]{0,15}").unwrap()
}

fn translation_strategy() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[A-Za-zäöüÄÖÜß ]{1,20}").unwrap()
}

fn locale_id_strategy() -> impl Strategy<Value = String> {
    // Pick from the locales currently in the workspace table, plus a couple
    // of plausible "future" locales that exercise the unknown-locale warning
    // path.
    prop_oneof![
        Just("en".to_owned()),
        Just("de_DE".to_owned()),
        Just("es_ES".to_owned()),
        Just("zh_Hans".to_owned()),
    ]
}

fn register_strategy() -> impl Strategy<Value = Register> {
    prop_oneof![
        Just(Register::Formal),
        Just(Register::Informal),
        Just(Register::Neutral),
    ]
}

#[derive(Debug, Clone)]
struct TermSpec {
    source: String,
    do_not_translate: bool,
    notes: Option<String>,
    translations: BTreeMap<String, String>,
}

fn term_spec_strategy() -> impl Strategy<Value = TermSpec> {
    (
        ident_strategy(),
        any::<bool>(),
        proptest::option::of(translation_strategy()),
        prop::collection::btree_map(locale_id_strategy(), translation_strategy(), 0..3),
    )
        .prop_map(
            |(source, do_not_translate, notes, translations)| TermSpec {
                source,
                do_not_translate,
                notes,
                translations,
            },
        )
}

#[derive(Debug, Clone)]
struct GlossarySpec {
    terms: Vec<TermSpec>,
    locales: BTreeMap<String, (Option<Register>, Option<String>)>,
}

fn glossary_spec_strategy() -> impl Strategy<Value = GlossarySpec> {
    (
        prop::collection::vec(term_spec_strategy(), 0..6),
        prop::collection::btree_map(
            locale_id_strategy(),
            (
                proptest::option::of(register_strategy()),
                proptest::option::of(ident_strategy()),
            ),
            0..3,
        ),
    )
        .prop_map(|(terms, locales)| GlossarySpec { terms, locales })
        // Drop duplicate sources — proptest would generate them and the
        // loader correctly rejects them; here we are testing round-trip,
        // not duplicate handling.
        .prop_filter("unique sources", |spec: &GlossarySpec| {
            let mut seen = std::collections::HashSet::new();
            spec.terms.iter().all(|t| seen.insert(t.source.clone()))
        })
}

fn render_spec_to_toml(spec: &GlossarySpec) -> String {
    let mut out = String::from("[meta]\nschema_version = 1\n");
    for t in &spec.terms {
        out.push_str("\n[[term]]\n");
        out.push_str(&format!("source = {:?}\n", t.source));
        out.push_str(&format!("do_not_translate = {}\n", t.do_not_translate));
        if let Some(notes) = &t.notes {
            out.push_str(&format!("notes = {notes:?}\n"));
        }
        if !t.translations.is_empty() {
            out.push_str("[term.translations]\n");
            for (k, v) in &t.translations {
                out.push_str(&format!("{k} = {v:?}\n"));
            }
        }
    }
    for (locale, (register, variant)) in &spec.locales {
        out.push_str(&format!("\n[locale.{locale}]\n"));
        if let Some(r) = register {
            out.push_str(&format!("register = {:?}\n", r.as_str()));
        }
        if let Some(v) = variant {
            out.push_str(&format!("variant = {v:?}\n"));
        }
    }
    out
}

proptest! {
    /// Authored TOML → in-memory → serialized TOML → in-memory must be
    /// identity in the in-memory shape. The two TOML strings need not be
    /// byte-identical (serializer normalizes ordering / whitespace) — only
    /// the in-memory values are compared.
    #[test]
    fn round_trip_is_identity(spec in glossary_spec_strategy()) {
        let input = render_spec_to_toml(&spec);
        let (g1, _) = Glossary::from_toml(&input).expect("from_toml first");
        let serialized = g1.to_toml().expect("to_toml");
        let (g2, _) = Glossary::from_toml(&serialized).expect("from_toml second");
        prop_assert_eq!(g1, g2);
    }
}
