// MatrixView — the matrix-mode Translate surface. One card per source
// unit; cells fan out across the project locales. Replaces the previous
// 3-pane (CatalogList + UnitEditor + Inspector) inline layout — the 3-pane
// components live on as Focus-mode fallback in TranslatePanel.
//
// Data shape: the project carries N per-locale catalogs sharing a stem
// (e.g. app_de.ts, app_es.ts share stem "app"). We auto-load every catalog
// into the openCatalogs cache on mount, then group units by
// (catalog_stem, unit_id) so one card stitches together its per-locale
// cells.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { cn } from "../../lib/cn";
import type {
  BatchScope,
  CatalogRef,
  CatalogResponse,
  GateReport,
  ProjectSummary,
  TargetEdit,
  Unit,
  UnitId,
} from "../../lib/types";
import { severityOf } from "../../lib/types";
import { Eyebrow, LocaleTag, ProgressBar } from "../primitives";
import type { Filter as CatalogFilter } from "./CatalogList/CatalogList";
import { Inspector } from "./Inspector/Inspector";
import { MatrixCell } from "./MatrixCell";

// ── Extended filter union ────────────────────────────────────────────────
//
// The legacy CatalogList carries "all" | "untranslated" | "proposed" |
// "finished" | "needs-review". Matrix mode adds three more saved-view
// names — they reuse the existing filter state shape since the App state
// machine is unchanged. Filtering happens client-side over the materialised
// matrix; no new IPC needed for PR 4.

export type MatrixFilter =
  | CatalogFilter
  | "all-open"
  | "has-hard-flag"
  | "proposed-by-model";

interface Props {
  summary: ProjectSummary;
  openCatalogs: Map<string, CatalogResponse>;
  dirtyIds: Set<UnitId>;
  reports: Record<UnitId, GateReport>;
  busyIds: Set<UnitId>;
  batchActive: boolean;
  filter: MatrixFilter;
  search: string;
  onFilterChange: (f: MatrixFilter) => void;
  onSearchChange: (s: string) => void;
  /** Auto-load a project catalog into the App's openCatalogs cache. The
   *  call is silent — no toast, no selection change. Lets Matrix view
   *  fan out across every locale without disturbing the active-catalog
   *  state machine. */
  onEnsureCatalogLoaded: (absPath: string) => Promise<void>;
  /** Set the session focus locale (clicking a locale in the rail switches
   *  to Focus mode — PR 5 wires the dedicated view). */
  setFocusLocale: (locale: string | null) => void;
  /** Per-cell IPC dispatchers. The container (App) owns the actual
   *  IPC wiring + optimistic state merging. */
  onTranslateUnit: (catalogPath: string, unit: Unit) => void;
  onEditUnit: (catalogPath: string, unit: Unit, edit: TargetEdit) => void;
  onAcceptUnit: (catalogPath: string, unit: Unit) => void;
  /** Run-model-on-selection — dispatches `translate_batch_in_project`
   *  once per locale for the current saved-view scope. */
  onTranslateAll: (catalogPath: string, scope: BatchScope) => void;
}

// ── Matrix-row data model ────────────────────────────────────────────────
//
// One row keys on (stem, unit_id). `unitByLocale` maps a project locale to
// its Unit when the locale's catalog is loaded; missing entries render as
// the "loading…" cell.

interface MatrixRow {
  rowKey: string;
  stem: string;
  unitId: UnitId;
  /** Source string — taken from the first loaded locale (all per-locale
   *  catalogs share the same source for a given unit). */
  source: string;
  isPlural: boolean;
  placeholderCount: number;
  /** Project locales → unit + catalog path. */
  byLocale: Map<string, { unit: Unit; catalogPath: string }>;
}

// Strip the trailing `_<locale>` segment from a basename to compute the
// stem. Mirrors the App.tsx helper but kept local: that helper is private.
function stemOf(ref: CatalogRef): string {
  const basename =
    (ref.manifest_path || ref.absolute_path)
      .replace(/\\/g, "/")
      .split("/")
      .pop() ?? "";
  const noExt = basename.replace(/\.[^.]+$/, "");
  const escaped = ref.locale.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return noExt.replace(new RegExp(`_${escaped}$`, "i"), "");
}

function deriveMatrix(
  summary: ProjectSummary,
  openCatalogs: Map<string, CatalogResponse>,
): MatrixRow[] {
  const groups = new Map<string, MatrixRow>();

  for (const ref of summary.catalogs) {
    const stem = stemOf(ref);
    const cached = openCatalogs.get(ref.absolute_path);
    if (!cached) continue;
    for (const unit of cached.units) {
      const rowKey = `${stem}::${unit.id}`;
      const existing = groups.get(rowKey);
      if (existing) {
        existing.byLocale.set(ref.locale, {
          unit,
          catalogPath: ref.absolute_path,
        });
        continue;
      }
      const placeholders = Array.isArray(unit.placeholders)
        ? unit.placeholders.length
        : 0;
      const row: MatrixRow = {
        rowKey,
        stem,
        unitId: unit.id,
        source: unit.source,
        isPlural: unit.plural_arity != null,
        placeholderCount: placeholders,
        byLocale: new Map([
          [ref.locale, { unit, catalogPath: ref.absolute_path }],
        ]),
      };
      groups.set(rowKey, row);
    }
  }

  return Array.from(groups.values()).sort((a, b) => {
    const stemCmp = a.stem.localeCompare(b.stem);
    if (stemCmp !== 0) return stemCmp;
    return a.unitId.localeCompare(b.unitId);
  });
}

// Filter predicate over a materialised MatrixRow.
function rowMatchesFilter(
  row: MatrixRow,
  filter: MatrixFilter,
  hideFinished: boolean,
): boolean {
  // "Hide finished" hides rows where every loaded cell is finished.
  if (hideFinished) {
    const loaded = Array.from(row.byLocale.values());
    if (
      loaded.length > 0 &&
      loaded.every((entry) => entry.unit.state === "finished")
    ) {
      return false;
    }
  }
  const cells = Array.from(row.byLocale.values());
  switch (filter) {
    case "all":
    case "all-open":
      return true;
    case "untranslated":
      return cells.some((entry) => entry.unit.state === "untranslated");
    case "proposed":
    case "proposed-by-model":
      return cells.some((entry) => entry.unit.state === "proposed");
    case "finished":
      return cells.some((entry) => entry.unit.state === "finished");
    case "needs-review":
      return cells.some(
        (entry) =>
          entry.unit.review_status === "needs-review" ||
          (Array.isArray(entry.unit.flags) && entry.unit.flags.length > 0),
      );
    case "has-hard-flag":
      return cells.some(
        (entry) =>
          Array.isArray(entry.unit.flags) &&
          entry.unit.flags.some((flag) => severityOf(flag) === "hard"),
      );
    default:
      return true;
  }
}

function rowMatchesSearch(row: MatrixRow, lower: string): boolean {
  if (lower === "") return true;
  if (row.unitId.toLowerCase().includes(lower)) return true;
  if (row.source.toLowerCase().includes(lower)) return true;
  return false;
}

// ── Per-locale aggregate for the left rail ────────────────────────────────

interface LocaleStat {
  locale: string;
  finished: number;
  proposed: number;
  untranslated: number;
  total: number;
}

function aggregateLocaleStats(
  summary: ProjectSummary,
  openCatalogs: Map<string, CatalogResponse>,
): LocaleStat[] {
  const stats = new Map<string, LocaleStat>();
  for (const ref of summary.catalogs) {
    const cur = stats.get(ref.locale) ?? {
      locale: ref.locale,
      finished: 0,
      proposed: 0,
      untranslated: 0,
      total: 0,
    };
    const cached = openCatalogs.get(ref.absolute_path);
    if (cached) {
      for (const u of cached.units) {
        cur.total += 1;
        if (u.state === "finished") cur.finished += 1;
        else if (u.state === "proposed") cur.proposed += 1;
        else if (u.state === "untranslated") cur.untranslated += 1;
      }
    }
    stats.set(ref.locale, cur);
  }
  return summary.locales.map(
    (locale) =>
      stats.get(locale) ?? {
        locale,
        finished: 0,
        proposed: 0,
        untranslated: 0,
        total: 0,
      },
  );
}

// ── Per-card counters for the saved-view counts in the left rail ─────────

function countByFilter(rows: MatrixRow[]): Record<MatrixFilter, number> {
  const c: Record<MatrixFilter, number> = {
    all: 0,
    "all-open": 0,
    "needs-review": 0,
    "has-hard-flag": 0,
    "proposed-by-model": 0,
    proposed: 0,
    untranslated: 0,
    finished: 0,
  };
  for (const row of rows) {
    c.all += 1;
    c["all-open"] += 1;
    if (rowMatchesFilter(row, "needs-review", false)) c["needs-review"] += 1;
    if (rowMatchesFilter(row, "has-hard-flag", false)) c["has-hard-flag"] += 1;
    if (rowMatchesFilter(row, "proposed", false)) c.proposed += 1;
    if (rowMatchesFilter(row, "proposed", false)) c["proposed-by-model"] += 1;
    if (rowMatchesFilter(row, "untranslated", false)) c.untranslated += 1;
    if (rowMatchesFilter(row, "finished", false)) c.finished += 1;
  }
  return c;
}

// ── Saved-view definitions for the left rail ──────────────────────────────

interface SavedView {
  id: MatrixFilter;
  label: string;
  /** Tint class for the count badge — purely cosmetic. */
  tint?: "default" | "soft" | "hard" | "proposed";
}

const SAVED_VIEWS: SavedView[] = [
  { id: "all-open", label: "All open" },
  { id: "needs-review", label: "Needs review", tint: "soft" },
  { id: "has-hard-flag", label: "Has hard flag", tint: "hard" },
  { id: "proposed-by-model", label: "Proposed by model", tint: "proposed" },
  { id: "untranslated", label: "Untranslated" },
];

// ── Component ─────────────────────────────────────────────────────────────

export function MatrixView({
  summary,
  openCatalogs,
  dirtyIds,
  reports,
  busyIds,
  batchActive,
  filter,
  search,
  onFilterChange,
  onSearchChange,
  onEnsureCatalogLoaded,
  setFocusLocale,
  onTranslateUnit,
  onEditUnit,
  onAcceptUnit,
  onTranslateAll,
}: Props) {
  // ── Auto-load every project catalog into the cache ──────────────────────
  //
  // Matrix mode needs every per-locale catalog loaded to render its cells.
  // The legacy single-catalog flow only opened the active one on click; we
  // trigger the rest here. Concurrent calls for the same path are safe —
  // App's setOpenCatalogs runs a final-write-wins update and the cache key
  // is the absolute path.

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      for (const ref of summary.catalogs) {
        if (cancelled) break;
        if (openCatalogs.has(ref.absolute_path)) continue;
        try {
          await onEnsureCatalogLoaded(ref.absolute_path);
        } catch {
          // Best effort. The cell will render the loading placeholder.
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [summary.catalogs, openCatalogs, onEnsureCatalogLoaded]);

  // ── Local view state (not persisted) ────────────────────────────────────

  const [hideFinished, setHideFinished] = useState(false);
  const [focusedCell, setFocusedCell] = useState<{
    rowKey: string;
    locale: string;
  } | null>(null);

  // ── Derived matrix + filtered rows ──────────────────────────────────────

  const rows = useMemo(
    () => deriveMatrix(summary, openCatalogs),
    [summary, openCatalogs],
  );

  const filteredRows = useMemo(() => {
    const lower = search.trim().toLowerCase();
    return rows.filter(
      (row) =>
        rowMatchesFilter(row, filter, hideFinished) &&
        rowMatchesSearch(row, lower),
    );
  }, [rows, filter, hideFinished, search]);

  const counts = useMemo(() => countByFilter(rows), [rows]);
  const localeStats = useMemo(
    () => aggregateLocaleStats(summary, openCatalogs),
    [summary, openCatalogs],
  );

  // Open count = the total rows visible under "All open". Used in the header
  // subtitle. We define "open" as any cell not finished or vanished — which
  // is exactly the "All open" filter's intent.
  const openCount = useMemo(
    () =>
      rows.filter((row) =>
        Array.from(row.byLocale.values()).some(
          (entry) =>
            entry.unit.state === "untranslated" ||
            entry.unit.state === "proposed",
        ),
      ).length,
    [rows],
  );

  // ── Keyboard navigation ────────────────────────────────────────────────

  const visibleRows = filteredRows;
  const containerRef = useRef<HTMLDivElement | null>(null);

  // Track keyboard focus on a row when no cell is explicitly focused.
  const [focusedRowKey, setFocusedRowKey] = useState<string | null>(null);

  useEffect(() => {
    if (visibleRows.length === 0) {
      setFocusedRowKey(null);
      setFocusedCell(null);
      return;
    }
    // If the previously focused row vanished from the filter, snap to the
    // first visible row.
    if (
      focusedRowKey == null ||
      !visibleRows.some((r) => r.rowKey === focusedRowKey)
    ) {
      const first = visibleRows[0];
      if (first) setFocusedRowKey(first.rowKey);
    }
  }, [visibleRows, focusedRowKey]);

  const focusedRow = useMemo(
    () => visibleRows.find((r) => r.rowKey === focusedRowKey) ?? null,
    [visibleRows, focusedRowKey],
  );

  // The cell currently expressing focus, used by the Inspector + ⌘↵ accept.
  const focusedEntry = useMemo(() => {
    if (!focusedCell || !focusedRow) return null;
    if (focusedCell.rowKey !== focusedRow.rowKey) return null;
    return focusedRow.byLocale.get(focusedCell.locale) ?? null;
  }, [focusedCell, focusedRow]);

  // Window-level keydown listener — matrix view is the active surface when
  // it's rendered, so it captures arrow keys / Tab / ⌘↵ globally. Native
  // textarea editing for the focused cell is preserved (the listener bails
  // when the event target is inside a textarea / input, except for ⌘↵).
  useEffect(() => {
    function handler(e: KeyboardEvent) {
      // Skip when the matrix container is not in the DOM (we still depend
      // on the effect being scoped to the View's mount lifecycle).
      const root = containerRef.current;
      if (!root) return;

      const target = e.target as HTMLElement | null;
      const insideEditor =
        target && (target.tagName === "TEXTAREA" || target.tagName === "INPUT");

      // Allow ⌘↵ everywhere (including inside a textarea); other keys defer
      // to native editing semantics when the user is typing.
      const isAcceptCombo = (e.metaKey || e.ctrlKey) && e.key === "Enter";
      if (insideEditor && !isAcceptCombo) return;

      // Only react to events whose path stays inside the matrix root.
      // This stops the listener from hijacking keys when the user is in
      // another panel (e.g. the inspector). EventTarget.composedPath() is
      // not in the lib.dom types we use, so we walk parents manually.
      let node: HTMLElement | null = target;
      let inside = false;
      while (node) {
        if (node === root) {
          inside = true;
          break;
        }
        node = node.parentElement;
      }
      if (!inside && !isAcceptCombo) return;

      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        if (visibleRows.length === 0) return;
        e.preventDefault();
        const idx = focusedRowKey
          ? visibleRows.findIndex((r) => r.rowKey === focusedRowKey)
          : -1;
        const next =
          e.key === "ArrowDown"
            ? Math.min(visibleRows.length - 1, idx + 1)
            : Math.max(0, idx - 1);
        const targetRow = visibleRows[next];
        if (targetRow) {
          setFocusedRowKey(targetRow.rowKey);
          setFocusedCell(null);
        }
        return;
      }

      if (e.key === "Tab" && focusedRow) {
        // Cycle between cells of the focused card. Default browser tab
        // would walk all interactive elements (one per cell); we override
        // to mean "next cell" so the inspector follows.
        const locales = summary.locales.filter((l) =>
          focusedRow.byLocale.has(l),
        );
        if (locales.length === 0) return;
        e.preventDefault();
        const curIdx = focusedCell ? locales.indexOf(focusedCell.locale) : -1;
        const nextIdx = e.shiftKey
          ? curIdx <= 0
            ? locales.length - 1
            : curIdx - 1
          : (curIdx + 1) % locales.length;
        const nextLocale = locales[nextIdx];
        if (nextLocale) {
          setFocusedCell({ rowKey: focusedRow.rowKey, locale: nextLocale });
        }
        return;
      }

      if (isAcceptCombo) {
        if (!focusedEntry) return;
        const flags = focusedEntry.unit.flags;
        const hard =
          Array.isArray(flags) &&
          flags.some((flag) => severityOf(flag) === "hard");
        if (hard) return;
        e.preventDefault();
        onAcceptUnit(focusedEntry.catalogPath, focusedEntry.unit);
      }
    }
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [
    visibleRows,
    focusedRowKey,
    focusedRow,
    focusedCell,
    focusedEntry,
    summary.locales,
    onAcceptUnit,
  ]);

  // ── Run model on selected ──────────────────────────────────────────────
  //
  // The existing batch IPC takes a single (catalog, scope). Matrix mode's
  // "Run model on selected" is locale-fanout: one batch per locale, queued
  // sequentially by the existing single-slot stuck-guard in App. For PR 4
  // we dispatch immediately for every locale that has at least one
  // matching unit; the user gets one toast per batch.

  const onRunOnSelected = useCallback(() => {
    if (batchActive) return;
    const scope: BatchScope = "untranslated";
    for (const ref of summary.catalogs) {
      const cached = openCatalogs.get(ref.absolute_path);
      if (!cached) continue;
      const hasTarget = cached.units.some((u) => u.state === "untranslated");
      if (!hasTarget) continue;
      onTranslateAll(ref.absolute_path, scope);
    }
  }, [batchActive, summary.catalogs, openCatalogs, onTranslateAll]);

  // The Inspector accepts an `onAccept(unitId)` that is bound to App's
  // active catalog — but Matrix mode is multi-catalog. We curry the right
  // catalogPath at the call site.
  const onInspectorAccept = useCallback(
    async (unitId: UnitId) => {
      if (!focusedEntry || focusedEntry.unit.id !== unitId) return;
      onAcceptUnit(focusedEntry.catalogPath, focusedEntry.unit);
    },
    [focusedEntry, onAcceptUnit],
  );

  // ── Render ─────────────────────────────────────────────────────────────

  const headerTitle = labelOf(filter);
  const headerSubtitle = `${rows.length} unit${rows.length === 1 ? "" : "s"} · ${openCount} open`;

  return (
    <div
      ref={containerRef}
      className="flex-1 flex overflow-hidden min-h-0 bg-bg-base"
    >
      {/* Left rail */}
      <aside
        className={cn(
          "shrink-0 flex flex-col overflow-hidden",
          "bg-bg-surface border-r border-border-subtle",
        )}
        style={{ width: 232 }}
      >
        <div className="flex flex-col gap-1 px-3 pt-4 pb-2">
          <div className="px-1 pb-1.5">
            <Eyebrow>Inbox</Eyebrow>
          </div>
          {SAVED_VIEWS.map((view) => (
            <SavedViewButton
              key={view.id}
              view={view}
              active={filter === view.id}
              count={counts[view.id] ?? 0}
              onSelect={() => onFilterChange(view.id)}
            />
          ))}
        </div>
        <div className="flex flex-col gap-1 px-3 py-2 border-t border-border-subtle">
          <div className="px-1 pb-1.5">
            <Eyebrow>By locale</Eyebrow>
          </div>
          {localeStats.map((stat) => (
            <LocaleRowButton
              key={stat.locale}
              stat={stat}
              onSelect={() => setFocusLocale(stat.locale)}
            />
          ))}
        </div>
        <div className="mt-auto px-4 py-3 border-t border-border-subtle">
          <div className="pb-1.5">
            <Eyebrow>Backend</Eyebrow>
          </div>
          <div className="flex items-center gap-2">
            <span
              className="inline-block w-[7px] h-[7px] rounded-pill bg-state-finished"
              aria-hidden="true"
            />
            <span className="font-mono text-[11px] text-fg-secondary truncate">
              {summary.backend
                ? `${summary.backend.kind}${
                    summary.backend.model ? ` · ${summary.backend.model}` : ""
                  }`
                : "no backend configured"}
            </span>
          </div>
        </div>
      </aside>

      {/* Main column */}
      <div className="flex-1 flex flex-col overflow-hidden min-h-0">
        {/* Header strip */}
        <header
          className={cn(
            "shrink-0 px-6 pt-4 pb-3.5",
            "border-b border-border-subtle bg-bg-base",
          )}
        >
          <div className="flex items-baseline gap-3 mb-2">
            <div className="flex flex-col gap-1 flex-1 min-w-0">
              <Eyebrow>{headerTitle}</Eyebrow>
              <div className="flex items-baseline gap-2 flex-wrap">
                <h1 className="m-0 text-[18px] font-semibold text-fg-primary tracking-tight">
                  {headerTitle}
                </h1>
                <span className="font-mono text-xs text-fg-tertiary">
                  {headerSubtitle}
                </span>
              </div>
            </div>
            <label
              className={cn(
                "flex items-center gap-2 text-xs text-fg-secondary cursor-pointer",
                "select-none",
              )}
            >
              <input
                type="checkbox"
                checked={hideFinished}
                onChange={(e) => setHideFinished(e.target.checked)}
                className="accent-accent"
              />
              <span>Hide finished</span>
            </label>
            <button
              type="button"
              onClick={onRunOnSelected}
              disabled={batchActive || openCount === 0}
              className={cn(
                "inline-flex items-center gap-2 h-7 px-3 rounded-md text-xs font-medium",
                "transition-colors duration-100",
                "text-accent-fg bg-accent",
                "enabled:hover:bg-accent-hover enabled:active:bg-accent-active",
                "disabled:bg-accent-subtle disabled:text-fg-disabled disabled:cursor-not-allowed",
              )}
              title={
                batchActive
                  ? "A batch is already running"
                  : openCount === 0
                    ? "Nothing to translate"
                    : "Translate untranslated across all locales"
              }
            >
              Run model on selected
            </button>
          </div>
          <input
            type="search"
            placeholder="Filter by id or source…"
            value={search}
            onChange={(e) => onSearchChange(e.target.value)}
            spellCheck={false}
            aria-label="Filter rows"
            className={cn(
              "w-full h-8 px-3 rounded-md border bg-bg-input",
              "border-border-subtle text-sm text-fg-primary",
              "placeholder:text-fg-tertiary",
              "focus:border-accent focus:outline-none focus-visible:outline-none",
            )}
          />
        </header>

        {/* Card list + inspector */}
        <div className="flex-1 flex overflow-hidden min-h-0">
          <div className="flex-1 overflow-y-auto px-6 py-4 flex flex-col gap-2.5 min-w-0">
            {visibleRows.length === 0 ? (
              <div className="flex-1 flex flex-col items-center justify-center text-sm text-fg-tertiary gap-2 py-12">
                <p>No units match this view.</p>
                {filter !== "all" && filter !== "all-open" && (
                  <button
                    type="button"
                    onClick={() => onFilterChange("all-open")}
                    className="text-xs text-accent underline-offset-2 hover:underline"
                  >
                    Clear filter
                  </button>
                )}
              </div>
            ) : (
              visibleRows.map((row) => (
                <MatrixCard
                  key={row.rowKey}
                  row={row}
                  locales={summary.locales}
                  hideFinished={hideFinished}
                  dirtyIds={dirtyIds}
                  busyIds={busyIds}
                  focused={focusedRowKey === row.rowKey}
                  focusedCell={
                    focusedRowKey === row.rowKey ? focusedCell : null
                  }
                  onFocusCell={(locale) => {
                    setFocusedRowKey(row.rowKey);
                    setFocusedCell({ rowKey: row.rowKey, locale });
                  }}
                  onTranslate={onTranslateUnit}
                  onEdit={onEditUnit}
                  onAccept={onAcceptUnit}
                  onReopen={onEditUnit}
                />
              ))
            )}
          </div>
          {focusedEntry && (
            <Inspector
              unit={focusedEntry.unit}
              report={reports[focusedEntry.unit.id] ?? null}
              activeCatalogPath={focusedEntry.catalogPath}
              busyIds={busyIds}
              onAccept={onInspectorAccept}
            />
          )}
        </div>
      </div>
    </div>
  );
}

// ── Card ────────────────────────────────────────────────────────────────

interface MatrixCardProps {
  row: MatrixRow;
  locales: string[];
  hideFinished: boolean;
  dirtyIds: Set<UnitId>;
  busyIds: Set<UnitId>;
  focused: boolean;
  focusedCell: { rowKey: string; locale: string } | null;
  onFocusCell: (locale: string) => void;
  onTranslate: (catalogPath: string, unit: Unit) => void;
  onEdit: (catalogPath: string, unit: Unit, edit: TargetEdit) => void;
  onAccept: (catalogPath: string, unit: Unit) => void;
  /** Finished → Proposed transition. Re-uses onEdit: writing the same text
   *  back via update_unit_target reopens the unit (see UnitEditor's
   *  existing contract — editing a finished unit drops state to Proposed). */
  onReopen: (catalogPath: string, unit: Unit, edit: TargetEdit) => void;
}

function MatrixCard({
  row,
  locales,
  hideFinished,
  dirtyIds,
  busyIds,
  focused,
  focusedCell,
  onFocusCell,
  onTranslate,
  onEdit,
  onAccept,
  onReopen,
}: MatrixCardProps) {
  const hiddenFinishedLocales = hideFinished
    ? locales.filter((l) => {
        const entry = row.byLocale.get(l);
        return entry?.unit.state === "finished";
      })
    : [];

  const visibleLocales = hideFinished
    ? locales.filter((l) => !hiddenFinishedLocales.includes(l))
    : locales;

  return (
    <article
      className={cn(
        "rounded-lg overflow-hidden border bg-bg-surface",
        focused ? "border-border-default" : "border-border-subtle",
      )}
      aria-label={`Unit ${row.unitId}`}
    >
      {/* Card head */}
      <div className="flex items-start gap-3 px-4 py-2.5 border-b border-border-subtle">
        <div className="flex-1 min-w-0 flex flex-col gap-1">
          <div className="flex items-center gap-2 flex-wrap">
            <span
              className="font-mono text-[11.5px] text-fg-secondary"
              title={row.unitId}
            >
              {row.unitId}
            </span>
            {row.isPlural && (
              <span className="font-mono text-[10.5px] text-fg-tertiary px-1.5 py-px rounded-sm bg-bg-elevated border border-border-subtle">
                plural
              </span>
            )}
            {row.placeholderCount > 0 && (
              <span className="font-mono text-[10.5px] text-fg-tertiary px-1.5 py-px rounded-sm bg-bg-elevated border border-border-subtle">
                {"{}"}×{row.placeholderCount}
              </span>
            )}
          </div>
          <p className="m-0 font-mono text-sm text-fg-primary leading-snug break-words">
            {row.source}
          </p>
        </div>
        <LocaleSummaryStrip row={row} locales={locales} />
      </div>

      {/* Cells grid */}
      <div
        className="grid"
        style={{
          gridTemplateColumns: "repeat(auto-fit, minmax(240px, 1fr))",
        }}
      >
        {visibleLocales.map((locale) => {
          const entry = row.byLocale.get(locale) ?? null;
          const isFocused = focused && focusedCell?.locale === locale;
          const cellBusy = entry ? busyIds.has(entry.unit.id) : false;
          const cellEdited = entry ? dirtyIds.has(entry.unit.id) : false;
          return (
            <MatrixCell
              key={locale}
              locale={locale}
              unit={entry?.unit ?? null}
              catalogPath={entry?.catalogPath ?? null}
              busy={cellBusy}
              focused={isFocused}
              edited={cellEdited}
              onFocusCell={() => onFocusCell(locale)}
              onTranslate={onTranslate}
              onEdit={onEdit}
              onAccept={onAccept}
              onReopen={(p, u) => {
                // Reopening = sending the current target text back through
                // the edit IPC, which downgrades Finished → Proposed per
                // the existing state machine.
                const cur =
                  u.target.kind === "singular"
                    ? u.target.text
                    : (u.target.forms[0] ?? null);
                const edit: TargetEdit =
                  u.target.kind === "plural"
                    ? { kind: "plural", form_index: 0, text: cur }
                    : { kind: "singular", text: cur };
                onReopen(p, u, edit);
              }}
            />
          );
        })}
      </div>

      {/* Hidden-finished footer */}
      {hiddenFinishedLocales.length > 0 && (
        <div className="flex items-center gap-2 flex-wrap px-4 py-2 bg-bg-base">
          <span className="text-[11px] text-fg-tertiary inline-flex items-center gap-1">
            Finished:
          </span>
          {hiddenFinishedLocales.map((l) => (
            <LocaleTag key={l} locale={l} tone="default" />
          ))}
        </div>
      )}
    </article>
  );
}

function LocaleSummaryStrip({
  row,
  locales,
}: {
  row: MatrixRow;
  locales: string[];
}) {
  return (
    <div
      className="flex items-center gap-1 px-2 py-1 rounded-md border border-border-subtle bg-bg-base"
      role="img"
      aria-label="Locale coverage"
    >
      {locales.map((l) => {
        const entry = row.byLocale.get(l);
        const state = entry?.unit.state ?? "untranslated";
        return (
          <span
            key={l}
            title={`${l}: ${state}`}
            className="inline-block w-1.5 h-1.5 rounded-pill"
            style={{ background: dotColor(state) }}
          />
        );
      })}
    </div>
  );
}

function dotColor(state: Unit["state"]): string {
  switch (state) {
    case "finished":
      return "var(--color-state-finished)";
    case "proposed":
      return "var(--color-state-proposed)";
    case "untranslated":
      return "var(--color-state-untranslated)";
    case "vanished":
    case "obsolete":
      return "var(--color-state-vanished)";
  }
}

// ── Left-rail sub-components ─────────────────────────────────────────────

function SavedViewButton({
  view,
  active,
  count,
  onSelect,
}: {
  view: SavedView;
  active: boolean;
  count: number;
  onSelect: () => void;
}) {
  const tintClass =
    view.tint === "soft"
      ? "text-severity-soft"
      : view.tint === "hard"
        ? "text-severity-hard"
        : view.tint === "proposed"
          ? "text-state-proposed"
          : "text-fg-tertiary";

  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={active}
      className={cn(
        "w-full flex items-center gap-2 h-7 px-2.5 rounded-md text-xs",
        "transition-colors duration-100 ease-out",
        "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
        active
          ? "text-fg-primary bg-accent-subtle font-medium"
          : "text-fg-secondary hover:bg-bg-hover hover:text-fg-primary",
      )}
    >
      <span className="flex-1 text-left truncate">{view.label}</span>
      <span className={cn("font-mono tabular-nums text-[11px]", tintClass)}>
        {count}
      </span>
    </button>
  );
}

function LocaleRowButton({
  stat,
  onSelect,
}: {
  stat: LocaleStat;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      title={`Focus on ${stat.locale}`}
      className={cn(
        "w-full flex flex-col gap-1.5 px-2.5 py-1.5 rounded-md",
        "transition-colors duration-100 ease-out",
        "hover:bg-bg-hover focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
      )}
    >
      <div className="flex items-center gap-2 w-full">
        <LocaleTag locale={stat.locale} tone="default" />
        <span className="flex-1" />
        <span className="font-mono text-[10.5px] text-fg-tertiary tabular-nums">
          {stat.total > 0
            ? `${Math.round((stat.finished / stat.total) * 100)}%`
            : "—"}
        </span>
      </div>
      <ProgressBar
        total={stat.total}
        finished={stat.finished}
        proposed={stat.proposed}
        height={3}
      />
    </button>
  );
}

function labelOf(f: MatrixFilter): string {
  switch (f) {
    case "all":
    case "all-open":
      return "All open";
    case "untranslated":
      return "Untranslated";
    case "proposed":
    case "proposed-by-model":
      return "Proposed by model";
    case "finished":
      return "Finished";
    case "needs-review":
      return "Needs review";
    case "has-hard-flag":
      return "Has hard flag";
  }
}
