//! Wire-format types for the metrics view commands.

use serde::{Deserialize, Serialize};

/// One row from `.i18n-harness/metrics.jsonl`, re-shaped for the wire.
///
/// We don't depend on `i18n_harness_gate::Event` directly — its serde
/// flattened tag would force the UI to deal with two shapes. Here we
/// surface a flat record the TypeScript layer can render without a
/// custom serde dance.
#[derive(Debug, Serialize, Deserialize)]
pub struct MetricEvent {
    /// On-disk schema version (canonical metrics schema, not the
    /// in-memory `Unit` schema).
    pub schema: u32,
    /// RFC3339 timestamp (`…Z`, UTC, microsecond precision).
    pub ts: String,
    /// Backend that produced the unit (`"manual"`, `"ollama"`, …).
    pub backend: String,
    /// Target locale (`"de_DE"`, …).
    pub locale: String,
    /// Event kind discriminator (`"gate-reject"`, `"soft-warning"`,
    /// `"human-edit"`, `"retry"`).
    pub event: String,
    /// Unit id the event pertains to.
    pub unit_id: String,
    /// Lower-case kebab flag name (`"length-warn"`, …) or empty for
    /// non-flag events.
    pub rule: String,
    /// Per-rule detail payload. We pass it through opaquely; the UI
    /// renders rule-specific summaries.
    pub detail: serde_json::Value,
}

/// Wire response from `load_metrics`.
#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    /// Absolute path read.
    pub path: String,
    /// Successfully-parsed events, in file order.
    pub events: Vec<MetricEvent>,
    /// Lines that could not be parsed as JSON or were missing fields.
    /// Surfaced to the UI so a corrupted run is visible, not silent.
    pub error_count: usize,
    /// Total non-blank lines read (`events.len() + error_count`).
    pub line_count: usize,
}
