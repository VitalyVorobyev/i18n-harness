// RunOnScopeModal — pre-flight preview + per-pair progress for "Translate
// untranslated".
//
// The modal has three phases:
//   preview  — table of (catalog, locale) pairs with untranslated counts;
//              skipped pairs (count = 0) shown faintly. Start / Cancel.
//   running  — per-pair ProgressBar bars; aggregated ETA; Cancel translation.
//   terminal — bars complete (or failed). Close.
//
// The modal does NOT call translateBatchInProject directly. It receives a
// `startPair` callback from App.tsx that fires the IPC + subscribes to events
// and streams progress back via the three event hooks.

import { useCallback, useEffect, useRef, useState } from "react";
import { cn } from "../../lib/cn";
import { ProgressBar } from "../primitives/ProgressBar";

// Key used for the per-pair terminal waiter map.
function pairKey(pair: ScopePair): string {
  return `${pair.catalogPath}::${pair.locale}`;
}

// ── Public types ──────────────────────────────────────────────────────────────

/** One (catalog, locale) pair in the scope preview. */
export interface ScopePair {
  /** Absolute path of the catalog file. */
  catalogPath: string;
  /** Basename of the catalog file, for display. */
  catalogName: string;
  /** Locale id (`de_DE`, `es_ES`, …). */
  locale: string;
  /** Number of untranslated units at preview time. 0 means skipped. */
  untranslatedCount: number;
}

/** Handle returned by `startPair`; used to cancel the batch. */
export interface PairJobHandle {
  jobId: string;
  cancel: () => void;
}

/** Progress event forwarded from the Rust worker for one pair. */
export interface PairProgress {
  completed: number;
  total: number;
}

/** Terminal event for one pair. */
export interface PairTerminal {
  completed: number;
  total: number;
  cancelled: boolean;
  failedReason: string | null;
}

/**
 * Callback contract that App.tsx implements and passes down.
 *
 * `startPair` starts a batch for one (catalogPath, locale) pair and
 * wires up the three streaming callbacks. It returns a `PairJobHandle`
 * with the server-assigned jobId and a cancel function.
 *
 * The App-side implementation follows the same subscribe-before-start
 * protocol used by the existing `onTranslateAll` in App.tsx and keeps
 * the stuck-batch guard per job.
 */
export type StartPairFn = (
  pair: ScopePair,
  onProgress: (p: PairProgress) => void,
  onTerminal: (t: PairTerminal) => void,
) => Promise<PairJobHandle>;

// ── Internal pair state ───────────────────────────────────────────────────────

type PairStatus =
  | { kind: "pending" }
  | { kind: "queued"; total: number }
  | { kind: "running"; jobId: string; completed: number; total: number }
  | { kind: "done"; completed: number; total: number }
  | { kind: "cancelled"; completed: number; total: number }
  | { kind: "failed"; completed: number; total: number; reason: string };

interface PairState {
  pair: ScopePair;
  status: PairStatus;
}

type ModalPhase = "preview" | "running" | "terminal";

// ── Component ─────────────────────────────────────────────────────────────────

interface Props {
  open: boolean;
  pairs: ScopePair[];
  /** Called when the user dismisses (Cancel in preview, Close after terminal). */
  onClose: () => void;
  /** App-supplied function that starts one batch and returns a cancel handle. */
  startPair: StartPairFn;
}

export function RunOnScopeModal({ open, pairs, onClose, startPair }: Props) {
  const [phase, setPhase] = useState<ModalPhase>("preview");
  const [pairStates, setPairStates] = useState<PairState[]>([]);
  // Cancel function for the currently-running pair (at most one at a time).
  const activeCancelRef = useRef<(() => void) | null>(null);
  // Set to true when the user clicks Cancel; the sequential loop checks this.
  const cancelledRef = useRef(false);
  // Per-pair terminal resolvers: resolved when a pair's onTerminal fires.
  const terminalResolversRef = useRef<Map<string, () => void>>(new Map());
  // Start wall-clock time for ETA computation.
  const startedAtRef = useRef<number | null>(null);

  // Reset state when the modal opens.
  useEffect(() => {
    if (open) {
      setPhase("preview");
      setPairStates(
        pairs.map((p) => ({ pair: p, status: { kind: "pending" } })),
      );
      activeCancelRef.current = null;
      cancelledRef.current = false;
      terminalResolversRef.current = new Map();
      startedAtRef.current = null;
    }
  }, [open, pairs]);

  // Derivations
  const activePairs = pairs.filter((p) => p.untranslatedCount > 0);
  const skippedPairs = pairs.filter((p) => p.untranslatedCount === 0);
  const totalUnits = activePairs.reduce(
    (sum, p) => sum + p.untranslatedCount,
    0,
  );

  // Running-phase aggregates
  const totalCompleted = pairStates.reduce((sum, ps) => {
    const s = ps.status;
    if (s.kind === "running") return sum + s.completed;
    if (s.kind === "done" || s.kind === "cancelled" || s.kind === "failed")
      return sum + s.completed;
    return sum;
  }, 0);

  const totalForRunning = pairStates.reduce((sum, ps) => {
    const s = ps.status;
    if (s.kind === "running") return sum + s.total;
    if (s.kind === "queued") return sum + s.total;
    if (s.kind === "done" || s.kind === "cancelled" || s.kind === "failed")
      return sum + s.total;
    return sum;
  }, 0);

  const allTerminal =
    pairStates.length > 0 &&
    pairStates.every((ps) => {
      const k = ps.status.kind;
      return k === "done" || k === "cancelled" || k === "failed";
    });

  // ETA: estimate based on elapsed time and completed fraction
  const etaLabel = useEta(
    phase === "running" ? totalCompleted : 0,
    phase === "running" ? totalForRunning : 0,
    phase === "running" ? startedAtRef.current : null,
  );

  // ── Handlers ──────────────────────────────────────────────────────────────

  const handleStart = useCallback(async () => {
    if (activePairs.length === 0) return;
    setPhase("running");
    startedAtRef.current = Date.now();
    cancelledRef.current = false;

    // Mark all active pairs as queued upfront so the table shows their totals.
    setPairStates((prev) => {
      const next = [...prev];
      for (const pair of activePairs) {
        const idx = pairs.indexOf(pair);
        const cur = next[idx];
        if (!cur) continue;
        next[idx] = {
          ...cur,
          status: { kind: "queued", total: pair.untranslatedCount },
        };
      }
      return next;
    });

    // Run one pair at a time. Each iteration awaits the pair's terminal event
    // before proceeding, which prevents concurrent Ollama inference calls.
    for (const pair of activePairs) {
      if (cancelledRef.current) break;

      const idx = pairs.indexOf(pair);

      // Build a promise that resolves when onTerminal fires for this pair.
      const terminalPromise = new Promise<void>((resolve) => {
        terminalResolversRef.current.set(pairKey(pair), resolve);
      });

      const onProgress = (p: PairProgress) => {
        setPairStates((prev) => {
          const next = [...prev];
          const cur = next[idx];
          if (!cur) return prev;
          next[idx] = {
            ...cur,
            status: {
              kind: "running",
              jobId: (cur.status as { jobId?: string }).jobId ?? "",
              completed: p.completed,
              total: p.total,
            },
          };
          return next;
        });
      };

      const onTerminal = (t: PairTerminal) => {
        setPairStates((prev) => {
          const next = [...prev];
          const cur = next[idx];
          if (!cur) return prev;
          const newStatus: PairStatus = t.failedReason
            ? {
                kind: "failed",
                completed: t.completed,
                total: t.total,
                reason: t.failedReason,
              }
            : t.cancelled
              ? { kind: "cancelled", completed: t.completed, total: t.total }
              : { kind: "done", completed: t.completed, total: t.total };
          next[idx] = { ...cur, status: newStatus };
          return next;
        });
        // Resolve the per-pair waiter so the outer loop can proceed.
        const resolve = terminalResolversRef.current.get(pairKey(pair));
        if (resolve) {
          terminalResolversRef.current.delete(pairKey(pair));
          resolve();
        }
      };

      try {
        const handle = await startPair(pair, onProgress, onTerminal);
        activeCancelRef.current = handle.cancel;
        // Update status to running with jobId
        setPairStates((prev) => {
          const next = [...prev];
          const cur = next[idx];
          if (!cur) return prev;
          next[idx] = {
            ...cur,
            status: {
              kind: "running",
              jobId: handle.jobId,
              completed: 0,
              total: pair.untranslatedCount,
            },
          };
          return next;
        });

        // Wait for this pair to finish before starting the next.
        await terminalPromise;
      } catch (err) {
        setPairStates((prev) => {
          const next = [...prev];
          const cur = next[idx];
          if (!cur) return prev;
          next[idx] = {
            ...cur,
            status: {
              kind: "failed",
              completed: 0,
              total: pair.untranslatedCount,
              reason: err instanceof Error ? err.message : String(err),
            },
          };
          return next;
        });
        // Resolve the waiter so we don't hang if startPair itself throws.
        const resolve = terminalResolversRef.current.get(pairKey(pair));
        if (resolve) {
          terminalResolversRef.current.delete(pairKey(pair));
          resolve();
        }
      }

      activeCancelRef.current = null;
    }

    // If some pairs were cancelled mid-queue, mark remaining queued pairs
    // as cancelled so the table reflects the final state.
    if (cancelledRef.current) {
      setPairStates((prev) => {
        const next = [...prev];
        for (let i = 0; i < next.length; i++) {
          const ps = next[i];
          if (ps && ps.status.kind === "queued") {
            next[i] = {
              ...ps,
              status: {
                kind: "cancelled",
                completed: 0,
                total: ps.status.total,
              },
            };
          }
        }
        return next;
      });
    }
  }, [activePairs, pairs, startPair]);

  const handleCancel = useCallback(() => {
    cancelledRef.current = true;
    // Cancel the currently-running pair; the sequential loop will check
    // cancelledRef and skip any pairs that haven't started yet.
    activeCancelRef.current?.();
  }, []);

  // Transition to terminal phase when all active pairs reach a terminal state
  useEffect(() => {
    if (phase === "running" && allTerminal && pairStates.length > 0) {
      setPhase("terminal");
    }
  }, [phase, allTerminal, pairStates.length]);

  if (!open) return null;

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Translate untranslated units"
      className={cn(
        "fixed inset-0 z-50 flex items-center justify-center",
        "bg-black/40",
      )}
      onClick={(e) => {
        // Dismiss on backdrop click only in preview/terminal phases
        if (e.target === e.currentTarget && phase !== "running") onClose();
      }}
    >
      <div
        className={cn(
          "relative flex flex-col rounded-xl shadow-2xl",
          "bg-bg-surface border border-border-default",
          "w-full max-w-lg mx-4",
          "max-h-[80vh]",
        )}
        style={{ minWidth: 420 }}
      >
        {/* Header */}
        <div className="px-5 pt-5 pb-3 border-b border-border-subtle">
          <h2 className="m-0 text-sm font-semibold text-fg-primary">
            Translate untranslated
          </h2>
          <p className="mt-1 text-xs text-fg-secondary">
            {activePairs.length === 0 ? (
              "No pairs have untranslated units."
            ) : (
              <>
                <span className="font-mono font-medium text-fg-primary">
                  {activePairs.length}
                </span>{" "}
                pair{activePairs.length !== 1 ? "s" : ""}
                {" — "}
                <span className="font-mono font-medium text-fg-primary">
                  {totalUnits}
                </span>{" "}
                unit{totalUnits !== 1 ? "s" : ""} total
              </>
            )}
          </p>
        </div>

        {/* Pair table */}
        <div className="overflow-y-auto flex-1 px-5 py-3">
          <table className="w-full text-xs border-collapse">
            <thead>
              <tr
                className="text-left text-fg-tertiary"
                style={{
                  fontSize: 10,
                  letterSpacing: "0.07em",
                  textTransform: "uppercase",
                }}
              >
                <th className="pb-2 pr-3 font-semibold">Catalog</th>
                <th className="pb-2 pr-3 font-semibold">Locale</th>
                <th className="pb-2 pr-3 font-semibold text-right">Units</th>
                <th className="pb-2 font-semibold">Status</th>
              </tr>
            </thead>
            <tbody>
              {pairStates.map((ps, i) => (
                <PairRow
                  key={`${ps.pair.catalogPath}::${ps.pair.locale}`}
                  ps={ps}
                  phase={phase}
                  rowIndex={i}
                />
              ))}
              {skippedPairs.length > 0 &&
                pairStates.length === 0 &&
                skippedPairs.map((p) => (
                  <tr
                    key={`${p.catalogPath}::${p.locale}`}
                    className="opacity-40"
                  >
                    <td className="py-1.5 pr-3 font-mono truncate max-w-[160px]">
                      {p.catalogName}
                    </td>
                    <td className="py-1.5 pr-3 font-mono">{p.locale}</td>
                    <td className="py-1.5 pr-3 text-right tabular-nums">0</td>
                    <td className="py-1.5 text-fg-tertiary italic">skipped</td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>

        {/* Running-phase ETA strip */}
        {phase === "running" && (
          <div className="px-5 py-2 border-t border-border-subtle">
            <div className="flex items-center gap-3">
              <ProgressBar
                value={totalCompleted}
                max={totalForRunning || 1}
                variant="block"
                label="Overall progress"
              />
              <span className="shrink-0 font-mono text-[11px] text-fg-tertiary tabular-nums whitespace-nowrap">
                {totalCompleted} / {totalForRunning}
              </span>
            </div>
            {etaLabel && (
              <p className="mt-1 text-[11px] text-fg-tertiary">{etaLabel}</p>
            )}
          </div>
        )}

        {/* Footer buttons */}
        <div className="px-5 py-4 border-t border-border-subtle flex justify-end gap-2">
          {phase === "preview" && (
            <>
              <button
                type="button"
                onClick={onClose}
                className={cn(
                  "inline-flex items-center h-8 px-4 rounded-md text-xs font-medium",
                  "border border-border-default text-fg-secondary bg-bg-elevated",
                  "hover:bg-bg-hover hover:text-fg-primary transition-colors duration-100",
                  "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
                )}
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleStart}
                disabled={activePairs.length === 0}
                className={cn(
                  "inline-flex items-center h-8 px-4 rounded-md text-xs font-medium",
                  "bg-accent text-accent-fg",
                  "enabled:hover:bg-accent-hover enabled:active:bg-accent-active",
                  "disabled:bg-accent-subtle disabled:text-fg-disabled disabled:cursor-not-allowed",
                  "transition-colors duration-100",
                  "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
                )}
              >
                Start
              </button>
            </>
          )}
          {phase === "running" && (
            <button
              type="button"
              onClick={handleCancel}
              className={cn(
                "inline-flex items-center h-8 px-4 rounded-md text-xs font-medium",
                "border border-border-default text-fg-secondary bg-bg-elevated",
                "hover:bg-bg-hover hover:text-fg-primary transition-colors duration-100",
                "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
              )}
            >
              Cancel translation
            </button>
          )}
          {phase === "terminal" && (
            <button
              type="button"
              onClick={onClose}
              className={cn(
                "inline-flex items-center h-8 px-4 rounded-md text-xs font-medium",
                "border border-border-default text-fg-secondary bg-bg-elevated",
                "hover:bg-bg-hover hover:text-fg-primary transition-colors duration-100",
                "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
              )}
            >
              Close
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

// ── PairRow ───────────────────────────────────────────────────────────────────

interface PairRowProps {
  ps: PairState;
  phase: ModalPhase;
  rowIndex: number;
}

function PairRow({ ps, phase }: PairRowProps) {
  const { pair, status } = ps;
  const isSkipped = pair.untranslatedCount === 0;

  return (
    <tr className={cn(isSkipped && "opacity-40")}>
      {/* Catalog */}
      <td
        className="py-1.5 pr-3 font-mono text-fg-secondary truncate"
        style={{ maxWidth: 160 }}
        title={pair.catalogPath}
      >
        {pair.catalogName}
      </td>
      {/* Locale */}
      <td className="py-1.5 pr-3 font-mono text-fg-secondary">{pair.locale}</td>
      {/* Count */}
      <td className="py-1.5 pr-3 text-right tabular-nums text-fg-secondary">
        {pair.untranslatedCount}
      </td>
      {/* Status / progress */}
      <td className="py-1.5" style={{ minWidth: 120 }}>
        {isSkipped ? (
          <span className="text-fg-tertiary italic text-[11px]">skipped</span>
        ) : status.kind === "pending" ? (
          <span className="text-fg-tertiary text-[11px]">
            {phase === "preview" ? "queued" : "waiting…"}
          </span>
        ) : status.kind === "queued" ? (
          <span className="text-fg-tertiary text-[11px]">
            Queued ({status.total})
          </span>
        ) : status.kind === "running" ? (
          <div className="flex items-center gap-2">
            <ProgressBar
              value={status.completed}
              max={status.total || 1}
              variant="block"
              label={`Progress for ${pair.locale}`}
            />
            <span className="shrink-0 font-mono text-[11px] text-fg-tertiary tabular-nums">
              {status.completed}/{status.total}
            </span>
          </div>
        ) : status.kind === "done" ? (
          <span
            className="text-[11px] font-medium text-state-finished"
            title={`${status.completed} units translated`}
          >
            Done ({status.completed})
          </span>
        ) : status.kind === "cancelled" ? (
          <span className="text-[11px] text-fg-tertiary">
            Cancelled ({status.completed}/{status.total})
          </span>
        ) : (
          <span
            className="text-[11px] font-medium text-state-rejected"
            title={status.reason}
          >
            Failed
          </span>
        )}
      </td>
    </tr>
  );
}

// ── ETA hook ──────────────────────────────────────────────────────────────────

function useEta(
  completed: number,
  total: number,
  startedAt: number | null,
): string | null {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (startedAt === null || completed >= total) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [startedAt, completed, total]);

  if (startedAt === null || completed === 0 || total === 0) return null;

  const elapsed = now - startedAt;
  const rate = completed / elapsed; // units per ms
  if (rate <= 0) return null;

  const remaining = total - completed;
  const etaMs = remaining / rate;
  const etaSec = Math.round(etaMs / 1000);

  if (etaSec < 5) return "Almost done…";
  if (etaSec < 60) return `About ${etaSec}s remaining`;
  const etaMin = Math.round(etaSec / 60);
  return `About ${etaMin}m remaining`;
}
