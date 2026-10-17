# Extending the harness

Each kind of extension has one place to go, by design (see the
[first invariant](architecture.md#the-two-invariants)). The repository
also ships Claude Code skills that walk through the same steps:
`add-format`, `add-backend` and `add-locale` in `.claude/skills/`.

## A catalog format

A format is an implementation of two traits from `crates/catalog`
(`src/format.rs`):

- `CatalogFormat` — the plugin: `id()` (the string used in
  `i18n-harness.toml`, e.g. `"gettext-po"`), `display_name()`,
  `extensions()`, `sniff(path, head)` (recognise a file from its first
  64 KiB and report a confidence and, if present, its language),
  `open(path)`, and the placeholder converters `placeholders_to_icu` /
  `placeholders_from_icu`.
- `FormatDoc` — an opened file: its `units()` (mutable through
  `units_mut()`), `language()`, `plural_forms()` (the file's own plural
  count, if it has one), and `render(units)` / `write(units, path)`.

Steps:

1. Add a module next to `crates/catalog/src/po/` (or a crate like
   `crates/adapter-qt` when the parser is large).
2. Write the placeholder converter first, with a property test that
   `from_icu(to_icu(x)) == x` on the format's own syntax.
3. Write the reader and writer so that unchanged units reproduce the
   input bytes exactly; keep enough of the original (byte ranges,
   original spellings) to splice changed translations in.
4. Add fixtures under `fixtures/<format>/` and add them to the
   [round-trip suite](round-trip.md).
5. List the format in `FormatRegistry::builtin()` in
   `crates/formats/src/lib.rs` and extend the registry tests. Nothing
   else in the workspace should name the format.

## A translation backend

A backend implements `TranslationBackend` (`crates/backend/src/trait_def.rs`):

```rust
pub trait TranslationBackend: Send + Sync {
    fn name(&self) -> &str;
    fn is_deterministic(&self) -> bool;
    fn translate_batch(&self, batch: &Batch, locale: &Locale, glossary: Option<&Glossary>)
        -> Result<Vec<TranslationOutcome>, BackendError>;
    fn translate_batch_from(&self, batch: &Batch, source: &Locale, locale: &Locale,
        glossary: Option<&Glossary>) -> Result<Vec<TranslationOutcome>, BackendError>;
}
```

It returns one `TranslationOutcome` per unit (translated, skipped or
failed) and never validates or writes anything — the gate and the
session do that. Put a new server API behind its own Cargo feature in
`crates/backend/Cargo.toml`, add a smoke test, and wire it into the
session's `backend()` (`crates/session/src/models.rs`) if the app should
offer it. Most local and cloud servers already speak the OpenAI API and
need no new backend.

Model discovery is separate: the `ModelProvider` trait in
`crates/backend/src/discovery.rs` lists a server's models and describes
one (capabilities, template, context length), from which
`PromptStyle` — and so the [prompt profile](../reference/profiles.md) — is
derived.

## A language

There is nothing to add: every CLDR language is in
`crates/locales/src/cldr.rs`, generated from the CLDR JSON packages:

```sh
cargo run -p xtask -- gen-locales \
  --cldr-core <unpacked cldr-core> \
  --cldr-localenames <unpacked cldr-localenames-full>
```

Run this when moving to a new CLDR release; the output is checked in.

What CLDR does not say — the default register and how much longer than
English a translation may be before the length check warns — lives in
`crates/locales/data/defaults.toml`, looked up by exact id, then
language, then script, then a fallback:

```toml
[languages.de]
register = "formal"
length_warn_ratio = 1.4
```

A project can override both per language in its settings file. When you
change defaults for a language with unusual plural rules, add a fixture
that exercises its plural forms.

## A prompt profile

Profiles are TOML files in `crates/backend/profiles/`, embedded at build
time by `crates/backend/src/profile.rs`. The two built-ins cover the two
`PromptStyle`s the discovery reports. To change wording or generation
settings, edit the TOML and run the backend tests
(`cargo test -p i18n-harness-backend`). A new profile needs a new
`PromptStyle` (or another selection rule derived from what the server
reports — never from the model's name) and an entry in
`Profile::builtin`. The fields are documented in
[Prompt profiles](../reference/profiles.md#fields).

To try a profile against a real model on Ollama without the app:

```sh
cargo run -p i18n-harness-backend --features ollama --example try_model -- \
  <model> en de_DE "Open file" "Saved %1 to %2"
```
