// StatusFilter — three multi-select toggle pills over Unit.state.
//
// The pills are composable: any combination of Untranslated / Proposed /
// Finished can be selected at once. An empty selection means "show
// everything" (the natural "no filter" semantics).
//
// Two exports:
//   unitMatchesFilter — pure predicate for a single Unit
//   StatusFilter      — multi-select pill button row component

import { cn } from "../../lib/cn";
import type { Unit, UnitState } from "../../lib/types";

// ── Filter state shape ────────────────────────────────────────────────────────
//
// `Set<UnitState>` keeps the shape symmetric with `Unit.state` and makes the
// predicate trivial. The three UI-editable states are the only valid members;
// Vanished/Obsolete are never shown regardless of selection.

export type StatusFilterState = Set<UnitState>;

export const EMPTY_STATUS_FILTER: StatusFilterState = new Set<UnitState>();

// ── Pure predicate ────────────────────────────────────────────────────────────
//
// Empty filter → pass everything (the user has chosen "no filter").
// Non-empty → pass when the unit's state is in the set.
// Vanished/Obsolete units never pass — they aren't UI-editable.

export function unitMatchesFilter(
  unit: Unit,
  filter: StatusFilterState,
): boolean {
  if (unit.state === "vanished" || unit.state === "obsolete") return false;
  if (filter.size === 0) return true;
  return filter.has(unit.state);
}

// ── Pill definitions ──────────────────────────────────────────────────────────

interface Pill {
  id: Extract<UnitState, "untranslated" | "proposed" | "finished">;
  label: string;
}

const PILLS: readonly Pill[] = [
  { id: "untranslated", label: "Untranslated" },
  { id: "proposed", label: "Proposed" },
  { id: "finished", label: "Finished" },
];

// ── Component ─────────────────────────────────────────────────────────────────

interface StatusFilterProps {
  value: StatusFilterState;
  onChange: (next: StatusFilterState) => void;
}

export function StatusFilter({ value, onChange }: StatusFilterProps) {
  const toggle = (id: Pill["id"]) => {
    const next = new Set(value);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    onChange(next);
  };

  return (
    <div
      role="toolbar"
      aria-label="Status filter"
      className="flex flex-wrap gap-1"
    >
      {PILLS.map((pill) => {
        const active = value.has(pill.id);
        return (
          <button
            key={pill.id}
            type="button"
            aria-pressed={active}
            onClick={() => toggle(pill.id)}
            className={cn(
              "inline-flex items-center h-6 px-2 rounded-sm text-xs font-medium",
              "transition-colors duration-100 ease-out",
              "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
              active
                ? "text-fg-primary bg-accent-subtle border border-accent-subtle-border"
                : "text-fg-tertiary border border-transparent hover:text-fg-primary hover:bg-bg-hover",
            )}
          >
            {pill.label}
          </button>
        );
      })}
    </div>
  );
}
