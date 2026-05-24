import { useCallback, useEffect, useState } from "react";
import { cn } from "../../lib/cn";
import {
  createProject,
  discoverProject,
  openProject,
  pickProjectFolder,
} from "../../lib/tauri";
import type { Theme } from "../../lib/theme";
import type { DraftManifest, ProjectSummary } from "../../lib/types";
import { ThemeToggle } from "../ThemeToggle/ThemeToggle";
import { DiscoveryDraft } from "./DiscoveryDraft";

// Shape of one entry in the recent-projects list.
export interface RecentProject {
  path: string;
  name: string;
  lastOpenedAt: string; // ISO-8601
}

const RECENT_KEY = "i18n-harness:recent-projects";
const RECENT_MAX = 5;

function loadRecents(): RecentProject[] {
  try {
    const raw = localStorage.getItem(RECENT_KEY);
    if (!raw) return [];
    return JSON.parse(raw) as RecentProject[];
  } catch {
    return [];
  }
}

function saveRecents(list: RecentProject[]): void {
  localStorage.setItem(RECENT_KEY, JSON.stringify(list));
}

export function pushRecent(entry: RecentProject): void {
  const existing = loadRecents().filter((r) => r.path !== entry.path);
  const next = [entry, ...existing].slice(0, RECENT_MAX);
  saveRecents(next);
}

interface Props {
  onProjectOpened: (summary: ProjectSummary) => void;
  flashError: (msg: string) => void;
  flashInfo: (msg: string) => void;
  theme: Theme;
  onToggleTheme: () => void;
}

type HomeState =
  | { kind: "idle" }
  | { kind: "discovering" }
  | { kind: "draft"; root: string; draft: DraftManifest }
  | { kind: "opening" };

export function HomeScreen({
  onProjectOpened,
  flashError,
  flashInfo,
  theme,
  onToggleTheme,
}: Props) {
  const [recents, setRecents] = useState<RecentProject[]>(loadRecents);
  const [state, setState] = useState<HomeState>({ kind: "idle" });

  // Refresh recents when local state changes.
  useEffect(() => {
    setRecents(loadRecents());
  }, []);

  const handleOpenProject = useCallback(async () => {
    const folder = await pickProjectFolder();
    if (!folder) return;
    setState({ kind: "opening" });
    try {
      const resp = await openProject(folder);
      pushRecent({
        path: folder,
        name: resp.summary.name,
        lastOpenedAt: new Date().toISOString(),
      });
      setRecents(loadRecents());
      if (resp.warnings.length > 0) {
        flashInfo(
          `Project opened with ${resp.warnings.length} warning(s): ${resp.warnings[0]}`,
        );
      }
      onProjectOpened(resp.summary);
    } catch (e) {
      flashError(`Could not open project: ${formatError(e)}`);
      setState({ kind: "idle" });
    }
  }, [onProjectOpened, flashError, flashInfo]);

  const handleCreateProject = useCallback(async () => {
    const folder = await pickProjectFolder();
    if (!folder) return;
    setState({ kind: "discovering" });
    try {
      const draft = await discoverProject(folder);
      setState({ kind: "draft", root: folder, draft });
    } catch (e) {
      flashError(`Discovery failed: ${formatError(e)}`);
      setState({ kind: "idle" });
    }
  }, [flashError]);

  const handleConfirmDraft = useCallback(
    async (root: string, draft: DraftManifest) => {
      setState({ kind: "opening" });
      try {
        const resp = await createProject(root, draft);
        pushRecent({
          path: root,
          name: resp.summary.name,
          lastOpenedAt: new Date().toISOString(),
        });
        setRecents(loadRecents());
        if (resp.warnings.length > 0) {
          flashInfo(
            `Project created with ${resp.warnings.length} warning(s): ${resp.warnings[0]}`,
          );
        }
        onProjectOpened(resp.summary);
      } catch (e) {
        flashError(`Could not create project: ${formatError(e)}`);
        setState({ kind: "idle" });
      }
    },
    [onProjectOpened, flashError, flashInfo],
  );

  const handleCancelDraft = useCallback(() => {
    setState({ kind: "idle" });
  }, []);

  const handleOpenRecent = useCallback(
    async (recent: RecentProject) => {
      setState({ kind: "opening" });
      try {
        const resp = await openProject(recent.path);
        pushRecent({
          path: recent.path,
          name: resp.summary.name,
          lastOpenedAt: new Date().toISOString(),
        });
        setRecents(loadRecents());
        if (resp.warnings.length > 0) {
          flashInfo(
            `Project opened with ${resp.warnings.length} warning(s): ${resp.warnings[0]}`,
          );
        }
        onProjectOpened(resp.summary);
      } catch (e) {
        flashError(`Could not open ${recent.name}: ${formatError(e)}`);
        setState({ kind: "idle" });
      }
    },
    [onProjectOpened, flashError, flashInfo],
  );

  const handleRemoveRecent = useCallback((path: string) => {
    const next = loadRecents().filter((r) => r.path !== path);
    saveRecents(next);
    setRecents(next);
  }, []);

  const busy = state.kind === "discovering" || state.kind === "opening";

  return (
    <div className="h-screen w-screen flex flex-col bg-bg-base text-fg-primary overflow-hidden">
      {/* Corner controls */}
      <div className="absolute top-3 right-4 z-10">
        <ThemeToggle theme={theme} onToggle={onToggleTheme} />
      </div>

      {/* Centered content */}
      <div className="flex-1 flex items-center justify-center px-4 overflow-auto">
        <div className="w-full max-w-lg">
          {/* App wordmark */}
          <div className="mb-8 flex items-center gap-2.5">
            <span
              aria-hidden="true"
              className={cn(
                "w-5 h-5 rounded-sm border border-accent-subtle-border",
                "bg-gradient-to-br from-accent to-accent-active shrink-0",
              )}
            />
            <h1 className="text-xl font-semibold tracking-tight text-fg-primary">
              i18n-harness
            </h1>
          </div>

          {/* Action card */}
          <div className="rounded-lg border border-border-default bg-bg-elevated shadow-sm">
            <div className="p-6">
              <p className="text-sm text-fg-secondary mb-5 leading-relaxed">
                Open an existing project folder that contains an{" "}
                <code className="font-mono text-xs bg-bg-surface px-1 py-0.5 rounded border border-border-subtle">
                  i18n-harness.toml
                </code>{" "}
                manifest, or create one from any folder that has translation
                catalogs.
              </p>

              {/* Primary actions */}
              <div className="flex gap-3">
                <PrimaryButton
                  onClick={handleOpenProject}
                  disabled={busy}
                  aria-label="Open an existing project folder"
                >
                  Open project folder
                </PrimaryButton>
                <SecondaryButton
                  onClick={handleCreateProject}
                  disabled={busy}
                  aria-label="Create a project from a folder"
                >
                  Create from folder
                </SecondaryButton>
              </div>

              {/* Discovery draft — shown inline below buttons */}
              {state.kind === "draft" && (
                <div className="mt-5 pt-5 border-t border-border-subtle">
                  <DiscoveryDraft
                    root={state.root}
                    draft={state.draft}
                    onConfirm={handleConfirmDraft}
                    onCancel={handleCancelDraft}
                  />
                </div>
              )}

              {/* Busy indicator */}
              {busy && (
                <p aria-live="polite" className="mt-3 text-xs text-fg-tertiary">
                  {state.kind === "discovering"
                    ? "Scanning folder…"
                    : "Opening project…"}
                </p>
              )}
            </div>
          </div>

          {/* Recent projects */}
          <div className="mt-6">
            <h2 className="text-xs font-semibold tracking-widest uppercase text-fg-disabled mb-2 px-1">
              Recent projects
            </h2>
            {recents.length === 0 ? (
              <p className="text-sm text-fg-disabled px-1">
                No recent projects. Open or create one above.
              </p>
            ) : (
              <ul className="space-y-px">
                {recents.map((r) => (
                  <RecentRow
                    key={r.path}
                    recent={r}
                    onOpen={handleOpenRecent}
                    onRemove={handleRemoveRecent}
                    disabled={busy}
                  />
                ))}
              </ul>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Sub-components ────────────────────────────────────────────────────────────

function PrimaryButton({
  children,
  onClick,
  disabled,
  "aria-label": ariaLabel,
}: {
  children: React.ReactNode;
  onClick: () => void;
  disabled?: boolean;
  "aria-label"?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={ariaLabel}
      className={cn(
        "inline-flex items-center h-8 px-4 rounded-md border border-transparent",
        "text-sm font-medium text-accent-fg bg-accent",
        "enabled:hover:bg-accent-hover enabled:active:bg-accent-active",
        "disabled:opacity-50 disabled:cursor-not-allowed",
        "transition-colors duration-100 ease-out focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent",
      )}
    >
      {children}
    </button>
  );
}

function SecondaryButton({
  children,
  onClick,
  disabled,
  "aria-label": ariaLabel,
}: {
  children: React.ReactNode;
  onClick: () => void;
  disabled?: boolean;
  "aria-label"?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={ariaLabel}
      className={cn(
        "inline-flex items-center h-8 px-4 rounded-md border border-border-default bg-transparent",
        "text-sm font-medium text-fg-secondary",
        "enabled:hover:bg-bg-hover enabled:hover:text-fg-primary enabled:hover:border-border-strong",
        "enabled:active:bg-bg-selected",
        "disabled:opacity-50 disabled:cursor-not-allowed",
        "transition-colors duration-100 ease-out focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent",
      )}
    >
      {children}
    </button>
  );
}

function RecentRow({
  recent,
  onOpen,
  onRemove,
  disabled,
}: {
  recent: RecentProject;
  onOpen: (r: RecentProject) => void;
  onRemove: (path: string) => void;
  disabled: boolean;
}) {
  const segments = recent.path.replace(/\\/g, "/").split("/");
  const shortPath =
    segments.length > 2 ? `…/${segments.slice(-2).join("/")}` : recent.path;
  const ago = relativeTime(recent.lastOpenedAt);

  return (
    <li>
      <button
        type="button"
        onClick={() => onOpen(recent)}
        disabled={disabled}
        className={cn(
          "group w-full flex items-center justify-between gap-3 px-3 py-2 rounded-md",
          "text-left transition-colors duration-75",
          "enabled:hover:bg-bg-hover enabled:active:bg-bg-selected",
          "disabled:opacity-40 disabled:cursor-not-allowed",
          "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
        )}
        aria-label={`Open ${recent.name} from ${recent.path}`}
      >
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-fg-primary truncate">
            {recent.name}
          </p>
          <p
            className="font-mono text-xs text-fg-tertiary truncate mt-0.5"
            title={recent.path}
          >
            {shortPath}
          </p>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <span className="text-xs text-fg-disabled whitespace-nowrap">
            {ago}
          </span>
          <button
            type="button"
            tabIndex={disabled ? -1 : 0}
            onClick={(e) => {
              e.stopPropagation();
              onRemove(recent.path);
            }}
            aria-label={`Remove ${recent.name} from recent projects`}
            className={cn(
              "flex items-center justify-center w-5 h-5 rounded-sm border-0 bg-transparent",
              "text-fg-disabled opacity-0 group-hover:opacity-100",
              "hover:text-fg-secondary hover:bg-bg-surface",
              "transition-opacity duration-100 cursor-pointer",
              "focus:outline-none focus:ring-1 focus:ring-accent focus:opacity-100",
            )}
          >
            ×
          </button>
        </div>
      </button>
    </li>
  );
}

function relativeTime(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime();
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

function formatError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  return String(e);
}
