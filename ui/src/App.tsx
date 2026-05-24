import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CatalogList } from "./components/CatalogList/CatalogList";
import { GlossaryPanel } from "./components/GlossaryPanel/GlossaryPanel";
import { HomeScreen, pushRecent } from "./components/HomeScreen/HomeScreen";
import { Inspector } from "./components/Inspector/Inspector";
import { ProjectSettings } from "./components/ProjectSettings/ProjectSettings";
import { ProjectSidebar } from "./components/ProjectSidebar/ProjectSidebar";
import { QualityPanel } from "./components/QualityPanel/QualityPanel";
import type { ProjectView } from "./components/TopBar/TopBar";
import { ProjectTopBar } from "./components/TopBar/TopBar";
import {
  UnitEditor,
  type UnitEditorHandle,
} from "./components/UnitEditor/UnitEditor";
import {
  acceptUnitInProject,
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
  ProjectOpenResponse,
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

// When the user attempts to close the project (or open another while dirty),
// we present an inline confirmation with three choices.
type CloseConfirmPending =
  | { kind: "none" }
  | { kind: "pending"; reason: "close" | "open-other" };

export function App() {
  const [theme, setTheme] = useTheme();
  const [mode, setMode] = useState<AppMode>({ kind: "home" });

  // ── Catalog cache ─────────────────────────────────────────────────────────
  // All catalogs that have been opened in this session, keyed by absolute path.
  // Re-clicking a sidebar entry restores the cached state (including unsaved
  // edits) without re-extracting from disk.
  const [openCatalogs, setOpenCatalogs] = useState<
    Map<string, CatalogResponse>
  >(new Map());
  // Absolute path of the catalog currently displayed.
  const [activeCatalogPath, setActiveCatalogPath] = useState<string | null>(
    null,
  );
  // Derived: the catalog object for the active path (null when nothing selected).
  // Components that used to read `catalog` now read this derived value.
  const catalog = activeCatalogPath
    ? (openCatalogs.get(activeCatalogPath) ?? null)
    : null;

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

  // Inline close-project confirmation state (P1 #2 guard).
  const [closeConfirm, setCloseConfirm] = useState<CloseConfirmPending>({
    kind: "none",
  });

  // Project-mode view tab.
  const [projectView, setProjectView] = useState<ProjectView>("translate");

  // Locale chips multi-select filter. Empty set = "show all".
  const [activeLocaleFilter, setActiveLocaleFilter] = useState<Set<string>>(
    new Set(),
  );

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

  // Merge an updated unit into the cached CatalogResponse for the active path.
  const replaceUnit = useCallback(
    (updated: Unit) => {
      if (!activeCatalogPath) return;
      setOpenCatalogs((prev) => {
        const entry = prev.get(activeCatalogPath);
        if (!entry) return prev;
        const next = new Map(prev);
        next.set(activeCatalogPath, {
          ...entry,
          units: entry.units.map((u) => (u.id === updated.id ? updated : u)),
        });
        return next;
      });
    },
    [activeCatalogPath],
  );

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
    // Clear the catalog cache so stale data from a previous project is gone.
    setOpenCatalogs(new Map());
    setActiveCatalogPath(null);
    setSelectedId(null);
    setFilter("all");
    setSearch("");
    setDirtyIds(new Set());
    setDirtyCatalogPaths(new Set());
    setReports({});
    setBusyIds(new Set());
    setProjectView("translate");
    setActiveLocaleFilter(new Set());
    setError(null);
    setCloseConfirm({ kind: "none" });
  }, []);

  // Called after every Settings-view mutation. Updates the in-memory summary so
  // all views reflect the new manifest state without a round trip.
  const handleProjectMutation = useCallback(
    (response: ProjectOpenResponse) => {
      setMode({ kind: "project", summary: response.summary });
      if (response.warnings.length > 0) {
        flashInfo(
          `Manifest updated. Warnings: ${response.warnings.join("; ")}`,
        );
      }
    },
    [flashInfo],
  );

  // Internal: perform the close without any dirty-state checks.
  const _doCloseProject = useCallback(async () => {
    try {
      await closeProject();
    } catch {
      // Navigate home regardless of backend error.
    }
    setMode({ kind: "home" });
    setOpenCatalogs(new Map());
    setActiveCatalogPath(null);
    setSelectedId(null);
    setDirtyIds(new Set());
    setDirtyCatalogPaths(new Set());
    setReports({});
    setError(null);
    setCloseConfirm({ kind: "none" });
  }, []);

  // Public entry point — raises inline confirmation when there are unsaved catalogs.
  const handleCloseProject = useCallback(() => {
    if (dirtyCatalogPaths.size > 0) {
      setCloseConfirm({ kind: "pending", reason: "close" });
    } else {
      void _doCloseProject();
    }
  }, [dirtyCatalogPaths.size, _doCloseProject]);

  // "Save & Close" — save all dirty catalogs, then close (abort if save fails).
  const handleSaveAndClose = useCallback(async () => {
    try {
      await editorRef.current?.flushPendingEdit();
    } catch {
      // Best-effort flush; continue with save.
    }
    try {
      const resp = await saveAllDirty();
      if (resp.failed_path) {
        flashError(
          `Could not save ${shortenPath(resp.failed_path)}: ${resp.failed_reason ?? "unknown error"}. Close aborted.`,
        );
        setCloseConfirm({ kind: "none" });
        return;
      }
    } catch (e) {
      flashError(`Save failed: ${formatError(e)}. Close aborted.`);
      setCloseConfirm({ kind: "none" });
      return;
    }
    await _doCloseProject();
  }, [_doCloseProject, flashError]);

  // "Discard & Close" — close immediately, dropping all unsaved state.
  const handleDiscardAndClose = useCallback(() => {
    void _doCloseProject();
  }, [_doCloseProject]);

  // ── Project-mode catalog open ─────────────────────────────────────────────

  const handleCatalogSelect = useCallback(
    async (absPath: string) => {
      if (absPath === activeCatalogPath) return;

      // If the catalog is already cached, switch to it without re-extracting
      // from disk — this preserves any unsaved edits the user made before
      // switching away.
      const cachedEntry = openCatalogs.get(absPath);
      if (cachedEntry !== undefined) {
        setActiveCatalogPath(absPath);
        // Reset per-catalog UI state that does not carry over between catalogs.
        setFilter("all");
        setSearch("");
        setReports({});
        setBusyIds(new Set());
        // Select first unit in the newly active catalog.
        const first =
          cachedEntry.units.find((u) => u.state === "untranslated") ??
          cachedEntry.units[0];
        setSelectedId(first?.id ?? null);
        // Dirty IDs are per-active-catalog — reset when switching.
        setDirtyIds(new Set());
        return;
      }

      // First open: extract from disk via IPC and populate the cache.
      setLoading(true);
      setError(null);
      try {
        const response = await openCatalogInProject(absPath);
        setOpenCatalogs((prev) => new Map(prev).set(absPath, response));
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
    [activeCatalogPath, openCatalogs, flashError],
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
            ? "Translated. Gate clean — review and save to accept."
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
      // Legitimate "re-extract from disk" case — replace the cached entry.
      setOpenCatalogs((prev) => new Map(prev).set(activeCatalogPath, response));
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

  // ── Accept (M4.6.2) ─────────────────────────────────────────────────────────

  const onAccept = useCallback(
    async (id: UnitId) => {
      if (!activeCatalogPath) return;
      setBusyIds((prev) => new Set(prev).add(id));
      try {
        const updated = await acceptUnitInProject(activeCatalogPath, id);
        replaceUnit(updated);
        markCatalogDirty(activeCatalogPath);
        flashInfo(`Marked unit ${id} as reviewed`);
      } catch (e) {
        flashError(`Accept failed: ${formatError(e)}`);
      } finally {
        setBusyIds((prev) => {
          const next = new Set(prev);
          next.delete(id);
          return next;
        });
      }
    },
    [activeCatalogPath, replaceUnit, markCatalogDirty, flashInfo, flashError],
  );

  // ── Locale filter + sibling quick-switch ────────────────────────────────────

  // Naive stem heuristic: strip the trailing `_<locale>` segment from the
  // filename (everything before the last underscore-locale suffix), then
  // compare stems across catalogs.  Example: "app_de.ts" → stem "app";
  // "app_fr.ts" → stem "app". Only matches when there is exactly one
  // sibling for the target locale. Ambiguous or no-match cases fall back
  // to filter-only behavior.
  const handleLocaleFilterChange = useCallback(
    (next: Set<string>) => {
      setActiveLocaleFilter(next);

      // Sibling quick-switch: only when exactly one locale is now active AND
      // the currently-open catalog is for a *different* locale.
      if (next.size !== 1 || mode.kind !== "project") return;
      const [targetLocale] = [...next];
      if (!activeCatalogPath) return;

      const catalogs = mode.summary.catalogs;
      const currentRef = catalogs.find(
        (c) => c.absolute_path === activeCatalogPath,
      );
      if (!currentRef || currentRef.locale === targetLocale) return;

      // Compute stem by stripping the `_<locale>` suffix from the basename.
      function stemOf(ref: (typeof catalogs)[number]): string {
        const basename = ref.manifest_path || ref.absolute_path;
        const name = basename.replace(/\\/g, "/").split("/").pop() ?? basename;
        // Strip extension, then trailing `_<locale>` (case-insensitive locale match).
        const noExt = name.replace(/\.[^.]+$/, "");
        const escaped = ref.locale.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
        return noExt.replace(new RegExp(`_${escaped}$`, "i"), "");
      }

      const currentStem = stemOf(currentRef);
      const candidates = catalogs.filter(
        (c) => c.locale === targetLocale && stemOf(c) === currentStem,
      );
      if (candidates.length === 1 && candidates[0]) {
        void handleCatalogSelect(candidates[0].absolute_path);
      }
    },
    [mode, activeCatalogPath, handleCatalogSelect],
  );

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
        locales={summary.locales}
        activeLocaleFilter={activeLocaleFilter}
        onLocaleFilterChange={handleLocaleFilterChange}
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
          activeLocaleFilter={activeLocaleFilter}
          onCatalogSelect={handleCatalogSelect}
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
              <GlossaryPanel
                flashError={flashError}
                flashInfo={flashInfo}
                initialPath={glossaryPath}
              />
            ) : (
              <div className="flex-1 flex items-center justify-center text-sm text-fg-tertiary p-8 text-center">
                <p>
                  This project has no glossary. Add a{" "}
                  <code className="font-mono text-xs bg-bg-surface px-1 py-0.5 rounded border border-border-subtle">
                    glossary.toml
                  </code>{" "}
                  and update the manifest in Settings.
                </p>
              </div>
            )}
          </div>

          {/* Settings view — M4.3c manifest editor */}
          <div
            className={
              projectView === "settings" ? "flex-1 flex min-h-0" : "hidden"
            }
          >
            <ProjectSettings
              summary={summary}
              onMutation={handleProjectMutation}
              flashError={flashError}
              flashInfo={flashInfo}
            />
          </div>

          {/* Quality view — M4.3d */}
          <div
            className={
              projectView === "quality" ? "flex-1 flex min-h-0" : "hidden"
            }
          >
            <QualityPanel
              summary={summary}
              flashError={flashError}
              flashInfo={flashInfo}
            />
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

      {/* Close-project confirmation overlay — shown when there are unsaved
          catalogs and the user has clicked "Close project". Three actions:
          Save & Close, Discard & Close, Cancel. */}
      {closeConfirm.kind === "pending" && (
        <CloseConfirmOverlay
          unsavedCount={dirtyCatalogPaths.size}
          onSaveAndClose={() => void handleSaveAndClose()}
          onDiscardAndClose={handleDiscardAndClose}
          onCancel={() => setCloseConfirm({ kind: "none" })}
        />
      )}

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

// Inline modal that guards "Close project" when there are unsaved catalogs.
// Rendered as a fixed overlay; no native dialog dependency.
function CloseConfirmOverlay({
  unsavedCount,
  onSaveAndClose,
  onDiscardAndClose,
  onCancel,
}: {
  unsavedCount: number;
  onSaveAndClose: () => void;
  onDiscardAndClose: () => void;
  onCancel: () => void;
}) {
  // Dismiss on Escape.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onCancel]);

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="close-confirm-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div className="w-[360px] rounded-lg border border-border-default bg-bg-elevated shadow-xl p-5 flex flex-col gap-4">
        <p
          id="close-confirm-title"
          className="text-sm font-medium text-fg-primary"
        >
          You have{" "}
          <span className="text-state-proposed font-semibold">
            {unsavedCount} unsaved {unsavedCount === 1 ? "catalog" : "catalogs"}
          </span>
          . What would you like to do?
        </p>
        <div className="flex flex-col gap-2">
          <button
            type="button"
            onClick={onSaveAndClose}
            className="w-full h-8 px-3 rounded-md border border-border-default bg-bg-surface text-xs font-medium text-fg-primary hover:bg-bg-hover hover:border-border-strong active:bg-bg-selected transition-colors duration-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
          >
            Save all &amp; close
          </button>
          <button
            type="button"
            onClick={onDiscardAndClose}
            className="w-full h-8 px-3 rounded-md border border-border-default bg-bg-surface text-xs font-medium text-severity-hard hover:bg-bg-hover hover:border-border-strong active:bg-bg-selected transition-colors duration-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
          >
            Discard changes &amp; close
          </button>
          <button
            type="button"
            onClick={onCancel}
            className="w-full h-8 px-3 rounded-md border border-border-subtle bg-transparent text-xs font-medium text-fg-tertiary hover:bg-bg-hover hover:border-border-default hover:text-fg-secondary transition-colors duration-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}

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
