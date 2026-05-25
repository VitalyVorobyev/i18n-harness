//! Miscellaneous metadata commands (version, locales, filesystem helpers).

use crate::dto::GlossaryLoadResponse;
use crate::dto::LocaleInfo;
use i18n_harness_locales::Locale;

/// Return the package version baked at compile time.
///
/// Smoke-test command: confirms the IPC bridge is wired correctly
/// before any catalog has been opened.
#[tauri::command]
pub(crate) fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// List the workspace locales (`en`, `de_DE`, `es_ES`, `zh_Hans`).
/// The UI uses this for column headers in the glossary editor and to
/// render the locale badge on the catalog view.
#[tauri::command]
pub(crate) fn list_locales() -> Vec<LocaleInfo> {
    Locale::all()
        .map(|l| LocaleInfo {
            id: l.id.to_string(),
            register: match l.register {
                i18n_harness_locales::Register::Formal => "formal",
                i18n_harness_locales::Register::Informal => "informal",
                i18n_harness_locales::Register::Neutral => "neutral",
            },
            script: format!("{:?}", l.script),
            plural_arity: l.plural_arity(),
        })
        .collect()
}

// Suppress unused import warning — GlossaryLoadResponse is imported for
// consistency but only used by other modules; the import keeps the dto glob
// from being needed here.
const _: () = {
    let _ = std::mem::size_of::<GlossaryLoadResponse>();
};

/// Write `content` (UTF-8 text) to `path`, creating or overwriting the file.
///
/// This is a pure filesystem thin-wrapper used by the frontend "Save .md"
/// flow in ProofreadView — no business logic here.
#[tauri::command]
pub(crate) fn write_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content.as_bytes()).map_err(|e| format!("write failed: {e}"))
}
