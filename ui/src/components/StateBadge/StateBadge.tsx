import { cn } from "../../lib/cn";
import type { UnitState } from "../../lib/types";

interface Props {
  state: UnitState;
  variant?: "dot" | "pill";
}

const LABELS: Record<UnitState, string> = {
  untranslated: "Untranslated",
  proposed: "Proposed",
  finished: "Finished",
  vanished: "Vanished",
  obsolete: "Obsolete",
};

const PILL: Record<UnitState, string> = {
  untranslated:
    "bg-state-untranslated-bg text-state-untranslated border-state-untranslated-border",
  proposed:
    "bg-state-proposed-bg text-state-proposed border-state-proposed-border",
  finished:
    "bg-state-finished-bg text-state-finished border-state-finished-border",
  vanished:
    "bg-state-vanished-bg text-state-vanished border-state-vanished-border",
  obsolete:
    "bg-state-vanished-bg text-state-vanished border-state-vanished-border",
};

const DOT_COLOR: Record<UnitState, string> = {
  untranslated: "var(--color-state-untranslated)",
  proposed: "var(--color-state-proposed)",
  finished: "var(--color-state-finished)",
  vanished: "var(--color-state-vanished)",
  obsolete: "var(--color-state-vanished)",
};

export function StateBadge({ state, variant = "pill" }: Props) {
  if (variant === "dot") {
    return (
      <span
        role="img"
        aria-label={LABELS[state]}
        title={LABELS[state]}
        style={{
          display: "inline-block",
          width: 6,
          height: 6,
          borderRadius: 999,
          background: DOT_COLOR[state],
          flexShrink: 0,
          verticalAlign: "middle",
        }}
      />
    );
  }
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-pill border whitespace-nowrap",
        PILL[state],
      )}
      style={{ lineHeight: 1, padding: "2px 6px" }}
    >
      {/* state-coloured dot */}
      <span
        aria-hidden="true"
        style={{
          display: "inline-block",
          width: 6,
          height: 6,
          borderRadius: 999,
          background: DOT_COLOR[state],
          flexShrink: 0,
        }}
      />
      <span
        style={{
          fontSize: "var(--text-xs)",
          fontWeight: 500,
          textTransform: "uppercase",
          letterSpacing: "0.08em",
        }}
      >
        {LABELS[state]}
      </span>
    </span>
  );
}
