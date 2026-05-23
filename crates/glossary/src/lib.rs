//! Per-project [`Glossary`] — TOML schema, loader, and validator.
//!
//! See `docs/initial_design.md` §10. A glossary is checked into the project
//! root as `glossary.toml`; it captures three things the translation backend
//! needs at prompt time:
//!
//! 1. **Terms** — per-locale preferred translations of recurring words
//!    ("Open" → "Öffnen"), plus a `do_not_translate` flag for product names
//!    that should be rendered verbatim.
//! 2. **Locale overrides** — register (`formal`/`informal`/`neutral`) and
//!    variant tag overriding the workspace's locale defaults. These let one
//!    project ship "informal Spanish" while the locales table says
//!    "formal" — without forking the table.
//! 3. **Schema versioning** — every glossary file declares
//!    `meta.schema_version`; the loader refuses to read versions it does not
//!    understand rather than silently misinterpret fields.
//!
//! # What this crate guarantees
//!
//! - [`Glossary::load`] either returns a fully-validated value or a
//!   [`GlossaryError`] naming the first failure. Partial loads are not a
//!   thing.
//! - The serialized → loaded → serialized → loaded chain produces the same
//!   in-memory value (round-trip property tested).
//! - Lookups by `source` are case-sensitive (matches Qt and gettext conventions
//!   that treat the source string as the natural key).
//!
//! # What this crate does NOT guarantee
//!
//! - That a `[locale.*]` entry references a locale present in
//!   `crates/locales`. We **warn** but do not reject — the glossary may
//!   contain locales not yet in the workspace's table, and we do not want a
//!   well-formed glossary to fail loading when the maintainer is ahead of
//!   locale registration. (The CLI surfaces the warning when invoked.)
//! - That a translation is "correct" — only the model knows that. The gate
//!   checks structural fidelity; the glossary is a hint.
//!
//! # Stability
//!
//! [`Glossary`] is the type the rest of the workspace took a dependency on in
//! M1 (the gate's `Option<&Glossary>` signature). The shape grows fields
//! between M1 and M2 but never loses its name; downstream signatures stay
//! valid across the upgrade.

#![forbid(unsafe_code)]

mod error;
mod schema;

pub use error::{GlossaryError, GlossaryWarning};
pub use schema::{Register, SCHEMA_VERSION, Term};

use std::collections::BTreeMap;
use std::path::Path;

/// Per-project glossary — terms, do-not-translate list, and per-locale
/// register/variant overrides.
///
/// # Invariants
///
/// - Sources are case-sensitive and unique within the glossary. `load`
///   rejects duplicates so a downstream lookup cannot be ambiguous.
/// - Every term either has at least one translation (when
///   `do_not_translate == false`) or is flagged
///   `do_not_translate = true`. The loader treats an empty translation
///   table with `do_not_translate = false` as a *warning*, not an error
///   (the maintainer may be staging entries to be filled).
/// - Locale ids in `[locale.*]` are the underscored form (`de_DE`), to
///   match [`i18n_harness_locales::Locale::id`].
///
/// # What `Glossary` does NOT guarantee
///
/// - That every locale referenced by a term's translation table exists in
///   [`i18n_harness_locales::Locale`]. The loader emits a warning when one
///   does not; the caller decides whether to surface or suppress it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Glossary {
    schema_version: u32,
    /// Insertion-ordered map keyed by source. `BTreeMap` keeps order
    /// deterministic (alphabetical by source), which makes the glossary
    /// stable in prompts and in test snapshots.
    terms: BTreeMap<String, Term>,
    /// Per-locale overrides (register, variant). Keyed by locale id.
    locale_overrides: BTreeMap<String, LocaleOverride>,
}

/// Per-locale override for register / variant tag.
///
/// Both fields are optional: an entry that sets only `register` leaves the
/// `variant` tag untouched, and vice versa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleOverride {
    /// Register override (e.g. force `Informal` Spanish when the locales
    /// table defaults to `Formal`).
    pub register: Option<Register>,
    /// Variant tag override (rarely needed; most projects accept the locales
    /// table's choice).
    pub variant: Option<String>,
}

impl Glossary {
    /// Construct an empty glossary at the current [`SCHEMA_VERSION`].
    ///
    /// Used by callers (gate, backend) that want to exercise the
    /// `Some(&Glossary)` branch of an API without loading from disk.
    pub fn empty() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            terms: BTreeMap::new(),
            locale_overrides: BTreeMap::new(),
        }
    }

    /// Declared schema version of this glossary.
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Number of terms.
    pub fn len(&self) -> usize {
        self.terms.len()
    }

    /// `true` if no terms have been registered.
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// Read and validate a glossary from a TOML file.
    ///
    /// On success, returns the loaded glossary plus a (possibly empty) list
    /// of non-fatal warnings. The caller decides what to do with warnings
    /// (typically: print to stderr; do not block).
    ///
    /// # Errors
    ///
    /// - [`GlossaryError::Io`] if the file cannot be read.
    /// - [`GlossaryError::Toml`] if the file is not valid TOML.
    /// - [`GlossaryError::UnsupportedSchemaVersion`] if `meta.schema_version`
    ///   exceeds [`SCHEMA_VERSION`].
    /// - [`GlossaryError::DuplicateSource`] if two `[[term]]` entries share
    ///   a source string.
    /// - [`GlossaryError::InvalidRegister`] if a register string outside
    ///   `formal|informal|neutral` is used.
    /// - [`GlossaryError::MissingSchemaVersion`] if `meta.schema_version`
    ///   is missing.
    pub fn load(path: impl AsRef<Path>) -> Result<(Self, Vec<GlossaryWarning>), GlossaryError> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path).map_err(|source| GlossaryError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_toml(&contents)
    }

    /// Parse and validate a glossary from a TOML string.
    ///
    /// Useful for tests and for callers that already have the bytes in
    /// memory. Same error contract as [`Self::load`] minus the I/O variant.
    pub fn from_toml(input: &str) -> Result<(Self, Vec<GlossaryWarning>), GlossaryError> {
        let raw: schema::Raw = toml::from_str(input).map_err(GlossaryError::Toml)?;
        schema::validate(raw)
    }

    /// Serialize the glossary back to a TOML string.
    ///
    /// The output is deterministic: terms appear in alphabetical order by
    /// source; locale overrides in alphabetical order by id. Round-trip
    /// property: `Glossary::from_toml(g.to_toml())` returns an equal value.
    ///
    /// # Errors
    ///
    /// Returns [`GlossaryError::Toml`] only if `toml::to_string` fails — in
    /// practice this means a non-stringifiable map key, which our schema
    /// does not allow. Surfaced as an error so future shape changes do not
    /// silently corrupt files.
    pub fn to_toml(&self) -> Result<String, GlossaryError> {
        schema::serialize(self).map_err(|e| GlossaryError::Serialize(e.to_string()))
    }

    /// Look up a term by its source. Returns `None` if not present.
    pub fn term(&self, source: &str) -> Option<&Term> {
        self.terms.get(source)
    }

    /// Iterate every term in alphabetical order by source.
    ///
    /// Stable order matters: the prompt template embeds glossary lines in
    /// this order, and a deterministic order keeps prompts byte-stable
    /// across runs (so metrics stay comparable).
    pub fn terms(&self) -> impl Iterator<Item = (&str, &Term)> {
        self.terms.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Iterate `(source, target)` pairs for a given locale id. Skips:
    ///
    /// - Terms with no translation for `locale_id`.
    /// - Terms flagged `do_not_translate`: those are emitted separately by
    ///   [`Self::do_not_translate`] and rendered as a distinct prompt block.
    pub fn terms_for(&self, locale_id: &str) -> impl Iterator<Item = (&str, &str)> {
        self.terms.iter().filter_map(move |(src, term)| {
            if term.do_not_translate {
                return None;
            }
            term.translations
                .get(locale_id)
                .map(|tgt| (src.as_str(), tgt.as_str()))
        })
    }

    /// Iterate sources flagged `do_not_translate` — render-verbatim
    /// product names, brand tokens, technical identifiers.
    pub fn do_not_translate(&self) -> impl Iterator<Item = &str> {
        self.terms.iter().filter_map(|(src, term)| {
            if term.do_not_translate {
                Some(src.as_str())
            } else {
                None
            }
        })
    }

    /// Project-level register override for the given locale, if any.
    ///
    /// Backends consult this before consulting the locales table; an
    /// explicit project choice wins over the workspace default.
    pub fn register_for(&self, locale_id: &str) -> Option<Register> {
        self.locale_overrides.get(locale_id)?.register
    }

    /// Project-level variant override for the given locale, if any.
    pub fn variant_for(&self, locale_id: &str) -> Option<&str> {
        self.locale_overrides.get(locale_id)?.variant.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_glossary_is_at_current_schema() {
        let g = Glossary::empty();
        assert_eq!(g.schema_version(), SCHEMA_VERSION);
        assert!(g.is_empty());
        assert_eq!(g.len(), 0);
    }

    #[test]
    fn round_trip_through_toml_is_identity() {
        let toml = r#"
[meta]
schema_version = 1

[[term]]
source = "Open"
do_not_translate = false

[term.translations]
de_DE = "Öffnen"

[[term]]
source = "ChromaCheck"
do_not_translate = true

[locale.de_DE]
register = "formal"
variant = "de_DE"
"#;
        let (g1, warnings) = Glossary::from_toml(toml).expect("parse");
        assert!(
            warnings.is_empty(),
            "expected no warnings, got {warnings:?}"
        );
        let serialized = g1.to_toml().expect("serialize");
        let (g2, _) = Glossary::from_toml(&serialized).expect("re-parse");
        assert_eq!(g1, g2, "round trip identity (serialize then parse)");
    }

    #[test]
    fn terms_for_skips_dnt_and_missing_locale() {
        let toml = r#"
[meta]
schema_version = 1

[[term]]
source = "Open"
do_not_translate = false
[term.translations]
de_DE = "Öffnen"

[[term]]
source = "ChromaCheck"
do_not_translate = true

[[term]]
source = "Save"
do_not_translate = false
[term.translations]
es_ES = "Guardar"
"#;
        let (g, _) = Glossary::from_toml(toml).expect("parse");
        let de: Vec<_> = g.terms_for("de_DE").collect();
        assert_eq!(de, vec![("Open", "Öffnen")]);
        let dnt: Vec<_> = g.do_not_translate().collect();
        assert_eq!(dnt, vec!["ChromaCheck"]);
    }

    #[test]
    fn register_override_returns_per_locale_value() {
        let toml = r#"
[meta]
schema_version = 1

[locale.de_DE]
register = "informal"

[locale.es_ES]
variant = "es_419"
"#;
        let (g, _) = Glossary::from_toml(toml).expect("parse");
        assert_eq!(g.register_for("de_DE"), Some(Register::Informal));
        assert_eq!(g.register_for("es_ES"), None);
        assert_eq!(g.variant_for("es_ES"), Some("es_419"));
        assert_eq!(g.variant_for("fr_FR"), None);
    }
}
