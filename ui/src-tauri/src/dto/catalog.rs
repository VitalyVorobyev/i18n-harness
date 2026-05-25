//! Wire-format types for catalog open/save/edit commands.

use i18n_harness_core::Unit;
use serde::{Deserialize, Serialize};

/// Wire-format response from the `open_catalog` Tauri command.
#[derive(Debug, Serialize)]
pub struct CatalogResponse {
    /// Absolute path the catalog was read from; also the handle for
    /// follow-up commands.
    pub path: String,
    /// Number of units in the catalog (including non-writable ones).
    pub unit_count: usize,
    /// Target language as declared in the `.ts` root element. `None`
    /// if the catalog did not specify one — `translate_unit` will fail
    /// in that case until the locale is set explicitly.
    pub language: Option<String>,
    /// The units themselves, in document order. Serializes through
    /// [`Unit`]'s own serde derive — no flattening or projection here.
    pub units: Vec<Unit>,
}

/// Wire-format response from `save_catalog`.
#[derive(Debug, Serialize)]
pub struct SaveSummary {
    /// Absolute path the catalog was written to.
    pub path: String,
    /// Total units written (writable + preserved).
    pub unit_count: usize,
}

/// One edit to a unit's target, mirroring [`Target`] for singular and
/// plural cases. The frontend sends this when the user types into the
/// target editor; the command merges it into the in-memory unit.
///
/// [`Target`]: i18n_harness_core::Target
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TargetEdit {
    /// Replace the singular target's text. `None` empties it.
    Singular {
        /// New value, or `None` to clear the target.
        text: Option<String>,
    },
    /// Replace one form of a plural target. `form_index` is the CLDR
    /// position; `text` is the new value (`None` empties that form).
    Plural {
        /// CLDR-ordered position of the form to write (`0..arity`).
        form_index: u32,
        /// New value for that form, or `None` to clear it.
        text: Option<String>,
    },
}

/// Wire response from `save_all_dirty`. Carries both the catalogs that were
/// successfully written and (when the run stopped early) the path + reason of
/// the first failure. Using a single response shape — rather than
/// `Result<Vec<SaveSummary>, String>` — means the UI never loses the list of
/// already-saved catalogs when one apply mid-batch fails, so it can refresh
/// the right dirty pills without an extra IPC round trip.
#[derive(Debug, Serialize)]
pub struct SaveAllDirtyResponse {
    /// Catalogs written, in BTreeMap iteration order (absolute path order).
    /// Their `dirty` flag has been cleared in the in-memory store.
    pub saved: Vec<SaveSummary>,
    /// Absolute path of the catalog whose `apply` failed, if any. Catalogs
    /// after this entry in the iteration order were not attempted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_path: Option<String>,
    /// Human-readable failure reason, matched 1:1 with `failed_path`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_reason: Option<String>,
}
