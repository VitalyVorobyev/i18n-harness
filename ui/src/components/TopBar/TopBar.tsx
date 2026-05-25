import { cn } from "../../lib/cn";
import type { Theme } from "../../lib/theme";
import { ThemeToggle } from "../ThemeToggle/ThemeToggle";

// Inline spinner used for the Save button's in-flight state.
function Spinner({ size = 14 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 14 14"
      fill="none"
      aria-hidden="true"
      className="animate-spin"
    >
      <circle
        cx="7"
        cy="7"
        r="5.5"
        stroke="currentColor"
        strokeOpacity="0.25"
        strokeWidth="2"
      />
      <path
        d="M7 1.5A5.5 5.5 0 0 1 12.5 7"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
    </svg>
  );
}

// Views available in project mode.
// "translate" is the default; "glossary" reuses the existing GlossaryPanel.
// "settings" and "quality" are placeholders for M4.3c/d.
export type View =
  | "catalog"
  | "glossary"
  | "metrics"
  | "translate"
  | "settings"
  | "quality";

// ── File-centric TopBar (pre-M4.3a; keeps the old single-file UI working) ─────

interface LegacyProps {
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
  theme: Theme;
  onToggleTheme: () => void;
}

/** @deprecated Use ProjectTopBar for M4.3+ project mode. */
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
  theme,
  onToggleTheme,
}: LegacyProps) {
  const dirty = dirtyCount > 0;
  const catalogView = view === "catalog";
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
        <ThemeToggle theme={theme} onToggle={onToggleTheme} className="ml-1" />
        <div
          role="tablist"
          aria-label="View"
          className="ml-2 flex items-center gap-1"
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

// ── Project-mode TopBar (M4.3a) ───────────────────────────────────────────────

export type ProjectView =
  | "overview"
  | "translate"
  | "glossary"
  | "settings"
  | "quality"
  | "review";

interface ProjectTopBarProps {
  projectName: string;
  locales: string[];
  activeLocaleFilter: Set<string>;
  onLocaleFilterChange: (next: Set<string>) => void;
  view: ProjectView;
  onViewChange: (v: ProjectView) => void;
  theme: Theme;
  onToggleTheme: () => void;
  onCloseProject: () => void;
  /** Total units needing review across the project; drives the badge on the Review tab. */
  reviewQueueCount?: number;
  /** Number of catalogs with unsaved edits (0..N). */
  unsavedCount?: number;
  /** True while saveAllDirty is in-flight. */
  saving?: boolean;
  /** Called when the Save button is clicked. */
  onSave?: () => void;
}

export function ProjectTopBar({
  projectName,
  view,
  onViewChange,
  theme,
  onToggleTheme,
  onCloseProject,
  reviewQueueCount = 0,
  unsavedCount = 0,
  saving = false,
  onSave,
}: ProjectTopBarProps) {
  return (
    <header
      className={cn(
        "app-chrome shrink-0 h-11 px-4 flex items-center gap-3",
        "bg-bg-surface border-b border-border-subtle",
      )}
    >
      {/* Left: project name */}
      <div className="flex items-center gap-2 min-w-0 shrink-0">
        <span
          aria-hidden="true"
          className={cn(
            "shrink-0 w-3.5 h-3.5 rounded-sm border border-accent-subtle-border",
            "bg-gradient-to-br from-accent to-accent-active",
          )}
        />
        <span
          className="text-sm font-semibold text-fg-primary truncate max-w-[140px]"
          title={projectName}
        >
          {projectName}
        </span>
      </div>

      {/*
       * Topbar locale chips removed: the redesigned per-panel rails
       * (Matrix/Focus BY LOCALE list, ProofreadView side-nav) provide
       * per-locale navigation at a more appropriate scope.
       */}

      {/* Center: view tabs */}
      <div
        role="tablist"
        aria-label="Project view"
        className="flex items-center gap-1 shrink-0"
      >
        <TabButton
          active={view === "overview"}
          onClick={() => onViewChange("overview")}
        >
          Overview
        </TabButton>
        <TabButton
          active={view === "translate"}
          onClick={() => onViewChange("translate")}
        >
          Translate
        </TabButton>
        <TabButton
          active={view === "glossary"}
          onClick={() => onViewChange("glossary")}
        >
          Glossary
        </TabButton>
        <TabButton
          active={view === "settings"}
          onClick={() => onViewChange("settings")}
          aria-label="Settings (coming in M4.3c)"
        >
          Settings
        </TabButton>
        <TabButton
          active={view === "quality"}
          onClick={() => onViewChange("quality")}
          aria-label="Quality (coming in M4.3d)"
        >
          Quality
        </TabButton>
        <TabButtonWithBadge
          active={view === "review"}
          onClick={() => onViewChange("review")}
          aria-label={
            reviewQueueCount > 0
              ? `Review — ${reviewQueueCount} units need review`
              : "Review"
          }
          badge={reviewQueueCount > 0 ? reviewQueueCount : undefined}
        >
          Review
        </TabButtonWithBadge>
      </div>

      {/* Right: save button + theme toggle + close project */}
      <div className="flex items-center gap-2 justify-end shrink-0 ml-auto">
        <ActionButton
          onClick={onSave}
          disabled={unsavedCount === 0 && !saving}
          tone={unsavedCount > 0 ? "primary" : "default"}
          title={
            unsavedCount > 0
              ? `Save ${unsavedCount} catalog(s) — ⌘S`
              : "No unsaved changes"
          }
          aria-label={
            unsavedCount > 0
              ? `Save ${unsavedCount} unsaved catalog(s)`
              : "Save — no unsaved changes"
          }
        >
          {saving && <Spinner size={12} />}
          Save
          {unsavedCount > 0 && (
            <span className="rounded-pill bg-bg-base/40 px-1 text-[10px] font-semibold tabular-nums">
              {unsavedCount}
            </span>
          )}
        </ActionButton>
        <ThemeToggle theme={theme} onToggle={onToggleTheme} />
        <ActionButton onClick={onCloseProject} title="Close project">
          Close project
        </ActionButton>
      </div>
    </header>
  );
}

// ── Shared primitives ─────────────────────────────────────────────────────────

function ActionButton({
  children,
  onClick,
  disabled,
  title,
  tone = "default",
  "aria-label": ariaLabel,
}: {
  children: React.ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  title?: string;
  tone?: "default" | "primary";
  "aria-label"?: string;
}) {
  const primary = tone === "primary";
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      aria-label={ariaLabel}
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
  "aria-label": ariaLabel,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
  "aria-label"?: string;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      aria-label={ariaLabel}
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

/** Tab button that shows an optional count badge — used for the Review tab. */
function TabButtonWithBadge({
  active,
  onClick,
  children,
  badge,
  "aria-label": ariaLabel,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
  badge?: number;
  "aria-label"?: string;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      aria-label={ariaLabel}
      onClick={onClick}
      className={cn(
        "inline-flex items-center gap-1.5 h-6 px-2 rounded-sm text-xs font-medium",
        "transition-colors duration-100 ease-out",
        active
          ? "text-fg-primary bg-accent-subtle border border-accent-subtle-border"
          : "text-fg-tertiary border border-transparent hover:text-fg-primary hover:bg-bg-hover",
      )}
    >
      {children}
      {badge !== undefined && (
        <span
          className={cn(
            "inline-flex items-center h-4 px-1 rounded-pill border tabular-nums leading-none text-[10px] font-semibold",
            active
              ? "bg-severity-soft-bg border-severity-soft-border text-severity-soft"
              : "bg-severity-soft-bg border-severity-soft-border text-severity-soft",
          )}
          aria-hidden="true"
        >
          {badge}
        </span>
      )}
    </button>
  );
}

function shortenPath(p: string): string {
  if (p.length <= 56) return p;
  const parts = p.split(/[\\/]/);
  if (parts.length <= 3) return p;
  return `…/${parts.slice(-3).join("/")}`;
}
