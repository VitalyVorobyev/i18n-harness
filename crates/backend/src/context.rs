//! [`PromptContext`] — the read-only view a [`crate::TranslationBackend`]
//! consults at translation time.
//!
//! Carrying the context as one struct (rather than four separate
//! parameters on the trait method) has two pay-offs:
//!
//! - **Extensibility.** Adding `examples_so_far`, `metrics_snapshot`, or
//!   any other read-only signal is a struct field, not a trait method
//!   change. Every existing backend keeps compiling.
//! - **Symmetry with the prompt template.** The template renderer takes
//!   the same struct and substitutes its tokens; the backend's "build a
//!   prompt" code reads from the same shape its trait method receives.
//!
//! `PromptContext` is constructed by the **caller** (the CLI translate
//! loop, or a test harness), not the backend. The backend receives it by
//! reference, reads from it, and never mutates it.

use i18n_harness_core::{FlagSet, Unit};
use i18n_harness_glossary::{Glossary, Register};
use i18n_harness_locales::Locale;

/// Everything a backend can see when it translates one unit.
///
/// # Lifetime
///
/// All fields borrow from the caller's owned data; nothing is cloned.
/// The lifetime parameter `'a` ties the context to the caller's stack so
/// the backend cannot accidentally squirrel it away. Backends that need
/// long-lived state hold it on `self`, not on the context.
///
/// # `register` resolution
///
/// The caller resolves `register` once, threading the glossary's
/// per-locale override (if any) ahead of the workspace locales table's
/// default. By the time the backend sees [`Self::register`], it is the
/// effective value for *this* translation, not a "maybe override".
#[derive(Debug, Clone, Copy)]
pub struct PromptContext<'a> {
    /// The unit to translate. Read-only.
    pub unit: &'a Unit,

    /// Target locale.
    pub locale: &'a Locale,

    /// Effective register (glossary override winning over locales-table
    /// default). The backend renders this into the prompt as the
    /// formality instruction.
    pub register: Register,

    /// Project glossary, or `None` if no glossary is configured. The
    /// backend reads terms with [`Glossary::terms_for`] and DNT entries
    /// with [`Glossary::do_not_translate`] when building the prompt.
    pub glossary: Option<&'a Glossary>,

    /// Flags already attached to the unit before translation (e.g., a
    /// previous run's soft flags the model can condition on). The backend
    /// is free to ignore this; it is read-only signal.
    pub flags_so_far: &'a FlagSet,
}

impl<'a> PromptContext<'a> {
    /// Construct a context with all required fields. Convenience for
    /// tests; the CLI builds the same shape inline.
    pub fn new(
        unit: &'a Unit,
        locale: &'a Locale,
        register: Register,
        glossary: Option<&'a Glossary>,
        flags_so_far: &'a FlagSet,
    ) -> Self {
        Self {
            unit,
            locale,
            register,
            glossary,
            flags_so_far,
        }
    }

    /// Iterate `(source, target)` glossary pairs for the context's
    /// locale. Returns an empty iterator if no glossary is configured.
    ///
    /// Provided as a method so a backend that does not need a glossary
    /// (e.g., the trivial echo backend in tests) does not pull the
    /// `Glossary::terms_for` symbol into its surface.
    pub fn glossary_terms(&self) -> Box<dyn Iterator<Item = (&'a str, &'a str)> + 'a> {
        match self.glossary {
            Some(g) => Box::new(g.terms_for(self.locale.id)),
            None => Box::new(std::iter::empty()),
        }
    }

    /// Iterate do-not-translate sources from the glossary. Empty iterator
    /// if no glossary.
    pub fn do_not_translate(&self) -> Box<dyn Iterator<Item = &'a str> + 'a> {
        match self.glossary {
            Some(g) => Box::new(g.do_not_translate()),
            None => Box::new(std::iter::empty()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i18n_harness_core::Unit;

    #[test]
    fn glossary_terms_returns_empty_without_glossary() {
        let unit = Unit::untranslated_singular("x", "Hello");
        let locale = Locale::by_id("de_DE").unwrap();
        let flags = FlagSet::new();
        let ctx = PromptContext::new(&unit, locale, Register::Formal, None, &flags);
        assert_eq!(ctx.glossary_terms().count(), 0);
        assert_eq!(ctx.do_not_translate().count(), 0);
    }

    #[test]
    fn glossary_terms_filters_to_context_locale() {
        let toml = r#"
[meta]
schema_version = 1

[[term]]
source = "Open"
[term.translations]
de_DE = "Öffnen"
en = "Open"
"#;
        let (g, _) = Glossary::from_toml(toml).unwrap();
        let unit = Unit::untranslated_singular("x", "Hello");
        let locale = Locale::by_id("de_DE").unwrap();
        let flags = FlagSet::new();
        let ctx = PromptContext::new(&unit, locale, Register::Formal, Some(&g), &flags);
        let terms: Vec<_> = ctx.glossary_terms().collect();
        assert_eq!(terms, vec![("Open", "Öffnen")]);
    }
}
