// Mirror of the Rust serde shapes the Tauri commands return. Keep these
// hand-written rather than generated: the IPC surface is small, the
// schema is stable, and a hand-written file documents the wire format
// in one place.
//
// Project-mode additions are grouped at the bottom of this file.

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

// ModelFlag enumerates the semantic flag kinds the model may attach to a unit.
// These are the kebab-case serde renderings of `crates/core/src/flag.rs` Flag variants.
// The list is exhaustive — TypeScript enforces it in humanizeFlag below.
export type ModelFlag =
  | "ambiguous-source"
  | "idiom"
  | "insufficient-context"
  | "low-confidence"
  | "brand-term"
  | "tone-mismatch";

// AnyFlag covers model flags plus gate-produced flag strings.
// The full set mirrors the Rust Flag enum (kebab-case serde).
export type AnyFlag = string;

export interface Unit {
  id: UnitId;
  source: string;
  target: Target;
  placeholders: Placeholder[];
  plural_arity: number | null;
  // FlagSet serializes as a JSON array of kebab-case flag strings.
  flags: AnyFlag[];
  provenance: Provenance;
  state: UnitState;
  // May be absent on older JSONL.
  review_status?: ReviewStatus | null;
  source_hash?: string | null;
  // May be absent when the v1 backend was used.
  confidence?: number | null;
  // flag_notes is a map from kebab-case flag name to the model's note.
  flag_notes?: Record<AnyFlag, string> | null;
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

/** Per-catalog state + severity breakdown from {@link CatalogGateResponse}. */
export interface CatalogGateStats {
  total: number;
  finished: number;
  proposed: number;
  untranslated: number;
  vanished_obsolete: number;
  /** Units with at least one hard finding. */
  hard: number;
  /** Units with findings but no hard finding (soft/semantic only). */
  soft: number;
}

/** Result of gating a whole open catalog (inspector reasons + per-file stats). */
export interface CatalogGateResponse {
  path: string;
  /** Reports for flagged units only; clean units are omitted. */
  reports: GateReport[];
  stats: CatalogGateStats;
}

const HARD_FLAGS = new Set([
  "placeholder-mismatch",
  "plural-arity-mismatch",
  "icu-parse-error",
  "empty-target-when-finished",
  // Backend returned a non-parseable response (hard: no translation to ship).
  "backend-malformed-response",
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
  // New semantic flag variants.
  "brand-term",
  "tone-mismatch",
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
  // Number of model/gate flags on this unit (0 when clean).
  flagCount: number;
  // Names of the flags for the tooltip (kebab-case).
  flagNames: AnyFlag[];
  // True when unit.review_status === "needs-review" OR unit.flags is non-empty.
  // Drives the "needs-review" filter in the CatalogList.
  needsReview: boolean;
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
  const flagNames: AnyFlag[] = Array.isArray(unit.flags) ? unit.flags : [];
  const needsReview =
    unit.review_status === "needs-review" || flagNames.length > 0;
  return {
    id: unit.id,
    source: unit.source,
    preview: previewOf(unit.source),
    state: unit.state,
    isPlural,
    pluralFilled: filled,
    pluralTotal: totals,
    fileHint: unit.provenance.file || "",
    flagCount: flagNames.length,
    flagNames,
    needsReview,
  };
}

function previewOf(s: string): string {
  const oneLine = s.replace(/\s+/g, " ").trim();
  return oneLine.length > 80 ? `${oneLine.slice(0, 80)}…` : oneLine;
}

/// Human-readable label for a model-supplied flag. The switch is exhaustive
/// over all `ModelFlag` variants: if a new variant is added to the Rust enum
/// and the serde kebab-case rendering appears here, this function must be
/// updated. An `assertNever` call at the bottom of the switch ensures the
/// TypeScript compiler surfaces any forgotten case at build time.
export function humanizeFlag(flag: ModelFlag): string {
  switch (flag) {
    case "ambiguous-source":
      return "Ambiguous source";
    case "idiom":
      return "Idiom";
    case "insufficient-context":
      return "Insufficient context";
    case "low-confidence":
      return "Low confidence";
    case "brand-term":
      return "Brand term";
    case "tone-mismatch":
      return "Tone mismatch";
    default:
      return assertNever(flag);
  }
}

function assertNever(x: never): never {
  throw new Error(`Unhandled ModelFlag variant: ${String(x)}`);
}

// ── Project-mode types ────────────────────────────────────────────────────────

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

// CatalogEntry mirrors crates/project/src/manifest.rs — the input shape for
// add_catalog_to_project. path is PathBuf in Rust, serialized as a string.
export interface CatalogEntry {
  path: string;
  format: CatalogFormat;
  locale: string;
}

// PromptsConfig mirrors crates/project/src/manifest.rs.
// template_dir is PathBuf in Rust, serialized as a string.
export interface PromptsConfig {
  template_dir: string;
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

// ReferenceRef mirrors crates/project/src/project.rs.
// Parallel to CatalogRef but for [[references]] entries. Field names snake_case.
export interface ReferenceRef {
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
  references: ReferenceRef[];
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

// ── Project-wide review queue types ──────────────────────────────────────────

// ReviewQueueItem mirrors the Rust struct of the same name in ui/src-tauri/src/lib.rs.
export interface ReviewQueueItem {
  /** Absolute path to the catalog file on disk. */
  catalog_path: string;
  /** Manifest-relative path for display in the table. */
  catalog_manifest_path: string;
  /** Target locale id (e.g. "de_DE"). */
  locale: string;
  /** The unit's id string. */
  unit_id: string;
  /** Source text, truncated to 120 chars at a word boundary. */
  source_preview: string;
  /** Target text, truncated to 120 chars; empty string when untranslated. */
  target_preview: string;
  /** Kebab-case flag names; empty when only NeedsReview triggered inclusion. */
  flags: AnyFlag[];
  /** Kebab-case ReviewStatus variant, or null when not set. */
  review_status: ReviewStatus | null;
  /** Kebab-case unit state. */
  state: UnitState;
  /**
   * Reviewer note from the unit's last review event, if any. For `conflict`
   * units this carries the JSON-encoded ReferenceConflictCandidate[] so the
   * conflict view survives a project reopen.
   */
  reviewer_note: string | null;
}

// CatalogStateCounts mirrors the Rust struct of the same name — a per-catalog
// unit-state tally computed during the review scan.
export interface CatalogStateCounts {
  total: number;
  finished: number;
  proposed: number;
  untranslated: number;
  vanished_obsolete: number;
  needs_review: number;
}

// ReviewQueueResponse mirrors the Rust struct of the same name.
export interface ReviewQueueResponse {
  /** Total units that need review across all catalogs. */
  total_count: number;
  /** Per-catalog needs-review unit count, keyed by absolute catalog path. */
  by_catalog: Record<string, number>;
  /** Per-catalog unit-state tally, keyed by absolute catalog path. */
  stats_by_catalog: Record<string, CatalogStateCounts>;
  /** All items, sorted by catalog path then unit id. */
  items: ReviewQueueItem[];
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

// CuratedExample mirrors crates/project/src/memory.rs.
// correction is None for dangling references (correction deleted from jsonl).
export interface CuratedExample {
  id: CorrectionId;
  note: string | null;
  correction: Correction | null;
}

// ── Quality eval types ────────────────────────────────────────────────────────

// LocaleScore mirrors crates/project/src/evaluation.rs.
export interface LocaleScore {
  score: number;
  count: number;
}

// FlagScore mirrors crates/project/src/evaluation.rs.
export interface FlagScore {
  score: number;
  count: number;
}

// EvaluationRun mirrors crates/project/src/evaluation.rs.
export interface EvaluationRun {
  schema: number;
  ts: string;
  prompt_template_version: string;
  overall_score: number;
  per_locale: Record<string, LocaleScore>;
  per_flag_kind: Record<string, FlagScore>;
  example_count: number;
}

// EvaluationStarted mirrors ui/src-tauri/src/lib.rs.
export interface EvaluationStarted {
  job_id: string;
  total: number;
}

// EvaluationProgressPayload mirrors ui/src-tauri/src/lib.rs.
export interface EvaluationProgressPayload {
  job_id: string;
  completed: number;
  total: number;
  last_example_locale: string;
}

// EvaluationTerminalPayload mirrors ui/src-tauri/src/lib.rs.
export interface EvaluationTerminalPayload {
  job_id: string;
  cancelled: boolean;
  failed_reason: string | null;
  run: EvaluationRun | null;
}

// ── Tuning bundle types ───────────────────────────────────────────────────────

// ExportTuningBundleResponse mirrors ui/src-tauri/src/lib.rs ExportTuningBundleResponse.
export interface ExportTuningBundleResponse {
  /** Absolute path to the exported bundle directory. */
  path: string;
  /** Number of resolved examples written to examples.jsonl. */
  examples_count: number;
  /** Locale ids that appear in at least one example. */
  locales: string[];
  /** true if score.json was written (prior evaluation existed). */
  has_score: boolean;
  /** Prompt template version identifier baked into prompt.txt. */
  prompt_template_version: string;
}

// ── Reference reuse / remainder split / merge types ──────────────────────────

// ReferenceEntry mirrors crates/project/src/manifest.rs — the input shape for
// addProjectReference. path is PathBuf in Rust, serialized as a string.
export interface ReferenceEntry {
  path: string;
  format: CatalogFormat;
  locale: string;
}

// ReferenceConflictCandidateDto mirrors ui/src-tauri/src/dto/reuse.rs.
// `text` joins plural CLDR forms with the unit separator U+001F; split on it
// when `is_plural` is true.
export interface ReferenceConflictCandidate {
  /** Absolute path of the reference catalog this candidate came from. */
  reference: string;
  /** Other references that supplied this exact same translation. */
  also_from: string[];
  /** Whether `text` is a plural target (forms joined with U+001F) or singular. */
  is_plural: boolean;
  /** Candidate translation; plural forms joined with U+001F. */
  text: string;
}

// ReferenceConflictDto mirrors ui/src-tauri/src/dto/reuse.rs.
export interface ReferenceConflict {
  /** The base unit id where references disagreed. */
  unit_id: string;
  /** Distinct candidate translations, in declaration order. */
  candidates: ReferenceConflictCandidate[];
}

// ReuseReportDto mirrors ui/src-tauri/src/dto/reuse.rs.
// The in-memory catalog entry is refreshed server-side before this returns;
// re-open / re-list the catalog to pull the post-reuse units.
export interface ReuseReport {
  /** Absolute path of the base catalog the reuse was applied to. */
  catalog_path: string;
  /** Absolute paths of the reference catalogs consulted, in priority order. */
  references: string[];
  /** Units promoted to Finished (agreed candidate, gate-clean, complete). */
  copied_finished: number;
  /** Units that received an agreed candidate but kept at Proposed for review. */
  copied_needs_review: number;
  /** Units where references disagreed; nothing copied. Detail in `conflicts`. */
  conflict_count: number;
  /** Writable units with no candidate — feed a subsequent split. */
  remaining_count: number;
  /**
   * The exact remaining-set ids (catalog order), conflicts excluded. Pass these
   * to `splitRemainder` as `onlyIds` so an Export Remainder right after a reuse
   * pass carves out precisely the leftovers and never the conflicted units.
   */
  remaining_ids: string[];
  /** Full per-unit conflict detail. */
  conflicts: ReferenceConflict[];
}

// SplitReportDto mirrors ui/src-tauri/src/dto/reuse.rs.
export interface SplitReport {
  /** Absolute path of the base catalog the subset was carved from. */
  base_path: string;
  /** Absolute path the remainder subset was written to. */
  out_path: string;
  /** Number of units written into the remainder. */
  kept_count: number;
}

// BatchSplitReportDto mirrors ui/src-tauri/src/dto/reuse.rs — one batch run
// over every non-reference Qt catalog in the project.
export interface BatchSplitReport {
  /** One entry per catalog that had untranslated units and was written. */
  written: SplitReport[];
  /** Manifest-relative paths of catalogs skipped (fully translated). */
  skipped: string[];
}

// MergeReportDto mirrors ui/src-tauri/src/dto/reuse.rs.
// Overlap / stray-id guard failures come back as a rejected promise (Err
// string) that names the offending ids, not in this struct.
export interface MergeReport {
  /** Absolute path of the base catalog the remainder was folded into. */
  base_path: string;
  /** Absolute path of the translated remainder that was merged. */
  with_path: string;
  /** Absolute path the merged result was written to. */
  out_path: string;
  /** Remainder units folded into the base as override translations. */
  merged: number;
  /** Of `merged`, how many carried a complete (finished-ready) target. */
  merged_complete: number;
}

// ── Bulk translate with cancellation ─────────────────────────────────────────

// BatchScope mirrors ui/src-tauri/src/lib.rs. Kebab-case enum on the wire.
export type BatchScope = "untranslated" | "untranslated-and-proposed";

// TranslateBatchStarted mirrors ui/src-tauri/src/lib.rs.
// Returned synchronously from translate_batch_in_project; the worker runs in
// the background and emits batch-progress / batch-completed / batch-failed
// events keyed by job_id.
export interface TranslateBatchStarted {
  /** Opaque process-unique job id (UUID v4 hex, no hyphens). */
  job_id: string;
  /** Number of units the worker will attempt at start time. */
  total: number;
}

// BatchUnitStartedPayload mirrors ui/src-tauri/src/lib.rs.
// Emitted on `batch-unit-started-<job_id>` just before each backend call begins.
export interface BatchUnitStartedPayload {
  /** Id of the unit about to be translated. */
  unit_id: string;
  /** Target locale for this translation call. */
  locale: string;
}

// BatchProgressPayload mirrors ui/src-tauri/src/lib.rs.
// Emitted on `batch-progress-<job_id>` after each completed network round-trip.
export interface BatchProgressPayload {
  /** Units processed so far (1-indexed). */
  completed: number;
  /** Total units the worker started with. */
  total: number;
  /** The just-translated unit (post-merge). */
  unit: Unit;
  /** True if the gate or LLM attached one or more flags. */
  flagged: boolean;
}

// BatchTerminalPayload mirrors ui/src-tauri/src/lib.rs.
// Emitted exactly once on `batch-completed-<job_id>` (clean or cancelled) or
// `batch-failed-<job_id>` (mid-batch hard failure).
export interface BatchTerminalPayload {
  /** Units processed when the worker stopped. */
  completed: number;
  /** Total units the worker started with. */
  total: number;
  /** True if the worker observed cancellation. Mutually exclusive with `failed_reason`. */
  cancelled: boolean;
  /** Hard-failure reason; null on success / cancellation. */
  failed_reason: string | null;
}
