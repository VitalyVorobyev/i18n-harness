import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CatalogList } from "./components/CatalogList/CatalogList";
import { GlossaryPanel } from "./components/GlossaryPanel/GlossaryPanel";
import { HomeScreen, pushRecent } from "./components/HomeScreen/HomeScreen";
import { Inspector } from "./components/Inspector/Inspector";
import { ProjectSidebar } from "./components/ProjectSidebar/ProjectSidebar";
import type { ProjectView } from "./components/TopBar/TopBar";
import { ProjectTopBar } from "./components/TopBar/TopBar";
import {
  UnitEditor,
  type UnitEditorHandle,
} from "./components/UnitEditor/UnitEditor";
import {
  closeProject,
  currentProjectSummary,
  discardChangesInProject,
  openCatalogInProject,
  saveAllDirty,
  saveCatalogInProject,
  translateUnitInProject,
  updateUnitTargetInProject,
} from "./lib/tauri";
import { useTheme } from "./lib/theme";
import type {
  CatalogResponse,
  GateReport,
  ProjectSummary,
  TargetEdit,
  Unit,
  UnitId,
} from "./lib/types";

type Filter = "all" | "untranslated" | "proposed" | "finished";

interface Toast {
  kind: "info" | "error";
  message: string;
}

// Top-level mode: home screen when no project is open, project workspace otherwise.
type AppMode = { kind: "home" } | { kind: "project"; summary: ProjectSummary };

export function App() {
  const [theme, setTheme] = useTheme();
  const [mode, setMode] = useState<AppMode>({ kind: "home" });

  // ── Catalog / unit state ──────────────────────────────────────────────────
  const [catalog, setCatalog] = useState<CatalogResponse | null>(null);
  // Absolute path of the catalog currently open in the project store.
  const [activeCatalogPath, setActiveCatalogPath] = useState<string | null>(
    null,
  );
  const [selectedId, setSelectedId] = useState<UnitId | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [search, setSearch] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  // Per-unit dirty set for the currently-open catalog.
  const [dirtyIds, setDirtyIds] = useState<Set<UnitId>>(new Set());
  // Absolute paths of catalogs with unsaved edits (project-wide optimistic set).
  const [dirtyCatalogPaths, setDirtyCatalogPaths] = useState<Set<string>>(
    new Set(),
  );
  const [reports, setReports] = useState<Record<UnitId, GateReport>>({});
  const [busyIds, setBusyIds] = useState<Set<UnitId>>(new Set());
  const [toast, setToast] = useState<Toast | null>(null);

  // Project-mode view tab.
  const [projectView, setProjectView] = useState<ProjectView>("translate");

  const editorRef = useRef<UnitEditorHandle | null>(null);

  // On mount, check whether a project is already open (returns None on first
  // launch since the Rust slot resets after a restart).
  useEffect(() => {
    currentProjectSummary()
      .then((summary) => {
        if (summary) setMode({ kind: "project", summary });
      })
      .catch(() => {
        // Cannot determine; stay on home screen.
      });
  }, []);

  // Toast auto-dismisses after 4 s.
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

  const markCatalogDirty = useCallback((absPath: string) => {
    setDirtyCatalogPaths((prev) => {
      if (prev.has(absPath)) return prev;
      const next = new Set(prev);
      next.add(absPath);
      return next;
    });
  }, []);

  // ── Project mode transitions ──────────────────────────────────────────────

  const handleProjectOpened = useCallback((summary: ProjectSummary) => {
    setMode({ kind: "project", summary });
    setCatalog(null);
    setActiveCatalogPath(null);
    setSelectedId(null);
    setFilter("all");
    setSearch("");
    setDirtyIds(new Set());
    setDirtyCatalogPaths(new Set());
    setReports({});
    setBusyIds(new Set());
    setProjectView("translate");
    setError(null);
  }, []);

  const handleCloseProject = useCallback(async () => {
    try {
      await closeProject();
    } catch {
      // Navigate home regardless.
    }
    setMode({ kind: "home" });
    setCatalog(null);
    setActiveCatalogPath(null);
    setSelectedId(null);
    setDirtyIds(new Set());
    setDirtyCatalogPaths(new Set());
    setReports({});
    setError(null);
  }, []);

  // ── Project-mode catalog open ─────────────────────────────────────────────

  const handleCatalogSelect = useCallback(
    async (absPath: string) => {
      if (absPath === activeCatalogPath) return;
      setLoading(true);
      setError(null);
      try {
        const response = await openCatalogInProject(absPath);
        setCatalog(response);
        setActiveCatalogPath(absPath);
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
        setError(formatError(e));
        flashError(`Could not open catalog: ${formatError(e)}`);
      } finally {
        setLoading(false);
      }
    },
    [activeCatalogPath, flashError],
  );

  // ── Edit / translate / save / discard (project-scoped) ───────────────────

  const onEditTarget = useCallback(
    async (id: UnitId, edit: TargetEdit) => {
      if (!activeCatalogPath) return;
      try {
        const updated = await updateUnitTargetInProject(
          activeCatalogPath,
          id,
          edit,
        );
        replaceUnit(updated);
        markDirty(id);
        markCatalogDirty(activeCatalogPath);
      } catch (e) {
        flashError(`Edit failed: ${formatError(e)}`);
      }
    },
    [activeCatalogPath, replaceUnit, markDirty, markCatalogDirty, flashError],
  );

  const onTranslate = useCallback(
    async (id: UnitId) => {
      if (!activeCatalogPath) return;
      setBusyIds((prev) => new Set(prev).add(id));
      try {
        const result = await translateUnitInProject(activeCatalogPath, id);
        replaceUnit(result.unit);
        setReports((prev) => ({ ...prev, [id]: result.report }));
        markDirty(id);
        markCatalogDirty(activeCatalogPath);
        const findings = result.report.findings.length;
        flashInfo(
          findings === 0
            ? "Translated. Gate clean — promoted to Finished."
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
    [
      activeCatalogPath,
      replaceUnit,
      markDirty,
      markCatalogDirty,
      flashInfo,
      flashError,
    ],
  );

  const onSave = useCallback(async () => {
    if (!catalog || !activeCatalogPath) return;
    try {
      await editorRef.current?.flushPendingEdit();
    } catch (e) {
      flashError(`Save failed during flush: ${formatError(e)}`);
      return;
    }
    if (dirtyIds.size === 0) return;
    try {
      const summary = await saveCatalogInProject(activeCatalogPath);
      setDirtyIds(new Set());
      setDirtyCatalogPaths((prev) => {
        const next = new Set(prev);
        next.delete(activeCatalogPath);
        return next;
      });
      flashInfo(
        `Saved ${summary.unit_count} units to ${shortenPath(summary.path)}`,
      );
    } catch (e) {
      flashError(`Save failed: ${formatError(e)}`);
    }
  }, [catalog, activeCatalogPath, dirtyIds, flashInfo, flashError]);

  const onSaveAll = useCallback(async () => {
    if (dirtyCatalogPaths.size === 0) return;
    try {
      await editorRef.current?.flushPendingEdit();
    } catch {
      // Best-effort flush; continue saving.
    }
    try {
      const resp = await saveAllDirty();
      const savedCount = resp.saved.length;
      if (savedCount > 0) {
        setDirtyCatalogPaths((prev) => {
          const next = new Set(prev);
          for (const s of resp.saved) next.delete(s.path);
          return next;
        });
        flashInfo(
          `Saved ${savedCount} ${savedCount === 1 ? "catalog" : "catalogs"}.`,
        );
      }
      if (resp.failed_path) {
        flashError(
          `Failed to save ${shortenPath(resp.failed_path)}: ${resp.failed_reason ?? "unknown error"}`,
        );
      }
      // If the active catalog was saved, clear its per-unit dirty set.
      if (
        activeCatalogPath &&
        resp.saved.some((s) => s.path === activeCatalogPath)
      ) {
        setDirtyIds(new Set());
      }
    } catch (e) {
      flashError(`Save all failed: ${formatError(e)}`);
    }
  }, [dirtyCatalogPaths, activeCatalogPath, flashInfo, flashError]);

  const onDiscard = useCallback(async () => {
    if (!catalog || !activeCatalogPath) return;
    if (dirtyIds.size === 0) return;
    const ok = window.confirm(
      `Discard ${dirtyIds.size} unsaved change(s) and revert to disk?`,
    );
    if (!ok) return;
    try {
      const response = await discardChangesInProject(activeCatalogPath);
      setCatalog(response);
      setDirtyIds(new Set());
      setDirtyCatalogPaths((prev) => {
        const next = new Set(prev);
        next.delete(activeCatalogPath);
        return next;
      });
      setReports({});
      const stillThere = response.units.some((u) => u.id === selectedId);
      if (!stillThere) {
        setSelectedId(response.units[0]?.id ?? null);
      }
      flashInfo("Reverted to disk state.");
    } catch (e) {
      flashError(`Discard failed: ${formatError(e)}`);
    }
  }, [catalog, activeCatalogPath, dirtyIds, selectedId, flashInfo, flashError]);

  // ── Global keyboard shortcuts (project mode only) ─────────────────────────

  useEffect(() => {
    if (mode.kind !== "project") return;
    const handler = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      const k = e.key.toLowerCase();
      if (k === "s") {
        e.preventDefault();
        if (e.shiftKey) {
          void onSaveAll();
        } else {
          void onSave();
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [mode, onSave, onSaveAll]);

  const selectedUnit = useMemo(() => {
    if (!catalog || !selectedId) return null;
    return catalog.units.find((u) => u.id === selectedId) ?? null;
  }, [catalog, selectedId]);

  const selectedReport = selectedId ? (reports[selectedId] ?? null) : null;
  const selectedBusy = selectedId ? busyIds.has(selectedId) : false;

  // ── Home screen ───────────────────────────────────────────────────────────

  if (mode.kind === "home") {
    return (
      <>
        <HomeScreen
          onProjectOpened={(summary) => {
            pushRecent({
              path: summary.root,
              name: summary.name,
              lastOpenedAt: new Date().toISOString(),
            });
            handleProjectOpened(summary);
          }}
          flashError={flashError}
          flashInfo={flashInfo}
          theme={theme}
          onToggleTheme={() => setTheme(theme === "dark" ? "light" : "dark")}
        />
        {toast && <ToastBanner toast={toast} />}
      </>
    );
  }

  // ── Project mode ──────────────────────────────────────────────────────────

  const { summary } = mode;
  const unsavedCatalogCount = dirtyCatalogPaths.size;
  const glossaryPath = summary.glossary_path;

  return (
    <div className="h-screen w-screen flex flex-col overflow-hidden bg-bg-base text-fg-primary">
      <ProjectTopBar
        projectName={summary.name}
        view={projectView}
        onViewChange={setProjectView}
        theme={theme}
        onToggleTheme={() => setTheme(theme === "dark" ? "light" : "dark")}
        onCloseProject={handleCloseProject}
      />

      <div className="flex-1 flex overflow-hidden min-h-0">
        {/* Sidebar — always visible in project mode */}
        <ProjectSidebar
          summary={summary}
          activeCatalogPath={activeCatalogPath}
          dirtyCatalogPaths={dirtyCatalogPaths}
          onCatalogSelect={handleCatalogSelect}
          onCloseProject={handleCloseProject}
        />

        {/* Main content area */}
        <div className="flex-1 flex flex-col overflow-hidden min-h-0">
          {/* Translate view */}
          <div
            className={
              projectView === "translate"
                ? "flex-1 flex overflow-hidden min-h-0"
                : "hidden"
            }
          >
            {catalog ? (
              <>
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
              </>
            ) : (
              <div className="flex-1 flex flex-col items-center justify-center gap-3 text-fg-tertiary">
                <p className="text-sm">
                  Select a catalog from the sidebar to start translating.
                </p>
                {error && (
                  <p className="text-sm text-severity-hard max-w-sm text-center">
                    {error}
                  </p>
                )}
              </div>
            )}
          </div>

          {/* Glossary view */}
          <div
            className={
              projectView === "glossary" ? "flex-1 flex min-h-0" : "hidden"
            }
          >
            {glossaryPath ? (
              <GlossaryPanel flashError={flashError} flashInfo={flashInfo} />
            ) : (
              <div className="flex-1 flex items-center justify-center text-sm text-fg-tertiary p-8 text-center">
                <p>
                  This project has no glossary. Add a{" "}
                  <code className="font-mono text-xs bg-bg-surface px-1 py-0.5 rounded border border-border-subtle">
                    glossary.toml
                  </code>{" "}
                  and update the manifest in Settings (M4.3c).
                </p>
              </div>
            )}
          </div>

          {/* Settings view — placeholder for M4.3c */}
          <div
            className={
              projectView === "settings"
                ? "flex-1 flex items-center justify-center"
                : "hidden"
            }
          >
            <div className="text-center">
              <p className="text-sm font-medium text-fg-secondary">
                Project Settings
              </p>
              <p className="mt-1 text-xs text-fg-disabled">
                Coming in M4.3c — manifest editor, locale config, backend
                selection.
              </p>
            </div>
          </div>

          {/* Quality view — placeholder for M4.3d */}
          <div
            className={
              projectView === "quality"
                ? "flex-1 flex items-center justify-center"
                : "hidden"
            }
          >
            <div className="text-center">
              <p className="text-sm font-medium text-fg-secondary">Quality</p>
              <p className="mt-1 text-xs text-fg-disabled">
                Coming in M4.3d — acceptance rate, translation memory, prompt
                evaluation.
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Footer bar — shown in Translate view; Save all button when there are unsaved catalogs */}
      {projectView === "translate" && (
        <footer className="shrink-0 h-9 px-4 flex items-center justify-between border-t border-border-subtle bg-bg-surface text-xs text-fg-tertiary">
          <span>
            {summary.catalogs.length}{" "}
            {summary.catalogs.length === 1 ? "catalog" : "catalogs"}
            {unsavedCatalogCount > 0 && (
              <>
                {" "}
                &middot;{" "}
                <span className="text-state-proposed font-medium">
                  {unsavedCatalogCount} unsaved
                </span>
              </>
            )}
          </span>
          {unsavedCatalogCount > 0 && (
            <button
              type="button"
              onClick={onSaveAll}
              className="h-6 px-3 rounded-md border border-border-default bg-transparent text-xs font-medium text-fg-secondary hover:bg-bg-hover hover:text-fg-primary hover:border-border-strong active:bg-bg-selected transition-colors duration-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
              title="Save all unsaved catalogs (Cmd+Shift+S)"
            >
              Save all
            </button>
          )}
        </footer>
      )}

      {/* Loading overlay */}
      {loading && (
        <div
          aria-live="polite"
          className="fixed bottom-4 right-4 z-50 px-3 py-2 rounded-md border border-border-default bg-bg-elevated text-sm text-fg-secondary shadow-md"
        >
          Loading catalog…
        </div>
      )}

      {toast && <ToastBanner toast={toast} />}

      {/* Discard shortcut handler — accessible via onDiscard (no visible button
          in M4.3a; the per-catalog discard action is wired and callable via
          keyboard in later slices). */}
      <span
        className="sr-only"
        aria-hidden="true"
        data-discard={String(!!onDiscard)}
      />
    </div>
  );
}

// ── Shared primitives ─────────────────────────────────────────────────────────

function ToastBanner({
  toast,
}: {
  toast: { kind: "info" | "error"; message: string };
}) {
  return (
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
