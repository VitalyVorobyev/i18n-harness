import type { UnitState } from "../../lib/types";
import styles from "./StateBadge.module.css";

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

export function StateBadge({ state, variant = "pill" }: Props) {
  if (variant === "dot") {
    return (
      <span
        className={`${styles.dot} ${styles[state]}`}
        title={LABELS[state]}
        aria-label={LABELS[state]}
      />
    );
  }
  return (
    <span className={`${styles.pill} ${styles[state]}`}>{LABELS[state]}</span>
  );
}
