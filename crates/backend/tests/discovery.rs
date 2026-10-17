//! Model discovery against a mocked server: the harness lists whatever the
//! server offers and reads the prompt style from the template it reports.

#![cfg(all(feature = "ollama", feature = "openai-compatible"))]

mod support;

use i18n_harness_backend::BackendError;
use i18n_harness_backend::discovery::{ModelProvider, OllamaServer, OpenAiServer, PromptStyle};
use support::{ok_response, spawn_mock_server, status_response};

const TAGS: &str = r#"{"models":[
  {"name":"mt-model:q8","model":"mt-model:q8","size":4700000000,"digest":"dc12",
   "details":{"family":"gemma3","parameter_size":"3.9B","quantization_level":"Q8_0"}},
  {"name":"chat-model:9b","model":"chat-model:9b","size":9700000000,"digest":"716d",
   "details":{"family":"llama","parameter_size":"9.2B","quantization_level":"Q8_0"}}
]}"#;

const SHOW_COMPLETION: &str = r#"{
  "template":"{{ if .System }}{{ .System }}{{ end }}{{ if .Prompt }}{{ .Prompt }}{{ end }}{{ .Response }}",
  "details":{"format":"gguf","family":"gemma3","parameter_size":"3.9B","quantization_level":"Q8_0"},
  "model_info":{"general.architecture":"gemma3","gemma3.context_length":131072},
  "capabilities":["completion","vision"]
}"#;

#[test]
fn ollama_lists_every_model_the_server_reports() {
    let (host, _h) = spawn_mock_server(vec![ok_response(TAGS)]);
    let models = OllamaServer::new(host).list_models().expect("list");
    let ids: Vec<_> = models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["mt-model:q8", "chat-model:9b"]);
    assert_eq!(models[1].parameter_size.as_deref(), Some("9.2B"));
    assert_eq!(models[0].size_bytes, Some(4_700_000_000));
}

#[test]
fn ollama_describe_reads_context_capabilities_and_prompt_style() {
    // `describe` falls back to /api/tags for the digest, which /api/show lacks.
    let (host, _h) = spawn_mock_server(vec![ok_response(SHOW_COMPLETION), ok_response(TAGS)]);
    let info = OllamaServer::new(host)
        .describe("mt-model:q8")
        .expect("describe");
    assert_eq!(info.context_length, Some(131_072));
    assert_eq!(info.prompt_style, PromptStyle::Completion);
    assert!(info.can_generate());
    assert_eq!(info.summary.digest.as_deref(), Some("dc12"));
}

#[test]
fn ollama_not_running_is_a_network_error() {
    // Nothing listens on this port.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let err = OllamaServer::new(host).list_models().unwrap_err();
    assert!(matches!(err, BackendError::Network { .. }), "{err:?}");
}

#[test]
fn openai_compatible_lists_models_and_assumes_chat() {
    let body = r#"{"object":"list","data":[{"id":"local-model","object":"model"}]}"#;
    let (host, _h) = spawn_mock_server(vec![ok_response(body), ok_response(body)]);
    let server = OpenAiServer::new(format!("{host}/v1"));
    let ids: Vec<_> = server
        .list_models()
        .unwrap()
        .into_iter()
        .map(|m| m.id)
        .collect();
    assert_eq!(ids, ["local-model"]);
    let info = server.describe("local-model").unwrap();
    assert_eq!(info.prompt_style, PromptStyle::Chat);
}

#[test]
fn openai_compatible_rejected_key_is_an_auth_error() {
    let (host, _h) = spawn_mock_server(vec![status_response(401, "Unauthorized")]);
    let err = OpenAiServer::new(format!("{host}/v1"))
        .with_api_key(Some("bad".into()))
        .list_models()
        .unwrap_err();
    assert!(matches!(err, BackendError::Auth { .. }), "{err:?}");
}
