//! Tauri desktop shell for the i18n-harness.
//!
//! `lib.rs` is the entry point and the `AppState` re-export. The
//! Tauri command surface lives under `commands/` (organized by
//! feature bucket), the IPC wire types live under `dto/`, and the
//! Tauri-free business logic lives under `services/`. The shell
//! itself is a thin wrapper; translation, gate, and adapter logic
//! remain in the workspace's pure-Rust crates and are testable as
//! a headless library.

mod backing;
mod cancellation;
pub mod commands;
mod dto;
mod error;
mod jobs;
mod services;
mod state;

pub use dto::*;
pub use state::AppState;
pub(crate) use state::OpenCatalogEntry;

/// Entry point invoked from `main.rs` (and from the mobile entry point
/// macro when the crate is built for iOS/Android).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default());

    let builder = commands::build_handler(builder);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
