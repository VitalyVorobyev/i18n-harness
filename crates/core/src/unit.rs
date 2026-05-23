//! The [`Unit`] type — one translatable message — plus its support types.
//!
//! A `Unit` is the atomic thing that flows through the pipeline:
//!
//! ```text
//!   adapter.extract  →  Unit (source, no target)
//!   backend.translate →  Unit (source, target filled)
//!   gate.validate    →  Unit (with flags)
//!   adapter.apply    →  catalog (target written back)
//! ```
//!
//! The shape is the *intersection* of what every adapter and backend needs:
//! a stable id, source text, zero or more targets, per-placeholder metadata,
//! plural arity, flags, provenance. Adapter-specific bookkeeping (Qt's
//! `<location>`, gettext's `#:` comments, etc.) lives in the adapter's own
//! `Catalog` representation, not here.

use serde::{Deserialize, Serialize};

use crate::flag::FlagSet;
use crate::placeholder::Placeholder;

/// Stable identifier for a unit within a single catalog file.
///
/// Format and uniqueness scope are adapter-defined:
/// - For Qt `.ts`, the id is `"<context>::<source-key>"` (Qt has no explicit
///   id; the `(context, source)` pair is the natural key).
/// - For gettext PO, the id is `msgid` (optionally qualified by `msgctxt`).
/// - For ICU-JSON, the id is the message key.
///
/// `UnitId` is opaque to the gate and backend — they pass it through. Only
/// adapters interpret it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnitId(pub String);

impl UnitId {
    /// Borrow the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for UnitId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for UnitId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl std::fmt::Display for UnitId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Lifecycle state of a unit, mirroring Qt Linguist's `<translation type="...">`
/// vocabulary but applicable across all adapters.
///
/// State transitions on apply:
/// - [`Self::Untranslated`] → [`Self::Finished`] when we fill the target and
///   the gate passes.
/// - [`Self::Untranslated`] → [`Self::Proposed`] when the backend produced a
///   target but the gate flagged it (caller must edit or accept manually).
/// - [`Self::Vanished`] and [`Self::Obsolete`] are *never* written by the
///   harness; the adapter must preserve them verbatim and skip them when
///   selecting units to translate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnitState {
    /// No target text yet. Default state for newly-extracted units.
    Untranslated,
    /// Target text is present but unconfirmed (gate flagged it, or human has
    /// not signed off). In Qt this maps to `<translation type="unfinished">`
    /// with non-empty body.
    Proposed,
    /// Target text is present and confirmed; safe to ship. In Qt this is
    /// `<translation>` with no `type` attribute.
    Finished,
    /// The source string no longer exists in the source code; the catalog
    /// kept the historical entry. The harness must never touch these.
    Vanished,
    /// Stronger form of `Vanished`: marked for deletion at the next catalog
    /// regen. Again, never touched by the harness.
    Obsolete,
}

impl UnitState {
    /// Returns true if the harness is allowed to write a new target into a
    /// unit currently in this state.
    ///
    /// This is the single canonical source of that rule; adapters and the
    /// backend driver consult it instead of replicating the match.
    pub fn is_writable(self) -> bool {
        matches!(self, Self::Untranslated | Self::Proposed)
    }
}

/// One or more target strings for a unit.
///
/// Most messages are singular (`Singular(target)`). Plural messages have one
/// target per CLDR plural category for the target locale, in CLDR's
/// canonical category order: `zero, one, two, few, many, other`. The number
/// of slots equals the locale's plural arity; the gate (M1) enforces this.
///
/// Each slot is `Option<String>`: `None` = not yet filled; `Some("")` = filled
/// but empty (a valid translation choice in some languages).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Target {
    /// Single target string.
    Singular {
        /// The target text, or `None` if not yet translated.
        text: Option<String>,
    },
    /// One target per CLDR plural form for the target locale.
    Plural {
        /// Forms in CLDR canonical order; length equals the target locale's
        /// plural arity once the unit has been routed through a locale.
        forms: Vec<Option<String>>,
    },
}

impl Target {
    /// True if every required slot has been filled with some (possibly
    /// empty) string.
    pub fn is_complete(&self) -> bool {
        match self {
            Self::Singular { text } => text.is_some(),
            Self::Plural { forms } => forms.iter().all(Option::is_some),
        }
    }

    /// True if no slot has any text. New units extracted from a catalog with
    /// no prior translation are in this shape.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Singular { text } => text.is_none(),
            Self::Plural { forms } => forms.iter().all(Option::is_none),
        }
    }
}

/// Source-side jump-back information so the UI (and CLI) can show "this
/// string came from src/foo.cpp:123".
///
/// Adapters fill this on `extract` from whatever the catalog provides
/// (Qt's `<location filename="..." line="..."/>`, gettext's `#: file:line`,
/// none for ICU-JSON unless the build tooling preserved it).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Source file path as the catalog records it (typically relative to the
    /// project root). Empty if the catalog format does not record one.
    pub file: String,
    /// 1-based line number, or `None` if not recorded.
    pub line: Option<u32>,
    /// 0-based byte offset within the source file, or `None` if not
    /// recorded. Some catalog formats (Qt) record only a line; others may
    /// record both.
    pub byte_offset: Option<u32>,
}

/// One translatable message.
///
/// # Invariants
///
/// - [`Self::id`] is unique within the originating catalog file.
/// - [`Self::source`] is the ICU-normalized source text — placeholders
///   already converted from the catalog's native form.
/// - [`Self::placeholders`] is the *multiset* of placeholder occurrences in
///   [`Self::source`], in left-to-right order. The gate compares this against
///   the target's placeholders.
/// - If [`Self::plural_arity`] is `Some(n)`, then [`Self::target`] is
///   [`Target::Plural`] with `n` slots (once routed through a locale). If
///   `None`, target is [`Target::Singular`].
/// - [`Self::state`] reflects what the catalog says about this unit's
///   completeness; the harness mutates it only on successful apply.
///
/// # What `Unit` does NOT guarantee
///
/// - That the placeholder converter chose the right ICU index — the
///   adapter's round-trip property test is what proves that.
/// - That the target satisfies any locale's plural arity — the gate enforces
///   that.
/// - That the source text is non-empty — adapters may extract empty messages
///   (Qt allows them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    /// Stable identifier within the catalog file.
    pub id: UnitId,

    /// Source text, ICU-normalized.
    pub source: String,

    /// Target text(s). See [`Target`].
    pub target: Target,

    /// Placeholders occurring in [`Self::source`] (multiset, left-to-right).
    pub placeholders: Vec<Placeholder>,

    /// CLDR plural-category arity for plural units; `None` for singular
    /// units. The adapter sets this from the catalog (Qt: number of
    /// `<numerusform>` entries; ICU: presence of a `plural` selector).
    ///
    /// Note: the *value* is the source-side arity. The target locale's arity
    /// may differ; that mismatch is the gate's
    /// [`crate::Flag::PluralArityMismatch`] check.
    pub plural_arity: Option<u32>,

    /// Flags attached to this unit by the gate and/or the backend.
    pub flags: FlagSet,

    /// Where in the source code this string originated. May be default-empty
    /// if the catalog format does not record it.
    pub provenance: Provenance,

    /// Lifecycle state. See [`UnitState`].
    pub state: UnitState,
}

impl Unit {
    /// Construct a minimal untranslated singular unit. Useful for tests and
    /// for adapters that build up a unit incrementally.
    pub fn untranslated_singular(id: impl Into<UnitId>, source: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            target: Target::Singular { text: None },
            placeholders: Vec::new(),
            plural_arity: None,
            flags: FlagSet::new(),
            provenance: Provenance::default(),
            state: UnitState::Untranslated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writable_states_are_only_untranslated_and_proposed() {
        assert!(UnitState::Untranslated.is_writable());
        assert!(UnitState::Proposed.is_writable());
        assert!(!UnitState::Finished.is_writable());
        assert!(!UnitState::Vanished.is_writable());
        assert!(!UnitState::Obsolete.is_writable());
    }

    #[test]
    fn singular_completeness() {
        let mut t = Target::Singular { text: None };
        assert!(t.is_empty());
        assert!(!t.is_complete());
        t = Target::Singular {
            text: Some(String::new()),
        };
        assert!(!t.is_empty());
        assert!(t.is_complete());
    }

    #[test]
    fn plural_completeness() {
        let t = Target::Plural {
            forms: vec![None, None],
        };
        assert!(t.is_empty());
        assert!(!t.is_complete());
        let t = Target::Plural {
            forms: vec![Some("eins".into()), Some("andere".into())],
        };
        assert!(!t.is_empty());
        assert!(t.is_complete());
        let t = Target::Plural {
            forms: vec![Some("eins".into()), None],
        };
        assert!(!t.is_empty());
        assert!(!t.is_complete());
    }
}
