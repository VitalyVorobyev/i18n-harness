# tune-i18n-prompt — Claude Code skill spec

A Claude Code skill that consumes an i18n-harness tuning bundle, diffs the
LLM's proposals against the human-accepted translations, drafts a refined
prompt template, and writes it back to the project.

## Why this skill exists

The harness runs translations locally with a small model (Gemma 4 2B by
default). Small models drift on idioms, brand terms, and tone. The translator
accepts the parts that are right and edits the rest; every edit is recorded.
A larger model (Claude) is well-suited to look at the edits, infer the failure
pattern, and rewrite the prompt to compensate — but the larger model never
runs in-app (no API keys; no telemetry).

This skill bridges the two: the user exports a bundle from the app, runs the
skill in Claude Code, gets back a candidate prompt template, and runs
evaluation in the app to verify the new score.

## Bundle contents (`.i18n-harness/tuning/<ISO-timestamp>/`)

- `examples.jsonl` — one JSON object per line. Schema documented below.
- `prompt.txt` — the prompt template that produced these corrections.
- `score.json` — the latest evaluation run (omitted if none has been run yet).
- `locales.toml` — the project's locale records.
- `README.md` — this file (verbatim copy for self-documentation).

### `examples.jsonl` schema

Each line is a single self-contained JSON object. All string fields are UTF-8.

```json
{
  "schema": 1,
  "source": "Save file",
  "mt_proposal": "Datei speichern",
  "human_target": "Datei sichern",
  "locale": "de_DE",
  "flags": ["brand-term"],
  "note": "Translator prefers 'sichern' over 'speichern' for this product."
}
```

Field descriptions:

| Field | Type | Required | Description |
|---|---|---|---|
| `schema` | integer | yes | Line schema version. Currently `1`. |
| `source` | string | yes | Source text that was being translated. |
| `mt_proposal` | string | yes | What the small model proposed. Empty when human-from-scratch. |
| `human_target` | string | yes | The translation the human accepted/wrote. |
| `locale` | string | yes | Target locale id (e.g. `"de_DE"`, `"fr_FR"`). |
| `flags` | string array | no | Flag kinds the unit had at correction time (kebab-case). Omitted when empty. |
| `note` | string | no | Human teaching note added at curation time. Omitted when none. |

Allowed flag kind values (from `crates/core/src/flag.rs`, model-produced only):
- `ambiguous-source` — source could mean multiple things
- `insufficient-context` — source is too short or generic to translate confidently
- `idiom` — source uses idiom or wordplay; literal translation loses meaning
- `low-confidence` — model was unsure for an unspecified reason
- `brand-term` — looks like a product or brand name not in the glossary
- `tone-mismatch` — source register is ambiguous or hard to carry over

### `prompt.txt`

The active prompt template at export time. Verbatim — no preprocessing. The
template uses `{token}` substitution for runtime values (`{source}`,
`{locale}`, `{register}`, `{glossary_block_or_(none)}`, etc.). Unknown tokens
are left verbatim in the output, which is intentional: the template body
contains instructional examples like `{count}` and `{{var}}` that are not
tokens but illustrate placeholder syntax for the model.

The skill's job is to produce a refined version that fits the same strict JSON
output schema:
```json
{"translation": "<string>", "flags": [{"kind": "<kind>", "note": "<optional>"}], "confidence": <0.0–1.0>}
```

### `score.json` schema

The latest in-app evaluation run. Absent when no evaluation has been run yet;
the skill should note this and recommend the user run one before and after to
measure improvement.

```json
{
  "schema": 1,
  "ts": "2026-05-24T12:34:56.000000Z",
  "prompt_template_version": "ollama-translate-v2",
  "overall_score": 0.72,
  "example_count": 14,
  "per_locale": {
    "de_DE": {"score": 0.75, "count": 8},
    "fr_FR": {"score": 0.67, "count": 6}
  },
  "per_flag_kind": {
    "idiom": {"score": 0.40, "count": 5},
    "brand-term": {"score": 0.60, "count": 3}
  }
}
```

Key fields:

| Field | Description |
|---|---|
| `overall_score` | Mean exact-match score across all examples. `1.0` = perfect match; `0.0` = every example differed. |
| `per_locale` | Per-locale breakdown: `score` and `count`. Low scores identify locales that need targeted prompt help. |
| `per_flag_kind` | Per-flag breakdown: examples where the model had flagged the unit. Low scores here show which failure patterns the model itself recognized but still got wrong. |

### `locales.toml` schema

TOML table of per-locale config overrides from the project manifest. Uses the
locale id as the section header.

```toml
[de_DE]
register = "formal"
length_warn_ratio = 1.4

[fr_FR]
register = "neutral"
```

Fields per locale: `register` (`formal` | `informal` | `neutral`), `variant`
(e.g. `"es_419"`), `length_warn_ratio` (float). All fields are optional.

## What the skill does

1. Read and parse the bundle (all five files; `score.json` optional).
2. For each example in `examples.jsonl`, compute the character-level or
   token-level diff between `mt_proposal` and `human_target`.
3. Cluster the diffs by locale and by flag kind. Look for systematic patterns:
   - Brand terms swapped (flag: `brand-term`)
   - Tone inconsistency — formal phrasing in an informal locale (flag: `tone-mismatch`)
   - Idiom mistranslated literally (flag: `idiom`)
   - Translation consistently too long (check against `locales.toml`
     `length_warn_ratio`)
   - Glossary terms violated (compare `mt_proposal` against known term pairs)
4. Identify the top 3–5 failure patterns ranked by frequency across examples.
5. Draft a refined prompt template that addresses those patterns. Constraints:
   - Must produce the same strict JSON output schema shown above.
   - Allowed `flag.kind` values are unchanged (do not invent new kinds).
   - Template length under ~200 lines (Gemma latency budget).
   - Do not alter the `{token}` substitution surface — unknown tokens pass
     through verbatim and that behavior is load-bearing.
   - If `score.json` is present, note its `overall_score` as the baseline.
6. Write the candidate:
   - If failures are concentrated in one locale: write to
     `prompts/<locale_id>.txt` relative to the project root.
   - If failures are cross-locale: write to `prompts/default.txt`.
   - Ask the user to confirm the destination before writing.
7. Instruct the user to open the project in the app and click "Run evaluation"
   in the Quality tab to verify the new score is higher than the baseline in
   `score.json`.

## What this skill does NOT do

- Run the new prompt itself (no local Ollama access from inside the skill).
- Persist score history — the app owns `evaluations.jsonl`.
- Mutate the project manifest — only writes to `prompts/`.
- Choose between per-locale and global replacement unilaterally — the skill
  recommends and asks the user to confirm.
- Modify `examples.jsonl` or any other bundle file — the bundle is read-only
  input.

## How to invoke

Point the skill at the bundle directory. The simplest workflow:

1. In the app: Quality tab → "Export tuning bundle". Copy the path from the
   success toast.
2. In Claude Code:

   ```
   /tune-i18n-prompt <bundle-path>
   ```

   Example:
   ```
   /tune-i18n-prompt /home/user/my-app/.i18n-harness/tuning/2026-05-24T12-34-56-000000
   ```

3. Review the diff Claude proposes for `prompt.txt`.
4. Confirm writing `prompts/default.txt` (or the per-locale variant).
5. Back in the app: Quality tab → "Run evaluation". Compare the new overall
   score against the baseline in `score.json`.

## Failure modes

- **`score.json` absent**: warn the user and continue. The refined prompt will
  be written but there is no baseline to compare against. Recommend running an
  evaluation before the next export so future bundles include a baseline.
- **All `mt_proposal` fields empty**: the corrections were human-from-scratch
  (no model output to diff against). The skill can still analyze `human_target`
  values for glossary and tone patterns but cannot produce a diff-based
  diagnosis. Acknowledge this limitation in the output.
- **Fewer than 3 resolved examples**: warn that the sample is small and the
  diagnosis may be noisy. Suggest promoting more corrections in the app.

## Notes for skill implementers

- The `examples.jsonl` file is newline-delimited JSON. Parse line by line;
  skip blank lines. Each line is a complete JSON object.
- The `prompt.txt` file has a `[template=<version>]` header on its first
  non-blank line. Preserve this header in the output so the version tag
  survives the rewrite.
- `locales.toml` uses `[<locale_id>]` sections (not `[locales.<locale_id>]`).
  This is intentional — it matches the per-bundle export format, not the
  project manifest format.
- The bundle is self-contained and offline. No network calls are needed to
  read it.
