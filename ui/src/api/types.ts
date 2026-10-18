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
  /** Strings whose original contains a glossary term. */
  glossary: number;
  /** Translations that do not follow a glossary term in their original. */
  glossary_broken: number;
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
  /** The glossary terms in the original. */
  terms: TermHit[];
  context: UnitContext;
  editable: boolean;
}

/** A glossary term found in a string's original. */
export interface TermHit {
  term: string;
  keep: boolean;
  /** The translation the glossary requires for the string's language. */
  translation: string | null;
  /** The current translation does not follow the term. */
  broken: boolean;
}

/** Which strings to list beyond their status. */
export type Show =
  | "everything"
  | "problems"
  | "warnings"
  | "inconsistent"
  | "shared"
  | "glossary"
  | "glossary-broken";

export interface UnitFilter {
  statuses: UnitStatus[];
  show: Show;
  /** Only strings whose original contains this glossary term. */
  term: string | null;
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
  /** Glossary terms in the original. */
  terms: number;
  glossary_broken: boolean;
}

/** The same string in another file of the same language. */
export interface SiblingView {
  path: string;
  file: string;
  /** Its translation there, per form (empty: none), in the syntax of the file asked about. */
  target: string[];
  /** The text the file held when opened, when it is in another language (the file has a pivot). */
  existing: string[] | null;
  status: UnitStatus;
}

/** A string that occurs in several files of one language, with each file's translation. */
export interface SharedString {
  id: string;
  source: string;
  group: string | null;
  language: string;
  members: SiblingView[];
  /** How many different translations the files have (0: none yet). */
  translations: number;
  /** Files where it has no translation yet. */
  missing: number;
  /** The files' pivot-language texts differ from each other. */
  existing_differs: boolean;
}

/** What filling strings from other files did, or would do. */
export interface FillReport {
  filled: number;
  /** Left alone: other files translate them differently. */
  conflicting: number;
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

/** How the strings of one language use a glossary term. */
export interface TermUsage {
  term: string;
  strings: number;
  breaking: number;
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
