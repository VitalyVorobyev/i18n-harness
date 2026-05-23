//! Glossary model and TOML loader/validator.
//!
//! See `docs/initial_design.md` §10. The full TOML schema, per-locale terms,
//! `do_not_translate` lists, and the validator land in **M2** alongside the
//! backend trait — they are not exercised by anything in M1.
//!
//! This module currently exposes only a **marker type** [`Glossary`]. The
//! gate (M1) accepts `Option<&Glossary>` so that adding real glossary checks
//! in M2 does not change the gate's public signature; until then the option
//! is always passed as `None` by callers.
//!
//! # Stability
//!
//! When the real loader lands, [`Glossary`] gains fields rather than being
//! replaced. Public functions that take `Option<&Glossary>` will continue to
//! compile against this marker shape.

#![forbid(unsafe_code)]

/// Per-project glossary.
///
/// **Placeholder type.** The real shape lands in M2: TOML-loaded entries with
/// per-locale terms, a `do_not_translate` list, and a register/variant header.
/// For now this is a unit struct so that callers that thread `Option<&Glossary>`
/// can compile against the eventual API today.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Glossary;

impl Glossary {
    /// Construct an empty glossary. M1 callers never need this; it is provided
    /// for tests that want to exercise the `Some(&Glossary)` branch of an API
    /// even though M1 behaves identically for `None` and `Some`.
    pub fn empty() -> Self {
        Self
    }
}
