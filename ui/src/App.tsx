import type { UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import type { ActiveBatch } from "./components/BatchProgressWidget/BatchProgressWidget";
import { BatchProgressWidget } from "./components/BatchProgressWidget/BatchProgressWidget";
import { GlossaryPanel } from "./components/GlossaryPanel/GlossaryPanel";
import { HomeScreen, pushRecent } from "./components/HomeScreen/HomeScreen";
import { OverviewPanel } from "./components/OverviewPanel/OverviewPanel";
import { ProjectSettings } from "./components/ProjectSettings/ProjectSettings";
import { ProjectSidebar } from "./components/ProjectSidebar/ProjectSidebar";
import { QualityPanel } from "./components/QualityPanel/QualityPanel";
import { ReviewPanel } from "./components/ReviewPanel";
import type { ProjectView } from "./components/TopBar/TopBar";
import { ProjectTopBar } from "./components/TopBar/TopBar";
import { TranslatePanel } from "./components/TranslatePanel";
import type {
  PairJobHandle,
  PairProgress,
  PairTerminal,
  ScopePair,
} from "./components/TranslatePanel/RunOnScopeModal";
import type { UnitEditorHandle } from "./components/TranslatePanel/UnitEditor/UnitEditor";
import type { CommitNowFn, CommitRegistry } from "./lib/pending-commits";
import { PendingCommitsContext } from "./lib/pending-commits";
import { basenameOf } from "./lib/reuse";
import {
  acceptUnitInProject,
  cancelTranslation,
  closeProject,
  currentProjectSummary,
  discardChangesInProject,
  gateCatalogInProject,
  mergeCatalogs,
  openCatalogInProject,
  pickReferenceFiles,
  pickRemainderFile,
  pickTsSaveLocation,
  reuseReferencesInProject,
  saveAllDirty,
  saveCatalogInProject,
  scanProjectReviewState,
  splitRemainder,
  translateBatchInProject,
  translateUnitInProject,
  updateUnitTargetInProject,
} from "./lib/tauri";
import { listenBatchProgress } from "./lib/tauri-events";
import { useTheme } from "./lib/theme";
import type {
  BatchScope,
  CatalogGateStats,
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
  // Per-catalog gate statistics, keyed by absolute path. Populated when a
  // catalog is opened/switched and re-gated; drives the per-file stats view.
  const [catalogStats, setCatalogStats] = useState<
    Record<string, CatalogGateStats>
  >({});
  const [busyIds, setBusyIds] = useState<Set<UnitId>>(new Set());
  const [toast, setToast] = useState<Toast | null>(null);

  // ── Batch translate state ────────────────────────────────────────────────
  // Single active batch slot; the Rust side enforces one job per (catalog,
  // locale) pair — if a second start arrives for the same pair, the IPC call
  // errors and we toast it. The UI only tracks one batch at a time (the one
  // that is running most recently).
  const [activeBatch, setActiveBatch] = useState<ActiveBatch | null>(null);
  // True while saveAllDirty is in-flight; drives the topbar Save button spinner.
  const [isSaving, setIsSaving] = useState(false);
  const unlistenRef = useRef<UnlistenFn | null>(null);
  // Mirror of activeBatch kept in a ref so the unmount cleanup can read the
  // latest value without capturing a stale closure.
  const activeBatchRef = useRef<ActiveBatch | null>(null);
  useEffect(() => {
    activeBatchRef.current = activeBatch;
  });

  // ── Reference reuse / split / merge ───────────────────────────────────────
  // Absolute paths with a reuse/split/merge IPC call in flight (drives the
  // per-catalog actions-menu spinner).
  const [reuseBusyPaths, setReuseBusyPaths] = useState<Set<string>>(new Set());

  // ── Review queue ──────────────────────────────────────────────────────────
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

  // Pending-commit registry. Every matrix/focus textarea registers a
  // commit-now callback on mount; onSaveAll calls flushAll BEFORE the IPC
  // save so that drafts typed without a subsequent blur (the Cmd-S case)
  // make it to the Rust side first. Tracked in-flight IPC promises are
  // also awaited so a save cannot race past a still-pending edit. The
  // registry is created once and never re-allocated — the Set lives in
  // a ref so commit-now thunks keep their identity across renders.
  const pendingCommitsRef = useRef<Set<CommitNowFn>>(new Set());
  const inFlightIpcRef = useRef<Set<Promise<unknown>>>(new Set());
  const commitRegistry = useMemo<CommitRegistry>(
    () => ({
      register: (commit: CommitNowFn) => {
        pendingCommitsRef.current.add(commit);
        return () => {
          pendingCommitsRef.current.delete(commit);
        };
      },
      trackInFlight: (promise: Promise<unknown>) => {
        inFlightIpcRef.current.add(promise);
        // Use .finally so both fulfilled and rejected promises remove
        // themselves — leaking a rejected promise would block every
        // future Save All forever. The IPC trampolines (onEditUnitFor
        // etc.) all `.catch` internally and call flashError, so the
        // promises tracked here resolve to undefined. The .catch below
        // is paranoia: any caller that hands us a rejecting promise
        // would otherwise crash the page with an unhandled rejection.
        void promise
          .catch(() => {})
          .finally(() => {
            inFlightIpcRef.current.delete(promise);
          });
      },
      flushAll: async () => {
        const commits = Array.from(pendingCommitsRef.current);
        // Promise.allSettled keeps a slow/failed commit from blocking
        // the other textareas — Save All still proceeds.
        await Promise.allSettled(commits.map((c) => Promise.resolve(c())));
        const pending = Array.from(inFlightIpcRef.current);
        await Promise.allSettled(pending);
      },
    }),
    [],
  );

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
      setSearch("");
      setDirtyIds(new Set());
      setDirtyCatalogPaths(new Set());
      setReports({});
      setCatalogStats({});
      setBusyIds(new Set());
      setProjectView("overview");
      setFocusLocale(null);
      setActiveLocaleFilter(new Set());
      setError(null);
      setCloseConfirm({ kind: "none" });
      setReuseBusyPaths(new Set());
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
    setCatalogStats({});
    setReviewQueue(null);
    setError(null);
    setCloseConfirm({ kind: "none" });
    setReuseBusyPaths(new Set());
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
    // Same flush pattern as onSaveAll — pending textarea drafts and in-flight
    // IPC promises must land before we save, otherwise Save & Close abandons
    // the unflushed text. See onSaveAll for the full rationale.
    try {
      await commitRegistry.flushAll();
    } catch {
      // Best-effort — per-cell errors are reported as toasts.
    }
    try {
      await editorRef.current?.flushPendingEdit();
    } catch {
      // Best-effort flush; continue with save.
    }
    setIsSaving(true);
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
    } finally {
      setIsSaving(false);
    }
    await _doCloseProject();
  }, [_doCloseProject, flashError, commitRegistry]);

  // "Discard & Close" — close immediately, dropping all unsaved state.
  const handleDiscardAndClose = useCallback(() => {
    void _doCloseProject();
  }, [_doCloseProject]);

  // ── Project-mode catalog open ─────────────────────────────────────────────

  // Re-run the validation gate over an open catalog to refresh its per-file
  // stats and, when it is the active catalog, the per-unit reports the
  // inspector renders. The gate is advisory: any failure is swallowed so it
  // never blocks opening or switching catalogs.
  const refreshCatalogGate = useCallback(
    async (absPath: string, makeActive: boolean) => {
      try {
        const result = await gateCatalogInProject(absPath);
        setCatalogStats((prev) => ({ ...prev, [absPath]: result.stats }));
        if (makeActive) {
          const map: Record<UnitId, GateReport> = {};
          for (const r of result.reports) map[r.unit_id] = r;
          setReports(map);
        }
      } catch {
        // Advisory only — leave stats/reports untouched on failure.
      }
    },
    [],
  );

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
        void refreshCatalogGate(absPath, true);
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
        setSearch("");
        setDirtyIds(new Set());
        setReports({});
        setBusyIds(new Set());
        void refreshCatalogGate(absPath, true);
      } catch (e) {
        setError(formatError(e));
        flashError(`Could not open catalog: ${formatError(e)}`);
      } finally {
        setLoading(false);
      }
    },
    [activeCatalogPath, openCatalogs, flashError, refreshCatalogGate],
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
        // Edits don't affect review status today, but rescan so the
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
      // Force the busy-state render to commit synchronously BEFORE the IPC
      // starts. Without flushSync, React batches the busy=true commit with
      // the downstream state updates that fire after the await, so the
      // spinner renders only at the very end of the round-trip. Same fix
      // applied to onTranslateUnitFor and onAcceptUnitFor below.
      flushSync(() => {
        setBusyIds((prev) => new Set(prev).add(id));
      });
      try {
        const result = await translateUnitInProject(activeCatalogPath, id);
        replaceUnit(result.unit);
        setReports((prev) => ({ ...prev, [id]: result.report }));
        markDirty(id);
        markCatalogDirty(activeCatalogPath);
        // Translation may add flags — rescan the review queue.
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
    // Flush every registered matrix/focus textarea draft and every still-
    // pending IPC promise FIRST. The `dirtyIds.size === 0` early-return
    // below is otherwise the bug — typed-but-not-blurred edits would never
    // mark the unit dirty, the save would skip, and the disk file would
    // stay stale. See onSaveAll for the full rationale.
    try {
      await commitRegistry.flushAll();
    } catch {
      // Best-effort — per-cell errors are surfaced as toasts.
    }
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
      // State transitions on save may change the review queue.
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
    commitRegistry,
    scheduleRescan,
    flashInfo,
    flashError,
  ]);

  const onSaveAll = useCallback(async () => {
    // Flush every matrix/focus textarea's uncommitted draft AND every
    // still-pending IPC promise BEFORE deciding the save is a no-op. The
    // React-side `dirtyCatalogPaths` set is the UI's view of dirty state,
    // but it lags the Rust store by one render after a commit lands. We
    // deliberately do NOT short-circuit on `dirtyCatalogPaths.size === 0`
    // here: that gate would skip the save for the Cmd-S-without-blur case
    // because the draft has not yet committed when the user pressed Save.
    // `saveAllDirty()` is cheap when the Rust store has no dirty entries
    // (it returns an empty `saved` list and no error).
    try {
      await commitRegistry.flushAll();
    } catch {
      // Best-effort — individual commit errors are reported per-cell.
    }
    try {
      await editorRef.current?.flushPendingEdit();
    } catch {
      // Best-effort flush; continue saving.
    }
    setIsSaving(true);
    try {
      const resp = await saveAllDirty();
      const savedCount = resp.saved.length;
      if (savedCount > 0) {
        setDirtyCatalogPaths((prev) => {
          const next = new Set(prev);
          for (const s of resp.saved) next.delete(s.path);
          return next;
        });
        // State transitions on save may change the review queue.
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
    } finally {
      setIsSaving(false);
    }
  }, [
    activeCatalogPath,
    scheduleRescan,
    flashInfo,
    flashError,
    commitRegistry,
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
      // Discard resets state from disk; review queue may change.
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

  // ── Accept ───────────────────────────────────────────────────────────────────

  const onAccept = useCallback(
    async (id: UnitId) => {
      if (!activeCatalogPath) return;
      flushSync(() => {
        setBusyIds((prev) => new Set(prev).add(id));
      });
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
        // Accept clears flags — unit leaves the review queue.
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

  // ── Matrix-mode multi-catalog IPC helpers ────────────────────────────────
  //
  // The legacy onEdit / onTranslate / onAccept above assume the change
  // applies to `activeCatalogPath`. Matrix mode operates across every
  // per-locale catalog at once, so we expose `*For` variants that take an
  // explicit catalog path. Each merges into the shared `openCatalogs` /
  // `dirtyCatalogPaths` state machine so save / discard / batch all keep
  // working on the cells the matrix touched.

  // Merge an updated unit into the cached CatalogResponse for an arbitrary
  // catalog path (matrix mode — not bound to activeCatalogPath).
  const replaceUnitFor = useCallback((absPath: string, updated: Unit) => {
    setOpenCatalogs((prev) => {
      const entry = prev.get(absPath);
      if (!entry) return prev;
      const next = new Map(prev);
      next.set(absPath, {
        ...entry,
        units: entry.units.map((u) => (u.id === updated.id ? updated : u)),
      });
      return next;
    });
  }, []);

  // Open a project catalog into the cache without touching active selection.
  // Concurrent calls for the same path are safe — the setter is a final-
  // write-wins merge keyed on the absolute path.
  const ensureCatalogLoaded = useCallback(
    async (absPath: string) => {
      if (openCatalogs.has(absPath)) return;
      try {
        const response = await openCatalogInProject(absPath);
        setOpenCatalogs((prev) =>
          prev.has(absPath) ? prev : new Map(prev).set(absPath, response),
        );
      } catch (e) {
        // Surface the failure via toast but do not throw — the matrix
        // simply renders the locale's cells as "loading…" placeholders.
        flashError(`Could not open catalog: ${formatError(e)}`);
      }
    },
    [openCatalogs, flashError],
  );

  const onEditUnitFor = useCallback(
    (absPath: string, unit: Unit, edit: TargetEdit) => {
      // Track the IPC promise so onSaveAll's flushAll can await it. Without
      // this, a blur that fires immediately before Cmd-S could race the
      // save: dirty state has not yet propagated to the Rust store when
      // saveAllDirty walks it.
      const p = (async () => {
        try {
          const updated = await updateUnitTargetInProject(
            absPath,
            unit.id,
            edit,
          );
          replaceUnitFor(absPath, updated);
          markDirty(updated.id);
          markCatalogDirty(absPath);
          scheduleRescan();
        } catch (e) {
          flashError(`Edit failed: ${formatError(e)}`);
        }
      })();
      commitRegistry.trackInFlight(p);
    },
    [
      replaceUnitFor,
      markDirty,
      markCatalogDirty,
      scheduleRescan,
      flashError,
      commitRegistry,
    ],
  );

  const onTranslateUnitFor = useCallback(
    (absPath: string, unit: Unit) => {
      const id = unit.id;
      flushSync(() => {
        setBusyIds((prev) => new Set(prev).add(id));
      });
      void (async () => {
        try {
          const result = await translateUnitInProject(absPath, id);
          replaceUnitFor(absPath, result.unit);
          setReports((prev) => ({ ...prev, [id]: result.report }));
          markDirty(id);
          markCatalogDirty(absPath);
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
      })();
    },
    [
      replaceUnitFor,
      markDirty,
      markCatalogDirty,
      scheduleRescan,
      flashInfo,
      flashError,
    ],
  );

  const onAcceptUnitFor = useCallback(
    (absPath: string, unit: Unit) => {
      const id = unit.id;
      flushSync(() => {
        setBusyIds((prev) => new Set(prev).add(id));
      });
      void (async () => {
        try {
          const updated = await acceptUnitInProject(absPath, id);
          replaceUnitFor(absPath, updated);
          markCatalogDirty(absPath);
          setDirtyIds((prev) => {
            if (prev.has(id)) return prev;
            const next = new Set(prev);
            next.add(id);
            return next;
          });
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
      })();
    },
    [replaceUnitFor, markCatalogDirty, scheduleRescan, flashInfo, flashError],
  );

  // ── Batch translate ──────────────────────────────────────────────────────

  // Called from the "Translate all" button in CatalogList (focus-mode
  // fallback) and the "Run model on selected" button in Matrix mode. The
  // matrix dispatches one call per locale that has work; the existing
  // single-slot stuck guard on the Rust side serialises them.
  const onTranslateAll = useCallback(
    async (catalogPath: string, scope: BatchScope) => {
      if (!catalogPath) return;

      // Derive a display name from the catalog path (basename only).
      const catalogName =
        catalogPath.replace(/\\/g, "/").split("/").pop() ?? catalogPath;

      // Start the batch first to get the server-assigned job_id (we cannot
      // subscribe to the per-job event channels before knowing the id).
      // After receiving the id, we subscribe before arming the UI state so
      // the handlers exist as early as possible. A stuck-batch safety timer
      // handles the unlikely case where the terminal event was emitted in
      // the gap between the Rust handler spawning the worker and our
      // `listenBatchProgress` call resolving (possible with instant backends;
      // impossible with Ollama's network latency). Tracked for a cleaner fix
      // tracked as a follow-up (reserve-job-id command so subscribe can precede start).
      let started: { job_id: string; total: number };
      try {
        started = await translateBatchInProject(catalogPath, scope);
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
      // and return the job_id before starting the worker; tracked as a follow-up).
      let terminalReceived = false;
      const stuckGuardMs = 10_000;
      let stuckGuardTimer: ReturnType<typeof setTimeout> | null = null;

      // The path passed in is already captured; alias for clarity in callbacks.
      const batchCatalogPath = catalogPath;

      // Track which unit ids this batch has announced via onUnitStart so that
      // the terminal handler can defensively clear them all from busyIds even
      // if a progress event was missed.
      const batchStartedIds = new Set<string>();

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

          // Unit completed — remove it from the spinner set.
          setBusyIds((prev) => {
            if (!prev.has(payload.unit.id)) return prev;
            const next = new Set(prev);
            next.delete(payload.unit.id);
            return next;
          });

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

          // Defensively clear any unit ids this batch announced from busyIds.
          // Handles the case where a progress event was dropped (e.g. instant
          // backend completing before a React render cycle fires).
          if (batchStartedIds.size > 0) {
            setBusyIds((prev) => {
              const next = new Set(prev);
              for (const id of batchStartedIds) next.delete(id);
              return next;
            });
          }

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
        // onUnitStart — adds the unit to the spinner set so its matrix cell
        // and focus row show a busy indicator before the translation completes.
        (payload) => {
          batchStartedIds.add(payload.unit_id);
          setBusyIds((prev) => {
            const next = new Set(prev);
            next.add(payload.unit_id);
            return next;
          });
        },
      );

      // Stash so we can call it on cleanup or user-cancel.
      unlistenRef.current = unlisten;

      // Capture the wall-clock time now that listeners are registered and
      // the batch is about to be displayed. Used for ETA computation.
      const startedAt = Date.now();

      // Now that listeners are registered, initialise the batch UI state.
      setActiveBatch({
        jobId,
        catalogPath,
        catalogName,
        completed: 0,
        total,
        recent: [],
        startedAt,
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
    [markCatalogDirty, scheduleRescan, flashInfo, flashError],
  );

  // ── Per-pair batch starter for RunOnScopeModal ──────────────────────────
  //
  // Mirrors the `onTranslateAll` machinery but is shaped for the modal:
  // starts one (catalogPath, "untranslated") batch, subscribes to its events,
  // updates the catalog cache on progress, and returns a `PairJobHandle` so
  // the modal can track per-pair progress and cancel individual jobs.
  //
  // The stuck-batch guard is armed per job; the guard fires `onTerminal` with
  // a synthetic "stuck" error so the modal row reflects the failure.
  const startBatchForPair = useCallback(
    async (
      pair: ScopePair,
      onPairProgress: (p: PairProgress) => void,
      onPairTerminal: (t: PairTerminal) => void,
    ): Promise<PairJobHandle> => {
      const { catalogPath } = pair;

      // Start the batch to get the server-assigned job_id.
      let started: { job_id: string; total: number };
      try {
        started = await translateBatchInProject(catalogPath, "untranslated");
      } catch (e) {
        throw new Error(`Failed to start: ${formatError(e)}`);
      }
      const { job_id: jobId, total } = started;

      const batchCatalogPath = catalogPath;
      let terminalReceived = false;
      const stuckGuardMs = 10_000;
      let stuckGuardTimer: ReturnType<typeof setTimeout> | null = null;
      const batchStartedIds = new Set<string>();

      const unlisten = await listenBatchProgress(
        jobId,
        // onProgress
        (payload) => {
          terminalReceived = true;
          if (stuckGuardTimer !== null) {
            clearTimeout(stuckGuardTimer);
            stuckGuardTimer = null;
          }

          // Clear the unit from busyIds.
          setBusyIds((prev) => {
            if (!prev.has(payload.unit.id)) return prev;
            const next = new Set(prev);
            next.delete(payload.unit.id);
            return next;
          });

          // Merge translated unit into catalog cache.
          setOpenCatalogs((prev) => {
            const entry = prev.get(batchCatalogPath);
            if (!entry) return prev;
            const next = new Map(prev);
            next.set(batchCatalogPath, {
              ...entry,
              units: entry.units.map((u) =>
                u.id === payload.unit.id ? payload.unit : u,
              ),
            });
            return next;
          });

          // Mark unit + catalog dirty.
          setDirtyIds((prev) => {
            if (prev.has(payload.unit.id)) return prev;
            const next = new Set(prev);
            next.add(payload.unit.id);
            return next;
          });
          markCatalogDirty(batchCatalogPath);

          // Forward to modal.
          onPairProgress({
            completed: payload.completed,
            total: payload.total,
          });
        },
        // onTerminal
        (payload, status) => {
          terminalReceived = true;
          if (stuckGuardTimer !== null) {
            clearTimeout(stuckGuardTimer);
            stuckGuardTimer = null;
          }
          unlisten?.();

          // Clear busy ids for this job's units.
          if (batchStartedIds.size > 0) {
            setBusyIds((prev) => {
              const next = new Set(prev);
              for (const id of batchStartedIds) next.delete(id);
              return next;
            });
          }

          scheduleRescan();

          onPairTerminal({
            completed: payload.completed,
            total: payload.total,
            cancelled: payload.cancelled,
            failedReason:
              status === "failed" ? (payload.failed_reason ?? null) : null,
          });
        },
        // onUnitStart
        (payload) => {
          console.log("[batch] unit-started:", payload.unit_id);
          batchStartedIds.add(payload.unit_id);
          setBusyIds((prev) => {
            const next = new Set(prev);
            next.add(payload.unit_id);
            return next;
          });
        },
      );

      // Arm the stuck-batch guard: if no terminal event arrives within
      // stuckGuardMs, synthesise a failure so the modal row is not stuck.
      if (!terminalReceived) {
        stuckGuardTimer = setTimeout(() => {
          stuckGuardTimer = null;
          if (!terminalReceived) {
            unlisten?.();
            onPairTerminal({
              completed: 0,
              total,
              cancelled: false,
              failedReason:
                "No progress received — batch may have completed before listeners were ready.",
            });
            scheduleRescan();
          }
        }, stuckGuardMs);
      }

      const handle: PairJobHandle = {
        jobId,
        cancel: () => {
          cancelTranslation(jobId).catch(() => {});
        },
      };
      return handle;
    },
    [markCatalogDirty, scheduleRescan],
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
        // ⌘S in matrix mode (no single "active catalog") routes to Save All
        // so a user editing across several locales does not see a no-op.
        if (e.shiftKey || !activeCatalogPath) {
          void onSaveAll();
        } else {
          void onSave();
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [mode, activeCatalogPath, onSave, onSaveAll]);

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

  // ── Reference reuse / remainder split / merge ─────────────────────────────
  //
  // These run synchronously (no model, no network). Each toggles the catalog
  // into reuseBusyPaths for the actions-menu spinner, then refreshes UI state
  // via the same paths an edit/translate/save uses.

  const markReuseBusy = useCallback((path: string, busy: boolean) => {
    setReuseBusyPaths((prev) => {
      const next = new Set(prev);
      if (busy) next.add(path);
      else next.delete(path);
      return next;
    });
  }, []);

  const onApplyReferences = useCallback(
    async (catalogPath: string) => {
      markReuseBusy(catalogPath, true);
      try {
        let report = await reuseReferencesInProject(catalogPath, null).catch(
          (e) => {
            // No manifest references for the locale → fall back to an ad-hoc
            // multi-select picker so the user can point at reference .ts files.
            const msg = formatError(e);
            if (!msg.includes("no references available")) throw e;
            return null;
          },
        );

        if (report === null) {
          const picked = await pickReferenceFiles();
          if (!picked || picked.length === 0) {
            flashInfo("Apply references cancelled — no files selected.");
            return;
          }
          report = await reuseReferencesInProject(catalogPath, picked);
        }

        // Re-pull the post-reuse catalog (the backend refreshed it in place but
        // does not return units) and merge it into the cache.
        const refreshed = await openCatalogInProject(catalogPath);
        setOpenCatalogs((prev) => new Map(prev).set(catalogPath, refreshed));
        if (catalogPath === activeCatalogPath) {
          const stillThere = refreshed.units.some((u) => u.id === selectedId);
          if (!stillThere) setSelectedId(refreshed.units[0]?.id ?? null);
        }

        // The review-queue scan now surfaces conflict units (with their
        // candidate detail on the review note), so a rescan refreshes the
        // Review conflict view. Reuse writes the base catalog to disk in
        // place, so the in-memory entry is clean — do NOT mark it dirty.
        scheduleRescan();

        const parts: string[] = [];
        if (report.copied_finished > 0)
          parts.push(
            `${report.copied_finished} auto-finished (gate-clean, complete)`,
          );
        if (report.copied_needs_review > 0)
          parts.push(
            `${report.copied_needs_review} proposed for review (soft-flagged or incomplete plural)`,
          );
        if (report.conflict_count > 0)
          parts.push(
            `${report.conflict_count} conflict(s) — resolve in Review`,
          );
        if (report.remaining_count > 0)
          parts.push(
            `${report.remaining_count} remaining (no match in references)`,
          );
        flashInfo(`References applied: ${parts.join("; ")}.`);
      } catch (e) {
        flashError(`Apply references failed: ${formatError(e)}`);
      } finally {
        markReuseBusy(catalogPath, false);
      }
    },
    [
      markReuseBusy,
      activeCatalogPath,
      selectedId,
      scheduleRescan,
      flashInfo,
      flashError,
    ],
  );

  const onExportRemainder = useCallback(
    async (catalogPath: string) => {
      const base = basenameOf(catalogPath).replace(/\.ts$/i, "");
      const outPath = await pickTsSaveLocation(
        "Export remainder",
        `${base}.remainder.ts`,
      );
      if (!outPath) return;
      markReuseBusy(catalogPath, true);
      try {
        // Always recompute the writable-untranslated set fresh from disk
        // (only_ids = null) so the export reflects the catalog's current state.
        // Conflicted units stay writable-but-untranslated and are correctly
        // included — they still need translation.
        const report = await splitRemainder(catalogPath, outPath, null);
        flashInfo(
          `Wrote ${report.kept_count} unit(s) to ${shortenPath(report.out_path)}.`,
        );
      } catch (e) {
        flashError(`Export remainder failed: ${formatError(e)}`);
      } finally {
        markReuseBusy(catalogPath, false);
      }
    },
    [markReuseBusy, flashInfo, flashError],
  );

  const onMergeCatalog = useCallback(
    async (catalogPath: string) => {
      const withPath = await pickRemainderFile();
      if (!withPath) return;
      const base = basenameOf(catalogPath).replace(/\.ts$/i, "");
      const outPath = await pickTsSaveLocation(
        "Save merged catalog",
        `${base}.merged.ts`,
      );
      if (!outPath) return;
      markReuseBusy(catalogPath, true);
      try {
        const report = await mergeCatalogs(catalogPath, withPath, outPath);
        flashInfo(
          `Merged ${report.merged} unit(s) (${report.merged_complete} complete) into ${shortenPath(report.out_path)}.`,
        );
      } catch (e) {
        // The backend rejects with a string naming the offending unit ids —
        // render it directly.
        flashError(`Merge failed: ${formatError(e)}`);
      } finally {
        markReuseBusy(catalogPath, false);
      }
    },
    [markReuseBusy, flashInfo, flashError],
  );

  // ── Conflict resolution: "Use this" applies a candidate via the edit path ──
  //
  // Picking a candidate is normal editing: write the candidate text into the
  // unit's target through the existing per-catalog edit IPC, then refresh the
  // cache. The translator then accepts/saves as usual, which clears the
  // conflict status. Plural candidates are written form-by-form.

  const onUseConflictCandidate = useCallback(
    async (catalogPath: string, unitId: UnitId, forms: string[]) => {
      try {
        await ensureCatalogLoaded(catalogPath);
        const entry = openCatalogs.get(catalogPath);
        const unit = entry?.units.find((u) => u.id === unitId);
        const isPlural = unit ? unit.plural_arity != null : forms.length > 1;

        let updated: Unit | null = null;
        if (isPlural) {
          for (let i = 0; i < forms.length; i++) {
            updated = await updateUnitTargetInProject(catalogPath, unitId, {
              kind: "plural",
              form_index: i,
              text: forms[i] ?? null,
            });
          }
        } else {
          updated = await updateUnitTargetInProject(catalogPath, unitId, {
            kind: "singular",
            text: forms[0] ?? null,
          });
        }

        if (updated) replaceUnitFor(catalogPath, updated);
        markDirty(unitId);
        markCatalogDirty(catalogPath);
        scheduleRescan();
        flashInfo(`Applied candidate to ${unitId}. Review and save to accept.`);
      } catch (e) {
        flashError(`Could not apply candidate: ${formatError(e)}`);
      }
    },
    [
      ensureCatalogLoaded,
      openCatalogs,
      replaceUnitFor,
      markDirty,
      markCatalogDirty,
      scheduleRescan,
      flashInfo,
      flashError,
    ],
  );

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
    <PendingCommitsContext.Provider value={commitRegistry}>
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
          unsavedCount={unsavedCatalogCount}
          saving={isSaving}
          onSave={onSaveAll}
        />

        <div className="flex-1 flex overflow-hidden min-h-0">
          {/* Workspace sidebar — rendered only for views without their own
            left rail. Translate (Matrix/Focus), Glossary (master-detail
            rail), Settings (long-form), and Review (sub-tab + side-nav)
            provide their own navigation and would render two stacked
            rails otherwise. */}
          {(projectView === "overview" || projectView === "quality") && (
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
              reuseBusyPaths={reuseBusyPaths}
              onApplyReferences={(p) => void onApplyReferences(p)}
              onExportRemainder={(p) => void onExportRemainder(p)}
              onMergeCatalog={(p) => void onMergeCatalog(p)}
            />
          )}

          {/* Main content area */}
          <div className="flex-1 flex flex-col overflow-hidden min-h-0">
            {/* Overview view */}
            {projectView === "overview" && (
              <OverviewPanel
                summary={summary}
                openCatalogs={openCatalogs}
                dirtyCatalogPaths={dirtyCatalogPaths}
                statsByCatalog={reviewQueue?.stats_by_catalog ?? {}}
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
              <TranslatePanel
                summary={summary}
                openCatalogs={openCatalogs}
                activeCatalogPath={activeCatalogPath}
                catalog={catalog}
                selectedId={selectedId}
                search={search}
                dirtyIds={dirtyIds}
                reports={reports}
                stats={
                  activeCatalogPath
                    ? (catalogStats[activeCatalogPath] ?? null)
                    : null
                }
                busyIds={busyIds}
                batchActive={activeBatch !== null}
                error={error}
                editorRef={editorRef}
                focusLocale={focusLocale}
                setFocusLocale={setFocusLocale}
                onSelect={setSelectedId}
                onSearchChange={setSearch}
                onEdit={onEditTarget}
                onTranslate={onTranslate}
                onAccept={onAccept}
                onEnsureCatalogLoaded={ensureCatalogLoaded}
                onTranslateUnitFor={onTranslateUnitFor}
                onEditUnitFor={onEditUnitFor}
                onAcceptUnitFor={onAcceptUnitFor}
                onTranslateAll={onTranslateAll}
                startBatchForPair={startBatchForPair}
              />
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
                  projectPath={summary.root}
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

            {/* Settings view — manifest editor */}
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

            {/* Quality view */}
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

            {/* Review panel — Queue + Proofread sub-tabs */}
            <div
              className={
                projectView === "review" ? "flex-1 flex min-h-0" : "hidden"
              }
            >
              <ReviewPanel
                summary={summary}
                openCatalogs={openCatalogs}
                reports={reports}
                reviewQueue={reviewQueue}
                onUseConflictCandidate={(catalogPath, unitId, forms) =>
                  void onUseConflictCandidate(catalogPath, unitId, forms)
                }
                onOpenItem={onOpenReviewQueueItem}
                onNavigateToUnit={async (catalogPath, unitId, locale) => {
                  await handleCatalogSelect(catalogPath);
                  setSelectedId(unitId);
                  if (locale) {
                    setFocusLocale(locale);
                  }
                  setProjectView("translate");
                }}
                onOpenHardFlags={() => setProjectView("review")}
                onEnsureCatalogLoaded={ensureCatalogLoaded}
                onToast={(message, kind) =>
                  kind === "error" ? flashError(message) : flashInfo(message)
                }
              />
            </div>
          </div>
        </div>

        {/* Footer bar — always visible in project mode; shows batch progress or catalog/save summary */}
        {(projectView === "translate" || activeBatch !== null) && (
          <footer className="shrink-0 h-9 px-4 flex items-center justify-between gap-4 border-t border-border-subtle bg-bg-surface text-xs text-fg-tertiary">
            {activeBatch ? (
              /* Batch in-flight: show progress widget across full footer width */
              <BatchProgressWidget
                batch={activeBatch}
                onCancel={onCancelBatch}
              />
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
          in project mode; the per-catalog discard action is wired and callable via
          keyboard). */}
        <span
          className="sr-only"
          aria-hidden="true"
          data-discard={String(!!onDiscard)}
        />
      </div>
    </PendingCommitsContext.Provider>
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
