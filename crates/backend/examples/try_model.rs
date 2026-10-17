//! Translate a few texts with a model on a local Ollama server, using the
//! prompt profile its server-reported template calls for.
//! `cargo run -p i18n-harness-backend --features ollama --example try_model -- <model> <from> <to> <text>...`

use std::time::Instant;

use i18n_harness_backend::discovery::{ModelProvider, OllamaServer, ServerKind};
use i18n_harness_backend::profile::Profile;
use i18n_harness_backend::{ModelBackend, TranslatedText, TranslationBackend, TranslationOutcome};
use i18n_harness_core::{Batch, BatchKey, Unit};
use i18n_harness_locales::Locale;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [model, from, to, texts @ ..] = args.as_slice() else {
        eprintln!("usage: try_model <model> <from> <to> <text>...");
        std::process::exit(2);
    };
    let host = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".into());
    let info = OllamaServer::new(&host)
        .describe(model)
        .expect("describe model");
    let profile = Profile::builtin(info.prompt_style);
    println!("{model}: {:?} profile", profile.mode);
    let backend = ModelBackend::new(ServerKind::Ollama, host, model.clone(), profile);
    let from = Locale::by_id(from).expect("source locale");
    let to = Locale::by_id(to).expect("target locale");
    for text in texts {
        let batch = Batch::new(
            BatchKey::new("try", 0),
            vec![Unit::untranslated_singular("id", text.as_str())],
        );
        let started = Instant::now();
        let out = backend
            .translate_batch_from(&batch, from, to, None)
            .expect("translate");
        let shown = match &out[0] {
            TranslationOutcome::Translated {
                text: TranslatedText::Singular(s),
                ..
            } => s.clone(),
            other => format!("{other:?}"),
        };
        println!(
            "{:>6.2}s  {text}  →  {shown}",
            started.elapsed().as_secs_f32()
        );
    }
}
