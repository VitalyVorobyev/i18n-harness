# Sample project

A minimal `i18n-harness` project for exercising the desktop app and the
CLI end-to-end. Three Qt `.ts` catalogs (`de_DE`, `es_ES`, `zh_Hans`),
one glossary, no external dependencies.

## Try it in the CLI

```sh
cargo run -p i18n-harness-cli -- open examples/sample-project
```

Prints the manifest summary: 3 catalogs, 3 locales, glossary loaded.

## Try it in the desktop app

```sh
cd ui && bun tauri:dev
```

In the app's home screen, click **Open project folder** and pick
`examples/sample-project`. You should see:

- Sidebar with three catalogs.
- Triptych editor when you click one.
- Glossary tab with the three terms.
- Topbar's **Close project** returns to the home screen.

Most units start untranslated so you can exercise the editor and (if you
have a local Ollama server with a Gemma model) the Translate button.

## Runtime state

`.i18n-harness/` is created next to the manifest the first time you open
the project. It holds metrics, corrections, and review status — all
gitignored.
