import type { BreakdownSegment } from "@vitavision/ui";
import { Breakdown, breakdownText, cn, DropdownMenu, MenuItem } from "@vitavision/ui";
import { Languages, MoreHorizontal } from "lucide-react";

import type { CatalogSummary, StatusCounts } from "../api";
import { groupFiles, shortName } from "../lib/files";
import { plural } from "../lib/format";

/** The selection that shows the strings of every file together. */
export const ALL_FILES = "*";

/** A summary standing for every file at once, for the all-files view. */
export function allFilesSummary(catalogs: CatalogSummary[]): CatalogSummary | null {
  const first = catalogs[0];
  if (!first) return null;
  const same = <K extends keyof CatalogSummary>(key: K) =>
    catalogs.every((c) => c[key] === first[key]) ? first[key] : null;
  const counts: StatusCounts = {
    total: 0,
    to_do: 0,
    check: 0,
    done: 0,
    errors: 0,
    warnings: 0,
    inconsistent: 0,
    only_here: 0,
    glossary: 0,
    glossary_broken: 0,
  };
  for (const c of catalogs) {
    for (const key of Object.keys(counts) as (keyof StatusCounts)[]) {
      counts[key] += c.counts[key];
    }
  }
  return {
    ...first,
    path: ALL_FILES,
    name: "All files",
    relative_path: "",
    language: same("language") ?? "Translation",
    pivot: same("pivot"),
    pivot_language: same("pivot_language"),
    counts,
    dirty: catalogs.some((c) => c.dirty),
  };
}

const segments = (counts: StatusCounts): BreakdownSegment[] => [
  { value: counts.done, tone: "normal", label: "done" },
  { value: counts.check, tone: "warn", label: "to check" },
];

/** "412 done, 20 to check, 134 to do of 566", plus problems when there are any. */
function progressText(counts: StatusCounts): string {
  const text = breakdownText(segments(counts), { total: counts.total, remainderLabel: "to do" });
  return counts.errors > 0 ? `${text}; ${plural(counts.errors, "problem", "problems")}` : text;
}

/** "1,079 strings · 960 unique · 1,060 translated" under a file's name. */
function statsLine(counts: StatusCounts, distinct: number | null): string {
  const parts = [plural(counts.total, "string", "strings")];
  if (distinct !== null) parts.push(`${distinct.toLocaleString()} distinct`);
  else parts.push(`${counts.only_here.toLocaleString()} unique`);
  parts.push(`${(counts.total - counts.to_do).toLocaleString()} translated`);
  return parts.join(" · ");
}

/**
 * The catalog files of the open folder, grouped by what they share, each
 * with a bar of how far it is (done, to check, and the rest to do) and its
 * string counts.
 */
export function FileList(props: {
  catalogs: CatalogSummary[];
  selected: string | null;
  /** Different strings over every file, once known. */
  distinct: number | null;
  onSelect: (path: string) => void;
  onSetPivot: (catalog: CatalogSummary) => void;
}) {
  const all = props.catalogs.length > 1 ? allFilesSummary(props.catalogs) : null;
  return (
    <nav aria-label="Files" className="flex h-full flex-col overflow-y-auto pb-3">
      {all && (
        <ul className="px-1.5 pt-2">
          <FileRow
            label="All files"
            title={`All ${props.catalogs.length} files: ${progressText(all.counts)}`}
            stats={statsLine(all.counts, props.distinct)}
            statsTitle={`${plural(all.counts.total, "string", "strings")} in ${props.catalogs.length} files${props.distinct === null ? "" : `, ${props.distinct.toLocaleString()} of them different`}; ${(all.counts.total - all.counts.to_do).toLocaleString()} translated`}
            summary={all}
            active={props.selected === ALL_FILES}
            onSelect={() => props.onSelect(ALL_FILES)}
            strong
          />
        </ul>
      )}
      {groupFiles(props.catalogs).map((group) => (
        <section key={group.key} aria-label={group.title}>
          <h2 className="px-3 pb-1 pt-3 text-xs text-fg-subtle">{group.title}</h2>
          <ul className="flex flex-col px-1.5">
            {group.files.map((c) => (
              <FileRow
                key={c.path}
                label={shortName(c)}
                title={`${c.relative_path}: ${progressText(c.counts)}`}
                stats={statsLine(c.counts, null)}
                statsTitle={`${plural(c.counts.total, "string", "strings")}; ${c.counts.only_here.toLocaleString()} occur in no other file; ${(c.counts.total - c.counts.to_do).toLocaleString()} translated`}
                summary={c}
                active={c.path === props.selected}
                onSelect={() => props.onSelect(c.path)}
                menu={
                  <DropdownMenu
                    align="end"
                    trigger={
                      <button
                        type="button"
                        aria-label={`More for ${c.name}`}
                        className="rounded-control p-0.5 text-fg-subtle opacity-0 hover:bg-overlay hover:text-fg focus-visible:opacity-100 group-hover:opacity-100"
                      >
                        <MoreHorizontal aria-hidden size={14} />
                      </button>
                    }
                  >
                    <MenuItem
                      icon={<Languages aria-hidden size={14} />}
                      onSelect={() => props.onSetPivot(c)}
                    >
                      Language of the existing text…
                    </MenuItem>
                  </DropdownMenu>
                }
              />
            ))}
          </ul>
        </section>
      ))}
    </nav>
  );
}

function FileRow(props: {
  label: string;
  title: string;
  stats: string;
  statsTitle: string;
  summary: CatalogSummary;
  active: boolean;
  onSelect: () => void;
  strong?: boolean;
  menu?: React.ReactNode;
}) {
  const { counts } = props.summary;
  return (
    <li
      className={cn(
        "group flex items-center gap-1 rounded-control pr-1",
        props.active ? "bg-raised" : "hover:bg-raised/60",
      )}
    >
      <button
        type="button"
        aria-current={props.active ? "true" : undefined}
        aria-label={`${props.summary.name}: ${progressText(counts)}; ${props.statsTitle}${props.summary.dirty ? "; unsaved changes" : ""}`}
        title={props.summary.dirty ? `${props.title}; unsaved changes` : props.title}
        className="flex min-w-0 flex-1 flex-col gap-0.5 px-2 py-1.5 text-left"
        onClick={props.onSelect}
      >
        <span className="flex w-full min-w-0 items-center gap-2">
          <span className={cn("min-w-0 flex-1 truncate", props.strong && "font-medium")}>
            {props.label}
          </span>
          {props.summary.dirty && (
            <span className="size-1.5 shrink-0 rounded-full bg-signal" aria-hidden />
          )}
          {counts.errors > 0 && (
            <span className="shrink-0 font-mono text-xs tabular-nums text-defect" aria-hidden>
              {counts.errors}
            </span>
          )}
          <Breakdown
            aria-hidden
            className="w-16 shrink-0"
            segments={segments(counts)}
            total={counts.total}
          />
        </span>
        <span
          className="block w-full truncate text-xs tabular-nums text-fg-subtle"
          title={props.statsTitle}
        >
          {props.stats}
        </span>
      </button>
      {props.menu}
    </li>
  );
}
