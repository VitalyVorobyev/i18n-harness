import { cn, DropdownMenu, MenuItem, ProgressBar } from "@vitavision/ui";
import { Languages, MoreHorizontal } from "lucide-react";

import type { CatalogSummary, StatusCounts } from "../api";
import { countsLine, doneFraction, plural } from "../lib/format";

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

/** The catalog files of the open folder, with how far each one is. */
export function FileList(props: {
  catalogs: CatalogSummary[];
  selected: string | null;
  onSelect: (path: string) => void;
  onSetPivot: (catalog: CatalogSummary) => void;
}) {
  return (
    <nav aria-label="Files" className="flex h-full flex-col overflow-y-auto">
      <h2 className="px-3 pb-1 pt-3 text-xs font-medium uppercase tracking-wide text-fg-subtle">
        Files
      </h2>
      <ul className="flex flex-col gap-0.5 px-1.5 pb-3">
        {props.catalogs.length > 1 && (
          <AllFiles
            catalogs={props.catalogs}
            active={props.selected === ALL_FILES}
            onSelect={() => props.onSelect(ALL_FILES)}
          />
        )}
        {props.catalogs.map((c) => {
          const active = c.path === props.selected;
          return (
            <li key={c.path}>
              <div
                className={cn(
                  "group flex items-start gap-1 rounded-control px-2 py-2",
                  active ? "bg-raised" : "hover:bg-raised/60",
                )}
              >
                <button
                  type="button"
                  aria-current={active ? "true" : undefined}
                  className="min-w-0 flex-1 text-left"
                  onClick={() => props.onSelect(c.path)}
                >
                  <span className="flex items-center gap-1.5">
                    <span className="truncate font-medium">{c.name}</span>
                    {c.dirty && (
                      <span
                        className="size-1.5 shrink-0 rounded-full bg-signal"
                        title="Unsaved changes"
                        aria-label="Unsaved changes"
                      />
                    )}
                  </span>
                  <span className="block truncate text-xs text-fg-subtle">
                    {c.language}
                    {c.pivot_language ? ` · from ${c.pivot_language}` : ""} · {c.format_name}
                  </span>
                  <ProgressBar
                    className="mt-1.5"
                    fraction={doneFraction(c.counts)}
                    aria-label={`${c.name}: ${c.counts.done} of ${c.counts.total} done`}
                  />
                  <span className="mt-1 block text-xs text-fg-muted">{countsLine(c.counts)}</span>
                </button>
                <DropdownMenu
                  align="end"
                  trigger={
                    <button
                      type="button"
                      aria-label={`More for ${c.name}`}
                      className="rounded-control p-1 text-fg-subtle opacity-70 hover:bg-overlay hover:text-fg group-hover:opacity-100"
                    >
                      <MoreHorizontal aria-hidden size={16} />
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
              </div>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}

function AllFiles(props: { catalogs: CatalogSummary[]; active: boolean; onSelect: () => void }) {
  const all = allFilesSummary(props.catalogs);
  if (!all) return null;
  return (
    <li>
      <button
        type="button"
        aria-current={props.active ? "true" : undefined}
        className={cn(
          "w-full rounded-control px-2 py-2 text-left",
          props.active ? "bg-raised" : "hover:bg-raised/60",
        )}
        onClick={props.onSelect}
      >
        <span className="block font-medium">All files</span>
        <span className="block text-xs text-fg-subtle">
          {plural(props.catalogs.length, "file", "files")}
        </span>
        <ProgressBar
          className="mt-1.5"
          fraction={doneFraction(all.counts)}
          aria-label={`All files: ${all.counts.done} of ${all.counts.total} done`}
        />
        <span className="mt-1 block text-xs text-fg-muted">{countsLine(all.counts)}</span>
      </button>
    </li>
  );
}
