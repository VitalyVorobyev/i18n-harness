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

const DOT: Record<UnitState, string> = {
  untranslated: "bg-state-untranslated",
  proposed: "bg-state-proposed",
  finished: "bg-state-finished",
  vanished: "bg-state-vanished",
  obsolete: "bg-state-vanished",
};

export function StateBadge({ state, variant = "pill" }: Props) {
  if (variant === "dot") {
    return (
      <span
        role="img"
        aria-label={LABELS[state]}
        title={LABELS[state]}
        className={cn(
          "inline-block w-1.5 h-1.5 rounded-pill shrink-0",
          DOT[state],
        )}
      />
    );
  }
  return (
    <span
      className={cn(
        "inline-flex items-center h-[18px] px-2 rounded-pill border",
        "text-xs font-medium uppercase tracking-loose whitespace-nowrap",
        PILL[state],
      )}
    >
      {LABELS[state]}
    </span>
  );
}
