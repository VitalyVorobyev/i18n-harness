// TranslatePanel — Matrix / Focus dispatcher.
//
// The container takes the data the Translate tab needs from App and picks
// a sub-view based on `focusLocale`:
//
// - `focusLocale === null` → MatrixView (PR 4 — this PR).
// - `focusLocale !== null` → the legacy 3-pane Catalog + UnitEditor +
//   Inspector layout, kept alive as a fallback. PR 5 lands FocusView and
//   replaces this branch.

import type { Ref } from "react";
import type {
  BatchScope,
  CatalogResponse,
  GateReport,
  ProjectSummary,
  TargetEdit,
  Unit,
  UnitId,
} from "../../lib/types";
import type { Filter as CatalogFilter } from "./CatalogList/CatalogList";
import { CatalogList } from "./CatalogList/CatalogList";
import { Inspector } from "./Inspector/Inspector";
import type { MatrixFilter } from "./MatrixView";
import { MatrixView } from "./MatrixView";
import { UnitEditor, type UnitEditorHandle } from "./UnitEditor/UnitEditor";

interface Props {
  // ── Project + catalog cache ─────────────────────────────────────────────
  summary: ProjectSummary;
  openCatalogs: Map<string, CatalogResponse>;

  // ── Active-catalog (focus-mode fallback) state ──────────────────────────
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

  // ── Focus-locale dispatch (sacred per the redesign brief) ───────────────
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
}

export function TranslatePanel({
  summary,
  openCatalogs,
  activeCatalogPath,
  catalog,
  selectedId,
  filter,
  search,
  dirtyIds,
  reports,
  busyIds,
  batchActive,
  error,
  editorRef,
  focusLocale,
  setFocusLocale,
  onSelect,
  onFilterChange,
  onSearchChange,
  onEdit,
  onTranslate,
  onAccept,
  onEnsureCatalogLoaded,
  onTranslateUnitFor,
  onEditUnitFor,
  onAcceptUnitFor,
  onTranslateAll,
}: Props) {
  // ── Matrix mode (focus === null) ────────────────────────────────────────
  if (focusLocale === null) {
    return (
      <MatrixView
        summary={summary}
        openCatalogs={openCatalogs}
        dirtyIds={dirtyIds}
        reports={reports}
        busyIds={busyIds}
        batchActive={batchActive}
        filter={filter}
        search={search}
        onFilterChange={onFilterChange}
        onSearchChange={onSearchChange}
        onEnsureCatalogLoaded={onEnsureCatalogLoaded}
        setFocusLocale={setFocusLocale}
        onTranslateUnit={onTranslateUnitFor}
        onEditUnit={onEditUnitFor}
        onAcceptUnit={onAcceptUnitFor}
        onTranslateAll={onTranslateAll}
      />
    );
  }

  // ── Focus-mode fallback (PR 5 will replace this branch) ─────────────────
  //
  // For now we render the legacy 3-pane verbatim. The active catalog state
  // continues to be owned by App, so the existing edit / save / discard
  // wiring keeps working. A small banner advertises the (yet-unimplemented)
  // focus-locale switcher so the user has a way back to Matrix.
  const selectedUnit =
    catalog && selectedId
      ? (catalog.units.find((u) => u.id === selectedId) ?? null)
      : null;
  const selectedReport = selectedId ? (reports[selectedId] ?? null) : null;
  const selectedBusy = selectedId ? busyIds.has(selectedId) : false;
  // Cast the matrix filter back to the catalog-list filter union. The
  // legacy CatalogList does not understand the matrix-only saved views;
  // they map down to "all" for the fallback render.
  const fallbackFilter: CatalogFilter = isCatalogFilter(filter)
    ? filter
    : "all";

  return (
    <div className="flex-1 flex flex-col overflow-hidden min-h-0">
      <FocusBanner locale={focusLocale} onClear={() => setFocusLocale(null)} />
      <div className="flex-1 flex overflow-hidden min-h-0">
        {catalog ? (
          <>
            <CatalogList
              units={catalog.units}
              selectedId={selectedId}
              filter={fallbackFilter}
              search={search}
              dirtyIds={dirtyIds}
              onSelect={onSelect}
              onFilterChange={(f) => onFilterChange(f)}
              onSearchChange={onSearchChange}
              onTranslateAll={(scope) => {
                if (activeCatalogPath) onTranslateAll(activeCatalogPath, scope);
              }}
              batchActive={batchActive}
            />
            {selectedUnit ? (
              <UnitEditor
                ref={editorRef}
                unit={selectedUnit}
                busy={selectedBusy}
                hasOllama={Boolean(catalog.language)}
                onEdit={onEdit}
                onTranslate={onTranslate}
              />
            ) : (
              <div className="flex-1 flex items-center justify-center text-sm text-fg-tertiary bg-bg-base">
                <p>Select a unit on the left to inspect it.</p>
              </div>
            )}
            {selectedUnit && (
              <Inspector
                unit={selectedUnit}
                report={selectedReport}
                activeCatalogPath={activeCatalogPath}
                busyIds={busyIds}
                onAccept={onAccept}
              />
            )}
          </>
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center gap-3 text-fg-tertiary">
            <p className="text-sm">
              Select a catalog from the sidebar to start translating
              {focusLocale ? ` in ${focusLocale}` : ""}.
            </p>
            {error && (
              <p className="text-sm text-severity-hard max-w-sm text-center">
                {error}
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function FocusBanner({
  locale,
  onClear,
}: {
  locale: string;
  onClear: () => void;
}) {
  return (
    <div className="shrink-0 flex items-center gap-3 px-6 py-2 border-b border-border-subtle bg-bg-surface">
      <span className="text-xs uppercase tracking-loose text-fg-tertiary">
        Focus
      </span>
      <span className="font-mono text-xs text-accent">{locale}</span>
      <span className="text-xs text-fg-tertiary">
        Focus mode UI lands in a follow-up; the 3-pane editor is shown here in
        the meantime.
      </span>
      <span className="flex-1" />
      <button
        type="button"
        onClick={onClear}
        className="inline-flex items-center h-6 px-2 rounded-md border border-border-default text-xs text-fg-secondary hover:bg-bg-hover hover:text-fg-primary transition-colors duration-100"
      >
        Back to matrix
      </button>
    </div>
  );
}

function isCatalogFilter(f: MatrixFilter): f is CatalogFilter {
  return (
    f === "all" ||
    f === "untranslated" ||
    f === "proposed" ||
    f === "finished" ||
    f === "needs-review"
  );
}
