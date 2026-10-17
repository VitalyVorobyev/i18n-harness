import { cn, DropdownMenu, MenuItem, ProgressBar } from "@vitavision/ui";
import { Languages, MoreHorizontal } from "lucide-react";

import type { CatalogSummary } from "../api";
import { countsLine, doneFraction } from "../lib/format";

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
