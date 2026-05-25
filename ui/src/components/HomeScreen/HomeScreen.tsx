import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useCallback, useEffect, useRef, useState } from "react";
import { cn } from "../../lib/cn";
import {
  appVersion,
  createProject,
  discoverProject,
  openProject,
  pickProjectFolder,
} from "../../lib/tauri";
import type { Theme } from "../../lib/theme";
import type { DraftManifest, ProjectSummary } from "../../lib/types";
import { ThemeToggle } from "../ThemeToggle/ThemeToggle";
import { DiscoveryDraft } from "./DiscoveryDraft";

export interface RecentProject {
  path: string;
  name: string;
  lastOpenedAt: string;
  locales?: string[];
  catalogs?: number;
  units?: number;
  finished?: number;
  proposed?: number;
  needsReview?: number;
}

const RECENT_KEY = "i18n-harness:recent-projects";
const RECENT_MAX = 8;

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
  const [version, setVersion] = useState<string>("…");
  const [isDragOver, setIsDragOver] = useState(false);

  useEffect(() => {
    setRecents(loadRecents());
    appVersion()
      .then(setVersion)
      .catch(() => setVersion("dev"));
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
        locales: resp.summary.locales,
        catalogs: resp.summary.catalogs.length,
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
          locales: resp.summary.locales,
          catalogs: resp.summary.catalogs.length,
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
          locales: resp.summary.locales,
          catalogs: resp.summary.catalogs.length,
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

  const handleClearRecents = useCallback(() => {
    saveRecents([]);
    setRecents([]);
  }, []);

  const busy = state.kind === "discovering" || state.kind === "opening";

  // Global keyboard shortcuts: ⌘O → Open, ⌘N → Create.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      const k = e.key.toLowerCase();
      if (k === "o") {
        e.preventDefault();
        if (!busy) void handleOpenProject();
      } else if (k === "n") {
        e.preventDefault();
        if (!busy) void handleCreateProject();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [busy, handleOpenProject, handleCreateProject]);

  // Drag-and-drop: dropped folder → open if manifest exists, else discover.
  const busyRef = useRef(busy);
  busyRef.current = busy;
  const onProjectOpenedRef = useRef(onProjectOpened);
  onProjectOpenedRef.current = onProjectOpened;
  const flashErrorRef = useRef(flashError);
  flashErrorRef.current = flashError;
  const flashInfoRef = useRef(flashInfo);
  flashInfoRef.current = flashInfo;

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          setIsDragOver(true);
        } else if (event.payload.type === "leave") {
          setIsDragOver(false);
        } else if (event.payload.type === "drop") {
          setIsDragOver(false);
          if (busyRef.current) return;
          const paths = event.payload.paths;
          if (!paths || paths.length === 0) return;
          const folder = paths[0];
          if (!folder) return;
          setState({ kind: "opening" });
          openProject(folder)
            .then((resp) => {
              pushRecent({
                path: folder,
                name: resp.summary.name,
                lastOpenedAt: new Date().toISOString(),
                locales: resp.summary.locales,
                catalogs: resp.summary.catalogs.length,
              });
              setRecents(loadRecents());
              if (resp.warnings.length > 0) {
                flashInfoRef.current(
                  `Project opened with ${resp.warnings.length} warning(s): ${resp.warnings[0]}`,
                );
              }
              onProjectOpenedRef.current(resp.summary);
            })
            .catch(() => {
              // Manifest not found — try discovery instead.
              setState({ kind: "discovering" });
              discoverProject(folder)
                .then((draft) => {
                  setState({ kind: "draft", root: folder, draft });
                })
                .catch((err) => {
                  flashErrorRef.current(
                    `Discovery failed: ${formatError(err)}`,
                  );
                  setState({ kind: "idle" });
                });
            });
        }
      })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {});

    return () => {
      unlisten?.();
    };
  }, []);

  if (state.kind === "draft") {
    return (
      <DiscoveryDraft
        root={state.root}
        draft={state.draft}
        onConfirm={handleConfirmDraft}
        onCancel={handleCancelDraft}
        theme={theme}
        onToggleTheme={onToggleTheme}
        version={version}
      />
    );
  }

  return (
    <div
      className={cn(
        "h-screen w-screen flex flex-col bg-bg-base text-fg-primary overflow-hidden",
        "relative transition-colors duration-100",
        isDragOver && "ring-2 ring-inset ring-accent",
      )}
    >
      <div
        className="flex-1 overflow-auto flex flex-col items-center"
        style={{
          paddingTop: 56,
          paddingBottom: 24,
          paddingLeft: 32,
          paddingRight: 32,
        }}
      >
        <div
          className="w-full flex flex-col"
          style={{ maxWidth: 640, gap: 36 }}
        >
          <HomeIdentity />
          <HomeActions
            onOpen={handleOpenProject}
            onCreate={handleCreateProject}
            busy={busy}
          />
          <HomeRecents
            recents={recents}
            onOpen={handleOpenRecent}
            onClear={handleClearRecents}
            busy={busy}
          />
        </div>

        <div className="flex-1" style={{ minHeight: 24 }} />

        <HomeFooter
          version={version}
          theme={theme}
          onToggleTheme={onToggleTheme}
        />
      </div>

      {busy && (
        <div
          aria-live="polite"
          className="fixed bottom-4 right-4 z-50 px-3 py-2 rounded-md border border-border-default bg-bg-elevated text-xs text-fg-secondary shadow-md"
        >
          {state.kind === "discovering"
            ? "Scanning folder…"
            : "Opening project…"}
        </div>
      )}
    </div>
  );
}

// ── Identity block ──────────────────────────────────────────────────────────

function HomeIdentity() {
  return (
    <div
      className="flex flex-col items-center"
      style={{ gap: 14, paddingTop: 8 }}
    >
      <div
        aria-hidden="true"
        style={{
          width: 56,
          height: 56,
          borderRadius: 12,
          background:
            "linear-gradient(135deg, hsl(215 35% 45%), hsl(215 25% 22%))",
          border: "1px solid var(--color-border-default)",
          boxShadow:
            "0 1px 0 hsl(215 35% 50% / .15) inset, 0 12px 24px hsl(222 47% 4% / .35)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <CatalogGridMark />
      </div>

      <div className="flex flex-col items-center" style={{ gap: 4 }}>
        <h1
          style={{
            margin: 0,
            fontSize: 22,
            fontWeight: 600,
            letterSpacing: "-0.015em",
            color: "var(--color-fg-primary)",
          }}
        >
          i18n-harness
        </h1>
        <p
          style={{
            margin: 0,
            fontSize: 12.5,
            color: "var(--color-fg-tertiary)",
            letterSpacing: 0,
          }}
        >
          A local-first translation harness for Qt TS, gettext PO, and ICU JSON
        </p>
      </div>
    </div>
  );
}

function CatalogGridMark() {
  return (
    <svg
      width="22"
      height="22"
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
    >
      <rect
        x="3"
        y="3"
        width="8"
        height="8"
        rx="1.5"
        stroke="hsl(210 40% 96%)"
        strokeWidth="1.7"
      />
      <rect
        x="13"
        y="3"
        width="8"
        height="8"
        rx="1.5"
        stroke="hsl(210 40% 96%)"
        strokeWidth="1.7"
        opacity={0.55}
      />
      <rect
        x="3"
        y="13"
        width="8"
        height="8"
        rx="1.5"
        stroke="hsl(210 40% 96%)"
        strokeWidth="1.7"
        opacity={0.55}
      />
      <rect
        x="13"
        y="13"
        width="8"
        height="8"
        rx="1.5"
        stroke="hsl(210 40% 96%)"
        strokeWidth="1.7"
        opacity={0.25}
      />
    </svg>
  );
}

// ── Actions card ────────────────────────────────────────────────────────────

function HomeActions({
  onOpen,
  onCreate,
  busy,
}: {
  onOpen: () => void;
  onCreate: () => void;
  busy: boolean;
}) {
  return (
    <div
      className="flex flex-col overflow-hidden"
      style={{
        border: "1px solid var(--color-border-subtle)",
        borderRadius: 10,
        background: "var(--color-bg-surface)",
      }}
    >
      <ActionRow
        icon={<FolderIcon />}
        title="Open project folder"
        sub={
          <>
            A folder with an{" "}
            <code
              className="font-mono"
              style={{
                fontSize: 11,
                background: "var(--color-bg-input)",
                border: "1px solid var(--color-border-subtle)",
                borderRadius: 3,
                padding: "0 3px",
              }}
            >
              i18n-harness.toml
            </code>{" "}
            manifest at its root.
          </>
        }
        kbd="⌘O"
        primary
        onClick={onOpen}
        disabled={busy}
      />
      <div style={{ height: 1, background: "var(--color-border-subtle)" }} />
      <ActionRow
        icon={<ScanIcon />}
        title="Create from folder"
        sub="Scan a folder for translation catalogs and propose a manifest."
        kbd="⌘N"
        onClick={onCreate}
        disabled={busy}
      />
    </div>
  );
}

function ActionRow({
  icon,
  title,
  sub,
  kbd,
  primary,
  onClick,
  disabled,
}: {
  icon: React.ReactNode;
  title: string;
  sub: React.ReactNode;
  kbd: string;
  primary?: boolean;
  onClick: () => void;
  disabled: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={title}
      className={cn(
        "flex items-center text-left w-full",
        "transition-colors duration-75",
        "disabled:opacity-50 disabled:cursor-not-allowed",
        "enabled:hover:bg-bg-hover enabled:active:bg-bg-selected",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent",
      )}
      style={{ padding: "16px 20px", gap: 16, cursor: "pointer" }}
    >
      <div
        aria-hidden="true"
        style={{
          width: 36,
          height: 36,
          borderRadius: 8,
          background: primary
            ? "var(--color-accent-subtle)"
            : "var(--color-bg-elevated)",
          border: `1px solid ${primary ? "var(--color-accent-subtle-border)" : "var(--color-border-subtle)"}`,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          color: primary ? "var(--color-accent)" : "var(--color-fg-secondary)",
          flexShrink: 0,
        }}
      >
        {icon}
      </div>
      <div className="flex flex-col flex-1 min-w-0" style={{ gap: 3 }}>
        <span
          style={{
            fontSize: 14,
            fontWeight: 500,
            color: "var(--color-fg-primary)",
          }}
        >
          {title}
        </span>
        <span
          style={{
            fontSize: 12,
            color: "var(--color-fg-tertiary)",
            lineHeight: 1.5,
          }}
        >
          {sub}
        </span>
      </div>
      <kbd style={{ flexShrink: 0 }}>{kbd}</kbd>
      <ChevronRightIcon />
    </button>
  );
}

// ── Recent projects ─────────────────────────────────────────────────────────

function HomeRecents({
  recents,
  onOpen,
  onClear,
  busy,
}: {
  recents: RecentProject[];
  onOpen: (r: RecentProject) => void;
  onClear: () => void;
  busy: boolean;
}) {
  return (
    <div className="flex flex-col" style={{ gap: 10 }}>
      <div className="flex items-baseline" style={{ padding: "0 4px", gap: 8 }}>
        <span
          style={{
            fontSize: 10,
            fontWeight: 600,
            letterSpacing: "0.08em",
            textTransform: "uppercase",
            color: "var(--color-fg-disabled)",
          }}
        >
          Recent projects
        </span>
        <span
          className="font-mono"
          style={{ fontSize: 10.5, color: "var(--color-fg-disabled)" }}
        >
          {recents.length}
        </span>
        <div className="flex-1" />
        {recents.length > 0 && (
          <button
            type="button"
            onClick={onClear}
            className={cn(
              "h-6 px-2 rounded-md border border-transparent bg-transparent",
              "text-xs text-fg-disabled",
              "hover:bg-bg-hover hover:text-fg-secondary hover:border-border-subtle",
              "active:bg-bg-selected",
              "transition-colors duration-75 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
            )}
          >
            Clear list
          </button>
        )}
      </div>

      {recents.length === 0 ? (
        <RecentEmptyState />
      ) : (
        <div
          className="flex flex-col overflow-hidden"
          style={{
            border: "1px solid var(--color-border-subtle)",
            borderRadius: 8,
            background: "var(--color-bg-surface)",
          }}
        >
          {recents.map((r, i) => (
            <RecentProjectRow
              key={r.path}
              recent={r}
              last={i === recents.length - 1}
              onOpen={onOpen}
              disabled={busy}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function RecentEmptyState() {
  return (
    <div
      className="flex flex-col items-center justify-center"
      style={{
        padding: "32px 20px",
        gap: 12,
        border: "1px solid var(--color-border-subtle)",
        borderRadius: 8,
        background: "var(--color-bg-surface)",
      }}
    >
      <EmptyFolderIllustration />
      <p
        style={{
          margin: 0,
          fontSize: 12,
          color: "var(--color-fg-tertiary)",
          textAlign: "center",
          lineHeight: 1.5,
        }}
      >
        No recent projects yet. Open a folder above to get started.
      </p>
    </div>
  );
}

function EmptyFolderIllustration() {
  return (
    <svg
      width="48"
      height="36"
      viewBox="0 0 48 36"
      fill="none"
      aria-hidden="true"
    >
      <rect
        x="1"
        y="9"
        width="46"
        height="26"
        rx="3"
        stroke="var(--color-fg-disabled)"
        strokeWidth="1.5"
      />
      <path
        d="M1 13h46"
        stroke="var(--color-fg-disabled)"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
      <path
        d="M1 12V7a2 2 0 012-2h10l3 4H46"
        stroke="var(--color-fg-disabled)"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <line
        x1="17"
        y1="22"
        x2="31"
        y2="22"
        stroke="var(--color-fg-disabled)"
        strokeWidth="1.5"
        strokeLinecap="round"
        opacity={0.5}
      />
      <line
        x1="17"
        y1="27"
        x2="27"
        y2="27"
        stroke="var(--color-fg-disabled)"
        strokeWidth="1.5"
        strokeLinecap="round"
        opacity={0.3}
      />
    </svg>
  );
}

function RecentProjectRow({
  recent,
  last,
  onOpen,
  disabled,
}: {
  recent: RecentProject;
  last: boolean;
  onOpen: (r: RecentProject) => void;
  disabled: boolean;
}) {
  const units = recent.units ?? 0;
  const finished = recent.finished ?? 0;
  const proposed = recent.proposed ?? 0;
  const catalogs = recent.catalogs ?? 0;
  const needsReview = recent.needsReview ?? 0;
  const hasStats = units > 0;
  const pct = hasStats ? Math.round((finished / units) * 100) : 0;
  const fmtUnits =
    units >= 1000
      ? `${(units / 1000).toFixed(units % 1000 === 0 ? 0 : 1)}k`
      : `${units}`;

  const ago = relativeTime(recent.lastOpenedAt);

  return (
    <button
      type="button"
      onClick={() => onOpen(recent)}
      disabled={disabled}
      aria-label={`Open ${recent.name} from ${recent.path}`}
      className={cn(
        "flex flex-col w-full text-left transition-colors duration-75",
        "enabled:hover:bg-bg-hover enabled:active:bg-bg-selected",
        "disabled:opacity-40 disabled:cursor-not-allowed",
        "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
      )}
      style={{
        padding: "12px 16px",
        borderBottom: last ? undefined : "1px solid var(--color-border-subtle)",
        gap: 8,
        cursor: "pointer",
      }}
    >
      {/* Top band: name / path / time */}
      <div className="flex items-baseline" style={{ gap: 8 }}>
        <span
          style={{
            fontSize: 14,
            fontWeight: 500,
            color: "var(--color-fg-primary)",
            flexShrink: 0,
          }}
        >
          {recent.name}
        </span>
        <span
          className="font-mono min-w-0 flex-1"
          style={{
            fontSize: 11,
            color: "var(--color-fg-tertiary)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            direction: "rtl",
            textAlign: "left",
          }}
          title={recent.path}
        >
          {recent.path}
        </span>
        <span
          className="font-mono"
          style={{
            fontSize: 11,
            color: "var(--color-fg-tertiary)",
            flexShrink: 0,
            whiteSpace: "nowrap",
          }}
        >
          {ago}
        </span>
      </div>

      {/* Middle band: progress + counts + needs-review */}
      {hasStats ? (
        <div className="flex items-center" style={{ gap: 8 }}>
          <ProgressBar
            finished={finished}
            proposed={proposed}
            total={units}
            width={160}
          />
          <span
            className="font-mono"
            style={{ fontSize: 11, color: "var(--color-fg-secondary)" }}
          >
            {pct}%
          </span>
          <span style={{ fontSize: 11, color: "var(--color-fg-disabled)" }}>
            ·
          </span>
          <span
            className="font-mono"
            style={{ fontSize: 11, color: "var(--color-fg-secondary)" }}
          >
            {catalogs}
          </span>
          <span style={{ fontSize: 11, color: "var(--color-fg-tertiary)" }}>
            catalogs
          </span>
          <span style={{ fontSize: 11, color: "var(--color-fg-disabled)" }}>
            ·
          </span>
          <span
            className="font-mono"
            style={{ fontSize: 11, color: "var(--color-fg-secondary)" }}
          >
            {fmtUnits}
          </span>
          <span style={{ fontSize: 11, color: "var(--color-fg-tertiary)" }}>
            units
          </span>
          <div className="flex-1" />
          {needsReview > 0 && (
            <span
              className="flex items-center"
              style={{
                gap: 4,
                fontSize: 11,
                color: "var(--color-severity-soft)",
                background: "var(--color-severity-soft-bg)",
                border: "1px solid var(--color-severity-soft-border)",
                borderRadius: 4,
                padding: "1px 6px",
              }}
            >
              <FlagIcon />
              <span className="font-mono">{needsReview}</span>
              <span>needs review</span>
            </span>
          )}
        </div>
      ) : (
        <div className="flex items-center" style={{ gap: 8 }}>
          {catalogs > 0 && (
            <>
              <span
                className="font-mono"
                style={{ fontSize: 11, color: "var(--color-fg-secondary)" }}
              >
                {catalogs}
              </span>
              <span style={{ fontSize: 11, color: "var(--color-fg-tertiary)" }}>
                {catalogs === 1 ? "catalog" : "catalogs"}
              </span>
            </>
          )}
        </div>
      )}

      {/* Bottom band: locale tags */}
      {recent.locales && recent.locales.length > 0 && (
        <div className="flex flex-wrap" style={{ gap: 4 }}>
          {recent.locales.map((l) => (
            <LocaleChip key={l}>{l}</LocaleChip>
          ))}
        </div>
      )}
    </button>
  );
}

function ProgressBar({
  finished,
  proposed,
  total,
  width,
}: {
  finished: number;
  proposed: number;
  total: number;
  width: number;
}) {
  const finPct = total > 0 ? (finished / total) * 100 : 0;
  const proPct = total > 0 ? (proposed / total) * 100 : 0;
  return (
    <div
      aria-hidden="true"
      style={{
        width,
        height: 3,
        flexShrink: 0,
        borderRadius: 999,
        overflow: "hidden",
        background: "var(--color-bg-elevated)",
        display: "flex",
      }}
    >
      <span
        style={{
          width: `${finPct}%`,
          background: "var(--color-state-finished)",
          display: "block",
        }}
      />
      <span
        style={{
          width: `${proPct}%`,
          background: "var(--color-state-proposed)",
          display: "block",
        }}
      />
    </div>
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

// ── Footer ──────────────────────────────────────────────────────────────────

function HomeFooter({
  version,
  theme,
  onToggleTheme,
}: {
  version: string;
  theme: Theme;
  onToggleTheme: () => void;
}) {
  return (
    <div
      className="flex items-center w-full"
      style={{
        maxWidth: 640,
        padding: "12px 0 4px",
        fontSize: 11,
        color: "var(--color-fg-tertiary)",
        gap: 8,
      }}
    >
      <span className="font-mono">v{version}</span>
      <span style={{ color: "var(--color-fg-disabled)" }}>·</span>
      <span className="flex items-center" style={{ gap: 5 }}>
        <span
          aria-hidden="true"
          style={{
            width: 6,
            height: 6,
            borderRadius: 999,
            background: "var(--color-state-finished)",
            display: "inline-block",
          }}
        />
        {/* TODO: wire to real backend health; placeholder shows default backend */}
        <span className="font-mono">ollama · gemma3:4b ready</span>
      </span>
      <div className="flex-1" />
      <span className="flex items-center" style={{ gap: 5 }}>
        <InfoIcon />
        <span>Offline · no network egress</span>
      </span>
      <span style={{ color: "var(--color-fg-disabled)" }}>·</span>
      <ThemeToggle theme={theme} onToggle={onToggleTheme} />
    </div>
  );
}

// ── Inline SVG icons (no icon library dep) ──────────────────────────────────

function FolderIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 16 16"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M1.5 4.5A1 1 0 012.5 3.5h4l1.5 2H13.5a1 1 0 011 1v5.5a1 1 0 01-1 1h-11a1 1 0 01-1-1V4.5z"
        stroke="currentColor"
        strokeWidth="1.25"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function ScanIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 16 16"
      fill="none"
      aria-hidden="true"
    >
      <rect
        x="1.5"
        y="1.5"
        width="5"
        height="5"
        rx="1"
        stroke="currentColor"
        strokeWidth="1.25"
      />
      <rect
        x="9.5"
        y="1.5"
        width="5"
        height="5"
        rx="1"
        stroke="currentColor"
        strokeWidth="1.25"
      />
      <rect
        x="1.5"
        y="9.5"
        width="5"
        height="5"
        rx="1"
        stroke="currentColor"
        strokeWidth="1.25"
      />
      <path
        d="M9.5 12h5M12 9.5v5"
        stroke="currentColor"
        strokeWidth="1.25"
        strokeLinecap="round"
      />
    </svg>
  );
}

function ChevronRightIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 14 14"
      fill="none"
      aria-hidden="true"
      style={{ color: "var(--color-fg-disabled)", flexShrink: 0 }}
    >
      <path
        d="M5.5 3.5L9 7l-3.5 3.5"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function FlagIcon() {
  return (
    <svg
      width="10"
      height="10"
      viewBox="0 0 12 12"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M2 1v10M2 1h8l-2.5 3.5L10 8H2"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function InfoIcon() {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 12 12"
      fill="none"
      aria-hidden="true"
    >
      <circle cx="6" cy="6" r="5" stroke="currentColor" strokeWidth="1.3" />
      <path
        d="M6 5.5v3"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
      <circle cx="6" cy="3.5" r="0.6" fill="currentColor" />
    </svg>
  );
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function relativeTime(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime();
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d ago`;
  const weeks = Math.floor(days / 7);
  return `${weeks}w ago`;
}

function formatError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  return String(e);
}
