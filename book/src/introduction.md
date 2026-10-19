# Introduction

**i18n-harness** translates the text of software applications — menu
items, buttons, messages — with a language model that runs on your own
computer. You point it at the folder that holds your translation files,
pick a model, and it fills in what is missing. You then check the
suggestions, fix what needs fixing, and save.

![The i18n-harness workspace](images/workspace.png)

## What it does

- **Reads the translation files you already have.** Qt Linguist (`.ts`),
  gettext (`.po`), Lingui (`.po`, `.json`) and ICU message JSON (`.json`)
  files are found in the folder automatically, together with their
  languages. Every language in
  the Unicode CLDR database is supported.
- **Translates with a model of your choice.** The app lists the models
  installed on a local [Ollama](https://ollama.com) server, or on any
  server that speaks the OpenAI API (LM Studio, vLLM, llama.cpp server, a
  cloud provider). It ships no model and prefers none.
- **Checks every translation.** Lost placeholders such as `%1` or
  `{name}`, changed markup such as `<b>`, a term you asked to keep
  unchanged that was translated anyway, a missing plural form — such a
  translation is marked as a problem and is **never written to the
  file**.
- **Writes the files back exactly.** Only the translations change; every
  other byte of the file (comments, order, indentation, line endings)
  stays as it was.
- **Keeps track of your review.** Each string is *To do*, *Check* (a
  model suggestion or a draft awaiting your look) or *Done*.

## Local first

By default nothing leaves your computer: the model runs in Ollama on the
same machine, no account or API key is needed, and the app sends no
telemetry. A cloud model is an option you switch on per folder, not a
requirement. The API key for such a server is read from an environment
variable and never stored.

## What it does not do

- It does not extract strings from source code. Use your framework's
  tools (`lupdate`, `xgettext`, `formatjs extract`, …) to produce the
  translation files; i18n-harness translates them.
- It does not compile translations (`lrelease`, `msgfmt`); your build
  does that as before.
- It does not judge the quality of a translation. The checks catch
  *broken* translations, not *bad* ones. A person who knows the language
  should look at the suggestions under **Check** before they are
  delivered.
- It does not download models. You install them in Ollama (or your other
  server) yourself; see [Install Ollama and get a model](getting-started/models.md).

## How this guide is organised

- **[Quick start](quick-start.md)** gets you from download to translated
  strings in a few steps. Start here.
- **Getting started** covers installing the app and a model, and a first
  translation, in detail.
- **Tutorials** are step-by-step recipes for common situations, each
  with a small example you can reproduce.
- **Reference** describes the settings, the command-line tools, the file
  formats and the checks precisely.

Want to build i18n-harness yourself or contribute? See
[Contributing](https://github.com/VitalyVorobyev/i18n-harness/blob/main/CONTRIBUTING.md)
in the project's repository.
