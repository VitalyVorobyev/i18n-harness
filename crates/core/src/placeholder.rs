//! [`Placeholder`] metadata carried by every [`crate::Unit`].
//!
//! A placeholder is a span inside the source (or target) text that the
//! translation engine must preserve verbatim — `%1`, `%n`, `{count}`,
//! `{name}`, `%(user)s`, and so on, normalized into ICU MessageFormat on
//! extract.
//!
//! Adapters detect placeholders during `extract` and translate them into ICU
//! form; that ICU form (plus the original-form metadata, so write-back can
//! reverse the normalization) is what the backend sees. The gate checks that
//! the target text contains the **same multiset** of placeholders.

use serde::{Deserialize, Serialize};

/// What kind of placeholder this is.
///
/// The kind affects validation (e.g. the plural-count placeholder must appear
/// in plural messages) and reverse normalization (the adapter must know what
/// the original token looked like to write it back).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlaceholderKind {
    /// A positional argument: Qt `%1`, `%2`; gettext `%s`, `%d`; ICU `{0}`,
    /// `{1}`. Index is carried in [`Placeholder::index`].
    Positional,
    /// A named argument: ICU `{name}`, Python-style gettext `%(user)s`. Name
    /// is carried in [`Placeholder::name`].
    Named,
    /// The plural-count argument: Qt `%n`, ICU `{count, plural, …}`. By
    /// convention this crate normalizes to a placeholder named `count`. The
    /// gate requires it on any unit whose [`crate::Unit::plural_arity`] is
    /// `Some(_)`.
    PluralCount,
    /// A locale-aware integer: Qt `%L1`, `%L2`. Carries the same index as
    /// [`Self::Positional`] but the adapter must reinsert the `L` marker on
    /// write-back. Treated as a `Positional` by the gate.
    LocaleAwareInt,
}

/// The ICU-form rendering of a placeholder, as it appears in the intermediate
/// (ICU MessageFormat) text seen by the backend and the gate.
///
/// Carrying it explicitly (instead of just splicing into a string) lets
/// `from_icu` round-trip back to the original Qt/PO form without re-parsing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IcuForm {
    /// The literal ICU token, e.g. `{0}`, `{count}`, `{name}`.
    pub token: String,
}

/// A single placeholder occurrence inside source or target text.
///
/// One source token can produce many `Placeholder` entries — once per
/// occurrence (a multiset, not a set). Validation in the gate walks the
/// vector and compares multisets, not sets, because dropping a duplicate is a
/// real translation bug.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Placeholder {
    /// The kind of placeholder; drives both validation and reverse
    /// normalization.
    pub kind: PlaceholderKind,

    /// Positional index for [`PlaceholderKind::Positional`] and
    /// [`PlaceholderKind::LocaleAwareInt`]. **Zero-indexed** in this crate
    /// (matching ICU); adapters that use 1-indexed tokens (Qt) translate at
    /// the boundary.
    ///
    /// `None` for named placeholders and for the plural-count placeholder.
    pub index: Option<u32>,

    /// Named-placeholder name, e.g. `count`, `user`. `None` for positional
    /// placeholders.
    pub name: Option<String>,

    /// Byte offset of the placeholder token's first byte in the **ICU-form
    /// text** of the source unit. Used by UI jump-back and by the gate when
    /// reporting which placeholder is mismatched.
    ///
    /// Adapters may set this to `0` for placeholders extracted from target
    /// text (since target text is filled later); the gate does not require
    /// it.
    pub byte_offset: u32,

    /// The ICU-form token as it appears in the intermediate text. Carried
    /// explicitly so the inverse converter can rewrite the token without
    /// re-parsing the ICU string.
    pub icu_form: IcuForm,
}

impl Placeholder {
    /// Construct a positional placeholder with the given zero-based index.
    pub fn positional(index: u32, byte_offset: u32) -> Self {
        Self {
            kind: PlaceholderKind::Positional,
            index: Some(index),
            name: None,
            byte_offset,
            icu_form: IcuForm {
                token: format!("{{{index}}}"),
            },
        }
    }

    /// Construct a locale-aware integer placeholder (Qt `%L1`-style) with the
    /// given zero-based index.
    pub fn locale_aware_int(index: u32, byte_offset: u32) -> Self {
        Self {
            kind: PlaceholderKind::LocaleAwareInt,
            index: Some(index),
            name: None,
            byte_offset,
            icu_form: IcuForm {
                token: format!("{{{index}}}"),
            },
        }
    }

    /// Construct the plural-count placeholder (Qt `%n` → `{count}` by this
    /// crate's convention).
    pub fn plural_count(byte_offset: u32) -> Self {
        Self {
            kind: PlaceholderKind::PluralCount,
            index: None,
            name: Some("count".to_owned()),
            byte_offset,
            icu_form: IcuForm {
                token: "{count}".to_owned(),
            },
        }
    }

    /// Construct a named placeholder (e.g. ICU `{user}`).
    pub fn named(name: impl Into<String>, byte_offset: u32) -> Self {
        let name = name.into();
        let token = format!("{{{name}}}");
        Self {
            kind: PlaceholderKind::Named,
            index: None,
            name: Some(name),
            byte_offset,
            icu_form: IcuForm { token },
        }
    }
}
