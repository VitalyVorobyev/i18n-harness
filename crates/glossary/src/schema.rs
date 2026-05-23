//! On-disk TOML schema for [`crate::Glossary`].
//!
//! Kept separate from `lib.rs` so the public type surface is the in-memory
//! [`crate::Glossary`], not the deserialized [`Raw`] shape. The two diverge
//! deliberately: TOML is a tree of named tables (`[[term]]`, `[locale.de_DE]`),
//! while the in-memory shape is two flat `BTreeMap`s. Validating the bridge
//! lets us catch duplicate sources, invalid register strings, and unknown
//! locales in one place.

use std::collections::BTreeMap;

use i18n_harness_locales::Locale as WorkspaceLocale;
use serde::{Deserialize, Serialize};

use crate::error::{GlossaryError, GlossaryWarning};
use crate::{Glossary, LocaleOverride};

/// Highest `meta.schema_version` this binary knows how to read. Bump on
/// every breaking change to the on-disk shape; never decrease.
pub const SCHEMA_VERSION: u32 = 1;

/// Register / formality tag used in `[locale.<id>]` and surfaced to backends
/// at prompt time.
///
/// Mirrors [`i18n_harness_locales::Register`] but is its own type because
/// the workspace's `locales` crate (deliberately) does not depend on
/// `serde`. We use the on-disk lower-case form (`formal`, `informal`,
/// `neutral`) which is what a human would type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Register {
    /// Formal address. German *Sie*, Spanish *usted*.
    Formal,
    /// Informal address. German *du*, Spanish *tú*.
    Informal,
    /// Locale does not distinguish.
    Neutral,
}

impl Register {
    /// Render the kebab-case literal a maintainer would type
    /// (`"formal"`, `"informal"`, `"neutral"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Formal => "formal",
            Self::Informal => "informal",
            Self::Neutral => "neutral",
        }
    }

    /// Bridge to the workspace locales crate's `Register` (which is
    /// deliberately serde-free). Both shapes share the same variants in the
    /// same order; this is a one-to-one mapping.
    pub fn to_locales_register(self) -> i18n_harness_locales::Register {
        match self {
            Self::Formal => i18n_harness_locales::Register::Formal,
            Self::Informal => i18n_harness_locales::Register::Informal,
            Self::Neutral => i18n_harness_locales::Register::Neutral,
        }
    }
}

/// One glossary term as it appears in [`crate::Glossary`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    /// `true` if the term should never be translated — product names, brand
    /// tokens, technical identifiers.
    pub do_not_translate: bool,
    /// Free-form note shown next to the term in the UI. Useful for sense
    /// disambiguation ("verb sense; not the adjective").
    pub notes: Option<String>,
    /// Translations keyed by locale id (`de_DE`, `es_ES`, …). Empty for
    /// terms with `do_not_translate = true`; a non-empty `do_not_translate`
    /// row is permitted (the translations are then a fallback if the flag
    /// is later flipped) but unusual.
    pub translations: BTreeMap<String, String>,
}

// ── On-disk shape ─────────────────────────────────────────────────────────
//
// `Raw*` types mirror the TOML layout. We do **not** make these public:
// callers see the in-memory `Glossary`, and bridging happens in
// `validate` / `serialize`.

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Raw {
    /// `[meta]` table. Required.
    meta: Option<RawMeta>,
    /// `[[term]]` array. May be absent for a header-only glossary.
    #[serde(default)]
    term: Vec<RawTerm>,
    /// `[locale.<id>]` tables. Key is the locale id.
    #[serde(default)]
    locale: BTreeMap<String, RawLocale>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RawMeta {
    /// Schema version. Required.
    schema_version: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct RawTerm {
    /// Source string (case-sensitive natural key).
    source: String,
    /// `do_not_translate` flag (defaults to `false` if absent).
    #[serde(default)]
    do_not_translate: bool,
    /// Optional free-form notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
    /// `[term.translations]` sub-table: locale id → target text.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    translations: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RawLocale {
    /// Register override; raw string so we can produce a precise error.
    #[serde(skip_serializing_if = "Option::is_none")]
    register: Option<String>,
    /// Variant tag override.
    #[serde(skip_serializing_if = "Option::is_none")]
    variant: Option<String>,
}

/// Validate a parsed [`Raw`] and produce the in-memory [`Glossary`].
///
/// Single pass; allocates one `BTreeMap` per output map. Diagnostics are
/// emitted as errors (hard) or warnings (advisory).
pub(crate) fn validate(raw: Raw) -> Result<(Glossary, Vec<GlossaryWarning>), GlossaryError> {
    let meta = raw.meta.ok_or(GlossaryError::MissingSchemaVersion)?;
    if meta.schema_version > SCHEMA_VERSION {
        return Err(GlossaryError::UnsupportedSchemaVersion {
            found: meta.schema_version,
            supported: SCHEMA_VERSION,
        });
    }

    let mut warnings = Vec::new();
    let mut terms: BTreeMap<String, Term> = BTreeMap::new();

    for raw_term in raw.term {
        if terms.contains_key(&raw_term.source) {
            return Err(GlossaryError::DuplicateSource {
                term_source: raw_term.source,
            });
        }
        if !raw_term.do_not_translate && raw_term.translations.is_empty() {
            warnings.push(GlossaryWarning::TermHasNoTranslations {
                source: raw_term.source.clone(),
            });
        }
        // Unknown-locale warning per translation key. Deduplicate so an
        // unknown locale appearing in many terms produces one warning.
        for locale_id in raw_term.translations.keys() {
            if WorkspaceLocale::by_id(locale_id).is_none()
                && !warnings.iter().any(|w| {
                    matches!(w, GlossaryWarning::UnknownLocale { locale } if locale == locale_id)
                })
            {
                warnings.push(GlossaryWarning::UnknownLocale {
                    locale: locale_id.clone(),
                });
            }
        }
        terms.insert(
            raw_term.source.clone(),
            Term {
                do_not_translate: raw_term.do_not_translate,
                notes: raw_term.notes,
                translations: raw_term.translations,
            },
        );
    }

    let mut locale_overrides: BTreeMap<String, LocaleOverride> = BTreeMap::new();
    for (locale_id, raw_loc) in raw.locale {
        let register = match raw_loc.register {
            None => None,
            Some(s) => Some(parse_register(&locale_id, &s)?),
        };
        if WorkspaceLocale::by_id(&locale_id).is_none()
            && !warnings.iter().any(|w| {
                matches!(w, GlossaryWarning::UnknownLocale { locale } if locale == &locale_id)
            })
        {
            warnings.push(GlossaryWarning::UnknownLocale {
                locale: locale_id.clone(),
            });
        }
        locale_overrides.insert(
            locale_id,
            LocaleOverride {
                register,
                variant: raw_loc.variant,
            },
        );
    }

    Ok((
        Glossary {
            schema_version: meta.schema_version,
            terms,
            locale_overrides,
        },
        warnings,
    ))
}

fn parse_register(locale: &str, value: &str) -> Result<Register, GlossaryError> {
    match value {
        "formal" => Ok(Register::Formal),
        "informal" => Ok(Register::Informal),
        "neutral" => Ok(Register::Neutral),
        _ => Err(GlossaryError::InvalidRegister {
            locale: locale.to_owned(),
            value: value.to_owned(),
        }),
    }
}

/// Inverse of [`validate`]: emit a [`Raw`] shape and let `toml` serialize it.
pub(crate) fn serialize(g: &Glossary) -> Result<String, toml::ser::Error> {
    let raw = Raw {
        meta: Some(RawMeta {
            schema_version: g.schema_version,
        }),
        term: g
            .terms
            .iter()
            .map(|(source, term)| RawTerm {
                source: source.clone(),
                do_not_translate: term.do_not_translate,
                notes: term.notes.clone(),
                translations: term.translations.clone(),
            })
            .collect(),
        locale: g
            .locale_overrides
            .iter()
            .map(|(id, lo)| {
                (
                    id.clone(),
                    RawLocale {
                        register: lo.register.map(|r| r.as_str().to_owned()),
                        variant: lo.variant.clone(),
                    },
                )
            })
            .collect(),
    };
    toml::to_string(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_meta_is_error() {
        let raw: Raw = toml::from_str("").expect("empty toml parses");
        let err = validate(raw).expect_err("must error");
        assert!(matches!(err, GlossaryError::MissingSchemaVersion));
    }

    #[test]
    fn future_schema_version_is_rejected() {
        let toml_str = format!(
            "[meta]\nschema_version = {}\n",
            SCHEMA_VERSION + 1
        );
        let raw: Raw = toml::from_str(&toml_str).unwrap();
        let err = validate(raw).expect_err("must error");
        assert!(matches!(
            err,
            GlossaryError::UnsupportedSchemaVersion { found, supported }
                if found == SCHEMA_VERSION + 1 && supported == SCHEMA_VERSION
        ));
    }

    #[test]
    fn duplicate_source_is_error() {
        // The TOML array form prohibits duplicate sources at the structural
        // level (we hold them in a Vec, then catch duplicates while building
        // the BTreeMap).
        let toml_str = r#"
[meta]
schema_version = 1

[[term]]
source = "Open"
do_not_translate = true

[[term]]
source = "Open"
do_not_translate = false
"#;
        let raw: Raw = toml::from_str(toml_str).unwrap();
        let err = validate(raw).expect_err("must error");
        assert!(matches!(err, GlossaryError::DuplicateSource { term_source } if term_source == "Open"));
    }

    #[test]
    fn invalid_register_is_error() {
        let toml_str = r#"
[meta]
schema_version = 1

[locale.de_DE]
register = "casual"
"#;
        let raw: Raw = toml::from_str(toml_str).unwrap();
        let err = validate(raw).expect_err("must error");
        assert!(matches!(
            err,
            GlossaryError::InvalidRegister { locale, value }
                if locale == "de_DE" && value == "casual"
        ));
    }

    #[test]
    fn unknown_locale_warns_but_does_not_fail() {
        let toml_str = r#"
[meta]
schema_version = 1

[[term]]
source = "Save"
[term.translations]
xx_XX = "x"

[locale.xx_XX]
register = "neutral"
"#;
        let raw: Raw = toml::from_str(toml_str).unwrap();
        let (_, warnings) = validate(raw).expect("must load with warnings");
        let unknown = warnings
            .iter()
            .filter(|w| matches!(w, GlossaryWarning::UnknownLocale { .. }))
            .count();
        assert_eq!(unknown, 1, "expected exactly one UnknownLocale warning");
    }

    #[test]
    fn empty_translations_for_non_dnt_term_warns() {
        let toml_str = r#"
[meta]
schema_version = 1

[[term]]
source = "Save"
do_not_translate = false
"#;
        let raw: Raw = toml::from_str(toml_str).unwrap();
        let (_, warnings) = validate(raw).unwrap();
        let count = warnings
            .iter()
            .filter(|w| matches!(w, GlossaryWarning::TermHasNoTranslations { .. }))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn dnt_term_with_no_translations_does_not_warn() {
        let toml_str = r#"
[meta]
schema_version = 1

[[term]]
source = "ChromaCheck"
do_not_translate = true
"#;
        let raw: Raw = toml::from_str(toml_str).unwrap();
        let (_, warnings) = validate(raw).unwrap();
        assert!(
            warnings.is_empty(),
            "DNT term with no translations is fine, got {warnings:?}"
        );
    }

    #[test]
    fn register_maps_to_locales_crate_register() {
        assert_eq!(
            Register::Formal.to_locales_register(),
            i18n_harness_locales::Register::Formal
        );
        assert_eq!(
            Register::Informal.to_locales_register(),
            i18n_harness_locales::Register::Informal
        );
        assert_eq!(
            Register::Neutral.to_locales_register(),
            i18n_harness_locales::Register::Neutral
        );
    }
}
