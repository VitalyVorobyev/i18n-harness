# Command line: harness-lab

`harness-lab` compares models (and inputs) on real catalogs. It ships in
the same `i18n-harness-cli-…` archives as `harness`; from a source
checkout run `cargo run -p i18n-harness-lab -- <command>`. The
[tutorial](../tutorials/lab.md) shows a complete session.

## bench

```text
harness-lab bench [OPTIONS] --config <CONFIG>… --out <OUT> <FOLDER>
```

Copies the `.ts`, `.po`, `.pot` and `.json` files at the top level of
`<FOLDER>` (not in sub-folders) into `<OUT>/work`, draws a stratified
sample of strings, translates it with each configuration through the
same session code as the app, and writes the evidence to `<OUT>`. The
copy is deleted at the end; `<FOLDER>` is never modified.

| Option | Default | Meaning |
|---|---|---|
| `--config <MODEL@INPUT>` | — | Required, repeatable. `MODEL` as Ollama names it; `INPUT` is `auto` (pivot text when sound, else the original) or `source` (always the original). |
| `--pivot <LOCALE>` | — | Language of the existing texts of every file. |
| `--host <URL>` | `http://localhost:11434` | Ollama address. Only Ollama is supported. |
| `--sample <N>` | `200` | Strings in the sample. |
| `--seed <N>` | `42` | Seed for the sample and the blind letters. |
| `--out <DIR>` | — | Required. Output folder. |

The sample contains every plural string, then draws the rest by kind,
each string counted in the first kind it matches:

| Stratum | Share | A string… |
|---|---|---|
| `plural` | all | is a plural string |
| `markup` | 15 % | contains `<…>` |
| `placeholder` | 20 % | contains `%` or `{` |
| `identifier` | 10 % | contains a code-like word (`snake_case`, `ALLCAPS`, `CamelCase`) |
| `no-pivot` | 15 % | has no usable pivot text although `--pivot` is given |
| `long` | 10 % | is longer than 60 characters |
| `short` | 15 % | is at most 15 characters |
| `other` | 15 % | anything else |

Identical inputs are sampled once.

### Output files

| File | Contents |
|---|---|
| `sample.jsonl` | `row`, `catalog`, `id`, `stratum`, `source`, `pivot` per sampled string. |
| `outputs.jsonl` | Per configuration and row: `config`, `row`, `input_locale`, `output` (forms), `errors`, `warnings`, `failure`, `seconds`. |
| `summary.json` | Per configuration: `config`, `model`, `input`, `strings`, `translated`, `failed`, `with_errors`, `with_warnings`, `median_seconds`, `total_seconds`. |
| `report.html` | The summary and every sample with all outputs and problems. |
| `judge_sheet.md` | Blind sheet: per row the original, the existing text and the distinct outputs under shuffled letters. |
| `judge_key.json` | `{ "<row>": { "<letter>": ["<config>", …] } }`. |

Configuration names in the outputs are shortened to the model's last
path segment without its tag, plus `@auto` / `@source`.

## score

```text
harness-lab score <OUT>
```

Reads `<OUT>/scores.jsonl`, which you write — one line per judged row:

```json
{"row": 2, "scores": {"A": 4, "B": 5}}
```

— joins it with `judge_key.json` and `sample.jsonl`, and writes and
prints `<OUT>/scores-summary.json`:

| Key | Meaning |
|---|---|
| `mean_score` | Average score per configuration. |
| `best_share` | Rows won per configuration; a tie splits the row. |
| `mean_score_by_stratum` | Average per `"<config> / <stratum>"`. |
| `rows_scored` | Number of rows scored. |
