# i18n-harness — Open-Source Design & Claude Code Planning Handoff (Final)

**Status:** Planning handoff. Read in **plan mode** before writing code.
**Project type:** Personal open-source tool. Local-first desktop app. Rust core + Tauri/React UI.
**Working name:** `i18n-harness` (rename freely).
**Audience:** Claude Code — resolve open decisions (§13), produce a milestone plan, then implement.

---

## 0. Claude Code working agreement (read first)

- **Stay in plan mode** until §13 open decisions are resolved with the maintainer and a milestone plan is agreed. Do not implement from this document alone.
- **M0 (the round-trip stability test) is load-bearing.** Do not proceed past it until it is green. A tool that produces translations but corrupts catalogs is worse than no tool.
- **Do not "fix" things speculatively.** If a dependency version, CI action ref, model tag, or format quirk looks wrong, verify against the actual source/spec before changing it. Flag uncertainty; do not paper over it.
- **Clean-room.** This is independent of any employer-internal tool. Design only from public specs: the Qt `.ts` schema, ICU MessageFormat, Unicode CLDR, gettext PO, and the Ollama HTTP API. Keep commit history free of internal references.
- Conventions, build/test/lint commands, the workspace map, and the §1 invariants live in `CLAUDE.md` at the repo root. Keep it authoritative.

---

## 1. The one principle the whole design rests on

**Languages, translation engines, and catalog formats are all data or plugins behind stable extension points. The core names none of them specifically.**

Everything below is an application of that sentence. It is what makes the tool open/closed in the three dimensions that actually change over time:

- a **new language** is a config row (CLDR data), not a code change;
- a **new translation engine** is a trait impl, not a core edit;
- a **new catalog format** is a serializer, not a new adapter.

The second invariant, equally firm:

**The model translates text; deterministic Rust does everything else.** Parsing, validation, and write-back never go through the LLM. A weak model can therefore produce *worse text* but can never *corrupt structure* — the validation gate guarantees it.

---

## 2. What the product is

A **local-first desktop app**: install a local model server (Ollama by default), point the app at your translation catalogs, and translate UI strings end-to-end — offline, free, no API key, no cloud, no agent required.

A local model exposes a plain HTTP endpoint, so the app calls it directly. This is the shift that makes the app the product rather than a viewer: there is no agent in the core loop.

```
Tauri UI ─► Rust harness (in-process library)
                │
                ├─ adapter.extract(catalog) ─► intermediate units
                ├─ backend.translate(units, glossary) ─► filled units      [HTTP → local model]
                ├─ gate.validate(units) ─► pass / flags / report
                └─ adapter.apply(units, catalog) ─► catalog' (byte-stable)
                │
            glossary · flags · per-locale quality metrics · human review
```

The CLI and any agent-driven path are **power-user / automation surfaces** around the same core. The desktop app is the primary deliverable.

---

## 3. Architecture: core + three extension points

```
                         ┌────────────────────────────────────────┐
   catalog formats       │            i18n-harness core            │      translation engines
   ─────────────────     │                                         │      ───────────────────
   Qt .ts          ─┐    │  ┌───────────┐     ┌──────────────┐     │   ┌─ Ollama (default)
   PO (Lingui/      ─┼──► │  │ adapters  │ ──► │  validation  │     │ ─►├─ OpenAI-compatible*
   gettext)         │    │  │ (extract/ │     │     gate     │     │   ├─ agent (Claude Code/Copilot)
   ICU-JSON         ─┘    │  │  apply)   │     │ (CLDR-driven)│     │   ├─ manual (human only)
   (react-intl/         │  └───────────┘     └──────────────┘     │   └─ …contributor impls
    i18next)            │        │  ▲              │              │
   log catalog (later)  │        ▼  │              ▼              │   * covers vLLM, LM Studio,
                         │   intermediate      flags + report     │     many local servers, and
                         │      (ICU)          + metrics          │     cloud keys with one impl
                         └────────────────────────────────────────┘
                                       │  ▲
                                       ▼  │
                          Tauri/React UI (review · edit · glossary · metrics)
```

- **Intermediate representation** is the contract between every part, expressed in **ICU MessageFormat** for placeholders and plurals. Because plural/select arity in ICU is CLDR-derived, the gate is written once and reused across all adapters and locales.
- The **structured catalog is the source of truth**; the intermediate is a transient, regenerable working artifact. Never let it become a second source of truth.

---

## 4. Extension point A — Languages as data

A locale is a config record, never a Rust `match` arm:

```yaml
locales:
  de_DE:
    cldr_plural: [one, other]     # arity = 2; seeded from CLDR
    register: formal              # Sie
    variant: de_DE
    length_warn_ratio: 1.4        # German runs ~30–40% longer
    script: latin
  zh_CN:
    cldr_plural: [other]          # arity = 1
    register: formal
    variant: zh_Hans
    script: han                   # full-width punctuation, no inter-word spacing
```

Adding a language = add a row, seed arity from CLDR. The gate reads arity from the record and never names a language.

**Honest scope of this:** it gives *structural* support for free — gate, round-trip, and UI all just work. It does **not** guarantee *translation quality* into that language; that is the model's problem, surfaced by the per-locale quality metric (§8). Open/closed for correctness; quality is empirical.

Initial locales: source **en**; targets **de_DE**, then **es_ES** and a Chinese variant. Confirm `zh_Hans` vs `zh_Hant` and `es_ES` vs `es_419` before they accumulate translations (irreversible once they do).

---

## 5. Extension point B — Translation engines behind a trait

```rust
trait TranslationBackend {
    fn name(&self) -> &str;
    fn is_deterministic(&self) -> bool;          // false for LLMs
    fn translate_batch(&self, units: &[Unit], glossary: &Glossary) -> Result<Vec<Unit>>;
}
```

The trait is **public API and a documented extension point**. Nothing in the gate, adapters, or UI may name a specific engine. Backend is selected by config (name + endpoint).

Planned impls:

- **`ollama` (default).** Local HTTP (`localhost:11434`). Maintainer's setup: Gemma 4. **Must set `num_ctx` explicitly** — Ollama defaults to 4K context, which silently truncates batches with injected glossary context; Gemma 4 E2B/E4B support 128K. Default to **E4B** (Google/HF flag it as the best general-purpose size; ~5 GB at 4-bit, fits an M-series Mac comfortably); keep **E2B** as a fast-iteration option. Which is "good enough" per locale is decided by the §8 metric, not assumed.
- **`openai-compatible`.** High leverage: one impl reaches vLLM, LM Studio, many local servers, *and* cloud keys, because they share the protocol. This is how most *other people's* setups are supported without bespoke code.
- **`agent`.** Writes a batch + instructions to a known path for Claude Code / Copilot to fill. No API key. The "dogfood" and corporate-friendly path.
- **`manual`.** No model; harness prepares the intermediate and the human translates everything in the UI. Proves the tool is useful with zero AI.

Contributor guide: "implement the trait, register the impl, done." A user with any other engine plugs in without touching core.

---

## 6. Extension point C — Catalog formats as pluggable serializers

**Key clarification on i18n libraries (Lingui, react-intl, i18next):** these are part of *the app being translated*, not part of this tool. A React app uses them at runtime to display strings and at build time (e.g. Lingui's CLI) to extract messages into **catalog files**. This tool enters *only at the catalog file* — it reads the catalog, fills untranslated entries, validates, writes back. It never knows or cares which library produced the file.

Therefore the library choice does **not** belong in the core, and using Lingui in your own apps imposes nothing on anyone else. A library only ever determines *a file format*, and formats are pluggable:

```rust
trait CatalogFormat {
    fn read(&self, path: &Path) -> Result<Vec<Unit>>;
    fn write(&self, units: &[Unit], path: &Path) -> Result<()>;
}
```

Planned serializers:

- **PO** — covers **Lingui** *and* gettext. Build first (it is the maintainer's Lingui use case and has the widest reach).
- **ICU-JSON** — covers **react-intl** and **i18next-ICU**.

Because everything funnels through ICU in the intermediate, PO and ICU-JSON differ in *syntax*, not in the *information* they carry, so serializers are thin. A user on an unshipped format writes one serializer, not a new adapter.

---

## 7. Adapters

An adapter pairs a parser/writer with the round-trip guarantee (`extract` then `apply` with zero changes = byte-identical on disk).

- **Qt (`.ts`) — reference adapter.** Hardest parser (XML). Preserve `<location>`, `vanished`/`obsolete`, significant whitespace, message state; `unfinished` → finished on apply; never touch non-target states. Wrap `lrelease` for `.qm`. Defines "done" for M0 and the gate. **XML round-trip fidelity dominates the crate choice** (§13).
- **React (via §6 serializers).** Lowest risk once Qt works; mostly serializer pairs over the shared core. Does not dictate the consuming app's library.
- **Logs — separate phase (§9).**

---

## 8. Validation gate (shared, CLDR-driven) + quality metrics

**Hard (block apply):** placeholder multiset preserved; plural/select arity matches the target locale's CLDR arity; ICU parses; non-empty when marked finished.

**Soft (warn / flag for review):** accelerator (`&`) preservation; length expansion past the locale's `length_warn_ratio`; CJK full-width punctuation tolerance; **placeholder grammatical-agreement risk** — when a `{param}`/`%1` stands for a noun, German/Spanish gender/case cannot be resolved at translation time, so flag rather than guess.

**Model-supplied flags (semantic):** ambiguous source ("Record", "Empty", "Left"), idiom, insufficient context, low confidence. Stored in the unit's `flags` alongside deterministic ones; both surfaced in the UI triage view.

**Per-backend / per-locale quality metric.** Track gate-reject rate and human-edit rate, keyed by `(backend, locale)`. The gate and UI already produce these events, so capture is cheap. This turns "is Gemma 4 E2B good enough for German?" into a number visible in the app — the empirical answer to the §4 quality caveat and the §5 model-size question.

The gate is the trust boundary. It runs identically for every adapter, locale, and backend.

---

## 9. Logs — roadmap phase, not "just another adapter"

A log event is the **same abstract unit** (ID, source, params, plural arity) and flows through the **same** intermediate → gate → backend → catalog. That through-line is why it belongs in this project. But it is a *different kind of problem* and must be planned as one:

- Qt/React are **retrofit**: English strings already exist; you translate them.
- Logs done right are **greenfield**: stop logging English sentences; log **structured events** = stable message ID + typed params; render human-readable text **per-locale at view time** (or never, if a machine consumes the log). This changes how developers *emit* logs across Rust/C++/Python — a cross-team behavioral change, not a file format.

**Mini-design required before the logs adapter** (its own plan-mode pass): ID scheme stable across three source languages (e.g. namespaced `net.gige.timeout`); per-language definition extraction (a macro/helper registering `(id, default_source, param_types)` so the harness extracts a catalog without parsing arbitrary code); param typing for ICU formatting + agreement; and the render path (a small viewer/library turning event + locale → text).

**Sequencing:** gated on the software adapters proving the shared core. Rust first (maintainer's stack, reference impl); C++/Python emitters follow. Do not block software milestones on it.

---

## 10. UI (Tauri + React + Vite + TS)

The app owns the loop but the **harness stays a headless library with a CLI; Tauri calls that library.** No translation, XML, or gate logic in Tauri command handlers — the core crates remain testable and reusable, and M0/M1 tests stay meaningful. The app is a *frontend over the core*, exactly as the CLI is, even though it now also triggers the work.

Primary surfaces:
- **Glossary editor** — per-locale terms, `do_not_translate`, register/variant header. A main reason a custom UI exists (Qt Linguist can't do this).
- **Flag-triage dashboard** — units needing attention, grouped by flag type; the other main reason.
- **Per-locale quality metrics** — surfaced where the human already reviews.
- **Review/compare** — source vs proposed, with provenance jump-back. For Qt, note Linguist already does message-by-message review well; the UI's edge is showing glossary + flags + metrics *in the same place*. Don't reimplement Linguist beyond that.

---

## 11. Proposed repo layout (Cargo workspace)

```
i18n-harness/
├── CLAUDE.md
├── Cargo.toml                  # workspace
├── crates/
│   ├── core/                   # Unit, intermediate (ICU) (de)serialization, batching
│   ├── locales/                # locale records + CLDR plural data (data, not code)
│   ├── gate/                   # validation gate + quality-metric events
│   ├── glossary/               # glossary model + load/save
│   ├── backend/                # TranslationBackend trait + impls (feature-gated)
│   ├── catalog/                # CatalogFormat trait + PO + ICU-JSON serializers
│   ├── adapter-qt/             # .ts extract/apply + lrelease wrapper
│   ├── adapter-react/          # uses catalog serializers over the core
│   ├── adapter-log/            # (later) structured-log catalog + renderer
│   └── cli/                    # headless binary; wires everything
├── ui/                         # Tauri + React + Vite + TS (calls the library)
├── fixtures/                   # crafted .ts / PO / ICU-JSON / log fixtures for tests
└── .claude/{commands,skills}/  # optional agent-path orchestration
```

Feature-gate backends and serializers so a minimal build pulls no network/LLM/format deps it doesn't use.

---

## 12. Milestones

- **M0 — Core + Qt round-trip.** `core` + `locales` + `adapter-qt` + `cli`: extract→apply byte-stable on fixtures. **Blocking.**
- **M1 — Gate + metrics.** `gate` with CLDR arity, hard/soft checks, agreement-risk flag, `(backend, locale)` metric events; fixture tests (placeholders, plurals, accelerators, CJK, agreement).
- **M2 — Backends + glossary.** `manual` + `ollama` (with `num_ctx` set), `glossary`. First real `de_DE` batch via local Gemma 4, human-reviewed.
- **M3 — Desktop app.** Tauri shell over the library: glossary editor, flag triage, metrics, review/compare. The product takes shape.
- **M4 — React + reach.** `catalog` PO serializer (Lingui/gettext) then ICU-JSON; `openai-compatible` backend (covers vLLM/LM Studio/cloud); `agent` backend.
- **M5 — Logs phase.** ID-scheme mini-design → Rust `adapter-log` + catalog → renderer → C++/Python emitters.

Every milestone keeps M0's round-trip and M1's gate green.

---

## 13. Open decisions (resolve in plan mode before coding)

1. **`.ts` XML crate** — `quick-xml` with hand-managed round-trip (more work, more byte-faithful) vs parse+reserialize. Round-trip fidelity dominates; lean `quick-xml`.
2. **ICU implementation** — ICU4X (`icu`) vs a focused MessageFormat parser. Need plural/select parsing and arity, not full ICU.
3. **Intermediate** — JSONL (streams/resumes for large catalogs) confirmed?
4. **Batching & resumability** — deterministic batch size + stable ordering + resume key.
5. **Backend build order** — proposal: `manual` + `ollama` first (prove loop, no keys), then `openai-compatible`, then `agent`.
6. **Model default** — E4B default / E2B fast; confirm against the §8 metric on de/zh rather than assuming.
7. **Locale variants** — confirm `zh_Hans` vs `zh_Hant`, `es_ES` vs `es_419`; register defaults (de Sie, es usted).
8. **Catalog format order** — PO first (Lingui), ICU-JSON next; confirm.
9. **Logs ID scheme & extraction mechanism** — biggest logs decision; own mini-design before M5.
10. **License** — see §14.

---

## 14. Licensing & OSS hygiene

- **License:** choose before first public commit. Permissive (MIT / Apache-2.0, or dual) maximizes adoption; Apache-2.0 adds a patent grant and matches Gemma 4's own Apache-2.0 licensing. Not legal advice — choose deliberately.
- **Clean-room:** independent of any employer-internal tool; built from public specs only.
- **No bundled credentials;** all keys via env/config. The `manual` and `ollama` backends make the tool fully usable with zero cloud dependency — the core OSS selling point.
- Contribution guide + the round-trip and gate test suites as the contract for new adapters, backends, and serializers.

---

## 15. Summary for the implementer

Build a headless Rust core with three documented extension points — **locales as data, engines behind `TranslationBackend`, formats behind `CatalogFormat`** — guarded by a CLDR-driven validation gate and a byte-stable round-trip. Wrap it in a Tauri desktop app that calls a local model (Ollama/Gemma 4 by default) directly over HTTP, so the whole loop runs offline and free, with a human reviewing flagged units and a per-locale quality metric telling them where the local model needs help. Qt is the reference adapter; React is serializers over the same core; logs are a later, greenfield, structured-event phase on the same pipeline. The core names no specific language, engine, or format — that is what keeps it open to everyone else's setup, not just the maintainer's.
