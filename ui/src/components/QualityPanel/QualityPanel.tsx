// Quality view — headline numbers, translation memory, curated set,
// and prompt evaluation.
//
// Part 1: Per-locale headline cards + 30-day acceptance-rate sparkline.
// Part 2: Translation memory (corrections table) + curated set — carried
//         forward from the corrections/curated-set work.
// Part 3: Prompt evaluation — Run evaluation button, progress widget,
//         latest run summary, run history.

import { useCallback, useEffect, useRef, useState } from "react";
import {
  cancelEvaluation,
  exportTuningBundleInProject,
  type ListCorrectionsFilter,
  listCorrectionsInProject,
  listCuratedInProject,
  listEvaluationRunsInProject,
  listLocales,
  listTuningBundlesInProject,
  promoteCorrectionToCurated,
  runEvaluationInProject,
  unCurateCorrection,
} from "../../lib/tauri";
import { listenEvalProgress } from "../../lib/tauri-events";
import type {
  Correction,
  CorrectionId,
  CuratedExample,
  EvaluationProgressPayload,
  EvaluationRun,
  EvaluationTerminalPayload,
  ExportTuningBundleResponse,
  LocaleInfo,
  ProjectSummary,
} from "../../lib/types";

// ── Prop types ────────────────────────────────────────────────────────────────

interface Props {
  summary: ProjectSummary;
  flashError: (msg: string) => void;
  flashInfo: (msg: string) => void;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  return String(e);
}

/** Truncate a string to `max` characters, appending "…" when cut. */
function trunc(s: string, max: number): string {
  if (!s) return "";
  const trimmed = s.replace(/\s+/g, " ").trim();
  return trimmed.length > max ? `${trimmed.slice(0, max)}…` : trimmed;
}

/** Relative-time label: "3 days ago", "just now", etc. */
function relativeTime(isoTs: string): string {
  const ms = Date.now() - new Date(isoTs).getTime();
  if (Number.isNaN(ms)) return isoTs;
  const secs = Math.floor(ms / 1000);
  if (secs < 60) return "just now";
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins} minute${mins === 1 ? "" : "s"} ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours} hour${hours === 1 ? "" : "s"} ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} day${days === 1 ? "" : "s"} ago`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months} month${months === 1 ? "" : "s"} ago`;
  const years = Math.floor(months / 12);
  return `${years} year${years === 1 ? "" : "s"} ago`;
}

/** Format an ISO timestamp to a short locale string. */
function formatTs(isoTs: string): string {
  try {
    return new Date(isoTs).toLocaleString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return isoTs;
  }
}

/** Format a score (0–1) as a percentage string with one decimal. */
function pct(score: number): string {
  return `${(score * 100).toFixed(1)}%`;
}

/** Shorten a catalog path to its last two segments. */
function shortenCatalog(p: string): string {
  const parts = p.replace(/\\/g, "/").split("/");
  if (parts.length <= 2) return p;
  return `…/${parts.slice(-2).join("/")}`;
}

const MAX_ROWS = 200;

// ── Sub-components ────────────────────────────────────────────────────────────

/** Small spinner used while data is loading. */
function Spinner() {
  return (
    <span
      aria-hidden="true"
      className="inline-block w-3 h-3 border border-fg-tertiary border-t-fg-secondary rounded-full animate-spin"
    />
  );
}

// ── Part 1: Per-locale headline stats ─────────────────────────────────────────

interface LocaleStat {
  localeId: string;
  script: string;
  total: number;
  acceptedAsIs: number;
  edited: number;
  fromScratch: number;
  // Ordered daily acceptance rates for last 30 days (index 0 = oldest).
  // Each entry is the rate for that day (NaN = no data that day).
  sparkDays: number[];
}

function computeStats(
  localeId: string,
  script: string,
  corrections: Correction[],
): LocaleStat {
  const now = Date.now();
  const MS_PER_DAY = 86_400_000;
  // Bucket corrections by day index (0 = 29 days ago, 29 = today).
  const dayBuckets: { accepted: number; total: number }[] = Array.from(
    { length: 30 },
    () => ({ accepted: 0, total: 0 }),
  );

  let total = 0;
  let acceptedAsIs = 0;
  let edited = 0;
  let fromScratch = 0;

  for (const c of corrections) {
    total++;
    const isFromScratch = !c.mt_proposal || c.mt_proposal.trim().length === 0;
    const isAccepted =
      !isFromScratch && c.mt_proposal.trim() === c.human_target.trim();
    const isEdited = !isFromScratch && !isAccepted;

    if (isAccepted) acceptedAsIs++;
    else if (isFromScratch) fromScratch++;
    else if (isEdited) edited++;

    // Assign to the 30-day sparkline bucket.
    const ts = new Date(c.ts).getTime();
    if (!Number.isNaN(ts)) {
      const daysAgo = Math.floor((now - ts) / MS_PER_DAY);
      if (daysAgo >= 0 && daysAgo < 30) {
        const bucket = dayBuckets[29 - daysAgo];
        if (bucket) {
          bucket.total++;
          if (isAccepted) bucket.accepted++;
        }
      }
    }
  }

  const sparkDays = dayBuckets.map((b) =>
    b.total === 0 ? Number.NaN : b.accepted / b.total,
  );

  return {
    localeId,
    script,
    total,
    acceptedAsIs,
    edited,
    fromScratch,
    sparkDays,
  };
}

/** Inline SVG sparkline for 30-day acceptance rate. */
function Sparkline({ days }: { days: number[] }) {
  const W = 120;
  const H = 28;
  const PAD = 2;
  const innerW = W - PAD * 2;
  const innerH = H - PAD * 2;

  // Filter out NaN for min/max, but keep positions for x-axis alignment.
  const valid = days.filter((d) => !Number.isNaN(d));
  const hasData = valid.length > 0;

  // Build polyline points. Days with NaN are skipped (gap in line).
  const segments: { x: number; y: number }[][] = [];
  let current: { x: number; y: number }[] = [];

  for (let i = 0; i < days.length; i++) {
    const v = days[i];
    if (v === undefined) continue;
    const x = PAD + (i / (days.length - 1)) * innerW;
    if (Number.isNaN(v)) {
      if (current.length > 0) {
        segments.push(current);
        current = [];
      }
    } else {
      // Y is inverted: top = 100%, bottom = 0%.
      const y = PAD + (1 - v) * innerH;
      current.push({ x, y });
    }
  }
  if (current.length > 0) segments.push(current);

  if (!hasData) {
    return (
      <svg
        width={W}
        height={H}
        aria-label="No data for the last 30 days"
        className="text-fg-disabled"
      >
        <line
          x1={PAD}
          y1={H / 2}
          x2={W - PAD}
          y2={H / 2}
          stroke="currentColor"
          strokeWidth={1}
          strokeDasharray="3 3"
        />
      </svg>
    );
  }

  const toPoints = (pts: { x: number; y: number }[]) =>
    pts.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ");

  return (
    <svg
      width={W}
      height={H}
      aria-label="30-day acceptance rate trend"
      role="img"
    >
      <title>30-day acceptance rate trend</title>
      {/* Zero line */}
      <line
        x1={PAD}
        y1={H - PAD}
        x2={W - PAD}
        y2={H - PAD}
        stroke="var(--color-border-subtle)"
        strokeWidth={0.5}
      />
      {/* 50% guide */}
      <line
        x1={PAD}
        y1={PAD + innerH / 2}
        x2={W - PAD}
        y2={PAD + innerH / 2}
        stroke="var(--color-border-subtle)"
        strokeWidth={0.5}
        strokeDasharray="2 2"
      />
      {segments.map((seg, si) => (
        <polyline
          key={`seg-${si}`}
          points={toPoints(seg)}
          fill="none"
          stroke="var(--color-accent)"
          strokeWidth={1.5}
          strokeLinejoin="round"
          strokeLinecap="round"
        />
      ))}
    </svg>
  );
}

/** Acceptance-rate colour class: green >70%, yellow 40–70%, red <40%. */
function acceptColor(rate: number): string {
  if (rate >= 0.7) return "text-severity-pass";
  if (rate >= 0.4) return "text-severity-warn";
  return "text-severity-fail";
}

interface HeadlineCardsProps {
  stats: LocaleStat[];
  loading: boolean;
}

function HeadlineCards({ stats, loading }: HeadlineCardsProps) {
  if (loading && stats.length === 0) {
    return (
      <div className="flex items-center gap-2 text-xs text-fg-tertiary py-3">
        <Spinner /> Computing headline numbers…
      </div>
    );
  }

  if (!loading && stats.length === 0) {
    return (
      <p className="py-4 text-sm text-fg-tertiary">
        No corrections recorded yet — edit translated units to populate headline
        numbers.
      </p>
    );
  }

  return (
    <div
      className="grid gap-3"
      style={{
        gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))",
      }}
    >
      {stats.map((s) => {
        const acceptRate = s.total > 0 ? s.acceptedAsIs / s.total : 0;
        const editRate = s.total > 0 ? s.edited / s.total : 0;
        const scratchRate = s.total > 0 ? s.fromScratch / s.total : 0;

        return (
          <article
            key={s.localeId}
            className="rounded-md border border-border-subtle bg-bg-surface px-4 py-3 flex flex-col gap-2"
            aria-label={`Locale ${s.localeId} statistics`}
          >
            <header className="flex items-baseline justify-between gap-2">
              <div>
                <span className="text-sm font-semibold text-fg-primary font-mono">
                  {s.localeId}
                </span>
                {s.script && (
                  <span className="ml-2 text-xs text-fg-tertiary">
                    {s.script}
                  </span>
                )}
              </div>
              <span className="text-xs text-fg-tertiary tabular-nums">
                {s.total} correction{s.total === 1 ? "" : "s"}
              </span>
            </header>

            {/* Sparkline */}
            <figure title="Acceptance rate over the last 30 days">
              <Sparkline days={s.sparkDays} />
            </figure>

            {/* Rates row */}
            <div className="flex items-center gap-4 text-xs">
              <span
                className={`font-semibold tabular-nums ${acceptColor(acceptRate)}`}
                title="Accepted as-is"
              >
                {pct(acceptRate)}
                <span className="ml-1 font-normal text-fg-tertiary">
                  accepted
                </span>
              </span>
              <span
                className="tabular-nums text-fg-secondary"
                title="Edited before accepting"
              >
                {pct(editRate)}
                <span className="ml-1 text-fg-tertiary">edited</span>
              </span>
              <span
                className="tabular-nums text-fg-tertiary"
                title="Written from scratch (no MT proposal)"
              >
                {pct(scratchRate)}
                <span className="ml-1">scratch</span>
              </span>
            </div>
          </article>
        );
      })}
    </div>
  );
}

// ── Corrections table ─────────────────────────────────────────────────────────

interface CorrectionsTableProps {
  corrections: Correction[];
  curatedIds: Set<CorrectionId>;
  loading: boolean;
  onPromote: (id: CorrectionId) => void;
  onUncurate: (id: CorrectionId) => void;
}

function CorrectionsTable({
  corrections,
  curatedIds,
  loading,
  onPromote,
  onUncurate,
}: CorrectionsTableProps) {
  const displayed = corrections.slice(0, MAX_ROWS);
  const overflow = corrections.length > MAX_ROWS;

  if (!loading && corrections.length === 0) {
    return (
      <p className="py-6 text-center text-sm text-fg-tertiary">
        No corrections recorded yet. Edit a translated unit to record one.
      </p>
    );
  }

  return (
    <div className="relative">
      {loading && corrections.length > 0 && (
        <div
          aria-live="polite"
          className="absolute inset-0 bg-bg-base/60 z-10 flex items-start justify-end pr-3 pt-2"
        >
          <Spinner />
        </div>
      )}
      <div
        className={`overflow-auto rounded-md border border-border-subtle ${loading && corrections.length > 0 ? "opacity-60" : ""}`}
        style={{ maxHeight: "calc(100vh - 32rem)" }}
      >
        <table
          className="w-full text-xs border-collapse"
          aria-label="Corrections"
        >
          <thead>
            <tr className="bg-bg-surface border-b border-border-subtle text-fg-tertiary uppercase tracking-wider">
              <th scope="col" className="py-2 px-3 text-left font-medium">
                Catalog
              </th>
              <th scope="col" className="py-2 px-3 text-left font-medium">
                Locale
              </th>
              <th scope="col" className="py-2 px-3 text-left font-medium">
                Unit ID
              </th>
              <th scope="col" className="py-2 px-3 text-left font-medium">
                Source
              </th>
              <th scope="col" className="py-2 px-3 text-left font-medium">
                MT proposal
              </th>
              <th scope="col" className="py-2 px-3 text-left font-medium">
                Human target
              </th>
              <th scope="col" className="py-2 px-3 text-left font-medium">
                Recorded
              </th>
              <th scope="col" className="py-2 px-3 text-left font-medium">
                Action
              </th>
            </tr>
          </thead>
          <tbody>
            {displayed.map((c) => {
              const isCurated = curatedIds.has(c.id);
              return (
                <tr
                  key={c.id}
                  className="border-b border-border-subtle last:border-0 hover:bg-bg-hover transition-colors duration-75"
                >
                  <td
                    className="py-2 px-3 text-fg-secondary font-mono whitespace-nowrap"
                    title={c.catalog}
                  >
                    {shortenCatalog(c.catalog)}
                  </td>
                  <td className="py-2 px-3 text-fg-secondary whitespace-nowrap">
                    {c.locale}
                  </td>
                  <td
                    className="py-2 px-3 text-fg-secondary font-mono whitespace-nowrap"
                    title={c.unit_id}
                  >
                    {trunc(c.unit_id, 24)}
                  </td>
                  <td
                    className="py-2 px-3 text-fg-primary max-w-[200px]"
                    title={c.source}
                  >
                    {trunc(c.source, 60)}
                  </td>
                  <td
                    className="py-2 px-3 text-fg-secondary max-w-[200px]"
                    title={c.mt_proposal}
                  >
                    {c.mt_proposal ? (
                      trunc(c.mt_proposal, 60)
                    ) : (
                      <span className="text-fg-disabled italic">—</span>
                    )}
                  </td>
                  <td
                    className="py-2 px-3 text-fg-primary max-w-[200px]"
                    title={c.human_target}
                  >
                    {trunc(c.human_target, 60)}
                  </td>
                  <td
                    className="py-2 px-3 text-fg-tertiary whitespace-nowrap"
                    title={c.ts}
                  >
                    {relativeTime(c.ts)}
                  </td>
                  <td className="py-2 px-3 whitespace-nowrap">
                    {isCurated ? (
                      <button
                        type="button"
                        onClick={() => onUncurate(c.id)}
                        className="h-6 px-2 rounded border border-border-default bg-bg-surface text-xs text-fg-secondary hover:bg-bg-hover hover:border-border-strong active:bg-bg-selected transition-colors duration-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                        aria-label={`Remove correction ${c.id} from curated set`}
                      >
                        Un-curate
                      </button>
                    ) : (
                      <button
                        type="button"
                        onClick={() => onPromote(c.id)}
                        className="h-6 px-2 rounded border border-border-default bg-bg-surface text-xs text-fg-secondary hover:bg-bg-hover hover:border-border-strong active:bg-bg-selected transition-colors duration-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                        aria-label={`Promote correction ${c.id} to curated set`}
                      >
                        Promote to curated
                      </button>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {overflow && (
        <p className="mt-2 text-xs text-fg-tertiary text-center">
          Showing first {MAX_ROWS} of {corrections.length} — refine filters to
          see more.
        </p>
      )}
    </div>
  );
}

// ── Curated table ─────────────────────────────────────────────────────────────

interface CuratedTableProps {
  examples: CuratedExample[];
  loading: boolean;
  onUncurate: (id: CorrectionId) => void;
  onNoteBlur: (id: CorrectionId, note: string) => void;
}

function CuratedTable({
  examples,
  loading,
  onUncurate,
  onNoteBlur,
}: CuratedTableProps) {
  if (!loading && examples.length === 0) {
    return (
      <p className="py-6 text-center text-sm text-fg-tertiary">
        No curated examples yet. Promote a correction from the table above.
      </p>
    );
  }

  return (
    <div className="relative">
      {loading && examples.length > 0 && (
        <div
          aria-live="polite"
          className="absolute inset-0 bg-bg-base/60 z-10 flex items-start justify-end pr-3 pt-2"
        >
          <Spinner />
        </div>
      )}
      <div
        className={`overflow-auto rounded-md border border-border-subtle ${loading && examples.length > 0 ? "opacity-60" : ""}`}
        style={{ maxHeight: "calc(100vh - 28rem)" }}
      >
        <table
          className="w-full text-xs border-collapse"
          aria-label="Curated examples"
        >
          <thead>
            <tr className="bg-bg-surface border-b border-border-subtle text-fg-tertiary uppercase tracking-wider">
              <th scope="col" className="py-2 px-3 text-left font-medium">
                Catalog
              </th>
              <th scope="col" className="py-2 px-3 text-left font-medium">
                Locale
              </th>
              <th scope="col" className="py-2 px-3 text-left font-medium">
                Source
              </th>
              <th scope="col" className="py-2 px-3 text-left font-medium">
                Human target
              </th>
              <th scope="col" className="py-2 px-3 text-left font-medium">
                Note
              </th>
              <th scope="col" className="py-2 px-3 text-left font-medium">
                Action
              </th>
            </tr>
          </thead>
          <tbody>
            {examples.map((ex) => {
              const corr = ex.correction;
              return (
                <tr
                  key={ex.id}
                  className="border-b border-border-subtle last:border-0 hover:bg-bg-hover transition-colors duration-75"
                >
                  <td
                    className="py-2 px-3 text-fg-secondary font-mono whitespace-nowrap"
                    title={corr?.catalog}
                  >
                    {corr ? (
                      shortenCatalog(corr.catalog)
                    ) : (
                      <span className="text-fg-disabled italic">dangling</span>
                    )}
                  </td>
                  <td className="py-2 px-3 text-fg-secondary whitespace-nowrap">
                    {corr?.locale ?? (
                      <span className="text-fg-disabled italic">—</span>
                    )}
                  </td>
                  <td
                    className="py-2 px-3 text-fg-primary max-w-[200px]"
                    title={corr?.source}
                  >
                    {corr ? (
                      trunc(corr.source, 60)
                    ) : (
                      <span className="text-fg-disabled italic">—</span>
                    )}
                  </td>
                  <td
                    className="py-2 px-3 text-fg-primary max-w-[200px]"
                    title={corr?.human_target}
                  >
                    {corr ? (
                      trunc(corr.human_target, 60)
                    ) : (
                      <span className="text-fg-disabled italic">—</span>
                    )}
                  </td>
                  <td className="py-2 px-3 min-w-[140px]">
                    <NoteCell
                      id={ex.id}
                      initialNote={ex.note ?? ""}
                      onBlur={onNoteBlur}
                    />
                  </td>
                  <td className="py-2 px-3 whitespace-nowrap">
                    <button
                      type="button"
                      onClick={() => onUncurate(ex.id)}
                      className="h-6 px-2 rounded border border-border-default bg-bg-surface text-xs text-fg-secondary hover:bg-bg-hover hover:border-border-strong active:bg-bg-selected transition-colors duration-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                      aria-label={`Remove curated example ${ex.id}`}
                    >
                      Un-curate
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ── Note cell — inline editable ───────────────────────────────────────────────

function NoteCell({
  id,
  initialNote,
  onBlur,
}: {
  id: CorrectionId;
  initialNote: string;
  onBlur: (id: CorrectionId, note: string) => void;
}) {
  const [value, setValue] = useState(initialNote);
  // Keep local state in sync if the parent re-renders with new data (e.g., after refetch).
  const latestNote = useRef(initialNote);
  useEffect(() => {
    if (initialNote !== latestNote.current) {
      latestNote.current = initialNote;
      setValue(initialNote);
    }
  }, [initialNote]);

  return (
    <input
      type="text"
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onBlur={() => onBlur(id, value)}
      placeholder="Add a note…"
      aria-label="Curation note"
      className="w-full h-6 px-1.5 rounded border border-border-subtle bg-transparent text-xs text-fg-primary placeholder:text-fg-disabled focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent transition-colors duration-100"
    />
  );
}

// ── Filter bar ────────────────────────────────────────────────────────────────

interface FilterBarProps {
  catalogs: string[];
  locales: string[];
  catalog: string;
  locale: string;
  unitId: string;
  curatedOnly: boolean;
  onCatalogChange: (v: string) => void;
  onLocaleChange: (v: string) => void;
  onUnitIdChange: (v: string) => void;
  onCuratedOnlyChange: (v: boolean) => void;
}

function FilterBar({
  catalogs,
  locales,
  catalog,
  locale,
  unitId,
  curatedOnly,
  onCatalogChange,
  onLocaleChange,
  onUnitIdChange,
  onCuratedOnlyChange,
}: FilterBarProps) {
  const selectCls =
    "h-7 px-2 rounded border border-border-subtle bg-bg-surface text-xs text-fg-primary focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent transition-colors duration-100";

  return (
    <fieldset className="flex flex-wrap items-center gap-3 border-none p-0 m-0">
      <legend className="sr-only">Filter corrections</legend>

      {/* Catalog dropdown */}
      <label className="flex items-center gap-1.5 text-xs text-fg-secondary">
        <span>Catalog</span>
        <select
          value={catalog}
          onChange={(e) => onCatalogChange(e.target.value)}
          className={selectCls}
          aria-label="Filter by catalog"
        >
          <option value="">All catalogs</option>
          {catalogs.map((c) => (
            <option key={c} value={c}>
              {shortenCatalog(c)}
            </option>
          ))}
        </select>
      </label>

      {/* Locale dropdown */}
      <label className="flex items-center gap-1.5 text-xs text-fg-secondary">
        <span>Locale</span>
        <select
          value={locale}
          onChange={(e) => onLocaleChange(e.target.value)}
          className={selectCls}
          aria-label="Filter by locale"
        >
          <option value="">All locales</option>
          {locales.map((l) => (
            <option key={l} value={l}>
              {l}
            </option>
          ))}
        </select>
      </label>

      {/* Unit ID text input.
          Note: the underlying CorrectionFilter.unit_id is an exact match
          against the UnitId value in corrections.jsonl. This input sends
          the value verbatim — partial matches will return no results unless
          the user types the full unit id. */}
      <label className="flex items-center gap-1.5 text-xs text-fg-secondary">
        <span>Unit ID</span>
        <input
          type="text"
          value={unitId}
          onChange={(e) => onUnitIdChange(e.target.value)}
          placeholder="Exact unit id…"
          aria-label="Filter by unit id (exact match)"
          title="Exact match only: the project correction store does not yet support substring search."
          className="h-7 px-2 rounded border border-border-subtle bg-bg-surface text-xs text-fg-primary placeholder:text-fg-disabled focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent transition-colors duration-100 w-36"
        />
      </label>

      {/* Curated only checkbox */}
      <label className="flex items-center gap-1.5 text-xs text-fg-secondary cursor-pointer select-none">
        <input
          type="checkbox"
          checked={curatedOnly}
          onChange={(e) => onCuratedOnlyChange(e.target.checked)}
          className="rounded border-border-subtle accent-accent"
          aria-label="Show curated corrections only"
        />
        Curated only
      </label>
    </fieldset>
  );
}

// ── Part 3: Evaluation section ────────────────────────────────────────────────

/** Progress bar for an in-flight evaluation run. */
interface EvalProgressProps {
  completed: number;
  total: number;
  lastLocale: string;
  jobId: string;
  onCancel: () => void;
}

function EvalProgressBar({
  completed,
  total,
  lastLocale,
  jobId,
  onCancel,
}: EvalProgressProps) {
  const pctDone = total > 0 ? (completed / total) * 100 : 0;
  return (
    <div
      role="status"
      aria-live="polite"
      aria-label={`Evaluation progress: ${completed} of ${total}`}
      className="flex flex-col gap-2"
    >
      <div className="flex items-center justify-between text-xs text-fg-secondary">
        <span>
          Running evaluation — {completed} / {total} examples
          {lastLocale ? ` (last: ${lastLocale})` : ""}
        </span>
        <button
          type="button"
          onClick={onCancel}
          className="h-6 px-3 rounded border border-border-default bg-bg-surface text-xs text-fg-secondary hover:bg-bg-hover hover:border-border-strong active:bg-bg-selected transition-colors duration-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
          aria-label={`Cancel evaluation job ${jobId}`}
        >
          Cancel
        </button>
      </div>
      <div
        className="w-full h-1.5 rounded-full bg-bg-surface border border-border-subtle overflow-hidden"
        role="progressbar"
        aria-valuenow={completed}
        aria-valuemin={0}
        aria-valuemax={total}
      >
        <div
          className="h-full bg-accent rounded-full transition-all duration-300"
          style={{ width: `${pctDone.toFixed(1)}%` }}
        />
      </div>
    </div>
  );
}

/** Per-locale score table in the latest-run summary. */
function PerLocaleTable({
  perLocale,
}: {
  perLocale: Record<string, { score: number; count: number }>;
}) {
  const entries = Object.entries(perLocale).sort(([a], [b]) =>
    a.localeCompare(b),
  );
  if (entries.length === 0) {
    return (
      <p className="text-xs text-fg-tertiary">
        No per-locale data in this run.
      </p>
    );
  }
  return (
    <table
      className="text-xs border-collapse w-full"
      aria-label="Per-locale scores"
    >
      <thead>
        <tr className="border-b border-border-subtle text-fg-tertiary uppercase tracking-wider">
          <th scope="col" className="py-1.5 pr-4 text-left font-medium">
            Locale
          </th>
          <th scope="col" className="py-1.5 pr-4 text-right font-medium">
            Score
          </th>
          <th scope="col" className="py-1.5 text-right font-medium">
            Examples
          </th>
        </tr>
      </thead>
      <tbody>
        {entries.map(([locale, s]) => (
          <tr
            key={locale}
            className="border-b border-border-subtle last:border-0"
          >
            <td className="py-1.5 pr-4 text-fg-primary font-mono">{locale}</td>
            <td
              className={`py-1.5 pr-4 text-right tabular-nums font-semibold ${acceptColor(s.score)}`}
            >
              {pct(s.score)}
            </td>
            <td className="py-1.5 text-right text-fg-tertiary tabular-nums">
              {s.count}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** Per-flag-kind score table. */
function PerFlagTable({
  perFlag,
}: {
  perFlag: Record<string, { score: number; count: number }>;
}) {
  const entries = Object.entries(perFlag).sort(([a], [b]) =>
    a.localeCompare(b),
  );
  if (entries.length === 0) return null;
  return (
    <table
      className="text-xs border-collapse w-full"
      aria-label="Per-flag-kind scores"
    >
      <thead>
        <tr className="border-b border-border-subtle text-fg-tertiary uppercase tracking-wider">
          <th scope="col" className="py-1.5 pr-4 text-left font-medium">
            Flag kind
          </th>
          <th scope="col" className="py-1.5 pr-4 text-right font-medium">
            Score
          </th>
          <th scope="col" className="py-1.5 text-right font-medium">
            Examples
          </th>
        </tr>
      </thead>
      <tbody>
        {entries.map(([flag, s]) => (
          <tr
            key={flag}
            className="border-b border-border-subtle last:border-0"
          >
            <td className="py-1.5 pr-4 text-fg-primary font-mono">{flag}</td>
            <td
              className={`py-1.5 pr-4 text-right tabular-nums font-semibold ${acceptColor(s.score)}`}
            >
              {pct(s.score)}
            </td>
            <td className="py-1.5 text-right text-fg-tertiary tabular-nums">
              {s.count}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** Delta badge: +3.2 pp or -1.5 pp vs the prior run. */
function DeltaBadge({ current, prior }: { current: number; prior: number }) {
  const delta = (current - prior) * 100;
  if (Math.abs(delta) < 0.05) {
    return (
      <span className="text-xs text-fg-tertiary tabular-nums">no change</span>
    );
  }
  const positive = delta > 0;
  return (
    <span
      className={`text-xs font-semibold tabular-nums ${positive ? "text-severity-pass" : "text-severity-fail"}`}
      title={`${positive ? "+" : ""}${delta.toFixed(1)} pp vs previous run`}
    >
      {positive ? "+" : ""}
      {delta.toFixed(1)} pp
    </span>
  );
}

/** Latest run summary card. */
function LatestRunCard({
  run,
  prevRun,
}: {
  run: EvaluationRun;
  prevRun: EvaluationRun | null;
}) {
  const [showPerFlag, setShowPerFlag] = useState(false);
  const hasFlagData = Object.keys(run.per_flag_kind).length > 0;

  return (
    <article
      className="rounded-md border border-border-subtle bg-bg-surface px-4 py-3 flex flex-col gap-3"
      aria-label="Latest evaluation run"
    >
      <div className="flex items-baseline justify-between gap-3">
        <div className="flex items-baseline gap-3">
          <span
            className={`text-2xl font-semibold tabular-nums ${acceptColor(run.overall_score)}`}
          >
            {pct(run.overall_score)}
          </span>
          {prevRun && (
            <DeltaBadge
              current={run.overall_score}
              prior={prevRun.overall_score}
            />
          )}
          <span className="text-xs text-fg-tertiary">overall exact-match</span>
        </div>
        <div className="text-right text-xs text-fg-tertiary">
          <div title={run.ts}>{relativeTime(run.ts)}</div>
          <div className="font-mono">{run.prompt_template_version}</div>
          <div>
            {run.example_count} example{run.example_count === 1 ? "" : "s"}
          </div>
        </div>
      </div>

      <PerLocaleTable perLocale={run.per_locale} />

      {hasFlagData && (
        <div>
          <button
            type="button"
            onClick={() => setShowPerFlag((v) => !v)}
            className="text-xs text-fg-secondary underline decoration-dotted focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
            aria-expanded={showPerFlag}
          >
            {showPerFlag ? "Hide" : "Show"} per-flag-kind breakdown
          </button>
          {showPerFlag && (
            <div className="mt-2">
              <PerFlagTable perFlag={run.per_flag_kind} />
            </div>
          )}
        </div>
      )}
    </article>
  );
}

/** Collapsible run history table. */
function RunHistoryTable({ runs }: { runs: EvaluationRun[] }) {
  const [open, setOpen] = useState(false);

  if (runs.length === 0) return null;

  return (
    <div>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="text-xs text-fg-secondary underline decoration-dotted focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
        aria-expanded={open}
        aria-controls="eval-history-table"
      >
        {open ? "Hide" : "Show"} run history ({runs.length} run
        {runs.length === 1 ? "" : "s"})
      </button>

      {open && (
        <div
          id="eval-history-table"
          className="mt-2 overflow-auto rounded-md border border-border-subtle"
          style={{ maxHeight: "16rem" }}
        >
          <table
            className="w-full text-xs border-collapse"
            aria-label="Evaluation run history"
          >
            <thead>
              <tr className="bg-bg-surface border-b border-border-subtle text-fg-tertiary uppercase tracking-wider">
                <th scope="col" className="py-2 px-3 text-left font-medium">
                  Timestamp
                </th>
                <th scope="col" className="py-2 px-3 text-left font-medium">
                  Prompt version
                </th>
                <th scope="col" className="py-2 px-3 text-right font-medium">
                  Score
                </th>
                <th scope="col" className="py-2 px-3 text-right font-medium">
                  Examples
                </th>
              </tr>
            </thead>
            <tbody>
              {runs.map((r) => (
                <tr
                  key={r.ts}
                  className="border-b border-border-subtle last:border-0 hover:bg-bg-hover transition-colors duration-75"
                >
                  <td
                    className="py-1.5 px-3 text-fg-secondary whitespace-nowrap"
                    title={r.ts}
                  >
                    {formatTs(r.ts)}
                  </td>
                  <td className="py-1.5 px-3 text-fg-secondary font-mono">
                    {r.prompt_template_version}
                  </td>
                  <td
                    className={`py-1.5 px-3 text-right tabular-nums font-semibold ${acceptColor(r.overall_score)}`}
                  >
                    {pct(r.overall_score)}
                  </td>
                  <td className="py-1.5 px-3 text-right text-fg-tertiary tabular-nums">
                    {r.example_count}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

export function QualityPanel({ summary, flashError, flashInfo }: Props) {
  // ── Part 1: Headline stats state ──────────────────────────────────────────

  const [localeStats, setLocaleStats] = useState<LocaleStat[]>([]);
  const [statsLoading, setStatsLoading] = useState(false);

  // ── Corrections state ─────────────────────────────────────────────────────

  const [corrections, setCorrections] = useState<Correction[]>([]);
  const [corrLoading, setCorrLoading] = useState(false);

  // Filter controls
  const [filterCatalog, setFilterCatalog] = useState("");
  const [filterLocale, setFilterLocale] = useState("");
  const [filterUnitId, setFilterUnitId] = useState("");
  const [filterCuratedOnly, setFilterCuratedOnly] = useState(false);

  // ── Curated state ─────────────────────────────────────────────────────────

  const [curated, setCurated] = useState<CuratedExample[]>([]);
  const [curatedLoading, setCuratedLoading] = useState(false);

  // Derived set of curated ids for O(1) lookup in the corrections table.
  const curatedIds: Set<CorrectionId> = new Set(curated.map((e) => e.id));

  // ── Part 3: Evaluation state ──────────────────────────────────────────────

  const [evalRuns, setEvalRuns] = useState<EvaluationRun[]>([]);
  const [evalRunsLoading, setEvalRunsLoading] = useState(false);

  // ── Part 4: Tuning bundle state ───────────────────────────────────────────

  const [bundleExporting, setBundleExporting] = useState(false);
  const [pastBundles, setPastBundles] = useState<ExportTuningBundleResponse[]>(
    [],
  );
  const [bundlesLoading, setBundlesLoading] = useState(false);

  // In-flight job state.
  const [evalJobId, setEvalJobId] = useState<string | null>(null);
  const [evalProgress, setEvalProgress] =
    useState<EvaluationProgressPayload | null>(null);
  const evalUnlistenRef = useRef<(() => void) | null>(null);

  // ── Data fetching ─────────────────────────────────────────────────────────

  const fetchCurated = useCallback(async () => {
    setCuratedLoading(true);
    try {
      const data = await listCuratedInProject();
      setCurated(data);
    } catch (e) {
      flashError(`Failed to load curated set: ${formatError(e)}`);
    } finally {
      setCuratedLoading(false);
    }
  }, [flashError]);

  const fetchCorrections = useCallback(
    async (
      catalog: string,
      locale: string,
      unitId: string,
      curatedOnly: boolean,
    ) => {
      setCorrLoading(true);
      try {
        const filter: ListCorrectionsFilter = {
          catalog_path: catalog || null,
          locale: locale || null,
          unit_id: unitId || null,
          curated_only: curatedOnly,
        };
        const data = await listCorrectionsInProject(filter);
        setCorrections(data);
      } catch (e) {
        flashError(`Failed to load corrections: ${formatError(e)}`);
      } finally {
        setCorrLoading(false);
      }
    },
    [flashError],
  );

  const fetchEvalRuns = useCallback(async () => {
    setEvalRunsLoading(true);
    try {
      const data = await listEvaluationRunsInProject();
      setEvalRuns(data);
    } catch (e) {
      flashError(`Failed to load evaluation runs: ${formatError(e)}`);
    } finally {
      setEvalRunsLoading(false);
    }
  }, [flashError]);

  const fetchPastBundles = useCallback(async () => {
    setBundlesLoading(true);
    try {
      const data = await listTuningBundlesInProject();
      setPastBundles(data);
    } catch {
      // Silently ignore — the tuning root may not exist yet.
    } finally {
      setBundlesLoading(false);
    }
  }, []);

  /** Fetch per-locale correction stats for all project locales in parallel. */
  const fetchHeadlineStats = useCallback(async () => {
    const localeIds = summary.locales;
    if (localeIds.length === 0) return;

    setStatsLoading(true);
    try {
      // Fetch locale metadata (script field) and per-locale corrections in parallel.
      const [localeInfos, ...perLocaleCorrs] = await Promise.all([
        listLocales(),
        ...localeIds.map((id) =>
          listCorrectionsInProject({ locale: id }).catch(
            () => [] as Correction[],
          ),
        ),
      ]);

      const infoMap = new Map<string, LocaleInfo>(
        (localeInfos as LocaleInfo[]).map((li) => [li.id, li]),
      );

      const stats: LocaleStat[] = localeIds.map((id, idx) => {
        const info = infoMap.get(id);
        const corrs = perLocaleCorrs[idx] as Correction[];
        return computeStats(id, info?.script ?? "", corrs);
      });

      setLocaleStats(stats);
    } catch (e) {
      flashError(`Failed to compute headline numbers: ${formatError(e)}`);
    } finally {
      setStatsLoading(false);
    }
  }, [summary.locales, flashError]);

  // Debounce for the unit-id text field (200 ms).
  const unitIdTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Fetch on mount.
  useEffect(() => {
    void fetchCurated();
    void fetchCorrections("", "", "", false);
    void fetchEvalRuns();
    void fetchHeadlineStats();
    void fetchPastBundles();
  }, [
    fetchCurated,
    fetchCorrections,
    fetchEvalRuns,
    fetchHeadlineStats,
    fetchPastBundles,
  ]);

  // Refetch corrections when dropdown/checkbox filters change.
  // filterUnitId is intentionally excluded: handled via debounce below.
  // biome-ignore lint/correctness/useExhaustiveDependencies: filterUnitId handled via debounce
  useEffect(() => {
    if (unitIdTimerRef.current !== null) {
      clearTimeout(unitIdTimerRef.current);
      unitIdTimerRef.current = null;
    }
    void fetchCorrections(
      filterCatalog,
      filterLocale,
      filterUnitId,
      filterCuratedOnly,
    );
  }, [filterCatalog, filterLocale, filterCuratedOnly, fetchCorrections]);

  // Unit-id filter is debounced; separate effect.
  const handleUnitIdChange = useCallback(
    (v: string) => {
      setFilterUnitId(v);
      if (unitIdTimerRef.current !== null) clearTimeout(unitIdTimerRef.current);
      unitIdTimerRef.current = setTimeout(() => {
        void fetchCorrections(
          filterCatalog,
          filterLocale,
          v,
          filterCuratedOnly,
        );
      }, 200);
    },
    [filterCatalog, filterLocale, filterCuratedOnly, fetchCorrections],
  );

  // Clean up the debounce timer on unmount.
  useEffect(
    () => () => {
      if (unitIdTimerRef.current !== null) clearTimeout(unitIdTimerRef.current);
    },
    [],
  );

  // Clean up eval listener on unmount.
  useEffect(
    () => () => {
      evalUnlistenRef.current?.();
    },
    [],
  );

  // ── Actions ───────────────────────────────────────────────────────────────

  const refreshBoth = useCallback(async () => {
    await Promise.all([
      fetchCurated(),
      fetchCorrections(
        filterCatalog,
        filterLocale,
        filterUnitId,
        filterCuratedOnly,
      ),
    ]);
  }, [
    fetchCurated,
    fetchCorrections,
    filterCatalog,
    filterLocale,
    filterUnitId,
    filterCuratedOnly,
  ]);

  const handlePromote = useCallback(
    async (id: CorrectionId) => {
      try {
        await promoteCorrectionToCurated(id);
        flashInfo("Promoted to curated set.");
        await refreshBoth();
      } catch (e) {
        flashError(`Promote failed: ${formatError(e)}`);
      }
    },
    [flashInfo, flashError, refreshBoth],
  );

  const handleUncurate = useCallback(
    async (id: CorrectionId) => {
      try {
        await unCurateCorrection(id);
        flashInfo("Removed from curated set.");
        await refreshBoth();
      } catch (e) {
        flashError(`Un-curate failed: ${formatError(e)}`);
      }
    },
    [flashInfo, flashError, refreshBoth],
  );

  // Inline note update: promote is idempotent and accepts a fresh note.
  const handleNoteBlur = useCallback(
    async (id: CorrectionId, note: string) => {
      try {
        await promoteCorrectionToCurated(id, note || null);
        await fetchCurated();
      } catch (e) {
        flashError(`Note update failed: ${formatError(e)}`);
      }
    },
    [flashError, fetchCurated],
  );

  // ── Evaluation actions ────────────────────────────────────────────────────

  const handleRunEvaluation = useCallback(async () => {
    if (evalJobId !== null) return; // guard: already running

    try {
      // Subscribe before firing the command to avoid missing fast first-example.
      const unlisten = await listenEvalProgress(
        // Job id is not yet known; we use a placeholder and re-register once
        // the command returns. Because listenEvalProgress returns a combined
        // function, we teardown immediately after the real jobId is wired.
        // Alternative pattern: listen to a wildcard. Instead we do two-phase:
        // 1. Get the jobId from the command.
        // 2. Register listeners with the real jobId.
        // The window between command return and listener registration is
        // acceptable for evaluation (units take ~2 s each; the first emit
        // arrives well after registration).
        "__placeholder__",
        () => {},
        () => {},
      );
      // Tear down the placeholder immediately.
      unlisten();

      const started = await runEvaluationInProject();
      const { job_id: jobId, total } = started;

      setEvalJobId(jobId);
      setEvalProgress({
        job_id: jobId,
        completed: 0,
        total,
        last_example_locale: "",
      });

      const realUnlisten = await listenEvalProgress(
        jobId,
        (p: EvaluationProgressPayload) => {
          setEvalProgress(p);
        },
        (p: EvaluationTerminalPayload, status: "completed" | "failed") => {
          setEvalJobId(null);
          setEvalProgress(null);
          realUnlisten();
          evalUnlistenRef.current = null;

          if (status === "completed" && p.run) {
            flashInfo(
              `Evaluation complete — overall score: ${pct(p.run.overall_score)}`,
            );
            void fetchEvalRuns();
          } else if (status === "failed" && p.failed_reason) {
            flashError(`Evaluation failed: ${p.failed_reason}`);
          } else if (p.cancelled) {
            flashInfo("Evaluation cancelled.");
          }
        },
      );
      evalUnlistenRef.current = realUnlisten;
    } catch (e) {
      flashError(`Failed to start evaluation: ${formatError(e)}`);
      setEvalJobId(null);
      setEvalProgress(null);
    }
  }, [evalJobId, flashInfo, flashError, fetchEvalRuns]);

  const handleCancelEval = useCallback(async () => {
    if (!evalJobId) return;
    try {
      await cancelEvaluation(evalJobId);
    } catch (e) {
      flashError(`Cancel failed: ${formatError(e)}`);
    }
  }, [evalJobId, flashError]);

  // ── Tuning bundle action ──────────────────────────────────────────────────

  const handleExportBundle = useCallback(async () => {
    setBundleExporting(true);
    try {
      const result = await exportTuningBundleInProject();
      flashInfo(
        `Exported ${result.examples_count} example${result.examples_count === 1 ? "" : "s"} to ${result.path}`,
      );
      if (!result.has_score) {
        // Non-blocking advisory: suggest running evaluation first.
        flashInfo(
          "Tip: run an evaluation first so the bundle includes a score baseline.",
        );
      }
      await fetchPastBundles();
    } catch (e) {
      flashError(`Export failed: ${formatError(e)}`);
    } finally {
      setBundleExporting(false);
    }
  }, [flashInfo, flashError, fetchPastBundles]);

  // ── Catalog list for filter dropdown ─────────────────────────────────────

  const catalogPaths = summary.catalogs.map(
    (c) => c.manifest_path || c.absolute_path,
  );

  const isEvalRunning = evalJobId !== null;
  const latestRun = evalRuns.length > 0 ? evalRuns[0] : null;
  const prevRun = evalRuns.length > 1 ? evalRuns[1] : null;

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <section
      aria-label="Quality panel"
      className="flex-1 overflow-auto bg-bg-base p-6"
    >
      <div className="max-w-7xl mx-auto flex flex-col gap-8">
        {/* Part 1: Headline numbers per locale */}
        <section aria-labelledby="headline-heading">
          <h2
            id="headline-heading"
            className="text-xs font-semibold uppercase tracking-wider text-fg-tertiary mb-3"
          >
            Headline numbers by locale
          </h2>
          <HeadlineCards stats={localeStats} loading={statsLoading} />
        </section>

        {/* Translation memory section */}
        <section aria-labelledby="corrections-heading">
          <div className="flex items-center justify-between mb-3">
            <h2
              id="corrections-heading"
              className="text-sm font-semibold text-fg-primary"
            >
              Translation memory
              {corrLoading && corrections.length === 0 && (
                <span className="ml-2 inline-flex items-center gap-1 text-fg-tertiary font-normal text-xs">
                  <Spinner /> Loading…
                </span>
              )}
            </h2>
            {corrections.length > 0 && !corrLoading && (
              <span className="text-xs text-fg-tertiary">
                {corrections.length} correction
                {corrections.length === 1 ? "" : "s"}
              </span>
            )}
          </div>

          <div className="mb-3">
            <FilterBar
              catalogs={catalogPaths}
              locales={summary.locales}
              catalog={filterCatalog}
              locale={filterLocale}
              unitId={filterUnitId}
              curatedOnly={filterCuratedOnly}
              onCatalogChange={(v) => setFilterCatalog(v)}
              onLocaleChange={(v) => setFilterLocale(v)}
              onUnitIdChange={handleUnitIdChange}
              onCuratedOnlyChange={(v) => setFilterCuratedOnly(v)}
            />
          </div>

          <CorrectionsTable
            corrections={corrections}
            curatedIds={curatedIds}
            loading={corrLoading}
            onPromote={(id) => void handlePromote(id)}
            onUncurate={(id) => void handleUncurate(id)}
          />
        </section>

        {/* Curated set section */}
        <section aria-labelledby="curated-heading">
          <div className="flex items-center justify-between mb-3">
            <h2
              id="curated-heading"
              className="text-sm font-semibold text-fg-primary"
            >
              Curated set — {curated.length} example
              {curated.length === 1 ? "" : "s"}
              {curatedLoading && curated.length === 0 && (
                <span className="ml-2 inline-flex items-center gap-1 text-fg-tertiary font-normal text-xs">
                  <Spinner /> Loading…
                </span>
              )}
            </h2>
          </div>

          <CuratedTable
            examples={curated}
            loading={curatedLoading}
            onUncurate={(id) => void handleUncurate(id)}
            onNoteBlur={(id, note) => void handleNoteBlur(id, note)}
          />
        </section>

        {/* Part 3: Prompt evaluation */}
        <section aria-labelledby="eval-heading">
          <div className="flex items-center justify-between mb-3">
            <h2
              id="eval-heading"
              className="text-sm font-semibold text-fg-primary"
            >
              Prompt evaluation
            </h2>
            <button
              type="button"
              onClick={() => void handleRunEvaluation()}
              disabled={isEvalRunning || curated.length === 0}
              aria-label={
                curated.length === 0
                  ? "Run evaluation — requires at least one curated example"
                  : isEvalRunning
                    ? "Evaluation in progress…"
                    : "Run evaluation over curated examples"
              }
              title={
                curated.length === 0
                  ? "Curate at least one correction before running an evaluation."
                  : undefined
              }
              className="h-7 px-3 rounded border border-border-default bg-bg-surface text-xs text-fg-secondary hover:bg-bg-hover hover:border-border-strong active:bg-bg-selected disabled:opacity-40 disabled:cursor-not-allowed transition-colors duration-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
            >
              {isEvalRunning ? (
                <span className="inline-flex items-center gap-1.5">
                  <Spinner /> Running…
                </span>
              ) : (
                "Run evaluation"
              )}
            </button>
          </div>

          {/* In-flight progress */}
          {isEvalRunning && evalProgress && (
            <div className="mb-4">
              <EvalProgressBar
                completed={evalProgress.completed}
                total={evalProgress.total}
                lastLocale={evalProgress.last_example_locale}
                jobId={evalJobId ?? ""}
                onCancel={() => void handleCancelEval()}
              />
            </div>
          )}

          {/* No curated examples banner */}
          {!isEvalRunning && curated.length === 0 && (
            <p className="py-3 text-sm text-fg-tertiary">
              Curate at least one correction above to enable evaluation.
            </p>
          )}

          {/* Latest run summary */}
          {!isEvalRunning && latestRun && (
            <div className="flex flex-col gap-3">
              <LatestRunCard run={latestRun} prevRun={prevRun ?? null} />
              <RunHistoryTable runs={evalRuns.slice(1)} />
            </div>
          )}

          {/* No runs yet */}
          {!isEvalRunning &&
            !evalRunsLoading &&
            curated.length > 0 &&
            evalRuns.length === 0 && (
              <p className="py-3 text-sm text-fg-tertiary">
                No evaluation runs yet. Press "Run evaluation" to score the
                current prompt over your curated set.
              </p>
            )}

          {evalRunsLoading && evalRuns.length === 0 && (
            <div className="flex items-center gap-2 text-xs text-fg-tertiary py-3">
              <Spinner /> Loading evaluation history…
            </div>
          )}
        </section>

        {/* Part 4: Tuning bundle */}
        <section aria-labelledby="bundle-heading">
          <div className="flex items-center justify-between mb-3">
            <div>
              <h2
                id="bundle-heading"
                className="text-sm font-semibold text-fg-primary"
              >
                Tuning bundle
              </h2>
              <p className="text-xs text-fg-tertiary mt-0.5">
                Export the curated set + current prompt + latest score as a
                self-contained bundle for the Claude Code{" "}
                <span className="font-mono">tune-i18n-prompt</span> skill.
              </p>
            </div>
            <button
              type="button"
              onClick={() => void handleExportBundle()}
              disabled={bundleExporting || curated.length === 0}
              aria-label={
                curated.length === 0
                  ? "Export bundle — requires at least one curated example"
                  : bundleExporting
                    ? "Exporting bundle…"
                    : "Export tuning bundle"
              }
              title={
                curated.length === 0
                  ? "Curate at least one correction before exporting a bundle."
                  : undefined
              }
              className="flex-shrink-0 h-7 px-3 rounded border border-border-default bg-bg-surface text-xs text-fg-secondary hover:bg-bg-hover hover:border-border-strong active:bg-bg-selected disabled:opacity-40 disabled:cursor-not-allowed transition-colors duration-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
            >
              {bundleExporting ? (
                <span className="inline-flex items-center gap-1.5">
                  <Spinner /> Exporting…
                </span>
              ) : (
                "Export bundle"
              )}
            </button>
          </div>

          {/* Past bundles list */}
          {bundlesLoading && pastBundles.length === 0 && (
            <div className="flex items-center gap-2 text-xs text-fg-tertiary py-2">
              <Spinner /> Loading bundle history…
            </div>
          )}

          {!bundlesLoading && pastBundles.length === 0 && (
            <p className="py-2 text-xs text-fg-tertiary">
              No bundles exported yet.
            </p>
          )}

          {pastBundles.length > 0 && (
            <div className="rounded border border-border-subtle overflow-hidden text-xs">
              <table className="w-full border-collapse">
                <thead>
                  <tr className="bg-bg-surface border-b border-border-subtle text-fg-tertiary text-left">
                    <th scope="col" className="py-1.5 px-3 font-medium">
                      Directory
                    </th>
                    <th scope="col" className="py-1.5 px-3 font-medium">
                      Locales
                    </th>
                    <th
                      scope="col"
                      className="py-1.5 px-3 font-medium text-right"
                    >
                      Examples
                    </th>
                    <th
                      scope="col"
                      className="py-1.5 px-3 font-medium text-right"
                    >
                      Score
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {pastBundles.map((b) => {
                    const dirName = b.path.replace(/\\/g, "/").split("/").pop();
                    return (
                      <tr
                        key={b.path}
                        className="border-b border-border-subtle last:border-0 hover:bg-bg-hover transition-colors duration-75"
                      >
                        <td
                          className="py-1.5 px-3 text-fg-secondary font-mono whitespace-nowrap"
                          title={b.path}
                        >
                          {dirName ?? b.path}
                        </td>
                        <td className="py-1.5 px-3 text-fg-secondary">
                          {b.locales.join(", ") || "—"}
                        </td>
                        <td className="py-1.5 px-3 text-right tabular-nums text-fg-secondary">
                          {b.examples_count}
                        </td>
                        <td className="py-1.5 px-3 text-right text-fg-tertiary">
                          {b.has_score ? "included" : "none"}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}
        </section>
      </div>
    </section>
  );
}
