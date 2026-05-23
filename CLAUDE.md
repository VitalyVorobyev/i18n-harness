# CLAUDE.md — i18n-harness working agreement

A local-first, offline desktop translation harness. Install a local model
(Ollama + Gemma 4 by default), point it at translation catalogs (Qt `.ts`,
PO, ICU-JSON), translate UI strings — no API key, no cloud. The full design
lives in [`docs/initial_design.md`](docs/initial_design.md); the current
milestone scope in [`docs/implementation_plan.md`](docs/implementation_plan.md);
this file is the day-to-day working contract.

## The two invariants (never violate)

1. **Languages, translation engines, and catalog formats are data or plugins
   behind stable extension points.** The core names none of them
   specifically. A new language is a config row; a new engine is a trait
   impl; a new catalog format is a serializer.
2. **The model translates text; deterministic Rust does everything else.**
   Parsing, validation, and write-back never go through the LLM. A weak
   model can produce *worse text* but cannot *corrupt structure* — the
   validation gate guarantees it.

## Lab framing

The maintainer has no production translation workload yet. The immediate
purpose is to find out, empirically, how a local Gemma 4 handles
translation into target locales. M0 (byte-stable round-trip) and the
per-`(backend, locale)` quality metric in M1/M2 carry most of the value;
everything else is structure to make those measurements honest. Do not
ship features that are not in service of one of those two things until
M0–M2 are green.

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
| `crates/cli` | `harness` binary; entry point for humans and (later) Tauri UI |

`ui/` (Tauri) and `crates/adapter-log/` are *not* present and will not be
created until their milestones (M3 / M5 design-only).

## Commands

```sh
cargo build --workspace
cargo test  --workspace --all-targets
cargo fmt   --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo doc   --workspace --no-deps --document-private-items
cargo run   -p i18n-harness-cli -- --help
```

CI runs the same commands on push and PR. See `.github/workflows/ci.yml`.

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
