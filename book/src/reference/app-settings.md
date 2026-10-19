# App settings

The gear button in the toolbar opens **Settings** for the open folder.
**Save settings** stores them in the folder's `i18n-harness.toml` (see
[The settings file](settings-file.md)); they apply to that folder only.

## Checks

When a translation counts as too long, for the language of the selected
file:

![Check settings](../images/settings-checks.png)

- **Warn when a translation is longer than**: a factor of the original's
  length (1.3 means 30% longer).
- **Skip originals shorter than**: short labels are not checked,
  8 characters by default.

A translation over the limit gets a warning, not a problem: it is still
saved. See [Length](checks.md#length).

## Model server

Which server the model list and translations come from:

![Model server settings](../images/settings-server.png)

- **Kind of server**: *Ollama*, or *OpenAI-compatible* for LM Studio,
  vLLM, the llama.cpp server or a cloud provider (see
  [Use an OpenAI-compatible server](../tutorials/openai-compatible.md)).
- **Address**: where the server listens, `http://localhost:11434` for
  Ollama; for an OpenAI-compatible server everything up to and including
  `/v1`.
- **Context size (tokens)**: how much text the model considers at once.
  Leave it empty to use the model's default.
- **Test connection** lists the models the server offers, or shows why
  it could not connect.

The model you pick in the toolbar is remembered for the folder too.

## Glossary

The glossary is not in Settings but has its own view: **Glossary** in
the toolbar. See [Keep terms unchanged with a glossary](../tutorials/glossary.md).
