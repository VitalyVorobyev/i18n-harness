import type { UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ActiveBatch } from "./components/BatchProgressWidget/BatchProgressWidget";
import { BatchProgressWidget } from "./components/BatchProgressWidget/BatchProgressWidget";
import type { Filter } from "./components/CatalogList/CatalogList";
import { CatalogList } from "./components/CatalogList/CatalogList";
import { GlossaryPanel } from "./components/GlossaryPanel/GlossaryPanel";
import { HomeScreen, pushRecent } from "./components/HomeScreen/HomeScreen";
import { Inspector } from "./components/Inspector/Inspector";
import { OverviewPanel } from "./components/OverviewPanel/OverviewPanel";
import { ProjectSettings } from "./components/ProjectSettings/ProjectSettings";
import { ProjectSidebar } from "./components/ProjectSidebar/ProjectSidebar";
import { QualityPanel } from "./components/QualityPanel/QualityPanel";
import { ReviewQueue } from "./components/ReviewQueue/ReviewQueue";
import type { ProjectView } from "./components/TopBar/TopBar";
import { ProjectTopBar } from "./components/TopBar/TopBar";
import {
  UnitEditor,
  type UnitEditorHandle,
} from "./components/UnitEditor/UnitEditor";
import {
  acceptUnitInProject,
  cancelTranslation,
  closeProject,
  currentProjectSummary,
  discardChangesInProject,
  openCatalogInProject,
  saveAllDirty,
  saveCatalogInProject,
  scanProjectReviewState,
  translateBatchInProject,
  translateUnitInProject,
  updateUnitTargetInProject,
} from "./lib/tauri";
import { listenBatchProgress } from "./lib/tauri-events";
import { useTheme } from "./lib/theme";
import type {
  BatchScope,
  CatalogResponse,
  GateReport,
  ProjectOpenResponse,
  ProjectSummary,
  ReviewQueueResponse,
  TargetEdit,
  Unit,
  UnitId,
} from "./lib/types";

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

  // ── Batch translate state (M4.8) ─────────────────────────────────────────
  // Single active batch slot; the Rust side enforces one job per (catalog,
  // locale) pair — if a second start arrives for the same pair, the IPC call
  // errors and we toast it. The UI only tracks one batch at a time (the one
  // that is running most recently).
  const [activeBatch, setActiveBatch] = useState<ActiveBatch | null>(null);
  const unlistenRef = useRef<UnlistenFn | null>(null);
  // Mirror of activeBatch kept in a ref so the unmount cleanup can read the
  // latest value without capturing a stale closure.
  const activeBatchRef = useRef<ActiveBatch | null>(null);
  useEffect(() => {
    activeBatchRef.current = activeBatch;
  });

  // ── Review queue (M4.7) ───────────────────────────────────────────────────
  // null = not yet scanned; populated eagerly when a project is open and
  // refreshed after every significant mutation (translate / accept / save /
  // discard). Debounced 200ms to avoid hammering the IPC bridge during
  // rapid edits.
  const [reviewQueue, setReviewQueue] = useState<ReviewQueueResponse | null>(
    null,
  );
  const rescanTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Schedule a debounced review-queue rescan (200ms).
  const scheduleRescan = useCallback(() => {
    if (rescanTimerRef.current !== null) {
      clearTimeout(rescanTimerRef.current);
    }
    rescanTimerRef.current = setTimeout(() => {
      rescanTimerRef.current = null;
      scanProjectReviewState()
        .then((resp) => setReviewQueue(resp))
        .catch(() => {
          // Rescan failure is non-fatal — the badge simply stays stale.
        });
    }, 200);
  }, []);

  // Clear the debounce timer when the app unmounts (unlikely but clean).
  useEffect(() => {
    return () => {
      if (rescanTimerRef.current !== null) {
        clearTimeout(rescanTimerRef.current);
      }
    };
  }, []);

  // Inline close-project confirmation state (P1 #2 guard).
  const [closeConfirm, setCloseConfirm] = useState<CloseConfirmPending>({
    kind: "none",
  });

  // Project-mode view tab.
  const [projectView, setProjectView] = useState<ProjectView>("overview");

  // Session-only focus locale — set when the user picks a locale from the
  // Overview pickup card. NOT persisted to disk or localStorage. PRs 4-5
  // will consume this to drive Focus mode in TranslatePanel.
  const [focusLocale, setFocusLocale] = useState<string | null>(null);

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

  const handleProjectOpened = useCallback(
    (summary: ProjectSummary) => {
      // Cancel any in-flight batch from the previous project (fire-and-forget).
      if (activeBatch) {
        cancelTranslation(activeBatch.jobId).catch(() => {});
        if (unlistenRef.current) {
          unlistenRef.current();
          unlistenRef.current = null;
        }
        setActiveBatch(null);
      }
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
      setProjectView("overview");
      setFocusLocale(null);
      setActiveLocaleFilter(new Set());
      setError(null);
      setCloseConfirm({ kind: "none" });
      // Reset and immediately kick off a review-queue scan for the new project.
      setReviewQueue(null);
      scheduleRescan();
    },
    [scheduleRescan, activeBatch],
  );

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
    // Cancel any in-flight batch (fire-and-forget; don't block close on terminal).
    if (activeBatch) {
      cancelTranslation(activeBatch.jobId).catch(() => {});
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
      setActiveBatch(null);
    }
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
    setReviewQueue(null);
    setError(null);
    setCloseConfirm({ kind: "none" });
  }, [activeBatch]);

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
        // M4.7: edits don't affect review status today, but rescan so the
        // badge stays accurate if the gate is re-run in the future.
        scheduleRescan();
      } catch (e) {
        flashError(`Edit failed: ${formatError(e)}`);
      }
    },
    [
      activeCatalogPath,
      replaceUnit,
      markDirty,
      markCatalogDirty,
      scheduleRescan,
      flashError,
    ],
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
        // M4.7: translation may add flags → rescan the review queue.
        scheduleRescan();
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
      scheduleRescan,
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
      // M4.7: state transitions on save may change the review queue.
      scheduleRescan();
      flashInfo(
        `Saved ${summary.unit_count} units to ${shortenPath(summary.path)}`,
      );
    } catch (e) {
      flashError(`Save failed: ${formatError(e)}`);
    }
  }, [
    catalog,
    activeCatalogPath,
    dirtyIds,
    scheduleRescan,
    flashInfo,
    flashError,
  ]);

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
        // M4.7: state transitions on save may change the review queue.
        scheduleRescan();
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
  }, [
    dirtyCatalogPaths,
    activeCatalogPath,
    scheduleRescan,
    flashInfo,
    flashError,
  ]);

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
      // M4.7: discard resets state from disk; review queue may change.
      scheduleRescan();
      flashInfo("Reverted to disk state.");
    } catch (e) {
      flashError(`Discard failed: ${formatError(e)}`);
    }
  }, [
    catalog,
    activeCatalogPath,
    dirtyIds,
    selectedId,
    scheduleRescan,
    flashInfo,
    flashError,
  ]);

  // ── Accept (M4.6.2) ─────────────────────────────────────────────────────────

  const onAccept = useCallback(
    async (id: UnitId) => {
      if (!activeCatalogPath) return;
      setBusyIds((prev) => new Set(prev).add(id));
      try {
        const updated = await acceptUnitInProject(activeCatalogPath, id);
        replaceUnit(updated);
        markCatalogDirty(activeCatalogPath);
        // Codex P1: onSave / onDiscard are gated by dirtyIds.size === 0, so
        // without adding the unit here Ctrl-S and Discard would become no-ops
        // after Accept on an otherwise clean catalog.
        setDirtyIds((prev) => {
          const next = new Set(prev);
          next.add(id);
          return next;
        });
        // M4.7: Accept clears flags → unit leaves the review queue.
        scheduleRescan();
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
    [
      activeCatalogPath,
      replaceUnit,
      markCatalogDirty,
      scheduleRescan,
      flashInfo,
      flashError,
    ],
  );

  // ── Batch translate (M4.8) ───────────────────────────────────────────────

  // Called from the "Translate all" button in CatalogList.
  const onTranslateAll = useCallback(
    async (scope: BatchScope) => {
      if (!activeCatalogPath) return;

      // Derive a display name from the catalog path (basename only).
      const catalogName =
        activeCatalogPath.replace(/\\/g, "/").split("/").pop() ??
        activeCatalogPath;

      // Start the batch first to get the server-assigned job_id (we cannot
      // subscribe to the per-job event channels before knowing the id).
      // After receiving the id, we subscribe before arming the UI state so
      // the handlers exist as early as possible. A stuck-batch safety timer
      // handles the unlikely case where the terminal event was emitted in
      // the gap between the Rust handler spawning the worker and our
      // `listenBatchProgress` call resolving (possible with instant backends;
      // impossible with Ollama's network latency). Tracked for a cleaner fix
      // in M4.8.1 (reserve-job-id command so subscribe can precede start).
      let started: { job_id: string; total: number };
      try {
        started = await translateBatchInProject(activeCatalogPath, scope);
      } catch (e) {
        flashError(`Translate all failed to start: ${formatError(e)}`);
        return;
      }

      const { job_id: jobId, total } = started;

      // Subscribe to events BEFORE we set activeBatch so the handlers are
      // registered before any state transitions.
      //
      // Race note: the worker thread is spawned inside the Rust handler before
      // the IPC response is serialised and delivered to the JS side. For
      // low-latency backends (e.g. the `manual` backend used in tests) the
      // worker could complete and emit the terminal event before our
      // `listenBatchProgress` call resolves. To guard against a stuck-batch,
      // we install a timeout that clears the batch state if no terminal event
      // arrives within 10 s of the subscribe call completing. The timeout is
      // cancelled the moment any terminal event lands.
      //
      // A proper fix requires a Rust-side protocol change (reserve a job slot
      // and return the job_id before starting the worker; tracked in M4.8.1).
      let terminalReceived = false;
      const stuckGuardMs = 10_000;
      let stuckGuardTimer: ReturnType<typeof setTimeout> | null = null;

      // Capture activeCatalogPath in a local for the callbacks — the React
      // state captured in closures may be stale if the user navigates.
      const batchCatalogPath = activeCatalogPath;

      const unlisten = await listenBatchProgress(
        jobId,
        // onProgress
        (payload) => {
          // Any progress event means the terminal was not missed.
          terminalReceived = true;
          if (stuckGuardTimer !== null) {
            clearTimeout(stuckGuardTimer);
            stuckGuardTimer = null;
          }

          // Update batch progress UI.
          setActiveBatch((prev) => {
            if (!prev || prev.jobId !== jobId) return prev;
            const recent = [payload.unit.id, ...prev.recent].slice(0, 3);
            return { ...prev, completed: payload.completed, recent };
          });

          // Merge the translated unit into the cached catalog.
          setOpenCatalogs((prev) => {
            const entry = prev.get(batchCatalogPath);
            if (!entry) return prev; // catalog was removed mid-batch — ignore
            const next = new Map(prev);
            next.set(batchCatalogPath, {
              ...entry,
              units: entry.units.map((u) =>
                u.id === payload.unit.id ? payload.unit : u,
              ),
            });
            return next;
          });

          // Mark the unit + catalog dirty.
          setDirtyIds((prev) => {
            if (prev.has(payload.unit.id)) return prev;
            const next = new Set(prev);
            next.add(payload.unit.id);
            return next;
          });
          markCatalogDirty(batchCatalogPath);
        },
        // onTerminal
        (payload, status) => {
          terminalReceived = true;
          if (stuckGuardTimer !== null) {
            clearTimeout(stuckGuardTimer);
            stuckGuardTimer = null;
          }
          // Tear down listeners.
          if (unlistenRef.current) {
            unlistenRef.current();
            unlistenRef.current = null;
          }
          setActiveBatch(null);

          // Toast with outcome.
          if (status === "failed" && payload.failed_reason) {
            flashError(
              `Batch failed at ${payload.completed} of ${payload.total}: ${payload.failed_reason}`,
            );
          } else if (payload.cancelled) {
            flashInfo(
              `Batch cancelled at ${payload.completed} of ${payload.total} units.`,
            );
          } else {
            flashInfo(`Translated ${payload.completed} units.`);
          }

          // Rescan review queue to pick up newly-flagged units.
          scheduleRescan();
        },
      );

      // Stash so we can call it on cleanup or user-cancel.
      unlistenRef.current = unlisten;

      // Now that listeners are registered, initialise the batch UI state.
      setActiveBatch({
        jobId,
        catalogPath: activeCatalogPath,
        catalogName,
        completed: 0,
        total,
        recent: [],
      });

      // Arm the stuck-batch guard: if the terminal event never arrives
      // (missed before subscribe completed), clear the batch widget after
      // stuckGuardMs so the UI is not permanently blocked.
      if (!terminalReceived) {
        stuckGuardTimer = setTimeout(() => {
          stuckGuardTimer = null;
          if (!terminalReceived) {
            // Terminal was missed — clean up defensively.
            if (unlistenRef.current) {
              unlistenRef.current();
              unlistenRef.current = null;
            }
            setActiveBatch(null);
            flashError(
              `Batch for ${catalogName} may have completed before listeners were ready. Check the catalog for translated units.`,
            );
            scheduleRescan();
          }
        }, stuckGuardMs);
      }
    },
    [
      activeCatalogPath,
      markCatalogDirty,
      scheduleRescan,
      flashInfo,
      flashError,
    ],
  );

  // Cancel any in-flight batch.
  const onCancelBatch = useCallback(() => {
    if (!activeBatch) return;
    // Fire-and-forget: the terminal event will arrive asynchronously.
    // The UI stays in "batch active" state until then.
    cancelTranslation(activeBatch.jobId).catch(() => {
      // If cancel itself errors, we still wait for the terminal event.
    });
  }, [activeBatch]);

  // Cleanup on unmount: cancel any in-flight batch and unsubscribe.
  // Uses activeBatchRef to avoid a stale-closure over the activeBatch state.
  useEffect(() => {
    return () => {
      const batch = activeBatchRef.current;
      if (batch) {
        cancelTranslation(batch.jobId).catch(() => {});
      }
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
    };
  }, []);

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

  // ── Review queue navigation ───────────────────────────────────────────────

  // Called from the ReviewQueue table's "Open" button.
  // Switches to the target catalog, selects the unit, and navigates to the
  // Translate view so the editor is visible.
  const onOpenReviewQueueItem = useCallback(
    async (catalogPath: string, unitId: UnitId) => {
      await handleCatalogSelect(catalogPath);
      setSelectedId(unitId);
      setProjectView("translate");
    },
    [handleCatalogSelect],
  );

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
  const reviewQueueTotal = reviewQueue?.total_count ?? 0;
  const reviewQueueByCatalog = reviewQueue?.by_catalog ?? {};

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
        reviewQueueCount={reviewQueueTotal}
      />

      <div className="flex-1 flex overflow-hidden min-h-0">
        {/* Sidebar — always visible in project mode */}
        <ProjectSidebar
          summary={summary}
          activeCatalogPath={activeCatalogPath}
          dirtyCatalogPaths={dirtyCatalogPaths}
          activeLocaleFilter={activeLocaleFilter}
          onCatalogSelect={handleCatalogSelect}
          reviewQueueTotal={reviewQueueTotal}
          reviewQueueByCatalog={reviewQueueByCatalog}
          onOpenReviewQueue={() => setProjectView("review")}
          openCatalogs={openCatalogs}
        />

        {/* Main content area */}
        <div className="flex-1 flex flex-col overflow-hidden min-h-0">
          {/* Overview view */}
          {projectView === "overview" && (
            <OverviewPanel
              summary={summary}
              openCatalogs={openCatalogs}
              dirtyCatalogPaths={dirtyCatalogPaths}
              focusLocale={focusLocale}
              setFocusLocale={setFocusLocale}
              setProjectView={setProjectView}
            />
          )}

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
                  onTranslateAll={onTranslateAll}
                  batchActive={activeBatch !== null}
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

          {/* Review queue view — M4.7 */}
          <div
            className={
              projectView === "review" ? "flex-1 flex min-h-0" : "hidden"
            }
          >
            {reviewQueue ? (
              <ReviewQueue
                reviewQueue={reviewQueue}
                onOpenItem={onOpenReviewQueueItem}
              />
            ) : (
              <div className="flex-1 flex items-center justify-center text-sm text-fg-tertiary">
                <p>Loading review queue…</p>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Footer bar — always visible in project mode; shows batch progress or catalog/save summary */}
      {(projectView === "translate" || activeBatch !== null) && (
        <footer className="shrink-0 h-9 px-4 flex items-center justify-between gap-4 border-t border-border-subtle bg-bg-surface text-xs text-fg-tertiary">
          {activeBatch ? (
            /* Batch in-flight: show progress widget across full footer width */
            <BatchProgressWidget batch={activeBatch} onCancel={onCancelBatch} />
          ) : (
            /* Normal Translate view footer */
            <>
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
            </>
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
