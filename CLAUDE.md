# CLAUDE.md — i18n-harness working agreement

A local-first, offline-by-default desktop translation harness. The
default path is a local model server (Ollama + Gemma 4) with no API key
and no cloud egress; the `openai-compatible` backend supports cloud
LLMs (OpenAI, Anthropic, vLLM, LM Studio) as an opt-in per-project
config, and the two-phase agent CLI (`harness export-batch` /
`harness import-batch` plus the `translate-i18n-batch` skill) delegates
to an external Claude Code / Copilot / Codex session on the user's
machine. Catalogs: Qt `.ts`, gettext PO, ICU-JSON. The full design
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

The current shipping surface: the validation gate, all three catalog
adapters (Qt, PO, ICU-JSON) with byte-stable round-trip, project mode
(multi-catalog manifests + reference catalogs), the Tauri desktop UI
for translation and review, quality eval + tuning bundle export, the
two-phase agent translation CLI plus its skill spec, and deterministic
reference reuse / remainder split / merge for Qt (CLI + UI; copy expert
translations by exact unit-id match, carve the leftover, merge it back —
no model). The agent flow is currently a CLI surface; UI integration
(Settings backend picker + an in-app "Translate via Claude Code"
affordance) is the next visible step. See
[`docs/roadmap-product.md`](docs/roadmap-product.md) for the
sub-milestone history.

## Workspace map

| Crate | Purpose |
|---|---|
| `crates/core` | `Unit`, intermediate (ICU) representation, batches, shared errors |
| `crates/locales` | Locale records + CLDR plural-arity tables (data, not code) |
| `crates/gate` | CLDR-driven validation gate; hard + soft checks; metric events |
| `crates/glossary` | TOML glossary loader + validator |
| `crates/backend` | `TranslationBackend` trait + impls (manual, ollama, openai-compatible) |
| `crates/catalog` | `CatalogFormat` trait + PO/ICU-JSON serializers |
| `crates/adapter-qt` | Qt `.ts` adapter (reference adapter; XML round-trip + subset writer) |
| `crates/adapter-react` | React adapter over the catalog serializers |
| `crates/project` | Project manifest (multi-catalog + `[[references]]`), path/locale resolution, per-project state |
| `crates/reuse` | Deterministic reference-reuse / remainder-split / merge over Qt catalogs (no model) |
| `crates/cli` | `harness` binary; entry point for humans and the Tauri UI |
| `ui/` | Tauri 2 + Vite + React + TS desktop shell ([ui/README.md](ui/README.md)) |
| `ui/src-tauri` | Rust side of the desktop shell — thin wrappers over the library |

`crates/adapter-log/` is *not* present and will not be created until
the structured-log adapter milestone is planned — that work is
currently deferred. Translator-facing catalog formats (Qt, PO,
ICU-JSON) ship; logs are a separate concern.

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
**byte-identical on disk** for every fixture. This is the load-bearing
adapter contract and it never goes red without immediate rollback. New
fixtures get added to the round-trip suite, never carved out.

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

**Default to the main context.** Most work here — bug fixes, small
features, doc updates, and especially changes that span the Rust
backend *and* the `ui/` frontend together (a DTO field plus the React
handler that reads it, a command plus its mock) — is faster, more
coherent, and easier to verify in a single context than split across
agents. A round trip through a subagent costs a full context reload and
a handoff; for a change you could finish yourself in a few edits, that
overhead is pure loss and tends to produce a worse result.

Reach for a custom subagent only when delegation genuinely pays for
itself: a large, well-specified slice contained to **one** domain, or
several independent slices that can run in parallel. When in doubt, do
it inline.

- `rust-architect` (Opus) — load-bearing *design* and the trickiest
  implementations (gate, XML round-trip, ICU converters, trait
  surfaces, batching). Use when getting the abstraction wrong would
  cascade through the workspace — not for routine changes near those
  files.
- `rust-implementer` (Sonnet) — a sizeable, self-contained Rust slice
  that follows an established pattern and has a clear spec.
- `ui-implementer` (Sonnet) — a sizeable, self-contained `ui/` slice
  (Tauri + React + Vite). A small UI tweak alongside a backend change
  is *not* this — keep those together in the main context.

## Outstanding setup notes

- CI workflow uses tagged action refs; pin to commit SHAs before any
  public release tag.
- Repository URL in `Cargo.toml`: `github.com/VitalyVorobyev/i18n-harness`.
