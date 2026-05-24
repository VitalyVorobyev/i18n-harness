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

// Placeholder + Flag are opaque to the UI for now — we read them through
// but don't introspect their shape.
export type Placeholder = unknown;
export type FlagSet = unknown;

export interface Unit {
  id: UnitId;
  source: string;
  target: Target;
  placeholders: Placeholder[];
  plural_arity: number | null;
  flags: FlagSet;
  provenance: Provenance;
  state: UnitState;
}

export interface CatalogResponse {
  path: string;
  unit_count: number;
  units: Unit[];
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
  const totals =
    unit.target.kind === "plural" ? unit.target.forms.length : 1;
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
