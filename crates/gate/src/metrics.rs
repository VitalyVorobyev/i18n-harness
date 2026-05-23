//! Metrics events — append-only JSONL writer for per-`(backend, locale)`
//! quality telemetry.
//!
//! Each line is one [`Event`] in JSON form. The file is opened append-only
//! and every event is written with a single `write_all` call so that:
//!
//! - Tail-reading (`tail -f .i18n-harness/metrics.jsonl`) shows lines as
//!   they land.
//! - A crash mid-write leaves the file in a consistent state — either the
//!   line is fully there or not there at all (subject to filesystem
//!   atomicity guarantees for writes ≤ `PIPE_BUF`; the typical line size
//!   is a few hundred bytes, well under the 4 KiB POSIX guarantee).
//!
//! # Timestamp dependency
//!
//! We deliberately do **not** depend on `chrono` or `time`. The format we
//! need is RFC3339 with microsecond precision; that is ~30 lines of code
//! around [`std::time::SystemTime`]. Adding `time` for one writer would
//! double the dep graph of `crates/gate`.

#![allow(clippy::module_name_repetitions)]

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use i18n_harness_core::{Flag, FlagSeverity, UnitId};
use serde::{Deserialize, Serialize};

use crate::report::{Finding, GateReport};

/// Top-level event types emitted to `metrics.jsonl`.
///
/// The set is closed at the type level; M1 produces only `GateReject` and
/// `SoftWarning`. M2 adds `Retry`; M3 adds `HumanEdit`. Older readers that
/// do not recognize a variant will deserialize-fail on those lines, which
/// is exactly the behavior we want — silent drop would erode the metric.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum EventKind {
    /// At least one hard finding fired on this unit.
    GateReject,
    /// At least one soft finding fired on this unit.
    SoftWarning,
    /// A human edited the proposed translation. Reserved for M3+.
    HumanEdit,
    /// The backend was asked to retry on this unit (e.g. ICU parse fail).
    /// Reserved for M2+.
    Retry,
}

/// One metric event.
///
/// `rule` is the lower-case kebab name of the [`Flag`] that fired (e.g.
/// `"placeholder-mismatch"`). `detail` is the serialized
/// [`crate::FindingDetail`] payload, identical to what a [`GateReport`]
/// carries; consumers can re-deserialize it if they want structured access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// On-disk schema version (separate from `i18n-harness-core`'s line
    /// schema; metrics are append-only and have their own evolution).
    pub schema: u32,
    /// RFC3339 timestamp with microsecond precision (`Z` suffix; UTC).
    pub ts: String,
    /// Backend name (e.g. `"manual"`, `"ollama"`).
    pub backend: String,
    /// Locale id (e.g. `"de_DE"`).
    pub locale: String,
    /// Event kind.
    #[serde(flatten)]
    pub kind: EventKind,
    /// Unit the event pertains to.
    pub unit_id: UnitId,
    /// Lower-case kebab name of the flag that fired (or empty for events
    /// without a flag, e.g. M3 human-edit).
    pub rule: String,
    /// Per-rule payload (serialized [`crate::FindingDetail`]).
    pub detail: serde_json::Value,
}

/// Current metrics schema version. Bump on every breaking change.
pub const METRICS_SCHEMA_VERSION: u32 = 1;

/// Trait implemented by anything that accepts metric lines.
///
/// The file-backed implementation is [`FileSink`]; tests use [`MemorySink`].
pub trait MetricsSink {
    /// Append one line. Line must already include the trailing newline.
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error verbatim; callers typically log it
    /// and continue (metrics are best-effort).
    fn write_line(&self, line: &[u8]) -> io::Result<()>;
}

/// File-backed sink. Opens `path` append-only on first write; lazy so a
/// caller that never produces events does not touch the disk.
#[derive(Debug)]
pub struct FileSink {
    path: PathBuf,
    file: Mutex<Option<File>>,
}

impl FileSink {
    /// Construct a sink that will append to `path`. The path's parent
    /// directory must exist; the file itself is created on first write.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            file: Mutex::new(None),
        }
    }

    /// Path the sink writes to.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl MetricsSink for FileSink {
    fn write_line(&self, line: &[u8]) -> io::Result<()> {
        let mut guard = self
            .file
            .lock()
            .map_err(|_| io::Error::other("metrics sink mutex poisoned"))?;
        if guard.is_none() {
            *guard = Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.path)?,
            );
        }
        let file = guard.as_mut().expect("just inserted");
        file.write_all(line)
    }
}

/// In-memory sink for tests. Lines are buffered in a `Mutex<Vec<Vec<u8>>>`.
#[derive(Debug, Default)]
pub struct MemorySink {
    lines: Mutex<Vec<Vec<u8>>>,
}

impl MemorySink {
    /// Construct an empty sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the lines written so far. Each entry includes the trailing
    /// newline.
    pub fn lines(&self) -> Vec<Vec<u8>> {
        self.lines.lock().expect("sink mutex").clone()
    }

    /// Snapshot the lines as parsed events. Convenience for tests.
    pub fn events(&self) -> Vec<Event> {
        self.lines()
            .into_iter()
            .map(|raw| serde_json::from_slice(&raw).expect("metrics sink stores valid JSON"))
            .collect()
    }
}

impl MetricsSink for MemorySink {
    fn write_line(&self, line: &[u8]) -> io::Result<()> {
        let mut guard = self
            .lines
            .lock()
            .map_err(|_| io::Error::other("memory sink mutex poisoned"))?;
        guard.push(line.to_vec());
        Ok(())
    }
}

/// Wrapper that turns a [`MetricsSink`] into a structured writer.
///
/// One `MetricsWriter` corresponds to one `(backend, locale)` pair; callers
/// that translate into multiple locales create one writer per locale.
#[derive(Debug)]
pub struct MetricsWriter<S: MetricsSink> {
    backend: String,
    locale: String,
    sink: S,
}

impl<S: MetricsSink> MetricsWriter<S> {
    /// Construct a writer pinned to a `(backend, locale)` pair.
    pub fn new(backend: impl Into<String>, locale: impl Into<String>, sink: S) -> Self {
        Self {
            backend: backend.into(),
            locale: locale.into(),
            sink,
        }
    }

    /// Borrow the underlying sink (handy for tests).
    pub fn sink(&self) -> &S {
        &self.sink
    }

    /// Write one event line per finding in `report`. Hard findings emit
    /// `gate-reject`; soft findings emit `soft-warning`; semantic
    /// (model-supplied) findings are not written by this method — they are
    /// produced by the backend, not the gate.
    ///
    /// # Errors
    ///
    /// Returns the first underlying I/O error. Earlier-written lines are
    /// committed (the writer is line-by-line, not transactional). Callers
    /// log and continue; metrics are best-effort.
    pub fn record_report(&self, report: &GateReport) -> io::Result<()> {
        if report.findings.is_empty() {
            return Ok(());
        }
        let ts = rfc3339_now();
        for finding in &report.findings {
            let kind = match finding.flag.severity() {
                FlagSeverity::Hard => EventKind::GateReject,
                FlagSeverity::Soft => EventKind::SoftWarning,
                FlagSeverity::Semantic => continue,
            };
            self.write_finding(&ts, kind, &report.unit_id, finding)?;
        }
        Ok(())
    }

    fn write_finding(
        &self,
        ts: &str,
        kind: EventKind,
        unit_id: &UnitId,
        finding: &Finding,
    ) -> io::Result<()> {
        let detail =
            serde_json::to_value(&finding.detail).expect("FindingDetail serializes to JSON");
        let event = Event {
            schema: METRICS_SCHEMA_VERSION,
            ts: ts.to_owned(),
            backend: self.backend.clone(),
            locale: self.locale.clone(),
            kind,
            unit_id: unit_id.clone(),
            rule: flag_rule_name(finding.flag).to_owned(),
            detail,
        };
        let mut line = serde_json::to_vec(&event).expect("Event serializes");
        line.push(b'\n');
        self.sink.write_line(&line)
    }
}

/// Kebab-case rule name for a [`Flag`].
///
/// Matches the `serde(rename_all = "kebab-case")` rendering of the
/// `Flag` enum. Kept as a small private function rather than re-deriving
/// because we want a `&'static str` here (no allocation).
fn flag_rule_name(flag: Flag) -> &'static str {
    match flag {
        Flag::PlaceholderMismatch => "placeholder-mismatch",
        Flag::PluralArityMismatch => "plural-arity-mismatch",
        Flag::IcuParseError => "icu-parse-error",
        Flag::EmptyTargetWhenFinished => "empty-target-when-finished",
        Flag::AccelMismatch => "accel-mismatch",
        Flag::LengthWarn => "length-warn",
        Flag::CjkPunctuationTolerated => "cjk-punctuation-tolerated",
        Flag::PlaceholderAgreementRisk => "placeholder-agreement-risk",
        Flag::AmbiguousSource => "ambiguous-source",
        Flag::Idiom => "idiom",
        Flag::InsufficientContext => "insufficient-context",
        Flag::LowConfidence => "low-confidence",
    }
}

/// Format the current time as RFC3339 with microsecond precision.
///
/// Returns e.g. `"2026-05-23T14:33:22.123456Z"`. Always UTC, always `Z`
/// suffix; no offset support because metrics are local-process telemetry
/// and a fixed UTC keeps comparison trivial.
fn rfc3339_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = now.as_secs();
    let micros = now.subsec_micros();
    let (y, mo, d, h, mi, s) = civil_from_unix(total_secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{micros:06}Z")
}

/// Convert a Unix timestamp (seconds since 1970-01-01 UTC) into civil
/// year/month/day/hour/minute/second.
///
/// Uses the date algorithm from Howard Hinnant's "chrono-Compatible
/// Low-Level Date Algorithms" — branchless and proleptic Gregorian. We
/// implement it inline rather than depend on `time` for the same reason
/// noted in the module docs.
fn civil_from_unix(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let z = (secs / 86_400) as i64;
    let secs_of_day = (secs % 86_400) as u32;
    let (y, mo, d) = civil_from_days(z);
    let h = secs_of_day / 3600;
    let mi = (secs_of_day % 3600) / 60;
    let s = secs_of_day % 60;
    (y, mo, d, h, mi, s)
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    // Days since 1970-01-01 to days since 0000-03-01 (Howard Hinnant's epoch).
    let z = z + 719_468;
    let era = if z >= 0 {
        z / 146_097
    } else {
        (z - 146_096) / 146_097
    };
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y } as i32;
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use i18n_harness_core::Flag;

    use crate::report::{
        AccelDetail, Finding, FindingDetail, GateReport, PlaceholderMismatchDetail,
    };

    fn sample_report() -> GateReport {
        let findings = vec![
            Finding {
                flag: Flag::PlaceholderMismatch,
                detail: FindingDetail::PlaceholderMismatch(PlaceholderMismatchDetail {
                    slot: 0,
                    missing: vec!["{0}".into()],
                    extra: vec![],
                }),
            },
            Finding {
                flag: Flag::AccelMismatch,
                detail: FindingDetail::AccelMismatch(AccelDetail {
                    source_count: 1,
                    target_count: 0,
                }),
            },
        ];
        GateReport::from_findings("ctx::x".into(), findings)
    }

    #[test]
    fn writes_one_line_per_finding() {
        let sink = MemorySink::new();
        let w = MetricsWriter::new("manual", "de_DE", sink);
        w.record_report(&sample_report()).expect("write");
        let events = w.sink().events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, EventKind::GateReject);
        assert_eq!(events[1].kind, EventKind::SoftWarning);
        assert_eq!(events[0].rule, "placeholder-mismatch");
        assert_eq!(events[1].rule, "accel-mismatch");
        assert_eq!(events[0].backend, "manual");
        assert_eq!(events[0].locale, "de_DE");
    }

    #[test]
    fn lines_have_trailing_newline() {
        let sink = MemorySink::new();
        let w = MetricsWriter::new("manual", "de_DE", sink);
        w.record_report(&sample_report()).unwrap();
        for line in w.sink().lines() {
            assert!(line.ends_with(b"\n"));
            // Exactly one newline at the very end.
            assert_eq!(line.iter().filter(|&&b| b == b'\n').count(), 1);
        }
    }

    #[test]
    fn empty_report_writes_nothing() {
        let sink = MemorySink::new();
        let w = MetricsWriter::new("manual", "de_DE", sink);
        w.record_report(&GateReport::from_findings("x".into(), vec![]))
            .unwrap();
        assert!(w.sink().lines().is_empty());
    }

    #[test]
    fn timestamp_is_rfc3339_microseconds() {
        let ts = rfc3339_now();
        // Pattern: YYYY-MM-DDTHH:MM:SS.uuuuuuZ
        assert_eq!(ts.len(), "2026-05-23T14:33:22.123456Z".len());
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.as_bytes()[4], b'-');
        assert_eq!(ts.as_bytes()[7], b'-');
        assert_eq!(ts.as_bytes()[10], b'T');
        assert_eq!(ts.as_bytes()[13], b':');
        assert_eq!(ts.as_bytes()[16], b':');
        assert_eq!(ts.as_bytes()[19], b'.');
    }

    #[test]
    fn civil_from_days_known_epoch() {
        // 1970-01-01 → days = 0
        let (y, m, d) = civil_from_days(0);
        assert_eq!((y, m, d), (1970, 1, 1));
        // 2000-01-01 → 30 years and 7 leap days = 10957
        let (y, m, d) = civil_from_days(10_957);
        assert_eq!((y, m, d), (2000, 1, 1));
        // 2026-05-23 → 20596
        let (y, m, d) = civil_from_days(20_596);
        assert_eq!((y, m, d), (2026, 5, 23));
    }

    #[test]
    fn file_sink_appends() {
        let dir = std::env::temp_dir().join(format!("i18n-harness-metrics-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("metrics.jsonl");
        if path.exists() {
            std::fs::remove_file(&path).ok();
        }
        let sink = FileSink::new(&path);
        let w = MetricsWriter::new("manual", "de_DE", sink);
        w.record_report(&sample_report()).expect("write");
        w.record_report(&sample_report()).expect("write twice");
        let contents = std::fs::read_to_string(&path).expect("read");
        assert_eq!(
            contents.lines().count(),
            4,
            "expected 4 lines (2 reports × 2 findings)"
        );
        std::fs::remove_file(&path).ok();
    }
}
