# translate-i18n-batch — Claude Code skill spec

A Claude Code skill that fills in `targets.jsonl` for an i18n-harness
agent translation batch — the two-phase counterpart to the in-app Ollama
translation flow.

## Why this skill exists

The harness's in-app translation runs locally with a small model (Gemma 4
by default) for speed and privacy. Some workflows benefit from a larger,
more capable model: hard projects, languages where the small model
struggles, or organizations that prefer their corporate Claude /
Copilot / Codex license over running a local model.

The two-phase agent flow bridges the two: the harness exports a batch
(via `harness export-batch`), an external Claude Code session fills
`targets.jsonl` using this skill, and the harness ingests the result
(via `harness import-batch`). No API key ever leaves the agent's session;
the harness itself stays offline.

## Bundle contents (`.i18n-harness/agent-batch/<batch-id>/`)

A batch directory contains five files. The one you write is
`targets.jsonl`; the rest are inputs.

| File | Purpose | Who writes it |
|---|---|---|
| `README.md` | Full schema + rules. Read this first. | `harness export-batch` |
| `prompt.md` | Batch-specific system prompt: locale, register, glossary terms, do-not-translate list. | `harness export-batch` |
| `units.jsonl` | Source units, one [`ExportedUnit`](../../crates/backend/src/agent_batch.rs) per line. | `harness export-batch` |
| `targets.jsonl` | **Your output file.** One JSON line per unit, in the same order as `units.jsonl`. | This skill |
| `meta.json` | Audit record (locale id, source catalog path, started_at). Ignore during translation. | `harness export-batch` |

The in-folder `README.md` is the authoritative schema. This file is the
skill-runner's playbook — what *Claude Code* does when invoked against
a batch directory.

### `units.jsonl` line shape

Each line is one `ExportedUnit` JSON object:

```json
{
  "id": "app.greeting",
  "source": "Hello, {0}!",
  "plural_arity": null,
  "placeholders": [{"kind": "positional", "index": 0, "name": null, "byte_offset": 7, "icu_form": {"token": "{0}"}}],
  "flags_so_far": []
}
```

Field reference:

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes | Stable unit id within the catalog. Echo verbatim in your output. |
| `source` | string | yes | ICU-normalised source text. The string to translate. |
| `plural_arity` | integer \| null | omit when null | When set, the unit is a plural form; supply exactly this many translated forms in canonical CLDR order. When omitted or null, the unit is singular. |
| `placeholders` | array | omit when empty | Placeholder tokens occurring in `source`. You must preserve every token verbatim. |
| `flags_so_far` | array | always present (may be empty) | Flags a prior gate run attached. Informational only. |

### `targets.jsonl` line shape (your output)

One line per `units.jsonl` line, **in the same order**. Each line is one
of four variants — the `kind` field discriminates:

```jsonl
{"id": "<id>", "kind": "singular", "text": "<translated text>"}
{"id": "<id>", "kind": "plural", "texts": ["<form-1>", "<form-2>"]}
{"id": "<id>", "kind": "skip"}
{"id": "<id>", "kind": "fail", "reason": "<kebab-case>", "retryable": false}
```

The harness's `import-batch` step parses lines one-by-one; you can
append incrementally as you work. The batch is considered complete
when valid line count equals unit count — no sentinel, no done-marker.

## Rules of the game

These are stricter than they look. The harness has a validation gate
that runs after import; soft violations leave the unit in `Proposed`
state for human review, hard violations *block* the catalog write
entirely. Follow these rules and the gate will pass.

### 1. Placeholders are sacred

Placeholder tokens — `{0}`, `{name}`, `{count}`, `{0,number}`,
`{count, plural, …}` — must appear in your translation **with
identical spelling and braces**. Do not translate them, rename them,
add new ones, or drop any.

The `placeholders` array lists every token in `source`. Your output
must contain exactly the same multiset.

### 2. Plurals: one entry per CLDR form

When `plural_arity` is `N`, produce `N` translated forms in canonical
CLDR order:

| Locale | Arity | CLDR order |
|---|---|---|
| `de_DE`, `es_ES`, `en_US` | 2 | `[one, other]` |
| `fr_FR` | 2 | `[one, other]` (note: French uses `one` for 0 and 1) |
| `zh_Hans` | 1 | `[other]` |
| `ru_RU`, `pl_PL`, `cs_CZ` | 3 | `[one, few, many]` |
| `ar_SA` | 6 | `[zero, one, two, few, many, other]` |

Use the locale id from `meta.json` (or the second-line directive in
`prompt.md`) to look up the right arity.

**Output one form's content per array entry, not the wrapping ICU
template.** Example — when `units.jsonl` says

```json
{"id": "app.items", "source": "{count, plural, one {# item} other {# items}}", "plural_arity": 2}
```

the German `targets.jsonl` line is

```json
{"id": "app.items", "kind": "plural", "texts": ["# Eintrag", "# Einträge"]}
```

— just the per-form text. The harness reassembles each form into the
target catalog's native plural shape (Qt `<numerusform>`, ICU `{count,
plural, …}`, PO `msgstr[N]`).

### 3. Drop English plural markers

If the singular source contains hints like `(s)`, `(es)`, `(en)`, those
are **not** literal characters — they signal "this unit is plural."
Drop them and produce the natural target-locale word per form.

### 4. Markup tags: delimiters stay, content translates

`<b>`, `</b>`, `<a href="…">` — the tag delimiters stay verbatim, but
the **text inside** must be translated:

```
Click <b>Save</b>   →   Klicken Sie auf <b>Speichern</b>
```

Not:

```
Click <b>Save</b>   →   <b>Save</b> Klicken Sie auf   ✗ wrong: didn't translate content
```

### 5. Escape sequences and accelerator markers are preserved

`\n`, `\t`, `&amp;`, `&lt;` — preserve verbatim. Accelerator markers
(`&` or `_` before a letter, used by Qt for menu shortcuts) preserve
the marker character and re-anchor it to a reasonable letter in the
target:

```
&File   →   &Datei      ← `&` reanchored from F to D for German
_Open   →   _Öffnen
```

### 6. Glossary precedence

`prompt.md` includes a "Glossary terms" section listing source → target
mappings the project requires. When a glossary term appears in the
source, use the listed target. When a "Do not translate" entry appears
in the source, preserve it verbatim — do not translate it even if it
looks like a regular word.

### 7. Register

`prompt.md` declares the register (`formal`, `informal`, or `neutral`).
Use the matching pronoun and verb form throughout:

| Locale | Formal | Informal |
|---|---|---|
| `de_DE` | Sie | du |
| `es_ES` | usted | tú |
| `fr_FR` | vous (T-V distinction) | tu |

For languages without grammatical T-V (English, Chinese, Japanese
casual), match the register through tone (curt vs. polite) and word
choice.

### 8. Empty / placeholder-only sources

If `source` is empty, or contains only placeholders + markup with no
translatable text, return it unchanged via `kind: "singular"`.

### 9. Whitespace and punctuation

Preserve leading/trailing whitespace and final punctuation as in the
source. `"Save"` and `"Save "` are different units and translate to
different outputs.

## What this skill does

1. Parse the batch directory: read `meta.json` (for the locale id),
   `prompt.md` (for register + glossary + DNT), `units.jsonl`
   (one `ExportedUnit` per line).
2. For each unit, in `units.jsonl` order:
   - Translate the source text according to the rules above.
   - For plurals, produce `plural_arity` forms in canonical CLDR order.
   - When you cannot translate (ambiguous source, missing context,
     conflicting glossary terms), emit a `kind: "fail"` line with a
     short kebab-case reason. **Use `fail` sparingly** — the goal is
     to translate, not to defer.
   - When the unit truly should not be translated (already in target
     language, format-only, marker), emit a `kind: "skip"` line. Skips
     do not count as failures in metrics.
3. Append each line to `targets.jsonl` as you go. The file is append-
   only; you can re-run the skill on the same batch and it picks up
   where it left off (skip lines already present).
4. When line count in `targets.jsonl` equals line count in
   `units.jsonl`, the batch is complete. Print the absolute path of
   `targets.jsonl` and remind the user to run `harness import-batch`.

## What this skill does NOT do

- Run `harness export-batch` or `harness import-batch` itself — the
  user does both, in the harness CLI or app. This skill only fills
  `targets.jsonl`.
- Modify `units.jsonl`, `prompt.md`, `meta.json`, or `README.md` —
  those are read-only inputs.
- Apply translations to the catalog file — that's `import-batch`'s
  job, after the validation gate runs.
- Reorder `targets.jsonl` lines — they must match `units.jsonl` order
  exactly. The harness rejects reordered files.
- Invent new placeholders, drop existing ones, or rename them. See
  rule 1.
- Invent new `flag.kind` values. The agent flow uses no flag kinds in
  output — `flags_so_far` is read-only signal.

## How to invoke

The simplest workflow:

1. In a terminal where the harness is installed:

   ```
   harness export-batch path/to/catalog.ts --locale de_DE --out /tmp/batch-1 [--glossary path/to/glossary.toml]
   ```

   The command prints the absolute path of the batch directory and
   exits 0 immediately. No model runs.

2. In a separate Claude Code session:

   ```
   /translate-i18n-batch /tmp/batch-1
   ```

   Or, if no slash command is registered, point Claude Code at the
   directory and ask it to "fill targets.jsonl following the README
   and this skill spec." Claude reads `meta.json` + `prompt.md` +
   `units.jsonl`, then appends one line at a time to `targets.jsonl`.

3. Back in the first terminal:

   ```
   harness import-batch /tmp/batch-1 --apply path/to/catalog.ts --out path/to/catalog-de.ts [--metrics path/to/metrics.jsonl]
   ```

   The harness reads `targets.jsonl`, runs the validation gate, and
   writes the updated catalog (atomic). Hard findings block the write;
   soft findings leave the affected units in `Proposed` state for
   human review in the app.

## Failure modes

- **`prompt.md` declares an unknown locale**: the locale id is missing
  from CLDR. Abort with a clear message; the harness wouldn't have
  written this batch if the locale were truly unknown, so it's a sign
  the batch directory is corrupted.
- **`units.jsonl` line fails to parse as JSON**: skip the line and emit
  a `fail` entry with `reason: "unparseable-unit-line"`,
  `retryable: false`. Do not abort the whole batch — other units may
  still translate cleanly.
- **Source contains placeholder syntax the skill cannot identify**: if
  a `{...}` block isn't listed in `placeholders` and you can't tell
  whether it's a placeholder or literal braces, emit a `fail` entry
  with `reason: "ambiguous-placeholder"` and let a human triage it.
- **The skill is asked to re-run on a partially-filled batch**: read
  the existing `targets.jsonl`, identify the unit ids already covered,
  and resume from the next missing unit. Do not duplicate lines —
  `import-batch` validates 1:1 alignment with `units.jsonl` and
  rejects duplicates.
- **Glossary term conflicts with rule 1 (placeholder preservation)**:
  rule 1 wins. Placeholders are structural; glossary terms are
  textual. Don't substitute a placeholder for a glossary target.

## Notes for skill implementers

- `units.jsonl` and `targets.jsonl` are newline-delimited JSON. Parse
  line by line; skip blank lines. Each line is a complete JSON object.
- The `placeholders[*].icu_form.token` field is the canonical token
  string (`{0}`, `{name}`, etc.). Match against this string verbatim
  when validating your output.
- The batch directory may contain `*.tmp` files from atomic writes by
  `harness export-batch`. Ignore them — they're transient.
- The harness's validation gate runs *after* `import-batch` reads your
  output. Even if you produce a syntactically valid `targets.jsonl`,
  the gate may flag content issues (length, placeholders, glossary
  violations). Those become inline review items in the app, not
  failures you need to prevent — but rule 1 will block the catalog
  write outright, so make sure placeholders are correct.
- The skill is purely offline. No network calls are needed to read or
  write the batch directory.
- Schema version: `meta.json` records `"format_version": "1"`. If a
  future batch has a different version, refuse to run rather than
  guess at the new schema.
