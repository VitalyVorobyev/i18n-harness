# i18n-harness

[![CI](https://github.com/VitalyVorobyev/i18n-harness/actions/workflows/ci.yml/badge.svg)](https://github.com/VitalyVorobyev/i18n-harness/actions/workflows/ci.yml)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](LICENSE)
[![Rust: 1.85+](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![Status: pre-alpha](https://img.shields.io/badge/status-pre--alpha-red.svg)](#status)

Local-first, offline translation harness for UI strings. Point it at your
translation catalogs, let a local LLM fill in untranslated entries, and
write back a byte-stable result — no API key, no cloud, no agent in the
loop.

> **The two invariants this project rests on**
>
> 1. **Languages, translation engines, and catalog formats are data or
>    plugins behind stable extension points.** A new language is a config
>    row. A new engine is a trait impl. A new format is a serializer.
> 2. **The model translates text; deterministic Rust does everything
>    else.** Parsing, validation, and write-back never go through the LLM.
>    A weak model can produce worse *text*. It cannot corrupt *structure*.

## Why

Most translation tools either (a) call a paid cloud API, or (b) demand
you hand-edit catalogs in formats built for translators, not developers.
`i18n-harness` does neither: a local model server (Ollama by default)
runs on your laptop, a CLDR-driven validation gate guarantees the catalog
round-trips byte-stably, and a per-`(backend, locale)` quality metric
tells you empirically how well the local model is doing for each language
pair.

## Status

**Pre-alpha.** The Cargo workspace is bootstrapped (Phase 0) and the
architectural contracts are next (M0 — Qt `.ts` round-trip). See
[`docs/implementation_plan.md`](docs/implementation_plan.md) for milestone
scope and [`docs/initial_design.md`](docs/initial_design.md) for the full
design.

## Planned scope

**Catalog formats:** Qt Linguist `.ts` (M0), gettext / Lingui PO (M4),
ICU-JSON for react-intl / i18next (M4).

**Translation engines:** `manual` — no model (M2); `ollama` — local
Gemma 4 by default (M2); `openai-compatible` — covers vLLM, LM Studio,
and cloud APIs (M4). Plus a two-phase CLI
(`harness export-batch` / `harness import-batch`) for using Claude Code,
Copilot, or any out-of-process agent as the translator.

**Locales:** `en` source and `de_DE` target through M2; `es_ES` and
`zh_Hans` arrive in M3 as a single config-row change — data, not code.

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
│   ├── adapter-qt/     # Qt .ts adapter (reference adapter)
│   ├── adapter-react/  # React adapter over catalog serializers
│   └── cli/            # `harness` binary
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
