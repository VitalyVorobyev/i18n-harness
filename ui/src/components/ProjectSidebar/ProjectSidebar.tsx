import { cn } from "../../lib/cn";
import type {
  CatalogFormat,
  CatalogRef,
  CatalogResponse,
  ProjectSummary,
} from "../../lib/types";
import { CatalogActions } from "../CatalogActions";
import { LocaleTag, SegmentBar } from "../primitives";

interface Props {
  summary: ProjectSummary;
  activeCatalogPath: string | null;
  dirtyCatalogPaths: Set<string>;
  // Empty set = show all; non-empty = show only catalogs whose locale is in the set.
  activeLocaleFilter: Set<string>;
  onCatalogSelect: (absolutePath: string) => void;
  /** Total review-queue count across the project (drives the sidebar badge). */
  reviewQueueTotal?: number;
  /** Per-catalog review counts keyed by absolute path. */
  reviewQueueByCatalog?: Record<string, number>;
  /** Called when the user clicks the project-wide review badge. */
  onOpenReviewQueue?: () => void;
  /**
   * All catalogs that have been opened this session, keyed by absolute path.
   * Used to derive per-catalog progress stats (finished / proposed / total).
   * Only opened catalogs have stats — unloaded catalogs render a skeleton bar.
   */
  openCatalogs?: Map<string, CatalogResponse>;
  /** Absolute paths with a reuse/split/merge IPC call in flight. */
  reuseBusyPaths?: Set<string>;
  /** Apply manifest (or ad-hoc) reference translations into this catalog. */
  onApplyReferences?: (catalogPath: string) => void;
  /** Carve the writable-untranslated remainder of this catalog to a `.ts`. */
  onExportRemainder?: (catalogPath: string) => void;
  /** Fold a translated remainder `.ts` back into this catalog. */
  onMergeCatalog?: (catalogPath: string) => void;
}

type CatalogProgressStats = {
  finished: number;
  proposed: number;
  total: number;
};

/** Derive finished/proposed/total counts from a loaded CatalogResponse. */
function catalogStatsOf(resp: CatalogResponse): CatalogProgressStats {
  let finished = 0;
  let proposed = 0;
  const total = resp.units.length;
  for (const u of resp.units) {
    if (u.state === "finished") finished++;
    else if (u.state === "proposed") proposed++;
  }
  return { finished, proposed, total };
}

export function ProjectSidebar({
  summary,
  activeCatalogPath,
  dirtyCatalogPaths,
  activeLocaleFilter,
  onCatalogSelect,
  reviewQueueTotal = 0,
  reviewQueueByCatalog = {},
  onOpenReviewQueue,
  openCatalogs = new Map(),
  reuseBusyPaths,
  onApplyReferences,
  onExportRemainder,
  onMergeCatalog,
}: Props) {
  const totalCatalogs = summary.catalogs.length;

  const visibleCatalogs =
    activeLocaleFilter.size === 0
      ? summary.catalogs
      : summary.catalogs.filter((c) => activeLocaleFilter.has(c.locale));

  return (
    <aside
      className={cn(
        "flex flex-col shrink-0 w-56 min-h-0 h-full",
        "bg-bg-surface border-r border-border-subtle",
      )}
      aria-label="Project sidebar"
    >
      {/* Header */}
      <div className="px-3 pt-3 pb-2 border-b border-border-subtle">
        <div className="flex items-center gap-2">
          <p
            className="text-sm font-semibold text-fg-primary truncate flex-1 min-w-0"
            title={summary.name}
          >
            {summary.name}
          </p>
          {reviewQueueTotal > 0 && (
            <button
              type="button"
              onClick={onOpenReviewQueue}
              className={cn(
                "shrink-0 inline-flex items-center h-5 px-1.5 rounded-pill border",
                "text-[10px] font-semibold tabular-nums leading-none",
                "bg-severity-soft-bg border-severity-soft-border text-severity-soft",
                "hover:opacity-80 transition-opacity duration-75",
                "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
              )}
              title={`${reviewQueueTotal} ${reviewQueueTotal === 1 ? "unit needs" : "units need"} review — click to open review queue`}
              aria-label={`${reviewQueueTotal} units need review. Open review queue.`}
            >
              ⚑{reviewQueueTotal}
            </button>
          )}
        </div>
        <p className="text-xs text-fg-tertiary mt-0.5">
          {activeLocaleFilter.size > 0
            ? `${visibleCatalogs.length} of ${totalCatalogs} ${totalCatalogs === 1 ? "catalog" : "catalogs"}`
            : `${totalCatalogs} ${totalCatalogs === 1 ? "catalog" : "catalogs"}`}
        </p>
      </div>

      {/* Catalog list */}
      <nav
        className="flex-1 overflow-y-auto py-1"
        aria-label="Project catalogs"
      >
        {summary.catalogs.length === 0 ? (
          <p className="px-3 py-2 text-xs text-fg-disabled">
            No catalogs declared.
          </p>
        ) : visibleCatalogs.length === 0 ? (
          <p className="px-3 py-2 text-xs text-fg-disabled">
            No catalogs match the active locale filter.
          </p>
        ) : (
          <ul>
            {visibleCatalogs.map((ref) => {
              const loaded = openCatalogs.get(ref.absolute_path);
              return (
                <CatalogItem
                  key={ref.absolute_path}
                  catalogRef={ref}
                  isActive={activeCatalogPath === ref.absolute_path}
                  isDirty={dirtyCatalogPaths.has(ref.absolute_path)}
                  reviewCount={reviewQueueByCatalog[ref.absolute_path] ?? 0}
                  catalogStats={
                    loaded !== undefined ? catalogStatsOf(loaded) : null
                  }
                  onClick={() => onCatalogSelect(ref.absolute_path)}
                  reuseBusy={reuseBusyPaths?.has(ref.absolute_path) ?? false}
                  onApplyReferences={onApplyReferences}
                  onExportRemainder={onExportRemainder}
                  onMergeCatalog={onMergeCatalog}
                />
              );
            })}
          </ul>
        )}
      </nav>
    </aside>
  );
}

// ── CatalogItem ────────────────────────────────────────────────────────────────

function CatalogItem({
  catalogRef,
  isActive,
  isDirty,
  reviewCount,
  catalogStats: stats,
  onClick,
  reuseBusy,
  onApplyReferences,
  onExportRemainder,
  onMergeCatalog,
}: {
  catalogRef: CatalogRef;
  isActive: boolean;
  isDirty: boolean;
  reviewCount: number;
  /**
   * null when the catalog has not been opened yet (skeleton bar rendered).
   * Populated as soon as the catalog is extracted and cached.
   */
  catalogStats: CatalogProgressStats | null;
  onClick: () => void;
  reuseBusy: boolean;
  onApplyReferences?: (catalogPath: string) => void;
  onExportRemainder?: (catalogPath: string) => void;
  onMergeCatalog?: (catalogPath: string) => void;
}) {
  // Display the manifest-relative path; fall back to the absolute path's
  // last two segments if the manifest path is just a filename.
  const displayPath = catalogRef.manifest_path || catalogRef.absolute_path;
  const segments = displayPath.replace(/\\/g, "/").split("/");
  const filename = segments[segments.length - 1] ?? displayPath;
  const dir =
    segments.length > 1 ? `${segments.slice(0, -1).join("/")}/` : null;

  // Reuse/split/merge are Qt-only and only wired when handlers are supplied.
  const actionsEnabled =
    catalogRef.format === "qt-ts" &&
    onApplyReferences !== undefined &&
    onExportRemainder !== undefined &&
    onMergeCatalog !== undefined;

  return (
    // The nav button and the actions menu are siblings — the menu must not be
    // nested inside the row <button> (invalid interactive nesting).
    <li className="relative group">
      <button
        type="button"
        onClick={onClick}
        aria-current={isActive ? "page" : undefined}
        className={cn(
          "w-full flex flex-col gap-1.5 px-3 py-2 text-left",
          "transition-colors duration-75 ease-out",
          "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-accent",
          isActive
            ? "bg-accent-subtle text-fg-primary"
            : "text-fg-secondary hover:bg-bg-hover hover:text-fg-primary",
        )}
        title={catalogRef.absolute_path}
      >
        {/* Row 1: locale tag + filename + dirty mark + review badge */}
        <div className="flex items-center gap-2 w-full min-w-0">
          {/* Locale chip — upgraded to LocaleTag primitive */}
          <LocaleTag
            locale={catalogRef.locale}
            tone={isActive ? "accent" : "muted"}
          />

          <div className="min-w-0 flex-1">
            {/* Directory prefix */}
            {dir && (
              <p className="text-[10px] font-mono text-fg-disabled truncate leading-none mb-0.5">
                {dir}
              </p>
            )}

            {/* Filename + dirty mark + review badge.
                pr-5 reserves room for the absolutely-positioned actions menu. */}
            <div className="flex items-center gap-1 pr-5">
              <p className="font-mono text-xs truncate flex-1 min-w-0">
                {filename}
              </p>

              {/* Dirty mark — small mono bullet; title provides the accessible label */}
              {isDirty && (
                <span
                  role="img"
                  aria-label="Unsaved changes"
                  className="text-[10px] leading-none shrink-0 select-none"
                  style={{ color: "var(--color-fg-tertiary)" }}
                  title="Unsaved changes"
                >
                  •
                </span>
              )}

              {/* Needs-review badge */}
              {reviewCount > 0 && (
                <span
                  className={cn(
                    "shrink-0 inline-flex items-center h-4 px-1 rounded-sm border",
                    "text-[10px] font-semibold tabular-nums leading-none",
                    "bg-severity-soft-bg border-severity-soft-border text-severity-soft",
                  )}
                  title={`${reviewCount} ${reviewCount === 1 ? "unit needs" : "units need"} review`}
                >
                  <span className="sr-only">
                    {reviewCount} units need review
                  </span>
                  <span aria-hidden="true">{reviewCount}</span>
                </span>
              )}
            </div>

            {/* Format label */}
            <span className="text-[10px] text-fg-disabled mt-0.5 block">
              {formatLabel(catalogRef.format)}
            </span>
          </div>
        </div>

        {/* Row 2: stacked progress bar — only when catalog is loaded */}
        {stats !== null ? (
          <SegmentBar
            finished={stats.finished}
            proposed={stats.proposed}
            total={stats.total}
            height={4}
          />
        ) : (
          /* Skeleton bar for unloaded catalogs */
          <div
            style={{
              height: 4,
              borderRadius: 999,
              background: "var(--color-bg-input)",
              width: "100%",
            }}
            aria-hidden="true"
          />
        )}
      </button>

      {actionsEnabled &&
        onApplyReferences &&
        onExportRemainder &&
        onMergeCatalog && (
          <div className="absolute top-2 right-2 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-within:opacity-100 transition-opacity duration-75">
            <CatalogActions
              catalogPath={catalogRef.absolute_path}
              displayName={displayPath}
              busy={reuseBusy}
              enabled={actionsEnabled}
              onApplyReferences={onApplyReferences}
              onExportRemainder={onExportRemainder}
              onMerge={onMergeCatalog}
            />
          </div>
        )}
    </li>
  );
}

function formatLabel(format: CatalogFormat): string {
  switch (format) {
    case "qt-ts":
      return "Qt TS";
    case "gettext-po":
      return "Gettext PO";
    case "icu-json":
      return "ICU JSON";
  }
}
