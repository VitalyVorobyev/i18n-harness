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
- ~~Bulk-translate every untranslated unit in a locale with cancellable
  progress.~~ (shipped M4.8)
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

#### M4.2c.2 — Bulk translate with cancellation ✓ shipped

Cancellation primitive + job registry in the Tauri shell. The primitive
is a sticky one-shot `CancellationToken` (cloneable across threads,
polled cooperatively between units); the registry is a UUID-keyed map
of active jobs. Two new Tauri commands wrap them:

- `translate_batch_in_project(catalog_path, scope) -> { job_id, total }` —
  resolves locale/glossary/backend, claims a per-`(catalog, locale)`
  exclusion slot, registers a job, and spawns an OS worker thread.
  Returns synchronously. The worker translates unit-by-unit via the
  shared `translate_one` helper (released `project_catalogs` lock
  across the network call), emits `batch-progress-<job_id>` after each
  unit, and emits exactly one terminal event:
  `batch-completed-<job_id>` on clean exit OR observed cancellation, or
  `batch-failed-<job_id>` on mid-batch backend error.
- `cancel_translation(job_id) -> bool` — sets the cancellation flag for
  the named job; the worker observes it before its next unit and exits.
  Feature-agnostic (always available; the no-`ollama` build of the UI
  cannot start jobs but can cancel them).

Out of scope (deferred to later slices): the bulk-translate UI surface
(M4.8 will subscribe to these events), `BatchScope::All` re-translating
Finished units (needs a policy decision on Finished→Proposed demotion),
parallel network calls (no rate-limiting design yet), and on-disk
resumability (the registry is in-process only — restart drops the job).

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
TS types + Tauri wrappers for all M4.2 commands.

#### M4.3b — Locale chips + dirty pills ✓ shipped

Locale chips rendered in `ProjectTopBar` from `summary.locales`. Multi-select:
clicking a chip toggles it in `activeLocaleFilter` lifted to App.tsx; empty
set means "show all". A "Clear filter" affordance appears only when the set is
non-empty. The `ProjectSidebar` accepts `activeLocaleFilter` and renders only
catalogs whose locale is in the set (with a count badge showing "N of M
catalogs" when filtered). Per-catalog dirty pills (orange dot + "unsaved"
sr-only text) are already rendered by the sidebar; the dirty set is threaded
from App.tsx. Sibling quick-switch: when exactly one locale chip is active and
the user is viewing a catalog for a different locale, the filter change also
switches to the sibling catalog for that locale (stem heuristic: strip trailing
`_<locale>` from the manifest-relative basename). M4.3c (Settings view) is
shipped; M4.3d (Quality view) is shipped.

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

#### M4.3c — Settings view (manifest editor) ✓ shipped

Seven Tauri mutation commands (`add_catalog_to_project`,
`remove_catalog_from_project`, `update_locale_in_project`,
`remove_locale_from_project`, `set_backend_in_project`,
`set_glossary_in_project`, `set_prompts_in_project`) — each acquires
`&mut Project`, calls the library method, calls `save_manifest()`, and
returns a fresh `ProjectOpenResponse` so the UI re-renders without a
second round trip. `remove_catalog_from_project` also evicts the removed
catalog from the `project_catalogs` store.

UI: `ui/src/components/ProjectSettings/ProjectSettings.tsx` — four cards
(Project meta read-only, Locales table with inline add/remove, Catalogs
table with file-picker "Add catalog" flow, Backend form). Every mutation
auto-persists; a transient "Saved" badge confirms each write. App.tsx
wires `handleProjectMutation` to update the in-memory `summary` after
each command. Directly addresses user feedback #6 (add/remove `.ts` files
from a project without editing TOML by hand).

### M4.4 — PO serializer ✓ shipped

New `crates/catalog/src/po/` implementing the `CatalogFormat` trait — the
trait itself ships in this slice (in `crates/catalog/src/format.rs`).
Shipped:

- `CatalogFormat` trait with `id`, `extensions`, `extract`, `apply`,
  `placeholders_to_icu`, `placeholders_from_icu`. Qt stays in its own
  crate; a `BackingCatalog` enum on the Tauri side dispatches Qt vs.
  catalog-crate flavors uniformly. ICU-JSON (M4.5) adds one more arm.
- PO reader + writer with byte-stable round-trip over `fixtures/po/`
  (six fixtures: singular, Polish 3-form, Arabic 6-form, msgctxt,
  mixed placeholders, multi-line continuations).
- Placeholder converter gettext `%s`/`%d`/`%(name)s`/`%1$s` ↔ ICU `{n}`,
  with a property test on the converter pair and a per-unit
  conversion-specifier table so `%d` writes back as `%d` (not `%s`).
- Plural-form reconciliation (`reconcile_plural_arity`): PO header
  `nplurals` and locale CLDR arity are compared; mismatch logs a warning
  and prefers the locale for new writes without rewriting existing
  `msgstr[N]` blocks (the user's header stays authoritative).
- `open_catalog_in_project`, `save_catalog_in_project`,
  `save_all_dirty`, `discard_changes_in_project`, and
  `scan_project_review_state` all dispatch by format; opening a PO
  catalog through the manifest now works end-to-end.

### M4.5 — ICU-JSON serializer; wire `crates/adapter-react` ✓ shipped

`crates/catalog/src/icu_json/` with the same shape as PO: a hand-written
single-pass JSON tokenizer that captures the byte range of every leaf
string value (`parse.rs`), a splice-based writer that uses
`serde_json::to_string` for escape correctness (`write.rs`), and an
identity-with-validation placeholder converter that verifies ICU brace
balance to catch hand-edit corruption early (`placeholder.rs`).
Round-trip fixtures at `fixtures/icu-json/`: `flat.json`, `nested.json`,
`with_plural.json`, `mixed_quoting.json`, `empty_object.json`.
`ExtractState` gains an `IcuJson` variant; the `format_by_id` /
`open_catalog_by_format` dispatcher in the catalog crate routes
`"icu-json"`.

`crates/adapter-react` becomes a thin extension-based dispatcher
(`format_for_path` → `IcuJsonFormat` for `.json`) with `extract` and
`apply` helpers. The Tauri layer's `BackingCatalog::Generic` now carries
the manifest-declared format alongside the catalog so `apply` dispatches
back to the same serializer that produced the extract state;
`extract_for_format` and `scan_project_review_state` route ICU-JSON
through the catalog crate without the previous "not implemented" stub.

**M4 closes here.** The translator can now open a project that mixes
Qt, PO, and ICU-JSON catalogs, translate units in any of them, and rely
on byte-stable round-trip across the full format matrix. Remaining
roadmap items (M5: cloud / API-key backends) are deferred to a future
slice and intentionally out of scope for the product turn.

### M4.6 — Human-attention flagging

Split into two slices: the Rust-side protocol (M4.6.1) and the UI
consumption layer (M4.6.2).

#### M4.6.1 — Rust-side flagging protocol ✓ shipped

Switched the Ollama prompt from text-only to **strict JSON output**
(`crates/backend/prompts/ollama-translate-v2.txt`, the new default). The
v1 plain-text template remains available via
`OllamaBackend::with_template_v1()` for the CLI's `--prompt` override.

```
{"translation":"<the translated string>",
 "flags":[{"kind":"ambiguous-source","note":"..."}],
 "confidence":0.87}
```

Allowed flag kinds (kebab-case, matching `Flag` serde renderings):
`ambiguous-source`, `insufficient-context`, `idiom`, `low-confidence`,
`brand-term`, `tone-mismatch`. Gate-produced kinds
(`placeholder-mismatch`, `plural-arity-mismatch`, etc.) are forbidden in
the model's output; the parser rejects them as malformed.

Parsing is strict serde. Malformed responses become a hard gate finding
`BackendMalformedResponse(reason)` (`Flag::BackendMalformedResponse`,
`Hard` severity, surfaced inline alongside the unit) instead of being
lost behind a generic backend error. The Tauri command
`translate_unit_in_project` returns `Ok` with a synthesized `GateReport`
in that case; only network / backend-unavailable failures still bubble
up as `Err`. No silent retry, no fallback — the failure is visible so
the prompt can be tuned.

Data model:

- `Flag` (in `crates/core/src/flag.rs`) gained `BrandTerm` and
  `ToneMismatch` (semantic severity) and `BackendMalformedResponse`
  (hard severity). New finding detail type
  `BackendMalformedResponseDetail { reason }` in the gate.
- `Unit` (in `crates/core/src/unit.rs`) gained `confidence:
  Option<f32>` and `flag_notes: BTreeMap<Flag, String>` — both default
  to empty / `None`, so existing JSONL files deserialize unchanged.
- `TranslationOutcome::Translated` gained `confidence` and
  `flag_notes`; `Failed` gained a `FailureKind` enum (`Unspecified`,
  `Network`, `BackendUnavailable`, `MalformedResponse`) so the caller
  can route by category.

Auto-promotion rule **disabled** for translate (per M4.3a.1 user
feedback): translate always lands as `Proposed`. When the LLM attached
at least one semantic flag, `translate_unit_in_project` additionally
sets `review_status = NeedsReview` so the unit appears in the M4.7
review queue. Empty-flag units stay at their previous review status
(`None` for a freshly-translated unit) — the M4.6.2 "Accept" button is
the path to `Reviewed`.

#### M4.6.2 — UI surface for flagging ✓ shipped

Inspector renders model flags as severity-style chips with the model's
per-flag note; a confidence bar shows `unit.confidence` with muted-warning /
neutral / subdued-positive coloring at <50% / 50–85% / >85%. A "Needs
review" or "Reviewed" chip appears in the Inspector header. Unit list rows
show a ⚑-prefixed flag count badge next to the state dot. The "Accept"
button (visible when flags are present) calls `accept_unit_in_project`,
clears flags + flag_notes in memory, and appends a `Reviewed` event to
`review.jsonl`. The `BackendMalformedResponse` finding detail now renders
its `reason` field in `summarizeDetail`.

### M4.7 — Review queue ✓ shipped

New Tauri command `scan_project_review_state` scans every registered
catalog (eagerly opening any not yet in the project store via the Qt
adapter) and returns `ReviewQueueResponse` — a count per catalog plus
a flat sorted list of `ReviewQueueItem` covering every unit where
`review_status == NeedsReview` OR `flags` is non-empty.

UI additions:

- **"Needs review" filter** on the CatalogList unit list (fifth tab,
  joining All / Untrans. / Proposed / Finished). Filter logic:
  `unit.review_status === "needs-review" || unit.flags.length > 0`.
  `UnitRow` gained a `needsReview` boolean derived from those two fields.
- **Sidebar badge**: a warning-colored `⚑N` chip in the project name
  row when `total_count > 0`; click opens the Review view. Per-catalog
  count indicators appear next to each catalog entry.
- **Review tab** in the topbar (fifth tab after Translate / Glossary /
  Settings / Quality) with a live count badge driven by
  `reviewQueueCount`. Tab badge uses `severity-soft` palette.
- **Virtual review-queue view** (`ReviewQueue.tsx`): table of all items
  with Catalog / Locale / Unit ID / Source / Target / Flags / Status /
  State columns; "Open" button navigates to the catalog + unit in the
  Translate view. Catalog column is clickable to filter to that catalog.
  Empty state: "No units need review. Great work."
- **Live badge**: rescan is triggered (debounced 200ms) after every
  translate, accept, edit, save, discard, and project open.

CSS: added `--color-severity-soft-border` token (dark and light themes).

Non-`qt-ts` catalogs are skipped with a `tracing::warn!` and noted in
the command doc comment. PO and ICU-JSON will be wired in M4.4/M4.5.

"Source changed" filter (comparing `source_hash` against the last-saved
hash) is deferred to a follow-on slice once `source_hash` is written to
`review.jsonl` on Accept.

### M4.8 — Bulk translate ✓ shipped

Per-catalog × per-locale "Translate all untranslated" with live
progress, cancel mid-run, and flagged units routed to the review queue.
Backend job system in Rust (channel + cancellation token); Tauri events
stream progress to the UI. No cross-catalog runs in v1.

### M4.9 — Quality tab + in-app prompt evaluation ✓ shipped

Replaces today's value-proposition banner with a full quality dashboard.
Per-project, per-locale.

Sections shipped:

1. **Headline numbers per locale** — per-locale cards showing % accepted-as-is /
   % edited / % from-scratch; 30-day acceptance-rate inline SVG sparkline
   (green >70%, yellow 40–70%, red <40%); computed client-side from
   `corrections.jsonl` via parallel `listCorrectionsInProject` calls.
2. **Translation memory** — searchable corrections table (carried from M4.3d).
3. **Curated set** — editable notes; size counter; promote/un-curate actions
   (carried from M4.3d).
4. **Prompt evaluation** — "Run evaluation" button (disabled when no curated
   examples); live progress bar with completed/total/current locale and Cancel;
   latest-run summary (headline score, per-locale table, per-flag-kind
   breakdown, delta vs prior run); collapsible run history table.

Rust additions:
- `crates/project/src/evaluation.rs` — `EvaluationRun`, `LocaleScore`,
  `FlagScore`, `ScoreAccumulator`, `EvaluationStore`.
- `ProjectPaths::evaluations()` — `.i18n-harness/evaluations.jsonl`.
- `Project::evaluations() -> &EvaluationStore`.
- Tauri commands: `run_evaluation_in_project` (ollama feature only) and
  `list_evaluation_runs_in_project`.
- Events: `eval-progress-<job_id>`, `eval-completed-<job_id>`,
  `eval-failed-<job_id>`.
- Scoring v1: exact-match (`trim()` both sides; 1.0 match, 0.0 mismatch).

Deferred to M4.10:
- Tuning bundle export (`.i18n-harness/tuning/<ts>/examples.jsonl` + prompt +
  score).
- "Estimated runtime before starting" cost hint.

### M4.10 — Tuning-skill contract ✓ shipped

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

# M5 — Agent translation flow (CLI + skill)

**Status:** shipped (CLI-only). UI integration deferred to M6.

The offline Copilot path from
[`initial_design.md`](initial_design.md). The harness writes a
batch + instructions to a known path; an external Claude Code /
Copilot / Codex session — run by the user in a separate terminal —
fills `targets.jsonl`; the harness ingests the result through the
existing `ManualBackend` and runs the validation gate. No API key
ever leaves the agent's session; the harness itself stays offline.

The original docs M5 (structured-log adapter design) is **deferred
indefinitely** in favour of this work.

### M5.0 — On-disk format + serializer ✓ shipped

`crates/backend/src/agent_batch.rs` — `ExportedUnit` curated view,
`write_export` / `read_targets`, `AgentBatchError`. The batch
directory layout (`README.md`, `prompt.md`, `units.jsonl`,
`targets.jsonl`, `meta.json`) is the stable contract for M5.1,
M5.2, and any external agent. Property test for identity round-trip;
curated Qt `de_DE` fixture under `fixtures/agent-batch/`.

### M5.1 — CLI `export-batch` + `import-batch` ✓ shipped

Two new subcommands in `crates/cli/src/main.rs`:

- `harness export-batch <catalog> --locale <id> --out <dir>
  [--glossary <path>]` — extracts the catalog, filters writable
  units, writes the batch folder. No model runs.
- `harness import-batch <dir> --apply <catalog> [--out <path>]
  [--metrics <path>]` — reads `targets.jsonl`, wraps it in a
  `ManualBackend` closure, runs the gate, optionally writes back.
  `--out` semantics mirror `harness translate`: dry-run without it,
  atomic write with it, hard-finding write guard always.

Qt-only at the CLI surface (matches existing `harness translate`).
The format itself is format-agnostic.

### M5.1.1 — Project-mode for export-batch / import-batch ✓ shipped

Extended the M5.1 subcommands to accept `--project <dir>` and walk
every Qt catalog matching `--locale` in the project manifest. The
batch root holds one subfolder per catalog plus a project-level
`README.md` and `meta.json` (`mode: "project"`). Non-Qt catalogs
emit a `warning:` line and are skipped pending the multi-format
CLI dispatch (separate milestone).

### M5.2 — `translate-i18n-batch` skill ✓ shipped

Two-file skill spec for external Claude Code sessions:

- [`skills/translate-i18n-batch/README.md`](../skills/translate-i18n-batch/README.md)
  — the procedural body: bundle layout, `units.jsonl` /
  `targets.jsonl` schemas, ICU placeholder rules, plural CLDR forms,
  glossary precedence, register handling, markup tag rules,
  failure modes.
- [`.claude/skills/translate-i18n-batch/SKILL.md`](../.claude/skills/translate-i18n-batch/SKILL.md)
  — frontmatter + slash-command surface (`/translate-i18n-batch
  <path>`); body delegates to the README.

The skill is **external** — it runs in a Claude Code session the user
opens separately; the harness never spawns it.

---

## Out of scope for M5

- In-app UI integration (Settings backend picker, "Translate via
  Claude Code" affordance, batch-folder reveal). **Deferred to M6.**
- A real `agent` `TranslationBackend` impl that blocks on a
  filesystem watcher. Deferred to M6.
- Headless Claude Code / Copilot / Codex spawning from the harness.
  Deferred to M6+.
- PO + ICU-JSON CLI dispatch. The format spec is adapter-agnostic;
  wiring the non-Qt adapters into the CLI is a separate axis.
- Cloud / API-key backends.
- The structured-log adapter (was the original docs M5; postponed
  indefinitely).

---

## Verification (rolling)

After each M4.x / M5.x:

- `cargo test --workspace --all-targets` and
  `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo fmt --all -- --check` all green.
- `cd ui && bun run typecheck && bun run lint && bun audit --prod
  && bun run build` all green.
- Round-trip suite (`crates/adapter-qt/tests/fixtures/` + new
  PO/ICU-JSON fixtures from M4.4/M4.5 onward) is byte-identical on
  every fixture.
- Gate fixture matrix grows; no rule silently removed.
- M5 adds the agent-flow round-trip: `export-batch → fill verbatim
  → import-batch` produces a catalog that itself passes
  `harness round-trip` byte-stably.
