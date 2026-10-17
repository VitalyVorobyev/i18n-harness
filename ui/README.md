# Desktop app

The translator-facing app: Tauri 2 + React 19 + Vite, built on the
`@vitavision/ui` and `@vitavision/workbench` component packages.

- `src/` — the frontend. `api/` is the only place that talks to the shell
  (`tauri.ts`) or stands in for it (`mock.ts`); `screens/` are the start
  screen and the workspace; `workspace/` holds the workspace's parts.
- `src-tauri/` — thin Tauri commands over `i18n-harness-session`. Catalog
  handling, status rules, merging model answers and the save guard all live
  in that crate, not here.

## Commands

```sh
bun install
bun run dev:mock        # the app in a browser against an in-memory backend
bun run tauri:dev       # the desktop app against the real backend
bun run typecheck       # TypeScript 7 (native compiler)
bun run lint            # Oxlint, type-aware
bun run fmt:check       # oxfmt
bun run test:run        # Vitest
bun run test:e2e        # Playwright against the mock backend
bun run tauri:build     # installers for this platform
```

From the workspace root, `cargo app` builds the app and `cargo app-run`
builds and starts it, debug by default, optimised with `--release`
(aliases in `.cargo/config.toml`). The shell's build script rebuilds `dist/`
when the frontend changed, so only `bun install` is needed once.
