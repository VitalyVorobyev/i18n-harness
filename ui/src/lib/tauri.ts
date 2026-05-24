// Typed wrappers around `invoke()` so component code never sees the
// stringly-typed call. One function per Tauri command + dialog helpers.

import { invoke } from "@tauri-apps/api/core";
import {
  open as openDialog,
  save as saveDialog,
} from "@tauri-apps/plugin-dialog";
import type {
  CatalogResponse,
  GlossaryLoadResponse,
  GlossaryPayload,
  GlossarySaveResponse,
  LocaleInfo,
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
