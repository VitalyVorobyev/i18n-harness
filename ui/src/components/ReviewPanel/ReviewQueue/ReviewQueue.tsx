import { useMemo, useState } from "react";
import { cn } from "../../../lib/cn";
import {
  candidateForms,
  parseConflictCandidates,
  rowKey,
  shortenReusePath,
} from "../../../lib/reuse";
import type {
  ReferenceConflictCandidate,
  ReviewQueueItem,
  ReviewQueueResponse,
  UnitId,
} from "../../../lib/types";

interface Props {
  reviewQueue: ReviewQueueResponse;
  /** Apply a candidate's text into the unit via the normal edit path. */
  onUseConflictCandidate?: (
    catalogPath: string,
    unitId: UnitId,
    forms: string[],
  ) => void;
  /** Called when the user clicks "Open" on a queue item. */
  onOpenItem: (catalogPath: string, unitId: UnitId) => void;
}

export function ReviewQueue({
  reviewQueue,
  onUseConflictCandidate,
  onOpenItem,
}: Props) {
  const { by_catalog, items } = reviewQueue;

  // Optional: filter by catalog path (clicking a catalog column value).
  const [catalogFilter, setCatalogFilter] = useState<string | null>(null);

  // Expanded conflict row (catalogPath+unitId). At most one open at a time.
  const [expandedKey, setExpandedKey] = useState<string | null>(null);

  const totalCount = items.length;
  const catalogCount = useMemo(() => {
    const paths = new Set(Object.keys(by_catalog));
    for (const it of items) paths.add(it.catalog_path);
    return paths.size;
  }, [by_catalog, items]);

  const visible = useMemo(() => {
    if (!catalogFilter) return items;
    return items.filter((it) => it.catalog_path === catalogFilter);
  }, [items, catalogFilter]);

  if (totalCount === 0) {
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
            title={`${totalCount} units across ${catalogCount} catalogs need review`}
          >
            <span className="sr-only">
              {totalCount} units across {catalogCount} catalogs need review
            </span>
            <span aria-hidden="true">
              {totalCount} {totalCount === 1 ? "unit" : "units"} &middot;{" "}
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
              <Th>
                <span className="sr-only">Expand</span>
              </Th>
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
            {visible.map((item) => {
              const key = rowKey(item.catalog_path, item.unit_id);
              const isConflict = item.review_status === "conflict";
              const candidates = isConflict
                ? parseConflictCandidates(item.reviewer_note)
                : [];
              return (
                <ReviewRow
                  key={key}
                  item={item}
                  isConflict={isConflict}
                  candidates={candidates}
                  expanded={expandedKey === key}
                  onToggleExpand={() =>
                    setExpandedKey((prev) => (prev === key ? null : key))
                  }
                  onUseCandidate={onUseConflictCandidate}
                  onOpen={onOpenItem}
                  onFilterCatalog={(p) =>
                    setCatalogFilter((prev) => (prev === p ? null : p))
                  }
                  isCatalogFiltered={catalogFilter === item.catalog_path}
                />
              );
            })}
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
  isConflict,
  candidates,
  expanded,
  onToggleExpand,
  onUseCandidate,
  onOpen,
  onFilterCatalog,
  isCatalogFiltered,
}: {
  item: ReviewQueueItem;
  isConflict: boolean;
  candidates: ReferenceConflictCandidate[];
  expanded: boolean;
  onToggleExpand: () => void;
  onUseCandidate?: (
    catalogPath: string,
    unitId: UnitId,
    forms: string[],
  ) => void;
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

  const detailId = `conflict-detail-${rowKey(item.catalog_path, item.unit_id)}`;

  return (
    <>
      <tr
        className={cn(
          "border-b border-border-subtle",
          "hover:bg-bg-hover transition-colors duration-75",
          isCatalogFiltered && "bg-accent-subtle",
          isConflict && "bg-severity-hard-bg/30 border-severity-hard-border/40",
        )}
      >
        {/* Expand toggle — only for conflict rows */}
        <td className="px-2 py-2 align-top w-6">
          {isConflict ? (
            <button
              type="button"
              onClick={onToggleExpand}
              aria-expanded={expanded}
              aria-controls={detailId}
              aria-label={
                expanded
                  ? `Hide conflict candidates for ${item.unit_id}`
                  : `Show conflict candidates for ${item.unit_id}`
              }
              className={cn(
                "inline-flex items-center justify-center w-5 h-5 rounded-sm",
                "text-severity-hard hover:bg-bg-hover",
                "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
              )}
            >
              <ChevronIcon open={expanded} />
            </button>
          ) : (
            <span aria-hidden="true" className="block w-5" />
          )}
        </td>

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
              item.target_preview
                ? "text-fg-primary"
                : "italic text-fg-disabled",
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

      {isConflict && expanded && (
        <tr id={detailId} className="border-b border-border-subtle">
          <td colSpan={10} className="px-3 py-3 bg-bg-base">
            <ConflictDetail
              item={item}
              candidates={candidates}
              onUseCandidate={onUseCandidate}
            />
          </td>
        </tr>
      )}
    </>
  );
}

// ── Conflict candidate detail ──────────────────────────────────────────────────
//
// The candidate list is parsed from the unit's persisted review note (the
// reuse pass writes the candidate JSON there), so it survives a project reopen.
// When the note is absent or unparseable we explain how to resolve the unit.

function ConflictDetail({
  item,
  candidates,
  onUseCandidate,
}: {
  item: ReviewQueueItem;
  candidates: ReferenceConflictCandidate[];
  onUseCandidate?: (
    catalogPath: string,
    unitId: UnitId,
    forms: string[],
  ) => void;
}) {
  if (candidates.length === 0) {
    return (
      <p className="text-xs text-fg-tertiary leading-relaxed max-w-[640px]">
        References disagreed on this unit, but the candidate detail could not be
        read from its review note. Re-run{" "}
        <span className="font-medium text-fg-secondary">Apply references</span>{" "}
        on this catalog to refresh the candidates, or open the unit and
        translate it directly.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-2 max-w-[760px]">
      <p className="text-xs text-fg-secondary">
        <span className="font-semibold text-fg-primary">
          {candidates.length}
        </span>{" "}
        references disagree on{" "}
        <span className="font-mono text-fg-primary">{item.unit_id}</span>. Pick
        one to apply it, then review and save.
      </p>
      <ul className="flex flex-col gap-2" aria-label="Conflicting candidates">
        {candidates.map((candidate, i) => {
          const forms = candidateForms(candidate);
          const sources = [candidate.reference, ...candidate.also_from];
          return (
            <li
              // Candidates are distinct translations in declaration order; the
              // index is a stable key within one conflict's list.
              key={`${candidate.reference}-${i}`}
              className="flex items-start gap-3 px-3 py-2 rounded-md border border-border-subtle bg-bg-surface"
            >
              <div className="flex-1 min-w-0 flex flex-col gap-1.5">
                {candidate.is_plural ? (
                  <div className="flex flex-col gap-1">
                    {forms.map((form, fi) => (
                      <div
                        key={fi}
                        className="flex items-baseline gap-2 min-w-0"
                      >
                        <span className="text-[10px] uppercase tracking-loose text-fg-tertiary shrink-0 w-12">
                          {PLURAL_LABELS[fi] ?? `Form ${fi}`}
                        </span>
                        <span className="font-mono text-xs text-fg-primary break-words min-w-0">
                          {form || (
                            <span className="italic text-fg-disabled">
                              empty
                            </span>
                          )}
                        </span>
                      </div>
                    ))}
                  </div>
                ) : (
                  <span className="font-mono text-xs text-fg-primary break-words">
                    {forms[0] || (
                      <span className="italic text-fg-disabled">empty</span>
                    )}
                  </span>
                )}
                <div className="flex flex-wrap items-center gap-1">
                  <span className="text-[10px] text-fg-tertiary">from</span>
                  {sources.map((src) => (
                    <span
                      key={src}
                      title={src}
                      className={cn(
                        "inline-flex items-center h-4 px-1.5 rounded-sm border",
                        "font-mono text-[10px] text-fg-secondary",
                        "border-border-subtle bg-bg-elevated",
                      )}
                    >
                      {shortenReusePath(src, 1)}
                    </span>
                  ))}
                </div>
              </div>
              {onUseCandidate && (
                <button
                  type="button"
                  onClick={() =>
                    onUseCandidate(item.catalog_path, item.unit_id, forms)
                  }
                  className={cn(
                    "shrink-0 inline-flex items-center h-6 px-2.5 rounded-md border text-xs font-medium",
                    "border-accent bg-accent/10 text-accent",
                    "hover:bg-accent/20 active:bg-accent/30",
                    "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
                    "transition-colors duration-75",
                  )}
                  aria-label={`Use this candidate for ${item.unit_id}`}
                >
                  Use this
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

const PLURAL_LABELS = ["Zero", "One", "Two", "Few", "Many", "Other"];

function ChevronIcon({ open }: { open: boolean }) {
  return (
    <svg
      width={12}
      height={12}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      style={{
        transform: open ? "rotate(90deg)" : "none",
        transition: "transform 120ms ease",
      }}
    >
      <path d="m9 18 6-6-6-6" />
    </svg>
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
  const tone =
    status === "conflict"
      ? "bg-severity-hard-bg border-severity-hard-border text-severity-hard"
      : status === "needs-review"
        ? "bg-severity-soft-bg border-severity-soft-border text-severity-soft"
        : "bg-bg-elevated border-border-subtle text-fg-tertiary";
  return (
    <span
      className={cn(
        "inline-flex items-center h-4 px-1.5 rounded-pill border text-[10px] font-medium",
        tone,
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
