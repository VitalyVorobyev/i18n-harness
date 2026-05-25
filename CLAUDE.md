# CLAUDE.md — i18n-harness working agreement

A local-first, offline desktop translation harness. Install a local model
(Ollama + Gemma 4 by default), point it at translation catalogs (Qt `.ts`,
PO, ICU-JSON), translate UI strings — no API key, no cloud. The full design
lives in [`docs/initial_design.md`](docs/initial_design.md); the
translator-facing milestones in
[`docs/roadmap-product.md`](docs/roadmap-product.md); the maintainer-facing
lab work in [`docs/roadmap-lab.md`](docs/roadmap-lab.md). This file is the
day-to-day working contract.

## The two invariants (never violate)

1. **Languages, translation engines, and catalog formats are data or plugins
   behind stable extension points.** The core names none of them
   specifically. A new language is a config row; a new engine is a trait
   impl; a new catalog format is a serializer.
2. **The model translates text; deterministic Rust does everything else.**
   Parsing, validation, and write-back never go through the LLM. A weak
   model can produce *worse text* but cannot *corrupt structure* — the
   validation gate guarantees it.

## Two tracks: product and lab

The repo runs two roadmaps in parallel, with strictly separate audiences:

- **Product** ([`docs/roadmap-product.md`](docs/roadmap-product.md)) — what a
  translator user does in the Tauri app: open a project, work through
  units across multiple catalogs and locales, accept LLM suggestions,
  triage flagged ones, and feed corrections into a per-project
  prompt-tuning loop.
- **Lab** ([`docs/roadmap-lab.md`](docs/roadmap-lab.md)) — what the
  maintainer does to evaluate backends and locales empirically across
  projects. Lives in a separate binary / CLI + HTML report; **not**
  shipped inside the translator app.

Translator-facing features live in the translator app and only there.
Lab features live in the lab tool and only there. The shared substrate
is the validation gate, the catalog adapters, the backend trait, and
the per-project `.i18n-harness/` state; everything else is split.

M0–M4 are shipped (M4 closed with M4.5 ICU-JSON + M4.10 tuning bundle).
M5 ships the two-phase agent translation flow: `harness export-batch` /
`harness import-batch` CLI subcommands plus the
[`skills/translate-i18n-batch/`](skills/translate-i18n-batch/) spec for
filling the batch from an external Claude Code / Copilot / Codex
session. **No UI wiring yet** — the agent flow is a developer/CLI
surface until M6 promotes it into Settings + the Translate view.
See `docs/roadmap-product.md` for the sub-milestones.

## Workspace map

| Crate | Purpose |
|---|---|
| `crates/core` | `Unit`, intermediate (ICU) representation, batches, shared errors |
| `crates/locales` | Locale records + CLDR plural-arity tables (data, not code) |
| `crates/gate` | CLDR-driven validation gate; hard + soft checks; metric events |
| `crates/glossary` | TOML glossary loader + validator |
| `crates/backend` | `TranslationBackend` trait + impls (manual, ollama, openai-compatible) |
| `crates/catalog` | `CatalogFormat` trait + PO/ICU-JSON serializers |
| `crates/adapter-qt` | Qt `.ts` adapter (reference adapter; XML round-trip) |
| `crates/adapter-react` | React adapter over the catalog serializers |
| `crates/cli` | `harness` binary; entry point for humans and the Tauri UI |
| `ui/` | Tauri 2 + Vite + React + TS desktop shell ([ui/README.md](ui/README.md)) |
| `ui/src-tauri` | Rust side of the desktop shell — thin wrappers over the library |

`crates/adapter-log/` is *not* present and will not be created until its
milestone — the log adapter was the docs' original "M5 design-only" item
and has been deferred indefinitely. The current M5 (above) is the
agent translation flow; the log work moves to a later, separately-
planned milestone.

## Commands

```sh
# Rust workspace (includes ui/src-tauri as a workspace member)
cargo build --workspace
cargo test  --workspace --all-targets
cargo fmt   --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo doc   --workspace --no-deps --document-private-items
cargo run   -p i18n-harness-cli -- --help

# Desktop UI (Tauri + Vite + React, see ui/README.md for details)
cd ui && bun install
cd ui && bun run typecheck
cd ui && bun run lint           # Biome — single tool for lint + format
cd ui && bun audit --prod       # Production-dep vulnerability scan
cd ui && bun run build
cd ui && bun tauri:dev
```

CI runs all of the above on push and PR. See `.github/workflows/ci.yml`.

## Conventions

- **Clean-room.** Independent of any employer-internal tooling. Design
  only from public specs (Qt `.ts` schema, ICU MessageFormat, Unicode
  CLDR, gettext PO, Ollama HTTP API). Commit messages, docs, and code
  must not reference internal projects or identifiers.
- **No `unsafe`** outside FFI boundaries. Workspace lint enforces this.
- **No bundled credentials.** All keys via env or per-project config.
- **No telemetry phoning home.**
- **The structured catalog is the source of truth.** The intermediate
  (JSONL of ICU-form units) is transient and regenerable from the catalog.
- **Placeholders normalize to ICU on extract.** Per-format converters
  (Qt `%1`/`%n` ↔ ICU `{1}`/`{count, plural, …}`; gettext `%s`/`%d`/`%(name)s`
  ↔ ICU `{n, …}`) live with their adapter/serializer and ship with their
  own round-trip property tests.
- **State persistence:** glossary as `glossary.toml` per project
  (git-tracked, human-editable); metrics + batch state as JSONL under
  `.i18n-harness/` per project (gitignored).
- **No comments-as-narration.** Code says what; comments only say *why*
  when it is non-obvious.

## Round-trip is sacred

For any adapter, `extract → apply` with zero unit changes must be
**byte-identical on disk** for every fixture. This is the M0 contract and
it never goes red without immediate rollback. New fixtures get added to
the round-trip suite, never carved out.

## Adding things — use the project skills

- `add-locale`: new target locale (CLDR arity + locale record + fixture).
- `add-backend`: new `TranslationBackend` impl (crate/module + feature
  flag + smoke test).
- `add-format`: new `CatalogFormat` serializer (module + placeholder
  converter + round-trip fixtures).
- `translate-i18n-batch`: fill `targets.jsonl` for a batch produced by
  `harness export-batch`. Translator-facing; runs in a separate Claude
  Code session pointed at the batch directory.
- `tune-i18n-prompt`: rewrite a prompt template from a tuning bundle
  exported by the app. Translator-facing; runs separately from the
  in-app translation loop.

## Subagents

- `rust-implementer` (Sonnet) — straightforward Rust work that follows an
  existing pattern.
- `rust-architect` (Opus) — load-bearing design and tricky implementations
  (gate, XML round-trip, ICU converters, trait surfaces, batching).
- `ui-implementer` (Sonnet) — Tauri + React + Vite work (M3+ only).

## Outstanding setup notes

- CI workflow uses tagged action refs; pin to commit SHAs before any
  public release tag.
- Repository URL in `Cargo.toml`: `github.com/VitalyVorobyev/i18n-harness`.
