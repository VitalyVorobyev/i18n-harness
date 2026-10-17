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
  warnings: number;
  /** Strings another file of the same language translates differently. */
  inconsistent: number;
  /** Strings that occur in no other file of the same language. */
  only_here: number;
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

/** Which strings to list beyond their status. */
export type Show = "everything" | "problems" | "warnings" | "inconsistent" | "shared";

export interface UnitFilter {
  statuses: UnitStatus[];
  show: Show;
  search: string | null;
}

/** One line of a string list; the selected string's full view comes from `getUnit`. */
export interface UnitRow {
  path: string;
  file: string;
  id: string;
  source: string;
  existing: string | null;
  target: string;
  status: UnitStatus;
  group: string | null;
  error: boolean;
  warning: boolean;
  shared: boolean;
  inconsistent: boolean;
}

/** The checks a project can tune, per target language. */
export interface CheckSettings {
  length_ratio: number;
  length_min_chars: number;
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
  /** Only translations that break the glossary, whatever their status. */
  glossary_issues: boolean;
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
