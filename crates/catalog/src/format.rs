//! [`CatalogFormat`] and [`FormatDoc`] — the stable extension point every
//! catalog format implements (Qt `.ts`, gettext PO, ICU-JSON, and any future
//! serializer).
//!
//! # Why two traits
//!
//! `CLAUDE.md` invariant #1: "Languages, translation engines, and catalog
//! formats are data or plugins behind stable extension points."
//!
//! - [`CatalogFormat`] is the plugin: a stateless value that recognizes its
//!   files ([`CatalogFormat::sniff`]) and opens them
//!   ([`CatalogFormat::open`]).
//! - [`FormatDoc`] is one open file: its units plus whatever private state
//!   the format needs to write them back byte-stably.
//!
//! Callers (project discovery, the CLI, the desktop shell, the translation
//! session) only ever see these two traits, so a new format never requires a
//! change outside its own module and the registry that lists it.
//!
//! # Contract
//!
//! Implementations MUST satisfy the round-trip property:
//!
//! > For every catalog file `f` in the format's fixture corpus,
//! > `open(f).render(open(f).units())` produces bytes identical to `f`.
//!
//! This is the load-bearing round-trip contract. New fixtures get added to
//! the round-trip suite; they never get carved out.
//!
//! # What these traits do NOT guarantee
//!
//! - That `open` and `render` are async-safe. Implementations are
//!   synchronous, blocking I/O; async callers wrap them in `spawn_blocking`.
//! - That placeholder converters handle every printf-style or ICU construct.
//!   Each implementation documents its supported subset and refuses (with
//!   [`crate::PlaceholderError::UnsupportedSyntax`]) anything it cannot
//!   round-trip.
//! - That sniffing is conclusive. [`CatalogFormat::sniff`] reads only the
//!   head of a file and never executes anything; a project manifest can always
//!   name the format explicitly.

use std::collections::HashSet;
use std::path::Path;

use i18n_harness_core::{Unit, UnitId};

use crate::error::{CatalogError, PlaceholderError};

/// How sure a [`CatalogFormat::sniff`] call is that a file belongs to it.
///
/// Ordered: `Low < Medium < High`, so the registry can rank competing hits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Confidence {
    /// The extension matches but the content was not confirmed.
    Low,
    /// The content has the format's shape but no unambiguous marker.
    Medium,
    /// The content carries an unambiguous marker (a root element, a
    /// generator header).
    High,
}

/// A positive [`CatalogFormat::sniff`] result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SniffHit {
    /// How sure the format is.
    pub confidence: Confidence,
    /// Target language recorded in the file header, when the format has one
    /// and the head of the file contained it. Normalized to `ll_CC` form.
    pub language: Option<String>,
    /// One-line explanation of the decision, shown to users when the
    /// harness proposes a project layout.
    pub reason: String,
}

/// Strip a UTF-8 byte-order mark.
pub fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes)
}

/// Decode the longest valid UTF-8 prefix of `bytes`.
///
/// A sniff reads a fixed-size head, which can end in the middle of a
/// multi-byte character; the partial character is dropped instead of
/// rejecting the whole head.
pub fn utf8_prefix(bytes: &[u8]) -> &str {
    match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => std::str::from_utf8(&bytes[..e.valid_up_to()]).unwrap_or_default(),
    }
}

/// A catalog format plugin.
///
/// One impl per format, normally a unit struct held in a `static`; all
/// per-file state lives in the [`FormatDoc`] that [`Self::open`] returns.
pub trait CatalogFormat: std::fmt::Debug + Send + Sync {
    /// Stable identifier matching the manifest's `[[catalogs]] format = "..."`
    /// field (e.g. `"qt-ts"`, `"gettext-po"`).
    fn id(&self) -> &'static str;

    /// Human-readable name for user interfaces (e.g. `"Qt Linguist"`).
    fn display_name(&self) -> &'static str;

    /// File extensions the format claims, without the leading dot.
    fn extensions(&self) -> &'static [&'static str];

    /// Decide whether the file at `path`, whose first bytes are `head`,
    /// belongs to this format.
    ///
    /// Pure: reads nothing beyond `head` and never executes file content.
    /// Returns `None` when the file is not this format.
    fn sniff(&self, path: &Path, head: &[u8]) -> Option<SniffHit>;

    /// Parse the catalog file at `path`.
    ///
    /// The returned document's [`Unit::source`] strings carry
    /// **ICU-normalized** placeholders. The format's original placeholder
    /// spelling is preserved inside the document so [`FormatDoc::render`]
    /// can write it back unchanged.
    ///
    /// # Errors
    ///
    /// - [`CatalogError::Io`] on filesystem errors.
    /// - [`CatalogError::Parse`] when the file does not satisfy the format's
    ///   grammar.
    /// - [`CatalogError::PlaceholderConversion`] when the file contains a
    ///   placeholder construct the converter cannot represent.
    fn open(&self, path: &Path) -> Result<Box<dyn FormatDoc>, CatalogError>;

    /// Convert placeholders FROM the format's native syntax TO ICU
    /// MessageFormat.
    ///
    /// Exposed so the prompt pipeline can run model output through the same
    /// converter the catalog uses.
    ///
    /// # Errors
    ///
    /// [`PlaceholderError`] when the text holds a construct the converter
    /// cannot represent.
    fn placeholders_to_icu(&self, native: &str) -> Result<String, PlaceholderError>;

    /// Inverse of [`Self::placeholders_to_icu`]: render ICU-form text in the
    /// format's native placeholder syntax (what a translator, or a model,
    /// sees in the file).
    ///
    /// # Errors
    ///
    /// [`PlaceholderError`] when the ICU text holds a construct the format
    /// cannot express.
    fn placeholders_from_icu(&self, icu: &str) -> Result<String, PlaceholderError>;
}

/// One open catalog file.
///
/// Units are editable in place through [`Self::units_mut`]; the document keeps
/// a pristine parse-time snapshot internally, so [`Self::render`] can tell
/// edited units from untouched ones and leave untouched bytes alone.
pub trait FormatDoc: std::fmt::Debug + Send + Sync {
    /// The format plugin this document was opened with.
    fn format(&self) -> &'static dyn CatalogFormat;

    /// Path the document was read from.
    fn path(&self) -> &Path;

    /// Target language recorded in the file header, if any. Whatever the
    /// header says; the project layer cross-checks it against the manifest.
    fn language(&self) -> Option<&str>;

    /// Units in document order.
    fn units(&self) -> &[Unit];

    /// Units in document order, editable in place.
    fn units_mut(&mut self) -> &mut [Unit];

    /// Find a unit by id (linear scan).
    fn find_unit_mut(&mut self, id: &UnitId) -> Option<&mut Unit> {
        self.units_mut().iter_mut().find(|u| u.id == *id)
    }

    /// The exact bytes read from disk.
    fn source_bytes(&self) -> &[u8];

    /// Number of plural forms the file itself declares or exhibits, when it
    /// carries that evidence (a PO `Plural-Forms: nplurals=N` header; the
    /// `<numerusform>` count of a Qt plural message). `None` when the file
    /// gives no evidence.
    ///
    /// This, not the locale's CLDR category count, is what plural targets
    /// written into this file must match: Qt and gettext use their own
    /// per-language form tables.
    fn plural_forms(&self) -> Option<u32>;

    /// Render `units` back into the format. Units not present in `units`
    /// keep their original bytes; with `units == self.units()` and no edits
    /// the output is identical to [`Self::source_bytes`].
    ///
    /// # Errors
    ///
    /// - [`CatalogError::Apply`] when a unit does not belong to this document
    ///   or cannot be spliced back.
    /// - [`CatalogError::PlaceholderConversion`] when a target holds a
    ///   placeholder the format cannot express.
    fn render(&self, units: &[Unit]) -> Result<Vec<u8>, CatalogError>;

    /// Render `units` and write them atomically to `out`.
    ///
    /// # Errors
    ///
    /// Any [`Self::render`] error, or [`CatalogError::Io`] from the write.
    fn write(&self, units: &[Unit], out: &Path) -> Result<(), CatalogError> {
        let bytes = self.render(units)?;
        i18n_harness_core::fs::write_atomic(out, &bytes).map_err(|source| CatalogError::Io {
            op: "write",
            path: out.to_path_buf(),
            source,
        })
    }

    /// Render a standalone catalog that contains only the units in `keep`,
    /// with the file's header and structure intact. Used to hand a remainder
    /// of untranslated units to an external translator.
    ///
    /// # Errors
    ///
    /// [`CatalogError::Unsupported`] when the format does not implement
    /// subsetting.
    fn render_subset(&self, keep: &HashSet<UnitId>) -> Result<Vec<u8>, CatalogError> {
        let _ = keep;
        Err(CatalogError::Unsupported {
            format: self.format().id(),
            operation: "render_subset",
        })
    }
}
