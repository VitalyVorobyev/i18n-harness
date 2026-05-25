// FocusView — single-locale dense-list translation surface.
//
// Rendered when `focusLocale !== null`. Shares the same left-rail chrome as
// MatrixView (saved-view inbox + locale list + backend strip), but the centre
// is a dense row-per-unit list optimised for sustained single-locale work.
// The right inspector (260px) reuses the refactored Inspector.tsx.
//
// Layout: [left rail 232px] [header + list flex-1] [inspector 260px]

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { cn } from "../../lib/cn";
import type {
  BatchScope,
  CatalogResponse,
  GateReport,
  ProjectSummary,
  TargetEdit,
  Unit,
  UnitId,
} from "../../lib/types";
import { severityOf } from "../../lib/types";
import { Eyebrow, LocaleTag, ProgressBar } from "../primitives";
import { StateBadge } from "../StateBadge/StateBadge";
import { Inspector } from "./Inspector/Inspector";

// ── Prop types ─────────────────────────────────────────────────────────────

interface Props {
  summary: ProjectSummary;
  openCatalogs: Map<string, CatalogResponse>;
  focusLocale: string;
  setFocusLocale: (locale: string | null) => void;
  dirtyIds: Set<UnitId>;
  reports: Record<UnitId, GateReport>;
  busyIds: Set<UnitId>;
  batchActive: boolean;
  onEnsureCatalogLoaded: (absPath: string) => Promise<void>;
  onTranslateUnit: (catalogPath: string, unit: Unit) => void;
  onEditUnit: (catalogPath: string, unit: Unit, edit: TargetEdit) => void;
  onAcceptUnit: (catalogPath: string, unit: Unit) => void;
  onTranslateAll: (catalogPath: string, scope: BatchScope) => void;
}

// ── Per-locale stat helpers (shared with MatrixView) ──────────────────────

interface LocaleStat {
  locale: string;
  finished: number;
  proposed: number;
  untranslated: number;
  total: number;
}

function aggregateLocaleStats(
  summary: ProjectSummary,
  openCatalogs: Map<string, CatalogResponse>,
): LocaleStat[] {
  const stats = new Map<string, LocaleStat>();
  for (const ref of summary.catalogs) {
    const cur = stats.get(ref.locale) ?? {
      locale: ref.locale,
      finished: 0,
      proposed: 0,
      untranslated: 0,
      total: 0,
    };
    const cached = openCatalogs.get(ref.absolute_path);
    if (cached) {
      for (const u of cached.units) {
        cur.total += 1;
        if (u.state === "finished") cur.finished += 1;
        else if (u.state === "proposed") cur.proposed += 1;
        else if (u.state === "untranslated") cur.untranslated += 1;
      }
    }
    stats.set(ref.locale, cur);
  }
  return summary.locales.map(
    (locale) =>
      stats.get(locale) ?? {
        locale,
        finished: 0,
        proposed: 0,
        untranslated: 0,
        total: 0,
      },
  );
}

// ── Focus-locale unit list ─────────────────────────────────────────────────

interface FocusEntry {
  unit: Unit;
  catalogPath: string;
}

function buildFocusEntries(
  summary: ProjectSummary,
  openCatalogs: Map<string, CatalogResponse>,
  locale: string,
): FocusEntry[] {
  const entries: FocusEntry[] = [];
  for (const ref of summary.catalogs) {
    if (ref.locale !== locale) continue;
    const cached = openCatalogs.get(ref.absolute_path);
    if (!cached) continue;
    for (const unit of cached.units) {
      entries.push({ unit, catalogPath: ref.absolute_path });
    }
  }
  return entries;
}

// ── Component ──────────────────────────────────────────────────────────────

export function FocusView({
  summary,
  openCatalogs,
  focusLocale,
  setFocusLocale,
  dirtyIds,
  reports,
  busyIds,
  batchActive,
  onEnsureCatalogLoaded,
  onTranslateUnit,
  onEditUnit,
  onAcceptUnit,
  onTranslateAll,
}: Props) {
  // Auto-load catalogs for the focused locale.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      for (const ref of summary.catalogs) {
        if (ref.locale !== focusLocale) continue;
        if (cancelled) break;
        if (openCatalogs.has(ref.absolute_path)) continue;
        try {
          await onEnsureCatalogLoaded(ref.absolute_path);
        } catch {
          // Cell will render as "loading…" — non-fatal.
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [summary.catalogs, openCatalogs, focusLocale, onEnsureCatalogLoaded]);

  // ── Derived data ────────────────────────────────────────────────────────

  const localeStats = useMemo(
    () => aggregateLocaleStats(summary, openCatalogs),
    [summary, openCatalogs],
  );

  const focusStat = useMemo(
    () =>
      localeStats.find((s) => s.locale === focusLocale) ?? {
        locale: focusLocale,
        finished: 0,
        proposed: 0,
        untranslated: 0,
        total: 0,
      },
    [localeStats, focusLocale],
  );

  const allEntries = useMemo(
    () => buildFocusEntries(summary, openCatalogs, focusLocale),
    [summary, openCatalogs, focusLocale],
  );

  // ── Local view state ────────────────────────────────────────────────────

  const [search, setSearch] = useState("");
  const [selectedIdx, setSelectedIdx] = useState<number | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);

  const filteredEntries = useMemo(() => {
    const lower = search.trim().toLowerCase();
    if (!lower) return allEntries;
    return allEntries.filter(
      (e) =>
        e.unit.id.toLowerCase().includes(lower) ||
        e.unit.source.toLowerCase().includes(lower),
    );
  }, [allEntries, search]);

  // Clamp/reset selection when filter changes.
  useEffect(() => {
    setSelectedIdx((prev) => {
      if (prev === null) return null;
      if (filteredEntries.length === 0) return null;
      return Math.min(prev, filteredEntries.length - 1);
    });
  }, [filteredEntries.length]);

  // Default select first entry when entries arrive and nothing selected.
  useEffect(() => {
    if (selectedIdx === null && filteredEntries.length > 0) {
      setSelectedIdx(0);
    }
  }, [filteredEntries.length, selectedIdx]);

  const selectedEntry =
    selectedIdx !== null ? (filteredEntries[selectedIdx] ?? null) : null;

  // ── Catalog manifest path display ───────────────────────────────────────

  const manifestPath = useMemo(() => {
    const ref = summary.catalogs.find((c) => c.locale === focusLocale);
    return ref?.manifest_path ?? focusLocale;
  }, [summary.catalogs, focusLocale]);

  const openCount = focusStat.untranslated + focusStat.proposed;

  // ── Translate untranslated (batch for focused locale) ───────────────────

  const onTranslateUntranslated = useCallback(() => {
    if (batchActive) return;
    for (const ref of summary.catalogs) {
      if (ref.locale !== focusLocale) continue;
      const cached = openCatalogs.get(ref.absolute_path);
      if (!cached) continue;
      const hasWork = cached.units.some((u) => u.state === "untranslated");
      if (!hasWork) continue;
      onTranslateAll(ref.absolute_path, "untranslated");
    }
  }, [
    batchActive,
    summary.catalogs,
    openCatalogs,
    focusLocale,
    onTranslateAll,
  ]);

  // ── Accept callback (bridges to Inspector prop shape) ──────────────────

  const onInspectorAccept = useCallback(
    async (unitId: UnitId) => {
      if (!selectedEntry || selectedEntry.unit.id !== unitId) return;
      onAcceptUnit(selectedEntry.catalogPath, selectedEntry.unit);
    },
    [selectedEntry, onAcceptUnit],
  );

  // ── Keyboard map ────────────────────────────────────────────────────────
  //
  // J / K  — step down / up
  // ⌘T     — translate selected unit
  // ⌘↵     — accept (hard-flag guard)
  // ⌘→     — skip without saving

  useEffect(() => {
    function handler(e: KeyboardEvent) {
      const root = containerRef.current;
      if (!root) return;

      const target = e.target as HTMLElement | null;
      const insideEditor =
        target && (target.tagName === "TEXTAREA" || target.tagName === "INPUT");

      const isAcceptCombo = (e.metaKey || e.ctrlKey) && e.key === "Enter";
      const isTranslateCombo =
        (e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "t";
      const isSkipCombo = (e.metaKey || e.ctrlKey) && e.key === "ArrowRight";

      // Allow ⌘↵, ⌘T, ⌘→ from inside editors; plain J/K only when not editing.
      if (insideEditor && !isAcceptCombo && !isTranslateCombo && !isSkipCombo)
        return;

      if (e.key === "j" || e.key === "J") {
        if (filteredEntries.length === 0) return;
        e.preventDefault();
        setSelectedIdx((prev) =>
          Math.min(filteredEntries.length - 1, (prev ?? -1) + 1),
        );
        return;
      }

      if (e.key === "k" || e.key === "K") {
        if (filteredEntries.length === 0) return;
        e.preventDefault();
        setSelectedIdx((prev) => Math.max(0, (prev ?? 1) - 1));
        return;
      }

      if (isTranslateCombo) {
        if (!selectedEntry) return;
        e.preventDefault();
        onTranslateUnit(selectedEntry.catalogPath, selectedEntry.unit);
        return;
      }

      if (isAcceptCombo) {
        if (!selectedEntry) return;
        const flags = selectedEntry.unit.flags;
        const hard =
          Array.isArray(flags) &&
          flags.some((flag) => severityOf(flag) === "hard");
        if (hard) return; // guard: hard flag blocks accept
        e.preventDefault();
        onAcceptUnit(selectedEntry.catalogPath, selectedEntry.unit);
        return;
      }

      if (isSkipCombo) {
        if (filteredEntries.length === 0) return;
        e.preventDefault();
        // Advance to next row without committing any pending draft.
        setSelectedIdx((prev) =>
          prev !== null ? Math.min(filteredEntries.length - 1, prev + 1) : 0,
        );
        return;
      }
    }
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [filteredEntries, selectedEntry, onTranslateUnit, onAcceptUnit]);

  // ── Peer locales for the inspector ─────────────────────────────────────

  const peerLocales = useMemo(() => {
    if (!selectedEntry) return [];
    const unitId = selectedEntry.unit.id;
    return summary.locales
      .filter((l) => l !== focusLocale)
      .map((l) => {
        const ref = summary.catalogs.find((c) => c.locale === l);
        if (!ref) return null;
        const cached = openCatalogs.get(ref.absolute_path);
        const unit = cached?.units.find((u) => u.id === unitId) ?? null;
        if (!unit) return null;
        const text =
          unit.target.kind === "singular"
            ? unit.target.text
            : (unit.target.forms[0] ?? null);
        return { locale: l, text, state: unit.state };
      })
      .filter((p): p is NonNullable<typeof p> => p !== null);
  }, [
    selectedEntry,
    summary.locales,
    summary.catalogs,
    openCatalogs,
    focusLocale,
  ]);

  // ── Render ─────────────────────────────────────────────────────────────

  return (
    <div
      ref={containerRef}
      className="flex-1 flex overflow-hidden min-h-0 bg-bg-base"
    >
      {/* Left rail — same chrome as MatrixView */}
      <FocusLeftRail
        localeStats={localeStats}
        focusLocale={focusLocale}
        summary={summary}
        setFocusLocale={setFocusLocale}
      />

      {/* Centre: header + list */}
      <div className="flex-1 flex flex-col overflow-hidden min-h-0">
        {/* Header */}
        <header className="shrink-0 px-6 pt-4 pb-3.5 border-b border-border-subtle bg-bg-base">
          <div className="flex items-baseline gap-3 mb-2 flex-wrap">
            <div className="flex flex-col gap-1 flex-1 min-w-0">
              <Eyebrow>Translate</Eyebrow>
              <div className="flex items-baseline gap-2 flex-wrap">
                <h1 className="m-0 text-[18px] font-semibold text-fg-primary tracking-tight">
                  Focused on
                </h1>
                {/* Accent locale tag — larger than standard */}
                <span
                  style={{
                    display: "inline-block",
                    fontFamily: "var(--font-mono)",
                    fontSize: "13px",
                    letterSpacing: "0.04em",
                    lineHeight: 1,
                    padding: "3px 10px",
                    borderRadius: 4,
                    whiteSpace: "nowrap",
                    background: "var(--color-accent-subtle)",
                    border: "1px solid var(--color-accent-subtle-border)",
                    color: "var(--color-accent)",
                  }}
                >
                  {focusLocale}
                </span>
                {/* Clear focus chip */}
                <button
                  type="button"
                  onClick={() => setFocusLocale(null)}
                  aria-label="Clear focus locale, return to matrix view"
                  title="Clear focus"
                  className={cn(
                    "inline-flex items-center justify-center w-5 h-5 rounded-full border",
                    "text-xs text-fg-tertiary border-border-default bg-bg-elevated",
                    "hover:bg-bg-hover hover:text-fg-primary transition-colors duration-100",
                    "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
                  )}
                >
                  <XIcon size={10} />
                </button>
                <span className="font-mono text-xs text-fg-tertiary truncate">
                  {manifestPath}
                </span>
              </div>
              <div className="text-xs text-fg-tertiary mt-px">
                <span className="font-mono">{focusStat.total}</span> units
                {" · "}
                <span className="font-mono">{openCount}</span> open
              </div>
            </div>

            {/* Right controls: filter, Single/Matrix toggle, action */}
            <div className="flex items-center gap-2 shrink-0">
              <div className="relative">
                <input
                  type="search"
                  placeholder="Filter…"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  spellCheck={false}
                  aria-label="Filter units"
                  className={cn(
                    "h-8 pl-8 pr-3 rounded-md border bg-bg-input",
                    "border-border-subtle text-sm text-fg-primary",
                    "placeholder:text-fg-tertiary",
                    "focus:border-accent focus:outline-none focus-visible:outline-none",
                  )}
                  style={{ width: 180 }}
                />
                <span
                  className="absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-tertiary pointer-events-none"
                  aria-hidden="true"
                >
                  <SearchIcon size={13} />
                </span>
              </div>

              {/* Single / Matrix segmented toggle */}
              <div
                className="flex items-center rounded-md border border-border-default bg-bg-elevated"
                style={{ padding: 2, gap: 2 }}
              >
                <span
                  className={cn(
                    "inline-flex items-center gap-1.5 h-[22px] px-2.5 rounded-sm",
                    "text-xs font-medium bg-bg-selected text-fg-primary",
                  )}
                  aria-current="true"
                >
                  <ListIcon size={11} />
                  Single
                </span>
                <button
                  type="button"
                  onClick={() => setFocusLocale(null)}
                  aria-label="Switch to matrix view"
                  className={cn(
                    "inline-flex items-center gap-1.5 h-[22px] px-2.5 rounded-sm",
                    "text-xs font-medium text-fg-secondary",
                    "hover:bg-bg-hover hover:text-fg-primary transition-colors duration-100",
                    "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
                  )}
                >
                  <GridIcon size={11} />
                  Matrix
                </button>
              </div>

              <button
                type="button"
                onClick={onTranslateUntranslated}
                disabled={batchActive || focusStat.untranslated === 0}
                className={cn(
                  "inline-flex items-center gap-1.5 h-8 px-3 rounded-md text-xs font-medium",
                  "transition-colors duration-100",
                  "text-accent-fg bg-accent",
                  "enabled:hover:bg-accent-hover enabled:active:bg-accent-active",
                  "disabled:bg-accent-subtle disabled:text-fg-disabled disabled:cursor-not-allowed",
                )}
                title={
                  batchActive
                    ? "A batch is already running"
                    : focusStat.untranslated === 0
                      ? "No untranslated units"
                      : `Translate ${focusStat.untranslated} untranslated unit(s)`
                }
              >
                <SparklesIcon size={12} />
                Translate untranslated
              </button>
            </div>
          </div>

          {/* Progress strip */}
          <div className="flex items-center gap-2 mt-1">
            <span
              className="font-mono text-[11px] text-fg-tertiary shrink-0"
              style={{ width: 56 }}
            >
              {focusLocale}
            </span>
            <ProgressBar
              total={focusStat.total}
              finished={focusStat.finished}
              proposed={focusStat.proposed}
              height={4}
            />
            <span className="font-mono text-[11px] text-fg-tertiary shrink-0 tabular-nums">
              {focusStat.total > 0
                ? `${Math.round((focusStat.finished / focusStat.total) * 100)}%`
                : "—"}
            </span>
          </div>
        </header>

        {/* Column header strip */}
        <div
          className="shrink-0 flex items-center px-[22px] py-2 bg-bg-surface border-b border-border-subtle"
          style={{
            fontSize: 10,
            letterSpacing: "0.08em",
            textTransform: "uppercase",
            color: "var(--color-fg-tertiary)",
            fontWeight: 600,
          }}
        >
          <span style={{ width: 14 }} />
          <span className="font-mono ml-3" style={{ flex: "0 0 240px" }}>
            Source / id
          </span>
          <span className="flex-1 font-mono">Target — {focusLocale}</span>
          <span
            className="font-mono"
            style={{ flex: "0 0 110px", textAlign: "right" }}
          >
            State
          </span>
        </div>

        {/* Unit rows or empty state */}
        <div className="flex-1 overflow-y-auto">
          {filteredEntries.length === 0 ? (
            <div className="flex flex-col items-center justify-center gap-2 py-16 text-sm text-fg-tertiary">
              <p>Nothing to translate here.</p>
              {search && (
                <button
                  type="button"
                  onClick={() => setSearch("")}
                  className="text-xs text-accent underline-offset-2 hover:underline"
                >
                  Clear filter
                </button>
              )}
            </div>
          ) : (
            filteredEntries.map((entry, idx) => (
              <FocusRow
                key={`${entry.catalogPath}::${entry.unit.id}`}
                entry={entry}
                selected={idx === selectedIdx}
                busy={busyIds.has(entry.unit.id)}
                dirty={dirtyIds.has(entry.unit.id)}
                onSelect={() => setSelectedIdx(idx)}
                onTranslate={() =>
                  onTranslateUnit(entry.catalogPath, entry.unit)
                }
                onEdit={(edit) =>
                  onEditUnit(entry.catalogPath, entry.unit, edit)
                }
                onAccept={() => onAcceptUnit(entry.catalogPath, entry.unit)}
                onSkip={() =>
                  setSelectedIdx(Math.min(filteredEntries.length - 1, idx + 1))
                }
              />
            ))
          )}
        </div>

        {/* Keyboard legend footer */}
        <footer className="shrink-0 flex items-center gap-4 px-5 py-1.5 border-t border-border-subtle bg-bg-surface">
          <KbdLegendItem kbd="J" label="next" />
          <KbdLegendItem kbd="K" label="prev" />
          <KbdLegendItem kbd="⌘T" label="translate" />
          <KbdLegendItem kbd="⌘↵" label="accept" />
          <KbdLegendItem kbd="⌘→" label="skip" />
          <span className="flex-1" />
          <span className="font-mono text-[11px] text-fg-tertiary tabular-nums">
            {filteredEntries.length} unit
            {filteredEntries.length !== 1 ? "s" : ""}
          </span>
        </footer>
      </div>

      {/* Right inspector */}
      {selectedEntry && (
        <Inspector
          unit={selectedEntry.unit}
          report={reports[selectedEntry.unit.id] ?? null}
          activeCatalogPath={selectedEntry.catalogPath}
          busyIds={busyIds}
          onAccept={onInspectorAccept}
          peerLocales={peerLocales}
          onJumpToLocale={(locale) => setFocusLocale(locale)}
          width={260}
        />
      )}
    </div>
  );
}

// ── Left rail ──────────────────────────────────────────────────────────────

function FocusLeftRail({
  localeStats,
  focusLocale,
  summary,
  setFocusLocale,
}: {
  localeStats: LocaleStat[];
  focusLocale: string;
  summary: ProjectSummary;
  setFocusLocale: (locale: string | null) => void;
}) {
  return (
    <aside
      className={cn(
        "shrink-0 flex flex-col overflow-hidden",
        "bg-bg-surface border-r border-border-subtle",
      )}
      style={{ width: 232 }}
    >
      {/* By locale section — focus-mode badge next to eyebrow */}
      <div className="flex flex-col gap-1 px-3 py-4 border-b border-border-subtle">
        <div className="flex items-center gap-2 px-1 pb-1.5">
          <Eyebrow>By locale</Eyebrow>
          <span
            className={cn(
              "inline-flex items-center h-4 px-1.5 rounded-sm border",
              "font-mono text-[9px] uppercase tracking-loose",
              "bg-accent-subtle border-accent-subtle-border text-accent",
            )}
            title="Focus mode active"
          >
            focus mode
          </span>
        </div>
        {localeStats.map((stat) => (
          <LocaleRowButton
            key={stat.locale}
            stat={stat}
            active={stat.locale === focusLocale}
            onSelect={() => setFocusLocale(stat.locale)}
          />
        ))}
      </div>

      {/* Backend strip */}
      <div className="mt-auto px-4 py-3 border-t border-border-subtle">
        <div className="pb-1.5">
          <Eyebrow>Backend</Eyebrow>
        </div>
        <div className="flex items-center gap-2">
          <span
            className="inline-block w-[7px] h-[7px] rounded-pill bg-state-finished"
            aria-hidden="true"
          />
          <span className="font-mono text-[11px] text-fg-secondary truncate">
            {summary.backend
              ? `${summary.backend.kind}${
                  summary.backend.model ? ` · ${summary.backend.model}` : ""
                }`
              : "no backend configured"}
          </span>
        </div>
      </div>
    </aside>
  );
}

function LocaleRowButton({
  stat,
  active,
  onSelect,
}: {
  stat: LocaleStat;
  active: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={active}
      title={`Focus on ${stat.locale}`}
      className={cn(
        "w-full flex flex-col gap-1.5 px-2.5 py-1.5 rounded-md",
        "transition-colors duration-100 ease-out",
        "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
        active
          ? "bg-accent-subtle ring-1 ring-accent ring-inset"
          : "hover:bg-bg-hover",
      )}
    >
      <div className="flex items-center gap-2 w-full">
        <LocaleTag locale={stat.locale} tone={active ? "accent" : "default"} />
        <span className="flex-1" />
        <span className="font-mono text-[10.5px] text-fg-tertiary tabular-nums">
          {stat.total > 0
            ? `${Math.round((stat.finished / stat.total) * 100)}%`
            : "—"}
        </span>
      </div>
      <ProgressBar
        total={stat.total}
        finished={stat.finished}
        proposed={stat.proposed}
        height={3}
      />
    </button>
  );
}

// ── Unit row ────────────────────────────────────────────────────────────────

interface FocusRowProps {
  entry: FocusEntry;
  selected: boolean;
  busy: boolean;
  dirty: boolean;
  onSelect: () => void;
  onTranslate: () => void;
  onEdit: (edit: TargetEdit) => void;
  onAccept: () => void;
  onSkip: () => void;
}

function FocusRow({
  entry,
  selected,
  busy,
  dirty,
  onSelect,
  onTranslate,
  onEdit,
  onAccept,
  onSkip,
}: FocusRowProps) {
  const { unit } = entry;
  const state = unit.state;
  const placeholders = Array.isArray(unit.placeholders)
    ? unit.placeholders.length
    : 0;
  const isPlural = unit.plural_arity != null;
  const anyFlag = Array.isArray(unit.flags) && unit.flags.length > 0;
  const hardFlag =
    Array.isArray(unit.flags) &&
    unit.flags.some((flag) => severityOf(flag) === "hard");

  // For glossary hit detection we have no glossary data at this layer,
  // so the chip is omitted (the inspector panel shows full glossary hits).

  return (
    // role="option" is a valid interactive implicit role that accepts tabIndex
    // and aria-selected. The parent list uses custom keyboard navigation (J/K).
    <div
      role="option"
      aria-selected={selected}
      onClick={onSelect}
      onFocus={onSelect}
      tabIndex={0}
      className={cn(
        "flex items-start px-[22px] py-[10px]",
        "border-b border-border-subtle transition-colors duration-100",
        "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-accent",
        selected
          ? "bg-bg-selected border-l-2 border-l-accent"
          : "border-l-2 border-l-transparent hover:bg-bg-hover",
      )}
    >
      {/* State dot */}
      <span className="mt-[6px] shrink-0">
        <StateBadge state={state} variant="dot" />
      </span>

      {/* Source + id column (~240px) */}
      <div
        className="flex flex-col gap-[3px] shrink-0 ml-3 pr-4"
        style={{ flex: "0 0 240px" }}
      >
        <span className="font-mono text-[13px] text-fg-primary leading-snug break-words">
          {unit.source}
        </span>
        <span
          className="font-mono text-[10.5px] text-fg-tertiary truncate"
          title={unit.id}
        >
          {unit.id}
        </span>
        {/* Micro chips */}
        {(isPlural || placeholders > 0) && (
          <div className="flex items-center gap-1 mt-0.5">
            {isPlural && (
              <MicroChip
                label={`×${unit.plural_arity ?? 2}`}
                title="Plural forms"
              />
            )}
            {placeholders > 0 && (
              <MicroChip label={`{}×${placeholders}`} title="Placeholders" />
            )}
            {anyFlag && (
              <MicroChip
                label="flagged"
                tone="soft"
                title={unit.flags.join(", ")}
              />
            )}
            {dirty && (
              <MicroChip label="edited" tone="accent" title="Unsaved edits" />
            )}
          </div>
        )}
      </div>

      {/* Target column */}
      <div className="flex flex-col gap-1.5 flex-1 min-w-0">
        {selected ? (
          <SelectedTarget
            unit={unit}
            busy={busy}
            hardFlag={hardFlag}
            onEdit={onEdit}
            onTranslate={onTranslate}
            onAccept={onAccept}
            onSkip={onSkip}
          />
        ) : (
          <ReadOnlyTarget unit={unit} />
        )}

        {/* Inline flag strip */}
        {anyFlag && (
          <div
            className="flex items-center gap-2 px-2 py-1 rounded text-[11px]"
            style={{
              background: "var(--color-severity-soft-bg)",
              border: "1px solid var(--color-severity-soft-border)",
              color: "var(--color-severity-soft)",
            }}
          >
            <FlagIcon size={11} />
            <span className="font-mono truncate">{unit.flags[0]}</span>
            <span className="flex-1" />
            <span className="text-fg-tertiary">review intent</span>
          </div>
        )}
      </div>

      {/* State pill — anchored right */}
      <div
        className="shrink-0 flex justify-end pt-[2px] pl-3"
        style={{ flex: "0 0 110px" }}
      >
        <StateBadge state={state} variant="pill" />
      </div>
    </div>
  );
}

// ── Read-only target ───────────────────────────────────────────────────────

function ReadOnlyTarget({ unit }: { unit: Unit }) {
  const { state, target } = unit;
  if (state === "untranslated") {
    return (
      <span className="text-[12.5px] italic text-fg-tertiary leading-snug py-[2px]">
        untranslated
      </span>
    );
  }
  const text =
    target.kind === "singular" ? target.text : (target.forms[0] ?? null);
  if (!text) {
    return (
      <span className="text-[12.5px] italic text-fg-tertiary leading-snug py-[2px]">
        —
      </span>
    );
  }
  return (
    <span className="font-mono text-[13px] text-fg-primary leading-snug py-[2px] break-words">
      {text}
    </span>
  );
}

// ── Selected target: editable area + action bar ────────────────────────────

function SelectedTarget({
  unit,
  busy,
  hardFlag,
  onEdit,
  onTranslate,
  onAccept,
  onSkip,
}: {
  unit: Unit;
  busy: boolean;
  hardFlag: boolean;
  onEdit: (edit: TargetEdit) => void;
  onTranslate: () => void;
  onAccept: () => void;
  onSkip: () => void;
}) {
  const isPlural = unit.plural_arity != null;

  if (isPlural) {
    return (
      <PluralEditor
        unit={unit}
        busy={busy}
        hardFlag={hardFlag}
        onEdit={onEdit}
        onTranslate={onTranslate}
        onAccept={onAccept}
        onSkip={onSkip}
      />
    );
  }

  return (
    <SingularEditor
      unit={unit}
      busy={busy}
      hardFlag={hardFlag}
      onEdit={onEdit}
      onTranslate={onTranslate}
      onAccept={onAccept}
      onSkip={onSkip}
    />
  );
}

// ── Singular editor ────────────────────────────────────────────────────────

function SingularEditor({
  unit,
  busy,
  hardFlag,
  onEdit,
  onTranslate,
  onAccept,
  onSkip,
}: {
  unit: Unit;
  busy: boolean;
  hardFlag: boolean;
  onEdit: (edit: TargetEdit) => void;
  onTranslate: () => void;
  onAccept: () => void;
  onSkip: () => void;
}) {
  const initial =
    unit.target.kind === "singular" ? (unit.target.text ?? "") : "";
  const [draft, setDraft] = useState(initial);
  const initialRef = useRef(initial);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  useEffect(() => {
    setDraft(initial);
    initialRef.current = initial;
  }, [initial]);

  // Auto-grow on content: keep the textarea tall enough for the current
  // draft so multi-line translations stay visible without manual resize.
  // The minimum (~4 rows) is enforced by `rows={4}`; we only ever grow.
  // The `draft` dep is needed even though it's read off the DOM — biome
  // cannot see the dependency because we read scrollHeight after React
  // commits the value change.
  // biome-ignore lint/correctness/useExhaustiveDependencies: dom resize needs to fire after each draft change
  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [draft]);

  const commit = () => {
    if (draft === initialRef.current) return;
    const text = draft.length === 0 ? null : draft;
    onEdit({ kind: "singular", text });
    initialRef.current = draft;
  };

  return (
    <>
      <textarea
        ref={textareaRef}
        value={draft}
        rows={4}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        disabled={busy}
        spellCheck
        aria-label="Translation draft"
        placeholder="Type the translation here…"
        className={cn(
          "w-full px-2.5 py-1.5 rounded-md border resize-none",
          "font-mono text-[13px] leading-snug text-fg-primary",
          "bg-bg-input border-border-subtle",
          "focus:border-accent focus:outline-none focus-visible:outline-none",
          "disabled:bg-bg-surface disabled:text-fg-disabled disabled:cursor-not-allowed",
        )}
      />
      <ActionBar
        busy={busy}
        hardFlag={hardFlag}
        onTranslate={onTranslate}
        onAccept={onAccept}
        onSkip={onSkip}
      />
    </>
  );
}

// ── Plural editor — one textarea per plural form ───────────────────────────

function PluralEditor({
  unit,
  busy,
  hardFlag,
  onEdit,
  onTranslate,
  onAccept,
  onSkip,
}: {
  unit: Unit;
  busy: boolean;
  hardFlag: boolean;
  onEdit: (edit: TargetEdit) => void;
  onTranslate: () => void;
  onAccept: () => void;
  onSkip: () => void;
}) {
  const arity = unit.plural_arity ?? 2;
  const forms =
    unit.target.kind === "plural"
      ? unit.target.forms
      : Array<string | null>(arity).fill(null);

  const [drafts, setDrafts] = useState<string[]>(
    Array.from({ length: arity }, (_, i) => forms[i] ?? ""),
  );
  const initialRef = useRef([...drafts]);

  // Intentionally reset only when the unit identity changes; arity and forms
  // are derived from unit and chasing them would cause spurious re-initialisation
  // on every re-render where the translator has in-progress edits.
  const unitIdRef = useRef(unit.id);
  useEffect(() => {
    if (unitIdRef.current === unit.id) return;
    unitIdRef.current = unit.id;
    const next = Array.from({ length: arity }, (_, i) => forms[i] ?? "");
    setDrafts(next);
    initialRef.current = [...next];
  }, [unit.id, arity, forms]);

  const commitForm = (idx: number) => {
    const text = drafts[idx] ?? "";
    if (text === (initialRef.current[idx] ?? "")) return;
    const val = text.length === 0 ? null : text;
    onEdit({ kind: "plural", form_index: idx, text: val });
    initialRef.current = [...drafts];
  };

  const FORM_LABELS = ["Zero", "One", "Two", "Few", "Many", "Other"];

  return (
    <>
      {drafts.map((draft, idx) => (
        <PluralFormEditor
          key={idx}
          label={FORM_LABELS[idx] ?? `Form ${idx}`}
          value={draft}
          busy={busy}
          onChange={(value) => {
            const next = [...drafts];
            next[idx] = value;
            setDrafts(next);
          }}
          onBlur={() => commitForm(idx)}
        />
      ))}
      <ActionBar
        busy={busy}
        hardFlag={hardFlag}
        onTranslate={onTranslate}
        onAccept={onAccept}
        onSkip={onSkip}
      />
    </>
  );
}

// Per-form plural editor — auto-growing textarea with rows=4 minimum, mirrors
// SingularEditor's growth pattern so multi-form layouts stay scannable.
function PluralFormEditor({
  label,
  value,
  busy,
  onChange,
  onBlur,
}: {
  label: string;
  value: string;
  busy: boolean;
  onChange: (value: string) => void;
  onBlur: () => void;
}) {
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: dom resize needs to fire after each value change
  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [value]);

  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[10px] uppercase tracking-loose text-fg-tertiary font-medium">
        {label}
      </span>
      <textarea
        ref={textareaRef}
        value={value}
        rows={4}
        onChange={(e) => onChange(e.target.value)}
        onBlur={onBlur}
        disabled={busy}
        spellCheck
        aria-label={`Plural form ${label}`}
        placeholder="…"
        className={cn(
          "w-full px-2.5 py-1.5 rounded-md border resize-none",
          "font-mono text-[13px] leading-snug text-fg-primary",
          "bg-bg-input border-border-subtle",
          "focus:border-accent focus:outline-none focus-visible:outline-none",
          "disabled:bg-bg-surface disabled:text-fg-disabled disabled:cursor-not-allowed",
        )}
      />
    </div>
  );
}

// ── Action bar (Translate / Accept / Skip) ──────────────────────────────────

function ActionBar({
  busy,
  hardFlag,
  onTranslate,
  onAccept,
  onSkip,
}: {
  busy: boolean;
  hardFlag: boolean;
  onTranslate: () => void;
  onAccept: () => void;
  onSkip: () => void;
}) {
  return (
    <div className="flex items-center gap-2 mt-1">
      <button
        type="button"
        onClick={onTranslate}
        disabled={busy}
        className={cn(
          "inline-flex items-center gap-1.5 h-7 px-3 rounded-md text-xs font-medium",
          "transition-colors duration-100",
          "text-accent-fg bg-accent",
          "enabled:hover:bg-accent-hover enabled:active:bg-accent-active",
          "disabled:bg-accent-subtle disabled:text-fg-disabled disabled:cursor-not-allowed",
        )}
      >
        <SparklesIcon size={11} />
        {busy ? "Translating…" : "Translate"}
        <KbdChip>⌘T</KbdChip>
      </button>
      <button
        type="button"
        onClick={onAccept}
        disabled={hardFlag || busy}
        title={
          hardFlag
            ? "Resolve the hard gate flag before accepting"
            : busy
              ? "Working…"
              : "Accept and mark Finished"
        }
        className={cn(
          "inline-flex items-center gap-1.5 h-7 px-3 rounded-md text-xs font-medium border",
          "transition-colors duration-100",
          hardFlag || busy
            ? "opacity-40 cursor-not-allowed border-border-subtle text-fg-tertiary bg-transparent"
            : "border-border-default text-fg-primary bg-bg-elevated hover:bg-bg-hover hover:border-border-strong active:bg-bg-selected",
        )}
      >
        <CheckIcon size={11} />
        Accept
        <KbdChip>⌘↵</KbdChip>
      </button>
      <button
        type="button"
        onClick={onSkip}
        className={cn(
          "inline-flex items-center gap-1.5 h-7 px-3 rounded-md text-xs font-medium",
          "text-fg-secondary hover:bg-bg-hover hover:text-fg-primary transition-colors duration-100",
          "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
        )}
      >
        Skip
        <KbdChip>⌘→</KbdChip>
      </button>
    </div>
  );
}

// ── Micro chips ────────────────────────────────────────────────────────────

function MicroChip({
  label,
  tone = "default",
  title,
}: {
  label: string;
  tone?: "default" | "soft" | "accent";
  title?: string;
}) {
  const style: React.CSSProperties =
    tone === "soft"
      ? {
          background: "var(--color-severity-soft-bg)",
          border: "1px solid var(--color-severity-soft-border)",
          color: "var(--color-severity-soft)",
        }
      : tone === "accent"
        ? {
            background: "var(--color-accent-subtle)",
            border: "1px solid var(--color-accent-subtle-border)",
            color: "var(--color-accent)",
          }
        : {
            background: "var(--color-bg-elevated)",
            border: "1px solid var(--color-border-subtle)",
            color: "var(--color-fg-tertiary)",
          };
  return (
    <span
      title={title}
      style={{
        display: "inline-flex",
        alignItems: "center",
        height: 14,
        padding: "0 4px",
        borderRadius: 4,
        fontFamily: "var(--font-mono)",
        fontSize: 9.5,
        whiteSpace: "nowrap",
        ...style,
      }}
    >
      {label}
    </span>
  );
}

function KbdChip({ children }: { children: React.ReactNode }) {
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        height: 16,
        padding: "0 4px",
        borderRadius: 3,
        fontFamily: "var(--font-mono)",
        fontSize: 9,
        background: "transparent",
        border: "1px solid hsla(222, 47%, 11%, 0.25)",
        color: "inherit",
        opacity: 0.7,
      }}
    >
      {children}
    </span>
  );
}

function KbdLegendItem({ kbd, label }: { kbd: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5 text-[11px] text-fg-tertiary">
      <span className="inline-flex items-center h-[18px] px-1.5 rounded-sm border border-border-default bg-bg-elevated font-mono text-[10px] text-fg-secondary">
        {kbd}
      </span>
      <span>{label}</span>
    </span>
  );
}

// ── Inline SVG icons ────────────────────────────────────────────────────────

function SparklesIcon({ size = 12 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.582a.5.5 0 0 1 0 .962L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z" />
      <path d="M20 3v4" />
      <path d="M22 5h-4" />
      <path d="M4 17v2" />
      <path d="M5 18H3" />
    </svg>
  );
}

function CheckIcon({ size = 12 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M20 6 9 17l-5-5" />
    </svg>
  );
}

function XIcon({ size = 12 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M18 6 6 18" />
      <path d="m6 6 12 12" />
    </svg>
  );
}

function SearchIcon({ size = 13 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx={11} cy={11} r={8} />
      <path d="m21 21-4.3-4.3" />
    </svg>
  );
}

function FlagIcon({ size = 11 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z" />
      <line x1={4} x2={4} y1={22} y2={15} />
    </svg>
  );
}

function ListIcon({ size = 11 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <line x1={8} x2={21} y1={6} y2={6} />
      <line x1={8} x2={21} y1={12} y2={12} />
      <line x1={8} x2={21} y1={18} y2={18} />
      <line x1={3} x2={3.01} y1={6} y2={6} />
      <line x1={3} x2={3.01} y1={12} y2={12} />
      <line x1={3} x2={3.01} y1={18} y2={18} />
    </svg>
  );
}

function GridIcon({ size = 11 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect width={7} height={7} x={3} y={3} rx={1} />
      <rect width={7} height={7} x={14} y={3} rx={1} />
      <rect width={7} height={7} x={14} y={14} rx={1} />
      <rect width={7} height={7} x={3} y={14} rx={1} />
    </svg>
  );
}
