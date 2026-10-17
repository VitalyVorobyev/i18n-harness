//! Print the models a local Ollama server offers and how each would be
//! prompted. `cargo run -p i18n-harness-backend --features ollama --example
//! list_models [host]`.

use i18n_harness_backend::discovery::{ModelProvider, OllamaServer};

fn main() {
    let host = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("OLLAMA_HOST").ok())
        .unwrap_or_else(|| "http://localhost:11434".to_owned());
    let server = OllamaServer::new(&host);
    let models = match server.list_models() {
        Ok(models) => models,
        Err(err) => {
            eprintln!("cannot list models at {host}: {err}");
            std::process::exit(1);
        }
    };
    for summary in models {
        match server.describe(&summary.id) {
            Ok(info) => println!(
                "{:<58} {:>6} ctx={:<7} style={:?} generate={} caps={:?}",
                info.summary.id,
                info.summary.parameter_size.as_deref().unwrap_or("?"),
                info.context_length.map_or("?".into(), |n| n.to_string()),
                info.prompt_style,
                info.can_generate(),
                info.capabilities,
            ),
            Err(err) => println!("{:<58} describe failed: {err}", summary.id),
        }
    }
}
