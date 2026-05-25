// TranslatePanel — Matrix / Focus dispatcher.
//
// Truly binary:
//   focusLocale === null  → MatrixView
//   focusLocale !== null  → FocusView (PR 5)
//
// The shared sub-header (Single ↔ Matrix toggle + StatusFilter) lives here
// so both modes always expose the same controls, and the status-filter
// selection persists when the user switches modes.
//
// RunOnScopeModal is also hosted here so it has access to startBatchForPair
// from App.tsx without threading the prop through MatrixView/FocusView.

import type { Ref } from "react";
import { useCallback, useState } from "react";
import { cn } from "../../lib/cn";
import type {
  BatchScope,
  CatalogResponse,
  GateReport,
  ProjectSummary,
  TargetEdit,
  Unit,
  UnitId,
} from "../../lib/types";
import { FocusView } from "./FocusView";
import type { MatrixFilter } from "./MatrixView";
import { MatrixView } from "./MatrixView";
import type { ScopePair, StartPairFn } from "./RunOnScopeModal";
import { RunOnScopeModal } from "./RunOnScopeModal";
import type { StatusFilterId } from "./StatusFilter";
import { StatusFilter } from "./StatusFilter";
import type { UnitEditorHandle } from "./UnitEditor/UnitEditor";

interface Props {
  // ── Project + catalog cache ─────────────────────────────────────────────
  summary: ProjectSummary;
  openCatalogs: Map<string, CatalogResponse>;

  // ── Active-catalog (legacy fields — kept for future reference) ──────────
  activeCatalogPath: string | null;
  catalog: CatalogResponse | null;
  selectedId: UnitId | null;
  filter: MatrixFilter;
  search: string;
  dirtyIds: Set<UnitId>;
  reports: Record<UnitId, GateReport>;
  busyIds: Set<UnitId>;
  batchActive: boolean;
  error: string | null;
  editorRef: Ref<UnitEditorHandle | null>;

  // ── Focus-locale dispatch ────────────────────────────────────────────────
  focusLocale: string | null;
  setFocusLocale: (locale: string | null) => void;

  // ── Per-unit IPC dispatchers (owned by App) ─────────────────────────────
  onSelect: (id: UnitId) => void;
  onFilterChange: (filter: MatrixFilter) => void;
  onSearchChange: (search: string) => void;
  onEdit: (id: UnitId, edit: TargetEdit) => Promise<void>;
  onTranslate: (id: UnitId) => void;
  onAccept: (id: UnitId) => Promise<void>;

  // ── Matrix-specific IPC dispatchers (multi-catalog) ─────────────────────
  onEnsureCatalogLoaded: (absPath: string) => Promise<void>;
  onTranslateUnitFor: (catalogPath: string, unit: Unit) => void;
  onEditUnitFor: (catalogPath: string, unit: Unit, edit: TargetEdit) => void;
  onAcceptUnitFor: (catalogPath: string, unit: Unit) => void;

  // ── Batch ────────────────────────────────────────────────────────────────
  onTranslateAll: (catalogPath: string, scope: BatchScope) => void;
  /** Per-pair batch starter for RunOnScopeModal. Provided by App.tsx. */
  startBatchForPair: StartPairFn;
}

export function TranslatePanel({
  summary,
  openCatalogs,
  dirtyIds,
  reports,
  busyIds,
  batchActive,
  filter,
  search,
  focusLocale,
  setFocusLocale,
  onFilterChange,
  onSearchChange,
  onEnsureCatalogLoaded,
  onTranslateUnitFor,
  onEditUnitFor,
  onAcceptUnitFor,
  onTranslateAll: _onTranslateAll,
  startBatchForPair,
}: Props) {
  // ── Shared sub-header state ─────────────────────────────────────────────
  //
  // statusFilter persists across Single ↔ Matrix switches so the user's
  // selection is not lost when toggling modes.

  const [statusFilter, setStatusFilter] = useState<StatusFilterId>("all");

  // ── RunOnScopeModal state ───────────────────────────────────────────────
  //
  // The modal is hosted here (not in MatrixView/FocusView) so it has access
  // to startBatchForPair without threading props two levels deeper.

  const [modalOpen, setModalOpen] = useState(false);
  const [modalPairs, setModalPairs] = useState<ScopePair[]>([]);

  // Build the pair list for a given locale scope and open the modal.
  // `localeFilter` = null means all locales (Matrix mode); a locale string
  // limits the scope to that one locale (Focus mode).
  const openTranslateModal = useCallback(
    (localeFilter: string | null) => {
      const pairs: ScopePair[] = [];
      for (const ref of summary.catalogs) {
        if (localeFilter !== null && ref.locale !== localeFilter) continue;
        const catalogName =
          ref.absolute_path.replace(/\\/g, "/").split("/").pop() ??
          ref.absolute_path;
        const cached = openCatalogs.get(ref.absolute_path);
        const untranslatedCount = cached
          ? cached.units.filter((u) => u.state === "untranslated").length
          : 0;
        pairs.push({
          catalogPath: ref.absolute_path,
          catalogName,
          locale: ref.locale,
          untranslatedCount,
        });
      }
      setModalPairs(pairs);
      setModalOpen(true);
    },
    [summary.catalogs, openCatalogs],
  );

  // Resolve the first available locale for the "Single" button — used when
  // switching from Matrix mode (focusLocale === null) into Single mode.
  const firstLocale = summary.locales[0] ?? null;

  const isSingle = focusLocale !== null;

  // ── Shared sub-header ──────────────────────────────────────────────────

  const subHeader = (
    <div
      className={cn(
        "shrink-0 flex items-center gap-3 px-4 h-9",
        "bg-bg-surface border-b border-border-subtle",
      )}
    >
      {/* Single ↔ Matrix segmented toggle */}
      <div
        className="flex items-center rounded-md border border-border-default bg-bg-elevated"
        style={{ padding: 2, gap: 2 }}
      >
        {isSingle ? (
          <span
            className={cn(
              "inline-flex items-center gap-1.5 h-[22px] px-2.5 rounded-sm",
              "text-xs font-medium bg-bg-selected text-fg-primary",
            )}
            aria-current="true"
          >
            <ListIcon size={11} />
            Single
          </span>
        ) : (
          <button
            type="button"
            onClick={() => {
              if (firstLocale) setFocusLocale(firstLocale);
            }}
            disabled={!firstLocale}
            aria-label="Switch to single-locale view"
            className={cn(
              "inline-flex items-center gap-1.5 h-[22px] px-2.5 rounded-sm",
              "text-xs font-medium text-fg-secondary",
              "hover:bg-bg-hover hover:text-fg-primary transition-colors duration-100",
              "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
              "disabled:opacity-40 disabled:cursor-not-allowed",
            )}
          >
            <ListIcon size={11} />
            Single
          </button>
        )}
        {!isSingle ? (
          <span
            className={cn(
              "inline-flex items-center gap-1.5 h-[22px] px-2.5 rounded-sm",
              "text-xs font-medium bg-bg-selected text-fg-primary",
            )}
            aria-current="true"
          >
            <GridIcon size={11} />
            Matrix
          </span>
        ) : (
          <button
            type="button"
            onClick={() => setFocusLocale(null)}
            aria-label="Switch to matrix view"
            className={cn(
              "inline-flex items-center gap-1.5 h-[22px] px-2.5 rounded-sm",
              "text-xs font-medium text-fg-secondary",
              "hover:bg-bg-hover hover:text-fg-primary transition-colors duration-100",
              "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
            )}
          >
            <GridIcon size={11} />
            Matrix
          </button>
        )}
      </div>

      {/* Spacer */}
      <span className="flex-1" />

      {/* Status filter — always visible, selection persists across mode switches */}
      <StatusFilter value={statusFilter} onChange={setStatusFilter} />
    </div>
  );

  // ── Modal (renders above the panel in both modes) ──────────────────────
  const modal = (
    <RunOnScopeModal
      open={modalOpen}
      pairs={modalPairs}
      onClose={() => setModalOpen(false)}
      startPair={startBatchForPair}
    />
  );

  // ── Matrix mode (focusLocale === null) ──────────────────────────────────
  if (focusLocale === null) {
    return (
      <div className="flex-1 flex flex-col overflow-hidden min-h-0">
        {modal}
        {subHeader}
        <MatrixView
          summary={summary}
          openCatalogs={openCatalogs}
          dirtyIds={dirtyIds}
          reports={reports}
          busyIds={busyIds}
          batchActive={batchActive}
          filter={filter}
          search={search}
          statusFilter={statusFilter}
          onFilterChange={onFilterChange}
          onSearchChange={onSearchChange}
          onEnsureCatalogLoaded={onEnsureCatalogLoaded}
          setFocusLocale={setFocusLocale}
          onTranslateUnit={onTranslateUnitFor}
          onEditUnit={onEditUnitFor}
          onAcceptUnit={onAcceptUnitFor}
          onOpenTranslateModal={() => openTranslateModal(null)}
        />
      </div>
    );
  }

  // ── Focus mode (focusLocale !== null) ───────────────────────────────────
  return (
    <div className="flex-1 flex flex-col overflow-hidden min-h-0">
      {modal}
      {subHeader}
      <FocusView
        summary={summary}
        openCatalogs={openCatalogs}
        focusLocale={focusLocale}
        setFocusLocale={setFocusLocale}
        dirtyIds={dirtyIds}
        reports={reports}
        busyIds={busyIds}
        batchActive={batchActive}
        statusFilter={statusFilter}
        onStatusFilterChange={setStatusFilter}
        onEnsureCatalogLoaded={onEnsureCatalogLoaded}
        onTranslateUnit={onTranslateUnitFor}
        onEditUnit={onEditUnitFor}
        onAcceptUnit={onAcceptUnitFor}
        onOpenTranslateModal={() => openTranslateModal(focusLocale)}
      />
    </div>
  );
}

// ── Inline SVG icons (shared toggle icons) ─────────────────────────────────

function ListIcon({ size = 11 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <line x1={8} x2={21} y1={6} y2={6} />
      <line x1={8} x2={21} y1={12} y2={12} />
      <line x1={8} x2={21} y1={18} y2={18} />
      <line x1={3} x2={3.01} y1={6} y2={6} />
      <line x1={3} x2={3.01} y1={12} y2={12} />
      <line x1={3} x2={3.01} y1={18} y2={18} />
    </svg>
  );
}

function GridIcon({ size = 11 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect width={7} height={7} x={3} y={3} rx={1} />
      <rect width={7} height={7} x={14} y={3} rx={1} />
      <rect width={7} height={7} x={14} y={14} rx={1} />
      <rect width={7} height={7} x={3} y={14} rx={1} />
    </svg>
  );
}
