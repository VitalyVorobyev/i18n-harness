// FileFilter — narrow the Translate surface to a single catalog file.
//
// A project can register several catalogs that share a locale (e.g. one file
// per module). Locale selection alone merges them; this filter picks one
// concrete file. It composes with the status filter and the locale focus:
//   - Matrix mode (focusLocale === null): options span every catalog.
//   - Single mode (focusLocale !== null): options are the catalogs for that
//     locale, since the other files are not on screen anyway.
// An empty value ("All files") is the no-filter default.

import { cn } from "../../lib/cn";
import type { CatalogRef, ProjectSummary } from "../../lib/types";

interface FileFilterProps {
  summary: ProjectSummary;
  /** Active focus locale, or null in Matrix mode. */
  focusLocale: string | null;
  /** Selected catalog absolute path, or null for "all files". */
  value: string | null;
  onChange: (next: string | null) => void;
}

export function fileFilterOptions(
  summary: ProjectSummary,
  focusLocale: string | null,
): CatalogRef[] {
  return focusLocale === null
    ? summary.catalogs
    : summary.catalogs.filter((c) => c.locale === focusLocale);
}

export function FileFilter({
  summary,
  focusLocale,
  value,
  onChange,
}: FileFilterProps) {
  const options = fileFilterOptions(summary, focusLocale);
  // With one or zero files in scope there is nothing to disambiguate.
  if (options.length <= 1) return null;

  return (
    <label className="inline-flex items-center gap-1.5 text-xs text-fg-tertiary">
      <span className="sr-only">Filter by file</span>
      <select
        aria-label="Filter by file"
        value={value ?? ""}
        onChange={(e) =>
          onChange(e.target.value === "" ? null : e.target.value)
        }
        className={cn(
          "h-6 max-w-[200px] px-2 rounded-sm border bg-bg-elevated",
          "text-xs text-fg-secondary truncate",
          "border-border-default",
          "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
        )}
      >
        <option value="">All files</option>
        {options.map((ref) => (
          <option key={ref.absolute_path} value={ref.absolute_path}>
            {ref.manifest_path}
          </option>
        ))}
      </select>
    </label>
  );
}
