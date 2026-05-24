//! Tauri build script: regenerates capability schemas and embeds
//! `tauri.conf.json` into the binary at compile time.

fn main() {
    tauri_build::build()
}
