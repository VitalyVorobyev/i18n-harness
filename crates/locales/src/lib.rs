//! Locale records — languages as **data**, never as `match` arms.
//!
//! See `docs/internal/initial_design.md` §4 and `CLAUDE.md` invariant #1.
//!
//! # Storage choice
//!
//! Two layers of data, no per-language code:
//!
//! - `src/cldr.rs` — plural categories, likely script and English display
//!   name for every CLDR language, generated from pinned CLDR JSON by
//!   `cargo run -p xtask -- gen-locales` and checked in. Compile-time
//!   checked, no runtime parse.
//! - `data/defaults.toml` — the harness's own defaults (register, length
//!   ratio) by language and script, embedded and parsed once.
//!
//! A new language therefore needs no code at all; a project tunes it in its
//! manifest's `[locales.<id>]` table.
//!
//! # What this crate guarantees
//!
//! - [`Locale::by_id`] returns the same `&'static Locale` for the same id
//!   across the lifetime of the process.
//! - An id whose language CLDR does not know yields `None`, never a
//!   default — there is no "fallback locale" because there is no honest
//!   default for translation quality.
//!
//! # What this crate does NOT guarantee
//!
//! - That a locale's `cldr_plural` field matches the latest CLDR release —
//!   the table is pinned to [`CLDR_VERSION`] and regenerated deliberately,
//!   not silently.
//! - That `register` semantics are universally agreed on. "Formal" for
//!   German means *Sie*; for Spanish it means *usted*; we do not try to
//!   model this beyond the tag.

#![forbid(unsafe_code)]

use core::fmt;

/// CLDR plural category. The canonical order across the codebase is the one
/// CLDR itself uses: `zero, one, two, few, many, other`. Comparisons and
/// arity computations rely on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PluralCategory {
    /// Zero count form (Arabic, Welsh, …).
    Zero,
    /// Singular form (most Indo-European).
    One,
    /// Dual form (Arabic, Slovenian, …).
    Two,
    /// Small-count form (some Slavic).
    Few,
    /// Large-count form (some Slavic, Arabic).
    Many,
    /// Catch-all form. Present in every locale.
    Other,
}

impl fmt::Display for PluralCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Zero => "zero",
            Self::One => "one",
            Self::Two => "two",
            Self::Few => "few",
            Self::Many => "many",
            Self::Other => "other",
        };
        f.write_str(s)
    }
}

/// Register / formality. We do not exhaustively enumerate registers because
/// every language nuances them differently; we capture only the practical
/// distinction the maintainer needs at the moment of routing a prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Register {
    /// Formal address. German *Sie*, Spanish *usted*.
    Formal,
    /// Informal address. German *du*, Spanish *tú*.
    Informal,
    /// Locale does not distinguish — used for the source locale `en` and for
    /// languages where the choice is not a fixed dialectal pick.
    Neutral,
}

/// Script family. Drives whitespace/punctuation expectations (CJK uses
/// full-width punctuation and no inter-word spaces; RTL scripts need
/// directional handling at render time, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Script {
    /// Latin script (de, en, es, fr, …).
    Latin,
    /// Han / CJK ideographs (Simplified or Traditional Chinese).
    Han,
    /// Japanese (Han plus kana).
    Japanese,
    /// Korean (Hangul plus Han).
    Korean,
    /// Arabic script.
    Arabic,
    /// Cyrillic script.
    Cyrillic,
    /// Greek script.
    Greek,
    /// Hebrew script.
    Hebrew,
    /// Devanagari script.
    Devanagari,
    /// Thai script.
    Thai,
    /// Any other script.
    Other,
}

impl Script {
    /// Map an ISO 15924 code (`Latn`, `Hans`, …) to a script family.
    pub fn from_iso15924(code: &str) -> Self {
        match code {
            "Latn" => Self::Latin,
            "Hans" | "Hant" | "Hani" => Self::Han,
            "Jpan" => Self::Japanese,
            "Kore" | "Hang" => Self::Korean,
            "Arab" => Self::Arabic,
            "Cyrl" => Self::Cyrillic,
            "Grek" => Self::Greek,
            "Hebr" => Self::Hebrew,
            "Deva" => Self::Devanagari,
            "Thai" => Self::Thai,
            _ => Self::Other,
        }
    }
}

/// One locale record.
///
/// All fields are `'static`: a record is built once per id from the CLDR
/// table and the harness defaults, then reused for the life of the process.
#[derive(Debug, Clone, Copy)]
pub struct Locale {
    /// Locale identifier in the underscored form most i18n tooling uses
    /// (`de_DE`, `zh_Hans`, `en`), not the dash form (`de-DE`). Lookups accept
    /// either and normalize to this form.
    pub id: &'static str,

    /// Base language subtag (`sk` for `sk_SK`).
    pub language: &'static str,

    /// English display name of the language (`"Slovak"`), from CLDR.
    pub name: &'static str,

    /// CLDR plural categories applicable to **cardinal** numbers in this
    /// locale, in CLDR canonical order. Arity = `cldr_plural.len()`.
    ///
    /// This is the category set ICU MessageFormat uses. File formats with
    /// their own per-language form tables (Qt `<numerusform>`, gettext
    /// `Plural-Forms`) may use fewer forms; see [`Self::integer_plural_forms`].
    pub cldr_plural: &'static [PluralCategory],

    /// The categories that a small integer count can select, each with a
    /// representative count, in canonical order. For Slovak:
    /// `[(One, 1), (Few, 2), (Other, 0)]` — `many` applies only to fractions.
    ///
    /// Qt and gettext tables have one form per entry here, which makes this the
    /// fallback when a file carries no plural-form count of its own.
    pub integer_plural_forms: &'static [(PluralCategory, u32)],

    /// Register / formality.
    pub register: Register,

    /// Variant string. For most locales this duplicates `id`; for languages
    /// with script variants (`zh_Hans` vs `zh_Hant`) the catalog typically
    /// records the variant, not just the language.
    pub variant: &'static str,

    /// Script family.
    pub script: Script,

    /// Length-expansion threshold past which the gate's `length-warn` flag
    /// fires. A target longer than `source.len() * length_warn_ratio` is
    /// flagged for review. Source locale `en` uses `1.0` (no expansion
    /// expected since it is the reference).
    pub length_warn_ratio: f32,

    /// Originals shorter than this many characters are not length-checked:
    /// a short label has too little text for a ratio to mean anything.
    pub length_warn_min_chars: u32,
}

impl Locale {
    /// Plural arity for this locale (number of CLDR plural categories).
    pub fn plural_arity(&self) -> u32 {
        u32::try_from(self.cldr_plural.len()).unwrap_or(u32::MAX)
    }

    /// Number of plural forms a per-language plural table (Qt
    /// `<numerusform>`, gettext `msgstr[N]`) has for this locale, for files
    /// that do not state their own count. One per [`Self::integer_plural_forms`]
    /// entry.
    pub fn plural_slot_count(&self) -> u32 {
        u32::try_from(self.integer_plural_forms.len()).unwrap_or(u32::MAX)
    }

    /// Look up a locale by id (`sk_SK`, `sk-SK`, `zh_Hans`, `pt_BR`, `en`).
    ///
    /// Resolution: the CLDR entry for `language_REGION` when CLDR has
    /// region-specific rules (`pt_PT`), else the entry for the language. An
    /// explicit script subtag (`sr_Latn`) overrides the language's likely
    /// script. Returns `None` for ids whose language CLDR does not know.
    ///
    /// The same id always yields the same `&'static Locale`.
    pub fn by_id(id: &str) -> Option<&'static Locale> {
        registry::resolve(id)
    }

    /// Every language in the CLDR table, by its base id (`ar`, `de`, …,
    /// `pt_PT`, …), in id order.
    pub fn all() -> impl Iterator<Item = &'static Locale> {
        cldr::LANGUAGES
            .iter()
            .filter_map(|lang| Locale::by_id(lang.code))
    }
}

/// CLDR release the plural and name data comes from.
pub use cldr::CLDR_VERSION;

mod cldr;
mod registry;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn en_and_de_de_load_with_expected_arity_and_ratio() {
        let en = Locale::by_id("en").expect("en must be present");
        assert_eq!(en.plural_arity(), 2, "en CLDR arity");
        assert_eq!(
            en.cldr_plural,
            &[PluralCategory::One, PluralCategory::Other]
        );
        assert!(
            (en.length_warn_ratio - 1.0).abs() < f32::EPSILON,
            "en length_warn_ratio must be 1.0 (source locale, no expansion expected)",
        );
        assert_eq!(en.register, Register::Neutral);
        assert_eq!(en.script, Script::Latin);

        let de = Locale::by_id("de_DE").expect("de_DE must be present");
        assert_eq!(de.plural_arity(), 2, "de_DE CLDR arity");
        assert_eq!(
            de.cldr_plural,
            &[PluralCategory::One, PluralCategory::Other]
        );
        assert!(
            (de.length_warn_ratio - 1.4).abs() < f32::EPSILON,
            "de_DE length_warn_ratio must be 1.4 (German runs 30–40% longer)",
        );
        assert_eq!(de.register, Register::Formal);
        assert_eq!(de.script, Script::Latin);
    }

    #[test]
    fn es_es_loads_with_expected_arity_and_ratio() {
        let es = Locale::by_id("es_ES").expect("es_ES must be present");
        // CLDR 38+ gives Spanish a `many` category for large round numbers
        // ("1 millón de …"); gettext and Qt still use two forms.
        assert_eq!(
            es.plural_arity(),
            3,
            "es_ES CLDR arity is 3 (one, many, other)"
        );
        assert_eq!(
            es.cldr_plural,
            &[
                PluralCategory::One,
                PluralCategory::Many,
                PluralCategory::Other
            ]
        );
        assert_eq!(es.integer_plural_forms.len(), 2);
        assert!(
            (es.length_warn_ratio - 1.3).abs() < f32::EPSILON,
            "es_ES length_warn_ratio must be 1.3 (Romance ~25% longer)",
        );
        assert_eq!(es.register, Register::Formal, "default to usted");
        assert_eq!(es.script, Script::Latin);
    }

    #[test]
    fn zh_hans_loads_with_arity_1_and_han_script() {
        let zh = Locale::by_id("zh_Hans").expect("zh_Hans must be present");
        assert_eq!(zh.plural_arity(), 1, "Mandarin has no plural distinction");
        assert_eq!(zh.cldr_plural, &[PluralCategory::Other]);
        assert!(
            (zh.length_warn_ratio - 0.6).abs() < f32::EPSILON,
            "zh_Hans length_warn_ratio must be 0.6 (CJK typically shorter)",
        );
        assert_eq!(zh.register, Register::Neutral);
        assert_eq!(
            zh.script,
            Script::Han,
            "Script::Han activates the gate's CJK punctuation tolerance rule"
        );
    }

    #[test]
    fn unknown_locale_yields_none() {
        assert!(Locale::by_id("xx_XX").is_none());
        assert!(Locale::by_id("").is_none());
    }

    #[test]
    fn all_lists_every_cldr_language_once() {
        let ids: Vec<_> = Locale::all().map(|l| l.id).collect();
        assert!(
            ids.len() > 200,
            "CLDR has ~220 languages, got {}",
            ids.len()
        );
        assert!(ids.contains(&"sk") && ids.contains(&"de") && ids.contains(&"pt_PT"));
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "no duplicates");
    }

    #[test]
    fn slovak_has_four_cldr_categories_and_three_integer_forms() {
        let sk = Locale::by_id("sk_SK").expect("sk_SK resolves through the language");
        assert_eq!(sk.id, "sk_SK");
        assert_eq!(sk.language, "sk");
        assert_eq!(sk.name, "Slovak");
        assert_eq!(
            sk.cldr_plural,
            &[
                PluralCategory::One,
                PluralCategory::Few,
                PluralCategory::Many,
                PluralCategory::Other
            ]
        );
        let integer: Vec<_> = sk.integer_plural_forms.iter().map(|(c, _)| *c).collect();
        assert_eq!(
            integer,
            [
                PluralCategory::One,
                PluralCategory::Few,
                PluralCategory::Other
            ]
        );
        assert_eq!(sk.register, Register::Formal);
        assert_eq!(sk.script, Script::Latin);
    }

    #[test]
    fn dash_and_underscore_ids_resolve_to_the_same_record() {
        let a = Locale::by_id("sk-SK").unwrap();
        let b = Locale::by_id("sk_SK").unwrap();
        assert!(std::ptr::eq(a, b), "one record per canonical id");
    }

    #[test]
    fn region_specific_plural_rules_win_over_the_language() {
        // Brazilian Portuguese counts 0 as singular ("0 arquivo"); European
        // Portuguese does not. Same category names, different rules.
        let pt = Locale::by_id("pt").unwrap();
        let pt_pt = Locale::by_id("pt_PT").unwrap();
        assert_eq!(pt.integer_plural_forms[0], (PluralCategory::One, 0));
        assert_eq!(pt_pt.integer_plural_forms[0], (PluralCategory::One, 1));
        // A region without its own rules falls back to the language.
        let pt_br = Locale::by_id("pt_BR").unwrap();
        assert_eq!(pt_br.integer_plural_forms, pt.integer_plural_forms);
    }

    #[test]
    fn explicit_script_subtag_overrides_the_likely_script() {
        assert_eq!(Locale::by_id("sr").unwrap().script, Script::Cyrillic);
        assert_eq!(Locale::by_id("sr_Latn").unwrap().script, Script::Latin);
    }

    #[test]
    fn french_large_number_category_is_not_an_integer_form() {
        let fr = Locale::by_id("fr_FR").unwrap();
        assert!(fr.cldr_plural.contains(&PluralCategory::Many));
        assert_eq!(
            fr.integer_plural_forms.len(),
            2,
            "one / other, like gettext"
        );
    }
}
