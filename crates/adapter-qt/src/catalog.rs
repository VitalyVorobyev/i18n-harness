//! [`Catalog`] — the parser's output, holding both the structured units and
//! the byte-level state needed for byte-stable round-trip on apply.

use std::path::PathBuf;

use i18n_harness_core::{Unit, UnitId};

/// The output of [`crate::extract`]: a list of [`Unit`]s plus the
/// preserved byte-level state required for byte-stable round-trip.
///
/// The structure is deliberately opaque to non-adapter code; callers should
/// treat it as a token they pass to [`crate::apply`].
#[derive(Debug, Clone)]
pub struct Catalog {
    /// Path the catalog was read from. Carried for diagnostic messages and
    /// for `apply` to emit useful errors; `apply` writes to a caller-supplied
    /// output path, not back to this one.
    pub(crate) source_path: PathBuf,

    /// Target language as recorded in `<TS language="…">`. `None` if the
    /// root element did not declare one. Backends consume this to pick the
    /// CLDR record + register; the harness does not infer it from filename.
    pub(crate) language: Option<String>,

    /// The full bytes of the source `.ts` file. We reuse this on `apply` so
    /// anything we did not deliberately edit comes through verbatim.
    pub(crate) source_bytes: Vec<u8>,

    /// Per-unit byte ranges (parallel to [`Self::units`]). The ranges point
    /// into [`Self::source_bytes`].
    pub(crate) edit_points: Vec<EditPoint>,

    /// The structured units, in document order.
    pub(crate) units: Vec<Unit>,

    /// Pristine snapshot of `units` as produced by [`crate::extract`], before
    /// any mutation through [`Self::units_mut`] / [`Self::find_unit_mut`].
    /// Length and order match [`Self::units`] one-to-one.
    ///
    /// This exists so [`crate::apply`] can answer "did the caller actually
    /// change this unit?" by comparing the candidate against the parse-time
    /// truth instead of the live (possibly mutated) [`Self::units`] slice.
    /// The Tauri frontend uses the [`Self::find_unit_mut`] / `units_mut`
    /// pattern, which mutates [`Self::units`] in place; without this
    /// snapshot, `plan_edits_for_unit`'s `candidate.target == original.target`
    /// shortcut would silently treat every save as a no-op.
    pub(crate) original_units: Vec<Unit>,

    /// Byte span of each unit's full `<message …>…</message>` element,
    /// parallel to [`Self::units`]. Ranges point into [`Self::source_bytes`]
    /// with the usual inclusive-start / exclusive-end convention: `start` is
    /// the `<` of `<message`, `end` is the byte just past the `>` of
    /// `</message>`.
    ///
    /// Used by [`crate::render_subset`] for byte-subtraction: a kept message
    /// is copied verbatim, a dropped message is spliced out. The leading
    /// whitespace before `start` is **not** included here; the subset writer
    /// absorbs it separately so the two concerns stay independent.
    pub(crate) message_spans: Vec<(usize, usize)>,

    /// One [`ContextSpan`] per `<context>` element, in document order.
    /// Records the context's block span plus the indices (into
    /// [`Self::units`]) of the messages it contains. Used by
    /// [`crate::render_subset`] to prune a context whose every member was
    /// dropped.
    pub(crate) contexts: Vec<ContextSpan>,
}

impl Catalog {
    /// Borrow the units in document order.
    pub fn units(&self) -> &[Unit] {
        &self.units
    }

    /// Mutably borrow the units. The frontend (or any in-process owner of
    /// the catalog) edits targets through this slice; the byte buffer and
    /// edit-points stay untouched until [`crate::apply`] runs.
    pub fn units_mut(&mut self) -> &mut [Unit] {
        &mut self.units
    }

    /// Find a unit by id, returning a mutable borrow. Linear scan because
    /// catalogs are small (hundreds of units) and we don't want to keep a
    /// parallel index in sync on every mutation.
    pub fn find_unit_mut(&mut self, id: &UnitId) -> Option<&mut Unit> {
        self.units.iter_mut().find(|u| u.id == *id)
    }

    /// Take ownership of the units (mutating a caller-owned `Vec<Unit>` is
    /// the more common path; we expose this for completeness).
    pub fn into_units(self) -> Vec<Unit> {
        self.units
    }

    /// Borrow the source bytes (useful for callers diffing the input vs
    /// the apply output).
    pub fn source_bytes(&self) -> &[u8] {
        &self.source_bytes
    }

    /// Borrow the source path the catalog was read from.
    pub fn source_path(&self) -> &PathBuf {
        &self.source_path
    }

    /// Target language declared on the `<TS>` root element, if present.
    /// Returned as the raw CLDR-style id (`de_DE`, `es_ES`, `zh_Hans`, …).
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }
}

/// Byte-level handle for one unit: where its `<translation>` body, its
/// optional `<numerusform>` bodies, and its optional `type=` attribute on
/// `<translation>` live in [`Catalog::source_bytes`].
///
/// All ranges are `start..end` byte indices into the source bytes, with the
/// **inclusive** start and **exclusive** end convention `Range<usize>` uses.
#[derive(Debug, Clone)]
pub(crate) struct EditPoint {
    /// `(start, end)` of the content between `<translation>` and
    /// `</translation>`. For plural messages this contains the
    /// `<numerusform>` children plus their surrounding whitespace.
    pub(crate) translation_body: (usize, usize),

    /// Position of the `type` attribute value (without quotes) on the
    /// `<translation>` open tag, if present. `None` means the open tag was
    /// `<translation>` (no `type`) — i.e. the unit was already finished.
    ///
    /// On apply we may rewrite the attribute *value* (e.g. `unfinished` →
    /// removing the attribute entirely when promoting to finished). The
    /// rewrite logic in `write.rs` handles attribute insertion/removal,
    /// not just value replacement.
    pub(crate) type_attr: Option<TypeAttr>,

    /// For plural messages: byte ranges of each `<numerusform>` element's
    /// content, in document order. Empty for singular messages.
    pub(crate) numerus_bodies: Vec<(usize, usize)>,

    /// Byte range of the `<translation ...>` open tag (from `<` to `>`).
    /// Used to surgically rewrite the attribute set without disturbing the
    /// element body.
    pub(crate) translation_open_tag: (usize, usize),

    /// State of the unit *as recorded in the source bytes*, so apply can
    /// detect when the in-memory `Unit::state` was changed by the caller.
    pub(crate) original_state: i18n_harness_core::UnitState,

    /// Original translation body bytes, used by apply to decide whether the
    /// caller actually changed anything (vs. just round-tripping). Stored
    /// rather than re-sliced so the comparison is one memcmp.
    pub(crate) original_translation_body: Vec<u8>,

    /// Original numerus form bytes, parallel to [`Self::numerus_bodies`].
    pub(crate) original_numerus_bodies: Vec<Vec<u8>>,
}

/// Byte-level handle for one `<context>` element, used by
/// [`crate::render_subset`] to drop contexts that become empty after a
/// subset deletes all their messages.
///
/// `block` is `start..end` into [`Catalog::source_bytes`]: `start` is the
/// `<` of `<context>`, `end` is the byte just past the `>` of `</context>`.
/// `member_unit_indices` lists the indices (into [`Catalog::units`]) of the
/// messages parsed inside this context, in document order. A `<message>`
/// that parsed to no unit (e.g. one with no `<source>`) contributes no
/// index, which is intentional: such bytes are part of the context block and
/// are removed only when the *whole context* is removed.
#[derive(Debug, Clone)]
pub(crate) struct ContextSpan {
    pub(crate) block: (usize, usize),
    pub(crate) member_unit_indices: Vec<usize>,
}

/// Where the `type="..."` attribute on a `<translation>` lives.
#[derive(Debug, Clone)]
pub(crate) struct TypeAttr {
    /// Byte range of the whole attribute, including any leading space, the
    /// name, `=`, the quote characters, and the value. So removing the
    /// attribute means splicing `b""` over this range.
    pub(crate) full_range: (usize, usize),
    /// The original value (`"unfinished"`, `"vanished"`, or `"obsolete"`).
    pub(crate) original_value: String,
}
