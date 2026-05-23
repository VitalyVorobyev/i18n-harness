//! Error and warning types for the glossary loader.
//!
//! Errors abort `load`; warnings are returned alongside a successful load
//! and the caller decides whether to print them.

use thiserror::Error;

/// Failure modes of [`crate::Glossary::load`] / [`crate::Glossary::from_toml`].
///
/// Each variant names a single, recoverable-from-the-caller's-side failure.
/// Higher-level errors (e.g. "the catalog points at a glossary that no
/// longer exists") are not modeled here; that is the CLI's concern.
#[derive(Debug, Error)]
pub enum GlossaryError {
    /// File could not be read from disk.
    ///
    /// Occurs when the path does not exist or the process lacks permission.
    /// Caller: surface the path and let the user fix it.
    #[error("read glossary `{path}`: {source}")]
    Io {
        /// Path that failed to open.
        path: String,
        /// Underlying I/O error.
        source: std::io::Error,
    },

    /// File contents are not valid TOML.
    ///
    /// Occurs when the file is corrupted or hand-edited into invalid syntax.
    /// Caller: surface the parser error to the user verbatim (`toml::de` has
    /// good line/column info).
    #[error("parse glossary TOML: {0}")]
    Toml(#[from] toml::de::Error),

    /// `meta.schema_version` is from a future version this binary does not
    /// understand.
    ///
    /// Occurs when a glossary is loaded by an older `i18n-harness`. Caller:
    /// upgrade the harness or downgrade the glossary.
    #[error(
        "glossary schema_version {found} is newer than this binary's supported maximum {supported}"
    )]
    UnsupportedSchemaVersion {
        /// Version read from `meta.schema_version`.
        found: u32,
        /// Highest version this binary supports.
        supported: u32,
    },

    /// `meta.schema_version` is missing.
    ///
    /// Occurs when the file has no `[meta]` table or the table omits the
    /// field. We treat this as a hard error because silently defaulting to
    /// version 1 would let a future v2 file load into a v1 binary that
    /// misinterprets its fields.
    #[error("glossary missing `meta.schema_version`")]
    MissingSchemaVersion,

    /// Two `[[term]]` entries share the same `source` string.
    ///
    /// Occurs when a glossary has been mis-edited (rare; the structure
    /// usually catches this) or when two contributors add the same term in
    /// parallel. Caller: surface the duplicated source so the user can pick
    /// which entry to keep.
    #[error("duplicate term source `{term_source}` in glossary")]
    DuplicateSource {
        /// The source string that appeared twice.
        term_source: String,
    },

    /// A `register` value is not one of `formal|informal|neutral`.
    ///
    /// Occurs when a user types a typo or invents a register name. Caller:
    /// surface the bad value and the entry it appeared in.
    #[error(
        "invalid register `{value}` in `[locale.{locale}]` (must be one of formal|informal|neutral)"
    )]
    InvalidRegister {
        /// Locale id whose `[locale.<id>]` table contained the bad value.
        locale: String,
        /// The bad register string.
        value: String,
    },

    /// Re-serializing the glossary to TOML failed.
    ///
    /// Effectively unreachable for the current schema; surfaced so future
    /// shape changes do not silently corrupt round-trip.
    #[error("serialize glossary to TOML: {0}")]
    Serialize(String),
}

/// Non-fatal note produced by [`crate::Glossary::load`]; never aborts the
/// load.
///
/// We return these as a vector alongside the loaded glossary so the CLI can
/// print them once at startup. The set is closed and machine-inspectable,
/// not free-form strings, so a future "ignore-warnings" flag could filter
/// them by kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlossaryWarning {
    /// A `[locale.<id>]` table references a locale id not present in the
    /// workspace's `crates/locales` table.
    ///
    /// This is permitted: the glossary may be ahead of locale registration.
    /// The warning lets the user notice typos early (`de_DE` vs `de-DE` vs
    /// `de`).
    UnknownLocale {
        /// The locale id as it appears in the glossary.
        locale: String,
    },

    /// A non-DNT term has an empty translations table.
    ///
    /// This is permitted: the maintainer may be staging an entry that will
    /// be filled later. The warning surfaces the stale row.
    TermHasNoTranslations {
        /// The term's source string.
        source: String,
    },
}

impl std::fmt::Display for GlossaryWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownLocale { locale } => {
                write!(
                    f,
                    "glossary references locale `{locale}` which is not in the workspace locales table"
                )
            }
            Self::TermHasNoTranslations { source } => {
                write!(
                    f,
                    "glossary term `{source}` has no translations and is not marked do_not_translate"
                )
            }
        }
    }
}
