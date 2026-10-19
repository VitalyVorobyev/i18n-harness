# Changelog

What changed in each release of i18n-harness, for the people who use it.
The [user guide](https://vitalyvorobyev.github.io/i18n-harness/)
describes the current version in full.

## Unreleased

## 0.2.0 — 2026-10-19

The first public release: a desktop app for translating the text of
software applications with a language model on your own computer, and
command-line tools for unattended runs.

### Download

| System | File |
|---|---|
| macOS, Apple silicon | `i18n-harness_0.2.0_aarch64.dmg` |
| macOS, Intel | `i18n-harness_0.2.0_x64.dmg` |
| Windows 10 / 11 | `i18n-harness_0.2.0_x64-setup.exe` (or the `.msi`) |
| Linux | `i18n-harness_0.2.0_amd64.deb` or `.AppImage` |
| Command-line tools | `i18n-harness-cli-0.2.0-<system>.tar.gz` / `.zip` |

The builds are not signed by a known publisher yet; your system warns
you the first time you open the app.
[Install the app](https://vitalyvorobyev.github.io/i18n-harness/getting-started/install.html)
shows how to open it anyway. New here? Start with the
[Quick start](https://vitalyvorobyev.github.io/i18n-harness/quick-start.html).

### What you can do

- **Open a folder of translation files** — Qt Linguist (`.ts`), gettext
  (`.po`), Lingui (`.po`, `.json`) and ICU message JSON (react-intl,
  i18next). Files and their languages are found by themselves, also in
  `locales/<language>/` folders; every language in Unicode CLDR is
  supported.
- **Try it on a sample** from the start screen, without files of your
  own.
- **Translate with a model of your choice:** any model on a local Ollama
  server, or on an OpenAI-compatible server (LM Studio, vLLM, llama.cpp,
  a cloud provider). General chat models and dedicated translation
  models are both supported, and plural forms are translated one by one
  with an example number each.
- **Review** every suggestion under *To do*, *Check* and *Done*, with
  search, filters and keyboard shortcuts.
- **Trust the files:** a translation that loses a placeholder, changes
  markup, misses a plural form or breaks a glossary term is never
  written. Everything outside the translations stays byte for byte as it
  was.
- **Keep terms consistent** with a per-folder glossary: terms to keep
  unchanged or to translate one way, imported from a spreadsheet if you
  like, and applied to strings already translated.
- **Translate files pre-filled with another language** (for example a
  Slovak file that still holds German), from the original or from the
  existing text.
- **Work across files:** see strings that several files share, copy a
  translation everywhere, fill strings from what other files agree on.
- **On the command line:** `harness translate-folder` for unattended
  runs, `harness gate` as a CI check, `export-batch` / `import-batch` to
  hand strings to an AI coding agent such as Claude Code, and
  `harness-lab` to compare models on your own files.

### Known limitations

- The builds are unsigned (macOS and Windows warn on first open).
- A file whose language cannot be detected from its content, name or
  folder is left out with a warning; add it to `i18n-harness.toml` by
  hand. There is no language picker for it in the app yet.
- ICU plural category names (`one`, `other`, …) are not checked; look at
  plural messages yourself.
- The app does not extract strings from source code or compile
  translations; use your framework's tools for that.
