//! Desktop shell: Tauri commands over [`i18n_harness_session::Session`].
//!
//! Every command is a thin wrapper; catalog handling, status rules, the
//! merge of model answers and the save guard all live in the session crate.

mod commands;
mod error;
mod state;
mod translation;

pub use state::AppState;

/// Start the desktop application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_languages,
            commands::open_folder,
            commands::close_folder,
            commands::list_catalogs,
            commands::list_units,
            commands::set_translation,
            commands::set_done,
            commands::save,
            commands::set_pivot,
            commands::get_glossary,
            commands::set_glossary,
            commands::read_glossary_table,
            commands::plan_size,
            commands::get_model_settings,
            commands::set_model_settings,
            commands::list_models,
            translation::translate_one,
            translation::start_translation,
            translation::cancel_translation,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
