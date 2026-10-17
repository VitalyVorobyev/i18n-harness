//! Builds [`Locale`] records from the CLDR table plus the harness defaults,
//! once per id.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;

use crate::cldr::{CldrLanguage, LANGUAGES};
use crate::{Locale, Register, Script};

/// The embedded `data/defaults.toml`.
const DEFAULTS_TOML: &str = include_str!("../data/defaults.toml");

#[derive(Debug, Deserialize)]
struct Defaults {
    fallback: Entry,
    #[serde(default)]
    scripts: BTreeMap<String, f32>,
    #[serde(default)]
    languages: BTreeMap<String, Entry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    register: Option<RegisterName>,
    length_warn_ratio: Option<f32>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RegisterName {
    Formal,
    Informal,
    Neutral,
}

impl From<RegisterName> for Register {
    fn from(r: RegisterName) -> Self {
        match r {
            RegisterName::Formal => Self::Formal,
            RegisterName::Informal => Self::Informal,
            RegisterName::Neutral => Self::Neutral,
        }
    }
}

fn defaults() -> &'static Defaults {
    static DEFAULTS: OnceLock<Defaults> = OnceLock::new();
    DEFAULTS.get_or_init(|| {
        // The file is part of the crate and covered by tests; a parse failure
        // is a build defect, not a runtime condition.
        toml::from_str(DEFAULTS_TOML).expect("crates/locales/data/defaults.toml is valid")
    })
}

/// A parsed locale tag: `language[_Script][_REGION]`, any `-`/`_` mix.
#[derive(Debug, PartialEq, Eq)]
struct Tag {
    language: String,
    script: Option<String>,
    region: Option<String>,
}

impl Tag {
    fn parse(id: &str) -> Option<Self> {
        let mut parts = id.split(['_', '-']).filter(|p| !p.is_empty());
        let language = parts.next()?;
        if !(2..=3).contains(&language.len()) || !language.bytes().all(|b| b.is_ascii_alphabetic())
        {
            return None;
        }
        let mut tag = Tag {
            language: language.to_ascii_lowercase(),
            script: None,
            region: None,
        };
        for part in parts {
            let is_alpha = part.bytes().all(|b| b.is_ascii_alphabetic());
            if part.len() == 4 && is_alpha && tag.script.is_none() && tag.region.is_none() {
                let mut s = part.to_ascii_lowercase();
                s[..1].make_ascii_uppercase();
                tag.script = Some(s);
            } else if (part.len() == 2 && is_alpha)
                || (part.len() == 3 && part.bytes().all(|b| b.is_ascii_digit()))
            {
                tag.region = Some(part.to_ascii_uppercase());
            } else {
                // Variants and extensions (`-u-…`, `-valencia`) do not change
                // anything the harness models; ignore them.
                break;
            }
        }
        Some(tag)
    }

    /// Canonical underscored id: `ll`, `ll_RR`, `ll_Ssss`, `ll_Ssss_RR`.
    fn canonical(&self) -> String {
        let mut id = self.language.clone();
        for part in [&self.script, &self.region].into_iter().flatten() {
            id.push('_');
            id.push_str(part);
        }
        id
    }
}

fn cldr_entry(tag: &Tag) -> Option<&'static CldrLanguage> {
    let find = |code: &str| LANGUAGES.iter().find(|l| l.code == code);
    tag.region
        .as_ref()
        .and_then(|region| find(&format!("{}_{region}", tag.language)))
        .or_else(|| find(&tag.language))
}

fn build(id: String, tag: &Tag, cldr: &'static CldrLanguage) -> Locale {
    let defaults = defaults();
    let script_code = tag.script.as_deref().unwrap_or(cldr.script);
    let exact = defaults.languages.get(&id);
    let language = defaults.languages.get(&tag.language);
    let register = exact
        .and_then(|e| e.register)
        .or_else(|| language.and_then(|e| e.register))
        .or(defaults.fallback.register)
        .map_or(Register::Neutral, Register::from);
    let length_warn_ratio = exact
        .and_then(|e| e.length_warn_ratio)
        .or_else(|| language.and_then(|e| e.length_warn_ratio))
        .or_else(|| defaults.scripts.get(script_code).copied())
        .or(defaults.fallback.length_warn_ratio)
        .unwrap_or(1.3);

    let id: &'static str = Box::leak(id.into_boxed_str());
    Locale {
        id,
        language: Box::leak(tag.language.clone().into_boxed_str()),
        name: cldr.name,
        cldr_plural: cldr.plurals,
        integer_plural_forms: cldr.integer_forms,
        register,
        variant: id,
        script: Script::from_iso15924(script_code),
        length_warn_ratio,
    }
}

/// Resolve `id` to a shared record, building it on first use.
pub(crate) fn resolve(id: &str) -> Option<&'static Locale> {
    static CACHE: OnceLock<Mutex<HashMap<String, &'static Locale>>> = OnceLock::new();
    let tag = Tag::parse(id)?;
    let canonical = tag.canonical();
    let cache = CACHE.get_or_init(Default::default);
    // A poisoned cache only means another thread panicked mid-insert; the map
    // itself is still consistent (inserts are single operations).
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(locale) = cache.get(&canonical) {
        return Some(locale);
    }
    let cldr = cldr_entry(&tag)?;
    // Records live for the whole process, one per distinct id the harness is
    // asked about, so leaking them is bounded and gives callers `&'static`.
    let locale: &'static Locale = Box::leak(Box::new(build(canonical.clone(), &tag, cldr)));
    cache.insert(canonical, locale);
    Some(locale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_file_parses() {
        let d = defaults();
        assert!(d.languages.contains_key("de"));
        assert!(d.scripts.contains_key("Latn"));
    }

    #[test]
    fn tags_normalize() {
        let t = Tag::parse("zh-hant-tw").unwrap();
        assert_eq!(t.canonical(), "zh_Hant_TW");
        assert_eq!(Tag::parse("sk_SK").unwrap().canonical(), "sk_SK");
        assert_eq!(Tag::parse("es-419").unwrap().canonical(), "es_419");
        assert_eq!(Tag::parse("ca-ES-valencia").unwrap().canonical(), "ca_ES");
        assert!(Tag::parse("").is_none());
        assert!(Tag::parse("x").is_none());
        assert!(Tag::parse("12_AB").is_none());
    }
}
