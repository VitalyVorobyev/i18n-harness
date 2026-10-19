# Use an OpenAI-compatible server

Besides Ollama, the app works with any server that speaks the OpenAI
HTTP API: local servers such as **LM Studio**, the **llama.cpp server**
or **vLLM**, and cloud providers. The harness lists the server's models
from `/v1/models` and translates through `/v1/chat/completions`.

> **Privacy.** With a local server, nothing leaves your computer, as with
> Ollama. With a cloud provider, every string you translate (and your
> glossary terms that occur in it) is sent to that provider.

## 1. Start the server

| Server | How to start it | Address to enter |
|---|---|---|
| LM Studio | Load a model, then start the local server in the *Developer* view. | `http://localhost:1234/v1` |
| llama.cpp | `llama-server -m model.gguf --port 8080` | `http://localhost:8080/v1` |
| vLLM | `vllm serve <model>` | `http://localhost:8000/v1` |
| A cloud provider | — | the provider's API base URL, e.g. `https://api.openai.com/v1` |

The address is everything up to and including `/v1`.

## 2. Provide the API key (if the server needs one)

The key is read from the environment variable **`OPENAI_API_KEY`** and
sent as `Authorization: Bearer <key>`. It is **never written** to the
settings file or anywhere else. Local servers usually need no key.

The variable must be visible to the app:

- **macOS** — apps started from the Dock or Finder do not see variables
  from your shell profile. Set it for your login session, then start the
  app:

  ```sh
  launchctl setenv OPENAI_API_KEY 'sk-…'
  ```

- **Windows** — *Settings → System → About → Advanced system settings →
  Environment Variables*, add `OPENAI_API_KEY` under *User variables*,
  then start the app.
- **Linux** — `export OPENAI_API_KEY='sk-…'` in the shell from which you
  start the app (or in your session's environment).

## 3. Point the app at the server

1. Open your folder and click the **gear** button (**Settings**).
2. Choose the **Model server** tab.
3. Set **Kind of server** to **OpenAI-compatible**. The address changes
   to `http://localhost:1234/v1`; change it to your server's address.
4. Click **Test connection**. It should say *Connected: N models
   available*. If not, the message says why (wrong address, server not
   running, key missing or rejected).
5. Click **Save settings**, then pick a model in the toolbar.

The choice is saved in the folder's `i18n-harness.toml`:

```toml
[backend.default]
kind = "openai-compatible"
host = "http://localhost:1234/v1"
model = "your-model-id"
```

## Differences from Ollama

- An OpenAI-compatible server does not say how a model wants to be
  prompted, so every model is sent written instructions plus the text,
  the way chat models expect. Dedicated
  translation models that need their own completion format work best
  through Ollama.
- The server's list is shown as is; it may include embedding models,
  which cannot translate. Pick a text model.
- **Context size** in Settings applies to Ollama only; configure the
  context on the server instead.
- The start screen always checks the local Ollama server; its *Ollama is
  not running* note does not concern your OpenAI-compatible server.

## From the command line

```sh
OPENAI_API_KEY='sk-…' harness translate-folder app/i18n \
  --server openai-compatible --host http://localhost:1234/v1 \
  --model your-model-id --save
```
