//! [`Catalog`] — the format-neutral output of a
//! [`crate::CatalogFormat::extract`] call.
//!
//! Mirrors the shape of `i18n_harness_adapter_qt::Catalog`: structured
//! [`Unit`]s plus the original source bytes so that
//! [`crate::CatalogFormat::apply`] can byte-stably round-trip every fixture.
//!
//! Deliberately **format-agnostic**: the value holds no PO-, JSON-, or
//! XML-specific state. Per-format edit indices (line offsets for PO msgstr
//! blocks, byte ranges for ICU-JSON value positions) live in
//! [`Catalog::extract_state`] as an opaque blob the format owns.

use std::path::PathBuf;

use i18n_harness_core::{Unit, UnitId};

/// The output of [`crate::CatalogFormat::extract`].
///
/// Format-neutral container. Apply implementations recover their per-unit
/// edit information from a private format-specific state blob — it is
/// opaque to callers and only the format that produced it knows its shape.
///
/// # Invariants
///
/// - The source bytes are the exact contents of the file on disk at extract
///   time. Apply implementations rely on this for byte-stable round-trip on
///   unmodified units.
/// - [`Self::units`] is in document order.
/// - Each unit's [`Unit::id`] is unique within this catalog.
/// - [`Self::language`] reflects whatever the format records in its header
///   (PO `Language:` field, ICU JSON convention). `None` when the format does
///   not record one or the file omits it; the project layer is responsible
///   for cross-checking against the manifest-declared locale.
///
/// # What this type does NOT guarantee
///
/// - That `units` and the format-private extract state stay in sync if a
///   caller manually replaces `units` with a shorter or longer vec. The
///   `apply` impl will then refuse to write (returning
///   [`crate::CatalogError::Apply`]).
/// - That the language string is a valid CLDR locale id. It is whatever the
///   format header contained.
#[derive(Debug, Clone)]
pub struct Catalog {
    /// Absolute path the catalog was read from.
    pub(crate) source_path: PathBuf,

    /// Target language as recorded by the format (PO `Language:`, ICU-JSON
    /// convention). `None` when the format does not record one.
    pub(crate) language: Option<String>,

    /// Original file bytes. Preserved so `apply` can splice changes without
    /// reserializing untouched regions.
    pub(crate) source_bytes: Vec<u8>,

    /// Structured units in document order.
    pub(crate) units: Vec<Unit>,

    /// Format-specific per-unit edit information. Opaque to callers; each
    /// format's `apply` impl pattern-matches on the enum directly.
    pub(crate) extract_state: ExtractState,
}

impl Catalog {
    /// Borrow the units in document order.
    pub fn units(&self) -> &[Unit] {
        &self.units
    }

    /// Mutably borrow the units. The frontend edits target strings in-place;
    /// the source bytes and extract state remain untouched until `apply` runs.
    pub fn units_mut(&mut self) -> &mut [Unit] {
        &mut self.units
    }

    /// Find a unit by id. Linear scan; catalogs are small.
    pub fn find_unit_mut(&mut self, id: &UnitId) -> Option<&mut Unit> {
        self.units.iter_mut().find(|u| u.id == *id)
    }

    /// Take ownership of the units.
    pub fn into_units(self) -> Vec<Unit> {
        self.units
    }

    /// Borrow the original source bytes (callers may diff the input against
    /// `apply` output to verify byte-stability in tests).
    pub fn source_bytes(&self) -> &[u8] {
        &self.source_bytes
    }

    /// Path the catalog was read from.
    pub fn source_path(&self) -> &PathBuf {
        &self.source_path
    }

    /// Target language declared in the catalog header (PO `Language:` field).
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// `nplurals=N` parsed from the format-specific header, when the format
    /// records one (PO `Plural-Forms:` field). `None` for ICU-JSON and for
    /// PO files whose header omits the field.
    ///
    /// Pair with [`crate::reconcile_plural_arity`] to decide the arity used
    /// for newly-written plural blocks.
    pub fn header_plural_arity(&self) -> Option<u32> {
        match &self.extract_state {
            ExtractState::Po(state) => state.header_nplurals,
            ExtractState::IcuJson(_) => None,
        }
    }
}

/// Opaque per-format edit information. Each format module owns one variant.
///
/// Wrapped in an enum (rather than `Box<dyn Any>`) so the compiler enforces
/// exhaustive handling when a new format is added.
#[derive(Debug, Clone)]
pub(crate) enum ExtractState {
    /// PO-format edit points. See `crate::po::ExtractStatePo`.
    Po(crate::po::ExtractStatePo),
    /// ICU-JSON format edit points. See `crate::icu_json::ExtractStateIcuJson`.
    IcuJson(crate::icu_json::ExtractStateIcuJson),
}
