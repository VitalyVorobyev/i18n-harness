//! IPC error helpers for the Tauri command layer.
//!
//! Commands return `Result<T, String>` because that is what Tauri
//! serializes to the JS layer. This module owns the small set of
//! string-error mappers used across every command — lock-poisoned,
//! "no catalog open", and similar.

use std::sync::{MutexGuard, PoisonError};

#[allow(dead_code)] // used in future phases when commands move to their own modules
pub(crate) type IpcResult<T> = Result<T, String>;

/// Generic poisoned-lock mapper. Use as
/// `.map_err(lock_poisoned("project"))`. The label is interpolated
/// into the error string so every existing message stays byte-stable
/// (current strings: `"catalog state lock poisoned"`,
/// `"project state lock poisoned"`, …).
pub(crate) fn lock_poisoned<T>(
    label: &'static str,
) -> impl FnOnce(PoisonError<MutexGuard<'_, T>>) -> String {
    move |_| format!("{label} state lock poisoned")
}

pub(crate) fn no_catalog() -> String {
    "no catalog open".into()
}

pub(crate) fn no_project() -> String {
    "no project open".into()
}

pub(crate) fn no_catalog_in_project() -> String {
    "catalog not open in project".into()
}
