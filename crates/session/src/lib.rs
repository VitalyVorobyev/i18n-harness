//! The translation session: the one orchestration layer between the
//! libraries (formats, gate, backends, project) and the people using them.
//!
//! The CLI and the desktop shell both drive a [`Session`]; neither
//! re-implements catalog handling, status rules, the merge of a model answer
//! into a catalog, or what is safe to save.
//!
//! - [`Session::open_folder`] opens a project folder (creating its settings
//!   file from the files found when there is none).
//! - [`Session::catalogs`] / [`Session::units`] / [`Session::rows`] list
//!   catalogs and strings as plain views, with each string's [`UnitStatus`],
//!   its problems in plain words, and whether other files of the same
//!   language share or translate it differently.
//! - [`Session::set_translation`] / [`Session::set_done`] record edits and
//!   decisions; [`Session::save`] writes, never writing a translation that
//!   would break the application.
//! - [`Session::plan`], [`Session::prepare`], [`translate_task`] and
//!   [`Session::apply`] run machine translation one string at a time;
//!   [`Session::translate`] chains them.
//! - [`list_models`] and [`backend`] connect to a model server.

#![forbid(unsafe_code)]

mod across;
mod cancel;
mod error;
mod glossary;
mod issues;
mod models;
mod session;
mod shared;
mod translate;
mod view;

pub use cancel::CancelToken;
pub use error::{Result, SessionError};
pub use glossary::{GlossaryEntry, GlossaryImport, TermUsage, glossary_table_entries};
pub use models::{DEFAULT_OLLAMA_HOST, ModelChoice, ModelSettings, backend, list_models};
pub use session::Session;
pub use translate::{
    Applied, InputText, Memo, MemoKey, Progress, Promotion, TranslateScope, TranslationTask,
    translate_task,
};
pub use view::{
    CatalogSummary, CheckSettings, FillReport, Issue, IssueSeverity, PluralSlot, SaveReport, Show,
    SiblingView, StatusCounts, UnitFilter, UnitPage, UnitRow, UnitStatus, UnitView,
};

pub use i18n_harness_backend::discovery::{PromptStyle, ServerKind};
