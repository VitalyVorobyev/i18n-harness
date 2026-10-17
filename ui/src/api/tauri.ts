import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import type { Api } from "./api";
import type { TranslationEvent } from "./types";

/** The real backend: Tauri commands. */
export const tauriApi: Api = {
  appInfo: () => invoke("app_info"),
  listLanguages: () => invoke("list_languages"),
  pickFolder: async () => {
    const picked = await open({ directory: true, multiple: false });
    return typeof picked === "string" ? picked : null;
  },
  openFolder: (path) => invoke("open_folder", { path }),
  closeFolder: () => invoke("close_folder"),
  listCatalogs: () => invoke("list_catalogs"),
  listUnits: (path, filter, offset, limit) => invoke("list_units", { path, filter, offset, limit }),
  setTranslation: (path, id, forms, done) => invoke("set_translation", { path, id, forms, done }),
  setDone: (path, ids, done) => invoke("set_done", { path, ids, done }),
  save: (path) => invoke("save", { path }),
  setPivot: (path, pivot) => invoke("set_pivot", { path, pivot }),
  getGlossary: (locale) => invoke("get_glossary", { locale }),
  setGlossary: (locale, entries) => invoke("set_glossary", { locale, entries }),
  pickTable: async () => {
    const picked = await open({
      multiple: false,
      filters: [{ name: "Term table", extensions: ["csv", "tsv", "txt"] }],
    });
    return typeof picked === "string" ? picked : null;
  },
  readGlossaryTable: (path) => invoke("read_glossary_table", { path }),
  planSize: (scope) => invoke("plan_size", { scope }),
  getModelSettings: () => invoke("get_model_settings"),
  setModelSettings: (settings) => invoke("set_model_settings", { settings }),
  listModels: (settings) => invoke("list_models", { settings }),
  translateOne: (path, id, input) => invoke("translate_one", { path, id, input }),
  startTranslation: (scope, input) => invoke("start_translation", { scope, input }),
  cancelTranslation: () => invoke("cancel_translation"),
  onTranslation: (listener) => listen<TranslationEvent>("translation", (e) => listener(e.payload)),
};
