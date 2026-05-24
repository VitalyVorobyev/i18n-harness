// Typed wrappers around `invoke()` so component code never sees the
// stringly-typed call. One function per Tauri command + dialog helpers.

import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type {
  CatalogResponse,
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
