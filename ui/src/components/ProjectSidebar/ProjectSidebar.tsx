import { cn } from "../../lib/cn";
import type {
  CatalogFormat,
  CatalogRef,
  ProjectSummary,
} from "../../lib/types";

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
            {visibleCatalogs.map((ref) => (
              <CatalogItem
                key={ref.absolute_path}
                catalogRef={ref}
                isActive={activeCatalogPath === ref.absolute_path}
                isDirty={dirtyCatalogPaths.has(ref.absolute_path)}
                reviewCount={reviewQueueByCatalog[ref.absolute_path] ?? 0}
                onClick={() => onCatalogSelect(ref.absolute_path)}
              />
            ))}
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
  onClick,
}: {
  catalogRef: CatalogRef;
  isActive: boolean;
  isDirty: boolean;
  reviewCount: number;
  onClick: () => void;
}) {
  // Display the manifest-relative path; fall back to the absolute path's
  // last two segments if the manifest path is just a filename.
  const displayPath = catalogRef.manifest_path || catalogRef.absolute_path;
  const segments = displayPath.replace(/\\/g, "/").split("/");
  const filename = segments[segments.length - 1] ?? displayPath;
  const dir =
    segments.length > 1 ? `${segments.slice(0, -1).join("/")}/` : null;

  return (
    <li>
      <button
        type="button"
        onClick={onClick}
        aria-current={isActive ? "page" : undefined}
        className={cn(
          "group w-full flex items-start gap-2 px-3 py-2 text-left",
          "transition-colors duration-75 ease-out",
          "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-accent",
          isActive
            ? "bg-accent-subtle text-fg-primary"
            : "text-fg-secondary hover:bg-bg-hover hover:text-fg-primary",
        )}
        title={catalogRef.absolute_path}
      >
        {/* Locale chip */}
        <span
          className={cn(
            "inline-flex items-center h-5 px-1.5 rounded-pill border shrink-0 mt-0.5",
            "font-mono text-[10px] font-medium tracking-loose",
            isActive
              ? "border-accent-subtle-border bg-accent-subtle text-accent-hover"
              : "border-border-subtle bg-bg-surface text-fg-tertiary group-hover:border-border-default",
          )}
          title={`Locale: ${catalogRef.locale}`}
        >
          {catalogRef.locale}
        </span>

        <div className="min-w-0 flex-1">
          {/* Directory prefix */}
          {dir && (
            <p className="text-[10px] font-mono text-fg-disabled truncate leading-none mb-0.5">
              {dir}
            </p>
          )}
          {/* Filename + dirty indicator + review count */}
          <div className="flex items-center gap-1">
            <p className="font-mono text-xs truncate flex-1 min-w-0">
              {filename}
            </p>
            {isDirty && (
              <span
                className="text-state-proposed text-xs leading-none shrink-0"
                title="Unsaved changes"
              >
                •<span className="sr-only"> (unsaved)</span>
              </span>
            )}
            {reviewCount > 0 && (
              <span
                className={cn(
                  "shrink-0 inline-flex items-center h-4 px-1 rounded-sm border",
                  "text-[10px] font-semibold tabular-nums leading-none",
                  "bg-severity-soft-bg border-severity-soft-border text-severity-soft",
                )}
                title={`${reviewCount} ${reviewCount === 1 ? "unit needs" : "units need"} review`}
              >
                <span className="sr-only">{reviewCount} units need review</span>
                <span aria-hidden="true">{reviewCount}</span>
              </span>
            )}
          </div>
          {/* Format chip */}
          <span className="text-[10px] text-fg-disabled mt-0.5 block">
            {formatLabel(catalogRef.format)}
          </span>
        </div>
      </button>
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
