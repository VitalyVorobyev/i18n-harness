//! Serialisation tests for `CorrectionProvenance`.
//!
//! Contracts:
//! - Manual correction (all-empty provenance) serialises to compact JSON without
//!   any provenance field keys.
//! - Full-populated provenance serialises all fields.
//! - Forward-compat: deserialisation succeeds for v1 records that lacked a
//!   `provenance` field entirely.

use std::path::PathBuf;

use i18n_harness_core::UnitId;
use i18n_harness_project::{Correction, CorrectionId, CorrectionProvenance};

fn base_correction(provenance: CorrectionProvenance) -> Correction {
    Correction {
        schema: 1,
        id: CorrectionId("corr_aabbccddeeff".to_owned()),
        ts: "2026-05-24T12:00:00.000000Z".to_owned(),
        catalog: PathBuf::from("cat.ts"),
        locale: "de_DE".to_owned(),
        unit_id: UnitId("ctx::open".to_owned()),
        source: "Open".to_owned(),
        mt_proposal: "Öffnen".to_owned(),
        human_target: "Öffnen".to_owned(),
        provenance,
        flags_at_correction: vec![],
    }
}

#[test]
fn empty_provenance_omits_field_keys() {
    let correction = base_correction(CorrectionProvenance::default());
    let json = serde_json::to_string(&correction).unwrap();

    assert!(
        !json.contains("\"backend\""),
        "empty backend must be omitted: {json}"
    );
    assert!(
        !json.contains("\"model\""),
        "empty model must be omitted: {json}"
    );
    assert!(
        !json.contains("\"model_version\""),
        "empty model_version must be omitted: {json}"
    );
    assert!(
        !json.contains("\"prompt_template_version\""),
        "empty prompt_template_version must be omitted: {json}"
    );
    assert!(
        !json.contains("\"glossary_version\""),
        "empty glossary_version must be omitted: {json}"
    );
}

#[test]
fn full_provenance_serialises_all_fields() {
    let prov = CorrectionProvenance {
        backend: "ollama".to_owned(),
        model: "gemma4:e2b".to_owned(),
        model_version: "sha256:abc123def456".to_owned(),
        prompt_template_version: "ollama-translate-v2".to_owned(),
        glossary_version: "sha256:fedcba987654".to_owned(),
    };
    let correction = base_correction(prov);
    let json = serde_json::to_string(&correction).unwrap();

    assert!(
        json.contains("\"backend\":\"ollama\""),
        "backend missing: {json}"
    );
    assert!(
        json.contains("\"model\":\"gemma4:e2b\""),
        "model missing: {json}"
    );
    assert!(
        json.contains("\"model_version\":\"sha256:abc123def456\""),
        "model_version missing: {json}"
    );
    assert!(
        json.contains("\"prompt_template_version\":\"ollama-translate-v2\""),
        "prompt_template_version missing: {json}"
    );
    assert!(
        json.contains("\"glossary_version\":\"sha256:fedcba987654\""),
        "glossary_version missing: {json}"
    );
}

#[test]
fn full_provenance_roundtrips() {
    let prov = CorrectionProvenance {
        backend: "ollama".to_owned(),
        model: "gemma4:e2b".to_owned(),
        model_version: "sha256:abc".to_owned(),
        prompt_template_version: "v3".to_owned(),
        glossary_version: "sha256:xyz".to_owned(),
    };
    let original = base_correction(prov);
    let json = serde_json::to_string(&original).unwrap();
    let parsed: Correction = serde_json::from_str(&json).unwrap();
    assert_eq!(original, parsed);
}

/// Forward-compat: a v1 JSONL record that was written before `provenance` was
/// added to the schema must deserialise without error, with `provenance` being
/// the default (all empty).
#[test]
fn deserialise_v1_record_without_provenance_field() {
    let v1_json = r#"{
        "schema": 1,
        "id": "corr_aabbccddeeff",
        "ts": "2026-05-24T12:00:00.000000Z",
        "catalog": "cat.ts",
        "locale": "de_DE",
        "unit_id": "ctx::open",
        "source": "Open",
        "mt_proposal": "Öffnen",
        "human_target": "Öffnen"
    }"#;

    let correction: Correction =
        serde_json::from_str(v1_json).expect("v1 record without provenance must parse");

    assert_eq!(correction.provenance, CorrectionProvenance::default());
    assert!(correction.flags_at_correction.is_empty());
}

/// Forward-compat: a record with only some provenance fields should deserialise
/// with the missing fields defaulting to empty strings.
#[test]
fn deserialise_partial_provenance() {
    let json = r#"{
        "schema": 1,
        "id": "corr_aabbccddeeff",
        "ts": "2026-05-24T12:00:00.000000Z",
        "catalog": "cat.ts",
        "locale": "de_DE",
        "unit_id": "ctx::open",
        "source": "Open",
        "mt_proposal": "",
        "human_target": "Öffnen",
        "provenance": {"backend": "ollama"}
    }"#;

    let correction: Correction = serde_json::from_str(json).expect("partial provenance must parse");

    assert_eq!(correction.provenance.backend, "ollama");
    assert!(correction.provenance.model.is_empty());
    assert!(correction.provenance.model_version.is_empty());
    assert!(correction.provenance.prompt_template_version.is_empty());
    assert!(correction.provenance.glossary_version.is_empty());
}

/// Verify the `flags_at_correction` field is omitted when empty.
#[test]
fn empty_flags_omitted_from_json() {
    let correction = base_correction(CorrectionProvenance::default());
    let json = serde_json::to_string(&correction).unwrap();
    assert!(
        !json.contains("\"flags_at_correction\""),
        "empty flags must be omitted: {json}"
    );
}

/// Verify the one-line constraint: serialised JSON must not contain literal
/// newlines (control characters would be escaped by serde_json).
#[test]
fn serialised_json_is_one_line() {
    let prov = CorrectionProvenance {
        backend: "ollama".to_owned(),
        model: "gemma4".to_owned(),
        ..Default::default()
    };
    let mut correction = base_correction(prov);
    // Embed a newline in source to verify it's escaped, not literal.
    correction.source = "line1\nline2".to_owned();
    let json = serde_json::to_string(&correction).unwrap();
    assert!(
        !json.contains('\n'),
        "serialised JSON must not contain literal newlines: {json}"
    );
    // serde_json encodes it as \\n.
    assert!(
        json.contains("line1\\nline2"),
        "newline must be escaped: {json}"
    );
}
