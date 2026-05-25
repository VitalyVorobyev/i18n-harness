# Translation batch — de_DE

## What is this folder?

This folder contains a translation batch produced by `harness export-batch`.
It contains 2 unit(s) to translate into **de_DE**.

## Files

| File | Purpose |
|---|---|
| `units.jsonl` | Source units — one JSON object per line. Read-only. |
| `prompt.md` | System prompt — read this before translating. |
| `targets.jsonl` | **Your output file.** Write one JSON line per unit. |
| `meta.json` | Audit record. Ignore during translation. |

## How to produce `targets.jsonl`

Write exactly **2 line(s)** to `targets.jsonl` — one per unit,
in the **same order** as `units.jsonl`. Each line must be a valid JSON object.

See the generated README for full instructions on singular, plural, skip, and fail entries.
