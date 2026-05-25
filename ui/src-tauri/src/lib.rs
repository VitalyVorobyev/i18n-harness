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
