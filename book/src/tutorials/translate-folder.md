# Unattended runs with translate-folder

For large files, nightly jobs or a build server, `harness translate-folder`
does what **Translate all files** does in the app — without the app. It
uses the same code as the app, so the statuses, checks and save rules are
identical, and you can open the folder in the app afterwards to review.

## Before you start

Install the command-line tools (see
[Get the command-line tools](../reference/cli.md#get-the-command-line-tools))
and make sure the
model is available on the server (`ollama list`).

## 1. A trial run

```sh
harness translate-folder qt-demo --model <model>
```

This opens `qt-demo` (creating `i18n-harness.toml` if there is none),
translates every **To do** string of every file, prints what it found and
**writes nothing** to the translation files:

```text
translating 4 strings with <model>
  4/4 (0.5 strings/s)
done: 4 translated, 0 failed in 8 s
app_de.ts: <b>Warning:</b> the file is read-only.
  → Warnung: Die Datei ist schreibgeschützt.
  ! Formatting tags do not match the original: missing </b>, <b>
not saved (pass --save to write the catalogs)
app_de.ts: 0 to do, 4 to check, 1 done, 1 with errors
```

- Progress goes to standard error, every 100 strings and at the end.
- Every translation with an error is listed on standard output: the
  original, `→` the translation, `!` the problem.
- The last lines give the counts per file.

The first run of a session is slower: the model is loaded first.

## 2. Save

```sh
harness translate-folder qt-demo --model <model> --save
```

Now the files are written at the end of the run. As in the app,
translations with errors are held back — the file keeps its previous
text for them — and the run says so:

```text
saved 1 file(s); 1 translation(s) with errors kept their previous text
```

The new translations have the status **Check**. Open the folder in the
app to review them; fix the held-back ones there.

## 3. Deliver

When the output is delivered without a human review (a first draft for
translators, a preview build), add `--deliver`:

```sh
harness translate-folder qt-demo --model <model> --save --deliver
```

Translations **without errors** become **Done** (in a Qt file,
`type="unfinished"` is removed), including those with a warning; review
the warnings later in the app if you can. Translations with an error are
still held back.

## All options

| Option | Meaning |
|---|---|
| `<FOLDER>` | Folder with the catalogs. |
| `--model <MODEL>` | Model to translate with, as the server names it. Required. |
| `--pivot <LANG>` | The language the files' existing texts are in, e.g. `de_DE` (see [Files pre-filled with another language](pivot.md)). Applied to **every** file in the folder and recorded in `i18n-harness.toml`. |
| `--input auto\|source` | `auto` (default): translate from the existing text when there is a pivot language and that text is sound, else from the original. `source`: always from the original. |
| `--server ollama\|openai-compatible` | Kind of server. Default `ollama`. |
| `--host <URL>` | Server address. Default `http://localhost:11434`; for an OpenAI-compatible server give the address up to `/v1`, e.g. `http://localhost:1234/v1`. |
| `--save` | Write the files at the end. |
| `--deliver` | Mark translations without errors as done. |

The model server settings stored in the folder's `i18n-harness.toml` (by
the app) are **not** used by this command; pass `--model`, `--server`
and `--host` explicitly. An API key is read from `OPENAI_API_KEY` (or
`OLLAMA_API_KEY` for an Ollama server behind a proxy).

## In a script

`translate-folder` exits with status 0 when the run completed, even if
some translations have errors (they are listed and held back). It exits
non-zero when the folder cannot be opened or the server cannot be
reached. To fail a job on translation errors, check the standard output
for lines starting with `  ! `:

```sh
harness translate-folder app/i18n --model "$MODEL" --save | tee run.log
! grep -q '^  ! ' run.log
```
