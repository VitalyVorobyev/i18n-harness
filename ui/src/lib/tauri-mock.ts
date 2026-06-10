// Tauri IPC mock for browser-only tests (Playwright + Vitest).
//
// Activated when VITE_USE_TAURI_MOCK === "true".  The real `invoke` from
// @tauri-apps/api/core is replaced with a dispatch table that mirrors every
// command in tauri.ts, maintaining in-memory state so lifecycle transitions
// (untranslated → proposed → finished) work correctly across calls.
//
// Import this file only through the test setup or the conditional in tauri.ts.

const MOCK_TRANSLATE_DELAY_MS = 400;
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

import type {
  CatalogGateResponse,
  CatalogGateStats,
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
  // Reviewer notes keyed by `${catalogPath}\u{1F}${unitId}`, mirroring the
  // review.jsonl note the real backend surfaces on the review-queue item
  // (carries conflict-candidate JSON for `conflict` units).
  reviewNotes: Map<string, string>;
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
  reviewNotes: new Map(),
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
  state.reviewNotes = new Map();
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
  state.reviewNotes = new Map();
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

function emptyGateStats(): CatalogGateStats {
  return {
    total: 0,
    finished: 0,
    proposed: 0,
    untranslated: 0,
    vanished_obsolete: 0,
    hard: 0,
    soft: 0,
  };
}

// The mock does not run the real validation gate; it derives state counts
// from the catalog's units and reports no findings. Enough for component
// tests that only exercise the per-file stats wiring.
function cmdGateCatalogInProject(args: {
  catalogPath: string;
}): CatalogGateResponse {
  const catalog = state.catalogs.get(args.catalogPath);
  if (!catalog)
    throw new Error(`Catalog not found in mock: ${args.catalogPath}`);
  const stats = emptyGateStats();
  stats.total = catalog.units.length;
  for (const u of catalog.units) {
    if (u.state === "finished") stats.finished++;
    else if (u.state === "proposed") stats.proposed++;
    else if (u.state === "untranslated") stats.untranslated++;
    else stats.vanished_obsolete++;
  }
  return { path: args.catalogPath, reports: [], stats };
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

async function cmdTranslateUnitInProject(args: {
  catalogPath: string;
  unitId: UnitId;
}): Promise<TranslateResult> {
  await sleep(MOCK_TRANSLATE_DELAY_MS);
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
  const statsByCatalog: ReviewQueueResponse["stats_by_catalog"] = {};

  for (const [catalogPath, catalog] of state.catalogs) {
    const project = state.openProject;
    const catRef = project?.catalogs.find(
      (c) => c.absolute_path === catalogPath,
    );
    let count = 0;
    const counts = {
      total: 0,
      finished: 0,
      proposed: 0,
      untranslated: 0,
      vanished_obsolete: 0,
      needs_review: 0,
    };
    for (const unit of catalog.units) {
      counts.total++;
      if (unit.state === "finished") counts.finished++;
      else if (unit.state === "proposed") counts.proposed++;
      else if (unit.state === "untranslated") counts.untranslated++;
      else counts.vanished_obsolete++;
      const needsReview =
        unit.review_status === "needs-review" ||
        unit.review_status === "conflict" ||
        unit.flags.length > 0;
      if (!needsReview) continue;
      count++;
      counts.needs_review++;
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
        reviewer_note:
          state.reviewNotes.get(`${catalogPath}\u{1F}${unit.id}`) ?? null,
      });
    }
    if (count > 0) byCatalog[catalogPath] = count;
    statsByCatalog[catalogPath] = counts;
  }

  return {
    total_count: items.length,
    by_catalog: byCatalog,
    stats_by_catalog: statsByCatalog,
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
  gate_catalog_in_project: (a) =>
    cmdGateCatalogInProject(a as { catalogPath: string }),
  gate_catalog: () => ({
    path: "",
    reports: [],
    stats: emptyGateStats(),
  }),
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
  add_catalogs_from_folder: () => ({
    summary: state.openProject,
    warnings: ["Added 0 catalog(s); skipped 0 already-present."],
  }),
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
  write_text_file: () => undefined,
  // Reference reuse / remainder / merge — browser-mode stubs (the real work is
  // local file IO in the Rust library; not exercised by browser-only tests).
  reuse_references_in_project: (a) => {
    const args = a as { catalogPath: string; referencePaths: string[] | null };
    const catalog = state.catalogs.get(args.catalogPath);
    const refs = args.referencePaths ?? ["/sample-project/refs/expert_de.ts"];

    // Synthesize a representative outcome: copy a finished candidate into the
    // first untranslated singular unit, mark the second as a conflict (two
    // disagreeing candidates). This exercises the conflict view end-to-end.
    const untranslated = (catalog?.units ?? []).filter(
      (u) => u.state === "untranslated" && u.target.kind === "singular",
    );
    const conflicts: Array<{
      unit_id: string;
      candidates: Array<{
        reference: string;
        also_from: string[];
        is_plural: boolean;
        text: string;
      }>;
    }> = [];

    let copiedFinished = 0;
    const first = untranslated[0];
    if (first && catalog) {
      const unit = catalog.units.find((u) => u.id === first.id);
      if (unit) {
        unit.target = { kind: "singular", text: `[ref] ${unit.source}` };
        unit.state = "finished";
        unit.review_status = "reviewed";
        copiedFinished = 1;
      }
    }

    const second = untranslated[1];
    if (second && catalog) {
      const unit = catalog.units.find((u) => u.id === second.id);
      if (unit) {
        unit.review_status = "conflict";
      }
      const candidates = [
        {
          reference: refs[0] ?? "/sample-project/refs/expert_de.ts",
          also_from: [],
          is_plural: false,
          text: `[ref-A] ${second.source}`,
        },
        {
          reference: refs[1] ?? "/sample-project/refs/legacy_de.ts",
          also_from: [],
          is_plural: false,
          text: `[ref-B] ${second.source}`,
        },
      ];
      // Persist the candidate JSON as the review note, mirroring the real
      // backend — the scan surfaces it on the queue item's reviewer_note.
      state.reviewNotes.set(
        `${args.catalogPath}\u{1F}${second.id}`,
        JSON.stringify(candidates),
      );
      conflicts.push({ unit_id: second.id, candidates });
    }

    if (catalog) state.dirtyCatalogs.delete(args.catalogPath);

    // Everything untranslated except the copied-finished first unit and the
    // conflicted second unit is the remaining set (conflicts excluded, matching
    // the real reuse contract).
    const consumedIds = new Set(
      [first?.id, second?.id].filter((id): id is string => id != null),
    );
    const remainingIds = untranslated
      .filter((u) => !consumedIds.has(u.id))
      .map((u) => u.id);
    return {
      catalog_path: args.catalogPath,
      references: refs,
      copied_finished: copiedFinished,
      copied_needs_review: 0,
      conflict_count: conflicts.length,
      remaining_count: remainingIds.length,
      remaining_ids: remainingIds,
      conflicts,
    };
  },
  split_remainder: (a) => {
    const args = a as {
      catalogPath: string;
      outPath: string;
      onlyIds: string[] | null;
    };
    const catalog = state.catalogs.get(args.catalogPath);
    const writableUntranslated = (catalog?.units ?? []).filter(
      (u) => u.state === "untranslated",
    ).length;
    return {
      base_path: args.catalogPath,
      out_path: args.outPath,
      kept_count: args.onlyIds?.length ?? writableUntranslated,
    };
  },
  split_all_remainders: (a) => {
    const args = a as { outDir: string };
    const project = state.openProject;
    const referencePaths = new Set(
      (project?.references ?? []).map((r) => r.absolute_path),
    );
    const written: {
      base_path: string;
      out_path: string;
      kept_count: number;
    }[] = [];
    const skipped: string[] = [];
    for (const c of project?.catalogs ?? []) {
      if (c.format !== "qt-ts" || referencePaths.has(c.absolute_path)) continue;
      const catalog = state.catalogs.get(c.absolute_path);
      const keptCount = (catalog?.units ?? []).filter(
        (u) => u.state === "untranslated",
      ).length;
      if (keptCount === 0) {
        skipped.push(c.manifest_path);
        continue;
      }
      const base =
        c.absolute_path.split("/").pop()?.replace(/\.ts$/i, "") ?? "catalog";
      written.push({
        base_path: c.absolute_path,
        out_path: `${args.outDir}/${base}.remainder.ts`,
        kept_count: keptCount,
      });
    }
    return { written, skipped };
  },
  merge_catalogs: (a) => {
    const args = a as {
      basePath: string;
      withPath: string;
      outPath: string;
    };
    return {
      base_path: args.basePath,
      with_path: args.withPath,
      out_path: args.outPath,
      merged: 0,
      merged_complete: 0,
    };
  },
  add_project_reference: () => ({ summary: state.openProject, warnings: [] }),
  remove_project_reference: () => ({
    summary: state.openProject,
    warnings: [],
  }),
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

// Overrideable in tests via window.__mockSaveResult.
export function mockSaveDialog(_opts: unknown): Promise<string | null> {
  const result =
    (window as { __mockSaveResult?: string | null }).__mockSaveResult ??
    "/mock/save/location.toml";
  return Promise.resolve(result);
}
