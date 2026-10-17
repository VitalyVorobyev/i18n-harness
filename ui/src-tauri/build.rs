//! Tauri build script: regenerates capability schemas and embeds
//! `tauri.conf.json` into the binary at compile time.
//!
//! With the `custom-protocol` feature the frontend bundle in `ui/dist` is
//! embedded into the binary, so it must exist and be current. When it is
//! missing or older than the frontend sources, this script rebuilds it with
//! `bun run build:web`. Set `I18N_HARNESS_SKIP_FRONTEND_BUILD=1` to skip the
//! rebuild (CI jobs that only lint Rust); a missing bundle is still an error.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

const FRONTEND_INPUTS: &[&str] = &[
    "src",
    "index.html",
    "package.json",
    "bun.lock",
    "vite.config.ts",
    "tsconfig.json",
];

fn main() {
    if std::env::var_os("CARGO_FEATURE_CUSTOM_PROTOCOL").is_some() {
        ensure_frontend_bundle();
    }
    tauri_build::build()
}

fn ensure_frontend_bundle() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let ui_dir = manifest_dir
        .parent()
        .expect("src-tauri lives inside ui/")
        .to_path_buf();
    let bundle_index = ui_dir.join("dist").join("index.html");

    for input in FRONTEND_INPUTS {
        println!("cargo:rerun-if-changed={}", ui_dir.join(input).display());
    }
    println!("cargo:rerun-if-changed={}", bundle_index.display());
    println!("cargo:rerun-if-env-changed=I18N_HARNESS_SKIP_FRONTEND_BUILD");

    let bundle_time = modified(&bundle_index);
    let sources_time = FRONTEND_INPUTS
        .iter()
        .filter_map(|input| newest_mtime(&ui_dir.join(input)))
        .max();
    let stale = match (bundle_time, sources_time) {
        (None, _) => true,
        (Some(bundle), Some(sources)) => sources > bundle,
        (Some(_), None) => false,
    };
    if !stale {
        return;
    }

    if std::env::var_os("I18N_HARNESS_SKIP_FRONTEND_BUILD").is_some() {
        if bundle_time.is_none() {
            panic!(
                "frontend bundle {} is missing and I18N_HARNESS_SKIP_FRONTEND_BUILD is set; \
                 run `bun install && bun run build` in {}",
                bundle_index.display(),
                ui_dir.display()
            );
        }
        println!("cargo:warning=frontend bundle in ui/dist is older than ui/src (rebuild skipped)");
        return;
    }

    if !ui_dir.join("node_modules").is_dir() {
        panic!(
            "frontend dependencies are not installed; run `bun install` in {} and rebuild",
            ui_dir.display()
        );
    }
    let status = Command::new("bun")
        .args(["run", "build:web"])
        .current_dir(&ui_dir)
        .status()
        .unwrap_or_else(|err| {
            panic!(
                "could not run `bun run build:web` in {} ({err}); install bun or build the frontend \
                 manually with `bun run build`",
                ui_dir.display()
            )
        });
    if !status.success() {
        panic!("`bun run build:web` failed in {}", ui_dir.display());
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn newest_mtime(path: &Path) -> Option<SystemTime> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_dir() {
        return meta.modified().ok();
    }
    std::fs::read_dir(path)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| newest_mtime(&entry.path()))
        .max()
}
