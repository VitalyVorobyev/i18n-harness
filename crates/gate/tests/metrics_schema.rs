//! Stability snapshot for `.i18n-harness/metrics.jsonl` on-wire shape.
//!
//! The metrics file is a public on-disk contract: downstream readers
//! (dashboards, the M3 UI's metrics view, future exporters) parse it.
//! This test pins the JSON shape of every event kind. A breaking change
//! must bump `METRICS_SCHEMA_VERSION` and update the assertions below — the
//! point is that the change is intentional and reviewed, not silent.
//!
//! We do not snapshot bytes verbatim because `ts` is wall-clock and the
//! key order inside `detail` depends on `serde_json`'s map ordering. We
//! assert structural properties instead: required keys, value types, and
//! per-rule detail shapes.

use i18n_harness_core::{Flag, UnitId};
use i18n_harness_gate::metrics::{METRICS_SCHEMA_VERSION, MemorySink, MetricsWriter};
use i18n_harness_gate::{
    AccelDetail, CjkPunctuationDetail, EmptyTargetDetail, Finding, FindingDetail, GateReport,
    IcuParseDetail, LengthWarnDetail, PlaceholderAgreementDetail, PlaceholderMismatchDetail,
    PluralArityMismatchDetail,
};
use serde_json::Value;

/// Build a `GateReport` carrying one finding of every rule the gate
/// produces. Semantic flags (`AmbiguousSource`, `Idiom`, …) are model-
/// supplied; the metrics writer skips them, so they are absent here.
fn report_with_every_rule() -> GateReport {
    let findings = vec![
        Finding {
            flag: Flag::PlaceholderMismatch,
            detail: FindingDetail::PlaceholderMismatch(PlaceholderMismatchDetail {
                slot: 0,
                missing: vec!["{0}".into()],
                extra: vec!["{name}".into()],
            }),
        },
        Finding {
            flag: Flag::PluralArityMismatch,
            detail: FindingDetail::PluralArityMismatch(PluralArityMismatchDetail {
                expected: 2,
                found: 1,
                wrong_variant: false,
            }),
        },
        Finding {
            flag: Flag::IcuParseError,
            detail: FindingDetail::IcuParseError(IcuParseDetail {
                slot: 0,
                byte_offset: 7,
                message: "unexpected end of input inside placeholder".into(),
            }),
        },
        Finding {
            flag: Flag::EmptyTargetWhenFinished,
            detail: FindingDetail::EmptyTargetWhenFinished(EmptyTargetDetail { slot: 0 }),
        },
        Finding {
            flag: Flag::AccelMismatch,
            detail: FindingDetail::AccelMismatch(AccelDetail {
                source_count: 1,
                target_count: 0,
            }),
        },
        Finding {
            flag: Flag::LengthWarn,
            detail: FindingDetail::LengthWarn(LengthWarnDetail {
                source_chars: 8,
                target_chars: 14,
                threshold: 1.4,
                ratio: 1.75,
            }),
        },
        Finding {
            flag: Flag::CjkPunctuationTolerated,
            detail: FindingDetail::CjkPunctuationTolerated(CjkPunctuationDetail {
                characters: vec!['，', '。'],
            }),
        },
        Finding {
            flag: Flag::PlaceholderAgreementRisk,
            detail: FindingDetail::PlaceholderAgreementRisk(PlaceholderAgreementDetail {
                placeholder: "{0}".into(),
                determiner: "der".into(),
            }),
        },
    ];
    GateReport::from_findings(UnitId::from("ctx::id"), findings)
}

/// Top-level keys every event line must carry. If you change this list,
/// you are making a SemVer-breaking change to the metrics format — bump
/// `METRICS_SCHEMA_VERSION` and document the upgrade path.
const REQUIRED_TOP_LEVEL_KEYS: &[&str] = &[
    "schema", "ts", "backend", "locale", "event", "unit_id", "rule", "detail",
];

#[test]
fn current_schema_version_is_one() {
    // Trip-wire: any bump of this constant is a deliberate breaking change.
    assert_eq!(METRICS_SCHEMA_VERSION, 1);
}

#[test]
fn every_event_carries_the_required_top_level_keys() {
    let sink = MemorySink::new();
    let writer = MetricsWriter::new("manual", "de_DE", sink);
    writer.record_report(&report_with_every_rule()).unwrap();
    let events = writer.sink().events();

    // 4 hard + 4 soft = 8 events; semantic findings are not written by the
    // metrics writer (they are model-supplied, not gate-supplied).
    assert_eq!(events.len(), 8, "expected one event per hard+soft finding");

    let lines = writer.sink().lines();
    for raw in &lines {
        let v: Value = serde_json::from_slice(raw).expect("event is valid JSON");
        let obj = v.as_object().expect("event is a JSON object");
        for k in REQUIRED_TOP_LEVEL_KEYS {
            assert!(obj.contains_key(*k), "missing key `{k}` in event:\n{v}");
        }
        // Type contract for each top-level key.
        assert_eq!(obj["schema"], serde_json::json!(METRICS_SCHEMA_VERSION));
        assert!(obj["ts"].is_string(), "ts must be string");
        assert!(obj["backend"].is_string());
        assert!(obj["locale"].is_string());
        assert!(obj["event"].is_string());
        assert!(obj["unit_id"].is_string());
        assert!(obj["rule"].is_string());
        assert!(obj["detail"].is_object());

        // The `ts` value must be RFC3339 with microseconds, UTC `Z`.
        let ts = obj["ts"].as_str().unwrap();
        assert!(ts.ends_with('Z'), "ts must be UTC `Z`: {ts}");
        assert_eq!(ts.len(), "2026-05-23T14:33:22.123456Z".len(), "ts: {ts}");
    }
}

#[test]
fn hard_rules_emit_gate_reject_soft_rules_emit_soft_warning() {
    let sink = MemorySink::new();
    let writer = MetricsWriter::new("manual", "de_DE", sink);
    writer.record_report(&report_with_every_rule()).unwrap();
    let events = writer.sink().events();

    let hard_rules: std::collections::HashSet<&str> = [
        "placeholder-mismatch",
        "plural-arity-mismatch",
        "icu-parse-error",
        "empty-target-when-finished",
    ]
    .into_iter()
    .collect();

    for e in &events {
        let v = serde_json::to_value(e).unwrap();
        let rule = v["rule"].as_str().unwrap();
        let event_kind = v["event"].as_str().unwrap();
        let expected = if hard_rules.contains(rule) {
            "gate-reject"
        } else {
            "soft-warning"
        };
        assert_eq!(
            event_kind, expected,
            "rule `{rule}` should emit `{expected}`, got `{event_kind}`"
        );
    }
}

#[test]
fn per_rule_detail_shapes_are_stable() {
    let sink = MemorySink::new();
    let writer = MetricsWriter::new("manual", "de_DE", sink);
    writer.record_report(&report_with_every_rule()).unwrap();

    let mut by_rule: std::collections::BTreeMap<String, Value> = std::collections::BTreeMap::new();
    for raw in writer.sink().lines() {
        let v: Value = serde_json::from_slice(&raw).unwrap();
        by_rule.insert(v["rule"].as_str().unwrap().to_owned(), v);
    }

    // placeholder-mismatch
    let pm = &by_rule["placeholder-mismatch"]["detail"];
    assert!(pm["slot"].is_u64());
    assert!(pm["missing"].is_array());
    assert!(pm["extra"].is_array());

    // plural-arity-mismatch
    let pa = &by_rule["plural-arity-mismatch"]["detail"];
    assert!(pa["expected"].is_u64());
    assert!(pa["found"].is_u64());
    assert!(pa["wrong_variant"].is_boolean());

    // icu-parse-error
    let ipe = &by_rule["icu-parse-error"]["detail"];
    assert!(ipe["slot"].is_u64());
    assert!(ipe["byte_offset"].is_u64());
    assert!(ipe["message"].is_string());

    // empty-target-when-finished
    let et = &by_rule["empty-target-when-finished"]["detail"];
    assert!(et["slot"].is_u64());

    // accel-mismatch
    let am = &by_rule["accel-mismatch"]["detail"];
    assert!(am["source_count"].is_u64());
    assert!(am["target_count"].is_u64());

    // length-warn
    let lw = &by_rule["length-warn"]["detail"];
    assert!(lw["source_chars"].is_u64());
    assert!(lw["target_chars"].is_u64());
    assert!(lw["threshold"].is_number());
    assert!(lw["ratio"].is_number());

    // cjk-punctuation-tolerated
    let cjk = &by_rule["cjk-punctuation-tolerated"]["detail"];
    assert!(cjk["characters"].is_array());

    // placeholder-agreement-risk
    let par = &by_rule["placeholder-agreement-risk"]["detail"];
    assert!(par["placeholder"].is_string());
    assert!(par["determiner"].is_string());
}
