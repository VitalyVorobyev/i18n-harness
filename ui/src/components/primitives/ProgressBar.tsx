// ProgressBar — accessible single-value progress indicator.
//
// Implements the ARIA progressbar role with explicit valuenow / valuemin /
// valuemax attributes. Two visual variants: "inline" (1.5px tall, suited for
// row-level indicators) and "block" (2.5px tall, suited for modals and cards).
// Fill width transitions smoothly via CSS; no indeterminate mode (that is a
// separate primitive).

interface ProgressBarProps {
  value: number;
  max: number;
  variant?: "inline" | "block";
  label?: string;
  showPercent?: boolean;
}

export function ProgressBar({
  value,
  max,
  variant = "inline",
  label,
  showPercent = false,
}: ProgressBarProps) {
  // Clamp value defensively so callers can pass raw counters without guards.
  const safeMax = max > 0 ? max : 1;
  const clamped = Math.max(0, Math.min(safeMax, value));
  const pct = (clamped / safeMax) * 100;

  const trackClass = variant === "block" ? "h-2.5" : "h-1.5";

  return (
    <div className="flex items-center gap-2 w-full">
      <div
        role="progressbar"
        aria-valuenow={clamped}
        aria-valuemin={0}
        aria-valuemax={safeMax}
        aria-label={label}
        className={`relative flex-1 rounded-pill overflow-hidden bg-bg-input ${trackClass}`}
      >
        <div
          className={`absolute inset-y-0 left-0 rounded-pill bg-accent transition-[width] duration-150 ease-out ${trackClass}`}
          style={{ width: `${pct}%` }}
        />
      </div>
      {showPercent && (
        <span
          className="shrink-0 font-mono text-[11px] text-fg-tertiary tabular-nums"
          aria-hidden="true"
        >
          {Math.round(pct)}%
        </span>
      )}
    </div>
  );
}
