//! Mocked HTTP tests for [`OllamaBackend`].
//!
//! Each test spawns a minimal `TcpListener` on a random loopback port,
//! wires an `OllamaBackend` to that address, and asserts the outcome.
//! No async runtime, no external mock crate — just `std::net`.
//!
//! The mock server reads until the end of the HTTP request headers
//! (`\r\n\r\n`), then reads the body if `Content-Length` is present,
//! and writes a canned HTTP/1.1 response. It handles exactly as many
//! connections as the test needs; afterwards the thread exits.

#![cfg(feature = "ollama")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use i18n_harness_backend::{BackendError, OllamaBackend, TranslationBackend, TranslationOutcome};
use i18n_harness_core::{Batch, BatchKey, Target, Unit};
use i18n_harness_locales::Locale;

// ── Mock server helpers ──────────────────────────────────────────────────────

/// A canned HTTP/1.1 200 response with the given JSON body.
fn ok_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

/// A canned HTTP/1.1 response with an arbitrary status and no body.
fn status_response(code: u16, reason: &str) -> String {
    format!("HTTP/1.1 {code} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
}

/// Read an HTTP request from `stream` until the header block ends.
/// Also reads the body if `Content-Length` is present in the headers.
fn drain_request(stream: &mut std::net::TcpStream) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1];
    // Read byte-by-byte until \r\n\r\n.
    loop {
        if stream.read(&mut tmp).unwrap_or(0) == 0 {
            break;
        }
        buf.push(tmp[0]);
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    // Parse Content-Length and drain the body so the response is not
    // sent before the client has finished sending its request.
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

/// Spawn a mock server that handles `conn_count` connections sequentially.
///
/// `responses` must have exactly `conn_count` entries. The server sends
/// `responses[i]` for the i-th connection, in order.
///
/// Returns `(host_url, join_handle)`.
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

// ── Unit factories ────────────────────────────────────────────────────────────

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
    Batch::new(BatchKey::new("test", 0), units)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// 1. Happy path: model returns `{"response": "Hallo Welt"}`.
///    Outcome must be `Translated { text: Singular("Hallo Welt") }`.
#[test]
fn happy_path_singular_translation() {
    let body = r#"{"response": "Hallo Welt", "done": true}"#;
    let (host, _handle) = spawn_mock_server(vec![ok_response(body)]);

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_timeout(Duration::from_secs(5));

    let batch = make_batch(vec![singular_unit("greet", "Hello World")]);
    let outcomes = backend.translate_batch(&batch, de_de(), None).unwrap();

    assert_eq!(outcomes.len(), 1);
    match &outcomes[0] {
        TranslationOutcome::Translated {
            text: i18n_harness_backend::TranslatedText::Singular(s),
            ..
        } => assert_eq!(s, "Hallo Welt"),
        other => panic!("expected Translated singular, got {other:?}"),
    }
}

/// 2. Empty response field: `{"response": ""}`.
///    Outcome must be `Failed { reason: "ollama-empty-response", retryable: true }`.
#[test]
fn empty_response_field_is_failed_retryable() {
    let body = r#"{"response": "", "done": true}"#;
    let (host, _handle) = spawn_mock_server(vec![ok_response(body)]);

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_timeout(Duration::from_secs(5));

    let batch = make_batch(vec![singular_unit("a", "Hello")]);
    let outcomes = backend.translate_batch(&batch, de_de(), None).unwrap();

    assert_eq!(outcomes.len(), 1);
    match &outcomes[0] {
        TranslationOutcome::Failed { reason, retryable } => {
            assert_eq!(reason, "ollama-empty-response");
            assert!(retryable, "empty response should be retryable");
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

/// 3. Non-JSON response body: server returns `not json`.
///    Must produce `Err(BackendError::Protocol(...))`.
#[test]
fn non_json_body_is_protocol_error() {
    let (host, _handle) = spawn_mock_server(vec![ok_response("not json")]);

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_timeout(Duration::from_secs(5));

    let batch = make_batch(vec![singular_unit("a", "Hello")]);
    let result = backend.translate_batch(&batch, de_de(), None);

    match result {
        Err(BackendError::Protocol { backend, .. }) => {
            assert_eq!(backend, "ollama");
        }
        other => panic!("expected Protocol error, got {other:?}"),
    }
}

/// 4. 503 status: server returns 503.
///    Must produce `Err(BackendError::Network(...))`.
#[test]
fn http_503_is_network_error() {
    let (host, _handle) = spawn_mock_server(vec![status_response(503, "Service Unavailable")]);

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_timeout(Duration::from_secs(5));

    let batch = make_batch(vec![singular_unit("a", "Hello")]);
    let result = backend.translate_batch(&batch, de_de(), None);

    match result {
        Err(BackendError::Network { backend, .. }) => {
            assert_eq!(backend, "ollama");
        }
        other => panic!("expected Network error, got {other:?}"),
    }
}

/// 5. 401 status: server returns 401.
///    Must produce `Err(BackendError::Auth(...))`.
#[test]
fn http_401_is_auth_error() {
    let (host, _handle) = spawn_mock_server(vec![status_response(401, "Unauthorized")]);

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_timeout(Duration::from_secs(5));

    let batch = make_batch(vec![singular_unit("a", "Hello")]);
    let result = backend.translate_batch(&batch, de_de(), None);

    match result {
        Err(BackendError::Auth { backend, .. }) => {
            assert_eq!(backend, "ollama");
        }
        other => panic!("expected Auth error, got {other:?}"),
    }
}

/// 6a. Plural unit (de_DE, arity 2): one HTTP call per CLDR form, in
///     canonical order. Outcomes assembled into `TranslatedText::Plural`.
#[test]
fn plural_unit_issues_one_call_per_cldr_form() {
    // de_DE has arity 2 (one, other), so two HTTP calls.
    let responses = vec![
        ok_response(r#"{"response": "1 Element"}"#),
        ok_response(r#"{"response": "%n Elemente"}"#),
    ];
    let (host, handle) = spawn_mock_server(responses);

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_timeout(Duration::from_secs(5));

    let batch = make_batch(vec![plural_unit("p", "%n items")]);
    let outcomes = backend.translate_batch(&batch, de_de(), None).unwrap();
    handle.join().unwrap();

    assert_eq!(outcomes.len(), 1);
    match &outcomes[0] {
        TranslationOutcome::Translated {
            text: i18n_harness_backend::TranslatedText::Plural(forms),
            ..
        } => {
            assert_eq!(forms.len(), 2, "de_DE arity is 2");
            assert_eq!(forms[0], "1 Element");
            assert_eq!(forms[1], "%n Elemente");
        }
        other => panic!("expected Translated plural, got {other:?}"),
    }
}

/// 6b. Plural unit where one form returns empty (`retryable: true` Failed).
///     The whole unit becomes Failed with the form name in the reason.
#[test]
fn plural_unit_partial_failure_marks_whole_unit_failed() {
    let responses = vec![
        ok_response(r#"{"response": "1 Element"}"#),
        ok_response(r#"{"response": ""}"#), // empty → per-form Failed
    ];
    let (host, handle) = spawn_mock_server(responses);

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_timeout(Duration::from_secs(5));

    let batch = make_batch(vec![plural_unit("p", "%n items")]);
    let outcomes = backend.translate_batch(&batch, de_de(), None).unwrap();
    handle.join().unwrap();

    assert_eq!(outcomes.len(), 1);
    match &outcomes[0] {
        TranslationOutcome::Failed { reason, retryable } => {
            assert!(
                reason.contains("ollama-plural-form-other"),
                "expected per-form reason, got: {reason}"
            );
            assert!(reason.contains("ollama-empty-response"), "reason: {reason}");
            assert!(retryable, "underlying empty-response is retryable");
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

/// 6c. Mandarin plural arity is 1: exactly one HTTP call, single form.
#[test]
fn plural_unit_zh_hans_arity_1() {
    let responses = vec![ok_response(r#"{"response": "%n 条消息"}"#)];
    let (host, handle) = spawn_mock_server(responses);

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_timeout(Duration::from_secs(5));

    let zh = Locale::by_id("zh_Hans").expect("zh_Hans");
    let mut unit = plural_unit("msg", "%n messages");
    unit.plural_arity = Some(1);
    unit.target = Target::Plural { forms: vec![None] };
    let batch = make_batch(vec![unit]);
    let outcomes = backend.translate_batch(&batch, zh, None).unwrap();
    handle.join().unwrap();

    assert_eq!(outcomes.len(), 1);
    match &outcomes[0] {
        TranslationOutcome::Translated {
            text: i18n_harness_backend::TranslatedText::Plural(forms),
            ..
        } => {
            assert_eq!(forms.len(), 1, "zh_Hans arity is 1 (other only)");
            assert_eq!(forms[0], "%n 条消息");
        }
        other => panic!("expected Translated plural, got {other:?}"),
    }
}

/// 6d. Trip-wire on the model default: must be a Gemma 4 family tag. A
///     bump to a different Gemma 4 size (`gemma4:e2b`, `gemma4:e4b`, …) is
///     fine; silently dropping to another family is not.
#[test]
fn default_model_is_gemma_4_family() {
    assert!(
        i18n_harness_backend::ollama::DEFAULT_MODEL.starts_with("gemma4:"),
        "DEFAULT_MODEL must be a Gemma 4 family tag; got `{}`",
        i18n_harness_backend::ollama::DEFAULT_MODEL
    );
}

/// 6e. `with_model` overrides the model tag, and the chosen tag travels
///     through to the request body. Exercises the override path without
///     touching env vars (which are `unsafe` to mutate under our lint).
#[test]
fn with_model_overrides_request_payload() {
    // Capture the request body so we can assert the `model` field.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let host = format!("http://{addr}");
    let captured: std::sync::Arc<std::sync::Mutex<Vec<u8>>> = Default::default();
    let captured_clone = captured.clone();
    let handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut all = Vec::new();
            let mut tmp = [0u8; 1];
            loop {
                if stream.read(&mut tmp).unwrap_or(0) == 0 {
                    break;
                }
                all.push(tmp[0]);
                if all.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            let headers = String::from_utf8_lossy(&all);
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
                let mut body = vec![0u8; content_length];
                let _ = stream.read_exact(&mut body);
                *captured_clone.lock().unwrap() = body;
            }
            let _ = stream.write_all(ok_response(r#"{"response": "ok"}"#).as_bytes());
        }
    });

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_model("gemma4:e4b")
        .with_timeout(Duration::from_secs(5));

    let batch = make_batch(vec![singular_unit("u", "Hello")]);
    let _ = backend.translate_batch(&batch, de_de(), None).unwrap();
    handle.join().unwrap();

    let body = captured.lock().unwrap().clone();
    let parsed: serde_json::Value = serde_json::from_slice(&body)
        .unwrap_or_else(|_| panic!("body is not valid JSON: {}", String::from_utf8_lossy(&body)));
    assert_eq!(
        parsed["model"], "gemma4:e4b",
        "request body must reflect with_model override; full body:\n{parsed}"
    );
}

/// 7. Order preservation: 3-unit batch, each gets a distinct response.
///    Outcomes must be in the same order as input units.
#[test]
fn order_preserved_across_three_units() {
    let responses = vec![
        ok_response(r#"{"response": "Eins"}"#),
        ok_response(r#"{"response": "Zwei"}"#),
        ok_response(r#"{"response": "Drei"}"#),
    ];
    let (host, handle) = spawn_mock_server(responses);

    let backend = OllamaBackend::new()
        .unwrap()
        .with_host(host)
        .with_timeout(Duration::from_secs(5));

    let units = vec![
        singular_unit("u1", "One"),
        singular_unit("u2", "Two"),
        singular_unit("u3", "Three"),
    ];
    let batch = make_batch(units.clone());
    let outcomes = backend.translate_batch(&batch, de_de(), None).unwrap();

    handle.join().unwrap();

    assert_eq!(outcomes.len(), units.len());
    let expected = ["Eins", "Zwei", "Drei"];
    for (i, (outcome, want)) in outcomes.iter().zip(expected.iter()).enumerate() {
        match outcome {
            TranslationOutcome::Translated {
                text: i18n_harness_backend::TranslatedText::Singular(s),
                ..
            } => assert_eq!(s, want, "unit {i} order mismatch"),
            other => panic!("unit {i}: expected Translated singular, got {other:?}"),
        }
    }
}
