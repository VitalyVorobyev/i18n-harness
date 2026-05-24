//! Mocked HTTP tests for the v2 strict-JSON contract on
//! [`OllamaBackend`].
//!
//! These cover the wire path end-to-end: the backend issues `POST
//! /api/generate`, the mock server returns a canned Ollama envelope
//! whose `response` field is the model's v2 JSON object, and we assert
//! the resulting [`TranslationOutcome`]. Unit-level tests on the pure
//! `parse_v2_response` function live in `crates/backend/src/ollama.rs`
//! under `mod tests`; this file proves the wiring matches.

#![cfg(feature = "ollama")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use i18n_harness_backend::{
    FailureKind, OllamaBackend, TranslatedText, TranslationBackend, TranslationOutcome,
};
use i18n_harness_core::{Batch, BatchKey, Flag, Target, Unit};
use i18n_harness_locales::Locale;

// ── Mock server helpers ──────────────────────────────────────────────────────

fn ok_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

fn drain_request(stream: &mut std::net::TcpStream) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1];
    loop {
        if stream.read(&mut tmp).unwrap_or(0) == 0 {
            break;
        }
        buf.push(tmp[0]);
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let headers = String::from_utf8_lossy(&buf);
    let content_length: usize = headers
        .lines()
        .find_map(|line| {
            let lower = line.to_ascii_lowercase();
            lower
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse().unwrap_or(0))
        })
        .unwrap_or(0);
    if content_length > 0 {
        let mut body_buf = vec![0u8; content_length];
        let _ = stream.read_exact(&mut body_buf);
    }
}

fn spawn_mock_server(responses: Vec<String>) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let host = format!("http://{addr}");
    let handle = std::thread::spawn(move || {
        for response in responses {
            if let Ok((mut stream, _)) = listener.accept() {
                drain_request(&mut stream);
                let _ = stream.write_all(response.as_bytes());
            }
        }
    });
    (host, handle)
}

/// Build an Ollama-shaped envelope whose `response` field is the
/// model's raw output (the v2 JSON object, or anything else we want to
/// test the parser against). We embed the raw text with JSON-string
/// escaping so the outer JSON envelope stays valid.
fn ollama_envelope(raw_response: &str) -> String {
    let escaped = serde_json::to_string(raw_response).expect("escape");
    format!(r#"{{"response":{escaped},"done":true}}"#)
}

fn singular_unit(id: &str, source: &str) -> Unit {
    Unit::untranslated_singular(id, source)
}

fn plural_unit(id: &str, source: &str) -> Unit {
    let mut u = Unit::untranslated_singular(id, source);
    u.plural_arity = Some(2);
    u.target = Target::Plural {
        forms: vec![None, None],
    };
    u
}

fn de_de() -> &'static Locale {
    Locale::by_id("de_DE").expect("de_DE")
}

fn make_batch(units: Vec<Unit>) -> Batch {
    Batch::new(BatchKey::new("v2-test", 0), units)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Happy-path singular: model emits a v2 JSON object with translation +
/// confidence and no flags. Outcome carries the text, confidence, and
/// empty flags / flag_notes.
#[test]
fn v2_happy_path_singular_translation() {
    let model_response = r#"{"translation":"Hallo Welt","confidence":0.94}"#;
    let (host, _h) = spawn_mock_server(vec![ok_response(&ollama_envelope(model_response))]);

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_timeout(Duration::from_secs(5));

    let batch = make_batch(vec![singular_unit("greet", "Hello World")]);
    let outcomes = backend.translate_batch(&batch, de_de(), None).unwrap();

    assert_eq!(outcomes.len(), 1);
    match &outcomes[0] {
        TranslationOutcome::Translated {
            text,
            flags,
            confidence,
            flag_notes,
        } => {
            assert_eq!(text, &TranslatedText::Singular("Hallo Welt".into()));
            assert!(flags.is_empty());
            assert_eq!(*confidence, Some(0.94));
            assert!(flag_notes.is_empty());
        }
        other => panic!("expected Translated, got {other:?}"),
    }
}

/// Happy-path plural: arity 2 (de_DE) means two HTTP calls, one per
/// CLDR form. Each form's v2 response is a singular JSON object — the
/// backend assembles them into `TranslatedText::Plural`. Confidence is
/// the minimum across forms; flags are unioned.
#[test]
fn v2_happy_path_plural_translation_aggregates_per_form() {
    let one = r#"{"translation":"%n Element","confidence":0.9}"#;
    let other = r#"{"translation":"%n Elemente","flags":[{"kind":"low-confidence","note":"unsure"}],"confidence":0.6}"#;
    let (host, h) = spawn_mock_server(vec![
        ok_response(&ollama_envelope(one)),
        ok_response(&ollama_envelope(other)),
    ]);

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_timeout(Duration::from_secs(5));

    let batch = make_batch(vec![plural_unit("p", "%n items")]);
    let outcomes = backend.translate_batch(&batch, de_de(), None).unwrap();
    h.join().unwrap();

    assert_eq!(outcomes.len(), 1);
    match &outcomes[0] {
        TranslationOutcome::Translated {
            text: TranslatedText::Plural(forms),
            flags,
            confidence,
            flag_notes,
        } => {
            assert_eq!(
                forms,
                &vec!["%n Element".to_string(), "%n Elemente".to_string()]
            );
            assert_eq!(flags, &vec![Flag::LowConfidence]);
            // Min across forms; the second form is the weakest.
            assert_eq!(*confidence, Some(0.6));
            assert_eq!(
                flag_notes.get(&Flag::LowConfidence),
                Some(&"unsure".to_string())
            );
        }
        other => panic!("expected Translated plural, got {other:?}"),
    }
}

/// A v2 response with multiple semantic flags and per-flag notes lands
/// intact on the outcome (order is the model's; notes are keyed by flag
/// kind).
#[test]
fn v2_with_multiple_flags_and_notes() {
    let model_response = r#"{"translation":"Aufnahme","flags":[{"kind":"ambiguous-source","note":"noun vs verb"},{"kind":"brand-term","note":"looks like a product name"}],"confidence":0.5}"#;
    let (host, _h) = spawn_mock_server(vec![ok_response(&ollama_envelope(model_response))]);

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_timeout(Duration::from_secs(5));

    let batch = make_batch(vec![singular_unit("rec", "Record")]);
    let outcomes = backend.translate_batch(&batch, de_de(), None).unwrap();

    match &outcomes[0] {
        TranslationOutcome::Translated {
            flags, flag_notes, ..
        } => {
            assert_eq!(flags, &vec![Flag::AmbiguousSource, Flag::BrandTerm]);
            assert_eq!(flag_notes.len(), 2);
            assert_eq!(
                flag_notes.get(&Flag::AmbiguousSource),
                Some(&"noun vs verb".to_string())
            );
            assert_eq!(
                flag_notes.get(&Flag::BrandTerm),
                Some(&"looks like a product name".to_string())
            );
        }
        other => panic!("expected Translated, got {other:?}"),
    }
}

/// An unknown flag `kind` in the model's response is malformed under
/// the strict parser. The outcome is `Failed { failure_kind:
/// MalformedResponse, retryable: false }` — no fallback, no retry.
#[test]
fn v2_unknown_flag_kind_is_malformed_response() {
    let model_response =
        r#"{"translation":"Hallo","flags":[{"kind":"sounds-funny"}],"confidence":0.9}"#;
    let (host, _h) = spawn_mock_server(vec![ok_response(&ollama_envelope(model_response))]);

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_timeout(Duration::from_secs(5));

    let batch = make_batch(vec![singular_unit("a", "Hello")]);
    let outcomes = backend.translate_batch(&batch, de_de(), None).unwrap();

    match &outcomes[0] {
        TranslationOutcome::Failed {
            failure_kind,
            retryable,
            reason,
        } => {
            assert_eq!(*failure_kind, FailureKind::MalformedResponse);
            assert!(!retryable, "malformed responses must not retry");
            assert!(reason.contains("v2-parse-error"), "reason: {reason}");
        }
        other => panic!("expected Failed MalformedResponse, got {other:?}"),
    }
}

/// Confidence outside `[0.0, 1.0]` is malformed under the strict
/// parser.
#[test]
fn v2_confidence_out_of_bounds_is_malformed_response() {
    let model_response = r#"{"translation":"Hallo","confidence":2.0}"#;
    let (host, _h) = spawn_mock_server(vec![ok_response(&ollama_envelope(model_response))]);

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_timeout(Duration::from_secs(5));

    let batch = make_batch(vec![singular_unit("a", "Hello")]);
    let outcomes = backend.translate_batch(&batch, de_de(), None).unwrap();

    match &outcomes[0] {
        TranslationOutcome::Failed {
            failure_kind,
            retryable,
            reason,
        } => {
            assert_eq!(*failure_kind, FailureKind::MalformedResponse);
            assert!(!retryable);
            assert!(
                reason.contains("v2-confidence-out-of-bounds"),
                "reason: {reason}"
            );
        }
        other => panic!("expected Failed MalformedResponse, got {other:?}"),
    }
}

/// Selecting the v1 template via `with_template_v1` makes the backend
/// accept plain-text responses again — backwards-compatible behaviour
/// for the CLI's `--prompt` override path.
#[test]
fn v1_plain_text_template_round_trips_text_without_confidence() {
    let model_response = "Hallo Welt"; // bare text, no JSON
    let (host, _h) = spawn_mock_server(vec![ok_response(&ollama_envelope(model_response))]);

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_template_v1()
        .with_timeout(Duration::from_secs(5));

    let batch = make_batch(vec![singular_unit("a", "Hello World")]);
    let outcomes = backend.translate_batch(&batch, de_de(), None).unwrap();

    match &outcomes[0] {
        TranslationOutcome::Translated {
            text,
            flags,
            confidence,
            flag_notes,
        } => {
            assert_eq!(text, &TranslatedText::Singular("Hallo Welt".into()));
            assert!(flags.is_empty());
            assert_eq!(*confidence, None, "v1 never reports confidence");
            assert!(flag_notes.is_empty());
        }
        other => panic!("expected Translated, got {other:?}"),
    }
}
