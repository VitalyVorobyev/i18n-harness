// StatusFilter — segmented pill buttons for filtering units by translation state
// and gate severity. Reusable across MatrixView, FocusView, and any future
// consumer that evaluates one Unit at a time.
//
// Two exports:
//   unitMatchesFilter — pure predicate for a single Unit
//   StatusFilter      — segmented pill button row component

import { cn } from "../../lib/cn";
import type { Unit } from "../../lib/types";
import { severityOf } from "../../lib/types";

// ── Filter id union ──────────────────────────────────────────────────────────

export type StatusFilterId =
  | "all"
  | "all-open"
  | "untranslated"
  | "proposed"
  | "needs-review"
  | "has-hard-flag"
  | "proposed-by-model";

// ── Pure predicate ────────────────────────────────────────────────────────────
//
// Evaluates a single Unit against a StatusFilterId. FocusView evaluates one
// unit at a time; MatrixView maps across rows. Both use this shared predicate.
//
// Note on "proposed-by-model": the Unit wire type does not currently expose a
// distinct origin field for model vs. human proposals, so this filter matches
// all proposed units (same behaviour as MatrixView's rowMatchesFilter). When
// a `proposed_by` or `origin` field is added to the Rust type, update the
// predicate here first.

export function unitMatchesFilter(unit: Unit, filter: StatusFilterId): boolean {
  switch (filter) {
    case "all":
      return true;
    case "all-open":
      return (
        unit.state !== "finished" &&
        unit.state !== "vanished" &&
        unit.state !== "obsolete"
      );
    case "untranslated":
      return unit.state === "untranslated";
    case "proposed":
      return unit.state === "proposed";
    case "needs-review":
      return (
        Array.isArray(unit.flags) &&
        unit.flags.some((flag) => severityOf(flag) === "soft")
      );
    case "has-hard-flag":
      return (
        Array.isArray(unit.flags) &&
        unit.flags.some((flag) => severityOf(flag) === "hard")
      );
    case "proposed-by-model":
      // No origin field on Unit yet — matches all proposed units as a
      // conservative approximation. Update when the wire type gains origin.
      return unit.state === "proposed";
  }
}

// ── Pill definitions ──────────────────────────────────────────────────────────

interface Pill {
  id: StatusFilterId;
  label: string;
}

const PILLS: Pill[] = [
  { id: "all", label: "All" },
  { id: "all-open", label: "Open" },
  { id: "untranslated", label: "Untranslated" },
  { id: "proposed", label: "Proposed" },
  { id: "needs-review", label: "Needs review" },
  { id: "has-hard-flag", label: "Hard flag" },
  { id: "proposed-by-model", label: "Model proposal" },
];

// ── Component ─────────────────────────────────────────────────────────────────

interface StatusFilterProps {
  value: StatusFilterId;
  onChange: (id: StatusFilterId) => void;
}

export function StatusFilter({ value, onChange }: StatusFilterProps) {
  return (
    <div
      role="tablist"
      aria-label="Status filter"
      className="flex flex-wrap gap-1"
    >
      {PILLS.map((pill) => (
        <button
          key={pill.id}
          type="button"
          role="tab"
          aria-selected={value === pill.id}
          onClick={() => onChange(pill.id)}
          className={cn(
            "inline-flex items-center h-6 px-2 rounded-sm text-xs font-medium",
            "transition-colors duration-100 ease-out",
            "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
            value === pill.id
              ? "text-fg-primary bg-accent-subtle border border-accent-subtle-border"
              : "text-fg-tertiary border border-transparent hover:text-fg-primary hover:bg-bg-hover",
          )}
        >
          {pill.label}
        </button>
      ))}
    </div>
  );
}
