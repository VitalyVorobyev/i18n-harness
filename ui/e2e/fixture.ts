// Playwright test fixture that wires up the Tauri IPC mock before each test.
//
// Usage: import { test, expect } from "./fixture" in spec files instead of
// importing from "@playwright/test" directly.

import { test as base, type Page } from "@playwright/test";
import fixtureData from "./fixtures/sample-project.json" with { type: "json" };

export { expect } from "@playwright/test";

// ── Shared helpers ────────────────────────────────────────────────────────────

/**
 * Opens the sample project through the mock IPC.
 * Returns after the Overview tab is visible.
 */
export async function openSampleProject(page: Page): Promise<void> {
  // Tell the dialog mock which path to return when the user clicks Open.
  await page.evaluate((root: string) => {
    (window as unknown as Record<string, unknown>).__mockPickResult = root;
  }, fixtureData.summary.root);

  // Click the Open button on the home screen.
  await page.getByRole("button", { name: /open/i }).first().click();

  // Wait for the workspace to appear (Overview tab becomes visible).
  await page.waitForSelector('[role="tab"][aria-selected="true"]', {
    timeout: 5000,
  });
}

// ── Extended test fixture ─────────────────────────────────────────────────────

interface Fixtures {
  /** Sample project fixture data (convenience reference). */
  sampleProject: typeof fixtureData;
}

export const test = base.extend<Fixtures>({
  // biome-ignore lint/correctness/noEmptyPattern: Playwright requires this pattern
  sampleProject: async ({}, use) => {
    await use(fixtureData);
  },

  page: async ({ page }, use) => {
    // Inject the IPC mock before any page script runs.
    await page.addInitScript((data) => {
      // Install __TAURI_INTERNALS__ so invoke() routes through our mock.
      type MockData = typeof fixtureData;
      (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
        transformCallback: (cb: (v: unknown) => void, once?: boolean) => {
          const id = Math.random();
          (window as unknown as Record<string, unknown>)[`_tauri_cb_${id}`] =
            once
              ? (v: unknown) => {
                  cb(v);
                }
              : cb;
          return id;
        },
        invoke: async (
          cmd: string,
          args?: Record<string, unknown>,
        ): Promise<unknown> => {
          return (
            window as unknown as {
              __tauriMockInvoke: (
                cmd: string,
                args?: Record<string, unknown>,
              ) => Promise<unknown>;
            }
          ).__tauriMockInvoke(cmd, args);
        },
        metadata: {
          currentWebview: { label: "main" },
          currentWindow: { label: "main" },
        },
        unregisterCallback: (_id: number) => undefined,
        plugins: {},
      };

      // State storage mirroring tauri-mock.ts logic but inline for the browser.
      // Deep-copy the fixture so mutations are isolated per test.
      const fixture = JSON.parse(JSON.stringify(data)) as MockData;
      const catalogs: Record<
        string,
        (typeof fixtureData.catalogs)[keyof typeof fixtureData.catalogs]
      > = JSON.parse(JSON.stringify(fixture.catalogs));
      let openProject: typeof fixtureData.summary | null = null;
      const dirtyCatalogs = new Set<string>();
      let glossaryPayload = JSON.parse(
        JSON.stringify(fixture.glossary.payload),
      );

      const HARD_FLAGS = new Set([
        "placeholder-mismatch",
        "plural-arity-mismatch",
        "icu-parse-error",
        "empty-target-when-finished",
        "backend-malformed-response",
      ]);

      function findUnit(catalogPath: string, unitId: string) {
        const cat = catalogs[catalogPath];
        if (!cat) throw new Error(`Catalog not found: ${catalogPath}`);
        const unit = cat.units.find((u) => u.id === unitId);
        if (!unit) throw new Error(`Unit not found: ${unitId}`);
        return { cat, unit };
      }

      const COMMANDS: Record<
        string,
        (args: Record<string, unknown>) => unknown
      > = {
        app_version: () => "0.0.0-test",
        current_project_summary: () => openProject,
        open_project: (a) => {
          const root = a.root as string;
          if (
            root !== fixture.summary.root &&
            !root.endsWith("sample-project")
          ) {
            throw new Error(`Unknown project root: ${root}`);
          }
          openProject = { ...fixture.summary };
          return { summary: openProject, warnings: [] };
        },
        close_project: () => {
          openProject = null;
          Object.assign(catalogs, JSON.parse(JSON.stringify(fixture.catalogs)));
          dirtyCatalogs.clear();
        },
        open_catalog_in_project: (a) => {
          const cat = catalogs[a.catalogPath as string];
          if (!cat)
            throw new Error(`Catalog not found: ${a.catalogPath as string}`);
          return JSON.parse(JSON.stringify(cat));
        },
        update_unit_target_in_project: (a) => {
          const { unit } = findUnit(
            a.catalogPath as string,
            a.unitId as string,
          );
          const edit = a.edit as {
            kind: "singular" | "plural";
            text?: string | null;
            form_index?: number;
          };
          if (edit.kind === "singular") {
            unit.target = { kind: "singular", text: edit.text ?? null };
          } else {
            if (unit.target.kind !== "plural") {
              unit.target = { kind: "plural", forms: [] };
            }
            const forms: (string | null)[] = [
              ...(unit.target as { kind: "plural"; forms: (string | null)[] })
                .forms,
            ];
            forms[edit.form_index ?? 0] = edit.text ?? null;
            (unit as { target: unknown }).target = { kind: "plural", forms };
          }
          const hasText =
            edit.kind === "singular"
              ? edit.text != null && (edit.text as string).length > 0
              : edit.text != null;
          if (hasText && unit.state !== "proposed") unit.state = "proposed";
          dirtyCatalogs.add(a.catalogPath as string);
          return JSON.parse(JSON.stringify(unit));
        },
        accept_unit_in_project: (a) => {
          const { unit } = findUnit(
            a.catalogPath as string,
            a.unitId as string,
          );
          const hasHard = (unit.flags as string[]).some((f) =>
            HARD_FLAGS.has(f),
          );
          if (hasHard) throw new Error("Cannot accept: hard flag present");
          unit.state = "finished";
          unit.flags = [];
          unit.review_status = "reviewed";
          dirtyCatalogs.add(a.catalogPath as string);
          return JSON.parse(JSON.stringify(unit));
        },
        translate_unit_in_project: async (a) => {
          // Simulate model latency so the spinner is visible during tests.
          await new Promise<void>((r) => setTimeout(r, 400));
          const { unit } = findUnit(
            a.catalogPath as string,
            a.unitId as string,
          );
          if (unit.target.kind === "singular") {
            unit.target = { kind: "singular", text: `[MT] ${unit.source}` };
          }
          unit.state = "proposed";
          dirtyCatalogs.add(a.catalogPath as string);
          return {
            unit: JSON.parse(JSON.stringify(unit)),
            report: { unit_id: unit.id, findings: [], flags: [] },
          };
        },
        save_catalog_in_project: (a) => {
          const cat = catalogs[a.catalogPath as string];
          if (!cat)
            throw new Error(`Catalog not found: ${a.catalogPath as string}`);
          dirtyCatalogs.delete(a.catalogPath as string);
          return { path: a.catalogPath, unit_count: cat.units.length };
        },
        save_all_dirty: () => {
          const saved: { path: string; unit_count: number }[] = [];
          for (const p of dirtyCatalogs) {
            const c = catalogs[p];
            if (c) saved.push({ path: p, unit_count: c.units.length });
          }
          dirtyCatalogs.clear();
          return { saved };
        },
        discard_changes_in_project: (a) => {
          const orig =
            fixture.catalogs[a.catalogPath as keyof typeof fixture.catalogs];
          if (!orig)
            throw new Error(`Catalog not found: ${a.catalogPath as string}`);
          const fresh = JSON.parse(JSON.stringify(orig));
          catalogs[a.catalogPath as string] = fresh;
          dirtyCatalogs.delete(a.catalogPath as string);
          return JSON.parse(JSON.stringify(fresh));
        },
        is_catalog_dirty: (a) => dirtyCatalogs.has(a.catalogPath as string),
        list_open_catalogs: () => Object.keys(catalogs),
        scan_project_review_state: () => {
          const items: unknown[] = [];
          const byCatalog: Record<string, number> = {};
          for (const [path, cat] of Object.entries(catalogs)) {
            const ref = openProject?.catalogs.find(
              (c) => c.absolute_path === path,
            );
            let count = 0;
            for (const unit of cat.units) {
              const needs =
                unit.review_status === "needs-review" ||
                (unit.flags as string[]).length > 0;
              if (!needs) continue;
              count++;
              items.push({
                catalog_path: path,
                catalog_manifest_path: ref?.manifest_path ?? path,
                locale: ref?.locale ?? "",
                unit_id: unit.id,
                source_preview: (unit.source as string).slice(0, 120),
                target_preview: "",
                flags: unit.flags,
                review_status: unit.review_status ?? null,
                state: unit.state,
              });
            }
            if (count > 0) byCatalog[path] = count;
          }
          return {
            total_count: items.length,
            by_catalog: byCatalog,
            items,
          };
        },
        load_glossary: (a) => ({
          path: a.path,
          payload: JSON.parse(JSON.stringify(glossaryPayload)),
          warnings: [],
        }),
        save_glossary: (a) => {
          glossaryPayload = JSON.parse(JSON.stringify(a.payload));
          return { path: a.path, warnings: [] };
        },
        list_locales: () => [
          {
            id: "de_DE",
            register: "neutral",
            script: "Latn",
            plural_arity: 2,
          },
          {
            id: "es_ES",
            register: "neutral",
            script: "Latn",
            plural_arity: 2,
          },
          {
            id: "zh_Hans",
            register: "neutral",
            script: "Hans",
            plural_arity: 1,
          },
        ],
        translate_batch_in_project: () => ({
          job_id: "mock-batch-job",
          total: 0,
        }),
        cancel_translation: () => true,
        // Stubs
        list_catalogs: () => [],
        save_manifest: () => undefined,
        discover_project: () => ({
          root: "",
          name: "",
          locales: {},
          catalogs: [],
          glossary: null,
          backend: null,
        }),
        create_project: () => ({ summary: openProject, warnings: [] }),
        add_catalog_to_project: () => ({ summary: openProject, warnings: [] }),
        remove_catalog_from_project: () => ({
          summary: openProject,
          warnings: [],
        }),
        update_locale_in_project: () => ({
          summary: openProject,
          warnings: [],
        }),
        remove_locale_from_project: () => ({
          summary: openProject,
          warnings: [],
        }),
        set_backend_in_project: () => ({ summary: openProject, warnings: [] }),
        set_glossary_in_project: () => ({
          summary: openProject,
          warnings: [],
        }),
        set_prompts_in_project: () => ({
          summary: openProject,
          warnings: [],
        }),
        list_corrections_in_project: () => [],
        promote_correction_to_curated: () => undefined,
        un_curate_correction: () => true,
        list_curated_in_project: () => [],
        run_evaluation_in_project: () => ({
          job_id: "mock-eval-job",
          total: 0,
        }),
        list_evaluation_runs_in_project: () => [],
        export_tuning_bundle_in_project: () => ({
          path: "/mock/tuning",
          examples_count: 0,
          locales: [],
          has_score: false,
          prompt_template_version: "mock-v1",
        }),
        list_tuning_bundles_in_project: () => [],
        load_metrics: () => ({
          path: "",
          events: [],
          error_count: 0,
          line_count: 0,
        }),
        // Tauri plugin stubs — called by webview/window/event APIs internally.
        "plugin:event|listen": () => Math.floor(Math.random() * 100000),
        "plugin:event|unlisten": () => undefined,
        "plugin:event|emit": () => undefined,
        "plugin:event|emit_to": () => undefined,
        "plugin:drag-drop|start": () => undefined,
        "plugin:webview|create": () => undefined,
        "plugin:webview|get_all_webviews": () => [],
        "plugin:window|get_all_windows": () => [],
        "plugin:dialog|open": () =>
          (window as unknown as Record<string, unknown>).__mockPickResult ??
          null,
        "plugin:dialog|save": () => "/mock/save/location.toml",
      };

      (
        window as unknown as {
          __tauriMockInvoke: (
            cmd: string,
            args?: Record<string, unknown>,
          ) => Promise<unknown>;
        }
      ).__tauriMockInvoke = async (
        cmd: string,
        args?: Record<string, unknown>,
      ) => {
        const handler = COMMANDS[cmd];
        if (!handler) throw new Error(`Mock: unknown command "${cmd}"`);
        return handler(args ?? {});
      };

      // Mock the dialog plugin (used by pickProjectFolder etc.)
      (window as unknown as Record<string, unknown>).__TAURI_PLUGIN_DIALOG__ = {
        open: async () =>
          (window as unknown as Record<string, unknown>).__mockPickResult ??
          null,
        save: async () => "/mock/save/location.toml",
      };
    }, fixtureData);

    await use(page);
  },
});
