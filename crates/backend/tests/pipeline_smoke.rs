//! End-to-end smoke test that wires the [`TranslationBackend`] trait
//! through the full M2 pipeline:
//!
//! ```text
//!   adapter-qt::extract → batch → manual backend → gate → adapter-qt::apply
//! ```
//!
//! This is the test the architect uses to prove the trait surface fits
//! the rest of the workspace before any HTTP backend lands. The manual
//! backend is the test substrate; replace it with `ollama` and the
//! pipeline shape is unchanged.

use i18n_harness_adapter_qt as adapter_qt;
use i18n_harness_backend::{
    ManualBackend, ManualResponse, TranslatedText, TranslationBackend, TranslationOutcome,
};
use i18n_harness_core::{Batch, BatchKey, Target, Unit, UnitState};
use i18n_harness_gate::validate_batch;
use i18n_harness_glossary::Glossary;
use i18n_harness_locales::Locale;

// Short source strings chosen so the gate's soft length-warn (1.4× for
// de_DE) does not fire on plausible German translations. We are testing
// the pipeline shape, not pushing the length heuristic.
const SAMPLE_TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="de_DE" sourcelanguage="en">
<context>
    <name>Greet</name>
    <message>
        <source>Open the requested document</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <source>Save the document as %1</source>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>
"#;

fn tempdir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "i18n-harness-backend-{label}-{pid}-{ts}",
        pid = std::process::id(),
        ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

#[test]
fn full_pipeline_with_manual_backend_translates_then_gate_passes() {
    let dir = tempdir("pipe");
    let path = dir.join("greet.ts");
    std::fs::write(&path, SAMPLE_TS).unwrap();

    let catalog = adapter_qt::extract(&path).expect("extract");
    let de_de = Locale::by_id("de_DE").unwrap();

    // Build a batch from extracted units.
    let units: Vec<Unit> = catalog.units().to_vec();
    let batch = Batch::new(BatchKey::new("smoke", 0), units.clone());

    // A manual closure that does a *correct* translation for each unit.
    // Target lengths are kept under de_DE's 1.4× length-warn ratio.
    let backend = ManualBackend::new(|ctx| match ctx.unit.source.as_str() {
        "Open the requested document" => ManualResponse::Singular("Geforderte Datei öffnen".into()),
        s if s.contains("{0}") => ManualResponse::Singular("Datei speichern als {0}".into()),
        _ => ManualResponse::Skip,
    });

    let outcomes = backend
        .translate_batch(&batch, de_de, None)
        .expect("manual backend never errors at the batch level");
    assert_eq!(outcomes.len(), batch.units.len());

    // Merge translation text into a working unit vector. This is exactly
    // what the M2 translate CLI driver does.
    let mut translated: Vec<Unit> = batch.units.clone();
    for (i, outcome) in outcomes.iter().enumerate() {
        if let TranslationOutcome::Translated { text, .. } = outcome {
            match text {
                TranslatedText::Singular(s) => {
                    translated[i].target = Target::Singular {
                        text: Some(s.clone()),
                    };
                    translated[i].state = UnitState::Proposed;
                }
                TranslatedText::Plural(forms) => {
                    translated[i].target = Target::Plural {
                        forms: forms.iter().cloned().map(Some).collect(),
                    };
                    translated[i].state = UnitState::Proposed;
                }
            }
        }
    }

    // Gate the translated units. Both should pass cleanly.
    let reports = validate_batch(&translated, de_de, None);
    for r in &reports {
        assert!(
            r.is_clean(),
            "expected clean report, got findings: {:?}",
            r.findings
        );
    }

    // Structural fields are untouched.
    for (orig, new) in batch.units.iter().zip(translated.iter()) {
        assert_eq!(orig.id, new.id, "id must not change");
        assert_eq!(
            orig.placeholders, new.placeholders,
            "placeholders must not change",
        );
        assert_eq!(
            orig.provenance, new.provenance,
            "provenance must not change"
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn manual_backend_skip_does_not_advance_state() {
    let dir = tempdir("skip");
    let path = dir.join("greet.ts");
    std::fs::write(&path, SAMPLE_TS).unwrap();
    let catalog = adapter_qt::extract(&path).expect("extract");
    let de_de = Locale::by_id("de_DE").unwrap();

    let units = catalog.units().to_vec();
    let batch = Batch::new(BatchKey::new("skip", 0), units.clone());

    let backend = ManualBackend::new(|_| ManualResponse::Skip);
    let outcomes = backend.translate_batch(&batch, de_de, None).unwrap();

    // All outcomes are Skipped.
    assert!(
        outcomes
            .iter()
            .all(|o| matches!(o, TranslationOutcome::Skipped { .. }))
    );

    // The batch's units are still untranslated and Untranslated state.
    for u in &batch.units {
        assert!(matches!(u.target, Target::Singular { text: None }));
        assert_eq!(u.state, UnitState::Untranslated);
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn manual_backend_with_glossary_round_trips_register() {
    // Build an explicit glossary so the context's `register` reflects it.
    let glossary_toml = r#"
[meta]
schema_version = 1

[[term]]
source = "Open"
[term.translations]
de_DE = "Öffnen"

[locale.de_DE]
register = "formal"
"#;
    let (g, _) = Glossary::from_toml(glossary_toml).unwrap();

    let units = vec![Unit::untranslated_singular("greet::1", "Open the file")];
    let batch = Batch::new(BatchKey::new("g", 0), units);

    // The closure asserts what the context shows it; if PromptContext were
    // mis-wired, this test would fail with a precise message.
    let backend = ManualBackend::new(|ctx| {
        assert_eq!(ctx.locale.id, "de_DE");
        // glossary register override is `formal`; the workspace locales
        // table also says `formal` for de_DE — either way the effective
        // value passed in is `formal`.
        assert_eq!(
            ctx.register,
            i18n_harness_glossary::Register::Formal,
            "expected formal register from glossary override"
        );
        // Glossary terms are accessible.
        let pairs: Vec<_> = ctx.glossary_terms().collect();
        assert_eq!(pairs, vec![("Open", "Öffnen")]);
        ManualResponse::Singular("Datei öffnen".into())
    });

    let _ = backend
        .translate_batch(&batch, Locale::by_id("de_DE").unwrap(), Some(&g))
        .unwrap();
}
