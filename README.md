# i18n-harness

[![Release](https://img.shields.io/github/v/release/VitalyVorobyev/i18n-harness)](https://github.com/VitalyVorobyev/i18n-harness/releases/latest)
[![User guide](https://img.shields.io/badge/docs-user_guide-blue)](https://vitalyvorobyev.github.io/i18n-harness/)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](LICENSE)

**Translate your application's text with a language model that runs on
your own computer.** Open the folder that holds your translation files,
pick a model, and i18n-harness fills in what is missing. You review the
suggestions, fix what needs fixing, and save. Every other byte of your
files stays exactly as it was.

![The i18n-harness workspace](book/src/images/workspace.png)

## Quick start

1. **Install the app.** Download the installer for your system from the
   [latest release](https://github.com/VitalyVorobyev/i18n-harness/releases/latest)
   (macOS `.dmg`, Windows `-setup.exe`, Linux `.deb` or `.AppImage`).
   The builds are not signed yet; [Install the app](https://vitalyvorobyev.github.io/i18n-harness/getting-started/install.html)
   shows how to open it the first time.
2. **Get a model.** Install [Ollama](https://ollama.com/download), then
   download a model in a terminal, for example:

   ```sh
   ollama pull gemma3:4b
   ```

   Any model that writes text works; see [Install Ollama and get a model](https://vitalyvorobyev.github.io/i18n-harness/getting-started/models.html)
   for how to choose.
3. **Translate.** Start the app and click **Try a sample** (or
   **Open folder…** for your own files). Choose the model in the toolbar,
   click **Translate**, check the suggestions under **Check**, and
   **Save**.

The [Quick start](https://vitalyvorobyev.github.io/i18n-harness/quick-start.html)
page of the user guide walks through the same steps with screenshots.

## What it does

- **Reads the files you already have:** Qt Linguist (`.ts`), gettext
  (`.po`), Lingui (`.po` and `.json`) and ICU message JSON (react-intl,
  i18next). Files and their languages are found automatically, and every
  language in the Unicode CLDR database is supported.
- **Translates with the model you choose:** any model on a local Ollama
  server, or on a server that speaks the OpenAI API (LM Studio, vLLM,
  llama.cpp, a cloud provider). No model is bundled or preferred.
- **Never writes a broken translation.** A lost placeholder (`%1`,
  `{name}`), changed markup, a missing plural form or a glossary term
  translated against your wishes is marked as a problem and held back
  from the file.
- **Keeps track of your review:** each string is *To do*, *Check* or
  *Done*, and a glossary keeps terms consistent across files.
- **Stays on your computer.** By default nothing leaves your machine: no
  account, no API key, no telemetry. A cloud model is an option you turn
  on per folder.

For unattended runs and automation there are command-line tools
(`harness`, `harness-lab`) in the same release, and a way to hand a batch
of strings to an AI coding agent such as Claude Code. See the
[user guide](https://vitalyvorobyev.github.io/i18n-harness/).

## What it does not do

It does not extract strings from source code (use `lupdate`, `xgettext`,
`lingui extract`, `formatjs extract`, …) or compile translations
(`lrelease`, `msgfmt`). It translates the files those tools produce. It
catches *broken* translations, not *bad* ones: a person who knows the
language should check the suggestions before they ship.

## Documentation

- **[User guide](https://vitalyvorobyev.github.io/i18n-harness/)**:
  installing, getting a model, tutorials for each file format, and the
  reference for settings, checks and the command-line tools.
- **[Changelog](CHANGELOG.md)**: what changed in each release.
- **[Contributing](CONTRIBUTING.md)**: building from source, the
  architecture and how to extend it.

## License

Apache-2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
