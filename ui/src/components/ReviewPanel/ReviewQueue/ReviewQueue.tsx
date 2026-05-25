import { useMemo, useState } from "react";
import { cn } from "../../../lib/cn";
import type {
  ReviewQueueItem,
  ReviewQueueResponse,
  UnitId,
} from "../../../lib/types";

interface Props {
  reviewQueue: ReviewQueueResponse;
  /** Called when the user clicks "Open" on a queue item. */
  onOpenItem: (catalogPath: string, unitId: UnitId) => void;
}

export function ReviewQueue({ reviewQueue, onOpenItem }: Props) {
  const { total_count, by_catalog, items } = reviewQueue;
  const catalogCount = Object.keys(by_catalog).length;

  // Optional: filter by catalog path (clicking a catalog column value).
  const [catalogFilter, setCatalogFilter] = useState<string | null>(null);

  const visible = useMemo(() => {
    if (!catalogFilter) return items;
    return items.filter((it) => it.catalog_path === catalogFilter);
  }, [items, catalogFilter]);

  if (total_count === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center gap-3 text-fg-tertiary p-8 text-center">
        <span
          aria-hidden="true"
          className="text-3xl select-none text-fg-disabled"
        >
          ✓
        </span>
        <p className="text-sm font-medium text-fg-secondary">
          No units need review. Great work.
        </p>
        <p className="text-xs text-fg-tertiary">
          Units will appear here when they carry flags or are marked
          &ldquo;Needs review&rdquo;.
        </p>
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col overflow-hidden min-h-0">
      {/* Header */}
      <div className="shrink-0 px-4 pt-4 pb-3 border-b border-border-subtle bg-bg-surface">
        <div className="flex items-center gap-3 flex-wrap">
          <h2 className="text-sm font-semibold text-fg-primary">
            Review queue
          </h2>
          <span
            className={cn(
              "inline-flex items-center h-5 px-2 rounded-pill border",
              "text-xs font-medium tabular-nums",
              "bg-severity-soft-bg border-severity-soft-border text-severity-soft",
            )}
            title={`${total_count} units across ${catalogCount} catalogs need review`}
          >
            <span className="sr-only">
              {total_count} units across {catalogCount} catalogs need review
            </span>
            <span aria-hidden="true">
              {total_count} {total_count === 1 ? "unit" : "units"} &middot;{" "}
              {catalogCount} {catalogCount === 1 ? "catalog" : "catalogs"}
            </span>
          </span>
          {catalogFilter && (
            <button
              type="button"
              onClick={() => setCatalogFilter(null)}
              className={cn(
                "inline-flex items-center h-5 px-2 rounded-pill border text-xs font-medium",
                "border-border-subtle bg-transparent text-fg-tertiary",
                "hover:border-border-default hover:text-fg-secondary",
                "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
              )}
              aria-label="Clear catalog filter"
            >
              Clear filter
            </button>
          )}
        </div>
        {catalogFilter && (
          <p className="mt-1 text-xs text-fg-tertiary font-mono truncate">
            Filtering: {catalogFilter}
          </p>
        )}
      </div>

      {/* Scrollable table */}
      <div className="flex-1 overflow-auto min-h-0">
        <table
          className="w-full text-xs border-collapse"
          aria-label="Review queue"
        >
          <thead className="sticky top-0 z-10 bg-bg-surface border-b border-border-subtle">
            <tr>
              <Th>Catalog</Th>
              <Th>Locale</Th>
              <Th>Unit ID</Th>
              <Th>Source</Th>
              <Th>Target</Th>
              <Th>Flags</Th>
              <Th>Status</Th>
              <Th>State</Th>
              <Th>
                <span className="sr-only">Actions</span>
              </Th>
            </tr>
          </thead>
          <tbody>
            {visible.map((item) => (
              <ReviewRow
                key={`${item.catalog_path}::${item.unit_id}`}
                item={item}
                onOpen={onOpenItem}
                onFilterCatalog={(p) =>
                  setCatalogFilter((prev) => (prev === p ? null : p))
                }
                isCatalogFiltered={catalogFilter === item.catalog_path}
              />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ── Sub-components ─────────────────────────────────────────────────────────────

function Th({ children }: { children: React.ReactNode }) {
  return (
    <th
      scope="col"
      className="px-3 py-2 text-left font-medium text-fg-tertiary whitespace-nowrap"
    >
      {children}
    </th>
  );
}

function ReviewRow({
  item,
  onOpen,
  onFilterCatalog,
  isCatalogFiltered,
}: {
  item: ReviewQueueItem;
  onOpen: (catalogPath: string, unitId: UnitId) => void;
  onFilterCatalog: (catalogPath: string) => void;
  isCatalogFiltered: boolean;
}) {
  // Shorten manifest path for display: show last two segments.
  const displayPath = (() => {
    const segments = item.catalog_manifest_path.replace(/\\/g, "/").split("/");
    return segments.length > 2
      ? `…/${segments.slice(-2).join("/")}`
      : item.catalog_manifest_path;
  })();

  return (
    <tr
      className={cn(
        "border-b border-border-subtle",
        "hover:bg-bg-hover transition-colors duration-75",
        isCatalogFiltered && "bg-accent-subtle",
      )}
    >
      {/* Catalog column — clickable to filter */}
      <td className="px-3 py-2 max-w-[140px]">
        <button
          type="button"
          onClick={() => onFilterCatalog(item.catalog_path)}
          className={cn(
            "font-mono text-[10px] text-left truncate block w-full max-w-full",
            isCatalogFiltered
              ? "text-accent-hover font-semibold"
              : "text-fg-secondary hover:text-fg-primary",
            "focus-visible:outline-none focus-visible:underline",
          )}
          title={item.catalog_path}
          aria-pressed={isCatalogFiltered}
          aria-label={`Filter to catalog ${item.catalog_manifest_path}`}
        >
          {displayPath}
        </button>
      </td>

      {/* Locale */}
      <td className="px-3 py-2 whitespace-nowrap">
        <span
          className={cn(
            "inline-flex items-center h-5 px-1.5 rounded-pill border",
            "font-mono text-[10px] font-medium",
            "border-border-subtle bg-bg-surface text-fg-tertiary",
          )}
        >
          {item.locale}
        </span>
      </td>

      {/* Unit ID */}
      <td className="px-3 py-2 max-w-[120px]">
        <span
          className="font-mono text-[10px] text-fg-secondary block truncate"
          title={item.unit_id}
        >
          {item.unit_id}
        </span>
      </td>

      {/* Source preview */}
      <td className="px-3 py-2 max-w-[160px]">
        <span
          className="text-fg-primary block truncate leading-snug"
          title={item.source_preview}
        >
          {item.source_preview || (
            <span className="italic text-fg-disabled">empty</span>
          )}
        </span>
      </td>

      {/* Target preview */}
      <td className="px-3 py-2 max-w-[160px]">
        <span
          className={cn(
            "block truncate leading-snug",
            item.target_preview ? "text-fg-primary" : "italic text-fg-disabled",
          )}
          title={item.target_preview || undefined}
        >
          {item.target_preview || "—"}
        </span>
      </td>

      {/* Flags */}
      <td className="px-3 py-2">
        {item.flags.length > 0 ? (
          <div className="flex flex-wrap gap-1" title={item.flags.join(", ")}>
            <span className="sr-only">Flags: {item.flags.join(", ")}</span>
            {item.flags.slice(0, 3).map((f) => (
              <FlagChip key={f} flag={f} />
            ))}
            {item.flags.length > 3 && (
              <span className="inline-flex items-center h-4 px-1 rounded-sm text-[10px] text-fg-tertiary bg-bg-elevated border border-border-subtle">
                +{item.flags.length - 3}
              </span>
            )}
          </div>
        ) : (
          <span className="text-fg-disabled">—</span>
        )}
      </td>

      {/* Review status */}
      <td className="px-3 py-2 whitespace-nowrap">
        {item.review_status ? (
          <ReviewStatusChip status={item.review_status} />
        ) : (
          <span className="text-fg-disabled">—</span>
        )}
      </td>

      {/* State */}
      <td className="px-3 py-2 whitespace-nowrap">
        <StateChip state={item.state} />
      </td>

      {/* Open button */}
      <td className="px-3 py-2 whitespace-nowrap">
        <button
          type="button"
          onClick={() => onOpen(item.catalog_path, item.unit_id)}
          className={cn(
            "inline-flex items-center h-6 px-2 rounded-md border text-xs font-medium",
            "border-border-default bg-transparent text-fg-secondary",
            "hover:bg-bg-hover hover:text-fg-primary hover:border-border-strong",
            "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
            "transition-colors duration-75",
          )}
          aria-label={`Open unit ${item.unit_id} in ${item.catalog_manifest_path}`}
        >
          Open
        </button>
      </td>
    </tr>
  );
}

function FlagChip({ flag }: { flag: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center h-4 px-1 rounded-sm border",
        "text-[10px] font-medium leading-none",
        "bg-severity-soft-bg border-severity-soft-border text-severity-soft",
      )}
    >
      {flag}
    </span>
  );
}

function ReviewStatusChip({ status }: { status: string }) {
  const isNeeds = status === "needs-review";
  return (
    <span
      className={cn(
        "inline-flex items-center h-4 px-1.5 rounded-pill border text-[10px] font-medium",
        isNeeds
          ? "bg-severity-soft-bg border-severity-soft-border text-severity-soft"
          : "bg-bg-elevated border-border-subtle text-fg-tertiary",
      )}
    >
      {status}
    </span>
  );
}

function StateChip({ state }: { state: string }) {
  const colors: Record<string, string> = {
    untranslated: "text-fg-tertiary",
    proposed: "text-state-proposed",
    finished: "text-state-finished",
    vanished: "text-fg-disabled",
    obsolete: "text-fg-disabled",
  };
  return (
    <span
      className={cn(
        "text-[10px] font-medium",
        colors[state] ?? "text-fg-tertiary",
      )}
    >
      {state}
    </span>
  );
}
