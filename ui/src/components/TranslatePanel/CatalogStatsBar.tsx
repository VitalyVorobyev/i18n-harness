// Compact per-file statistics for the active catalog, shown in the translate
// sub-header. Counts come from the on-demand validation gate (see App's
// `refreshCatalogGate`): unit states plus how many units the gate flagged
// hard vs soft. Renders nothing until stats are available for the catalog.

import type { CatalogGateStats } from "../../lib/types";

interface StatProps {
  color: string;
  count: number;
  label: string;
}

function Stat({ color, count, label }: StatProps) {
  return (
    <span className="flex items-center gap-1 whitespace-nowrap" title={label}>
      <span
        className="inline-block h-2 w-2 rounded-full"
        style={{ background: color }}
        aria-hidden
      />
      <span className="tabular-nums font-medium">{count}</span>
      <span className="text-fg-tertiary">{label}</span>
    </span>
  );
}

interface Props {
  stats: CatalogGateStats | null;
}

export function CatalogStatsBar({ stats }: Props) {
  if (!stats) return null;
  const translatable = stats.finished + stats.proposed + stats.untranslated;
  return (
    <div className="flex items-center gap-3 text-xs text-fg-secondary">
      <span className="text-fg-tertiary tabular-nums">
        {translatable} unit{translatable === 1 ? "" : "s"}
      </span>
      <Stat
        color="var(--color-state-finished)"
        count={stats.finished}
        label="finished"
      />
      <Stat
        color="var(--color-state-proposed)"
        count={stats.proposed}
        label="proposed"
      />
      <Stat
        color="var(--color-state-untranslated, var(--color-fg-tertiary))"
        count={stats.untranslated}
        label="untranslated"
      />
      {stats.hard > 0 && (
        <Stat
          color="var(--color-severity-hard)"
          count={stats.hard}
          label="blocking"
        />
      )}
      {stats.soft > 0 && (
        <Stat
          color="var(--color-severity-soft)"
          count={stats.soft}
          label="warnings"
        />
      )}
    </div>
  );
}
