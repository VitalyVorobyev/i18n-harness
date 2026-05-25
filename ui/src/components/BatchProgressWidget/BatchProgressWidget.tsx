// BatchProgressWidget — shows live batch translation progress in the footer.
//
// Rendered when `activeBatch !== null`. Replaces the normal save-all pill
// while a batch is in flight. Provides a linear progress bar, ETA, unit
// count, recent-activity micro-feed, and a Cancel button.

import { cn } from "../../lib/cn";
import type { UnitId } from "../../lib/types";
import { ProgressBar } from "../primitives/ProgressBar";

export interface ActiveBatch {
  jobId: string;
  catalogPath: string;
  /** Basename of the catalog for display (e.g. "app_de.ts"). */
  catalogName: string;
  completed: number;
  total: number;
  /** Last up-to-3 translated unit ids for the live activity feed. */
  recent: UnitId[];
  /**
   * `Date.now()` captured when the batch listeners were registered and the
   * UI state was armed. Used to compute a live ETA.
   */
  startedAt: number;
}

interface Props {
  batch: ActiveBatch;
  onCancel: () => void;
}

/** Format remaining milliseconds as a human-readable string.
 *
 * Rounds to the nearest second. Returns "<1s" for sub-second remainders so
 * the label never shows "0s remaining" just before the bar fills.
 */
function formatEta(ms: number): string {
  const secs = Math.round(ms / 1000);
  if (secs < 1) return "<1s";
  return `${secs}s`;
}

export function BatchProgressWidget({ batch, onCancel }: Props) {
  const { completed, total, catalogName, recent, startedAt } = batch;

  // Estimate time remaining. Only meaningful after at least one unit has
  // completed — with zero completions there is no rate signal yet.
  let etaLabel: string | null = null;
  if (completed > 0 && completed < total) {
    const elapsed = Date.now() - startedAt;
    const msPerUnit = elapsed / completed;
    const remaining = msPerUnit * (total - completed);
    etaLabel = `~${formatEta(remaining)} remaining`;
  }

  return (
    <div
      role="status"
      aria-live="polite"
      aria-label={`Translating ${catalogName}: ${completed} of ${total} units`}
      className="flex items-center gap-3 flex-1 min-w-0"
    >
      {/* Progress bar + label block */}
      <div className="flex-1 flex flex-col gap-0.5 min-w-0">
        <div className="flex items-center justify-between gap-2">
          <span className="text-xs font-medium text-fg-secondary truncate">
            Translating{" "}
            <span className="font-semibold text-fg-primary">{catalogName}</span>
            {": "}
            <span className="tabular-nums text-accent">
              {completed}/{total}
            </span>
          </span>
          {/* ETA — hidden when there is no signal yet.
              Plain text; screen readers read it as part of the row. */}
          {etaLabel !== null && (
            <span
              className="text-xs tabular-nums text-fg-tertiary shrink-0"
              aria-live="off"
            >
              {etaLabel}
            </span>
          )}
        </div>

        {/* ProgressBar primitive — carries all ARIA progressbar semantics */}
        <ProgressBar
          value={completed}
          max={total}
          label={`${completed} of ${total} units translated`}
          showPercent
        />

        {/* Recent activity micro-feed — kept below the bar so it never
            pushes the bar off-screen on narrow widgets. */}
        {recent.length > 0 && (
          <p className="text-xs text-fg-disabled truncate mt-0.5">
            <span>Last: </span>
            <span className="font-mono" aria-live="off">
              {recent.join(", ")}
            </span>
          </p>
        )}
      </div>

      {/* Cancel button */}
      <button
        type="button"
        onClick={onCancel}
        aria-label="Cancel batch translation"
        title="Cancel batch translation"
        className={cn(
          "shrink-0 h-6 px-3 rounded-md border text-xs font-medium",
          "border-severity-hard-border text-severity-hard",
          "bg-transparent hover:bg-severity-hard-bg",
          "active:opacity-80 transition-colors duration-100",
          "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-severity-hard",
        )}
      >
        Cancel
      </button>
    </div>
  );
}
