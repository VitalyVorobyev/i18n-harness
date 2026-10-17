//! `ModelBackend` requests and answers against a mocked server.

#![cfg(all(feature = "ollama", feature = "openai-compatible"))]

mod support;

use i18n_harness_backend::discovery::{PromptStyle, ServerKind};
use i18n_harness_backend::profile::Profile;
use i18n_harness_backend::{ModelBackend, TranslatedText, TranslationBackend, TranslationOutcome};
use i18n_harness_core::{Batch, BatchKey, Target, Unit};
use i18n_harness_locales::Locale;
use support::{ok_response, spawn_capturing_server};

fn one(source: &str) -> Batch {
    Batch::new(
        BatchKey::new("t", 0),
        vec![Unit::untranslated_singular("id", source)],
    )
}

fn text(outcome: &TranslationOutcome) -> &str {
    match outcome {
        TranslationOutcome::Translated {
            text: TranslatedText::Singular(s),
            ..
        } => s,
        other => panic!("not translated: {other:?}"),
    }
}

fn sk() -> &'static Locale {
    Locale::by_id("sk_SK").unwrap()
}

#[test]
fn completion_profile_sends_the_raw_prompt_to_generate() {
    let (host, requests) =
        spawn_capturing_server(vec![ok_response(r#"{"response":" Veľkosť filtra"}"#)]);
    let backend = ModelBackend::new(
        ServerKind::Ollama,
        host,
        "mt",
        Profile::builtin(PromptStyle::Completion),
    );
    let out = backend
        .translate_batch_from(
            &one("Filtergröße"),
            Locale::by_id("de_DE").unwrap(),
            sk(),
            None,
        )
        .unwrap();
    assert_eq!(text(&out[0]), "Veľkosť filtra");
    let (line, body) = requests.recv().unwrap();
    assert!(line.starts_with("POST /api/generate"), "{line}");
    let body: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["raw"], true);
    assert_eq!(
        body["prompt"],
        "Translate this from German to Slovak:\nGerman: Filtergröße\nSlovak:"
    );
    assert_eq!(body["options"]["temperature"], 0.0);
    assert_eq!(body["options"]["top_k"], 1);
    assert_eq!(body["options"]["stop"], serde_json::json!(["\n"]));
}

#[test]
fn chat_profile_sends_system_and_user_messages() {
    let (host, requests) = spawn_capturing_server(vec![ok_response(
        r#"{"message":{"role":"assistant","content":"\"Otvoriť %1\""}}"#,
    )]);
    let backend = ModelBackend::new(
        ServerKind::Ollama,
        host,
        "chat",
        Profile::builtin(PromptStyle::Chat),
    )
    .with_num_ctx(Some(4096));
    let out = backend
        .translate_batch(&one("Open %1"), sk(), None)
        .unwrap();
    assert_eq!(
        text(&out[0]),
        "Otvoriť %1",
        "wrapping quotes the original lacks are removed"
    );
    let (line, body) = requests.recv().unwrap();
    assert!(line.starts_with("POST /api/chat"), "{line}");
    let body: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["messages"][0]["role"], "system");
    let user = body["messages"][1]["content"].as_str().unwrap();
    assert!(
        user.contains("English user-interface text into Slovak"),
        "{user}"
    );
    assert!(user.ends_with("Open %1"));
    assert_eq!(body["options"]["num_ctx"], 4096);
}

#[test]
fn openai_compatible_uses_chat_completions() {
    let (host, requests) = spawn_capturing_server(vec![ok_response(
        r#"{"choices":[{"message":{"role":"assistant","content":"Uložiť"}}]}"#,
    )]);
    let backend = ModelBackend::new(
        ServerKind::OpenAiCompatible,
        format!("{host}/v1"),
        "local",
        Profile::builtin(PromptStyle::Chat),
    );
    let out = backend.translate_batch(&one("Save"), sk(), None).unwrap();
    assert_eq!(text(&out[0]), "Uložiť");
    let (line, _) = requests.recv().unwrap();
    assert!(line.starts_with("POST /v1/chat/completions"), "{line}");
}

#[test]
fn plural_units_are_refused_and_empty_answers_fail_softly() {
    let (host, _requests) = spawn_capturing_server(vec![ok_response(r#"{"response":"   "}"#)]);
    let backend = ModelBackend::new(
        ServerKind::Ollama,
        host,
        "mt",
        Profile::builtin(PromptStyle::Completion),
    );
    let mut plural = Unit::untranslated_singular("p", "%n files");
    plural.plural_arity = Some(2);
    plural.target = Target::Plural {
        forms: vec![None, None],
    };
    let batch = Batch::new(
        BatchKey::new("t", 0),
        vec![plural, Unit::untranslated_singular("s", "x")],
    );
    let out = backend.translate_batch(&batch, sk(), None).unwrap();
    assert!(
        out.iter()
            .all(|o| matches!(o, TranslationOutcome::Failed { .. })),
        "{out:?}"
    );
}
