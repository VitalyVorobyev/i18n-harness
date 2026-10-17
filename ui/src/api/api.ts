import type {
  AppInfo,
  CatalogSummary,
  CommandError,
  FolderView,
  GlossaryEntry,
  InputText,
  Language,
  ModelChoice,
  ModelSettings,
  CheckSettings,
  SaveReport,
  TranslateScope,
  TranslationEvent,
  UnitFilter,
  UnitRow,
  UnitView,
} from "./types";

/** The desktop shell's commands (ui/src-tauri/src/commands.rs, translation.rs). */
export interface Api {
  appInfo(): Promise<AppInfo>;
  listLanguages(): Promise<Language[]>;
  /** Ask the user for a folder; `null` when they cancel. */
  pickFolder(): Promise<string | null>;
  openFolder(path: string): Promise<FolderView>;
  closeFolder(): Promise<void>;
  listCatalogs(): Promise<CatalogSummary[]>;
  /** The strings of one file, or of every file when `path` is `null`, that match `filter`. */
  listRows(path: string | null, filter: UnitFilter): Promise<UnitRow[]>;
  getUnit(path: string, id: string): Promise<UnitView>;
  setTranslation(path: string, id: string, forms: string[], done: boolean): Promise<UnitView>;
  setDone(path: string, ids: string[], done: boolean): Promise<UnitView[]>;
  save(path: string | null): Promise<SaveReport>;
  setPivot(path: string, pivot: string | null): Promise<CatalogSummary>;
  getGlossary(locale: string): Promise<GlossaryEntry[]>;
  setGlossary(locale: string, entries: GlossaryEntry[]): Promise<GlossaryEntry[]>;
  /** Ask the user for a term table (CSV or tab separated); `null` when they cancel. */
  pickTable(): Promise<string | null>;
  /** The terms of a table file, for the glossary editor to merge. */
  readGlossaryTable(path: string): Promise<GlossaryEntry[]>;
  /** How many strings a run over `scope` would cover. */
  planSize(scope: TranslateScope): Promise<number>;
  getCheckSettings(locale: string): Promise<CheckSettings>;
  setCheckSettings(locale: string, settings: CheckSettings): Promise<CheckSettings>;
  getModelSettings(): Promise<ModelSettings>;
  setModelSettings(settings: ModelSettings): Promise<ModelSettings>;
  listModels(settings: ModelSettings): Promise<ModelChoice[]>;
  translateOne(path: string, id: string, input: InputText): Promise<UnitView>;
  /** Starts a background run; resolves with the number of strings in it. */
  startTranslation(scope: TranslateScope, input: InputText): Promise<number>;
  cancelTranslation(): Promise<void>;
  /** Subscribe to translation progress; returns the unsubscribe function. */
  onTranslation(listener: (event: TranslationEvent) => void): Promise<() => void>;
}

export const DEFAULT_MODEL_SETTINGS: ModelSettings = {
  server: "ollama",
  host: "http://localhost:11434",
  model: null,
  num_ctx: null,
};

/** The message of whatever a command rejected with. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as CommandError).message);
  }
  return String(error);
}

/** The kind of a command error (`"backend"`, `"needs-model"`, …). */
export function errorKind(error: unknown): string | null {
  if (error && typeof error === "object" && "kind" in error) {
    return String((error as CommandError).kind);
  }
  return null;
}
