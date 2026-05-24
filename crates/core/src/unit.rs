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
use sha2::{Digest, Sha256};

use crate::flag::FlagSet;
use crate::placeholder::Placeholder;
use crate::review::ReviewStatus;

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

    /// Short content-addressed digest of the translator-visible identity of the
    /// source for this unit.
    ///
    /// Populated by the adapter on extract; `None` when the adapter does not
    /// (yet) compute one (PO and ICU-JSON adapters in early M4 ship `None` and
    /// start filling it in later milestones without a `Unit` schema bump).
    ///
    /// Format: `"sha256:<12 hex chars>"` — the first 48 bits of a SHA-256 over
    /// the components defined in the M4.1.5 design §2.
    ///
    /// # What this guarantees
    ///
    /// - Stable across repeated extracts of the *same* on-disk bytes for the
    ///   same unit.
    /// - Changes when the source text, the disambiguation comment, or the
    ///   developer comment changes.
    ///
    /// # What this explicitly does NOT guarantee
    ///
    /// - Collision resistance at cryptographic strength. The hash is truncated
    ///   to 48 bits; this is enough for one project's corpus (~10k units), not
    ///   enough for cross-project comparison.
    /// - That a missing `source_hash` (`None`) implies the unit is new.
    ///   `None` means the adapter that produced this unit did not compute one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,

    /// Translator-facing review state, **orthogonal to [`UnitState`]**.
    ///
    /// Where [`UnitState`] mirrors the catalog's structural state, `review_status`
    /// records the reviewer's process state. `None` is the canonical default for
    /// a freshly-extracted unit; the project crate fills it in from
    /// `<state_dir>/review.jsonl` on load. **Never serialized into the catalog
    /// file.**
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_status: Option<ReviewStatus>,

    /// True when this unit's `review_status` was recorded against an earlier
    /// `source_hash` that no longer matches the current extract.
    ///
    /// Derived at project-open time by comparing `source_hash` against the hash
    /// stored in `review.jsonl` at the time of the last status change. **Not
    /// serialized** — recomputed on every open so it cannot drift from the truth
    /// on disk.
    #[serde(skip)]
    pub source_changed_since_review: bool,
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
            source_hash: None,
            review_status: None,
            source_changed_since_review: false,
        }
    }
}

/// Compute the M4.1.5 source hash for a unit.
///
/// `disambiguation` and `extracomment` are the empty string when the catalog
/// format does not record them (PO has no Qt-style comment distinction;
/// ICU-JSON typically has neither). The plural flag is derived from the unit's
/// `plural_arity.is_some()`.
///
/// # Algorithm
///
/// ```text
/// input = source ‖ 0x1F ‖ disambiguation ‖ 0x1F ‖ extracomment ‖ 0x1F ‖ "P"|"S"
/// digest = SHA-256(input)
/// result = "sha256:" + hex(digest)[..12]
/// ```
///
/// See `docs/m4.1.5-source-hash-review-status-design.md` §2 for the rationale.
pub fn compute_source_hash(
    source: &str,
    disambiguation: &str,
    extracomment: &str,
    plural: bool,
) -> String {
    const SEP: u8 = 0x1F; // ASCII unit-separator
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    hasher.update([SEP]);
    hasher.update(disambiguation.as_bytes());
    hasher.update([SEP]);
    hasher.update(extracomment.as_bytes());
    hasher.update([SEP]);
    hasher.update(if plural { b"P" } else { b"S" });
    let digest = hasher.finalize();
    let hex = format!("{digest:x}");
    format!("sha256:{}", &hex[..12])
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

    // ── compute_source_hash ───────────────────────────────────────────────────

    #[test]
    fn hash_is_stable_across_calls() {
        let h1 = compute_source_hash("Open file", "menu", "", false);
        let h2 = compute_source_hash("Open file", "menu", "", false);
        assert_eq!(h1, h2, "hash must be deterministic");
    }

    #[test]
    fn hash_changes_with_each_input() {
        let base = compute_source_hash("Open file", "menu", "some context", false);
        assert_ne!(
            base,
            compute_source_hash("Open filX", "menu", "some context", false),
            "changing source must change hash"
        );
        assert_ne!(
            base,
            compute_source_hash("Open file", "menX", "some context", false),
            "changing disambiguation must change hash"
        );
        assert_ne!(
            base,
            compute_source_hash("Open file", "menu", "some contexX", false),
            "changing extracomment must change hash"
        );
        assert_ne!(
            base,
            compute_source_hash("Open file", "menu", "some context", true),
            "flipping plural must change hash"
        );
    }

    #[test]
    fn hash_format_matches_prefix_and_length() {
        let h = compute_source_hash("Hello", "", "", false);
        assert!(
            h.starts_with("sha256:"),
            "hash must start with 'sha256:': {h}"
        );
        let hex_part = h.trim_start_matches("sha256:");
        assert_eq!(hex_part.len(), 12, "hex part must be exactly 12 chars: {h}");
        assert!(
            hex_part.chars().all(|c| c.is_ascii_hexdigit()),
            "hex part must be lowercase hex: {h}"
        );
    }

    #[test]
    fn empty_input_hash_is_deterministic_and_pinned() {
        // Pin this value: if the algorithm changes, this test fails loudly.
        let h = compute_source_hash("", "", "", false);
        // Preimage: b"\x1F\x1F\x1FS"
        // Verify format first (correctness), then pin the value.
        assert!(h.starts_with("sha256:"), "unexpected format: {h}");
        assert_eq!(h.len(), 19, "unexpected length: {h}");
        // Pinned value — changing the algorithm requires updating this assert.
        assert_eq!(
            h, "sha256:2ea032865565",
            "empty-input hash changed — algorithm was modified"
        );
    }

    #[test]
    fn unit_serde_round_trip_with_new_fields() {
        let mut unit = Unit::untranslated_singular("ctx::hello", "Hello");
        unit.source_hash = Some("sha256:aabbccddeeff".to_owned());
        unit.review_status = Some(ReviewStatus::Approved);
        unit.source_changed_since_review = true; // must NOT appear in JSON

        let json = serde_json::to_string(&unit).expect("serialize");

        // source_changed_since_review is skipped
        assert!(
            !json.contains("source_changed_since_review"),
            "skipped field leaked into JSON: {json}"
        );

        let restored: Unit = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.source_hash, Some("sha256:aabbccddeeff".to_owned()));
        assert_eq!(restored.review_status, Some(ReviewStatus::Approved));
        // Derived field resets to false on deserialize.
        assert!(!restored.source_changed_since_review);
    }

    #[test]
    fn unit_serde_backward_compat_without_new_fields() {
        // A JSONL line without source_hash or review_status (old format)
        // must deserialize cleanly with None defaults.
        let json = r#"{"id":"ctx::hi","source":"Hi","target":{"kind":"singular","text":null},"placeholders":[],"plural_arity":null,"flags":[],"provenance":{"file":"","line":null,"byte_offset":null},"state":"untranslated"}"#;
        let unit: Unit = serde_json::from_str(json).expect("deserialize old format");
        assert!(unit.source_hash.is_none());
        assert!(unit.review_status.is_none());
        assert!(!unit.source_changed_since_review);
    }

    #[test]
    fn unit_with_none_fields_omits_them_in_json() {
        let unit = Unit::untranslated_singular("ctx::x", "X");
        let json = serde_json::to_string(&unit).expect("serialize");
        assert!(
            !json.contains("source_hash"),
            "None source_hash must be omitted: {json}"
        );
        assert!(
            !json.contains("review_status"),
            "None review_status must be omitted: {json}"
        );
    }

    #[test]
    fn review_status_serde_uses_kebab_case() {
        let s = serde_json::to_string(&ReviewStatus::MachineTranslated).expect("serialize");
        assert_eq!(s, r#""machine-translated""#);
        let s = serde_json::to_string(&ReviewStatus::NeedsReview).expect("serialize");
        assert_eq!(s, r#""needs-review""#);
    }
}

#[cfg(test)]
mod hash_proptest {
    use proptest::prelude::*;

    use super::compute_source_hash;

    proptest! {
        /// Over 1000 random (source, disambiguation, extracomment, plural) tuples,
        /// two different tuples should produce different hashes with overwhelming
        /// probability (birthday bound for 48-bit hash and 1000 inputs is ~1 in 10^9).
        #[test]
        fn no_hash_collisions_over_random_tuples(
            s1 in ".*",
            d1 in ".*",
            e1 in ".*",
            p1: bool,
            s2 in ".*",
            d2 in ".*",
            e2 in ".*",
            p2: bool,
        ) {
            let h1 = compute_source_hash(&s1, &d1, &e1, p1);
            let h2 = compute_source_hash(&s2, &d2, &e2, p2);
            // Only assert inequality when the inputs actually differ.
            if (s1 != s2) || (d1 != d2) || (e1 != e2) || (p1 != p2) {
                prop_assert_ne!(
                    h1,
                    h2,
                    "collision between ({:?},{:?},{:?},{:?}) and ({:?},{:?},{:?},{:?})",
                    s1, d1, e1, p1, s2, d2, e2, p2
                );
            }
        }
    }
}
