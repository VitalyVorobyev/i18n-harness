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
├── vite.config.ts          Vite 8 + Tailwind plugin, dev port 1420
├── index.html              single mount point (#root)
├── src/
│   ├── main.tsx            React 19 createRoot
│   ├── App.tsx             top-level layout + Tauri command wiring
│   ├── styles/tailwind.css Tailwind v4 entry + @theme design tokens
│   ├── lib/                tauri.ts (typed invoke wrappers), types.ts,
│   │                       highlight.ts, cn.ts (class concatenator)
│   └── components/         TopBar, CatalogList, UnitEditor, Inspector,
│                           StateBadge, EmptyState
└── src-tauri/              The Rust side (workspace member: i18n-harness-ui)
    ├── Cargo.toml
    ├── tauri.conf.json
    ├── capabilities/
    └── src/                main.rs (entry), lib.rs (commands)
```

## What it does today (M3.0 – M3.5)

**Catalog view** — the default surface.

- Open a Qt Linguist `.ts` file via a native dialog (⌘O).
- List every unit on the left with state badges, search, filter
  chips, keyboard arrow navigation, and per-row dirty markers.
- Edit the target inline. Singular targets use one textarea; plural
  targets get one tab per CLDR form. Empty edits demote state back to
  `Untranslated`; any text promotes to `Proposed`.
- **Translate** the selected unit through the Ollama backend
  (Gemma 4 by default). The gate runs on the result and findings
  appear in the right-hand inspector, grouped by severity. A
  gate-clean translation auto-promotes to `Finished`.
- **Save** (⌘S) writes the catalog back via the byte-stable Qt
  adapter. **Discard** reverts every in-memory edit to disk.
- Locale badge in the top bar shows the language declared in the
  `.ts` root (`de_DE`, `es_ES`, `zh_Hans`).

**Glossary view** — switch via the tab strip in the top bar.

- Open a `glossary.toml` (or create one from scratch).
- Per-term editor: source string, do-not-translate flag, free-form
  notes, and one column per workspace locale.
- Per-locale overrides table: register (formal / informal / neutral)
  and variant tags.
- Validation is server-side via `Glossary::from_toml` — save refuses
  to write a malformed file; non-fatal warnings (unknown locale id,
  empty translations) surface inline.

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

Standard CI gates (the same ones GitHub Actions runs):

```sh
bun run typecheck            # tsc --noEmit
bun run lint                 # biome check  (lint + import order + format)
bun run lint:fix             # biome check --write   (apply safe fixes)
bun audit --prod             # production-dep vulnerability scan
bun run build                # tsc --noEmit && vite build
```

A change cannot land without all of these passing.

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

Tokens live in `src/styles/tailwind.css` under `@theme`. Tailwind v4
generates utility classes from those tokens; components compose the
utilities directly in JSX. No `*.module.css`, no raw hex literals.

- **Palette:** neutral slate, single muted indigo accent, explicit
  semantic colors per state and gate severity. Adding a new token is a
  new `--color-X` line in `@theme` and `bg-X` / `text-X` / `border-X`
  become available immediately.
- **Typography:** Inter (UI) + JetBrains Mono (source/target text and
  identifiers). Both shipped as local woff2 via `@fontsource-variable/*`
  — no Google Fonts fetch.
- **Density:** balanced-to-compact. `--text-base` is `13px`; `--spacing`
  stays at `4px` so `p-2` is 8px regardless of body font size.
- **Theme:** dark is canonical (matches the IDE-tool culture); light is
  the inversion, applied by setting `<html data-theme="light">`. The
  variant is attribute-driven, not OS-preference-driven, so the product
  decides the look.

## Invariants (carried over from the workspace)

- The model translates text; deterministic Rust does everything else.
  Tauri commands are thin wrappers — no business logic.
- Catalog formats are plugins behind `CatalogFormat`; the UI never
  parses `.ts` itself.
- No telemetry phoning home. Fonts are local; the dialog plugin only
  reads filesystem paths the user picked.
