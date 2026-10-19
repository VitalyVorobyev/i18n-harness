# Hand a batch to an AI coding agent

If you already work with an AI coding agent — Claude Code, GitHub
Copilot, Codex or similar — it can do the translating, while the harness
keeps doing everything else: extracting the strings, checking the
answers and writing the file. The exchange happens through a folder of
plain files, in two phases:

1. `harness export-batch` writes a **batch folder** with the strings to
   translate and the rules.
2. The agent fills in one file in that folder, `targets.jsonl`.
3. `harness import-batch` reads the answers, runs the
   [checks](../reference/checks.md) and writes the catalog.

The harness itself makes no network connection in either phase.

## 1. Export

Using `app_de.ts` from the [Qt tutorial](qt.md):

```sh
harness export-batch app_de.ts --locale de_DE --out batch-de
```

The command prints the absolute path of the batch folder. It contains:

| File | Purpose |
|---|---|
| `README.md` | The rules and the exact line format of `targets.jsonl`. The agent reads this first. |
| `prompt.md` | Target language, register, glossary terms and terms to keep. |
| `units.jsonl` | The strings, one JSON object per line. |
| `targets.jsonl` | Empty. The agent writes its answers here. |
| `meta.json` | Where the batch came from. |

`units.jsonl` holds every string that is not finished yet, one per
line. Placeholders appear in one common notation, whatever the file
format — Qt's `%1` becomes `{0}`, `%n` becomes `{count}` — so the agent
needs to know only one set of rules:

```json
{"id":"MainWindow::Saved %1 to %2","source":"Saved {0} to {1}","placeholders":[…],"flags_so_far":[]}
```

`id` identifies the string and must be copied unchanged into the
answer; `source` is the text to translate; `placeholders` lists the
placeholders the translation must keep. A plural string also has
`plural_arity`, the number of forms the answer must give. `flags_so_far` lists problems
already known for the string (usually none) and needs no action.

Add `--glossary glossary.toml` to include a glossary in `prompt.md`.
For a folder with an `i18n-harness.toml`, use `--project <folder>`
instead of a file: every file of that language gets its own sub-folder,
and the folder's glossary is used.

## 2. Let the agent translate

**Claude Code.** There is a ready-made skill for exactly this job,
[`translate-i18n-batch`](https://github.com/VitalyVorobyev/i18n-harness/tree/main/.claude/skills/translate-i18n-batch).
Copy that folder into your project's `.claude/skills/` (or into
`~/.claude/skills/` to have it everywhere) — or create a folder
`translate-i18n-batch` there and download its `SKILL.md` into it. Then
start Claude Code and run:

```text
/translate-i18n-batch /absolute/path/to/batch-de
```

**Any other agent.** Point it at the folder and ask:

> Read `README.md` and `prompt.md` in `batch-de/`, then translate every
> line of `units.jsonl` and write one line per unit to `targets.jsonl`,
> in the same order, exactly in the format `README.md` describes.

The answer lines look like this:

```json
{"id":"MainWindow::%n file(s) selected","kind":"plural","texts":["{count} Datei ausgewählt","{count} Dateien ausgewählt"]}
{"id":"MainWindow::&Open...","kind":"singular","text":"&Öffnen..."}
{"id":"MainWindow::<b>Warning:</b> the file is read-only.","kind":"singular","text":"<b>Warnung:</b> Die Datei ist schreibgeschützt."}
{"id":"MainWindow::Saved %1 to %2","kind":"singular","text":"{0} wurde unter {1} gespeichert"}
```

A line may also be `{"id": …, "kind": "skip"}` or
`{"id": …, "kind": "fail", "reason": "…", "retryable": false}` for strings
the agent cannot translate.

## 3. Import

First a dry run, which checks without writing:

```sh
harness import-batch batch-de --apply app_de.ts
```

```text
MainWindow::Saved %1 to %2
  [soft] length-warn: 31 chars vs source 16 (ratio 1.94, threshold 1.40)

dry-run (no --out); skipped write-back
translate report: app_de.ts → locale=de_DE backend=agent  writable=4 translated=4 skipped=0 failed=0 finished=3 flagged=1 hard=0 soft=1
```

Each string with a finding is listed with its findings. `[HARD]` marks
an error that blocks writing (such as a lost placeholder); `[soft]`
marks a warning, here that the translation is much longer than the
original. The last line sums up: `writable` strings in the batch,
`translated` answers received, `skipped` and `failed` lines,
`finished` translations without findings, `flagged` translations with
at least one finding, and the number of errors (`hard`) and warnings
(`soft`). See [Checks](../reference/checks.md) for what each finding
means.

Then write the result — to a new file, or over the original:

```sh
harness import-batch batch-de --apply app_de.ts --out app_de.ts
```

Placeholders are converted back (`{0}` → `%1`, `{count}` → `%n`).
Translations without findings are written as finished; those with a
warning (`[soft]`) are written but stay unfinished (`type="unfinished"`
in Qt) for review.

If **any** answer has an error (`[HARD]`, for example a lost
placeholder), `import-batch` writes **nothing** for that catalog, prints
`blocked: N hard finding(s); not writing …` and exits with a non-zero
status:

```text
MainWindow::Saved %1 to %2
  [HARD] placeholder-mismatch: slot 0: missing=["{1}"] extra=[]

blocked: 1 hard finding(s); not writing app_de.ts (use `harness gate` to inspect)
```

`slot 0` is the first (or only) form of the string; `{1}` is the second
placeholder, `%2` in the Qt file. Correct those lines in `targets.jsonl` (or ask the agent to) and
import again. This is stricter than the app, which holds back only the
broken strings.

Open the folder in the app afterwards to review what is left under
**Check**.

In project mode, use `--project <folder>` and `--out-dir <dir>`; each
catalog is written under `<dir>` with its path inside the project.
