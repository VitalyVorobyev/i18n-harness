# Choose a model with harness-lab

Which model translates *your* application best is not something a model
card can tell you. `harness-lab` answers it on your own files: it
translates the same sample of strings with several models (and, for
files pre-filled with another language, with both inputs), counts the
problems the checks find, measures the time, and prepares a **blind
judging sheet** so a person who knows the language can score the
results without knowing which model wrote what.

`harness-lab` is a separate command-line tool for whoever chooses the
model; it is not part of the desktop app. It works with Ollama.

## Before you start

- Install the command-line tools (see
  [Get the command-line tools](../reference/cli.md#get-the-command-line-tools)).
- Pull the candidate models into Ollama
  ([Install Ollama and get a model](../getting-started/models.md)).
- Put the catalogs to test **directly** in one folder. `harness-lab`
  copies the `.ts`, `.po`, `.pot` and `.json` files at the top level of
  that folder into its output folder and works on the copies; files in
  sub-folders are not included. Your files are never modified.

## 1. Run the benchmark

```sh
harness-lab bench my-catalogs \
  --config 'model-a:latest@auto' \
  --config 'model-b:latest@auto' \
  --config 'model-b:latest@source' \
  --pivot de_DE \
  --sample 200 \
  --out bench-sk
```

- Each `--config` is `<model>@<input>`, with the model named as
  `ollama list` shows it. `@auto` translates from the existing text when
  `--pivot` is given and that text is sound, otherwise from the original;
  `@source` always translates from the original.
- `--pivot` is only for files pre-filled with another language (see
  [that tutorial](pivot.md)); leave it out otherwise.
- `--sample` is the number of strings (default 200). The sample is
  *stratified*: every plural string is included, and the rest is drawn
  across kinds of strings — with markup, with placeholders, with
  identifiers, without a usable pre-filled text, long, short, other — so
  the hard cases are represented.
- `--seed` (default 42) makes the sample and the blind letters
  repeatable. Same files, same seed, same sample.
- `--host` points at another Ollama address.

Each model is loaded once before timing starts. Progress and a one-line
summary per configuration are printed:

```text
sample: 200 strings
model-a@auto: 200/200 translated, 3 with errors, 11 with warnings, median 0.48s
...
wrote bench-sk
```

## 2. Read the report

`bench-sk/` now contains:

| File | Contents |
|---|---|
| `report.html` | Open in a browser: a summary table per configuration (translated, with errors, with warnings, median and total seconds) and every sampled string with each configuration's output and problems side by side. |
| `summary.json` | The summary table as JSON. |
| `sample.jsonl` | The sampled strings: row number, file, id, kind (`stratum`), original and pre-filled text. |
| `outputs.jsonl` | One line per configuration and row: output forms, errors, warnings, failure, seconds. |
| `judge_sheet.md` | The blind judging sheet (next step). |
| `judge_key.json` | Which letter is which configuration, per row. **Do not open it before judging.** |

Errors and warnings are the same [checks](../reference/checks.md) the app
runs. A configuration with many errors needs much more manual repair; one
with few errors may still translate badly — that is what judging is for.

## 3. Judge blind

Give `judge_sheet.md` to someone fluent in the target language. For each
row it lists the original, the pre-filled text if any, and the candidate
translations under letters:

```markdown
## 2 (markup)

- Original: `<b>Ignore</b> defects under %1 mm`
- Existing: `<b>Ignoriere</b> Defekte unter %1 mm`
- A: `<b>Ignoruj</b> chyby menšie ako %1 mm`
- B: `<b>Ignorovať</b> chyby pod %1 mm`
```

Letters are shuffled per row, and identical outputs share a letter, so
the judge scores each distinct text once. Score each candidate from 1
(wrong) to 5 (perfect UI text).

Write the scores into `bench-sk/scores.jsonl`, one line per judged row,
with the row number and a score per letter:

```json
{"row": 1, "scores": {"A": 3}}
{"row": 2, "scores": {"A": 4, "B": 5}}
```

Rows you skip are simply left out.

## 4. Score

```sh
harness-lab score bench-sk
```

This joins the scores with `judge_key.json` and prints (and writes to
`bench-sk/scores-summary.json`):

- `mean_score` — average score per configuration;
- `best_share` — how many rows each configuration won (ties split);
- `mean_score_by_stratum` — the average per configuration and kind of
  string, e.g. `model-b@auto / markup`;
- `rows_scored`.

Pick the configuration with the best scores at an error count and speed
you can live with, and select that model in the app.
