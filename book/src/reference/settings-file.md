# The settings file

Every folder opened with i18n-harness gets a settings file,
`i18n-harness.toml`, at its top level, and a state folder,
`.i18n-harness/`. The app creates the settings file the first time it
opens a folder (as does `harness translate-folder`; `harness init` creates
it explicitly). It is plain [TOML](https://toml.io): edit it by hand when
the app has no button for what you need, then reopen the folder.

A complete example:

```toml
[project]
name = "photo-app"
schema = 1

[locales.de_DE]
register = "formal"
length_warn_ratio = 1.5

[locales.sk_SK]

[[catalogs]]
path = "translations/app_de_DE.ts"
format = "qt-ts"
locale = "de_DE"

[[catalogs]]
path = "translations/app_sk_SK.ts"
format = "qt-ts"
locale = "sk_SK"
pivot = "de_DE"

[[references]]
path = "reference/app_de_DE_reviewed.ts"
format = "qt-ts"
locale = "de_DE"

[glossary]
path = "glossary.toml"

[backend.default]
kind = "ollama"
host = "http://localhost:11434"
model = "some-model:latest"
num_ctx = 8192

[paths]
state_dir = ".i18n-harness"
```

Unknown keys inside a known table are an error (typos are the usual
cause); unknown top-level tables are ignored, so a newer file still opens
in an older version where possible.

## `[project]`

| Key | Type | Meaning |
|---|---|---|
| `name` | string | Display name. Defaults to the folder name. |
| `schema` | integer | Version of the file layout. Currently `1`. Required. A file with a higher number than the program knows is refused. |

## `[locales.<id>]`

One table per language used by the catalogs, keyed by locale id
(`de`, `de_DE`, `zh_Hans`, `pt_BR`, …). All keys are optional; an empty
table means "use the defaults".

| Key | Type | Meaning |
|---|---|---|
| `register` | `"formal"`, `"informal"`, `"neutral"` | How to address the user. Chat models are told *Address the user formally/informally*. The built-in default is formal for languages that distinguish (German, Spanish, French, …) and neutral otherwise. |
| `length_warn_ratio` | number | The [length check](checks.md#length) warns when a translation is longer than the original by this factor. Defaults depend on the script (1.3 for Latin, 0.6 for Chinese, …; German 1.4). |
| `variant` | string | A variant tag such as `es_419`. Recorded; the current translation path does not use it. |

Plural rules, script and the language's name always come from CLDR and
cannot be overridden.

## `[[catalogs]]`

One entry per translation file.

| Key | Type | Meaning |
|---|---|---|
| `path` | string | Path relative to the folder. |
| `format` | string | `qt-ts`, `gettext-po` or `icu-json`. |
| `locale` | string | The language the file is translated into. Detected from the file (Qt `language` attribute, PO `Language:` header) or its name (`app_de.ts`, `de.po`, `fr.json`). |
| `pivot` | string, optional | The language the file's existing texts are in, when it is not `locale`. Set by **Language of the existing text…** in the app or `--pivot` on the command line. See [Files pre-filled with another language](../tutorials/pivot.md). |

A folder's files are discovered once, when the settings file is created.
To add a file later, add a `[[catalogs]]` entry (or delete the settings
file and reopen the folder, which loses your edits to it).

## `[[references]]`

Expert-translated files whose translations `harness reuse --project`
copies into the catalogs of the same language, by exact string id. They
are read, never written. Same keys as `[[catalogs]]` without `pivot`.

## `[glossary]`

| Key | Type | Meaning |
|---|---|---|
| `path` | string | Path of the glossary file, relative to the folder. See [The glossary file](glossary-file.md). |

Written by the app the first time you save a glossary. `harness init`
adds it when a `glossary.toml` exists.

## `[backend.default]`

The model server, as chosen in **Settings → Model server** and the model
picker.

| Key | Type | Meaning |
|---|---|---|
| `kind` | `"ollama"`, `"openai-compatible"` | Kind of server. (`"manual"` and `"agent"` are also accepted by the parser; the app treats anything but `openai-compatible` as Ollama.) |
| `host` | string | Server address. Ollama: `http://localhost:11434`; OpenAI-compatible: up to `/v1`. |
| `model` | string | The model, as the server names it. |
| `num_ctx` | integer | Context window in tokens (Ollama only). Omit for the model's default. |

No API key is ever stored here. Keys come from the environment:
`OPENAI_API_KEY` for OpenAI-compatible servers, `OLLAMA_API_KEY` for an
Ollama server behind an authenticating proxy. Without a `[backend.default]`
table, the app uses Ollama at the address in `OLLAMA_HOST`, or
`http://localhost:11434`.

The command-line tools do not read this table; they take the server and
model as options.

## `[prompts]`

| Key | Type | Meaning |
|---|---|---|
| `template_dir` | string | A folder of per-language prompt templates. Accepted for compatibility; the current translation path uses the built-in [prompt profiles](profiles.md) and does not read it. |

## `[paths]`

| Key | Type | Meaning |
|---|---|---|
| `state_dir` | string | Where the state folder lives. Relative to the settings file, or absolute. Default `.i18n-harness`. |

## The state folder `.i18n-harness/`

Bookkeeping the program writes as you work; it can be deleted (you lose
the *Done* / *Check* decisions not yet reflected in the files). Do not
commit it; add it to `.gitignore`.

| File | Contents |
|---|---|
| `review.jsonl` | One line per decision: a string confirmed (*Done*), a machine translation or draft awaiting a check. Each records a fingerprint of the original text; when the original changes, the decision no longer applies and the string is reviewed again. |
| other `.jsonl` files | Logs of earlier translation runs and evaluations, written by some command-line paths. |
