# Troubleshooting

## "Ollama is not running"

The start screen, or **Model server not reachable** in the toolbar,
means the app got no answer from the model server.

1. Start Ollama (menu bar / system tray on macOS and Windows;
   `ollama serve` or the system service on Linux).
2. In a terminal, run `ollama list`. If that fails too, Ollama is not
   running or not installed.
3. If Ollama runs on another address, set it in **Settings → Model
   server → Address** and click **Test connection**. The message under
   the button shows the exact error.
4. The default address can also come from the `OLLAMA_HOST` environment
   variable. It must be a full URL such as `http://192.168.1.10:11434`.
   A value like `0.0.0.0:11434` (common for making Ollama listen on all
   interfaces) is not a usable address for the app — unset it for the
   app, or enter the address in Settings.

The start screen always checks Ollama at the default address. If your
folder uses an OpenAI-compatible server instead, ignore that message.

## A model is not in the list

- Run `ollama list`; the app shows what Ollama reports. Pull the model
  first ([Install Ollama and get a model](../getting-started/models.md)).
- **Embedding models are hidden on purpose**: models that only produce
  embeddings (for search) cannot write text.
- The list is read when a folder is opened and after **Save settings**.
  Pulled a model while the app was open? Close the folder and open it
  again.
- On an OpenAI-compatible server, the list comes from its `/v1/models`;
  in LM Studio, load the model first.

## The first string takes a long time

The first request of a session loads the model into memory, which can
take from seconds to a few minutes for a large model on a slow disk.
The following strings are much faster. Ollama keeps the model loaded for
15 minutes after the last request; after that, the next request loads it
again.

If every string is slow, the model is probably too large for your
memory and runs partly on the CPU. Try a smaller model or a smaller
quantization, and keep **Context size** empty unless you need it.

## Translations were "held back" when saving

*Saved, with problems held back* means some translations have an error
(red) and were not written; the file keeps its previous text for them.
Nothing is lost: the translations are still in the app.

1. Turn on **Problems only** (it shows the number of problems).
2. Fix each string as the message says — usually a placeholder, tag or
   protected term that went missing — or click **Translate again**.
3. Save again.

See [Checks](checks.md) for every message.

## Translation of one string failed

The run bar counts failed strings (*N failed*). A string fails when the
model returns nothing usable, or — for plural strings — changed the
example number. Failed strings stay **To do**; translate them again or
try another model.

## "Could not open the folder"

- *no translation files found* — the folder (and its sub-folders, up to
  8 levels, excluding hidden folders, `node_modules`, `target`, `dist`,
  `build`) contains no recognised file. Check the formats in
  [Formats and placeholders](formats.md).
- *unsupported placeholder syntax* — a PO file uses printf width or
  precision (`%-10s`, `%.2f`). Run `harness gate --locale <id> <file>` on
  each `.po` file to find which.
- *TOML parse error* in `i18n-harness.toml` — the settings file was edited
  and has a mistake; the message gives the line.
- *declared as icu-json but its content looks like an unknown format* — a
  JSON file whose top level holds only nested objects; see the
  [ICU JSON tutorial](../tutorials/icu-json.md).

## A new translation file is not listed

Files are discovered when the settings file is first created. Add a
`[[catalogs]]` entry for the new file to `i18n-harness.toml` (see
[The settings file](settings-file.md#catalogs)) and reopen the folder.

## The language of a file is wrong

Edit `locale` in the file's `[[catalogs]]` entry and reopen the folder.
If the file's existing texts are in another language, use **Language of
the existing text…** instead; see
[Files pre-filled with another language](../tutorials/pivot.md).

## A blank window after building the app yourself

Older versions of the app opened an empty window when built with a
plain `cargo build`. Current versions embed the interface in every
build; update your checkout. If you build with
`--no-default-features`, add `--features custom-protocol` back (only
`bun tauri:dev` builds without it, on purpose, to load the interface
from the development server).

## The app does not see my API key

The key must be in the environment of the app's process, as
`OPENAI_API_KEY`. On macOS, apps started from the Dock do not read your
shell profile; see
[Use an OpenAI-compatible server](../tutorials/openai-compatible.md#2-provide-the-api-key-if-the-server-needs-one).
