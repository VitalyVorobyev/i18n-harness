// Tauri IPC mock for browser-only tests (Playwright + Vitest).
//
// Activated when VITE_USE_TAURI_MOCK === "true".  The real `invoke` from
// @tauri-apps/api/core is replaced with a dispatch table that mirrors every
// command in tauri.ts, maintaining in-memory state so lifecycle transitions
// (untranslated → proposed → finished) work correctly across calls.
//
// Import this file only through the test setup or the conditional in tauri.ts.

import type {
  CatalogResponse,
  GlossaryLoadResponse,
  GlossaryPayload,
  GlossarySaveResponse,
  ProjectOpenResponse,
  ProjectSummary,
  ReviewQueueResponse,
  SaveAllDirtyResponse,
  SaveSummary,
  TargetEdit,
  TranslateBatchStarted,
  TranslateResult,
  Unit,
  UnitId,
} from "./types";

// ── Fixture data loaded at module init ───────────────────────────────────────

// The fixture is bundled via import() so Vite can process it in tests.
// The actual import is done lazily in initMock() below.

interface FixtureData {
  summary: ProjectSummary;
  catalogs: Record<string, CatalogResponse>;
  glossary: {
    path: string;
    payload: GlossaryPayload;
    warnings: string[];
  };
}

// ── In-memory mock state ──────────────────────────────────────────────────────

interface MockState {
  fixture: FixtureData | null;
  // Mutable catalog state — deep copies from fixture so mutations don't corrupt
  // the original.
  openProject: ProjectSummary | null;
  catalogs: Map<string, CatalogResponse>;
  // Dirty tracking (catalog path → dirty)
  dirtyCatalogs: Set<string>;
  glossaryPath: string | null;
  glossaryPayload: GlossaryPayload | null;
  // Batch event emitters keyed by job_id
  batchListeners: Map<
    string,
    {
      onProgress: ((payload: unknown) => void)[];
      onTerminal: ((payload: unknown) => void)[];
    }
  >;
}

const state: MockState = {
  fixture: null,
  openProject: null,
  catalogs: new Map(),
  dirtyCatalogs: new Set(),
  glossaryPath: null,
  glossaryPayload: null,
  batchListeners: new Map(),
};

export async function initMock(fixture: FixtureData): Promise<void> {
  state.fixture = fixture;
  // Deep copy catalogs so mutations are isolated per test session.
  state.catalogs = new Map(
    Object.entries(fixture.catalogs).map(([path, cat]) => [
      path,
      JSON.parse(JSON.stringify(cat)) as CatalogResponse,
    ]),
  );
  state.dirtyCatalogs = new Set();
  state.openProject = null;
  state.glossaryPath = fixture.glossary.path;
  state.glossaryPayload = JSON.parse(
    JSON.stringify(fixture.glossary.payload),
  ) as GlossaryPayload;
}

// Reset state between tests (call in beforeEach).
export function resetMock(): void {
  if (!state.fixture) return;
  state.catalogs = new Map(
    Object.entries(state.fixture.catalogs).map(([path, cat]) => [
      path,
      JSON.parse(JSON.stringify(cat)) as CatalogResponse,
    ]),
  );
  state.dirtyCatalogs = new Set();
  state.openProject = null;
  state.glossaryPayload = JSON.parse(
    JSON.stringify(state.fixture.glossary.payload),
  ) as GlossaryPayload;
  state.batchListeners = new Map();
}

// ── Command implementations ───────────────────────────────────────────────────

function cmdAppVersion(): string {
  return "0.0.0-test";
}

function cmdCurrentProjectSummary(): ProjectSummary | null {
  return state.openProject;
}

function cmdOpenProject(args: { root: string }): ProjectOpenResponse {
  const fixture = state.fixture;
  if (!fixture) throw new Error("mock not initialised");
  // Accept the sample project path or the fixture root.
  if (
    args.root !== fixture.summary.root &&
    !args.root.endsWith("sample-project")
  ) {
    throw new Error(`Unknown project root: ${args.root}`);
  }
  state.openProject = { ...fixture.summary };
  return { summary: state.openProject, warnings: [] };
}

function cmdCloseProject(): void {
  state.openProject = null;
  state.catalogs = new Map(
    Object.entries(state.fixture?.catalogs ?? {}).map(([path, cat]) => [
      path,
      JSON.parse(JSON.stringify(cat)) as CatalogResponse,
    ]),
  );
  state.dirtyCatalogs = new Set();
}

function cmdOpenCatalogInProject(args: {
  catalogPath: string;
}): CatalogResponse {
  const catalog = state.catalogs.get(args.catalogPath);
  if (!catalog)
    throw new Error(`Catalog not found in mock: ${args.catalogPath}`);
  return JSON.parse(JSON.stringify(catalog)) as CatalogResponse;
}

function cmdUpdateUnitTargetInProject(args: {
  catalogPath: string;
  unitId: UnitId;
  edit: TargetEdit;
}): Unit {
  const catalog = state.catalogs.get(args.catalogPath);
  if (!catalog) throw new Error(`Catalog not found: ${args.catalogPath}`);
  const unit = catalog.units.find((u) => u.id === args.unitId);
  if (!unit) throw new Error(`Unit not found: ${args.unitId}`);

  // Apply the edit to the target.
  if (args.edit.kind === "singular") {
    unit.target = { kind: "singular", text: args.edit.text };
  } else {
    if (unit.target.kind !== "plural") {
      unit.target = { kind: "plural", forms: [] };
    }
    const forms = [...unit.target.forms];
    forms[args.edit.form_index] = args.edit.text;
    unit.target = { kind: "plural", forms };
  }

  // Transition state: untranslated/finished → proposed when a non-null edit lands.
  const hasText =
    args.edit.kind === "singular"
      ? args.edit.text != null && args.edit.text.length > 0
      : args.edit.text != null;
  if (hasText && unit.state !== "proposed") {
    unit.state = "proposed";
  }

  state.dirtyCatalogs.add(args.catalogPath);
  return JSON.parse(JSON.stringify(unit)) as Unit;
}

function cmdAcceptUnitInProject(args: {
  catalogPath: string;
  unitId: UnitId;
}): Unit {
  const catalog = state.catalogs.get(args.catalogPath);
  if (!catalog) throw new Error(`Catalog not found: ${args.catalogPath}`);
  const unit = catalog.units.find((u) => u.id === args.unitId);
  if (!unit) throw new Error(`Unit not found: ${args.unitId}`);

  // Guard: hard flags block accept.
  const HARD_FLAGS = new Set([
    "placeholder-mismatch",
    "plural-arity-mismatch",
    "icu-parse-error",
    "empty-target-when-finished",
    "backend-malformed-response",
  ]);
  const hasHard = unit.flags.some((f) => HARD_FLAGS.has(f));
  if (hasHard) throw new Error("Cannot accept: hard flag present");

  unit.state = "finished";
  unit.flags = [];
  unit.review_status = "reviewed";
  state.dirtyCatalogs.add(args.catalogPath);
  return JSON.parse(JSON.stringify(unit)) as Unit;
}

function cmdTranslateUnitInProject(args: {
  catalogPath: string;
  unitId: UnitId;
}): TranslateResult {
  const catalog = state.catalogs.get(args.catalogPath);
  if (!catalog) throw new Error(`Catalog not found: ${args.catalogPath}`);
  const unit = catalog.units.find((u) => u.id === args.unitId);
  if (!unit) throw new Error(`Unit not found: ${args.unitId}`);

  // Fake translation: prepend "[MT] " to source.
  if (unit.target.kind === "singular") {
    unit.target = { kind: "singular", text: `[MT] ${unit.source}` };
  } else {
    unit.target = {
      kind: "plural",
      forms: unit.target.forms.map(() => `[MT] ${unit.source}`),
    };
  }
  unit.state = "proposed";
  state.dirtyCatalogs.add(args.catalogPath);
  const snapshot = JSON.parse(JSON.stringify(unit)) as Unit;
  return {
    unit: snapshot,
    report: { unit_id: unit.id, findings: [], flags: [] },
  };
}

function cmdSaveCatalogInProject(args: { catalogPath: string }): SaveSummary {
  const catalog = state.catalogs.get(args.catalogPath);
  if (!catalog) throw new Error(`Catalog not found: ${args.catalogPath}`);
  state.dirtyCatalogs.delete(args.catalogPath);
  return { path: args.catalogPath, unit_count: catalog.units.length };
}

function cmdSaveAllDirty(): SaveAllDirtyResponse {
  const saved: SaveSummary[] = [];
  for (const path of state.dirtyCatalogs) {
    const catalog = state.catalogs.get(path);
    if (catalog) {
      saved.push({ path, unit_count: catalog.units.length });
    }
  }
  state.dirtyCatalogs.clear();
  return { saved };
}

function cmdDiscardChangesInProject(args: {
  catalogPath: string;
}): CatalogResponse {
  const fixture = state.fixture;
  if (!fixture) throw new Error("mock not initialised");
  const original = fixture.catalogs[args.catalogPath];
  if (!original) throw new Error(`Catalog not found: ${args.catalogPath}`);
  const fresh = JSON.parse(JSON.stringify(original)) as CatalogResponse;
  state.catalogs.set(args.catalogPath, fresh);
  state.dirtyCatalogs.delete(args.catalogPath);
  return JSON.parse(JSON.stringify(fresh)) as CatalogResponse;
}

function cmdIsCatalogDirty(args: { catalogPath: string }): boolean {
  return state.dirtyCatalogs.has(args.catalogPath);
}

function cmdListOpenCatalogs(): string[] {
  return [...state.catalogs.keys()];
}

function cmdTranslateBatchInProject(args: {
  catalogPath: string;
  scope: string;
}): TranslateBatchStarted {
  const catalog = state.catalogs.get(args.catalogPath);
  if (!catalog) throw new Error(`Catalog not found: ${args.catalogPath}`);

  const inScope = catalog.units.filter((u) =>
    args.scope === "untranslated"
      ? u.state === "untranslated"
      : u.state === "untranslated" || u.state === "proposed",
  );

  const jobId = `mock-job-${Date.now()}`;

  // Emit progress events asynchronously (one per unit, 10ms apart).
  setTimeout(() => {
    for (let i = 0; i < inScope.length; i++) {
      const unit = inScope[i];
      if (!unit) continue;
      setTimeout(
        () => {
          // Mutate the unit.
          if (unit.target.kind === "singular") {
            unit.target = { kind: "singular", text: `[MT] ${unit.source}` };
          }
          unit.state = "proposed";
          state.dirtyCatalogs.add(args.catalogPath);

          // Emit progress event via window custom event (Playwright-compatible).
          const progressEvent = new CustomEvent(`batch-progress-${jobId}`, {
            detail: {
              completed: i + 1,
              total: inScope.length,
              unit: JSON.parse(JSON.stringify(unit)),
              flagged: false,
            },
          });
          window.dispatchEvent(progressEvent);

          if (i === inScope.length - 1) {
            const terminalEvent = new CustomEvent(`batch-completed-${jobId}`, {
              detail: {
                completed: inScope.length,
                total: inScope.length,
                cancelled: false,
                failed_reason: null,
              },
            });
            window.dispatchEvent(terminalEvent);
          }
        },
        10 * (i + 1),
      );
    }

    if (inScope.length === 0) {
      const terminalEvent = new CustomEvent(`batch-completed-${jobId}`, {
        detail: {
          completed: 0,
          total: 0,
          cancelled: false,
          failed_reason: null,
        },
      });
      window.dispatchEvent(terminalEvent);
    }
  }, 0);

  return { job_id: jobId, total: inScope.length };
}

function cmdCancelTranslation(_args: { jobId: string }): boolean {
  return true;
}

function cmdScanProjectReviewState(): ReviewQueueResponse {
  const items: ReviewQueueResponse["items"] = [];
  const byCatalog: Record<string, number> = {};

  for (const [catalogPath, catalog] of state.catalogs) {
    const project = state.openProject;
    const catRef = project?.catalogs.find(
      (c) => c.absolute_path === catalogPath,
    );
    let count = 0;
    for (const unit of catalog.units) {
      const needsReview =
        unit.review_status === "needs-review" || unit.flags.length > 0;
      if (!needsReview) continue;
      count++;
      items.push({
        catalog_path: catalogPath,
        catalog_manifest_path: catRef?.manifest_path ?? catalogPath,
        locale: catRef?.locale ?? "",
        unit_id: unit.id,
        source_preview: unit.source.slice(0, 120),
        target_preview:
          unit.target.kind === "singular"
            ? (unit.target.text?.slice(0, 120) ?? "")
            : (unit.target.forms[0]?.slice(0, 120) ?? ""),
        flags: unit.flags,
        review_status: unit.review_status ?? null,
        state: unit.state,
      });
    }
    if (count > 0) byCatalog[catalogPath] = count;
  }

  return {
    total_count: items.length,
    by_catalog: byCatalog,
    items,
  };
}

function cmdLoadGlossary(args: { path: string }): GlossaryLoadResponse {
  if (!state.glossaryPayload) throw new Error("No glossary in mock fixture");
  return {
    path: args.path,
    payload: JSON.parse(
      JSON.stringify(state.glossaryPayload),
    ) as GlossaryPayload,
    warnings: [],
  };
}

function cmdSaveGlossary(args: {
  path: string;
  payload: GlossaryPayload;
}): GlossarySaveResponse {
  state.glossaryPayload = JSON.parse(
    JSON.stringify(args.payload),
  ) as GlossaryPayload;
  state.glossaryPath = args.path;
  return { path: args.path, warnings: [] };
}

function cmdListLocales() {
  return [
    { id: "de_DE", register: "neutral", script: "Latn", plural_arity: 2 },
    { id: "es_ES", register: "neutral", script: "Latn", plural_arity: 2 },
    { id: "zh_Hans", register: "neutral", script: "Hans", plural_arity: 1 },
  ];
}

// ── Dispatch table ────────────────────────────────────────────────────────────

type Args = Record<string, unknown>;

const COMMANDS: Record<string, (args: Args) => unknown> = {
  app_version: () => cmdAppVersion(),
  current_project_summary: () => cmdCurrentProjectSummary(),
  open_project: (a) => cmdOpenProject(a as { root: string }),
  close_project: () => cmdCloseProject(),
  open_catalog_in_project: (a) =>
    cmdOpenCatalogInProject(a as { catalogPath: string }),
  update_unit_target_in_project: (a) =>
    cmdUpdateUnitTargetInProject(
      a as { catalogPath: string; unitId: UnitId; edit: TargetEdit },
    ),
  accept_unit_in_project: (a) =>
    cmdAcceptUnitInProject(a as { catalogPath: string; unitId: UnitId }),
  translate_unit_in_project: (a) =>
    cmdTranslateUnitInProject(a as { catalogPath: string; unitId: UnitId }),
  save_catalog_in_project: (a) =>
    cmdSaveCatalogInProject(a as { catalogPath: string }),
  save_all_dirty: () => cmdSaveAllDirty(),
  discard_changes_in_project: (a) =>
    cmdDiscardChangesInProject(a as { catalogPath: string }),
  is_catalog_dirty: (a) => cmdIsCatalogDirty(a as { catalogPath: string }),
  list_open_catalogs: () => cmdListOpenCatalogs(),
  translate_batch_in_project: (a) =>
    cmdTranslateBatchInProject(a as { catalogPath: string; scope: string }),
  cancel_translation: (a) => cmdCancelTranslation(a as { jobId: string }),
  scan_project_review_state: () => cmdScanProjectReviewState(),
  load_glossary: (a) => cmdLoadGlossary(a as { path: string }),
  save_glossary: (a) =>
    cmdSaveGlossary(a as { path: string; payload: GlossaryPayload }),
  list_locales: () => cmdListLocales(),
  // Stubs for commands not exercised by current tests.
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
  create_project: () => ({ summary: state.openProject, warnings: [] }),
  add_catalog_to_project: () => ({ summary: state.openProject, warnings: [] }),
  remove_catalog_from_project: () => ({
    summary: state.openProject,
    warnings: [],
  }),
  update_locale_in_project: () => ({
    summary: state.openProject,
    warnings: [],
  }),
  remove_locale_from_project: () => ({
    summary: state.openProject,
    warnings: [],
  }),
  set_backend_in_project: () => ({ summary: state.openProject, warnings: [] }),
  set_glossary_in_project: () => ({ summary: state.openProject, warnings: [] }),
  set_prompts_in_project: () => ({ summary: state.openProject, warnings: [] }),
  list_corrections_in_project: () => [],
  promote_correction_to_curated: () => undefined,
  un_curate_correction: () => true,
  list_curated_in_project: () => [],
  run_evaluation_in_project: () => ({ job_id: "mock-eval-job", total: 0 }),
  list_evaluation_runs_in_project: () => [],
  cancel_evaluation: () => true,
  export_tuning_bundle_in_project: () => ({
    path: "/mock/tuning/bundle",
    examples_count: 0,
    locales: [],
    has_score: false,
    prompt_template_version: "mock-v1",
  }),
  list_tuning_bundles_in_project: () => [],
  load_metrics: () => ({ path: "", events: [], error_count: 0, line_count: 0 }),
  open_catalog: () => ({ path: "", unit_count: 0, language: null, units: [] }),
  update_unit_target: () => {
    throw new Error("legacy command — use in-project variant");
  },
  save_catalog: () => {
    throw new Error("legacy command — use in-project variant");
  },
  discard_changes: () => {
    throw new Error("legacy command — use in-project variant");
  },
  translate_unit: () => {
    throw new Error("legacy command — use in-project variant");
  },
};

// ── Public mock invoke ────────────────────────────────────────────────────────

export function mockInvoke(command: string, args?: Args): Promise<unknown> {
  const handler = COMMANDS[command];
  if (!handler) {
    return Promise.reject(new Error(`Mock: unknown command "${command}"`));
  }
  try {
    const result = handler(args ?? {});
    return Promise.resolve(result);
  } catch (e) {
    return Promise.reject(e);
  }
}

// ── Dialog mocks ──────────────────────────────────────────────────────────────

// Overrideable in tests via window.__mockPickResult.
export function mockOpenDialog(_opts: unknown): Promise<string | null> {
  const result =
    (window as { __mockPickResult?: string | null }).__mockPickResult ?? null;
  return Promise.resolve(result);
}

export function mockSaveDialog(_opts: unknown): Promise<string | null> {
  return Promise.resolve("/mock/save/location.toml");
}
