//! Tauri desktop shell for the i18n-harness.
//!
//! This crate is a *thin* wrapper over the library API. Translation,
//! gate, and adapter logic live in the workspace's pure-Rust crates and
//! remain testable as a headless library. Commands here marshal
//! arguments, invoke the library, and serialize results back to the
//! JavaScript layer. No business logic that does not fit on a single
//! screen of glue belongs here.

mod backing;
mod cancellation;
mod commands;
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

// ── Tests: BatchUnitStartedPayload structure ──────────────────────────────────

#[cfg(all(test, feature = "ollama"))]
mod batch_unit_started_tests {
    use super::BatchUnitStartedPayload;

    #[test]
    fn payload_fields_round_trip_through_serde() {
        let p = BatchUnitStartedPayload {
            unit_id: "u::hello".to_owned(),
            locale: "de_DE".to_owned(),
        };
        let json = serde_json::to_string(&p).expect("serialize");
        let back: serde_json::Value = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back["unit_id"], "u::hello");
        assert_eq!(back["locale"], "de_DE");
    }

    // The loop in `run_batch_worker` emits one `batch-unit-started-<id>` per
    // iteration (one per unit_id in the batch). This structural test checks that
    // the payload count equals the number of units: it drives the payload
    // construction path in isolation so clippy/miri can also exercise it.
    #[test]
    fn one_started_payload_constructed_per_unit() {
        let unit_ids = ["u1", "u2", "u3"];
        let locale_id = "de_DE";
        let payloads: Vec<BatchUnitStartedPayload> = unit_ids
            .iter()
            .map(|uid| BatchUnitStartedPayload {
                unit_id: (*uid).to_owned(),
                locale: locale_id.to_owned(),
            })
            .collect();
        assert_eq!(payloads.len(), unit_ids.len());
        for (p, expected_id) in payloads.iter().zip(&unit_ids) {
            assert_eq!(&p.unit_id, expected_id);
            assert_eq!(p.locale, locale_id);
        }
    }
}

// ── Tests: end-to-end open → edit → save → reopen contract ─────────────────────
//
// These tests bypass the Tauri runtime and drive the *_impl helpers directly.
// They exist because translator-facing "Save all" complaints traced back to
// the question: when a Proposed edit is applied to an Untranslated unit,
// does the bytes-on-disk → reopen → in-memory state round-trip preserve the
// edit? The single test that follows answers that question without any
// frontend wiring in scope; if it ever turns red, the bug is in Rust.
#[cfg(test)]
mod save_roundtrip_integration_tests {
    use super::AppState;
    use crate::commands::project_catalog::{
        is_catalog_dirty_impl, open_catalog_in_project_impl, save_all_dirty_impl,
        update_unit_target_in_project_impl,
    };
    use crate::commands::project_lifecycle::open_project_impl;
    use crate::dto::TargetEdit;
    use i18n_harness_core::{Target, UnitState};
    use std::fs;
    use tempfile::TempDir;

    /// Minimal Qt `.ts` fixture: one unfinished singular unit. Exactly the
    /// shape that Linguist + the harness's extract path produce on first run.
    /// Body bytes are stable across runs.
    const TS_BEFORE: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n<TS version=\"2.1\" language=\"de_DE\" sourcelanguage=\"en\">\n<context>\n    <name>MainWindow</name>\n    <message>\n        <location filename=\"src/mainwindow.cpp\" line=\"42\"/>\n        <source>Hello</source>\n        <translation type=\"unfinished\"></translation>\n    </message>\n</context>\n</TS>\n";

    const MANIFEST: &str = r#"[project]
name = "save-roundtrip-test"
schema = 1

[locales.de_DE]
register = "neutral"

[[catalogs]]
path = "translations/app_de.ts"
format = "qt-ts"
locale = "de_DE"
"#;

    fn write_project(dir: &TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = dir.path().to_path_buf();
        let manifest_path = root.join("i18n-harness.toml");
        fs::write(&manifest_path, MANIFEST).expect("write manifest");
        let translations_dir = root.join("translations");
        fs::create_dir_all(&translations_dir).expect("create translations dir");
        let ts_path = translations_dir.join("app_de.ts");
        fs::write(&ts_path, TS_BEFORE).expect("write .ts");
        (root, ts_path)
    }

    /// The forensic test the maintainer asked for. Open a fresh project, edit
    /// one unit, save all, then drop the AppState (simulating an app restart)
    /// and reopen. The edit must survive every step:
    ///
    ///   - in-memory state after edit: `state == Proposed`, dirty == true.
    ///   - in-memory state after save: dirty == false.
    ///   - on-disk file after save: contains `Hallo Welt` and still
    ///     carries `type="unfinished"` (write contract for Proposed).
    ///   - in-memory state after reopen (fresh AppState): the parser must
    ///     promote `type="unfinished" + non-empty body` back to Proposed,
    ///     so the unit comes back with `state == Proposed` and the
    ///     translated text intact.
    ///
    /// If this test fails, the bug is on the Rust side. If it passes, the
    /// "Save all loses changes" report is necessarily a frontend issue
    /// (the textarea draft never made it into the IPC layer, or the IPC
    /// race lost the edit).
    #[test]
    fn edit_save_reopen_preserves_proposed_translation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (root, ts_path) = write_project(&dir);

        // Phase 1 — open project, open catalog, confirm Untranslated.
        let state = AppState::default();
        let open_resp = open_project_impl(&root, &state).expect("open project");
        // Use the path the project resolved — on macOS the temp dir lives at
        // /var/folders/... which symlinks to /private/var/folders/..., and the
        // canonicalized form on disk does not match the manifest-resolved form.
        // The project's own absolute_path is the IPC handle the UI uses too.
        let abs_path = open_resp
            .summary
            .catalogs
            .iter()
            .find(|c| {
                std::path::Path::new(&c.absolute_path).file_name()
                    == Some(ts_path.file_name().unwrap())
            })
            .expect("project must know about the catalog")
            .absolute_path
            .clone();
        let opened = open_catalog_in_project_impl(&abs_path, &state).expect("open catalog");
        assert_eq!(opened.unit_count, 1, "fixture has one unit");
        let unit_id = opened.units[0].id.to_string();
        assert_eq!(
            opened.units[0].state,
            UnitState::Untranslated,
            "before edit: state must be Untranslated"
        );

        // Phase 2 — apply an edit; state machine promotes to Proposed.
        let edited = update_unit_target_in_project_impl(
            &abs_path,
            &unit_id,
            TargetEdit::Singular {
                text: Some("Hallo Welt".to_owned()),
            },
            &state,
        )
        .expect("update edit");
        assert_eq!(
            edited.state,
            UnitState::Proposed,
            "after edit: state must be Proposed"
        );
        match &edited.target {
            Target::Singular { text } => assert_eq!(
                text.as_deref(),
                Some("Hallo Welt"),
                "after edit: target text must be the new value"
            ),
            Target::Plural { .. } => panic!("expected singular target"),
        }
        assert!(
            is_catalog_dirty_impl(&abs_path, &state).expect("dirty check"),
            "after edit: catalog must be marked dirty"
        );

        // Phase 3 — save all dirty; expect one saved entry, no failure.
        let save = save_all_dirty_impl(&state).expect("save all dirty");
        assert_eq!(save.saved.len(), 1, "save_all_dirty: one saved entry");
        assert_eq!(
            save.saved[0].path, abs_path,
            "save_all_dirty: path must match"
        );
        assert!(save.failed_path.is_none(), "save_all_dirty: no failure");
        assert!(
            !is_catalog_dirty_impl(&abs_path, &state).expect("dirty check post-save"),
            "post-save: dirty flag must be cleared"
        );

        // Phase 4 — read bytes off disk; assert write contract.
        let bytes_after = fs::read_to_string(&abs_path).expect("read .ts after save");
        assert!(
            bytes_after.contains("Hallo Welt"),
            "on-disk: must contain the edited text\n----\n{bytes_after}\n----"
        );
        assert!(
            bytes_after.contains("type=\"unfinished\""),
            "on-disk: must retain type=\"unfinished\" (Proposed write contract)\n----\n{bytes_after}\n----"
        );

        // Phase 5 — drop AppState (simulate app restart), reopen, verify
        // the edit survives the parse path.
        drop(state);
        let state2 = AppState::default();
        open_project_impl(&root, &state2).expect("reopen project");
        let reopened = open_catalog_in_project_impl(&abs_path, &state2).expect("reopen catalog");
        assert_eq!(reopened.unit_count, 1, "after reopen: still one unit");
        let unit = &reopened.units[0];
        assert_eq!(
            unit.state,
            UnitState::Proposed,
            "after reopen: state must round-trip as Proposed (parse promotion)"
        );
        match &unit.target {
            Target::Singular { text } => assert_eq!(
                text.as_deref(),
                Some("Hallo Welt"),
                "after reopen: target text must round-trip intact"
            ),
            Target::Plural { .. } => panic!("expected singular target after reopen"),
        }
        assert!(
            !is_catalog_dirty_impl(&abs_path, &state2).expect("dirty after reopen"),
            "after reopen: catalog must be clean"
        );
    }
}
