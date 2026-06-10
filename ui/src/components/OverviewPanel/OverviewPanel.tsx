// OverviewPanel — project landing view.
// Shows a pickup card, per-locale progress grid, recent activity,
// and a glossary summary. Consumed by App.tsx when projectView === "overview".

import { useEffect, useState } from "react";
import { loadGlossary, loadMetrics } from "../../lib/tauri";
import type {
  CatalogRef,
  CatalogResponse,
  CatalogStateCounts,
  GlossaryLoadResponse,
  MetricEvent,
  MetricsResponse,
  ProjectSummary,
  ReferenceRef,
} from "../../lib/types";
import { Eyebrow, LocaleTag, SegmentBar } from "../primitives";

// ── Inline SVG icons (no external icon library dependency) ────────────────────

function ArrowRightIcon({ size = 16 }: { size?: number }) {
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
      <path d="M5 12h14" />
      <path d="m12 5 7 7-7 7" />
    </svg>
  );
}

function FlagIcon({ size = 16 }: { size?: number }) {
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
      <line x1="4" x2="4" y1="22" y2="15" />
    </svg>
  );
}

// ── Types ─────────────────────────────────────────────────────────────────────

interface LocaleStats {
  locale: string;
  /** Manifest-relative display path for one representative catalog. */
  displayPath: string;
  finished: number;
  proposed: number;
  untranslated: number;
  total: number;
  needsReview: number;
  isDirty: boolean;
}

export interface OverviewPanelProps {
  summary: ProjectSummary;
  openCatalogs: Map<string, CatalogResponse>;
  dirtyCatalogPaths: Set<string>;
  /**
   * Per-catalog unit-state tally from the latest review scan, keyed by absolute
   * catalog path. Covers every catalog (even unopened ones), so progress
   * numbers are accurate without opening each file.
   */
  statsByCatalog: Record<string, CatalogStateCounts>;
  focusLocale: string | null;
  setFocusLocale: (locale: string | null) => void;
  setProjectView: (
    view:
      | "overview"
      | "translate"
      | "glossary"
      | "settings"
      | "quality"
      | "review",
  ) => void;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function relativeTime(isoTs: string): string {
  const diffMs = Date.now() - new Date(isoTs).getTime();
  const s = Math.floor(diffMs / 1000);
  if (s < 60) return "just now";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}

function unitCounts(units: CatalogResponse["units"]): {
  finished: number;
  proposed: number;
  untranslated: number;
  needsReview: number;
} {
  let finished = 0;
  let proposed = 0;
  let untranslated = 0;
  let needsReview = 0;
  for (const u of units) {
    if (u.state === "finished") finished++;
    else if (u.state === "proposed") proposed++;
    else if (u.state === "untranslated") untranslated++;
    if (u.review_status === "needs-review" || (u.flags && u.flags.length > 0)) {
      needsReview++;
    }
  }
  return { finished, proposed, untranslated, needsReview };
}

/**
 * Resolve per-catalog counts, preferring the scan tally (covers unopened
 * catalogs) and falling back to the open-catalog cache when the scan map has no
 * entry for the path yet.
 */
function countsForCatalog(
  ref: CatalogRef,
  statsByCatalog: Record<string, CatalogStateCounts>,
  openCatalogs: Map<string, CatalogResponse>,
): {
  finished: number;
  proposed: number;
  untranslated: number;
  needsReview: number;
} {
  const scan = statsByCatalog[ref.absolute_path];
  if (scan) {
    return {
      finished: scan.finished,
      proposed: scan.proposed,
      untranslated: scan.untranslated,
      needsReview: scan.needs_review,
    };
  }
  const cached = openCatalogs.get(ref.absolute_path);
  if (cached) return unitCounts(cached.units);
  return { finished: 0, proposed: 0, untranslated: 0, needsReview: 0 };
}

/**
 * Compute per-locale aggregated stats, preferring the review-scan tally (which
 * covers every catalog) and falling back to the open-catalog cache.
 */
function buildLocaleStats(
  summary: ProjectSummary,
  openCatalogs: Map<string, CatalogResponse>,
  dirtyCatalogPaths: Set<string>,
  statsByCatalog: Record<string, CatalogStateCounts>,
): LocaleStats[] {
  // Group catalog refs by locale.
  const byLocale = new Map<string, CatalogRef[]>();
  for (const ref of summary.catalogs) {
    const group = byLocale.get(ref.locale) ?? [];
    group.push(ref);
    byLocale.set(ref.locale, group);
  }

  const result: LocaleStats[] = [];
  for (const [locale, refs] of byLocale) {
    let finished = 0;
    let proposed = 0;
    let untranslated = 0;
    let needsReview = 0;
    let isDirty = false;

    for (const ref of refs) {
      const counts = countsForCatalog(ref, statsByCatalog, openCatalogs);
      finished += counts.finished;
      proposed += counts.proposed;
      untranslated += counts.untranslated;
      needsReview += counts.needsReview;
      // Even without a cached catalog, count dirty state.
      if (dirtyCatalogPaths.has(ref.absolute_path)) isDirty = true;
    }

    // Display path: use manifest_path of the first ref.
    const displayPath =
      refs[0]?.manifest_path ?? refs[0]?.absolute_path ?? locale;
    const total = finished + proposed + untranslated;

    result.push({
      locale,
      displayPath,
      finished,
      proposed,
      untranslated,
      total,
      needsReview,
      isDirty,
    });
  }

  return result;
}

/**
 * Pick the most-actionable locale (highest untranslated + proposed count).
 * Returns [primary, secondary] — secondary may be null.
 * Handles: empty project, all-finished project.
 */
export function pickMostActionable(
  stats: LocaleStats[],
): [LocaleStats | null, LocaleStats | null] {
  if (stats.length === 0) return [null, null];

  const scored = [...stats].sort((a, b) => {
    const scoreA = a.untranslated + a.proposed;
    const scoreB = b.untranslated + b.proposed;
    if (scoreB !== scoreA) return scoreB - scoreA;
    // Tiebreak by locale name (alphabetical, stable).
    return a.locale.localeCompare(b.locale);
  });

  const primary = scored[0] ?? null;
  const secondary = scored[1] ?? null;
  return [primary, secondary];
}

// ── Sub-components ────────────────────────────────────────────────────────────

function HeaderStrip({ summary }: { summary: ProjectSummary }) {
  const backendLabel = summary.backend
    ? `${summary.backend.kind}${summary.backend.model ? ` · ${summary.backend.model}` : ""} · ready`
    : "no backend";

  return (
    <div
      style={{
        padding: "22px 28px 18px",
        borderBottom: "1px solid var(--color-border-subtle)",
        display: "flex",
        alignItems: "flex-start",
        gap: 16,
      }}
    >
      <div style={{ flex: 1, minWidth: 0 }}>
        <Eyebrow>Project</Eyebrow>
        <div
          style={{
            display: "flex",
            alignItems: "baseline",
            gap: 10,
            marginTop: 6,
            flexWrap: "wrap",
          }}
        >
          <h1
            style={{
              margin: 0,
              fontSize: 22,
              fontWeight: 600,
              letterSpacing: "-0.015em",
              color: "var(--color-fg-primary)",
              lineHeight: 1.2,
            }}
          >
            {summary.name}
          </h1>
          <span
            style={{
              fontFamily: "var(--font-mono)",
              fontSize: 11,
              color: "var(--color-fg-tertiary)",
              wordBreak: "break-all",
            }}
          >
            {summary.root}
          </span>
        </div>
      </div>
      {/* Backend chip */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "4px 10px",
          borderRadius: "var(--radius-pill)",
          border: "1px solid var(--color-border-default)",
          background: "var(--color-bg-elevated)",
          fontSize: 11.5,
          color: "var(--color-fg-secondary)",
          fontFamily: "var(--font-mono)",
          whiteSpace: "nowrap",
          flexShrink: 0,
        }}
        title="Backend status"
      >
        <span
          style={{
            width: 6,
            height: 6,
            borderRadius: 999,
            background: "var(--color-state-finished)",
            flexShrink: 0,
          }}
          aria-hidden="true"
        />
        {backendLabel}
      </div>
    </div>
  );
}

interface PickupCardProps {
  stats: LocaleStats[];
  onContinue: (locale: string) => void;
  onOpenOther: (locale: string) => void;
}

function PickupCard({ stats, onContinue, onOpenOther }: PickupCardProps) {
  const [primary, secondary] = pickMostActionable(stats);

  // All-caught-up state: no actionable work.
  const primaryActionable = primary
    ? primary.untranslated + primary.proposed
    : 0;

  if (!primary || primaryActionable === 0) {
    return (
      <div
        style={{
          padding: "18px 20px",
          border: "1px solid var(--color-border-subtle)",
          background: "var(--color-bg-surface)",
          borderRadius: 10,
          display: "flex",
          alignItems: "center",
          gap: 14,
        }}
      >
        <div
          style={{
            width: 38,
            height: 38,
            borderRadius: 8,
            background: "var(--color-state-finished-bg)",
            border: "1px solid var(--color-state-finished-border)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: "var(--color-state-finished)",
            flexShrink: 0,
          }}
          aria-hidden="true"
        >
          <ArrowRightIcon size={18} />
        </div>
        <p
          style={{
            margin: 0,
            fontSize: 14,
            color: "var(--color-fg-secondary)",
          }}
        >
          All caught up — no untranslated or proposed units across any locale.
        </p>
      </div>
    );
  }

  return (
    <div
      style={{
        padding: "16px 20px",
        border: "1px solid var(--color-accent-subtle-border)",
        background:
          "linear-gradient(145deg, var(--color-accent-subtle), transparent)",
        borderRadius: 10,
        display: "flex",
        alignItems: "center",
        gap: 18,
      }}
    >
      {/* Icon */}
      <div
        style={{
          width: 38,
          height: 38,
          borderRadius: 8,
          background: "var(--color-accent-subtle)",
          border: "1px solid var(--color-accent-subtle-border)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          color: "var(--color-accent)",
          flexShrink: 0,
        }}
        aria-hidden="true"
      >
        <ArrowRightIcon size={18} />
      </div>

      {/* Centre text */}
      <div
        style={{
          flex: 1,
          minWidth: 0,
          display: "flex",
          flexDirection: "column",
          gap: 4,
        }}
      >
        <Eyebrow>Pick up where you left off</Eyebrow>
        <p
          style={{
            margin: 0,
            fontSize: 13.5,
            color: "var(--color-fg-primary)",
            lineHeight: 1.5,
          }}
        >
          <span style={{ fontFamily: "var(--font-mono)" }}>
            {primary.locale}
          </span>{" "}
          has{" "}
          <strong style={{ color: "var(--color-fg-primary)" }}>
            {primary.untranslated} untranslated
          </strong>{" "}
          and{" "}
          <strong style={{ color: "var(--color-state-proposed)" }}>
            {primary.proposed} proposed
          </strong>{" "}
          units waiting on you.
        </p>
      </div>

      {/* Buttons */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          flexShrink: 0,
        }}
      >
        {secondary && (
          <button
            type="button"
            onClick={() => onOpenOther(secondary.locale)}
            style={{
              height: 30,
              padding: "0 12px",
              borderRadius: "var(--radius-md)",
              border: "1px solid var(--color-border-default)",
              background: "transparent",
              fontSize: 12.5,
              fontWeight: 500,
              color: "var(--color-fg-secondary)",
              cursor: "pointer",
              whiteSpace: "nowrap",
            }}
            aria-label={`Open ${secondary.locale}`}
          >
            Open {secondary.locale}
          </button>
        )}
        <button
          type="button"
          onClick={() => onContinue(primary.locale)}
          style={{
            height: 30,
            padding: "0 14px",
            borderRadius: "var(--radius-md)",
            border: "1px solid transparent",
            background: "var(--color-accent)",
            fontSize: 12.5,
            fontWeight: 600,
            color: "var(--color-accent-fg)",
            cursor: "pointer",
            display: "inline-flex",
            alignItems: "center",
            gap: 6,
            whiteSpace: "nowrap",
          }}
          aria-label={`Continue ${primary.locale}`}
        >
          Continue {primary.locale}
          <ArrowRightIcon size={13} />
        </button>
      </div>
    </div>
  );
}

interface BreakdownDotProps {
  color: string;
  count: number;
  label: string;
}

function BreakdownDot({ color, count, label }: BreakdownDotProps) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 5 }}>
      <span
        style={{
          width: 6,
          height: 6,
          borderRadius: 999,
          background: color,
          flexShrink: 0,
        }}
        aria-hidden="true"
      />
      <span
        style={{
          fontFamily: "var(--font-mono)",
          fontSize: 11.5,
          color: "var(--color-fg-primary)",
        }}
      >
        {count}
      </span>
      <span style={{ fontSize: 11, color: "var(--color-fg-tertiary)" }}>
        {label}
      </span>
    </div>
  );
}

function LocaleProgressCard({
  stat,
  onClick,
}: {
  stat: LocaleStats;
  onClick: () => void;
}) {
  const pct =
    stat.total > 0 ? Math.round((stat.finished / stat.total) * 100) : 0;

  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={`Open ${stat.locale} — ${pct}% finished`}
      style={{
        width: "100%",
        padding: "14px 16px",
        borderRadius: "var(--radius-lg)",
        border: "1px solid var(--color-border-subtle)",
        background: "var(--color-bg-surface)",
        cursor: "pointer",
        textAlign: "left",
        transition: "background 0.1s ease-out, border-color 0.1s ease-out",
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.background = "var(--color-bg-hover)";
        e.currentTarget.style.borderColor = "var(--color-border-default)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = "var(--color-bg-surface)";
        e.currentTarget.style.borderColor = "var(--color-border-subtle)";
      }}
    >
      {/* Header row */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          marginBottom: 12,
        }}
      >
        <LocaleTag locale={stat.locale} tone="accent" />
        <span
          style={{
            flex: 1,
            fontFamily: "var(--font-mono)",
            fontSize: 10.5,
            color: "var(--color-fg-tertiary)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
          title={stat.displayPath}
        >
          {stat.displayPath}
        </span>
        {stat.isDirty && (
          <span
            style={{
              color: "var(--color-state-proposed)",
              fontSize: 14,
              lineHeight: 1,
            }}
            title="Unsaved changes"
          >
            •
          </span>
        )}
      </div>

      {/* Percentage */}
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          gap: 4,
          marginBottom: 10,
        }}
      >
        <span
          style={{
            fontFamily: "var(--font-mono)",
            fontSize: 28,
            fontWeight: 600,
            lineHeight: 1,
            color: "var(--color-fg-primary)",
            letterSpacing: "-0.02em",
          }}
        >
          {pct}
        </span>
        <span style={{ fontSize: 13, color: "var(--color-fg-tertiary)" }}>
          %
        </span>
        <span
          style={{
            fontSize: 11,
            color: "var(--color-fg-tertiary)",
            marginLeft: 2,
          }}
        >
          finished
        </span>
      </div>

      {/* Progress bar */}
      <div style={{ marginBottom: 12 }}>
        <SegmentBar
          total={stat.total}
          finished={stat.finished}
          proposed={stat.proposed}
          height={5}
        />
      </div>

      {/* Breakdown */}
      <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
        <BreakdownDot
          color="var(--color-state-finished)"
          count={stat.finished}
          label="finished"
        />
        <BreakdownDot
          color="var(--color-state-proposed)"
          count={stat.proposed}
          label="proposed"
        />
        <BreakdownDot
          color="var(--color-state-untranslated)"
          count={stat.untranslated}
          label="left"
        />
      </div>

      {/* Needs review row */}
      {stat.needsReview > 0 && (
        <div
          style={{
            marginTop: 10,
            padding: "5px 10px",
            background: "var(--color-severity-soft-bg)",
            border: "1px solid var(--color-severity-soft-border)",
            borderRadius: "var(--radius-sm)",
            display: "flex",
            alignItems: "center",
            gap: 6,
            fontSize: 11.5,
            color: "var(--color-severity-soft)",
          }}
        >
          <FlagIcon size={11} />
          <span>{stat.needsReview} needs review</span>
        </div>
      )}
    </button>
  );
}

// ── Per-file stats ────────────────────────────────────────────────────────────

/**
 * One row per translation-target catalog (references excluded), showing total /
 * untranslated / needs-review counts from the latest review scan. Numbers
 * populate for every catalog without opening each one.
 */
function FilesSection({
  summary,
  openCatalogs,
  statsByCatalog,
  onOpenCatalog,
}: {
  summary: ProjectSummary;
  openCatalogs: Map<string, CatalogResponse>;
  statsByCatalog: Record<string, CatalogStateCounts>;
  onOpenCatalog: (locale: string) => void;
}) {
  // Exclude catalogs that are also registered as references (read-only memory).
  const referencePaths = new Set(
    summary.references.map((r) => r.absolute_path),
  );
  const catalogs = summary.catalogs.filter(
    (c) => !referencePaths.has(c.absolute_path),
  );

  if (catalogs.length === 0) return null;

  return (
    <section aria-labelledby="overview-files-heading">
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          marginBottom: 10,
        }}
      >
        <span id="overview-files-heading">
          <Eyebrow>Files</Eyebrow>
        </span>
        <span style={{ fontSize: 11, color: "var(--color-fg-tertiary)" }}>
          {catalogs.length} {catalogs.length === 1 ? "file" : "files"}
        </span>
      </div>

      <div
        style={{
          display: "flex",
          flexDirection: "column",
          border: "1px solid var(--color-border-subtle)",
          borderRadius: "var(--radius-lg)",
          background: "var(--color-bg-surface)",
          overflow: "hidden",
        }}
      >
        {catalogs.map((c, i) => {
          const counts = countsForCatalog(c, statsByCatalog, openCatalogs);
          const total = counts.finished + counts.proposed + counts.untranslated;
          const loaded =
            statsByCatalog[c.absolute_path] !== undefined ||
            openCatalogs.has(c.absolute_path);
          return (
            <button
              type="button"
              key={c.manifest_path}
              onClick={() => onOpenCatalog(c.locale)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 12,
                padding: "10px 14px",
                border: "none",
                background: "transparent",
                cursor: "pointer",
                textAlign: "left",
                borderBottom:
                  i < catalogs.length - 1
                    ? "1px solid var(--color-border-subtle)"
                    : undefined,
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.background = "var(--color-bg-hover)";
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.background = "transparent";
              }}
              aria-label={`Open ${c.manifest_path}`}
            >
              <span
                style={{
                  fontFamily: "var(--font-mono)",
                  fontSize: 11.5,
                  color: "var(--color-fg-primary)",
                  flex: 1,
                  minWidth: 0,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
                title={c.manifest_path}
              >
                {c.manifest_path}
              </span>
              {loaded ? (
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 12,
                    flexShrink: 0,
                  }}
                >
                  <span
                    style={{
                      fontFamily: "var(--font-mono)",
                      fontSize: 11,
                      color: "var(--color-fg-tertiary)",
                    }}
                    title="Total units"
                  >
                    {total} total
                  </span>
                  <BreakdownDot
                    color="var(--color-state-untranslated)"
                    count={counts.untranslated}
                    label="untranslated"
                  />
                  {counts.needsReview > 0 && (
                    <span
                      style={{
                        display: "inline-flex",
                        alignItems: "center",
                        gap: 5,
                        fontSize: 11.5,
                        color: "var(--color-severity-soft)",
                      }}
                      title="Units needing review"
                    >
                      <FlagIcon size={11} />
                      {counts.needsReview}
                    </span>
                  )}
                </div>
              ) : (
                <span
                  style={{
                    fontSize: 11,
                    color: "var(--color-fg-tertiary)",
                  }}
                >
                  …
                </span>
              )}
            </button>
          );
        })}
      </div>
    </section>
  );
}

// ── References summary ────────────────────────────────────────────────────────

function ReferencesSummarySection({
  references,
  onOpenSettings,
}: {
  references: ReferenceRef[];
  onOpenSettings: () => void;
}) {
  // Group by locale for a tidy, scannable list.
  const byLocale = new Map<string, ReferenceRef[]>();
  for (const r of references) {
    const group = byLocale.get(r.locale) ?? [];
    group.push(r);
    byLocale.set(r.locale, group);
  }

  return (
    <section aria-labelledby="overview-refs-heading">
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          marginBottom: 10,
        }}
      >
        <span id="overview-refs-heading">
          <Eyebrow>Reference files</Eyebrow>
        </span>
        <div style={{ flex: 1 }} />
        <button
          type="button"
          onClick={onOpenSettings}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 4,
            padding: "0 8px",
            height: 24,
            borderRadius: "var(--radius-md)",
            border: "1px solid var(--color-border-subtle)",
            background: "transparent",
            fontSize: 11.5,
            fontWeight: 500,
            color: "var(--color-fg-tertiary)",
            cursor: "pointer",
          }}
          aria-label="Edit reference files in Settings"
        >
          Edit in Settings →
        </button>
      </div>

      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 8,
          border: "1px solid var(--color-border-subtle)",
          borderRadius: "var(--radius-lg)",
          background: "var(--color-bg-surface)",
          overflow: "hidden",
        }}
      >
        {[...byLocale.entries()].map(([locale, refs], groupIdx) => (
          <div
            key={locale}
            style={{
              padding: "10px 14px",
              borderBottom:
                groupIdx < byLocale.size - 1
                  ? "1px solid var(--color-border-subtle)"
                  : undefined,
            }}
          >
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                marginBottom: 6,
              }}
            >
              <LocaleTag locale={locale} tone="muted" />
              <span style={{ fontSize: 11, color: "var(--color-fg-tertiary)" }}>
                {refs.length} {refs.length === 1 ? "file" : "files"}
              </span>
            </div>
            <ul
              style={{
                margin: 0,
                padding: 0,
                listStyle: "none",
                display: "flex",
                flexDirection: "column",
                gap: 3,
              }}
            >
              {refs.map((r) => (
                <li
                  key={r.manifest_path}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                    fontSize: 11.5,
                  }}
                >
                  <span
                    style={{
                      fontFamily: "var(--font-mono)",
                      color: "var(--color-fg-primary)",
                      flex: 1,
                      minWidth: 0,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                    title={r.manifest_path}
                  >
                    {r.manifest_path}
                  </span>
                  {r.status !== "ok" && (
                    <span
                      style={{
                        fontSize: 10,
                        padding: "1px 5px",
                        borderRadius: 3,
                        border: "1px solid var(--color-severity-hard-border)",
                        background: "var(--color-severity-hard-bg)",
                        color: "var(--color-severity-hard)",
                        flexShrink: 0,
                      }}
                      title={`File status: ${r.status}`}
                    >
                      {r.status}
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
    </section>
  );
}

// ── Activity dot colour by event kind ────────────────────────────────────────

function activityDotColor(event: MetricEvent): string {
  if (event.event === "gate-reject") return "var(--color-severity-hard)";
  if (event.event === "soft-warning") return "var(--color-severity-soft)";
  if (event.event === "human-edit") return "var(--color-state-finished)";
  return "var(--color-severity-info)";
}

function eventLabel(event: MetricEvent): string {
  switch (event.event) {
    case "gate-reject":
      return `Gate reject · ${event.rule} · ${event.locale}`;
    case "soft-warning":
      return `Soft warning · ${event.rule} · ${event.locale}`;
    case "human-edit":
      return `Human edit · ${event.unit_id} · ${event.locale}`;
    case "retry":
      return `Retry · ${event.unit_id} · ${event.locale}`;
    default:
      return `${event.event} · ${event.locale}`;
  }
}

function RecentActivityCard({ metricsPath }: { metricsPath: string | null }) {
  const [events, setEvents] = useState<MetricEvent[]>([]);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (!metricsPath) {
      setLoaded(true);
      return;
    }
    loadMetrics(metricsPath)
      .then((resp: MetricsResponse) => {
        // Most recent events first, cap at 5.
        const sorted = [...resp.events]
          .sort((a, b) => b.ts.localeCompare(a.ts))
          .slice(0, 5);
        setEvents(sorted);
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }, [metricsPath]);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <Eyebrow>Recent activity</Eyebrow>
      <div
        style={{
          border: "1px solid var(--color-border-subtle)",
          borderRadius: "var(--radius-lg)",
          background: "var(--color-bg-surface)",
          overflow: "hidden",
        }}
      >
        {!loaded ? (
          <p
            style={{
              margin: 0,
              padding: "16px 14px",
              fontSize: 12,
              color: "var(--color-fg-tertiary)",
            }}
          >
            Loading…
          </p>
        ) : events.length === 0 ? (
          <p
            style={{
              margin: 0,
              padding: "16px 14px",
              fontSize: 12,
              color: "var(--color-fg-tertiary)",
            }}
          >
            No activity yet.
          </p>
        ) : (
          events.map((ev, i) => (
            <div
              key={`${ev.ts}-${i}`}
              style={{
                display: "flex",
                alignItems: "flex-start",
                gap: 10,
                padding: "9px 14px",
                borderBottom:
                  i < events.length - 1
                    ? "1px solid var(--color-border-subtle)"
                    : undefined,
              }}
            >
              <span
                style={{
                  width: 6,
                  height: 6,
                  borderRadius: 999,
                  background: activityDotColor(ev),
                  flexShrink: 0,
                  marginTop: 4,
                }}
                aria-hidden="true"
              />
              <div
                style={{
                  flex: 1,
                  minWidth: 0,
                  display: "flex",
                  flexDirection: "column",
                  gap: 2,
                }}
              >
                <span
                  style={{
                    fontSize: 12.5,
                    color: "var(--color-fg-primary)",
                    lineHeight: 1.4,
                  }}
                >
                  {eventLabel(ev)}
                </span>
                <span
                  style={{
                    fontFamily: "var(--font-mono)",
                    fontSize: 10.5,
                    color: "var(--color-fg-tertiary)",
                  }}
                >
                  {relativeTime(ev.ts)} · {ev.backend}
                </span>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function GlossarySummaryCard({
  glossaryPath,
  locales,
}: {
  glossaryPath: string | null;
  locales: string[];
}) {
  const [glossary, setGlossary] = useState<GlossaryLoadResponse | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (!glossaryPath) {
      setLoaded(true);
      return;
    }
    loadGlossary(glossaryPath)
      .then((resp: GlossaryLoadResponse) => {
        setGlossary(resp);
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }, [glossaryPath]);

  const terms = glossary?.payload.terms ?? [];
  const missingCount = terms.reduce((acc, term) => {
    const missing = locales.filter((l) => !term.translations[l]);
    return acc + missing.length;
  }, 0);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <Eyebrow>Glossary</Eyebrow>
      <div
        style={{
          border: "1px solid var(--color-border-subtle)",
          borderRadius: "var(--radius-lg)",
          background: "var(--color-bg-surface)",
          overflow: "hidden",
        }}
      >
        {!loaded ? (
          <p
            style={{
              margin: 0,
              padding: "16px 14px",
              fontSize: 12,
              color: "var(--color-fg-tertiary)",
            }}
          >
            Loading…
          </p>
        ) : !glossaryPath ? (
          <p
            style={{
              margin: 0,
              padding: "16px 14px",
              fontSize: 12,
              color: "var(--color-fg-tertiary)",
            }}
          >
            No glossary configured.
          </p>
        ) : terms.length === 0 ? (
          <p
            style={{
              margin: 0,
              padding: "16px 14px",
              fontSize: 12,
              color: "var(--color-fg-tertiary)",
            }}
          >
            Glossary is empty.
          </p>
        ) : (
          <>
            {/* Header */}
            <div
              style={{
                padding: "11px 16px",
                borderBottom: "1px solid var(--color-border-subtle)",
                fontSize: 12,
                color: "var(--color-fg-secondary)",
              }}
            >
              <span style={{ fontFamily: "var(--font-mono)" }}>
                {terms.length}
              </span>{" "}
              term{terms.length !== 1 ? "s" : ""}
              {missingCount > 0 && (
                <>
                  {" · "}
                  <span style={{ color: "var(--color-severity-soft)" }}>
                    {missingCount} missing
                  </span>
                </>
              )}
            </div>
            {/* Term rows */}
            {terms.map((term) => (
              <div
                key={term.source}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  padding: "6px 16px",
                  fontSize: 12,
                  borderBottom: "1px solid var(--color-border-subtle)",
                }}
              >
                <span
                  style={{
                    fontFamily: "var(--font-mono)",
                    color: "var(--color-fg-primary)",
                    flex: 1,
                    minWidth: 0,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {term.source}
                </span>
                <div style={{ display: "flex", gap: 4, flexShrink: 0 }}>
                  {locales.map((l) => {
                    const has = Boolean(term.translations[l]);
                    return (
                      <span
                        key={l}
                        title={`${l}: ${has ? "set" : "empty"}`}
                        style={{
                          fontFamily: "var(--font-mono)",
                          fontSize: 9.5,
                          padding: "1px 5px",
                          borderRadius: 3,
                          border: "1px solid",
                          borderColor: has
                            ? "var(--color-state-finished-border)"
                            : "var(--color-border-subtle)",
                          background: has
                            ? "var(--color-state-finished-bg)"
                            : "transparent",
                          color: has
                            ? "var(--color-state-finished)"
                            : "var(--color-fg-disabled)",
                          opacity: has ? 1 : 0.6,
                        }}
                      >
                        {l}
                      </span>
                    );
                  })}
                </div>
              </div>
            ))}
          </>
        )}
      </div>
    </div>
  );
}

// ── Main panel ────────────────────────────────────────────────────────────────

export function OverviewPanel({
  summary,
  openCatalogs,
  dirtyCatalogPaths,
  statsByCatalog,
  setFocusLocale,
  setProjectView,
}: OverviewPanelProps) {
  const stats = buildLocaleStats(
    summary,
    openCatalogs,
    dirtyCatalogPaths,
    statsByCatalog,
  );

  // Metrics path: <state_dir>/metrics.jsonl (harness convention).
  const metricsPath = summary.state_dir
    ? `${summary.state_dir}/metrics.jsonl`
    : null;

  function handleContinue(locale: string) {
    setFocusLocale(locale);
    setProjectView("translate");
  }

  function handleOpenOther(locale: string) {
    setFocusLocale(locale);
    setProjectView("translate");
  }

  function handleViewAsMatrix() {
    setFocusLocale(null);
    setProjectView("translate");
  }

  return (
    <div
      style={{
        flex: 1,
        overflow: "auto",
        background: "var(--color-bg-base)",
        display: "flex",
        flexDirection: "column",
      }}
    >
      {/* Header */}
      <HeaderStrip summary={summary} />

      {/* Scrollable content */}
      <div
        style={{
          padding: "22px 28px 32px",
          display: "flex",
          flexDirection: "column",
          gap: 28,
          maxWidth: 1100,
          width: "100%",
          boxSizing: "border-box",
        }}
      >
        {/* Pickup card */}
        <PickupCard
          stats={stats}
          onContinue={handleContinue}
          onOpenOther={handleOpenOther}
        />

        {/* Progress by locale */}
        <section aria-labelledby="overview-locales-heading">
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              marginBottom: 12,
            }}
          >
            <span id="overview-locales-heading">
              <Eyebrow>Progress by locale</Eyebrow>
            </span>
            <div style={{ flex: 1 }} />
            <button
              type="button"
              onClick={handleViewAsMatrix}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 5,
                padding: "0 8px",
                height: 24,
                borderRadius: "var(--radius-md)",
                border: "1px solid var(--color-border-subtle)",
                background: "transparent",
                fontSize: 11.5,
                fontWeight: 500,
                color: "var(--color-fg-tertiary)",
                cursor: "pointer",
              }}
              aria-label="View as matrix"
            >
              View as matrix
              <ArrowRightIcon size={12} />
            </button>
          </div>

          {stats.length === 0 ? (
            <p style={{ fontSize: 13, color: "var(--color-fg-tertiary)" }}>
              No locales configured. Add catalogs in Settings.
            </p>
          ) : (
            <ul
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(3, 1fr)",
                gap: 14,
                listStyle: "none",
                margin: 0,
                padding: 0,
              }}
              aria-label="Locale progress cards"
            >
              {stats.map((stat) => (
                <li key={stat.locale}>
                  <LocaleProgressCard
                    stat={stat}
                    onClick={() => handleContinue(stat.locale)}
                  />
                </li>
              ))}
            </ul>
          )}
        </section>

        {/* Per-file stats */}
        <FilesSection
          summary={summary}
          openCatalogs={openCatalogs}
          statsByCatalog={statsByCatalog}
          onOpenCatalog={handleContinue}
        />

        {/* Reference files (only when present) */}
        {summary.references.length > 0 && (
          <ReferencesSummarySection
            references={summary.references}
            onOpenSettings={() => setProjectView("settings")}
          />
        )}

        {/* Activity + glossary */}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "1.4fr 1fr",
            gap: 14,
            alignItems: "start",
          }}
        >
          <RecentActivityCard metricsPath={metricsPath} />
          <GlossarySummaryCard
            glossaryPath={summary.glossary_path}
            locales={summary.locales}
          />
        </div>
      </div>
    </div>
  );
}
