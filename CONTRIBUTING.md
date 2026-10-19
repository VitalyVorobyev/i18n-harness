# Contributing to i18n-harness

This page is for people who build, change or extend i18n-harness. If you
want to *use* it, the [user guide](https://vitalyvorobyev.github.io/i18n-harness/)
is the place to start.

## Where things are documented

| Audience | Where |
|---|---|
| People using the app and the command-line tools | [`README.md`](README.md) and the user guide in [`book/`](book/), published at <https://vitalyvorobyev.github.io/i18n-harness/> |
| Contributors | this file and [`docs/contributing/`](docs/contributing/) |
| Maintainers | design notes and roadmaps in [`docs/internal/`](docs/internal/) |
| AI coding agents working on the repo | [`CLAUDE.md`](CLAUDE.md) and the agents and skills in [`.claude/`](.claude/) |

Keep the boundary: the README and the book speak to translators and
developers who *use* the tools. Crate names, Rust types, build commands
and roadmap labels belong in the contributor and maintainer pages.

## Build and test

You need Rust 1.85 or newer, [Bun](https://bun.sh) 1.3 for the desktop
app, and on Linux the WebKitGTK development packages. Details are in
[Building and testing](docs/contributing/building.md).

```sh
# once
cd ui && bun install && cd ..

# the Rust workspace (includes the desktop shell)
cargo build --workspace
cargo test  --workspace --all-targets
cargo fmt   --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# the desktop app
cargo app-run                 # build and start it (add --release)
cd ui && bun run dev:mock     # the interface in a browser, no Rust build

# the front end checks
cd ui && bun run typecheck && bun run lint && bun run fmt:check && bun run test:run

# the user guide
mdbook serve book             # live preview at http://localhost:3000
```

CI runs all of these on every pull request.

## The two invariants

1. **Languages, translation engines and catalog formats are data or
   plugins behind stable extension points.** The core names none of them.
   A new language is a data row, a new engine a trait implementation, a
   new format a serializer listed in one registry.
2. **The model translates text; deterministic Rust does everything
   else.** Parsing, validation and writing never go through the model. A
   weak model can produce worse *text*; it cannot corrupt *structure*.

And one contract that never goes red: for every format and every
fixture, reading a file and writing it back with no changes is
**byte-identical**. See [The round-trip contract](docs/contributing/round-trip.md).

## How it fits together

```text
catalog ─► format (read) ─► units (ICU form) ─► backend ─► gate ─► format (write) ─► catalog'
```

The catalog on disk is the source of truth. Formats know about files and
placeholders, backends know about text, and the gate knows about CLDR
plural rules, placeholders, markup and the glossary. The session crate
orchestrates them for both the CLI and the desktop app. See
[Architecture](docs/contributing/architecture.md) for the crate map.

```text
crates/        the Rust library crates and the harness / harness-lab binaries
ui/            the desktop app: React front end (src/) and Tauri shell (src-tauri/)
book/          the user guide (mdBook)
docs/          contributor pages, maintainer notes, benchmarks
examples/      the sample folder bundled with the app ("Try a sample")
fixtures/      round-trip and check fixtures
xtask/         maintainer tasks (regenerating the CLDR table)
```

## Extending

Three kinds of contribution land without touching the core. Each has a
guide in [Extending the harness](docs/contributing/extending.md) and a
Claude Code skill under [`.claude/skills/`](.claude/skills/):

- **A language setting** (`add-locale`): every CLDR language already
  works; a locale record tunes register, length and script, with a
  fixture for its plural forms.
- **A translation engine** (`add-backend`): a feature-gated
  `TranslationBackend` implementation with a smoke test.
- **A catalog format** (`add-format`): a `CatalogFormat` serializer with
  a placeholder converter, round-trip fixtures and property tests.

## Pull requests

- A user-visible change updates the matching user-guide page, and its
  screenshots, in the same pull request
  ([how to regenerate them](docs/contributing/building.md#screenshots)).
- Notable changes get a line in [`CHANGELOG.md`](CHANGELOG.md) under
  *Unreleased*, written for users.
- Design only from public specifications (Qt `.ts`, gettext PO, ICU
  MessageFormat, Unicode CLDR, the Ollama and OpenAI HTTP APIs).
- No `unsafe` outside FFI boundaries, no bundled credentials, no
  telemetry.

Releases are described in [Releasing](docs/contributing/releasing.md).
