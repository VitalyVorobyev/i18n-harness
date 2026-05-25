---
name: tune-i18n-prompt
description: Use when refining an i18n-harness prompt template based on a tuning bundle exported from the app's Quality tab. The bundle (`.i18n-harness/tuning/<ISO-timestamp>/`) contains `examples.jsonl` (translator corrections of model proposals), `prompt.txt` (the active template), and optional `score.json` (baseline evaluation). The skill diffs `mt_proposal` vs `human_target` across examples, identifies systematic failure patterns, drafts a refined prompt template adhering to the v2 strict-JSON output contract, and writes the candidate to `prompts/<locale>.txt` (per-locale failures) or `prompts/default.txt` (cross-locale). Use this skill exclusively for prompt rewriting; do NOT use it to translate strings, run gate checks, or apply translations to the catalog. Trigger phrases include "tune the prompt for <bundle>", "improve the i18n prompt from this tuning bundle", "refine the prompt template", or any explicit `/tune-i18n-prompt <path>` invocation.
---

# tune-i18n-prompt

Refines an i18n-harness prompt template from a tuning bundle.

## When you DO need this skill

- The user points you at an `.i18n-harness/tuning/<ISO-timestamp>/`
  directory exported from the app's Quality tab.
- The user runs the slash command `/tune-i18n-prompt <path>`.
- The user says "tune the prompt for <path>", "improve the i18n
  prompt", or "rewrite the prompt template based on these corrections."
- The user has noticed a systematic failure pattern (idiom mishandling,
  brand-term drift, register inconsistency) and wants the prompt
  template to compensate.

## When you do NOT need this skill

- Translating individual strings — use `/translate-i18n-batch` instead.
- Designing the bundle format itself — that lives in the Rust crates
  (`crates/project/src/tuning.rs`).
- Running the in-app evaluation — the harness owns
  `evaluations.jsonl`; the skill recommends running an evaluation
  *after* writing the new prompt, but never runs one itself.
- Applying translations to a catalog file — that is `import-batch`
  or the in-app review-and-save loop.

## How to invoke

```
/tune-i18n-prompt <absolute-path-to-tuning-bundle>
```

Or, equivalently: drop the bundle path in chat and say "tune the
prompt."

## Procedure

The procedural body lives in
[`skills/tune-i18n-prompt/README.md`](../../../skills/tune-i18n-prompt/README.md)
at the repo root. **Read that file in full before doing anything.**
It documents:

- The bundle layout (`examples.jsonl`, `prompt.txt`, `score.json`,
  `locales.toml`, in-folder README).
- The `examples.jsonl` schema (`source`, `mt_proposal`,
  `human_target`, `locale`, `flags`, optional `note`).
- The allowed flag kinds (model-produced only: `ambiguous-source`,
  `insufficient-context`, `idiom`, `low-confidence`, `brand-term`,
  `tone-mismatch`).
- The strict v2 JSON output contract every prompt must produce
  (`{"translation": "...", "flags": [...], "confidence": <0–1>}`).
- The `{token}` substitution surface (`{source}`, `{locale}`,
  `{register}`, `{glossary_block_or_(none)}`,
  `{do_not_translate_block_or_(none)}`, `{template_version}`,
  `{locale_example_block_or_empty}`,
  `{plural_category_line_or_empty}`).
- Failure modes (missing `score.json`, no model proposals, too few
  examples).

In short: read the bundle; cluster diffs by locale and flag kind;
identify the top 3–5 failure patterns; draft a refined template that
keeps the JSON output contract and the `{token}` substitution
surface intact; ask the user to confirm the destination
(`prompts/<locale>.txt` for locale-specific failures, otherwise
`prompts/default.txt`); write the file; remind the user to run "Run
evaluation" in the app to confirm the new score beats the baseline.

## Hard rules (the harness will reject the new prompt otherwise)

1. **Preserve the v2 strict JSON output schema.** Every prompt must
   instruct the model to emit one JSON object per call, with the
   `translation`, `flags`, and `confidence` fields. Do not invent
   new top-level fields.
2. **Preserve the `[template=<version>]` header.** The version tag
   is recorded in metrics events so a future analysis can correlate
   "model X gave bad German" with "we used template Y at the time."
   Bump the version number when you write a new template; do not
   delete the line.
3. **Do not invent new `flag.kind` values.** The allowed kinds are
   listed in the README — they match `crates/core/src/flag.rs`. Use
   the existing vocabulary even when reorganizing the template.
4. **Unknown `{token}` patterns pass through verbatim** — that is
   intentional behaviour the harness depends on (instructional
   examples like `{count}` and `{{var}}` are NOT substitution
   tokens). Do not alter this contract.
5. **Cap template length at ~200 lines.** Local-model latency
   budget — longer prompts slow the hot loop.

## After you finish

Print:
- The relative path of the new prompt file you wrote
  (`prompts/<locale>.txt` or `prompts/default.txt`).
- A short diff summary: which failure patterns the new template
  addresses, ranked.
- A reminder to open the project in the app and click "Run
  evaluation" in the Quality tab to verify the new score beats the
  baseline in `score.json` (if a baseline exists).

Do NOT run the harness CLI or the in-app evaluator yourself.

## What you do NOT do

- Do not modify `examples.jsonl`, `score.json`, `locales.toml`, or
  the bundle's in-folder README — they are read-only inputs.
- Do not modify the project manifest (`i18n-harness.toml`) — only
  files under `prompts/` are in scope for writes.
- Do not unilaterally pick between per-locale and global
  replacement — recommend, then ask the user to confirm.
- Do not run `harness translate`, `harness gate`, or any evaluation
  command. The skill ends when the new prompt is on disk.
- Do not translate strings directly — use
  `/translate-i18n-batch` for that.

## Verification before reporting done

- The new prompt file parses as a valid template (it has the
  `[template=<version>]` header and uses `{token}` substitution
  syntax).
- The output schema instruction in the new prompt mentions
  `translation`, `flags`, and `confidence` (the v2 contract).
- The allowed flag kinds enumerated in the new prompt are a subset
  of the kinds in `crates/core/src/flag.rs` (no inventions).
- The user has been asked to confirm the destination
  (`prompts/<locale>.txt` vs `prompts/default.txt`) before write.
