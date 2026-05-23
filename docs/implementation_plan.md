# i18n-harness — implementation plan

Mirror of the working plan agreed in plan mode at the start of the project.
Refer to `docs/initial_design.md` for the architectural intent; this file
is the milestone-level execution plan derived from it.

## Context

`i18n-harness` is a brand-new local-first translation tool: install a local
LLM (Ollama + Gemma 4 by default), point it at your translation catalogs
(`.ts`, PO, ICU-JSON), translate UI strings end-to-end offline. Rust core
does deterministic parse/validate/write-back; LLM only produces text; a
CLDR-driven validation gate is the trust boundary.

This plan implements the design with the following deliberate tightening:

- **Lab framing.** Maintainer has no real catalog yet; the immediate goal
  is to see how local Gemma 4 handles translation. M0 (round-trip) and M2
  (per-`(backend, locale)` quality metrics) carry most of the value;
  everything else is structure to make those measurements honest.
- **CLI-first.** Tauri UI is deferred until after the CLI proves the full
  loop. M3 in the original doc shifts later.
- **Single target locale through M2.** `en → de_DE` only; `es_ES` and
  `zh_Hans` come in with the second-format/second-backend wave to validate
  the "locale as data" claim by being trivial to add.
- **No logs code.** §9 stays design-only in the doc; no crate, no fixtures.
- **Agent path is a two-phase CLI**, not a `TranslationBackend`. Keeps the
  trait sync and deterministic-shaped.
- **Placeholders normalize to ICU on extract.** Per-format converters are
  first-class with their own round-trip tests; backend prompts stay uniform.
- **State is hybrid TOML + JSONL.** Glossary as TOML (git-tracked,
  human-editable); metrics and batch state as JSONL in a `.i18n-harness/`
  per-project dir (.gitignored). No DB dependency.

## Resolved §13 open decisions

| # | Decision | Choice |
|---|---|---|
| 1 | `.ts` XML crate | `quick-xml` with hand-managed round-trip |
| 2 | ICU impl | focused MessageFormat parser (plural/select arity + placeholder set only), not full ICU4X |
| 3 | Intermediate | JSONL, per-locale file under `.i18n-harness/intermediate/` |
| 4 | Batching | deterministic batch size (config; default 32), stable ordering by `(file, unit-id)`, resume key = `(file hash, batch idx)` |
| 5 | Backend build order | `manual` → `ollama` → `openai-compatible`; `agent` becomes two-phase CLI |
| 6 | Model default | Gemma 4 E4B default, E2B fast; chosen per `(backend, locale)` metric not assumption |
| 7 | Locale variants | `de_DE` (Sie); `zh_Hans` and `es_ES` (usted) confirmed at the moment they land, not earlier |
| 8 | Format order | PO first (Lingui + gettext), then ICU-JSON |
| 9 | Logs scheme | deferred — design-only |
| 10 | License | Apache-2.0 (patent grant + aligns with Gemma 4's licensing) |

## Phased plan

### Phase 0 — Repo bootstrap (done)

Cargo workspace, CLAUDE.md, LICENSE, NOTICE, README, `.gitignore`,
`fixtures/` skeleton, CI workflow, `.claude/agents/`, `.claude/skills/`.

### M0 — Core + Qt round-trip (blocking)

The load-bearing milestone. Nothing else proceeds until this is green.

- `crates/core`: `Unit` shape (id, source, target, placeholders, plural-
  arity, flags, provenance), `Intermediate` (JSONL line = unit), batch
  types.
- `crates/locales`: locale records as data + CLDR plural-arity tables.
  Only `de_DE` and source `en` populated.
- `crates/adapter-qt`: `.ts` parser/writer using `quick-xml` with hand-
  managed round-trip. Preserve `<location>`, `vanished`/`obsolete`,
  significant whitespace, message state. `unfinished` → `finished` on
  apply; never touch non-target states. Placeholder normalizer Qt
  `%1`/`%n` ↔ ICU `{1}`/`{count}`.
- `crates/cli`: `harness extract`, `harness apply`, `harness round-trip`.
- Fixtures: hand-crafted `.ts` files covering placeholder kinds, plural
  arities, accelerators, comments, escapes, whitespace, CDATA,
  `<extracomment>`, `vanished`/`obsolete` states. Plus 1–2 small files
  borrowed from a permissively-licensed Qt OSS project (attribution in
  `fixtures/README.md`).
- Tests: `extract → apply` with zero unit changes must be byte-identical
  on disk for every fixture. Property test for placeholder normalizer:
  `qt → icu → qt` is identity on a generated corpus.

**Verification:** `cargo test -p i18n-harness-adapter-qt` green;
`harness round-trip fixtures/qt/*.ts` reports zero diffs.

### M1 — Validation gate + metrics events

- `crates/gate`: hard checks (placeholder multiset, plural/select arity
  from CLDR, ICU parses, non-empty when finished), soft checks
  (accelerator, length-warn ratio, CJK punctuation tolerance, placeholder
  agreement-risk). Model-supplied flags merged into `unit.flags`.
- Metrics events: structured records `{backend, locale, event_type,
  unit_id, timestamp}` appended to `.i18n-harness/metrics.jsonl`.
  Event types: gate reject, soft warning, human edit (M3+), retry.
- Fixture tests for every gate rule, organized as a table.

**Verification:** `cargo test -p i18n-harness-gate` covers each rule with
a positive and negative fixture; running gate on M0 fixtures with
intentional corruption produces the expected flags.

### M2 — Backends + glossary + first real translations

- `crates/glossary`: TOML schema, per-locale terms, `do_not_translate`,
  register/variant header. Loader + validator.
- `crates/backend`: trait + `manual` (no model) + `ollama` (HTTP
  `localhost:11434`, `num_ctx` explicit, streaming optional). Glossary
  injected into prompt as context block; prompt template is named and
  versioned under `crates/backend/prompts/`.
- CLI: `harness translate --backend ollama --locale de_DE <catalog>` runs
  the loop end-to-end. Never overwrites source until accepted.
- First real run: a Qt fixture translated en → de via local Gemma 4 E4B;
  metrics captured; results manually inspected.

**Verification:** Loop completes on a small fixture without false hard-
check rejections; metrics file contains events for every unit; produced
`.ts` file passes the M0 round-trip when re-loaded.

### M3 — Tauri desktop app

Lands once M0–M2 are stable and the CLI loop is honest.

- Add `ui/` Tauri + React + Vite + TS. Tauri commands are thin wrappers
  over the library API — zero translation/XML/gate logic in command
  handlers.
- Primary surfaces: glossary editor, flag-triage dashboard, per-
  `(backend, locale)` metrics view, source-vs-proposed review.
- Add `es_ES` and `zh_Hans` locale data + CLDR arity (the "locale as
  data" payoff). Fixture coverage extends.

**Verification:** Tauri app loads a Qt fixture, runs translation through
Ollama, surfaces flagged units, allows edit + accept, writes back a
round-trip-clean `.ts`.

### M4 — Reach: PO/ICU-JSON serializers + openai-compatible backend

- `crates/catalog`: `CatalogFormat` trait + PO serializer (Lingui +
  gettext) + ICU-JSON serializer (react-intl + i18next-ICU). Both go
  through the same ICU placeholder normalizer pattern as Qt; both ship
  with a round-trip fixture suite.
- `crates/adapter-react`: uses the catalog serializers (no new adapter
  logic).
- `openai-compatible` backend (covers vLLM, LM Studio, cloud keys).
- Two-phase agent CLI: `harness export-batch <out.json>` and
  `harness import-batch <in.json>`; documents the format and how to
  drive it with Claude Code / Copilot.

**Verification:** M0 round-trip suite extends to PO and ICU-JSON; the
same en→de translation produces equivalent results across two backends.

### M5 — Logs phase (design-only)

§9 mini-design stays in `docs/initial_design.md`. No code, no crate, no
fixtures land. If the maintainer's needs change, this becomes its own
design pass.

## Verification (end-to-end)

After each milestone:

- `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
  passes.
- `harness round-trip fixtures/**/*` reports zero byte-diffs on every
  fixture (M0's contract holds forever).
- Gate fixture matrix grows monotonically; no rule is silently removed.
- For M2+: one real translation run on a fixture using the maintainer's
  local Ollama+Gemma 4, with metrics captured and inspected.

## Out of scope (explicit non-goals)

- No cloud-only or paid-API-only features.
- No telemetry phoning home.
- No bundled credentials or secrets.
- No logs-phase code (design-only).
- No UI in M0–M2.
- No second target locale before M3.
- No `unsafe` code outside FFI boundaries (Tauri exempt only where it
  must be).
