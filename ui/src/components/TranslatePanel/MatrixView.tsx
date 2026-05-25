// MatrixView — the matrix-mode Translate surface. One card per source
// unit; cells fan out across the project locales. Replaces the previous
// 3-pane (CatalogList + UnitEditor + Inspector) inline layout — the 3-pane
// components live on as Focus-mode fallback in TranslatePanel.
//
// Data shape: we auto-load every per-locale catalog into the openCatalogs
// cache on mount, then pivot units by `unit.id` so one card stitches
// together its per-locale cells. Cross-catalog grouping is keyed purely on
// the unit id — see deriveMatrix below for why that's the only honest
// model.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { cn } from "../../lib/cn";
import type {
  CatalogResponse,
  GateReport,
  ProjectSummary,
  TargetEdit,
  Unit,
  UnitId,
} from "../../lib/types";
import { severityOf } from "../../lib/types";
import {
  Eyebrow,
  LocaleTag,
  SegmentBar,
  SparklesIcon,
  Spinner,
} from "../primitives";
import { Inspector } from "./Inspector/Inspector";
import { MatrixCell } from "./MatrixCell";
import type { StatusFilterState } from "./StatusFilter";
import { unitMatchesFilter } from "./StatusFilter";

// ── Inspector collapse persistence ────────────────────────────────────────
//
// Shared with FocusView via the same localStorage key so the user's
// preference is consistent across modes. View-only preference; does not
// belong in the project manifest.

const INSPECTOR_COLLAPSED_KEY = "inspector-collapsed";

function readInspectorCollapsed(): boolean {
  try {
    return localStorage.getItem(INSPECTOR_COLLAPSED_KEY) === "1";
  } catch {
    return false;
  }
}

function writeInspectorCollapsed(value: boolean): void {
  try {
    localStorage.setItem(INSPECTOR_COLLAPSED_KEY, value ? "1" : "0");
  } catch {
    // localStorage unavailable; skip persistence silently.
  }
}

interface Props {
  summary: ProjectSummary;
  openCatalogs: Map<string, CatalogResponse>;
  dirtyIds: Set<UnitId>;
  reports: Record<UnitId, GateReport>;
  busyIds: Set<UnitId>;
  batchActive: boolean;
  search: string;
  /** Status filter lifted from TranslatePanel sub-header. Multi-select set
   *  over the three UI-editable UnitStates. Empty set means "show all". */
  statusFilter: StatusFilterState;
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
  /** Open the "Translate untranslated" modal pre-filled with all locales. */
  onOpenTranslateModal: () => void;
}

// ── Matrix-row data model ────────────────────────────────────────────────
//
// One row keys on `unit.id` across every loaded catalog in the project.
// `byLocale` maps a project locale to its Unit when the locale's catalog
// is loaded; missing entries render as the "loading…" cell.
//
// Earlier iterations stripped a `_<locale>` filename suffix to compute a
// per-module stem; that broke immediately when filenames used short codes
// (`app_de.ts`) while locale ids were full (`de_DE`). Pivoting purely by
// unit id is correct for the typical single-module project and right-by-
// construction for multi-module ones too (different modules don't share
// unit ids).

interface MatrixRow {
  rowKey: string;
  unitId: UnitId;
  /** Source string — taken from the first loaded locale (all per-locale
   *  catalogs share the same source for a given unit). */
  source: string;
  isPlural: boolean;
  placeholderCount: number;
  /** Project locales → unit + catalog path. */
  byLocale: Map<string, { unit: Unit; catalogPath: string }>;
}

export function deriveMatrix(
  summary: ProjectSummary,
  openCatalogs: Map<string, CatalogResponse>,
): MatrixRow[] {
  const rows = new Map<UnitId, MatrixRow>();

  for (const ref of summary.catalogs) {
    const cached = openCatalogs.get(ref.absolute_path);
    if (!cached) continue;
    for (const unit of cached.units) {
      const existing = rows.get(unit.id);
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
      rows.set(unit.id, {
        rowKey: unit.id,
        unitId: unit.id,
        source: unit.source,
        isPlural: unit.plural_arity != null,
        placeholderCount: placeholders,
        byLocale: new Map([
          [ref.locale, { unit, catalogPath: ref.absolute_path }],
        ]),
      });
    }
  }

  return Array.from(rows.values()).sort((a, b) =>
    a.unitId.localeCompare(b.unitId),
  );
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

// ── Header label derivation ─────────────────────────────────────────────

function labelOfStatusFilter(filter: StatusFilterState): string {
  if (filter.size === 0) return "All units";
  if (filter.size === 1) {
    const [only] = filter;
    if (only === "untranslated") return "Untranslated";
    if (only === "proposed") return "Proposed";
    if (only === "finished") return "Finished";
  }
  return "Filtered units";
}

// ── Component ─────────────────────────────────────────────────────────────

export function MatrixView({
  summary,
  openCatalogs,
  dirtyIds,
  reports,
  busyIds,
  batchActive,
  search,
  statusFilter,
  onSearchChange,
  onEnsureCatalogLoaded,
  setFocusLocale,
  onTranslateUnit,
  onEditUnit,
  onAcceptUnit,
  onOpenTranslateModal,
}: Props) {
  // ── Auto-load every project catalog into the cache ──────────────────────
  //
  // Matrix mode needs every per-locale catalog loaded to render its cells.
  // We fire one parallel fan-out per project on mount (and whenever the
  // catalog list itself changes — e.g. after a manifest mutation). The
  // dependency list intentionally omits `openCatalogs` and the callback
  // identity: the callback closes over `openCatalogs` in App and would
  // make this effect re-run on every cache update, which can stall the
  // sequential variant and is wasted work here. ensureCatalogLoaded is
  // already a no-op for cached paths, and Promise.allSettled means a
  // single failure does not block the other locales.

  // biome-ignore lint/correctness/useExhaustiveDependencies: see comment above
  useEffect(() => {
    const toLoad = summary.catalogs.filter(
      (ref) => !openCatalogs.has(ref.absolute_path),
    );
    if (toLoad.length === 0) return;
    void Promise.allSettled(
      toLoad.map((ref) => onEnsureCatalogLoaded(ref.absolute_path)),
    );
  }, [summary.catalogs]);

  // ── Local view state (not persisted) ────────────────────────────────────

  const [focusedCell, setFocusedCell] = useState<{
    rowKey: string;
    locale: string;
  } | null>(null);

  // Inspector collapse state — persisted to localStorage so the user's
  // choice survives sub-tab switches and reloads (view preference only,
  // does not belong in the project manifest).
  const [inspectorCollapsed, setInspectorCollapsed] = useState<boolean>(() =>
    readInspectorCollapsed(),
  );
  const toggleInspector = useCallback(() => {
    setInspectorCollapsed((prev) => {
      const next = !prev;
      writeInspectorCollapsed(next);
      return next;
    });
  }, []);

  // ── Derived matrix + filtered rows ──────────────────────────────────────

  const rows = useMemo(
    () => deriveMatrix(summary, openCatalogs),
    [summary, openCatalogs],
  );

  const filteredRows = useMemo(() => {
    const lower = search.trim().toLowerCase();
    return rows.filter((row) => {
      if (!rowMatchesSearch(row, lower)) return false;
      // Status filter from TranslatePanel sub-header: pass if ANY cell in
      // the row satisfies the unit-level predicate. Empty filter passes
      // everything via the predicate's own short-circuit.
      if (statusFilter.size > 0) {
        const cells = Array.from(row.byLocale.values());
        if (!cells.some((entry) => unitMatchesFilter(entry.unit, statusFilter)))
          return false;
      }
      return true;
    });
  }, [rows, search, statusFilter]);

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

  // Scroll the focused row into view whenever it changes via keyboard
  // navigation. `block: "nearest"` avoids jumping to top/bottom when the row
  // is already visible, matching the CatalogList pattern.
  useEffect(() => {
    if (!focusedRowKey) return;
    const root = containerRef.current;
    if (!root) return;
    const el = root.querySelector<HTMLElement>(
      `[data-row-key="${CSS.escape(focusedRowKey)}"]`,
    );
    el?.scrollIntoView({ block: "nearest" });
  }, [focusedRowKey]);

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

      // j / J → next row; k / K → previous row. Bound alongside Arrow keys
      // so vim-style and arrow-key users get the same behavior.
      const isDownKey = e.key === "ArrowDown" || e.key === "j" || e.key === "J";
      const isUpKey = e.key === "ArrowUp" || e.key === "k" || e.key === "K";
      if (isDownKey || isUpKey) {
        if (visibleRows.length === 0) return;
        e.preventDefault();
        const idx = focusedRowKey
          ? visibleRows.findIndex((r) => r.rowKey === focusedRowKey)
          : -1;
        const next = isDownKey
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

  // ── Translate untranslated — open the scope modal ─────────────────────
  //
  // Previously dispatched directly to onTranslateAll per locale. Now opens
  // RunOnScopeModal so the user sees which pairs will run and which are
  // skipped before work begins.

  const onRunOnSelected = useCallback(() => {
    if (batchActive) return;
    onOpenTranslateModal();
  }, [batchActive, onOpenTranslateModal]);

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

  const headerTitle = labelOfStatusFilter(statusFilter);
  const headerSubtitle = `${rows.length} unit${rows.length === 1 ? "" : "s"} · ${openCount} open`;

  // Project-wide loading state — while the eager-load is still resolving the
  // first few catalogs, every cell hits the null-unit branch (loading…). A
  // page of identical skeletons is worse signal than one clear "loading"
  // indicator, so we hide the card list until every catalog is cached.
  const totalCatalogs = summary.catalogs.length;
  const loadedCatalogs = useMemo(
    () =>
      summary.catalogs.reduce(
        (acc, ref) => acc + (openCatalogs.has(ref.absolute_path) ? 1 : 0),
        0,
      ),
    [summary.catalogs, openCatalogs],
  );
  const allCatalogsLoaded =
    totalCatalogs === 0 || loadedCatalogs === totalCatalogs;

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
        <div className="flex flex-col gap-1 px-3 pt-4 py-2">
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
            <button
              type="button"
              onClick={onRunOnSelected}
              disabled={batchActive || openCount === 0}
              className={cn(
                "inline-flex items-center gap-2 h-7 px-3 rounded-md text-xs font-medium",
                "transition-colors duration-100",
                "text-accent-fg bg-accent",
                batchActive ? "cursor-wait" : null,
                batchActive
                  ? null
                  : "enabled:hover:bg-accent-hover enabled:active:bg-accent-active",
                batchActive
                  ? null
                  : "disabled:bg-accent-subtle disabled:text-fg-disabled disabled:cursor-not-allowed",
              )}
              title={
                batchActive
                  ? "A batch is already running"
                  : openCount === 0
                    ? "Nothing to translate"
                    : "Translate untranslated across all locales"
              }
            >
              {batchActive ? <Spinner size={12} /> : <SparklesIcon size={12} />}
              Translate untranslated
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
            {!allCatalogsLoaded ? (
              <div
                role="status"
                aria-live="polite"
                className="flex-1 flex flex-col items-center justify-center gap-3 py-12 text-fg-tertiary"
              >
                <span
                  className="inline-block w-3 h-3 rounded-pill bg-accent animate-pulse"
                  aria-hidden="true"
                />
                <p className="text-sm font-medium text-fg-secondary">
                  Loading {totalCatalogs}{" "}
                  {totalCatalogs === 1 ? "catalog" : "catalogs"}…
                </p>
                <p className="text-xs font-mono">
                  {loadedCatalogs}/{totalCatalogs} ready
                </p>
              </div>
            ) : visibleRows.length === 0 ? (
              <div className="flex-1 flex flex-col items-center justify-center text-sm text-fg-tertiary gap-2 py-12">
                <p>No units match this view.</p>
              </div>
            ) : (
              visibleRows.map((row) => (
                <MatrixCard
                  key={row.rowKey}
                  row={row}
                  locales={summary.locales}
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
              collapsed={inspectorCollapsed}
              onToggleCollapsed={toggleInspector}
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
  const visibleLocales = locales;

  return (
    <article
      data-row-key={row.rowKey}
      className={cn(
        "shrink-0 rounded-lg overflow-hidden border bg-bg-surface",
        focused ? "border-border-default" : "border-border-subtle",
      )}
      aria-label={`Unit ${row.unitId}`}
    >
      {/* Card head */}
      <div className="flex items-start gap-3 px-4 py-2.5 border-b border-border-subtle">
        <div className="flex-1 min-w-0 flex flex-col gap-1">
          <div className="flex items-center gap-2 min-w-0">
            <span
              className="flex-1 min-w-0 truncate font-mono text-[11.5px] text-fg-secondary"
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
          <p className="m-0 max-h-32 overflow-y-auto pr-1 font-mono text-sm text-fg-primary leading-snug break-words">
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
      aria-label={`Focus on ${stat.locale}`}
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
      <SegmentBar
        total={stat.total}
        finished={stat.finished}
        proposed={stat.proposed}
        height={3}
      />
    </button>
  );
}
