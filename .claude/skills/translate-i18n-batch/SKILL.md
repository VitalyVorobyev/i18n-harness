---
name: translate-i18n-batch
description: Use when filling `targets.jsonl` for an i18n-harness agent translation batch — a folder produced by `harness export-batch` that needs translations for one locale. The user (or another Claude Code session) points this skill at the batch directory; the skill reads `meta.json`, `prompt.md`, and `units.jsonl`, then appends one valid line to `targets.jsonl` per source unit. Use this skill exclusively for filling a batch folder; do NOT use it to design new prompts, run gate checks, or modify the catalog itself — those belong to other tooling. Trigger phrases include "translate the batch in <path>", "fill targets.jsonl", "run the agent translation flow", or any explicit `/translate-i18n-batch <path>` invocation.
---

# translate-i18n-batch

Fills `targets.jsonl` in an i18n-harness agent translation batch folder.

## When you DO need this skill

- The user points you at a `.i18n-harness/agent-batch/<batch-id>/`
  directory (or any folder containing `units.jsonl`, `prompt.md`,
  `meta.json` produced by `harness export-batch`) and asks you to
  translate the contents.
- The user runs the slash command `/translate-i18n-batch <path>`.
- The user says "fill the batch in <path>" or "run the agent
  translation flow on <path>".
- A multi-catalog project batch (root `meta.json` has `mode: "project"`
  and lists subfolders) — translate each subfolder in turn following
  the same procedure.

## When you do NOT need this skill

- Designing a new prompt template — use `/tune-i18n-prompt` instead.
- Running the gate, applying targets to the catalog, or writing back
  to the `.ts` / `.po` / `.json` file — that is what `harness
  import-batch` does after you finish; do not attempt it from inside
  this skill.
- Translating ad-hoc strings without a batch folder — this skill only
  runs against the on-disk batch format.

## How to invoke

```
/translate-i18n-batch <absolute-path-to-batch-folder>
```

Or, equivalently, ask the user to drop the batch folder path in chat
and say "translate the batch."

## Procedure

The body lives in [`skills/translate-i18n-batch/README.md`](../../../skills/translate-i18n-batch/README.md)
at the repo root. **Read that file in full before doing anything.** It
documents:

- The exact folder layout and which files you read vs. write.
- The `units.jsonl` schema (`ExportedUnit`: id, source, plural_arity,
  placeholders, flags_so_far).
- The `targets.jsonl` schema (per-line variants: singular / plural /
  skip / fail).
- The rules that the harness's validation gate enforces after import:
  placeholder preservation, plural CLDR forms, glossary precedence,
  markup tag handling, accelerator markers, escape sequences,
  whitespace and punctuation.
- Failure modes and what to do for each.

In short: read `meta.json` for the locale id and source catalog path;
read `prompt.md` for the per-batch context (register, glossary, DNT);
iterate `units.jsonl` line by line; append one `targets.jsonl` line
per unit in the same order; finish when line counts match.

## Hard rules (the validation gate will block the write otherwise)

These are restated from the README because violating them blocks the
catalog write outright — not just a soft flag.

1. **Placeholders are sacred.** Every token in
   `units.jsonl[].placeholders` must appear verbatim in your output,
   identical spelling and braces, same count. Do not add, remove, or
   rename.
2. **Plural arity must match.** When `plural_arity` is `N`, produce
   exactly `N` entries in `texts`, in canonical CLDR order for the
   locale (look up the order in the README's plural table).
3. **`id` echoes verbatim.** Every `targets.jsonl` line's `id` field
   must match `units.jsonl`'s id at the same line position. Reordering
   is rejected by `import-batch`.
4. **Line count equals unit count.** Fewer lines → batch incomplete;
   `import-batch` aborts.

## After you finish

Print the absolute path of the `targets.jsonl` you produced and remind
the user to run:

```
harness import-batch <batch-folder> --apply <catalog-path>
# or, for project-mode batches:
harness import-batch <batch-folder> --project <project-root>
```

Do NOT run `import-batch` yourself — the harness CLI is the user's
tool, not yours. Your job ends when `targets.jsonl` is complete and
schema-valid.

## What you do NOT do

- Do not modify `units.jsonl`, `prompt.md`, `README.md`, or
  `meta.json`. They are read-only inputs.
- Do not invent placeholder syntaxes not listed in `placeholders`.
- Do not invent new `flag.kind` values. `flags_so_far` is read-only
  signal; your output schema does not have a flags field.
- Do not run the harness CLI (`export-batch` / `import-batch` /
  `gate` / `round-trip`). Those are the user's commands.
- Do not edit catalog files (`.ts`, `.po`, `.json`) directly. The
  harness's adapters own write-back.

## Verification before reporting done

- Line count of `targets.jsonl` equals line count of `units.jsonl`.
- Each line is valid JSON with a recognized `kind`
  (`singular` / `plural` / `skip` / `fail`).
- Each line's `id` matches the same-position id in `units.jsonl`
  (validate by reading both files end-to-end and zipping).
- For every `kind: "plural"` line, `texts.len()` equals the
  corresponding unit's `plural_arity`.
- Every placeholder listed in a unit's `placeholders` array appears
  verbatim in that unit's translated `text` (or every `texts[i]` for
  plurals).
