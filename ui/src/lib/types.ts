// Mirror of the Rust serde shapes the Tauri commands return. Keep these
// hand-written rather than generated: the IPC surface is small, the
// schema is stable, and a hand-written file documents the wire format
// in one place.

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

// Mirrors the Rust enum #[serde(tag = "kind", rename_all = "kebab-case")]
// in ui/src-tauri/src/lib.rs.
export type TargetEdit =
  | { kind: "singular"; text: string | null }
  | { kind: "plural"; form_index: number; text: string | null };

// Severity classification of a gate flag. Kept in sync with
// `i18n_harness_core::Flag::severity()` in Rust — that file is the
// canonical mapping; this duplication is the IPC bridge.
export type Severity = "hard" | "soft" | "info";

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

const INFO_FLAGS = new Set(["cjk-punctuation-tolerated"]);

export function severityOf(flag: string): Severity {
  if (HARD_FLAGS.has(flag)) return "hard";
  if (INFO_FLAGS.has(flag)) return "info";
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
