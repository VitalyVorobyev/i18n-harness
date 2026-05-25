// TranslatePanel — Matrix / Focus dispatcher.
//
// Truly binary:
//   focusLocale === null  → MatrixView
//   focusLocale !== null  → FocusView (PR 5)

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
import { FocusView } from "./FocusView";
import type { MatrixFilter } from "./MatrixView";
import { MatrixView } from "./MatrixView";
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
  onTranslateAll,
}: Props) {
  // ── Matrix mode (focusLocale === null) ──────────────────────────────────
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

  // ── Focus mode (focusLocale !== null) ───────────────────────────────────
  return (
    <FocusView
      summary={summary}
      openCatalogs={openCatalogs}
      focusLocale={focusLocale}
      setFocusLocale={setFocusLocale}
      dirtyIds={dirtyIds}
      reports={reports}
      busyIds={busyIds}
      batchActive={batchActive}
      onEnsureCatalogLoaded={onEnsureCatalogLoaded}
      onTranslateUnit={onTranslateUnitFor}
      onEditUnit={onEditUnitFor}
      onAcceptUnit={onAcceptUnitFor}
      onTranslateAll={onTranslateAll}
    />
  );
}
