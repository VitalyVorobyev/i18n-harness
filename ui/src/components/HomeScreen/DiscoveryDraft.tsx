import { useCallback, useState } from "react";
import { cn } from "../../lib/cn";
import type {
  ClassificationConfidence,
  DraftCatalog,
  DraftManifest,
  FormatGuess,
} from "../../lib/types";

interface Props {
  root: string;
  draft: DraftManifest;
  onConfirm: (root: string, draft: DraftManifest) => void;
  onCancel: () => void;
}

export function DiscoveryDraft({ root, draft, onConfirm, onCancel }: Props) {
  const [name, setName] = useState(draft.name);

  const handleConfirm = useCallback(() => {
    onConfirm(root, { ...draft, name });
  }, [root, draft, name, onConfirm]);

  const localeIds = Object.keys(draft.locales).sort();
  const hasGlossary = draft.glossary != null;

  // Strip root prefix from catalog path for a cleaner display.
  const rootNorm = root.replace(/\\/g, "/");
  const relPath = (absPath: string) => {
    const norm = absPath.replace(/\\/g, "/");
    return norm.startsWith(`${rootNorm}/`)
      ? norm.slice(rootNorm.length + 1)
      : norm;
  };

  return (
    <div>
      <h3 className="text-sm font-semibold text-fg-primary mb-3">
        Review project draft
      </h3>

      {/* Project name — editable */}
      <div className="mb-4">
        <label
          htmlFor="draft-project-name"
          className="block text-xs font-medium text-fg-secondary mb-1"
        >
          Project name
        </label>
        <input
          id="draft-project-name"
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          className={cn(
            "w-full h-8 px-2.5 rounded-md border border-border-default bg-bg-base",
            "text-sm text-fg-primary placeholder:text-fg-disabled",
            "focus:outline-none focus:ring-1 focus:ring-accent focus:border-accent",
          )}
          aria-label="Project name"
        />
      </div>

      {/* Catalogs table */}
      {draft.catalogs.length > 0 ? (
        <div className="mb-4">
          <p className="text-xs font-medium text-fg-secondary mb-2">
            Catalogs found ({draft.catalogs.length})
          </p>
          <table
            className={cn(
              "w-full rounded-md border border-border-default overflow-hidden",
              "text-xs font-mono border-collapse",
            )}
            aria-label="Discovered catalogs"
          >
            <thead>
              <tr className="bg-bg-surface text-fg-tertiary border-b border-border-subtle">
                <th
                  scope="col"
                  className="px-3 py-1.5 text-left font-medium w-20"
                >
                  Format
                </th>
                <th
                  scope="col"
                  className="px-3 py-1.5 text-left font-medium w-20"
                >
                  Locale
                </th>
                <th scope="col" className="px-3 py-1.5 text-left font-medium">
                  Path
                </th>
                <th
                  scope="col"
                  className="px-3 py-1.5 text-left font-medium w-20"
                >
                  Confidence
                </th>
              </tr>
            </thead>
            <tbody>
              {draft.catalogs.map((dc) => (
                <CatalogRow
                  key={dc.path}
                  catalog={dc}
                  relPath={relPath(dc.path)}
                />
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="mb-4 text-sm text-fg-tertiary">
          No translation catalogs were found in this folder.
        </p>
      )}

      {/* Locale chips */}
      {localeIds.length > 0 && (
        <div className="mb-4">
          <p className="text-xs font-medium text-fg-secondary mb-2">Locales</p>
          <div className="flex flex-wrap gap-1.5">
            {localeIds.map((id) => (
              <span
                key={id}
                className={cn(
                  "inline-flex items-center h-5 px-2 rounded-pill border",
                  "border-accent-subtle-border bg-accent-subtle",
                  "font-mono text-xs text-accent-hover tracking-loose",
                )}
              >
                {id}
              </span>
            ))}
          </div>
        </div>
      )}

      {/* Glossary note */}
      {hasGlossary && (
        <p className="mb-4 text-xs text-fg-secondary">
          Glossary detected:{" "}
          <code className="font-mono bg-bg-surface px-1 py-0.5 rounded border border-border-subtle">
            glossary.toml
          </code>
        </p>
      )}

      {/* Action buttons */}
      <div className="flex gap-3">
        <button
          type="button"
          onClick={handleConfirm}
          disabled={draft.catalogs.length === 0 || !name.trim()}
          className={cn(
            "inline-flex items-center h-8 px-4 rounded-md border border-transparent",
            "text-sm font-medium text-accent-fg bg-accent",
            "enabled:hover:bg-accent-hover enabled:active:bg-accent-active",
            "disabled:opacity-50 disabled:cursor-not-allowed",
            "transition-colors duration-100 ease-out focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent",
          )}
        >
          Create project
        </button>
        <button
          type="button"
          onClick={onCancel}
          className={cn(
            "inline-flex items-center h-8 px-4 rounded-md border border-border-default bg-transparent",
            "text-sm font-medium text-fg-secondary",
            "hover:bg-bg-hover hover:text-fg-primary hover:border-border-strong",
            "active:bg-bg-selected",
            "transition-colors duration-100 ease-out focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent",
          )}
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

function CatalogRow({
  catalog,
  relPath,
}: {
  catalog: DraftCatalog;
  relPath: string;
}) {
  return (
    <>
      <tr className="border-b border-border-subtle last:border-b-0 bg-bg-base hover:bg-bg-hover transition-colors">
        <td className="px-3 py-2 text-fg-secondary">
          {formatLabel(catalog.format)}
        </td>
        <td className="px-3 py-2 text-fg-secondary">
          {catalog.locale ?? <span className="text-fg-disabled">—</span>}
        </td>
        <td className="px-3 py-2 text-fg-primary max-w-0">
          <span className="block truncate" title={relPath}>
            {relPath}
          </span>
        </td>
        <td className="px-3 py-2">
          <ConfidencePill confidence={catalog.confidence} />
        </td>
      </tr>
      {catalog.confidence === "low" && catalog.reason && (
        <tr className="bg-bg-base">
          <td
            colSpan={4}
            className="px-3 pb-2 text-xs text-fg-disabled leading-snug"
          >
            {catalog.reason}
          </td>
        </tr>
      )}
    </>
  );
}

function ConfidencePill({
  confidence,
}: {
  confidence: ClassificationConfidence;
}) {
  const cls =
    confidence === "high"
      ? "bg-state-finished/15 text-state-finished border-state-finished/30"
      : confidence === "medium"
        ? "bg-state-proposed/15 text-state-proposed border-state-proposed/30"
        : "bg-severity-soft-bg text-severity-soft border-severity-soft-border";

  return (
    <span
      className={cn(
        "inline-flex items-center h-5 px-1.5 rounded-pill border text-[10px] font-semibold uppercase tracking-wide",
        cls,
      )}
      title={`Confidence: ${confidence}`}
    >
      {confidence}
    </span>
  );
}

function formatLabel(format: FormatGuess): string {
  switch (format) {
    case "qt-ts":
      return "Qt TS";
    case "gettext-po":
      return "PO";
    case "icu-json":
      return "ICU JSON";
    case "unknown":
      return "Unknown";
  }
}
