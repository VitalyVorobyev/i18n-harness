// BatchProgressWidget — shows live batch translation progress in the footer.
//
// Rendered when `activeBatch !== null`. Replaces the normal save-all pill
// while a batch is in flight. Provides a linear progress bar, unit count,
// recent-activity micro-feed, and a Cancel button.

import { cn } from "../../lib/cn";
import type { UnitId } from "../../lib/types";

export interface ActiveBatch {
  jobId: string;
  catalogPath: string;
  /** Basename of the catalog for display (e.g. "app_de.ts"). */
  catalogName: string;
  completed: number;
  total: number;
  /** Last up-to-3 translated unit ids for the live activity feed. */
  recent: UnitId[];
}

interface Props {
  batch: ActiveBatch;
  onCancel: () => void;
}

export function BatchProgressWidget({ batch, onCancel }: Props) {
  const { completed, total, catalogName, recent } = batch;
  const pct = total > 0 ? Math.min(100, (completed / total) * 100) : 0;

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
          <span
            className="text-xs tabular-nums text-fg-tertiary shrink-0"
            aria-hidden="true"
          >
            {Math.round(pct)}%
          </span>
        </div>

        {/* Linear progress track */}
        <div
          role="progressbar"
          aria-valuenow={completed}
          aria-valuemin={0}
          aria-valuemax={total}
          aria-label={`${completed} of ${total} units translated`}
          className="h-[3px] rounded-pill bg-border-default overflow-hidden"
        >
          <div
            className="h-full rounded-pill bg-accent transition-[width] duration-300 ease-out"
            style={{ width: `${pct}%` }}
          />
        </div>

        {/* Recent activity micro-feed */}
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
