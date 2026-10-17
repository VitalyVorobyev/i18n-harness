# Prompt profiles

A prompt profile describes how to ask one kind of model for a
translation: chat messages or a raw text to continue, the wording, stop
sequences and generation settings. Profiles are data — TOML files in
`crates/backend/profiles/` built into the program — not code.

## Which profile a model gets

The choice is made from what the server reports about the model, never
from its name:

| The server reports… | Profile |
|---|---|
| Ollama, a GGUF model whose prompt template is a bare pass-through (only `{{ .Prompt }}`-style placeholders, no turn markers), and no `tools` / `thinking` capability | `completion` |
| Ollama, any other model: a template with turn markers (`<start_of_turn>`, `<\|im_start\|>`, `[INST]`, …), a non-GGUF model (MLX, safetensors), or a model with `tools` / `thinking` capability | `chat` |
| Ollama, no template at all | `chat` |
| An OpenAI-compatible server (it reports no template) | `chat` |

In the app's model picker, models that get the `completion` profile are
marked *translation model*.

## How a string is sent

- One string per request. The text is sent **as it is in the file**,
  with the file's own placeholders (`%1`, `%(name)s`, `{name}`).
- A **plural** string is sent once per plural form, with an example
  number in place of the count placeholder (`%n` in Qt, the count in PO):
  for Slovak, `1 file(s)`, `2 file(s)`, `0 file(s)`. In the answer, that
  number is turned back into the placeholder; if the number does not
  appear exactly as often as in the request, the string fails with *the
  model changed the number in a plural form* and stays to do.
- Identical texts in one run are translated once.
- The answer is tidied before it is checked: `<think>…</think>` blocks,
  a leading `Translation:` / `Text:` / `<Language>:` label, quotes the
  original did not have, and extra lines after a one-line translation are
  removed.

## The built-in profiles

### `chat`

For instruction-following models.

```toml
id = "chat"
mode = "chat"

system = """
You translate the user interface of a software application. You reply with the translation only: no quotes, no explanations, no notes.
Keep unchanged: placeholders such as %1, %2, %n, %s, {0} or {name}; markup and tags such as <b>, </b>, <br/>; units such as mm and ms; numbers; and identifiers written as code (CDepthBitmap, PF_XYZ32, ADS).
Keep the length close to the original: the text must fit the same place in the interface. Keep the original's capitalization style for labels and buttons.
"""

prompt = """
Translate this {source_language} user-interface text into {target_language}.
{notes}
Text:
{text}
"""

stop = []
keep_alive = "15m"

[options]
temperature = 0.0
seed = 42
top_k = 1
```

`{notes}` expands to the lines that apply to the string, for example:

```text
Address the user formally.
Keep these terms unchanged: ParamsManager.
Use these translations: "layer" → "Ebene".
The text belongs to: MainWindow.
Meaning: verb, as in "to record".
Developer note: Shown next to the size field.
```

### `completion`

For translation models that continue a fixed pattern instead of
following instructions. They get no notes; glossary terms to keep are
still enforced by the [checks](checks.md#protected-term).

```toml
id = "completion"
mode = "completion"

prompt = """Translate this from {source_language} to {target_language}:
{source_language}: {text}
{target_language}:"""

stop = ["\nTranslate this from"]
stop_single_line = ["\n"]
keep_alive = "15m"

[options]
temperature = 0.0
seed = 42
top_k = 1
```

## Fields

| Field | Type | Meaning |
|---|---|---|
| `id` | string | Name of the profile. |
| `mode` | `"chat"` or `"completion"` | `chat`: a system and a user message (Ollama `/api/chat`, OpenAI `/v1/chat/completions`). `completion`: one raw prompt the model continues (Ollama `/api/generate` with `raw: true`, OpenAI `/v1/completions`). |
| `system` | string, optional | The system message (chat mode). |
| `prompt` | string | The user message (chat) or the whole prompt (completion). |
| `stop` | list of strings | Stop sequences. OpenAI-compatible servers accept at most four; extra ones are dropped. |
| `stop_single_line` | list of strings, optional | Used instead of `stop` when the text to translate is a single line. |
| `keep_alive` | string, optional | How long Ollama keeps the model loaded after a request (`"15m"`). |
| `[options] temperature` | number | Sampling temperature; `0` for repeatable output. |
| `[options] seed` | integer, optional | Random seed. |
| `[options] top_k` | integer, optional | Top-k sampling (`1` = always the most likely token). Ollama only. |
| `[options] top_p` | number, optional | Top-p sampling. |
| `[options] repeat_penalty` | number, optional | Repetition penalty. Ollama only. |

Unknown fields are an error.

### Template variables

| Variable | Value |
|---|---|
| `{source_language}` | English name of the language translated from (`English`, or the pivot language such as `German`). |
| `{target_language}` | English name of the target language (`Slovak`). |
| `{text}` | The text to translate, in the file's placeholder notation. |
| `{notes}` | Register, glossary terms, context and developer notes for this string; empty lines are dropped. |

Any other `{…}` in a template — including placeholders inside the text —
is left untouched. Lines left empty after substitution are removed.

The profiles cannot be replaced from a project folder; changing or
adding one means changing the program (see
[Extending the harness](../developers/extending.md#a-prompt-profile)).
