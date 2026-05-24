// Mirror of the Rust serde shapes the Tauri commands return. Keep these
// hand-written rather than generated: the IPC surface is small, the
// schema is stable, and a hand-written file documents the wire format
// in one place.
//
// M4.2 project-mode additions are grouped at the bottom of this file.

export type UnitId = string;

export type UnitState =
  | "untranslated"
  | "proposed"
  | "finished"
  | "vanished"
  | "obsolete";

export type Target =
  | { kind: "singular"; text: string | null }
  | { kind: "plural"; forms: (string | null)[] };

export interface Provenance {
  file: string;
  line: number | null;
  byte_offset: number | null;
}

// Placeholder is opaque to the UI for now.
export type Placeholder = unknown;

export interface Unit {
  id: UnitId;
  source: string;
  target: Target;
  placeholders: Placeholder[];
  plural_arity: number | null;
  flags: unknown;
  provenance: Provenance;
  state: UnitState;
}

export interface CatalogResponse {
  path: string;
  unit_count: number;
  language: string | null;
  units: Unit[];
}

export interface SaveSummary {
  path: string;
  unit_count: number;
}

export interface LocaleInfo {
  id: string;
  register: "formal" | "informal" | "neutral";
  script: string;
  plural_arity: number;
}

export interface TermEntry {
  source: string;
  do_not_translate: boolean;
  notes?: string | null;
  translations: Record<string, string>;
}

export interface LocaleOverrideEntry {
  locale: string;
  register: "formal" | "informal" | "neutral" | null;
  variant: string | null;
}

export interface GlossaryPayload {
  schema_version: number;
  terms: TermEntry[];
  locale_overrides: LocaleOverrideEntry[];
}

export interface GlossaryLoadResponse {
  path: string;
  payload: GlossaryPayload;
  warnings: string[];
}

export interface GlossarySaveResponse {
  path: string;
  warnings: string[];
}

export interface MetricEvent {
  schema: number;
  ts: string;
  backend: string;
  locale: string;
  event: "gate-reject" | "soft-warning" | "human-edit" | "retry" | string;
  unit_id: string;
  rule: string;
  detail: Record<string, unknown> | unknown;
}

export interface MetricsResponse {
  path: string;
  events: MetricEvent[];
  error_count: number;
  line_count: number;
}

// Mirrors the Rust enum #[serde(tag = "kind", rename_all = "kebab-case")]
// in ui/src-tauri/src/lib.rs.
export type TargetEdit =
  | { kind: "singular"; text: string | null }
  | { kind: "plural"; form_index: number; text: string | null };

// Severity classification of a gate flag. Mirrors
// `i18n_harness_core::Flag::severity()` in Rust verbatim — that file is the
// canonical mapping; this duplication is the IPC bridge. Add or rename a
// flag there first, then update this file.
export type Severity = "hard" | "soft" | "semantic";

export interface Finding {
  flag: string;
  detail: { rule: string; [k: string]: unknown };
}

export interface GateReport {
  unit_id: UnitId;
  findings: Finding[];
  flags: unknown;
}

export interface TranslateResult {
  unit: Unit;
  report: GateReport;
}

const HARD_FLAGS = new Set([
  "placeholder-mismatch",
  "plural-arity-mismatch",
  "icu-parse-error",
  "empty-target-when-finished",
]);

const SOFT_FLAGS = new Set([
  "accel-mismatch",
  "length-warn",
  "cjk-punctuation-tolerated",
  "placeholder-agreement-risk",
  "markup-tag-mismatch",
]);

const SEMANTIC_FLAGS = new Set([
  "ambiguous-source",
  "idiom",
  "insufficient-context",
  "low-confidence",
]);

export function severityOf(flag: string): Severity {
  if (HARD_FLAGS.has(flag)) return "hard";
  if (SOFT_FLAGS.has(flag)) return "soft";
  if (SEMANTIC_FLAGS.has(flag)) return "semantic";
  // Unknown flag — be conservative: treat as soft so it surfaces but
  // does not look like a hard blocker.
  return "soft";
}

// Derived UI shape: per-row data used by the catalog list.
export interface UnitRow {
  id: UnitId;
  source: string;
  preview: string;
  state: UnitState;
  isPlural: boolean;
  pluralFilled: number;
  pluralTotal: number;
  fileHint: string;
}

export function unitRow(unit: Unit): UnitRow {
  const isPlural = unit.plural_arity != null;
  const totals = unit.target.kind === "plural" ? unit.target.forms.length : 1;
  const filled =
    unit.target.kind === "plural"
      ? unit.target.forms.filter((t) => t != null).length
      : unit.target.text != null
        ? 1
        : 0;
  return {
    id: unit.id,
    source: unit.source,
    preview: previewOf(unit.source),
    state: unit.state,
    isPlural,
    pluralFilled: filled,
    pluralTotal: totals,
    fileHint: unit.provenance.file || "",
  };
}

function previewOf(s: string): string {
  const oneLine = s.replace(/\s+/g, " ").trim();
  return oneLine.length > 80 ? `${oneLine.slice(0, 80)}…` : oneLine;
}

// ── M4.2 project-mode types ───────────────────────────────────────────────────

// ReviewStatus mirrors crates/core/src/review.rs — serde(rename_all = "kebab-case").
export type ReviewStatus =
  | "new"
  | "machine-translated"
  | "needs-review"
  | "reviewed"
  | "approved"
  | "locked"
  | "rejected"
  | "conflict";

// CatalogStatus mirrors crates/project/src/project.rs — serde(rename_all = "kebab-case").
export type CatalogStatus = "ok" | "missing" | "format-mismatch";

// CatalogFormat mirrors crates/project/src/manifest.rs — serde(rename_all = "kebab-case").
export type CatalogFormat = "qt-ts" | "gettext-po" | "icu-json";

// BackendKind mirrors crates/project/src/manifest.rs — serde(rename_all = "kebab-case").
export type BackendKind = "manual" | "ollama" | "open-ai-compatible" | "agent";

export interface BackendConfig {
  kind: BackendKind;
  model?: string | null;
  host?: string | null;
  num_ctx?: number | null;
}

// RegisterOverride mirrors crates/project/src/manifest.rs — serde(rename_all = "lowercase").
export type RegisterOverride = "formal" | "informal" | "neutral";

export interface LocaleConfig {
  register?: RegisterOverride | null;
  variant?: string | null;
  length_warn_ratio?: number | null;
}

export interface GlossaryConfig {
  path: string;
}

// CatalogRef mirrors crates/project/src/project.rs.
// Fields: absolute_path, manifest_path, format, locale, status.
// No serde(rename_all) on the struct — field names are snake_case.
export interface CatalogRef {
  absolute_path: string;
  manifest_path: string;
  format: CatalogFormat;
  locale: string;
  status: CatalogStatus;
}

// ProjectSummary mirrors crates/project/src/project.rs.
// All path fields are String (not PathBuf), already resolved.
export interface ProjectSummary {
  root: string;
  name: string;
  schema: number;
  locales: string[];
  catalogs: CatalogRef[];
  glossary_path: string | null;
  backend: BackendConfig | null;
  state_dir: string;
}

// ProjectOpenResponse — wire response for open_project / create_project.
export interface ProjectOpenResponse {
  summary: ProjectSummary;
  warnings: string[];
}

// FormatGuess mirrors crates/project/src/manifest.rs — serde(rename_all = "kebab-case").
export type FormatGuess = "qt-ts" | "gettext-po" | "icu-json" | "unknown";

// ClassificationConfidence mirrors crates/project/src/discovery/mod.rs — serde(rename_all = "kebab-case").
export type ClassificationConfidence = "high" | "medium" | "low";

export interface DraftAlternative {
  format: FormatGuess;
  locale: string | null;
  reason: string;
}

// DraftCatalog mirrors crates/project/src/discovery/mod.rs.
// path is PathBuf in Rust, serialized as a string by serde.
export interface DraftCatalog {
  path: string;
  format: FormatGuess;
  locale: string | null;
  confidence: ClassificationConfidence;
  reason: string;
  alternatives: DraftAlternative[];
}

// DraftManifest mirrors crates/project/src/discovery/mod.rs.
// locales is BTreeMap<String, LocaleConfig> — becomes Record<string, LocaleConfig>.
export interface DraftManifest {
  root: string;
  name: string;
  locales: Record<string, LocaleConfig>;
  catalogs: DraftCatalog[];
  glossary: GlossaryConfig | null;
  backend: BackendConfig | null;
}

// SaveAllDirtyResponse mirrors ui/src-tauri/src/lib.rs.
// failed_path / failed_reason are omitted by serde when None.
export interface SaveAllDirtyResponse {
  saved: SaveSummary[];
  failed_path?: string;
  failed_reason?: string;
}

// CorrectionId — a "corr_<12-hex>" string newtype on the Rust side.
export type CorrectionId = string;

// CorrectionProvenance mirrors crates/project/src/memory.rs.
export interface CorrectionProvenance {
  backend: string;
  model: string;
  model_version: string;
  prompt_template_version: string;
  glossary_version: string;
}

// Correction mirrors crates/project/src/memory.rs — full record.
export interface Correction {
  id: CorrectionId;
  catalog: string;
  locale: string;
  unit_id: UnitId;
  source: string;
  mt_proposal: string;
  human_target: string;
  provenance: CorrectionProvenance;
  flags_at_correction: unknown[];
  ts: string;
}
