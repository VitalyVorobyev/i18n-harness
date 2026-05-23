---
name: add-backend
description: Use when adding a new TranslationBackend implementation to i18n-harness (e.g., a new local server with a non-Ollama API, a cloud provider with a non-OpenAI-compatible API, a custom benchmark backend). Scaffolds the module under crates/backend, adds the Cargo feature, writes the trait impl stub, drops a prompt template under crates/backend/prompts/, and wires a smoke test that runs the manual backend through the same shape to confirm the wiring.
---

# add-backend

Adds a new `TranslationBackend` implementation behind a feature flag.

## When you DO need this skill

- A new local model server with a bespoke API (not OpenAI-compatible,
  not Ollama).
- A cloud provider with a non-OpenAI-compatible API.
- A benchmark/fake backend for testing or measurement.

## When you do NOT need this skill

- The provider speaks the OpenAI Chat Completions API → use the
  existing `openai-compatible` backend with a custom `endpoint` config.
- The provider speaks the Ollama API → use the existing `ollama`
  backend with a custom `host` config.
- You want to run a one-off translation by hand → use the `manual`
  backend or the two-phase `harness export-batch` / `harness import-batch`
  CLI.

## Steps

1. **Name the backend** in kebab-case (e.g., `my-runtime`) and confirm
   it does not conflict with `manual`, `ollama`, `openai-compatible`,
   or any contributor's existing one.
2. **Add a Cargo feature** in `crates/backend/Cargo.toml`:
   ```toml
   [features]
   my_runtime = ["dep:<http_crate>", "dep:<json_crate>"]
   ```
   Add the corresponding optional dependencies.
3. **Add the module** at `crates/backend/src/my_runtime.rs`, gated by
   `#[cfg(feature = "my_runtime")]`. Implement `TranslationBackend`:
   - `name(&self) -> &str` returns the kebab-case name.
   - `is_deterministic(&self) -> bool` — `false` for any LLM-backed
     path.
   - `translate_batch(&self, units, glossary) -> Result<Vec<Unit>>`
     does the HTTP/IPC dance, parses the response, and returns units
     with proposed `target` filled. **Never** edit `unit.id`,
     `placeholders`, or other structural fields — those are owned by
     the adapter.
4. **Add the prompt template** under
   `crates/backend/prompts/my-runtime.txt` (versioned, named template).
   Document the placeholders the template expects (source text, target
   locale, glossary terms, register, etc.).
5. **Register the backend** in `crates/cli/src/main.rs` so
   `--backend my-runtime` works. Gate the registration on the same
   feature flag.
6. **Write a smoke test** in `crates/backend/tests/` that runs the
   `manual` backend through the same call site (with a stubbed user
   response) to confirm the wiring and unit shape. If the new backend
   has a local fake (e.g., an in-process HTTP server), prefer that.

## What you do NOT do

- Do not let the backend write to the catalog directly — that is the
  adapter's job, after the gate.
- Do not embed gate logic in the backend; the gate runs *after* the
  backend returns and the same way for every backend.
- Do not bundle API keys or require an endpoint that needs one to
  build.

## Verification

- `cargo build --workspace --features my_runtime` succeeds.
- `cargo test --workspace --features my_runtime` passes the smoke test.
- A documented dry-run with the new backend produces metrics events
  with `backend = "my-runtime"`.
