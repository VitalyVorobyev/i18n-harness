import { cn } from "../../lib/cn";

export type View = "catalog" | "glossary" | "metrics";

interface Props {
  view: View;
  onViewChange: (v: View) => void;
  catalogPath: string | null;
  language: string | null;
  unitCount: number;
  dirtyCount: number;
  version: string;
  onOpen: () => void;
  onSave: () => void;
  onDiscard: () => void;
}

export function TopBar({
  view,
  onViewChange,
  catalogPath,
  language,
  unitCount,
  dirtyCount,
  version,
  onOpen,
  onSave,
  onDiscard,
}: Props) {
  const dirty = dirtyCount > 0;
  const catalogView = view === "catalog";
  // The Open/Save/Discard actions only make sense on the catalog view.
  // Hide them (but keep them in layout) when the user is on Glossary or
  // Metrics so the toolbar shape stays stable across tabs.
  return (
    <header
      className={cn(
        "app-chrome shrink-0 h-11 px-4 grid grid-cols-[1fr_auto_1fr] items-center",
        "bg-bg-surface border-b border-border-subtle",
      )}
    >
      <div className="flex items-center gap-2 min-w-0">
        <span
          aria-hidden="true"
          className={cn(
            "shrink-0 w-3.5 h-3.5 rounded-sm border border-accent-subtle-border",
            "bg-gradient-to-br from-accent to-accent-active",
          )}
        />
        <span className="text-md font-semibold tracking-tight text-fg-primary">
          i18n-harness
        </span>
        <span className="font-mono text-xs text-fg-tertiary tracking-loose">
          v{version}
        </span>
        <div
          role="tablist"
          aria-label="View"
          className="ml-3 flex items-center gap-1"
        >
          <TabButton
            active={catalogView}
            onClick={() => onViewChange("catalog")}
          >
            Catalog
          </TabButton>
          <TabButton
            active={view === "glossary"}
            onClick={() => onViewChange("glossary")}
          >
            Glossary
          </TabButton>
          <TabButton
            active={view === "metrics"}
            onClick={() => onViewChange("metrics")}
          >
            Metrics
          </TabButton>
        </div>
      </div>

      <div
        className={cn(
          "flex items-center gap-2",
          catalogView ? "" : "invisible pointer-events-none",
        )}
      >
        <ActionButton onClick={onOpen} title="Open a .ts catalog (⌘O)">
          Open
          <kbd>⌘O</kbd>
        </ActionButton>
        <ActionButton
          onClick={onSave}
          disabled={!dirty}
          tone={dirty ? "primary" : "default"}
          title={
            dirty ? `Save ${dirtyCount} change(s) — ⌘S` : "No unsaved changes"
          }
        >
          Save
          {dirty && (
            <span className="rounded-pill bg-bg-base/40 px-1 text-[10px] font-semibold tabular-nums">
              {dirtyCount}
            </span>
          )}
          <kbd>⌘S</kbd>
        </ActionButton>
        <ActionButton
          onClick={onDiscard}
          disabled={!dirty}
          title={dirty ? "Discard unsaved changes" : "No unsaved changes"}
        >
          Discard
        </ActionButton>
      </div>

      <div
        className={cn(
          "flex items-center gap-2 justify-end min-w-0",
          catalogView ? "" : "invisible",
        )}
      >
        {catalogPath ? (
          <>
            <span
              className="font-mono text-xs text-fg-secondary truncate max-w-[300px]"
              title={catalogPath}
            >
              {shortenPath(catalogPath)}
              {dirty && (
                <span
                  role="img"
                  className="ml-1 text-state-proposed"
                  aria-label="Unsaved changes"
                  title="Unsaved changes"
                >
                  •
                </span>
              )}
            </span>
            {language && (
              <span
                className={cn(
                  "shrink-0 inline-flex items-center h-5 px-2 rounded-pill border",
                  "border-accent-subtle-border bg-accent-subtle",
                  "font-mono text-xs text-accent-hover tracking-loose",
                )}
                title="Target language declared in the .ts root"
              >
                {language}
              </span>
            )}
            <span className="text-fg-disabled">·</span>
            <span className="text-xs text-fg-tertiary tracking-loose whitespace-nowrap">
              {unitCount} {unitCount === 1 ? "unit" : "units"}
            </span>
          </>
        ) : (
          <span className="text-xs text-fg-disabled tracking-loose uppercase">
            No catalog
          </span>
        )}
      </div>
    </header>
  );
}

function ActionButton({
  children,
  onClick,
  disabled,
  title,
  tone = "default",
}: {
  children: React.ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  title?: string;
  tone?: "default" | "primary";
}) {
  const primary = tone === "primary";
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={cn(
        "inline-flex items-center gap-2 h-7 px-3 rounded-md border",
        "text-sm font-medium transition-colors duration-100 ease-out",
        primary
          ? "text-accent-fg bg-accent border-transparent enabled:hover:bg-accent-hover enabled:active:bg-accent-active"
          : "text-fg-secondary border-border-default bg-transparent enabled:hover:bg-bg-hover enabled:hover:text-fg-primary enabled:hover:border-border-strong enabled:active:bg-bg-selected",
        "disabled:text-fg-disabled disabled:border-border-subtle disabled:cursor-not-allowed disabled:bg-transparent",
      )}
    >
      {children}
    </button>
  );
}

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={cn(
        "inline-flex items-center h-6 px-2 rounded-sm text-xs font-medium",
        "transition-colors duration-100 ease-out",
        active
          ? "text-fg-primary bg-accent-subtle border border-accent-subtle-border"
          : "text-fg-tertiary border border-transparent hover:text-fg-primary hover:bg-bg-hover",
      )}
    >
      {children}
    </button>
  );
}

function shortenPath(p: string): string {
  if (p.length <= 56) return p;
  const parts = p.split(/[\\/]/);
  if (parts.length <= 3) return p;
  return `…/${parts.slice(-3).join("/")}`;
}
