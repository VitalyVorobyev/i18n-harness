import { useMemo, useRef, useEffect } from "react";
import { StateBadge } from "../StateBadge/StateBadge";
import type { Unit, UnitId, UnitRow, UnitState } from "../../lib/types";
import { unitRow } from "../../lib/types";
import styles from "./CatalogList.module.css";

type Filter = "all" | "untranslated" | "proposed" | "finished";

interface Props {
  units: Unit[];
  selectedId: UnitId | null;
  filter: Filter;
  search: string;
  onSelect: (id: UnitId) => void;
  onFilterChange: (f: Filter) => void;
  onSearchChange: (s: string) => void;
}

export function CatalogList({
  units,
  selectedId,
  filter,
  search,
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
    };
    for (const r of rows) {
      c.all += 1;
      if (r.state === "untranslated") c.untranslated += 1;
      else if (r.state === "proposed") c.proposed += 1;
      else if (r.state === "finished") c.finished += 1;
    }
    return c;
  }, [rows]);

  const filtered = useMemo(() => {
    const lower = search.trim().toLowerCase();
    return rows.filter((r) => {
      if (filter !== "all" && r.state !== (filter as UnitState)) return false;
      if (lower && !rowMatches(r, lower)) return false;
      return true;
    });
  }, [rows, filter, search]);

  // Keyboard navigation: focus the list, then up/down moves the
  // selected unit. The container is the focusable element; rows are
  // not individually tabbable to keep the list scannable.
  const listRef = useRef<HTMLUListElement | null>(null);
  const onListKey = (e: React.KeyboardEvent<HTMLUListElement>) => {
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

  // Keep the selected row in view as it changes via keyboard.
  useEffect(() => {
    if (!selectedId || !listRef.current) return;
    const el = listRef.current.querySelector<HTMLElement>(
      `[data-unit-id="${cssEscape(selectedId)}"]`,
    );
    el?.scrollIntoView({ block: "nearest" });
  }, [selectedId]);

  return (
    <aside className={`${styles.root} app-chrome`}>
      <div className={styles.searchRow}>
        <input
          className={styles.search}
          type="search"
          placeholder="Filter…"
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
          aria-label="Filter units"
          spellCheck={false}
        />
      </div>

      <div className={styles.chips} role="tablist" aria-label="Unit filter">
        {(["all", "untranslated", "proposed", "finished"] as Filter[]).map(
          (f) => (
            <button
              key={f}
              type="button"
              role="tab"
              aria-selected={filter === f}
              className={`${styles.chip} ${filter === f ? styles.chipActive : ""}`}
              onClick={() => onFilterChange(f)}
            >
              <span className={styles.chipLabel}>{labelOf(f)}</span>
              <span className={styles.chipCount}>{counts[f]}</span>
            </button>
          ),
        )}
      </div>

      <ul
        ref={listRef}
        className={styles.list}
        tabIndex={0}
        onKeyDown={onListKey}
        aria-label="Translatable units"
      >
        {filtered.length === 0 ? (
          <li className={styles.empty}>
            <span>No units match this filter.</span>
          </li>
        ) : (
          filtered.map((r) => (
            <Row
              key={r.id}
              row={r}
              active={r.id === selectedId}
              onClick={() => onSelect(r.id)}
            />
          ))
        )}
      </ul>
    </aside>
  );
}

function Row({
  row,
  active,
  onClick,
}: {
  row: UnitRow;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <li
      className={`${styles.row} ${active ? styles.rowActive : ""}`}
      data-unit-id={row.id}
      onClick={onClick}
      role="option"
      aria-selected={active}
    >
      <div className={styles.rowTop}>
        <StateBadge state={row.state} variant="dot" />
        <span className={styles.rowId} title={row.id}>
          {row.id}
        </span>
        {row.isPlural && (
          <span
            className={styles.rowPlural}
            title={`${row.pluralFilled} of ${row.pluralTotal} plural forms filled`}
          >
            ×{row.pluralTotal}
          </span>
        )}
      </div>
      <div className={styles.rowPreview}>
        {row.preview || <span className={styles.rowPreviewMuted}>empty</span>}
      </div>
    </li>
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
  }
}

// CSS.escape isn't universally typed; tiny shim that handles what we
// throw at it (UnitIds may contain ::, &, spaces).
function cssEscape(s: string): string {
  if (typeof CSS !== "undefined" && typeof CSS.escape === "function") {
    return CSS.escape(s);
  }
  return s.replace(/(["\\\]\[!#$%&'()*+,./:;<=>?@^`{|}~])/g, "\\$1");
}
