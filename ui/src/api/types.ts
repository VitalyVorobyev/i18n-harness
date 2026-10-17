// Mirrors of the serialized session views (crates/session/src/view.rs and
// friends). Field names follow serde's defaults: snake_case fields,
// kebab-case enum values.

export type UnitStatus = "to-do" | "check" | "done";
export type UnitState = "untranslated" | "proposed" | "finished" | "vanished" | "obsolete";
export type IssueSeverity = "error" | "warning" | "note";

export interface Issue {
  severity: IssueSeverity;
  message: string;
}

export interface StatusCounts {
  total: number;
  to_do: number;
  check: number;
  done: number;
  errors: number;
}

export interface CatalogSummary {
  path: string;
  name: string;
  relative_path: string;
  format: string;
  format_name: string;
  locale: string;
  language: string;
  pivot: string | null;
  pivot_language: string | null;
  counts: StatusCounts;
  dirty: boolean;
}

export interface PluralSlot {
  category: string | null;
  example: number | null;
}

export interface UnitContext {
  group?: string;
  disambiguation?: string;
  developer_notes?: string[];
  translator_notes?: string[];
}

export interface UnitView {
  id: string;
  source: string;
  existing: string[] | null;
  target: string[];
  plural_slots: PluralSlot[];
  status: UnitStatus;
  state: UnitState;
  issues: Issue[];
  context: UnitContext;
  editable: boolean;
}

export interface UnitFilter {
  statuses: UnitStatus[];
  errors_only: boolean;
  search: string | null;
}

export interface UnitPage {
  units: UnitView[];
  matching: number;
}

export interface SaveReport {
  written: string[];
  held_back: number;
}

export interface GlossaryEntry {
  term: string;
  keep: boolean;
  translation: string | null;
  note: string | null;
}

export type ServerKind = "ollama" | "openai-compatible";
export type PromptStyle = "chat" | "completion" | "unknown";

export interface ModelSettings {
  server: ServerKind;
  host: string;
  model: string | null;
  num_ctx: number | null;
}

export interface ModelChoice {
  id: string;
  size_bytes: number | null;
  parameter_size: string | null;
  quantization: string | null;
  context_length: number | null;
  prompt_style: PromptStyle;
}

export type InputText = "auto" | "source";

export interface TranslateScope {
  catalogs: string[];
  statuses: UnitStatus[];
  ids: string[] | null;
}

export type Progress =
  | { event: "started"; total: number }
  | {
      event: "unit";
      catalog: string;
      id: string;
      done: number;
      total: number;
      translated: boolean;
      failure: string | null;
    }
  | { event: "finished"; translated: number; failed: number; cancelled: boolean };

export interface TranslationEvent {
  progress: Progress;
  unit: UnitView | null;
  catalog: CatalogSummary | null;
}

export interface FolderView {
  root: string;
  catalogs: CatalogSummary[];
  warnings: string[];
}

export interface Language {
  id: string;
  name: string;
}

export interface AppInfo {
  version: string;
  cldr: string;
}

/** What every command rejects with. */
export interface CommandError {
  kind: string;
  message: string;
}
