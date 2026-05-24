# i18n-harness — Product roadmap

The translator-facing roadmap. Everything in this file is in service of
"a translator opens a project and does actual translations" — not
backend benchmarking or maintainer research (that lives in
[`roadmap-lab.md`](roadmap-lab.md)).

For the architectural intent, see [`initial_design.md`](initial_design.md).
For the day-to-day working contract, see [`../CLAUDE.md`](../CLAUDE.md).

## What a translator can do today

Open a single Qt `.ts` file, edit unit targets inline, ask local Gemma 4
(via Ollama) to translate one unit at a time, save the catalog back to
disk with a byte-stable round-trip guarantee, and edit the project
glossary in a separate panel. Shipped in M0–M3.

## What a translator cannot yet do (the M4 gap)

- Open a *project* with multiple catalogs, locales, and shared glossary.
- Round-trip PO or ICU-JSON catalogs (only Qt today).
- Have the LLM flag units that need human attention.
- Triage a project-wide "needs review" queue.
- Bulk-translate every untranslated unit in a locale with cancellable
  progress.
- See per-project quality numbers (acceptance rate, edit rate per
  locale) and turn human corrections into a tuning loop that improves
  the project's prompt.
- Use a theme that looks like a professional tool.

M4 closes those gaps.

---

## Shipped milestones (M0–M3)

### M0 — Core + Qt round-trip ✓

`crates/core`, `crates/locales`, `crates/adapter-qt`, `crates/cli`:
`extract → apply` byte-identical on every fixture. The load-bearing
contract; never goes red without immediate rollback.

### M1 — Validation gate + metric events ✓

`crates/gate`: CLDR-driven hard checks (placeholder multiset, plural
arity, ICU parses, non-empty when finished) and soft checks
(accelerators, length-warn ratio, CJK punctuation, agreement risk,
HTML markup). Structured events appended to `.i18n-harness/metrics.jsonl`.

### M2 — Backends + glossary + first real translations ✓

`crates/glossary` (TOML loader/validator), `crates/backend`
(`TranslationBackend` trait + `manual` + `ollama`). End-to-end CLI loop:
`harness translate --backend ollama --locale de_DE <catalog>`.

### M3 — Desktop app shell ✓

`ui/` Tauri 2 + React + Vite + TS. Single-catalog editor with unit list
+ editor + inspector triptych, translate-via-Ollama, save/discard,
glossary editor panel, JSONL metrics viewer. Single-file-at-a-time
model.

---

## In flight — M4 (Product turn)

The shift: stop being a file-and-metrics inspector, become a
**project-scoped translation workspace**. Each M4.x sub-milestone is a
PR-sized piece.

### M4.0 — Theme migration (groundwork)

Adopt the "Technical Journal" light / "Observatory" dark palette
(HSL tokens, blueprint blue accent) the maintainer dropped in at
`ui/src/index.css`. Replace `ui/src/styles/tailwind.css` with a single
stylesheet that uses the new palette but keeps the existing token
namespace so no component code has to change. Add a topbar light/dark
toggle that persists per-OS-user (not per-project — themes are
personal). Delete the unused `ui/src/index.css` import.

Verification: app loads under both themes; all four unit-state badges
have ≥4.5:1 contrast against their surfaces; focus rings match
`--primary`; `bun run build` + `bun run typecheck` + `bun run lint`
green.

### M4.1 — `crates/project` (manifest, discovery, validation)

New crate. Owns the project model:

- `ProjectManifest` (serde over `toml_edit` so user comments and
  ordering round-trip), `Project` (loaded + resolved paths + catalog
  index), `ProjectError`.
- `Project::open(root: &Path)` — read manifest, validate every declared
  catalog and locale.
- `Project::discover(root: &Path)` — no manifest yet: scan for `.ts`,
  `.po`, `.json`, infer a draft (locales from filename suffix or
  per-format header).
- `Project::add_catalog`, `remove_catalog`, `update_locale`,
  `set_backend`, `save_manifest` — TOML round-trip-preserving edits.
- Translation-memory writer/reader: `corrections.jsonl` (append-only,
  one line per accepted edit), `curated.toml` (lists correction IDs +
  per-pair notes).

On-disk layout the manifest implies:

```
my-app/
  i18n-harness.toml          # committed — project manifest
  glossary.toml              # committed — project glossary
  translations/
    app_de.ts
    app_fr.ts
    settings_de.po
    onboarding_de.json
  prompts/                   # committed (optional) — per-locale overrides
    de_DE.txt
  .i18n-harness/             # gitignored — runtime state
    metrics.jsonl
    corrections.jsonl
    curated.toml
    state/batches.jsonl
    tuning/2026-05-24T12-00-00/
```

Manifest schema (v1):

```toml
[project]
name = "my-app"
schema = 1

[locales.de_DE]
register = "neutral"        # neutral | formal | informal
variant = "standard"
length_warn_ratio = 1.4

[locales.fr_FR]
register = "formal"

[[catalogs]]
path = "translations/app_de.ts"
format = "qt-ts"            # qt-ts | gettext-po | icu-json
locale = "de_DE"

[[catalogs]]
path = "translations/app_fr.ts"
format = "qt-ts"
locale = "fr_FR"

[glossary]
path = "glossary.toml"

[backend.default]
kind = "ollama"
model = "gemma4:e2b"
host = "http://localhost:11434"

[prompts]
template_dir = "prompts"    # optional; falls back to crate-embedded v2
```

### M4.1.5 — Core extensions: `Unit.source_hash` + `ReviewStatus`

Two small additive changes to `crates/core` (and the Qt adapter that
populates them) that the post-M4.1 milestones depend on. Driven by
external design input in
[`feedback-2026-05-24-chatgpt.md`](feedback-2026-05-24-chatgpt.md)
§§1–2 and the decisions captured during the M4.1c session.

- **`Unit.source_hash: Option<String>`** — short SHA-256 over
  `(source ‖ disambiguation ‖ comment ‖ location)`, populated by
  `adapter-qt::extract`. On re-open, comparing the hash to a stored
  prior value lets the UI surface "source changed since you last
  approved this translation" without re-translating. The M4.3 sidebar
  badge and the M4.7 review queue both rely on it.
- **`Unit.review_status: Option<ReviewStatus>`** —
  `New | MachineTranslated | NeedsReview | Reviewed | Approved | Locked | Rejected | Conflict`,
  orthogonal to Qt's structural `state`. The Qt round-trip stays
  byte-stable (review status is gitignored project state under
  `.i18n-harness/`, never written into the `.ts` file); the editor
  uses review status for filtering and the M4.7 review queue.
- The new fields default to `None` so existing fixtures and the
  round-trip contract are unchanged.

Verification: round-trip suite stays byte-identical; an extra unit
test confirms `source_hash` is stable across identical extracts and
changes when the source text changes.

### M4.2 — Tauri command surface refactor + CLI `init` / `open`

Replace the file-centric `open_catalog` / `save_catalog` API with a
project-scoped one. Lands in four sub-slices so each PR stays small
enough to review in one sitting.

#### M4.2a — Open-project surface + CLI `init` / `open` ✓

Strictly additive: existing file-centric commands keep working so the
M3 UI continues to function while M4.3 (the new shell) is in flight.

Tauri commands:

- `open_project(root) -> { summary: ProjectSummary, warnings }`
- `discover_project(root) -> DraftManifest`
- `create_project(root, draft) -> { summary, warnings }`
- `close_project()`
- `current_project_summary() -> Option<ProjectSummary>`
- `list_catalogs() -> Vec<CatalogRef>`
- `save_manifest()` — persists the in-memory `toml_edit` document
  that `Project::add_catalog` / `update_locale` / `set_backend`
  mutate.

Side effects: opening a project pins its glossary (if declared) into
the existing glossary slot so `translate_unit` benefits immediately,
and clears the stand-alone catalog slot so the two surfaces never
disagree about which file is "active".

CLI additions:

- `harness init <dir>` — discover catalogs under `<dir>` and write
  `i18n-harness.toml` (refuses to overwrite without `--force`).
- `harness open <dir>` — load and validate a manifest, print summary.

#### M4.2b — Per-catalog edit through project ✓

- `open_catalog_in_project(path) -> CatalogResponse`
- `save_catalog_in_project(path)`
- `save_all_dirty() -> Vec<SaveSummary>`
- `apply_review_state(path)` — folds `review.jsonl` into the open
  units (uses `Project::apply_review_state` from M4.1.5).

#### M4.2c.1 — Translate + correction recording ✓ (shipped)

Single-shot project-scoped commands over the IPC bridge:

- `translate_unit_in_project(catalog_path, unit_id)` — project-scoped
  sibling of `translate_unit`; picks up glossary, locale config, and
  backend kind from the project; updates the project-catalog store.
- `record_correction_in_project(req)` — appends an accepted human edit
  to `corrections.jsonl`; returns the content-addressed correction id.
- `list_corrections_in_project(filter)` — reads and filters `corrections.jsonl`.
- `promote_correction_to_curated(id, note)` — adds a correction to `curated.toml`.
- `un_curate_correction(id)` — removes a correction from `curated.toml`.
- `set_review_status_in_project(catalog_path, unit_id, input)` — appends
  a review event to `review.jsonl` and updates the in-memory unit.

#### M4.2c.2 — Bulk translate with cancellation (planned; depends on M4.8 cancellation primitive)

- `translate_batch(catalog_path, scope, cancel_token)` — streams
  progress via Tauri events. Deferred until the cancellation primitive
  design is settled by rust-architect.

#### M4.2d — Evaluation + tuning bundle (planned, depends on M4.9)

- `run_evaluation(prompt_path?) -> EvaluationReport`
- `export_tuning_bundle() -> Path`

The old `open_catalog` / `save_catalog` commands stay as thin shims
through M4.2b/c so power-user file-open paths and the existing CLI
still work.

### M4.3 — UI shell: sidebar + locale chips + view tabs + home screen

#### M4.3a — Project mode foundation ✓ shipped

Home screen (recent projects, "Open folder", "Create from folder"), project
sidebar (catalog list, close button), ProjectTopBar (project name, view tabs,
theme toggle), project-scoped translate/save/discard/translate wrappers, and
TS types + Tauri wrappers for all M4.2 commands. M4.3b (locale chips, dirty
pills), M4.3c (Settings view), and M4.3d (Quality view) follow.

New layout:

```
┌────────────┬──────────────────────────────────────────────────────┐
│ PROJECT    │ [my-app]  [de_DE • fr_FR • ja_JP]  Translate  Glossary│
│ Catalogs   │                                    Quality  Settings  │
│  ▾ translations/                                                   │
│    app_de  │ ┌──────────┬─────────────────┬───────────────────────┐│
│    app_fr  │ │Unit list │  Editor         │  Inspector            ││
│    ui_de   │ │          │                 │                       ││
│  ▾ glossary│ │          │                 │                       ││
│ Recent     │ │          │                 │                       ││
└────────────┴─└──────────┴─────────────────┴───────────────────────┘┘
```

- **Home screen** when no project is open: recent projects list,
  "Open folder", "Create from this folder". No catalog-by-itself entry
  point in the main UI.
- **Locale chips** in the topbar filter the catalog sidebar to that
  locale's files. Switching locale = switching to the sibling catalog
  for that locale (`app_de.ts` → `app_fr.ts`).
- **View tabs**: Translate (the unit list + editor + inspector
  triptych), Glossary, Quality, Settings (manifest editor — locales,
  catalogs, backend, prompts). Translate is the default.
- **Project settings** is its own view: edit locales, add/remove
  catalogs, set backend, edit prompt template path.

### M4.4 — PO serializer

New `crates/catalog/src/po.rs` implementing the `CatalogFormat` trait
(designed in this milestone since the trait itself doesn't exist yet).
Ships:

- Reader + writer with byte-stable round-trip over a fixture corpus at
  `fixtures/po/`.
- Placeholder converter gettext `%s`/`%d`/`%(name)s` ↔ ICU `{n}` with
  a property test on the converter pair.
- Plural-form reconciliation between PO's `Plural-Forms` header and
  the locale's CLDR arity.

### M4.5 — ICU-JSON serializer; wire `crates/adapter-react`

New `crates/catalog/src/icu_json.rs` with the same shape as PO:
reader + writer + round-trip fixtures at `fixtures/icu-json/` + an
identity-with-validation converter (react-intl ICU is already ICU).
`crates/adapter-react` becomes a thin glue crate that dispatches by
file shape.

### M4.6 — Human-attention flagging

Switch the Ollama prompt from text-only to **strict JSON output**.
New template `crates/backend/prompts/ollama-translate-v2.txt`:

```
You MUST respond with one JSON object on a single line:
{"translation":"<the translated string>",
 "flags":[{"kind":"ambiguous_source","note":"..."}],
 "confidence":0.87}

Allowed flag kinds:
- ambiguous_source     : source could mean multiple things
- insufficient_context : source is too short/generic to translate confidently
- idiom                : source uses idiom/wordplay; literal translation loses meaning
- low_confidence       : you self-report unsure for unspecified reason
- brand_term           : term looks like a product/brand name not in glossary
- tone_mismatch        : source register is ambiguous or hard to carry into target
```

Parsing: strict serde schema. Malformed responses become a gate finding
(`backend-malformed-response`, hard severity) — no silent retry; surface
the failure so the prompt can be tuned.

Data model: extend the existing `Flag` enum in
`crates/backend/src/outcome.rs` to include `BrandTerm` and
`ToneMismatch`; propagate into `Unit.flags`. Add `Unit.confidence:
Option<f32>` (shown in inspector; does not block).

Auto-promotion rule: a unit may only auto-promote to `Finished` if
`flags.is_empty()` AND gate is clean AND target is complete. Flagged
units land with `review_status = NeedsReview` (see M4.1.5) and stay
`Proposed`. The translator clears flags via an explicit "Accept"
action, which moves `review_status` to `Reviewed` (or `Approved`
on Save All).

UI: inspector renders flags as severity-style chips with the model's
per-flag note; unit list shows a flag badge next to the state badge.

### M4.7 — Review queue

- "Needs review" filter on the unit list inside the Translate tab,
  driven by `Unit.review_status == NeedsReview` (M4.1.5) and the
  per-unit `flags` from M4.6.
- "Source changed" filter, driven by comparing the current extract's
  `Unit.source_hash` against the last-saved value.
- Project-wide flag count badge in the sidebar ("12 flagged across
  project, 3 source-changed"); clicking opens a virtual catalog view
  listing every flagged or source-changed unit across catalogs and
  locales.

### M4.8 — Bulk translate

Per-catalog × per-locale "Translate all untranslated" with live
progress, cancel mid-run, and flagged units routed to the review queue.
Backend job system in Rust (channel + cancellation token); Tauri events
stream progress to the UI. No cross-catalog runs in v1.

### M4.9 — Quality tab + in-app prompt evaluation

Replaces today's Metrics tab. Per-project, per-locale only.

Sections:

1. **Headline numbers per locale** — % accepted-as-is / % edited / %
   rejected / % flagged; 30-day acceptance-rate sparkline.
2. **Translation memory** — searchable `(source, mt_proposal,
   human_target, provenance)` table; each row exposes which prompt
   version + model + glossary version produced its proposal (from
   `CorrectionProvenance`, captured at correction time per M4.1d).
   "Promote to golden" per row; bulk promote from a filter.
3. **Curated set** — the project's tuning examples. Editable notes;
   size counter; "Export tuning bundle" button.
4. **Prompt evaluation** — current prompt template + version + last
   evaluated score. "Run evaluation" re-runs the current prompt over
   the curated set using the local Ollama backend and computes
   per-locale acceptance rate. Because every correction carries its
   provenance, evaluations can compare "current prompt vs prompt that
   produced this golden example" honestly. Honest about cost (shows
   estimated runtime before starting).
5. **Tuning bundle export** — writes
   `.i18n-harness/tuning/<ISO-timestamp>/`:
   - `examples.jsonl` — every curated pair
   - `prompt.txt` — current template, verbatim
   - `score.json` — latest per-locale score
   - `locales.toml` — locale records from the project
   - `README.md` — bundle schema spec for the consuming Claude Code
     skill

### M4.10 — Tuning-skill contract

Ship the bundle schema as a stable contract and a sibling
[`skills/tune-i18n-prompt/`](../skills/tune-i18n-prompt/) Claude Code
skill that consumes it:

- Reads a tuning bundle path.
- Diffs `mt_proposal` vs `human_target` across examples; identifies
  failure patterns per locale.
- Drafts a new prompt template adhering to the v2 JSON output contract.
- Writes the candidate to `prompts/<locale>.txt` in the project (or
  proposes a global replacement of `prompts/default.txt`).
- Asks the user to run "Run evaluation" in the app to verify the new
  score.

Division of labor: the **local** model translates (the hot loop, fast,
free); a **large** model rewrites the prompt (the slow loop, run on
demand by an external skill the user invokes — Copilot or Claude Code).
The harness owns the bundle format and the in-app evaluation runner.

---

## Out of scope for M4

- Cloud / API-key backends (`openai-compatible`) — covered in M5 if
  pulled forward, otherwise post-M4.
- The Qt `.ts` adapter internals — unchanged; round-trip remains the
  invariant.
- The lab dashboard — see [`roadmap-lab.md`](roadmap-lab.md).
- Multi-project / multi-window — one project per app window in v1.

---

## Verification (rolling)

After each M4.x:

- `cargo test --workspace --all-targets` and
  `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo fmt --all -- --check` all green.
- `cd ui && bun run typecheck && bun run lint && bun audit --prod
  && bun run build` all green.
- Round-trip suite (`crates/adapter-qt/tests/fixtures/` + new
  PO/ICU-JSON fixtures from M4.4/M4.5 onward) is byte-identical on
  every fixture.
- Gate fixture matrix grows; no rule silently removed.
