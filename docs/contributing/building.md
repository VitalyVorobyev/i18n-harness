# Building and testing

## Prerequisites

- **Rust** 1.85 or newer (the workspace uses edition 2024), with
  `rustfmt` and `clippy`.
- **Bun** 1.3 for the desktop app's front end (`ui/`). Use Bun, not npm:
  the lockfile is `ui/bun.lock`.
- **Linux only**, for the desktop app: the WebKitGTK development
  packages, e.g. on Debian/Ubuntu
  `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`.
- Optionally **Ollama** with a model, to try translation for real.
- Optionally **mdBook** 0.4 to build the user guide.

## The Rust workspace

```sh
cargo build --workspace
cargo test  --workspace --all-targets
cargo fmt   --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo doc   --workspace --no-deps --document-private-items
```

The workspace includes the desktop shell (`ui/src-tauri`), whose build
script builds the front end with Bun when it is missing or out of date —
so `bun install` in `ui/` must have run once. Set
`I18N_HARNESS_SKIP_FRONTEND_BUILD=1` to skip that (Rust-only CI jobs).

Some backend features are not enabled by any workspace member by
default; CI checks them separately:

```sh
cargo clippy -p i18n-harness-backend --all-targets --all-features -- -D warnings
cargo test   -p i18n-harness-backend --all-features
```

Run the command-line tools from the checkout:

```sh
cargo run -p i18n-harness-cli -- --help
cargo run -p i18n-harness-lab -- --help
```

Tests never need a running model server; the model backends are tested
against a local fake server.

## The desktop app

```sh
cd ui
bun install
bun run dev:mock        # the UI in a browser against an in-memory backend
bun run tauri:dev       # the desktop app against the real backend
bun run typecheck
bun run lint            # Oxlint
bun run fmt:check       # oxfmt
bun run test:run        # Vitest
bun run test:e2e        # Playwright against the mock backend
bun run tauri:build     # installers for this platform
```

`bun run dev:mock` is the quickest way to work on the interface: no Rust
build, a sample folder (a Slovak Qt file pre-filled with German and a PO
file) and two fake models.

From the workspace root, two cargo aliases build the app with the
interface embedded (after `bun install` in `ui/` once):

```sh
cargo app                 # debug build: target/debug/i18n-harness-ui
cargo app --release       # optimised:   target/release/i18n-harness-ui
cargo app-run             # build and start (also with --release)
```

### A blank window

A build without the `custom-protocol` feature loads the interface from the
Vite development server and shows an empty window when none is running.
Every default build embeds it; if you build with `--no-default-features`,
add `--features custom-protocol` back (only `bun tauri:dev` leaves it out,
on purpose).

## The user guide

The user guide is the book in `book/`, built with
[mdBook](https://rust-lang.github.io/mdBook/):

```sh
mdbook build book       # writes book/build/
mdbook serve book       # live preview at http://localhost:3000
```

CI builds the book on every pull request; a chapter listed in
`book/src/SUMMARY.md` that does not exist fails the build. mdBook does
not check links inside pages, so follow the ones you add in
`mdbook serve`. The book is published to GitHub Pages from `main`.

### Screenshots

The screenshots in `book/src/images/` are taken from the mock backend
by a Playwright spec that only runs when asked to:

```sh
cd ui
BOOK_SCREENSHOTS=../book/src/images bunx playwright test e2e/book-screenshots.spec.ts
```

Re-run it after visible UI changes and commit the updated images.

## Continuous integration

`.github/workflows/ci.yml` runs, on every push to `main` and every pull
request: formatting, Clippy, tests and docs for the workspace; the
backend feature matrix; type checking, linting, formatting, building and
the dependency audit of the front end; Vitest; Playwright; and the book
build. `.github/workflows/book.yml` publishes the book.

The book is for people who use the app and the command-line tools. Keep
crate names, Rust types and build commands out of it; they belong in
these contributor pages.
