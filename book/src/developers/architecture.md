# Architecture

i18n-harness is a Rust workspace with a Tauri + React desktop shell. The
design rests on two invariants that every change must keep.

## The two invariants

1. **Languages, translation engines and catalog formats are data or
   plugins behind stable extension points.** The core names none of them
   specifically. A new language is a data row, a new engine an
   implementation of a trait, a new catalog format a serializer listed in
   one registry.
2. **The model translates text; deterministic Rust does everything
   else.** Parsing, validation and writing never go through the model. A
   weak model can produce *worse text*; it cannot corrupt *structure*,
   because the checks (the "gate") stand between every answer and the
   file.

## Data flow

```text
catalog ─► format (read) ─► units (ICU form) ─► backend ─► gate ─► format (write) ─► catalog'
```

- A **catalog** is a translation file on disk. It is the source of
  truth.
- A **format** reads it into **units** — one per translatable string,
  with placeholders normalised to ICU MessageFormat — and writes units
  back, changing only the bytes of changed translations.
- A **backend** turns a unit's text into a translation. It knows nothing
  about files.
- The **gate** compares each translation with its original using CLDR
  plural data, placeholder multisets, markup and the glossary, and
  returns findings with a severity. Hard findings block writing.
- The **session** orchestrates all of this for both the CLI and the
  desktop app: statuses, pivot input, plural expansion, review state,
  the save guard.

## Crates

| Crate | Role |
|---|---|
| `crates/core` | `Unit`, `UnitState`, `Flag` and severities, batches, the intermediate representation. |
| `crates/locales` | Every CLDR language: plural categories with example numbers, script, English name (generated into `src/cldr.rs`), plus harness defaults for register and length in `data/defaults.toml`. |
| `crates/gate` | The checks. Format- and backend-agnostic. |
| `crates/glossary` | The glossary TOML: load, validate, edit, save. |
| `crates/backend` | The `TranslationBackend` trait; `ModelBackend` (Ollama and OpenAI-compatible, driven by prompt profiles in `profiles/`); model discovery; the manual backend; the agent batch format. |
| `crates/catalog` | The `CatalogFormat` / `FormatDoc` traits and the gettext PO and ICU-JSON formats. |
| `crates/adapter-qt` | The Qt Linguist `.ts` format (its XML handling is large enough for its own crate) and the subset writer used by split-remainder. |
| `crates/formats` | `FormatRegistry`: the one list of built-in formats; detection by sniffing. |
| `crates/project` | `i18n-harness.toml`: parsing, editing with comments preserved, discovery of catalogs in a folder, locale resolution, the review store and other state under `.i18n-harness/`. |
| `crates/reuse` | Reference reuse, remainder split and merge (Qt). No model. |
| `crates/session` | The translation session shared by the CLI and the app. |
| `crates/cli` | The `harness` binary. |
| `crates/lab` | The `harness-lab` binary (model comparison). Not part of the app. |
| `ui/` | The desktop app: React front end (`ui/src`) and a thin Tauri shell (`ui/src-tauri`) whose commands call the session. |
| `xtask/` | Maintainer tasks: regenerating the CLDR table. |

Dependencies point one way. `core` and `locales` are at the bottom; the
formats (`catalog`, `adapter-qt`, `formats`), `glossary`, `gate` and
`backend` build on them; `project` and `reuse` above those; `session`
above everything it orchestrates; and the binaries — `cli`, `lab` and the
Tauri shell — at the top. Nothing below `session` knows about statuses
or the UI; nothing above it re-implements catalog handling.

## Units and statuses

A unit's `state` mirrors the file: `Untranslated`, `Proposed` (Qt
`type="unfinished"` with text, PO `fuzzy`), `Finished`, and `Vanished` /
`Obsolete` (never touched). The translator-facing **status** (*To do*,
*Check*, *Done*) is derived by the session from the state, the review
store (`.i18n-harness/review.jsonl`) and, for a catalog with a pivot
language, whether the text is still the unedited pivot text. A review
decision records a hash of the original; when the original changes, the
decision lapses.

## Two products

The repository serves two audiences that do not share features:

- the **translator** uses the desktop app (and `harness translate-folder`
  for unattended runs);
- the **maintainer** evaluating models uses `harness-lab`.

The shared substrate is the gate, the formats, the backend trait, the
session and the per-folder state.
