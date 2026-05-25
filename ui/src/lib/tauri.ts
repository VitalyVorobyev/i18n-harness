// Typed wrappers around `invoke()` so component code never sees the
// stringly-typed call. One function per Tauri command + dialog helpers.

import { invoke } from "@tauri-apps/api/core";
import {
  open as openDialog,
  save as saveDialog,
} from "@tauri-apps/plugin-dialog";
import type {
  BackendConfig,
  CatalogEntry,
  CatalogResponse,
  Correction,
  CuratedExample,
  DraftManifest,
  GlossaryConfig,
  GlossaryLoadResponse,
  GlossaryPayload,
  GlossarySaveResponse,
  LocaleConfig,
  LocaleInfo,
  MetricsResponse,
  ProjectOpenResponse,
  ProjectSummary,
  PromptsConfig,
  ReviewQueueResponse,
  SaveAllDirtyResponse,
  SaveSummary,
  TargetEdit,
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

// ── M4.7 — Project-wide review queue ─────────────────────────────────────────

/// Scan every catalog in the open project for units that need human review.
///
/// A unit qualifies if `review_status === "needs-review"` OR `flags.length > 0`.
/// Catalogs not yet open in the project store are extracted and cached as a
/// side effect (same as `openCatalogInProject`, but without returning the catalog).
export async function scanProjectReviewState(): Promise<ReviewQueueResponse> {
  return await invoke<ReviewQueueResponse>("scan_project_review_state");
}
