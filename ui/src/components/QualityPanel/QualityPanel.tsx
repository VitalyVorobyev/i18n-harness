// Quality view — translation memory (corrections) + curated set.
//
// M4.3d: corrections table with filter bar, curated-set table, and a
// value-proposition banner explaining what M4.9 will add. Headline
// numbers, time-series charts, "Run evaluation", and "Export tuning
// bundle" are all deferred to M4.9 / M4.10.

import { useCallback, useEffect, useRef, useState } from "react";
import {
  type ListCorrectionsFilter,
  listCorrectionsInProject,
  listCuratedInProject,
  promoteCorrectionToCurated,
  unCurateCorrection,
} from "../../lib/tauri";
import type {
  Correction,
  CorrectionId,
  CuratedExample,
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
          the user types the full unit id. A future improvement could add a
          server-side LIKE filter, but that requires a library-layer change. */}
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

// ── Main component ────────────────────────────────────────────────────────────

export function QualityPanel({ summary, flashError, flashInfo }: Props) {
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

  // Debounce for the unit-id text field (200 ms as specified).
  const unitIdTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Fetch on mount. fetchCurated and fetchCorrections are stable (useCallback
  // deps are [flashError] only) so including them here does not cause a loop.
  useEffect(() => {
    void fetchCurated();
    void fetchCorrections("", "", "", false);
  }, [fetchCurated, fetchCorrections]);

  // Refetch corrections when dropdown/checkbox filters change.
  // filterUnitId is intentionally excluded here: unit-id changes go through
  // the debounced handleUnitIdChange path instead to avoid a fetch per keystroke.
  // Cancel any pending debounce timer so stale unit-id requests cannot race
  // a freshly-triggered filter-change fetch.
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

  // ── Catalog list for filter dropdown ─────────────────────────────────────

  const catalogPaths = summary.catalogs.map(
    (c) => c.manifest_path || c.absolute_path,
  );

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <section
      aria-label="Quality panel"
      className="flex-1 overflow-auto bg-bg-base p-6"
    >
      <div className="max-w-7xl mx-auto flex flex-col gap-8">
        {/* Value-proposition banner */}
        <section aria-labelledby="quality-intro-heading">
          <h2
            id="quality-intro-heading"
            className="text-xs font-semibold uppercase tracking-wider text-fg-tertiary mb-2"
          >
            About this view
          </h2>
          <div className="rounded-md border border-border-subtle bg-bg-surface px-4 py-3 text-xs text-fg-secondary leading-relaxed max-w-3xl">
            Every human edit accepted in the editor is recorded as a correction.
            Curate the ones that represent the right per-locale translation
            style — they become the prompt-tuning corpus. The full Quality
            dashboard (acceptance rate, edit rate, run-evaluation, export tuning
            bundle) ships in M4.9 / M4.10.
          </div>
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
      </div>
    </section>
  );
}
