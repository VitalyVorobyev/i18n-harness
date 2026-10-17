//! `harness translate-folder` end to end against a scripted model server.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;

/// An Ollama stand-in: `/api/show` reports a pass-through template (so the
/// completion profile is chosen); `/api/generate` "translates" by tagging
/// the prompt's input line.
fn spawn_fake_ollama() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            let reply = if request_line.contains("/api/show") {
                serde_json::json!({
                    "template": "{{ .Prompt }}",
                    "details": { "format": "gguf" },
                    "capabilities": ["completion"]
                })
            } else {
                // Second line of the completion prompt: "<Language>: <text>".
                let prompt = body["prompt"].as_str().unwrap_or_default();
                let input = prompt
                    .lines()
                    .nth(1)
                    .and_then(|l| l.split_once(": "))
                    .map(|(_, t)| t)
                    .unwrap_or("");
                serde_json::json!({ "response": format!(" SK[{input}]") })
            };
            let text = reply.to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                text.len()
            );
        }
    });
    host
}

#[test]
fn translates_a_pivot_folder_and_delivers_finished_files() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/qt/sk_SK_pivot.ts");
    let catalog = dir.path().join("App_sk_SK.ts");
    std::fs::copy(fixture, &catalog).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .args([
            "translate-folder",
            "--model",
            "fake-mt",
            "--pivot",
            "de_DE",
            "--deliver",
            "--save",
            "--host",
        ])
        .arg(spawn_fake_ollama())
        .arg(dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");

    let written = std::fs::read_to_string(&catalog).unwrap();
    // From the German pivot text where it is sound…
    assert!(
        written.contains("<translation>SK[Filtergröße]</translation>"),
        "{written}"
    );
    // …from English where there is none…
    assert!(
        written.contains("<translation>SK[pix]</translation>"),
        "{written}"
    );
    assert!(
        written.contains("<translation>SK[Open ParamsManager]</translation>"),
        "{written}"
    );
    // …plural forms translated with their counts and %n restored.
    assert!(
        written.contains("<numerusform>SK[%n file(s) successfully imported.]</numerusform>"),
        "{written}"
    );
    // Delivered as finished: the unfinished marker is gone.
    assert!(!written.contains("type=\"unfinished\""), "{written}");
    // Structure outside the translations is untouched.
    assert!(written.contains("<location filename=\"../src/settings.cpp\" line=\"+29\"/>"));
    assert!(dir.path().join("i18n-harness.toml").is_file());
}

#[test]
fn import_glossary_writes_the_folders_glossary_from_a_spreadsheet_export() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/qt/sk_SK_pivot.ts");
    std::fs::copy(fixture, dir.path().join("App_sk_SK.ts")).unwrap();
    let table = dir.path().join("terms.csv");
    std::fs::write(
        &table,
        "Term;Translation;Note\nThreshold;Prahová hodnota;\nBlob;;keep\n",
    )
    .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(["import-glossary", "--language", "sk_SK"])
        .arg(dir.path())
        .arg(&table)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(stderr.contains("2 term(s) added, 0 updated"), "{stderr}");
    let glossary = std::fs::read_to_string(dir.path().join("glossary.toml")).unwrap();
    assert!(glossary.contains("Prahová hodnota"), "{glossary}");
    assert!(glossary.contains("source = \"Blob\""), "{glossary}");
}
