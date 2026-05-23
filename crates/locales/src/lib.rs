//! Locale records — languages as **data**, never as `match` arms.
//!
//! See `docs/initial_design.md` §4 and `CLAUDE.md` invariant #1.
//!
//! # Storage choice
//!
//! Locale records are stored as a `static` Rust slice with `&'static str`
//! fields, *not* as an embedded YAML/TOML asset. Rationale:
//!
//! - **Compile-time checked.** Typos in field names, wrong enum variants,
//!   missing fields are caught by `rustc`, not at first run.
//! - **No runtime parse cost.** Looking up a locale is an array scan; no
//!   `serde_yaml::from_str` overhead on every CLI invocation.
//! - **No new dependency.** The crate stays free of `serde_yaml` / `serde`,
//!   keeping the dep graph minimal.
//! - **The dataset is small.** Two rows now, perhaps a dozen by M4. The
//!   ergonomic argument for YAML kicks in around tens of rows with many
//!   reviewers; we are far from that. If/when we add the full CLDR locale
//!   list we will revisit this — the `Locale::by_id` API is the abstraction
//!   that lets the implementation change without breaking callers.
//!
//! # What this crate guarantees
//!
//! - [`Locale::by_id`] returns the same `&'static Locale` for the same id
//!   across the lifetime of the process.
//! - The set of locales is closed: a missing id yields `None`, never a
//!   default — there is no "fallback locale" because there is no honest
//!   default for translation quality.
//!
//! # What this crate does NOT guarantee
//!
//! - That a locale's `cldr_plural` field matches the latest CLDR release —
//!   we pin to a CLDR version when we add a locale and update deliberately,
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
    /// Han / CJK ideographs.
    Han,
    /// Arabic script.
    Arabic,
    /// Cyrillic script.
    Cyrillic,
}

/// One locale record.
///
/// All fields are `'static` so the whole table can live in `.rodata`.
#[derive(Debug, Clone, Copy)]
pub struct Locale {
    /// Locale identifier. We use the underscored BCP-47-ish form most i18n
    /// tooling uses (`de_DE`, `zh_Hans`, `en`), not the dash form
    /// (`de-DE`). Adapters that read the dash form must translate at the
    /// boundary.
    pub id: &'static str,

    /// CLDR plural categories applicable to **cardinal** numbers in this
    /// locale, in CLDR canonical order. Arity = `cldr_plural.len()`.
    pub cldr_plural: &'static [PluralCategory],

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
}

impl Locale {
    /// Plural arity for this locale (number of CLDR plural categories).
    pub fn plural_arity(&self) -> u32 {
        // Safe cast: arity is small (max 6 in CLDR).
        self.cldr_plural.len() as u32
    }

    /// Look up a locale by its `id` string. Returns `None` for unknown ids
    /// — there is no default-locale fallback.
    pub fn by_id(id: &str) -> Option<&'static Locale> {
        LOCALES.iter().find(|l| l.id == id)
    }

    /// Iterate every known locale in the order they appear in the table.
    pub fn all() -> impl Iterator<Item = &'static Locale> {
        LOCALES.iter()
    }
}

/// The locale table.
///
/// Rationale per row:
/// - `en` — source locale. `length_warn_ratio = 1.0` because expansion is
///   measured *from* English, not into it. `Register::Neutral` because the
///   source side does not have a single formality pick.
/// - `de_DE` — first target locale (M0–M3). German runs 30–40 % longer than
///   English in UI strings; `1.4` matches the reference table in
///   `.claude/skills/add-locale/SKILL.md`. CLDR cardinal arity is 2
///   (`[one, other]`). The maintainer ships `Sie`/formal as default.
/// - `es_ES` — Romance target. CLDR cardinal arity 2 (`[one, other]`). The
///   maintainer ships `usted`/formal as default; informal `tú` projects
///   override via the glossary's `[locale.es_ES]` block.
/// - `zh_Hans` — Simplified Chinese. CLDR cardinal arity 1 (`[other]` only)
///   — no singular/plural distinction, no agreement to worry about. Han
///   script activates the gate's `CjkPunctuationTolerated` soft rule.
///   `register = Neutral`: Mandarin's formal/informal distinction lives in
///   word choice (e.g., `您` vs `你`) rather than a fixed dialectal pick,
///   so we don't encode it at the locale level; projects with strong
///   register requirements should bake that into the glossary.
static LOCALES: &[Locale] = &[
    Locale {
        id: "en",
        cldr_plural: &[PluralCategory::One, PluralCategory::Other],
        register: Register::Neutral,
        variant: "en",
        script: Script::Latin,
        length_warn_ratio: 1.0,
    },
    Locale {
        id: "de_DE",
        cldr_plural: &[PluralCategory::One, PluralCategory::Other],
        register: Register::Formal,
        variant: "de_DE",
        script: Script::Latin,
        length_warn_ratio: 1.4,
    },
    Locale {
        id: "es_ES",
        cldr_plural: &[PluralCategory::One, PluralCategory::Other],
        register: Register::Formal,
        variant: "es_ES",
        script: Script::Latin,
        length_warn_ratio: 1.3,
    },
    Locale {
        id: "zh_Hans",
        cldr_plural: &[PluralCategory::Other],
        register: Register::Neutral,
        variant: "zh_Hans",
        script: Script::Han,
        length_warn_ratio: 0.6,
    },
];

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
        assert_eq!(es.plural_arity(), 2, "es_ES CLDR arity is 2 (one, other)");
        assert_eq!(
            es.cldr_plural,
            &[PluralCategory::One, PluralCategory::Other]
        );
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
    fn all_iterates_in_table_order() {
        let ids: Vec<_> = Locale::all().map(|l| l.id).collect();
        assert_eq!(ids, vec!["en", "de_DE", "es_ES", "zh_Hans"]);
    }
}
