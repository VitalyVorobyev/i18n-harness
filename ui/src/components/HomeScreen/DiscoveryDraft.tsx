import { useCallback, useState } from "react";
import { cn } from "../../lib/cn";
import type { Theme } from "../../lib/theme";
import type { DraftCatalog, DraftManifest, FormatGuess } from "../../lib/types";
import { ThemeToggle } from "../ThemeToggle/ThemeToggle";

interface Props {
  root: string;
  draft: DraftManifest;
  onConfirm: (root: string, draft: DraftManifest) => void;
  onCancel: () => void;
  theme: Theme;
  onToggleTheme: () => void;
  version: string;
}

export function DiscoveryDraft({
  root,
  draft,
  onConfirm,
  onCancel,
  theme,
  onToggleTheme,
  version,
}: Props) {
  const [name, setName] = useState(draft.name);

  const handleConfirm = useCallback(() => {
    onConfirm(root, { ...draft, name });
  }, [root, draft, name, onConfirm]);

  const rootNorm = root.replace(/\\/g, "/");
  const relPath = (absPath: string) => {
    const norm = absPath.replace(/\\/g, "/");
    return norm.startsWith(`${rootNorm}/`)
      ? norm.slice(rootNorm.length + 1)
      : norm;
  };

  const displayRoot =
    rootNorm.length > 60
      ? `…${rootNorm.slice(rootNorm.length - 57)}`
      : rootNorm;

  const allRows: Array<{ catalog: DraftCatalog; skipped: boolean }> =
    draft.catalogs.map((c) => ({
      catalog: c,
      skipped: c.confidence === "low" && c.format === "unknown",
    }));

  const visibleCount = allRows.filter((r) => !r.skipped).length;
  const tomlPreview = buildTomlPreview(name.trim() || draft.name, draft);
  const canCreate = visibleCount > 0 && name.trim().length > 0;

  return (
    <div className="h-screen w-screen flex flex-col bg-bg-base text-fg-primary overflow-hidden">
      <div
        className="flex-1 overflow-auto flex flex-col items-center"
        style={{ padding: "32px 32px 24px" }}
      >
        <div
          className="w-full flex flex-col"
          style={{ maxWidth: 720, gap: 20 }}
        >
          {/* Breadcrumb */}
          <div className="flex items-center" style={{ gap: 8 }}>
            <button
              type="button"
              onClick={onCancel}
              className={cn(
                "flex items-center h-7 px-3 rounded-md border border-transparent bg-transparent",
                "text-xs text-fg-secondary",
                "hover:bg-bg-hover hover:border-border-subtle hover:text-fg-primary",
                "active:bg-bg-selected",
                "transition-colors duration-75 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
              )}
              style={{ gap: 4 }}
            >
              <ChevronLeftIcon />
              Back
            </button>
            <div className="flex-1" />
            <button
              type="button"
              onClick={onCancel}
              className={cn(
                "flex items-center h-7 px-3 rounded-md border border-transparent bg-transparent",
                "text-xs text-fg-secondary",
                "hover:bg-bg-hover hover:border-border-subtle hover:text-fg-primary",
                "active:bg-bg-selected",
                "transition-colors duration-75 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
              )}
            >
              Cancel
            </button>
          </div>

          {/* Header */}
          <div className="flex flex-col" style={{ gap: 6 }}>
            <span
              style={{
                fontSize: 10,
                fontWeight: 600,
                letterSpacing: "0.08em",
                textTransform: "uppercase",
                color: "var(--color-fg-disabled)",
              }}
            >
              Discovery preview
            </span>
            <h1
              style={{
                margin: 0,
                fontSize: 22,
                fontWeight: 600,
                letterSpacing: "-0.015em",
                color: "var(--color-fg-primary)",
              }}
            >
              Found{" "}
              <span
                className="font-mono"
                style={{ color: "var(--color-fg-primary)" }}
              >
                {visibleCount} {visibleCount === 1 ? "catalog" : "catalogs"}
              </span>{" "}
              in this folder
            </h1>
            <span
              className="font-mono"
              style={{
                fontSize: 12,
                color: "var(--color-fg-tertiary)",
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
              title={root}
            >
              {displayRoot}
            </span>
          </div>

          {/* Project name input */}
          <div className="flex flex-col" style={{ gap: 6 }}>
            <label
              htmlFor="disc-project-name"
              style={{
                fontSize: 11,
                fontWeight: 500,
                color: "var(--color-fg-secondary)",
              }}
            >
              Project name
            </label>
            <input
              id="disc-project-name"
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className={cn(
                "h-8 px-3 rounded-md border border-border-default bg-bg-input",
                "text-sm font-mono text-fg-primary placeholder:text-fg-disabled",
                "focus:outline-none focus:ring-1 focus:ring-accent focus:border-accent",
              )}
              aria-label="Project name"
            />
          </div>

          {/* Found-catalogs card */}
          {allRows.length > 0 ? (
            <div
              className="flex flex-col overflow-hidden"
              style={{
                border: "1px solid var(--color-border-subtle)",
                borderRadius: 8,
                background: "var(--color-bg-surface)",
              }}
            >
              {allRows.map(({ catalog, skipped }, i) => (
                <DiscoveryRow
                  key={catalog.path}
                  catalog={catalog}
                  relPath={relPath(catalog.path)}
                  skipped={skipped}
                  last={i === allRows.length - 1}
                />
              ))}
            </div>
          ) : (
            <p
              style={{
                fontSize: 13,
                color: "var(--color-fg-tertiary)",
                margin: 0,
              }}
            >
              No translation catalogs were found in this folder.
            </p>
          )}

          {/* Proposed manifest */}
          <div className="flex flex-col" style={{ gap: 8 }}>
            <span
              style={{
                fontSize: 10,
                fontWeight: 600,
                letterSpacing: "0.08em",
                textTransform: "uppercase",
                color: "var(--color-fg-disabled)",
              }}
            >
              Proposed manifest
            </span>
            <pre
              style={{
                margin: 0,
                padding: "12px 14px",
                background: "var(--color-bg-input)",
                border: "1px solid var(--color-border-subtle)",
                borderRadius: 6,
                fontFamily: "var(--font-mono)",
                fontSize: 11.5,
                lineHeight: 1.6,
                color: "var(--color-fg-secondary)",
                whiteSpace: "pre",
                overflowX: "auto",
              }}
            >
              {tomlPreview}
            </pre>
          </div>

          {/* Footer actions */}
          <div className="flex items-center justify-end" style={{ gap: 8 }}>
            <button
              type="button"
              onClick={onCancel}
              className={cn(
                "inline-flex items-center h-8 px-4 rounded-md border border-border-default bg-transparent",
                "text-sm font-medium text-fg-secondary",
                "hover:bg-bg-hover hover:text-fg-primary hover:border-border-strong",
                "active:bg-bg-selected",
                "transition-colors duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent",
              )}
            >
              Edit before saving
            </button>
            <button
              type="button"
              onClick={handleConfirm}
              disabled={!canCreate}
              className={cn(
                "inline-flex items-center h-8 px-4 rounded-md border border-transparent",
                "text-sm font-medium text-accent-fg bg-accent",
                "enabled:hover:bg-accent-hover enabled:active:bg-accent-active",
                "disabled:opacity-50 disabled:cursor-not-allowed",
                "transition-colors duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent",
              )}
              style={{ gap: 6 }}
            >
              <CheckIcon />
              Create i18n-harness.toml
            </button>
          </div>
        </div>

        <div className="flex-1" style={{ minHeight: 24 }} />

        {/* Footer strip */}
        <div
          className="flex items-center w-full"
          style={{
            maxWidth: 720,
            paddingTop: 12,
            paddingBottom: 4,
            fontSize: 11,
            color: "var(--color-fg-tertiary)",
            gap: 8,
          }}
        >
          <span className="font-mono">v{version}</span>
          <div className="flex-1" />
          <ThemeToggle theme={theme} onToggle={onToggleTheme} />
        </div>
      </div>
    </div>
  );
}

function DiscoveryRow({
  catalog,
  relPath,
  skipped,
  last,
}: {
  catalog: DraftCatalog;
  relPath: string;
  skipped: boolean;
  last: boolean;
}) {
  const confColor =
    catalog.confidence === "high"
      ? "var(--color-state-finished)"
      : catalog.confidence === "medium"
        ? "var(--color-state-proposed)"
        : "var(--color-state-untranslated)";

  return (
    <div
      style={{
        padding: "12px 16px",
        borderBottom: last ? undefined : "1px solid var(--color-border-subtle)",
        opacity: skipped ? 0.55 : 1,
        display: "flex",
        alignItems: "flex-start",
        gap: 12,
      }}
    >
      {/* Confidence dot */}
      <span
        aria-hidden="true"
        style={{
          width: 7,
          height: 7,
          borderRadius: 999,
          background: confColor,
          flexShrink: 0,
          marginTop: 5,
        }}
      />

      <div className="flex flex-col flex-1 min-w-0" style={{ gap: 4 }}>
        {/* Path + chips + confidence */}
        <div className="flex items-center flex-wrap" style={{ gap: 6 }}>
          <span
            className="font-mono"
            style={{
              fontSize: 12.5,
              color: "var(--color-fg-primary)",
              wordBreak: "break-all",
            }}
          >
            {relPath}
          </span>
          <FormatChip format={catalog.format} />
          {skipped || catalog.locale == null ? (
            <SkippedPill />
          ) : (
            <LocaleChip>{catalog.locale}</LocaleChip>
          )}
          <div className="flex-1" />
          <span
            className="font-mono"
            style={{ fontSize: 10.5, color: confColor, flexShrink: 0 }}
          >
            {catalog.confidence}
          </span>
        </div>

        {/* Reason */}
        <span style={{ fontSize: 11.5, color: "var(--color-fg-secondary)" }}>
          {catalog.reason}
        </span>
      </div>
    </div>
  );
}

function FormatChip({ format }: { format: FormatGuess }) {
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        height: 18,
        padding: "0 6px",
        borderRadius: 3,
        border: "1px solid var(--color-border-default)",
        background: "var(--color-bg-elevated)",
        fontFamily: "var(--font-mono)",
        fontSize: 10,
        color: "var(--color-fg-secondary)",
        letterSpacing: "0.02em",
      }}
    >
      {formatLabel(format)}
    </span>
  );
}

function LocaleChip({ children }: { children: React.ReactNode }) {
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        height: 18,
        padding: "0 6px",
        borderRadius: 999,
        border: "1px solid var(--color-accent-subtle-border)",
        background: "var(--color-accent-subtle)",
        fontFamily: "var(--font-mono)",
        fontSize: 10,
        color: "var(--color-accent)",
        letterSpacing: "0.04em",
      }}
    >
      {children}
    </span>
  );
}

function SkippedPill() {
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        height: 16,
        padding: "0 6px",
        borderRadius: 999,
        border: "1px solid var(--color-border-default)",
        background: "var(--color-bg-elevated)",
        fontFamily: "var(--font-mono)",
        fontSize: 9.5,
        color: "var(--color-fg-disabled)",
        letterSpacing: "0.04em",
        textTransform: "uppercase",
      }}
    >
      skipped
    </span>
  );
}

function ChevronLeftIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 13 13"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M8 3L5 6.5 8 10"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 14 14"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M2.5 7.5l3 3 6-6"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function formatLabel(format: FormatGuess): string {
  switch (format) {
    case "qt-ts":
      return "qt-ts";
    case "gettext-po":
      return "po";
    case "icu-json":
      return "icu-json";
    case "unknown":
      return "unknown";
  }
}

function buildTomlPreview(projectName: string, draft: DraftManifest): string {
  const includedCatalogs = draft.catalogs.filter(
    (c) => !(c.confidence === "low" && c.format === "unknown"),
  );

  const lines: string[] = [`name = "${projectName}"`, `schema = 1`, ``];

  for (const c of includedCatalogs) {
    const rootNorm = draft.root.replace(/\\/g, "/");
    const norm = c.path.replace(/\\/g, "/");
    const rel = norm.startsWith(`${rootNorm}/`)
      ? norm.slice(rootNorm.length + 1)
      : norm;

    lines.push(`[[catalog]]`);
    lines.push(`path = "${rel}"`);
    lines.push(`format = "${c.format}"`);
    if (c.locale) {
      lines.push(`locale = "${c.locale}"`);
    }
    lines.push(``);
  }

  // Remove trailing blank line
  while (lines.length > 0 && lines[lines.length - 1] === "") {
    lines.pop();
  }

  return lines.join("\n");
}
