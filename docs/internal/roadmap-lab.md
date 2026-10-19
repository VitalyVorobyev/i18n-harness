# i18n-harness — Lab roadmap

The maintainer-facing roadmap. Empirical work on backends and locales
that is **not shipped inside the translator app**. The product
roadmap lives in [`roadmap-product.md`](roadmap-product.md).

## Why a separate lab track

The original [`initial_design.md`](initial_design.md) §8 framed the
per-`(backend, locale)` quality metric as a feature inside the
translator app. That conflated two audiences:

- A **translator** asks: "is the prompt for *this project*
  producing acceptable German?" — a per-project question, answered by
  the M4.9 Quality tab and the M4.10 tuning loop.
- A **maintainer / researcher** asks: "across all my projects and
  fixtures, is Gemma 4 E2B good enough for German *as a model*?" — a
  cross-project, cross-backend question. That belongs in a separate
  tool.

Mixing them in one UI tab gave translators a research surface they
didn't need and gave the maintainer a project-scoped surface that
couldn't compare across projects.

The lab track owns the second question.

---

## L1 — Carve-out and cross-project benchmark

### L1.0 — `crates/harness-lab` carve-out

When M4.9 lands the new Quality tab, the existing
`ui/src/components/MetricsPanel/` becomes a cross-project lab surface
that does not belong in the translator app. Move it (or rebuild it)
into one of:

- A separate Tauri binary `crates/harness-lab` consuming
  `.i18n-harness/metrics.jsonl` across projects pointed at via a
  config file or CLI flag.
- A CLI subcommand `harness lab report --projects <list>` that
  writes a static HTML report.

Either is fine; the decision is "smaller blast radius wins". Likely
the CLI + HTML report — no UI maintenance burden, opens in a browser,
trivially shareable.

### L1.1 — Cross-project benchmark fixture corpus

A `fixtures/lab/` corpus of representative source strings per locale
covering: short UI labels, full sentences, plurals, placeholders, idiom,
accelerators, CJK punctuation. Distinct from the round-trip fixtures —
those test structural fidelity; these test translation quality.

The lab tool runs every fixture through every configured
`(backend, locale)` pair and emits scored events. The score is the same
acceptance-style metric as the in-app Quality tab, but aggregated
across the fixture set instead of a single project's curated examples.

### L1.2 — Backend × locale regression harness

CLI: `harness lab regress --baseline <run-id>`. Runs the corpus,
compares against a baseline run-id (stored under `.i18n-harness/lab/`),
fails CI if any `(backend, locale)` pair drops below threshold. Lets
the maintainer catch quality regressions from a prompt change, a model
swap, or a gate-rule tightening.

HTML report: per-pair score table, per-rule histogram, regression
markers vs baseline.

---

## M5 — Logs phase (still design-only)

§9 of [`initial_design.md`](initial_design.md) describes a structured-
logs phase. Status unchanged: design-only, no crate, no fixtures. If
the maintainer's needs change, it gets its own plan-mode pass before
any code lands.

---

## What is explicitly *not* in the lab track

- Anything a translator user would see — that's all
  [`roadmap-product.md`](roadmap-product.md).
- Per-project prompt tuning — that's the product Quality tab's job,
  scoped to one project at a time.
- Telemetry phoning home — the lab tool reads local `.i18n-harness/`
  state only.

---

## Verification (rolling)

After each L1.x:

- The translator app has zero metrics UI by the end of L1.0; the
  Quality tab (M4.9) is the only quality surface inside the translator
  app.
- `harness lab report --projects <list>` writes a self-contained HTML
  file with no network fetches.
- Regression baselines are reproducible — the same corpus + the same
  prompt + the same model = the same score, modulo Ollama
  non-determinism (record and report the variance).
