# `ui/` — Tauri desktop shell (M3)

A thin Tauri 2 + Vite + React + TypeScript shell over the workspace's
pure-Rust library API. The translation, gate, and adapter logic live in
the Rust crates and remain testable as a headless library; this crate
just wraps them in a desktop window.

## Layout

```
ui/
├── package.json            Bun-managed JS package
├── tsconfig.json           strict TS, ES2022, react-jsx
├── vite.config.ts          Vite 6, dev port 1420
├── index.html              single mount point (#root)
├── src/
│   ├── main.tsx            React 18 createRoot
│   ├── App.tsx             top-level layout + Tauri command wiring
│   ├── styles/             tokens.css, reset.css, global.css
│   ├── lib/                tauri.ts (typed invoke wrappers), types.ts, highlight.ts
│   └── components/         TopBar, CatalogList, UnitEditor, Inspector,
│                           StateBadge, EmptyState
└── src-tauri/              The Rust side (workspace member: i18n-harness-ui)
    ├── Cargo.toml
    ├── tauri.conf.json
    ├── capabilities/
    └── src/                main.rs (entry), lib.rs (commands)
```

## What it does today (M3.0–M3.2)

- Open a Qt Linguist `.ts` file via a native dialog.
- List every unit on the left with state badges, search, and
  filter chips (All / Untranslated / Proposed / Finished).
- Show the selected unit's source and target side-by-side with
  monospace text, placeholder highlighting (`{count}`, `%1`, `&File`),
  and per-form tabs for plural units.
- An inspector pane showing the unit's state, plural arity,
  placeholder count, and provenance.

Read-only. Editing, save, and translation come in follow-up phases
(M3.3+).

## Development

```sh
# from the repo root
cd ui
bun install
bun tauri:dev          # opens the Tauri window with Vite HMR

# JS-only development (no Tauri window, browser only — most IPC commands
# will fail; useful for layout work)
bun dev
```

Type-check + production-build the web assets:

```sh
bun run typecheck
bun run build
```

Cargo-check just the Tauri shell:

```sh
cargo check -p i18n-harness-ui
```

## Adding a Tauri command

1. Write the Rust handler in `src-tauri/src/lib.rs` with
   `#[tauri::command]`.
2. Register it in `tauri::generate_handler![…]`.
3. Add a typed wrapper in `src/lib/tauri.ts` so component code
   stays out of the `invoke()` stringly-typed call.
4. If the command needs new permissions, list them in
   `src-tauri/capabilities/default.json`.

## Design system

- **Palette:** neutral slate, single muted indigo accent, explicit
  semantic colors per state and gate severity. All defined in
  `src/styles/tokens.css`. Components reference CSS variables, never
  raw hex.
- **Typography:** Inter (UI) + JetBrains Mono (source/target text and
  identifiers). Both shipped as local woff2 via `@fontsource-variable/*`
  — no Google Fonts fetch.
- **Density:** balanced-to-compact. `--text-base` is `13px` because
  translators scan hundreds of strings per session.
- **Theme:** dark is canonical (matches the IDE-tool culture); light is
  the inversion, switchable via `<html data-theme="light">`.

## Invariants (carried over from the workspace)

- The model translates text; deterministic Rust does everything else.
  Tauri commands are thin wrappers — no business logic.
- Catalog formats are plugins behind `CatalogFormat`; the UI never
  parses `.ts` itself.
- No telemetry phoning home. Fonts are local; the dialog plugin only
  reads filesystem paths the user picked.
