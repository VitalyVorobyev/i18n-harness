# Command line: harness

`harness` is the command-line tool, for unattended runs, scripts and CI.
Everything it does with a folder, the desktop app does too.

```text
harness <COMMAND> [OPTIONS]
harness <COMMAND> --help
```

| Command | What it does |
|---|---|
| [`translate-folder`](#translate-folder) | Translate everything still to do in a folder, as the app does. |
| [`import-glossary`](#import-glossary) | Merge a term table from a spreadsheet into a folder's glossary. |
| [`round-trip`](#round-trip) | Prove a file is read and written back byte for byte. |
| [`gate`](#gate) | Run the checks on a file's translations. |
| [`init`](#init) | Create `i18n-harness.toml` for a folder. |
| [`open`](#open) | Validate and summarise a folder's settings file. |
| [`export-batch`](#export-batch) | Write a batch folder for an AI coding agent. |
| [`import-batch`](#import-batch) | Check an agent's answers and write the catalog. |
| [`reuse`](#reuse) | Copy translations from reference files by string id. |
| [`split-remainder`](#split-remainder) | Write the untranslated strings to a separate file. |
| [`merge`](#merge) | Fold a translated remainder back into its base file. |

Locale ids are CLDR ids with underscores: `de`, `de_DE`, `sk_SK`,
`zh_Hans`, `pt_BR`. File formats are detected from the content.

## Get the command-line tools

Each [release](https://github.com/VitalyVorobyev/i18n-harness/releases/latest)
has an archive per system: `i18n-harness-cli-<version>-macos-arm64.tar.gz`
(Apple silicon), `…-macos-x64.tar.gz` (Intel Mac), `…-linux-x64.tar.gz`
and `…-windows-x64.zip`. Each holds `harness` and
[`harness-lab`](lab-cli.md). Unpack it and put the two programs in a
folder on your `PATH`, for example on macOS and Linux:

```sh
tar xzf i18n-harness-cli-*-macos-arm64.tar.gz
mkdir -p ~/.local/bin && cp i18n-harness-cli-*/harness* ~/.local/bin/
harness --version
```

On macOS, if the system refuses to run a downloaded program, remove the
quarantine mark once: `xattr -d com.apple.quarantine ~/.local/bin/harness ~/.local/bin/harness-lab`.
The `.sha256` file next to each archive holds its checksum
(`shasum -a 256 -c <file>.sha256`).

## translate-folder

```text
harness translate-folder [OPTIONS] --model <MODEL> <FOLDER>
```

Opens `<FOLDER>` (creating `i18n-harness.toml` when missing), translates
every **To do** string of every file with the model, prints the strings
whose translation has an error and the counts per file. It works
exactly like **Translate all files** in the desktop app.

| Option | Default | Meaning |
|---|---|---|
| `--model <MODEL>` | — | Required. Model id as the server names it. |
| `--pivot <LOCALE>` | — | Language of the existing texts of **every** file; recorded in the settings file. |
| `--input <auto\|source>` | `auto` | `auto`: from the sound pivot text when there is one, else the original. `source`: always the original. |
| `--server <ollama\|openai-compatible>` | `ollama` | Kind of server. |
| `--host <URL>` | `http://localhost:11434` for Ollama, `http://localhost:1234/v1` for OpenAI-compatible | Server address (up to `/v1` for OpenAI-compatible). |
| `--deliver` | off | Mark translations without errors as done (finished in the file), also those with a warning. Otherwise they await a check. |
| `--fix-glossary` | off | Instead of the strings to do, retranslate every translated string that breaks the glossary, whatever its status. Prints how many still break it afterwards. |
| `--save` | off | Write the files at the end. Translations with errors keep the file's previous text. |

Environment: `OPENAI_API_KEY` / `OLLAMA_API_KEY` (bearer token).
Exit status: 0 when the run completed (also with held-back
translations); non-zero when the folder cannot be opened or the server
fails. See the [tutorial](../tutorials/translate-folder.md).

## import-glossary

```text
harness import-glossary --language <LOCALE> <FOLDER> <TABLE>
```

Merges a term table into the glossary of `<FOLDER>` (creating
`i18n-harness.toml` and `glossary.toml` when missing). `<TABLE>` is text
as a spreadsheet saves it — CSV, semicolon-separated or tab-separated —
with the columns *term*, *translation*, *note*; see
[the term table](../tutorials/glossary.md#the-term-table). `--language`
is the language of the translation column (`sk_SK`). A term already in
the glossary is updated, a new one added; nothing is removed.

```sh
harness import-glossary --language sk_SK ./catalogs terms.csv
harness translate-folder ./catalogs --model <model> --fix-glossary --save
```

## round-trip

```text
harness round-trip <PATH>
```

Reads the file and writes it back with no changes, in memory, and
compares the bytes. Prints `round-trip OK: <path> (<bytes> bytes, <n>
units)` and exits 0, or reports the first difference and exits non-zero.
Use it to confirm the harness handles one of your files exactly before
you trust it with that file.

## gate

```text
harness gate [OPTIONS] --locale <LOCALE> <PATH>
```

Runs the [checks](checks.md) on every translation in the file and prints
one section per string with findings, then a summary:

```text
gate report: app_de.ts  locale=de_DE  units=5  hard=0  soft=0  clean=5
```

`hard` counts problems (translations that would break the application),
`soft` counts warnings. Exits non-zero if there is any problem, so it
can run as a CI step.

| Option | Default | Meaning |
|---|---|---|
| `--locale <LOCALE>` | — | Required. The file's target language. |
| `--metrics <FILE>` | — | Append one JSON line per finding to this file. |
| `--backend <LABEL>` | `manual` | Label recorded in the metrics lines. |

## init

```text
harness init [--force] <DIR>
```

Searches `<DIR>` for translation files (recursively, up to 8 levels,
skipping hidden folders and `node_modules`, `target`, `dist`, `build`, `__pycache__`), detects
format and language of each, and writes `<DIR>/i18n-harness.toml`. An
existing `glossary.toml` is referenced. Refuses to overwrite an existing
settings file unless `--force` is given. Prints what it found.

## open

```text
harness open <DIR>
```

Loads and validates `<DIR>/i18n-harness.toml` and prints a summary
(name, locales, catalogs, glossary, backend, state folder). Exits
non-zero if the file is missing or invalid.

## export-batch

```text
harness export-batch [OPTIONS] --locale <LOCALE> --out <OUT> <PATH>
harness export-batch [OPTIONS] --locale <LOCALE> --out <OUT> --project <DIR>
```

Writes a batch folder (`README.md`, `prompt.md`, `units.jsonl`, an empty
`targets.jsonl`, `meta.json`) with every string that is not finished,
for an external agent to translate. Prints the absolute path of the
folder. No model, no network. See
[Hand a batch to an AI coding agent](../tutorials/agent.md).

| Option | Meaning |
|---|---|
| `<PATH>` | Single-file mode: the catalog to export. |
| `--project <DIR>` | Folder mode: every catalog of `--locale` in the folder's settings file, one sub-folder each; the folder's glossary is used. |
| `--locale <LOCALE>` | Required. Target language. |
| `--out <OUT>` | Required. Where to write the batch folder (created if absent). |
| `--glossary <FILE>` | Single-file mode only: glossary to include in `prompt.md`. |

## import-batch

```text
harness import-batch [OPTIONS] --apply <CATALOG> <DIR>
harness import-batch [OPTIONS] --project <PROJECT> <DIR>
```

Reads `targets.jsonl` from the batch folder `<DIR>`, runs the checks and
reports. Without `--out` / `--out-dir` it is a dry run. With them, the
catalog is written: translations without findings as finished, those with
warnings as unfinished. **If any translation has an error, nothing is
written for that catalog** and the command exits non-zero.

| Option | Meaning |
|---|---|
| `--apply <CATALOG>` | Single-file mode: the catalog the batch was exported from. |
| `--project <DIR>` | Folder mode: the folder the batch was exported from. |
| `--out <FILE>` | Single-file mode: write the result here (may be the original file). |
| `--out-dir <DIR>` | Folder mode: write each catalog under this folder, keeping its relative path. |
| `--metrics <FILE>` | Append one JSON line per finding. |

## reuse

```text
harness reuse [OPTIONS] --locale <LOCALE> --reference <REF>… <PATH>
harness reuse --locale <LOCALE> --project <DIR>
```

Copies translations from reviewed reference files into a base file
wherever the **string id is identical** (no fuzzy matching, no model).
When several references agree, the translation is copied; when they
disagree, the string is reported as a conflict and left alone.

```text
reuse report: app_de.ts  copied_finished=2  copied_needs_review=0  conflicts=0  remaining=2
  copied provenance:
    MainWindow::&Open... ← reviewed_de.ts [finished]
```

| Option | Meaning |
|---|---|
| `<PATH>` | Single-file mode: the base file. Overwritten in place unless `--out` is given. |
| `--reference <REF>` | Single-file mode: a reference file in the same format. Repeat for several. |
| `--out <FILE>` | Single-file mode: write here instead of in place. |
| `--glossary <FILE>` | Single-file mode: glossary for the checks run on copied translations. |
| `--project <DIR>` | Folder mode: bases are the folder's catalogs of `--locale`, references its `[[references]]` of that locale; written in place. |
| `--locale <LOCALE>` | Required. |

Qt `.ts` files only.

## split-remainder

```text
harness split-remainder --out <OUT> <BASE>
```

Writes the strings of `<BASE>` that still have no translation into a new
file `<OUT>` of the same format — typically right after `reuse`, to send
only the leftover to a translator. `<BASE>` is not changed.

```text
split-remainder: app_de.ts → rest_de.ts (2 string(s) to translate)
```

## merge

```text
harness merge --with <REMAINDER> --out <OUT> <BASE>
```

Folds a translated remainder back into its base and writes the result to
`<OUT>`. Every id in the remainder must exist in the base; an id that is
finished in both files is an error (overlap).
