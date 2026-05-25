import { useEffect, useMemo, useRef } from "react";
import { cn } from "../../lib/cn";
import type { Unit, UnitId, UnitRow, UnitState } from "../../lib/types";
import { unitRow } from "../../lib/types";
import { StateBadge } from "../StateBadge/StateBadge";

export type Filter =
  | "all"
  | "untranslated"
  | "proposed"
  | "finished"
  | "needs-review";

interface Props {
  units: Unit[];
  selectedId: UnitId | null;
  filter: Filter;
  search: string;
  dirtyIds: Set<UnitId>;
  onSelect: (id: UnitId) => void;
  onFilterChange: (f: Filter) => void;
  onSearchChange: (s: string) => void;
}

export function CatalogList({
  units,
  selectedId,
  filter,
  search,
  dirtyIds,
  onSelect,
  onFilterChange,
  onSearchChange,
}: Props) {
  const rows = useMemo(() => units.map(unitRow), [units]);

  const counts = useMemo(() => {
    const c: Record<Filter, number> = {
      all: 0,
      untranslated: 0,
      proposed: 0,
      finished: 0,
      "needs-review": 0,
    };
    for (const r of rows) {
      c.all += 1;
      if (r.state === "untranslated") c.untranslated += 1;
      else if (r.state === "proposed") c.proposed += 1;
      else if (r.state === "finished") c.finished += 1;
      if (r.needsReview) c["needs-review"] += 1;
    }
    return c;
  }, [rows]);

  const filtered = useMemo(() => {
    const lower = search.trim().toLowerCase();
    return rows.filter((r) => {
      if (filter === "needs-review") {
        if (!r.needsReview) return false;
      } else if (filter !== "all" && r.state !== (filter as UnitState)) {
        return false;
      }
      if (lower && !rowMatches(r, lower)) return false;
      return true;
    });
  }, [rows, filter, search]);

  const listRef = useRef<HTMLDivElement | null>(null);
  const onListKey = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    if (filtered.length === 0) return;
    const idx = filtered.findIndex((r) => r.id === selectedId);
    const next =
      e.key === "ArrowDown"
        ? Math.min(filtered.length - 1, idx + 1)
        : Math.max(0, idx - 1);
    const target = filtered[next];
    if (target) onSelect(target.id);
  };

  useEffect(() => {
    if (!selectedId || !listRef.current) return;
    const el = listRef.current.querySelector<HTMLElement>(
      `[data-unit-id="${cssEscape(selectedId)}"]`,
    );
    el?.scrollIntoView({ block: "nearest" });
  }, [selectedId]);

  return (
    <aside
      className={cn(
        "app-chrome shrink-0 w-80 min-w-[240px] flex flex-col overflow-hidden",
        "bg-bg-surface border-r border-border-subtle",
      )}
    >
      <div className="px-3 pt-3 pb-2">
        <input
          type="search"
          placeholder="Filter…"
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
          aria-label="Filter units"
          spellCheck={false}
          className={cn(
            "w-full h-[30px] px-3 rounded-md border bg-bg-input",
            "border-border-default text-sm text-fg-primary placeholder:text-fg-tertiary",
            "transition-colors duration-100 ease-out",
            "focus:border-accent focus:outline-none focus-visible:outline-none",
          )}
        />
      </div>

      <div
        role="tablist"
        aria-label="Unit filter"
        className="flex gap-1 px-3 pb-2 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
      >
        {(
          [
            "all",
            "untranslated",
            "proposed",
            "finished",
            "needs-review",
          ] as Filter[]
        ).map((f) => (
          <button
            key={f}
            type="button"
            role="tab"
            aria-selected={filter === f}
            onClick={() => onFilterChange(f)}
            className={cn(
              "inline-flex items-center gap-2 h-6 px-2 rounded-md whitespace-nowrap",
              "text-xs font-medium border transition-colors duration-100 ease-out",
              filter === f
                ? "text-fg-primary bg-accent-subtle border-accent-subtle-border"
                : "text-fg-secondary bg-transparent border-transparent hover:bg-bg-hover hover:text-fg-primary",
            )}
          >
            <span>{labelOf(f)}</span>
            <span
              className={cn(
                "tabular-nums",
                filter === f ? "text-accent" : "text-fg-tertiary",
              )}
            >
              {counts[f]}
            </span>
          </button>
        ))}
      </div>

      <div
        ref={listRef}
        role="listbox"
        tabIndex={0}
        onKeyDown={onListKey}
        aria-label="Translatable units"
        aria-activedescendant={
          selectedId ? `unit-${cssEscape(selectedId)}` : undefined
        }
        className="flex-1 overflow-y-auto py-1 pb-3 outline-none focus-visible:[box-shadow:inset_0_0_0_2px_var(--color-accent)] rounded-sm"
      >
        {filtered.length === 0 ? (
          <div className="px-4 py-6 text-sm text-fg-tertiary text-center">
            No units match this filter.
          </div>
        ) : (
          filtered.map((r) => (
            <Row
              key={r.id}
              row={r}
              active={r.id === selectedId}
              dirty={dirtyIds.has(r.id)}
              onClick={() => onSelect(r.id)}
            />
          ))
        )}
      </div>
    </aside>
  );
}

function Row({
  row,
  active,
  dirty,
  onClick,
}: {
  row: UnitRow;
  active: boolean;
  dirty: boolean;
  onClick: () => void;
}) {
  return (
    <div
      id={`unit-${row.id}`}
      data-unit-id={row.id}
      onClick={onClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick();
        }
      }}
      role="option"
      aria-selected={active}
      tabIndex={-1}
      className={cn(
        "pl-[10px] pr-3 py-2 border-l-2 cursor-pointer",
        "transition-colors duration-100 ease-out",
        active
          ? "bg-bg-selected border-l-accent"
          : "border-l-transparent hover:bg-bg-hover",
      )}
    >
      <div className="flex items-center gap-2 mb-px">
        <StateBadge state={row.state} variant="dot" />
        <span
          className={cn(
            "flex-1 min-w-0 truncate font-mono text-xs",
            active ? "text-fg-primary" : "text-fg-secondary",
          )}
          title={row.id}
        >
          {row.id}
        </span>
        {dirty && (
          <span
            role="img"
            className="text-state-proposed font-bold leading-none"
            aria-label="Unsaved changes"
            title="Unsaved changes"
          >
            •
          </span>
        )}
        {row.flagCount > 0 && (
          <span
            role="img"
            className={cn(
              "inline-flex items-center gap-0.5 h-4 px-1 rounded-sm border",
              "text-[10px] font-medium leading-none tabular-nums",
              "bg-severity-soft-bg border-severity-soft-border text-severity-soft",
            )}
            title={`${row.flagCount} flag${row.flagCount === 1 ? "" : "s"}: ${row.flagNames.join(", ")}`}
            aria-label={`${row.flagCount} flag${row.flagCount === 1 ? "" : "s"}: ${row.flagNames.join(", ")}`}
          >
            <span aria-hidden="true">⚑</span>
            {row.flagCount}
          </span>
        )}
        {row.isPlural && (
          <span
            className="font-mono text-xs text-fg-tertiary px-1 rounded-sm bg-bg-elevated"
            title={`${row.pluralFilled} of ${row.pluralTotal} plural forms filled`}
          >
            ×{row.pluralTotal}
          </span>
        )}
      </div>
      <div className="font-sans text-sm leading-snug text-fg-primary truncate">
        {row.preview || <span className="text-fg-disabled italic">empty</span>}
      </div>
    </div>
  );
}

function rowMatches(row: UnitRow, lower: string): boolean {
  if (row.id.toLowerCase().includes(lower)) return true;
  if (row.source.toLowerCase().includes(lower)) return true;
  return false;
}

function labelOf(f: Filter): string {
  switch (f) {
    case "all":
      return "All";
    case "untranslated":
      return "Untrans.";
    case "proposed":
      return "Proposed";
    case "finished":
      return "Finished";
    case "needs-review":
      return "Needs review";
  }
}

function cssEscape(s: string): string {
  if (typeof CSS !== "undefined" && typeof CSS.escape === "function") {
    return CSS.escape(s);
  }
  return s.replace(/(["\\\][!#$%&'()*+,./:;<=>?@^`{|}~])/g, "\\$1");
}
