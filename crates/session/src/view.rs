//! What a session hands to its callers: plain, serializable views.
//!
//! Text in views is in the catalog's **native** placeholder syntax (`%1` for
//! Qt, `%s` for gettext, `{name}` for ICU), which is what translators read
//! in the file. The ICU-normalized form stays internal.

use i18n_harness_core::{UnitContext, UnitState};
use serde::{Deserialize, Serialize};

/// Where a string stands, from a translator's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnitStatus {
    /// Needs a translation.
    ToDo,
    /// Has a translation nobody has confirmed yet (machine output, or a
    /// file's unfinished entry).
    Check,
    /// Translated and confirmed.
    Done,
}

/// How bad an [`Issue`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IssueSeverity {
    /// The translation would break the application (a placeholder or a plural
    /// form is missing). It is never written to the file.
    Error,
    /// Worth a look, but safe to save.
    Warning,
    /// A remark from the model about its own translation.
    Note,
}

/// One problem with a translation, in plain words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    /// How bad it is.
    pub severity: IssueSeverity,
    /// What is wrong, for a translator ("Placeholder %1 is missing").
    pub message: String,
}

/// String counts for one catalog.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusCounts {
    /// Strings that can be translated (strings removed from the application
    /// are not counted).
    pub total: usize,
    /// Strings to translate.
    pub to_do: usize,
    /// Strings to check.
    pub check: usize,
    /// Finished strings.
    pub done: usize,
    /// Strings with at least one [`IssueSeverity::Error`].
    pub errors: usize,
}

/// One open catalog file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogSummary {
    /// Absolute path; the key for every per-catalog call.
    pub path: String,
    /// File name for display.
    pub name: String,
    /// Path relative to the project folder, for display.
    pub relative_path: String,
    /// Format id (`"qt-ts"`).
    pub format: String,
    /// Format name for display (`"Qt Linguist"`).
    pub format_name: String,
    /// Target locale id (`"sk_SK"`).
    pub locale: String,
    /// Target language name (`"Slovak"`).
    pub language: String,
    /// Locale id of the text currently in the targets when it is another
    /// language (see the project's `pivot` setting).
    pub pivot: Option<String>,
    /// Language name of [`Self::pivot`].
    pub pivot_language: Option<String>,
    /// Counts by status.
    pub counts: StatusCounts,
    /// Whether there are edits not yet saved.
    pub dirty: bool,
}

/// One plural slot of a plural string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluralSlot {
    /// CLDR category name when known (`"one"`, `"few"`, `"other"`).
    pub category: Option<String>,
    /// A count that selects this form, when known (1, 2, 5).
    pub example: Option<u32>,
}

/// One translatable string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitView {
    /// Stable id within the catalog.
    pub id: String,
    /// Original text (the application's source language).
    pub source: String,
    /// The text the catalog held when it was opened, one entry per form, when
    /// it had any. For a catalog with a pivot language this is the text to
    /// translate *from*.
    pub existing: Option<Vec<String>>,
    /// Current translation, one entry per form (empty string: no text yet).
    pub target: Vec<String>,
    /// Plural slots, empty for a non-plural string.
    pub plural_slots: Vec<PluralSlot>,
    /// Translator status.
    pub status: UnitStatus,
    /// The catalog's own state for the string.
    pub state: UnitState,
    /// Problems with the current translation.
    pub issues: Vec<Issue>,
    /// Context and comments recorded in the file.
    pub context: UnitContext,
    /// False for strings the application no longer uses.
    pub editable: bool,
}

/// Which strings to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitFilter {
    /// Only these statuses (all when empty).
    #[serde(default)]
    pub statuses: Vec<UnitStatus>,
    /// Only strings with an [`IssueSeverity::Error`].
    #[serde(default)]
    pub errors_only: bool,
    /// Case-insensitive text search over source, existing text, translation,
    /// context and id.
    #[serde(default)]
    pub search: Option<String>,
}

/// A page of [`UnitView`]s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitPage {
    /// The units on this page, in document order.
    pub units: Vec<UnitView>,
    /// How many units match the filter in total.
    pub matching: usize,
}

/// What a save wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveReport {
    /// Catalogs written.
    pub written: Vec<String>,
    /// Strings left at their previous text because their translation has an
    /// error that would break the application.
    pub held_back: usize,
}
