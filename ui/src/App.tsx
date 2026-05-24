import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CatalogList } from "./components/CatalogList/CatalogList";
import { EmptyState } from "./components/EmptyState/EmptyState";
import { Inspector } from "./components/Inspector/Inspector";
import { TopBar } from "./components/TopBar/TopBar";
import {
  UnitEditor,
  type UnitEditorHandle,
} from "./components/UnitEditor/UnitEditor";
import {
  appVersion,
  discardChanges,
  openCatalog,
  pickCatalogFile,
  saveCatalog,
  translateUnit,
  updateUnitTarget,
} from "./lib/tauri";
import type {
  CatalogResponse,
  GateReport,
  TargetEdit,
  Unit,
  UnitId,
} from "./lib/types";

type Filter = "all" | "untranslated" | "proposed" | "finished";

interface Toast {
  kind: "info" | "error";
  message: string;
}

export function App() {
  const [version, setVersion] = useState("0.0.0");
  const [catalog, setCatalog] = useState<CatalogResponse | null>(null);
  const [selectedId, setSelectedId] = useState<UnitId | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [search, setSearch] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [dirtyIds, setDirtyIds] = useState<Set<UnitId>>(new Set());
  const [reports, setReports] = useState<Record<UnitId, GateReport>>({});
  const [busyIds, setBusyIds] = useState<Set<UnitId>>(new Set());
  const [toast, setToast] = useState<Toast | null>(null);
  const editorRef = useRef<UnitEditorHandle | null>(null);

  useEffect(() => {
    appVersion()
      .then(setVersion)
      .catch(() => setVersion("dev"));
  }, []);

  // Toast auto-dismisses after 4s.
  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 4000);
    return () => clearTimeout(t);
  }, [toast]);

  const flashError = useCallback((message: string) => {
    setToast({ kind: "error", message });
  }, []);

  const flashInfo = useCallback((message: string) => {
    setToast({ kind: "info", message });
  }, []);

  const replaceUnit = useCallback((updated: Unit) => {
    setCatalog((prev) =>
      prev
        ? {
            ...prev,
            units: prev.units.map((u) => (u.id === updated.id ? updated : u)),
          }
        : prev,
    );
  }, []);

  const markDirty = useCallback((id: UnitId) => {
    setDirtyIds((prev) => {
      if (prev.has(id)) return prev;
      const next = new Set(prev);
      next.add(id);
      return next;
    });
  }, []);

  const openFile = useCallback(async () => {
    if (dirtyIds.size > 0) {
      const ok = window.confirm(
        `You have ${dirtyIds.size} unsaved change(s). Discard and open another catalog?`,
      );
      if (!ok) return;
    }
    setError(null);
    let path: string | null;
    try {
      path = await pickCatalogFile();
    } catch (e) {
      setError(`Open dialog failed: ${formatError(e)}`);
      return;
    }
    if (!path) return;
    setLoading(true);
    try {
      const response = await openCatalog(path);
      setCatalog(response);
      const first =
        response.units.find((u) => u.state === "untranslated") ??
        response.units[0];
      setSelectedId(first?.id ?? null);
      setFilter("all");
      setSearch("");
      setDirtyIds(new Set());
      setReports({});
      setBusyIds(new Set());
    } catch (e) {
      setError(`Could not open ${path}: ${formatError(e)}`);
      setCatalog(null);
      setSelectedId(null);
    } finally {
      setLoading(false);
    }
  }, [dirtyIds]);

  const onEditTarget = useCallback(
    async (id: UnitId, edit: TargetEdit) => {
      try {
        const updated = await updateUnitTarget(id, edit);
        replaceUnit(updated);
        markDirty(id);
      } catch (e) {
        flashError(`Edit failed: ${formatError(e)}`);
      }
    },
    [replaceUnit, markDirty, flashError],
  );

  const onTranslate = useCallback(
    async (id: UnitId) => {
      setBusyIds((prev) => new Set(prev).add(id));
      try {
        const result = await translateUnit(id);
        replaceUnit(result.unit);
        setReports((prev) => ({ ...prev, [id]: result.report }));
        markDirty(id);
        const findings = result.report.findings.length;
        flashInfo(
          findings === 0
            ? `Translated. Gate clean — promoted to Finished.`
            : `Translated with ${findings} finding(s).`,
        );
      } catch (e) {
        flashError(`Translate failed: ${formatError(e)}`);
      } finally {
        setBusyIds((prev) => {
          const next = new Set(prev);
          next.delete(id);
          return next;
        });
      }
    },
    [replaceUnit, markDirty, flashInfo, flashError],
  );

  const onSave = useCallback(async () => {
    if (!catalog) return;
    try {
      // Flush any pending textarea draft first — ⌘S while the editor
      // is still focused must land that edit before save_catalog runs.
      await editorRef.current?.flushPendingEdit();
    } catch (e) {
      flashError(`Save failed during flush: ${formatError(e)}`);
      return;
    }
    if (dirtyIds.size === 0) return;
    try {
      const summary = await saveCatalog();
      setDirtyIds(new Set());
      flashInfo(
        `Saved ${summary.unit_count} units to ${shortenPath(summary.path)}`,
      );
    } catch (e) {
      flashError(`Save failed: ${formatError(e)}`);
    }
  }, [catalog, dirtyIds, flashInfo, flashError]);

  const onDiscard = useCallback(async () => {
    if (dirtyIds.size === 0) return;
    const ok = window.confirm(
      `Discard ${dirtyIds.size} unsaved change(s) and revert to disk?`,
    );
    if (!ok) return;
    try {
      const response = await discardChanges();
      setCatalog(response);
      setDirtyIds(new Set());
      setReports({});
      // Keep the selected id if it still exists; otherwise reset.
      const stillThere = response.units.some((u) => u.id === selectedId);
      if (!stillThere) {
        setSelectedId(response.units[0]?.id ?? null);
      }
      flashInfo("Reverted to disk state.");
    } catch (e) {
      flashError(`Discard failed: ${formatError(e)}`);
    }
  }, [dirtyIds, selectedId, flashInfo, flashError]);

  // Global shortcuts.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      const k = e.key.toLowerCase();
      if (k === "o") {
        e.preventDefault();
        void openFile();
      } else if (k === "s") {
        e.preventDefault();
        void onSave();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [openFile, onSave]);

  const selectedUnit = useMemo(() => {
    if (!catalog || !selectedId) return null;
    return catalog.units.find((u) => u.id === selectedId) ?? null;
  }, [catalog, selectedId]);

  const selectedReport = selectedId ? (reports[selectedId] ?? null) : null;
  const selectedBusy = selectedId ? busyIds.has(selectedId) : false;

  return (
    <div className="h-screen w-screen flex flex-col overflow-hidden bg-bg-base text-fg-primary">
      <TopBar
        catalogPath={catalog?.path ?? null}
        language={catalog?.language ?? null}
        unitCount={catalog?.unit_count ?? 0}
        dirtyCount={dirtyIds.size}
        version={version}
        onOpen={openFile}
        onSave={onSave}
        onDiscard={onDiscard}
      />
      {catalog ? (
        <div className="flex-1 flex overflow-hidden min-h-0">
          <CatalogList
            units={catalog.units}
            selectedId={selectedId}
            filter={filter}
            search={search}
            dirtyIds={dirtyIds}
            onSelect={setSelectedId}
            onFilterChange={setFilter}
            onSearchChange={setSearch}
          />
          {selectedUnit ? (
            <UnitEditor
              ref={editorRef}
              unit={selectedUnit}
              busy={selectedBusy}
              hasOllama={Boolean(catalog.language)}
              onEdit={onEditTarget}
              onTranslate={onTranslate}
            />
          ) : (
            <div className="flex-1 flex items-center justify-center text-sm text-fg-tertiary bg-bg-base">
              <p>Select a unit on the left to inspect it.</p>
            </div>
          )}
          {selectedUnit && (
            <Inspector unit={selectedUnit} report={selectedReport} />
          )}
        </div>
      ) : (
        <EmptyState
          onOpen={openFile}
          {...(error ? { errorMessage: error } : {})}
        />
      )}
      {loading && (
        <div
          aria-live="polite"
          className="fixed bottom-4 right-4 z-50 px-3 py-2 rounded-md border border-border-default bg-bg-elevated text-sm text-fg-secondary shadow-md"
        >
          Loading catalog…
        </div>
      )}
      {toast && (
        <div
          role="status"
          aria-live="polite"
          className={`fixed bottom-4 right-4 z-50 px-3 py-2 rounded-md border text-sm shadow-md max-w-[420px] ${
            toast.kind === "error"
              ? "bg-severity-hard-bg border-severity-hard-border text-severity-hard"
              : "bg-bg-elevated border-border-default text-fg-secondary"
          }`}
        >
          {toast.message}
        </div>
      )}
    </div>
  );
}

function shortenPath(p: string): string {
  const parts = p.split(/[\\/]/);
  return parts.length > 2 ? `…/${parts.slice(-2).join("/")}` : p;
}

function formatError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  return String(e);
}
