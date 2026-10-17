# i18n-harness

[![CI](https://github.com/VitalyVorobyev/i18n-harness/actions/workflows/ci.yml/badge.svg)](https://github.com/VitalyVorobyev/i18n-harness/actions/workflows/ci.yml)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](LICENSE)
[![Rust: 1.85+](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![Status: pre-alpha](https://img.shields.io/badge/status-pre--alpha-red.svg)](#status)

Local-first, offline-by-default translation harness for UI strings.
Point it at your translation catalogs, let a model fill in untranslated
entries, and write back a byte-stable result. The default path is fully
offline (Ollama running on your laptop, no API key, no cloud egress).
Cloud LLM backends are supported as an opt-in via per-project config,
and the two-phase CLI lets you delegate translation to an external
agent (Claude Code / Copilot / Codex) when you want to.

> **The two invariants this project rests on**
>
> 1. **Languages, translation engines, and catalog formats are data or
>    plugins behind stable extension points.** A new language is a config
>    row. A new engine is a trait impl. A new format is a serializer.
> 2. **The model translates text; deterministic Rust does everything
>    else.** Parsing, validation, and write-back never go through the LLM.
>    A weak model can produce worse *text*. It cannot corrupt *structure*.

## Why

Most translation tools either (a) require a paid cloud API account to
do anything at all, or (b) demand you hand-edit catalogs in formats
built for translators, not developers. `i18n-harness` flips both.

- The **default path** runs entirely on your laptop against a local
  model server (Ollama). No API key needed, no cloud egress, free to
  evaluate. Cloud LLMs are an opt-in per-project backend when you
  want them, not a gate to clear before getting started.
- A CLDR-driven validation gate guarantees the catalog round-trips
  byte-stably regardless of which backend produced the translation,
  so weak models can produce *worse text* but never *broken files*.
- A per-`(backend, locale)` quality metric tells you empirically how
  well each engine is doing for each language pair, so you can pick
  your trade-off (local-and-free vs. cloud-and-strong vs.
  external-agent) on evidence, not vendor pitch.

## Status

**Pre-alpha.** Core, multi-catalog project mode (with reference
catalogs), all three target catalog formats (Qt `.ts`, PO, ICU-JSON),
the validation gate, the local Ollama and `openai-compatible` backends,
the Tauri desktop UI, quality evaluation, the two-phase agent
translation CLI, and deterministic reference reuse / remainder split /
merge for Qt (copy expert translations by exact unit-id match, carve
the leftover, merge it back — no model) are all in. UI integration of
the agent flow is the next visible step. See
[`docs/roadmap-product.md`](docs/roadmap-product.md) for the
translator-facing milestone log and
[`docs/initial_design.md`](docs/initial_design.md) for the full design.

## Scope

**Catalog formats:** Qt Linguist `.ts`, gettext / Lingui PO, ICU-JSON
for react-intl / i18next. Each is a `CatalogFormat` plugin behind a
stable extension point — new formats land as serializers, not
parser rewrites.

**Translation engines:**

- `manual` — closure-driven, no model. The trait substrate; also the
  engine the agent-translation CLI routes through.
- `ollama` — local Gemma 4 by default; the offline-by-default path.
- `openai-compatible` — covers vLLM, LM Studio, and cloud APIs
  (OpenAI, Anthropic, any provider speaking OpenAI Chat Completions).
  Endpoint and API key per-project; the harness itself stores no
  credentials.
- **Two-phase agent CLI** (`harness export-batch` /
  `harness import-batch` + the
  [`translate-i18n-batch`](skills/translate-i18n-batch/) skill) for
  delegating translation to an external Claude Code, Copilot, or
  Codex session running on the user's machine. The harness writes a
  self-contained batch folder; the agent fills `targets.jsonl`; the
  harness ingests the result and runs the validation gate. No
  in-process model dependency.

**Locales:** `en` source plus `de_DE`, `es_ES`, `zh_Hans` targets out
of the box. Adding a new locale is a single config-row change in
`crates/locales` plus a CLDR plural-arity fixture — data, not code.

## Quick check

```sh
cargo build --workspace
cargo test  --workspace --all-targets
cargo run   -p i18n-harness-cli -- --help
```

## Architecture (one paragraph)

```
catalog ─► adapter ─► intermediate (ICU JSONL) ─► backend ─► gate ─► adapter ─► catalog'
           extract                                translate           write-back
```

The catalog is the source of truth. The intermediate (per-`Unit` JSONL,
all placeholders normalized to ICU MessageFormat) is transient and
regenerable. Adapters know about file formats and placeholders; backends
know about text; the validation gate knows about CLDR plural arity and
placeholder integrity. The three never overlap.

## Repository layout

```
i18n-harness/
├── crates/
│   ├── core/           # Unit, intermediate (ICU), batches
│   ├── locales/        # locale records + CLDR plural-arity tables
│   ├── gate/           # validation gate (CLDR-driven, format-agnostic)
│   ├── glossary/       # per-project TOML glossary
│   ├── backend/        # TranslationBackend trait + impls
│   ├── catalog/        # CatalogFormat trait + PO/ICU-JSON serializers
│   ├── adapter-qt/     # Qt .ts adapter (reference adapter) + subset writer
│   ├── adapter-react/  # React adapter over catalog serializers
│   ├── project/        # project manifest (multi-catalog + references), state
│   ├── reuse/          # reference reuse / remainder split / merge (Qt, no model)
│   └── cli/            # `harness` binary
├── ui/                 # Tauri 2 + Vite + React desktop shell
├── docs/               # design + plan
├── fixtures/           # round-trip + gate test inputs
└── .claude/            # subagents + project skills
```

## Contributing

The project is in early scaffolding; the design lives in
[`docs/initial_design.md`](docs/initial_design.md) and the working
agreement in [`CLAUDE.md`](CLAUDE.md). Three kinds of contribution land
without touching the core, each guided by a project skill under
[`.claude/skills/`](.claude/skills/):

- **`add-locale`** — a new locale is a config row + CLDR arity + a
  round-trip fixture.
- **`add-backend`** — a new `TranslationBackend` is a feature-gated
  trait impl + a prompt template + a smoke test.
- **`add-format`** — a new `CatalogFormat` is a feature-gated serializer
  with an ICU placeholder converter and a round-trip property test.

The round-trip contract (byte-identical `extract → apply` on every
fixture) is non-negotiable and enforced in CI.

## License

Apache-2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
