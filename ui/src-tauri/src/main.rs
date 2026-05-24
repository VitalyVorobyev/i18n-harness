//! Tauri desktop binary entry point. The actual setup lives in the
//! library crate (`i18n_harness_ui_lib::run`) so tests can drive it.

// Prevent a second console window from popping up on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    i18n_harness_ui_lib::run()
}
