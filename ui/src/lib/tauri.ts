// Typed wrappers around `invoke()` so component code never sees the
// stringly-typed call. One function per Tauri command + dialog helpers.

import { invoke } from "@tauri-apps/api/core";
import {
  open as openDialog,
  save as saveDialog,
} from "@tauri-apps/plugin-dialog";
import type {
  BackendConfig,
  BatchScope,
  CatalogEntry,
  CatalogResponse,
  Correction,
  CuratedExample,
  DraftManifest,
  EvaluationRun,
  EvaluationStarted,
  ExportTuningBundleResponse,
  GlossaryConfig,
  GlossaryLoadResponse,
  GlossaryPayload,
  GlossarySaveResponse,
  LocaleConfig,
  LocaleInfo,
  MergeReport,
  MetricsResponse,
  ProjectOpenResponse,
  ProjectSummary,
  PromptsConfig,
  ReferenceEntry,
  ReuseReport,
  ReviewQueueResponse,
  SaveAllDirtyResponse,
  SaveSummary,
  SplitReport,
  TargetEdit,
  TranslateBatchStarted,
  TranslateResult,
  Unit,
  UnitId,
} from "./types";

export async function appVersion(): Promise<string> {
  return await invoke<string>("app_version");
}

export async function openCatalog(path: string): Promise<CatalogResponse> {
  return await invoke<CatalogResponse>("open_catalog", { path });
}

export async function updateUnitTarget(
  unitId: UnitId,
  edit: TargetEdit,
): Promise<Unit> {
  return await invoke<Unit>("update_unit_target", { unitId, edit });
}

export async function saveCatalog(outPath?: string): Promise<SaveSummary> {
  return await invoke<SaveSummary>("save_catalog", {
    outPath: outPath ?? null,
  });
}

export async function discardChanges(): Promise<CatalogResponse> {
  return await invoke<CatalogResponse>("discard_changes");
}

export async function translateUnit(unitId: UnitId): Promise<TranslateResult> {
  return await invoke<TranslateResult>("translate_unit", { unitId });
}

export async function pickCatalogFile(): Promise<string | null> {
  const selected = await openDialog({
    multiple: false,
    directory: false,
    filters: [
      {
        name: "Qt Linguist (.ts)",
        extensions: ["ts"],
      },
    ],
  });
  if (typeof selected === "string") return selected;
  return null;
}

export async function listLocales(): Promise<LocaleInfo[]> {
  return await invoke<LocaleInfo[]>("list_locales");
}

export async function loadGlossary(
  path: string,
): Promise<GlossaryLoadResponse> {
  return await invoke<GlossaryLoadResponse>("load_glossary", { path });
}

export async function saveGlossary(
  path: string,
  payload: GlossaryPayload,
): Promise<GlossarySaveResponse> {
  return await invoke<GlossarySaveResponse>("save_glossary", {
    path,
    payload,
  });
}

export async function pickGlossaryFile(): Promise<string | null> {
  const selected = await openDialog({
    multiple: false,
    directory: false,
    filters: [
      {
        name: "Glossary (.toml)",
        extensions: ["toml"],
      },
    ],
  });
  if (typeof selected === "string") return selected;
  return null;
}

export async function loadMetrics(path: string): Promise<MetricsResponse> {
  return await invoke<MetricsResponse>("load_metrics", { path });
}

export async function pickMetricsFile(): Promise<string | null> {
  const selected = await openDialog({
    multiple: false,
    directory: false,
    filters: [
      {
        name: "Metrics (.jsonl)",
        extensions: ["jsonl", "ndjson", "json"],
      },
    ],
  });
  if (typeof selected === "string") return selected;
  return null;
}

export async function pickGlossarySaveLocation(): Promise<string | null> {
  const selected = await saveDialog({
    title: "Save glossary",
    defaultPath: "glossary.toml",
    filters: [
      {
        name: "Glossary (.toml)",
        extensions: ["toml"],
      },
    ],
  });
  return selected ?? null;
}

export async function pickReviewSaveLocation(
  projectName: string,
): Promise<string | null> {
  const slug = projectName.toLowerCase().replace(/[^a-z0-9]+/g, "-");
  const selected = await saveDialog({
    title: "Save review report",
    defaultPath: `${slug}-review.md`,
    filters: [
      {
        name: "Markdown (.md)",
        extensions: ["md"],
      },
    ],
  });
  return selected ?? null;
}

/** Write UTF-8 text to `path`, creating or overwriting the file. */
export async function writeTextFile(
  path: string,
  content: string,
): Promise<void> {
  return await invoke<void>("write_text_file", { path, content });
}

// ── M4.2 project-mode wrappers ────────────────────────────────────────────────

export async function openProject(root: string): Promise<ProjectOpenResponse> {
  return await invoke<ProjectOpenResponse>("open_project", { root });
}

export async function discoverProject(root: string): Promise<DraftManifest> {
  return await invoke<DraftManifest>("discover_project", { root });
}

export async function createProject(
  root: string,
  draft: DraftManifest,
): Promise<ProjectOpenResponse> {
  return await invoke<ProjectOpenResponse>("create_project", { root, draft });
}

export async function closeProject(): Promise<void> {
  return await invoke<void>("close_project");
}

export async function currentProjectSummary(): Promise<ProjectSummary | null> {
  return await invoke<ProjectSummary | null>("current_project_summary");
}

export async function listCatalogs(): Promise<import("./types").CatalogRef[]> {
  return await invoke("list_catalogs");
}

export async function saveManifest(): Promise<void> {
  return await invoke<void>("save_manifest");
}

export async function openCatalogInProject(
  catalogPath: string,
): Promise<CatalogResponse> {
  return await invoke<CatalogResponse>("open_catalog_in_project", {
    catalogPath,
  });
}

export async function updateUnitTargetInProject(
  catalogPath: string,
  unitId: UnitId,
  edit: TargetEdit,
): Promise<Unit> {
  return await invoke<Unit>("update_unit_target_in_project", {
    catalogPath,
    unitId,
    edit,
  });
}

export async function saveCatalogInProject(
  catalogPath: string,
): Promise<SaveSummary> {
  return await invoke<SaveSummary>("save_catalog_in_project", { catalogPath });
}

export async function saveAllDirty(): Promise<SaveAllDirtyResponse> {
  return await invoke<SaveAllDirtyResponse>("save_all_dirty");
}

export async function discardChangesInProject(
  catalogPath: string,
): Promise<CatalogResponse> {
  return await invoke<CatalogResponse>("discard_changes_in_project", {
    catalogPath,
  });
}

export async function listOpenCatalogs(): Promise<string[]> {
  return await invoke<string[]>("list_open_catalogs");
}

export async function isCatalogDirty(catalogPath: string): Promise<boolean> {
  return await invoke<boolean>("is_catalog_dirty", { catalogPath });
}

export async function translateUnitInProject(
  catalogPath: string,
  unitId: UnitId,
): Promise<TranslateResult> {
  return await invoke<TranslateResult>("translate_unit_in_project", {
    catalogPath,
    unitId,
  });
}

/// Propose a translation for one glossary term using the project's default
/// backend. Returns the proposed translation string.
///
/// Errors if the project has no glossary, the term is not found,
/// the term has `do_not_translate = true`, or the backend call fails.
export async function translateGlossaryTerm(
  projectPath: string,
  termId: string,
  targetLocale: string,
): Promise<string> {
  return await invoke<string>("translate_glossary_term", {
    projectPath,
    termId,
    targetLocale,
  });
}

export async function pickProjectFolder(): Promise<string | null> {
  const selected = await openDialog({
    multiple: false,
    directory: true,
  });
  if (typeof selected === "string") return selected;
  return null;
}

// ── M4.3c — Settings view mutation wrappers ───────────────────────────────────

export async function addCatalogToProject(
  entry: CatalogEntry,
): Promise<ProjectOpenResponse> {
  return await invoke<ProjectOpenResponse>("add_catalog_to_project", { entry });
}

export async function removeCatalogFromProject(
  path: string,
): Promise<ProjectOpenResponse> {
  return await invoke<ProjectOpenResponse>("remove_catalog_from_project", {
    path,
  });
}

export async function updateLocaleInProject(
  id: string,
  config: LocaleConfig,
): Promise<ProjectOpenResponse> {
  return await invoke<ProjectOpenResponse>("update_locale_in_project", {
    id,
    config,
  });
}

export async function removeLocaleFromProject(
  id: string,
): Promise<ProjectOpenResponse> {
  return await invoke<ProjectOpenResponse>("remove_locale_from_project", {
    id,
  });
}

export async function setBackendInProject(
  config: BackendConfig,
): Promise<ProjectOpenResponse> {
  return await invoke<ProjectOpenResponse>("set_backend_in_project", {
    config,
  });
}

export async function setGlossaryInProject(
  config: GlossaryConfig,
): Promise<ProjectOpenResponse> {
  return await invoke<ProjectOpenResponse>("set_glossary_in_project", {
    config,
  });
}

export async function setPromptsInProject(
  config: PromptsConfig,
): Promise<ProjectOpenResponse> {
  return await invoke<ProjectOpenResponse>("set_prompts_in_project", {
    config,
  });
}

export async function pickCatalogFileForProject(): Promise<string | null> {
  const selected = await openDialog({
    multiple: false,
    directory: false,
    filters: [
      {
        name: "Catalogs",
        extensions: ["ts", "po", "json"],
      },
    ],
  });
  if (typeof selected === "string") return selected;
  return null;
}

// ── M4.3d — Quality view wrappers ─────────────────────────────────────────────

/** Filter shape for listCorrectionsInProject. All fields optional (AND semantics). */
export interface ListCorrectionsFilter {
  catalog_path?: string | null;
  locale?: string | null;
  unit_id?: string | null;
  curated_only?: boolean;
}

export async function listCorrectionsInProject(
  filter: ListCorrectionsFilter,
): Promise<Correction[]> {
  return await invoke<Correction[]>("list_corrections_in_project", { filter });
}

export async function promoteCorrectionToCurated(
  id: string,
  note?: string | null,
): Promise<void> {
  return await invoke<void>("promote_correction_to_curated", {
    id,
    note: note ?? null,
  });
}

export async function unCurateCorrection(id: string): Promise<boolean> {
  return await invoke<boolean>("un_curate_correction", { id });
}

export async function listCuratedInProject(): Promise<CuratedExample[]> {
  return await invoke<CuratedExample[]>("list_curated_in_project");
}

// ── M4.6.2 — Accept (clear flags, mark Reviewed) ─────────────────────────────

/// Mark the unit as reviewed: clears model flags + flag_notes in memory and
/// appends a `Reviewed` event to `review.jsonl`. Returns the updated unit.
export async function acceptUnitInProject(
  catalogPath: string,
  unitId: UnitId,
): Promise<Unit> {
  return await invoke<Unit>("accept_unit_in_project", { catalogPath, unitId });
}

// ── M4.2c.2 — bulk translate with cancellation ──────────────────────────────

/// Start a background bulk translation of in-scope units in `catalogPath`.
///
/// Returns the job id + total unit count synchronously; the worker runs in
/// the background and emits Tauri events keyed by job id:
/// - `batch-progress-<job_id>` after every successful unit
///   (payload: `BatchProgressPayload`).
/// - `batch-completed-<job_id>` on clean exit OR observed cancellation
///   (payload: `BatchTerminalPayload`; `cancelled` distinguishes the two).
/// - `batch-failed-<job_id>` on mid-batch hard failure
///   (payload: `BatchTerminalPayload` with `failed_reason` set).
///
/// Subscribe to all three event names before awaiting this call so the
/// progress emit of a very fast first unit is not missed; unsubscribe on
/// the terminal event.
///
/// Errors with `"a translation is already running for this catalog/locale"`
/// if another bulk run is in flight for the same pair.
export async function translateBatchInProject(
  catalogPath: string,
  scope: BatchScope,
): Promise<TranslateBatchStarted> {
  return await invoke<TranslateBatchStarted>("translate_batch_in_project", {
    catalogPath,
    scope,
  });
}

/// Signal cancellation for the named in-flight `translate_batch_in_project`
/// job. Returns `true` if a job with that id existed, `false` otherwise.
///
/// Cancellation is cooperative — the worker observes the flag between units,
/// so the currently-running unit completes before the worker exits. Wait for
/// the `batch-completed-<job_id>` event (with `cancelled: true`) to know the
/// worker has stopped.
export async function cancelTranslation(jobId: string): Promise<boolean> {
  return await invoke<boolean>("cancel_translation", { jobId });
}

// ── M4.7 — Project-wide review queue ─────────────────────────────────────────

// ── M4.9 — Quality eval wrappers ─────────────────────────────────────────────

/// Start a background evaluation run over all curated examples in the project.
///
/// Returns the job id + total example count synchronously. The worker emits:
/// - `eval-progress-<job_id>` after each example (EvaluationProgressPayload).
/// - `eval-completed-<job_id>` on clean exit (EvaluationTerminalPayload, run set).
/// - `eval-failed-<job_id>` on hard failure (EvaluationTerminalPayload, run null).
///
/// Only available when the ollama feature is compiled in. Errors with
/// "evaluation already running" if a prior run has not finished.
export async function runEvaluationInProject(): Promise<EvaluationStarted> {
  return await invoke<EvaluationStarted>("run_evaluation_in_project");
}

/// Returns all past evaluation runs for the open project, newest-first.
/// Always available (reads the store; does not require the ollama feature).
export async function listEvaluationRunsInProject(): Promise<EvaluationRun[]> {
  return await invoke<EvaluationRun[]>("list_evaluation_runs_in_project");
}

/// Signal cancellation for an in-flight evaluation job.
/// Returns true if the job existed, false otherwise.
export async function cancelEvaluation(jobId: string): Promise<boolean> {
  return await invoke<boolean>("cancel_translation", { jobId });
}

// ── M4.10 — Tuning bundle wrappers ───────────────────────────────────────────

/// Export a tuning bundle to `.i18n-harness/tuning/<timestamp>/`.
///
/// Calls `Project::export_tuning_bundle` under a brief project lock. The
/// bundle contains the curated example set, the active prompt template,
/// the latest evaluation run (if one exists), the locale config, and a copy
/// of the skill README.
///
/// Errors with "no curated examples; promote some corrections first" when the
/// curated set is empty.
export async function exportTuningBundleInProject(): Promise<ExportTuningBundleResponse> {
  return await invoke<ExportTuningBundleResponse>(
    "export_tuning_bundle_in_project",
  );
}

/// List previously-exported tuning bundles for the open project, newest-first.
///
/// Reads `.i18n-harness/tuning/` and returns one summary per bundle
/// subdirectory that contains a valid `examples.jsonl`. Returns an empty
/// array when no bundles have been exported yet.
export async function listTuningBundlesInProject(): Promise<
  ExportTuningBundleResponse[]
> {
  return await invoke<ExportTuningBundleResponse[]>(
    "list_tuning_bundles_in_project",
  );
}

/// Scan every catalog in the open project for units that need human review.
///
/// A unit qualifies if `review_status === "needs-review"` OR `flags.length > 0`.
/// Catalogs not yet open in the project store are extracted and cached as a
/// side effect (same as `openCatalogInProject`, but without returning the catalog).
export async function scanProjectReviewState(): Promise<ReviewQueueResponse> {
  return await invoke<ReviewQueueResponse>("scan_project_review_state");
}

// ── Reference reuse / remainder split / merge wrappers ───────────────────────

/// Reuse expert translations from reference catalogs into a project catalog by
/// exact unit-id match. Runs synchronously and locally (no model, no network).
///
/// When `referencePaths` is omitted the manifest's references matching the base
/// catalog's locale are used (in declaration / priority order); pass an explicit
/// list to override that selection ad-hoc.
///
/// On success the in-memory catalog entry for `catalogPath` is refreshed
/// server-side, so re-open / re-list the catalog to pull the copied
/// translations. Review-status events are persisted for conflicts (`conflict`,
/// with the candidate list stored as the review note so it survives a reopen)
/// and copied-needs-review units (`needs-review`).
export async function reuseReferencesInProject(
  catalogPath: string,
  referencePaths?: string[] | null,
): Promise<ReuseReport> {
  return await invoke<ReuseReport>("reuse_references_in_project", {
    catalogPath,
    referencePaths: referencePaths ?? null,
  });
}

/// Carve a remainder subset of `catalogPath` into `outPath`, keeping only the
/// untranslated leftovers.
///
/// Pass `onlyIds` (e.g. the reuse report's remaining ids) to keep an exact set;
/// omit it to keep the catalog's writable-untranslated units (standalone split).
export async function splitRemainder(
  catalogPath: string,
  outPath: string,
  onlyIds?: string[] | null,
): Promise<SplitReport> {
  return await invoke<SplitReport>("split_remainder", {
    catalogPath,
    outPath,
    onlyIds: onlyIds ?? null,
  });
}

/// Merge a translated remainder back into its base, writing the result to
/// `outPath`.
///
/// Rejects with an error string that names the offending unit ids when the merge
/// guards fail (remainder contains ids not in the base, or an id is finished in
/// both halves) so the UI can show which units broke the merge.
export async function mergeCatalogs(
  basePath: string,
  withPath: string,
  outPath: string,
): Promise<MergeReport> {
  return await invoke<MergeReport>("merge_catalogs", {
    basePath,
    withPath,
    outPath,
  });
}

/// Declare a reference catalog in the project manifest and persist it.
/// Errors if the path already exists in the references list or the file is not
/// found on disk. Returns a fresh project response so the UI can re-render.
export async function addProjectReference(
  entry: ReferenceEntry,
): Promise<ProjectOpenResponse> {
  return await invoke<ProjectOpenResponse>("add_project_reference", { entry });
}

/// Remove the reference at `path` (manifest-relative) from the project manifest
/// and persist it. Idempotent — succeeds whether or not an entry matched.
export async function removeProjectReference(
  path: string,
): Promise<ProjectOpenResponse> {
  return await invoke<ProjectOpenResponse>("remove_project_reference", {
    path,
  });
}

/// Pick one or more Qt Linguist `.ts` reference files (ad-hoc reuse / manifest
/// declaration). Returns absolute paths, or null when the dialog was dismissed.
export async function pickReferenceFiles(): Promise<string[] | null> {
  const selected = await openDialog({
    multiple: true,
    directory: false,
    filters: [{ name: "Qt Linguist (.ts)", extensions: ["ts"] }],
  });
  if (Array.isArray(selected)) return selected;
  if (typeof selected === "string") return [selected];
  return null;
}

/// Pick a single translated-remainder `.ts` file to merge back into a base.
export async function pickRemainderFile(): Promise<string | null> {
  const selected = await openDialog({
    multiple: false,
    directory: false,
    filters: [{ name: "Qt Linguist (.ts)", extensions: ["ts"] }],
  });
  if (typeof selected === "string") return selected;
  return null;
}

/// Save-dialog for a remainder / merged-output `.ts` path. `defaultName` seeds
/// the suggested filename (e.g. `app_de.remainder.ts`).
export async function pickTsSaveLocation(
  title: string,
  defaultName: string,
): Promise<string | null> {
  const selected = await saveDialog({
    title,
    defaultPath: defaultName,
    filters: [{ name: "Qt Linguist (.ts)", extensions: ["ts"] }],
  });
  return selected ?? null;
}
