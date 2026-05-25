// ProofreadView — read-only manuscript view for final project sign-off.
//
// Left side-nav:   module contents, show/filter toggles, Export… button.
// Header:          project + per-locale stat cards.
// Document:        centred max-width 1040px, grouped by catalog/module,
//                  each unit row with coverage gutter, source, translation grid.
// Sign-off footer: summary + "Mark project reviewed" + "Open remaining hard flags".
//
// Strictly read-only. No textareas, no Accept buttons, no edit paths here.
// Clicking a unit jumps to Translate (with focusLocale if a locale column is active).

import { useCallback, useEffect, useMemo, useState } from "react";
import type {
  CatalogResponse,
  GateReport,
  ProjectSummary,
  Unit,
  UnitId,
} from "../../lib/types";
import { ProgressBar } from "../primitives";

// ── Inline SVG icons ──────────────────────────────────────────────────────────

function FolderIcon({ size = 14 }: { size?: number }) {
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
      <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
    </svg>
  );
}

function BookOpenIcon({ size = 14 }: { size?: number }) {
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
      <path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" />
      <path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" />
    </svg>
  );
}

function AlertIcon({ size = 12 }: { size?: number }) {
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
      style={{ flexShrink: 0 }}
    >
      <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z" />
      <line x1="12" y1="9" x2="12" y2="13" />
      <line x1="12" y1="17" x2="12.01" y2="17" />
    </svg>
  );
}

function CheckIcon({ size = 14 }: { size?: number }) {
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
      <polyline points="20 6 9 17 4 12" />
    </svg>
  );
}

function ArrowRightIcon({ size = 14 }: { size?: number }) {
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

// ── Types ─────────────────────────────────────────────────────────────────────

export interface ProofreadViewProps {
  summary: ProjectSummary;
  openCatalogs: Map<string, CatalogResponse>;
  reports: Record<UnitId, GateReport>;
  // Navigation callbacks — called when user clicks a unit to jump to Translate.
  onNavigateToUnit: (
    catalogPath: string,
    unitId: UnitId,
    locale?: string,
  ) => void;
  // Called when "Open remaining hard flags" is clicked.
  onOpenHardFlags: () => void;
  /** Auto-load a project catalog into the App's openCatalogs cache. Optional
   *  for backward compatibility; when present, ProofreadView fan-loads every
   *  project catalog on mount so the manuscript renders without forcing the
   *  user to open each catalog manually first. */
  onEnsureCatalogLoaded?: (absPath: string) => Promise<void>;
}

// Show/filter toggle state — local to this view.
interface ShowToggles {
  sourceIds: boolean;
  emptyTranslations: boolean;
  gateFlags: boolean;
  placeholders: boolean;
}

type FilterChipKey = "flagged" | "untranslated" | "glossary";

// ── Per-locale head stat card ─────────────────────────────────────────────────

function LocaleHeadStat({
  locale,
  finished,
  total,
}: {
  locale: string;
  finished: number;
  total: number;
}) {
  const pct = total > 0 ? Math.round((finished / total) * 100) : 0;
  const isHealthy = pct >= 80;
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 4,
        padding: "6px 10px",
        border: "1px solid var(--color-border-subtle)",
        borderRadius: 5,
        minWidth: 80,
        background: "var(--color-bg-surface)",
        flexShrink: 0,
      }}
    >
      <span
        style={{
          fontFamily: "monospace",
          fontSize: 10.5,
          color: "var(--color-fg-tertiary)",
          letterSpacing: "0.04em",
          textTransform: "uppercase",
        }}
      >
        {locale}
      </span>
      <div style={{ display: "flex", alignItems: "baseline", gap: 6 }}>
        <span
          style={{
            fontFamily: "monospace",
            fontSize: 15,
            fontWeight: 600,
            color: isHealthy
              ? "var(--color-state-finished)"
              : "var(--color-state-proposed)",
          }}
        >
          {pct}
          <span style={{ fontSize: 9, color: "var(--color-fg-tertiary)" }}>
            %
          </span>
        </span>
        <ProgressBar
          total={total}
          finished={finished}
          proposed={0}
          height={2}
          width={36}
        />
      </div>
    </div>
  );
}

// ── Proofread row — pivoted by source unit id across all loaded catalogs ─────
//
// One row per unique unit.id. Each row carries per-locale targets sourced
// from the locale-matching catalog, so the rendering for "Save" in es_ES
// reads from app_es.ts, not from whichever catalog the previous iteration
// pulled blindly.
//
// Earlier shape: array of CatalogModuleData, each iterating one catalog's
// units; that data structure had no way to express "this unit in this
// locale" and silently copied the per-catalog target into every locale
// column. The pivot below makes the cross-locale view honest.

export interface ProofreadUnitRow {
  unitId: UnitId;
  source: string;
  placeholderCount: number;
  isPlural: boolean;
  pluralArity: number | null;
  flags: string[];
  /** locale -> { unit, catalogPath } when loaded; missing otherwise. */
  byLocale: Map<string, { unit: Unit; catalogPath: string }>;
}

export function buildProofreadRows(
  summary: ProjectSummary,
  openCatalogs: Map<string, CatalogResponse>,
): ProofreadUnitRow[] {
  const rows = new Map<UnitId, ProofreadUnitRow>();
  for (const ref of summary.catalogs) {
    const cached = openCatalogs.get(ref.absolute_path);
    if (!cached) continue;
    for (const unit of cached.units) {
      const existing = rows.get(unit.id);
      if (existing) {
        existing.byLocale.set(ref.locale, {
          unit,
          catalogPath: ref.absolute_path,
        });
        // Surface flags union (any locale flagged → flagged in review).
        for (const flag of unit.flags) {
          if (!existing.flags.includes(flag)) existing.flags.push(flag);
        }
        continue;
      }
      rows.set(unit.id, {
        unitId: unit.id,
        source: unit.source,
        placeholderCount: Array.isArray(unit.placeholders)
          ? unit.placeholders.length
          : 0,
        isPlural: unit.plural_arity != null,
        pluralArity: unit.plural_arity ?? null,
        flags: [...unit.flags],
        byLocale: new Map([
          [ref.locale, { unit, catalogPath: ref.absolute_path }],
        ]),
      });
    }
  }
  return Array.from(rows.values()).sort((a, b) =>
    a.unitId.localeCompare(b.unitId),
  );
}

// ── Side-nav contents item (per-catalog, for navigation only) ────────────
//
// The document body is a single flat unit list (pivoted by id), but the
// translator still thinks in terms of catalog files when navigating. The
// "Contents" list in the side nav enumerates the per-locale catalog files
// so a click can scroll the manuscript to that catalog's first unit.
//
// (For now, since the document is one flat section, every contents click
// just scrolls to the top. A future multi-module grouping pass can wire
// per-catalog anchors when we have a manifest-level module concept.)

interface CatalogNavItem {
  catalogPath: string;
  locale: string;
  /** Display name derived from the manifest path basename. */
  displayName: string;
  /** Short path hint for the tertiary line under the heading. */
  pathHint: string;
  finished: number;
  total: number;
}

function buildCatalogNavItems(
  summary: ProjectSummary,
  openCatalogs: Map<string, CatalogResponse>,
): CatalogNavItem[] {
  return summary.catalogs.map((ref) => {
    const response = openCatalogs.get(ref.absolute_path);
    const units = response?.units ?? [];
    const parts = ref.manifest_path.replace(/\\/g, "/").split("/");
    const basename = parts[parts.length - 1] ?? ref.manifest_path;
    const displayName = basename.replace(/\.[^.]+$/, ""); // strip extension
    const pathHint =
      parts.length > 2 ? `…/${parts.slice(-2).join("/")}` : ref.manifest_path;
    const finished = units.filter((u) => u.state === "finished").length;
    return {
      catalogPath: ref.absolute_path,
      locale: ref.locale,
      displayName,
      pathHint,
      finished,
      total: units.length,
    };
  });
}

// ── Coverage dot ──────────────────────────────────────────────────────────────

type DotState = "finished" | "hard" | "empty";

function CoverageDot({ locale, state }: { locale: string; state: DotState }) {
  const bg =
    state === "finished"
      ? "var(--color-state-finished)"
      : state === "hard"
        ? "var(--color-severity-hard)"
        : "var(--color-fg-disabled)";
  return (
    <span
      className="review-coverage-dot"
      data-state={state}
      title={`${locale}: ${state}`}
      style={{
        display: "block",
        width: 6,
        height: 6,
        borderRadius: 999,
        background: bg,
        opacity: state === "empty" ? 0.4 : 1,
        flexShrink: 0,
      }}
    />
  );
}

// ── Micro-badge ───────────────────────────────────────────────────────────────

function MicroBadge({
  children,
  tone = "neutral",
}: {
  children: React.ReactNode;
  tone?: "neutral" | "soft" | "info";
}) {
  const color =
    tone === "soft"
      ? "var(--color-severity-soft)"
      : tone === "info"
        ? "var(--color-severity-info)"
        : "var(--color-fg-tertiary)";
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 2,
        height: 16,
        padding: "0 5px",
        borderRadius: 3,
        border: "1px solid var(--color-border-subtle)",
        fontFamily: "monospace",
        fontSize: 10,
        color,
        background: "var(--color-bg-elevated)",
        flexShrink: 0,
      }}
    >
      {children}
    </span>
  );
}

// ── Translation row ───────────────────────────────────────────────────────────

function TranslationLine({
  locale,
  text,
  flags,
  showGateFlags,
}: {
  locale: string;
  text: string | null;
  flags: string[];
  showGateFlags: boolean;
}) {
  const empty = text == null || text === "";
  const flagged = showGateFlags && flags.length > 0;
  return (
    <div
      className="review-translation-line"
      style={{
        display: "flex",
        alignItems: "baseline",
        gap: 8,
        padding: "4px 0",
      }}
    >
      <span
        style={{
          flex: "0 0 56px",
          fontFamily: "monospace",
          fontSize: 10.5,
          color: "var(--color-fg-tertiary)",
          letterSpacing: "0.04em",
        }}
      >
        {locale}
      </span>
      {empty ? (
        <span
          style={{
            fontFamily: "monospace",
            fontSize: 12.5,
            fontStyle: "italic",
            color: "var(--color-fg-disabled)",
          }}
        >
          —
        </span>
      ) : (
        <span
          style={{
            fontFamily: "monospace",
            fontSize: 13,
            lineHeight: 1.45,
            color: "var(--color-fg-primary)",
            flex: 1,
          }}
        >
          {text}
        </span>
      )}
      {flagged && (
        <span
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 3,
            color: "var(--color-severity-hard)",
            flexShrink: 0,
          }}
          title={flags.join(", ")}
        >
          <AlertIcon size={11} />
          <span style={{ fontFamily: "monospace", fontSize: 10 }}>
            {flags[0]}
          </span>
        </span>
      )}
    </div>
  );
}

// ── Review unit row ───────────────────────────────────────────────────────────
//
// One row per source unit id. Each per-locale cell pulls its target from
// the matching catalog (via row.byLocale.get(locale)), so the de_DE
// column shows the German translation and the es_ES column shows the
// Spanish — never the same value copied across columns.

function ReviewUnitRow({
  row,
  locales,
  reports,
  show,
  activeLocale,
  onClick,
}: {
  row: ProofreadUnitRow;
  locales: string[];
  reports: Record<UnitId, GateReport>;
  show: ShowToggles;
  /** The locale the user has "active" in the side-nav, if any. */
  activeLocale: string | null;
  /** Click handler — receives the catalog path for the locale the user
   *  was looking at, so navigation lands in the right Focus context. */
  onClick: (catalogPath: string, unitId: UnitId, locale?: string) => void;
}) {
  // Per-locale target text — reads from THAT locale's catalog.
  function targetText(locale: string): string | null {
    const entry = row.byLocale.get(locale);
    if (!entry) return null;
    const t = entry.unit.target;
    if (t.kind === "singular") return t.text;
    return t.forms.find((f) => f != null) ?? null;
  }

  // Per-locale state for coverage dot — reflects THAT locale's unit state.
  function dotState(locale: string): DotState {
    const entry = row.byLocale.get(locale);
    if (!entry) return "empty";
    const unit = entry.unit;
    // Hard flag at the gate level wins.
    const report = reports[unit.id];
    const hasHardFinding = report?.findings.some((f) => {
      const s = f.flag?.toLowerCase() ?? "";
      return (
        s.includes("placeholder") ||
        s.includes("plural-arity") ||
        s.includes("icu-parse") ||
        s.includes("empty-target") ||
        s.includes("backend-malformed")
      );
    });
    if (hasHardFinding) return "hard";
    if (unit.state === "finished") return "finished";
    const text = targetText(locale);
    if (!text) return "empty";
    return "finished";
  }

  // Per-locale flags from the gate report for that locale's unit.
  function flagsFor(locale: string): string[] {
    const entry = row.byLocale.get(locale);
    if (!entry) return [];
    return reports[entry.unit.id]?.findings.map((f) => f.flag) ?? [];
  }

  const sourceText = row.source;
  const unitIdText = row.unitId;

  // Filled across locales — one per locale that has a non-null target.
  const filledCount = locales.reduce((acc, l) => {
    const text = targetText(l);
    return acc + (text != null && text !== "" ? 1 : 0);
  }, 0);
  const totalLocales = locales.length || 1;

  // Pick the catalog path to route the click to: prefer the active
  // locale, then the focus locale match, otherwise any loaded locale.
  const fallbackEntry = activeLocale
    ? row.byLocale.get(activeLocale)
    : undefined;
  const anyEntry = fallbackEntry ?? row.byLocale.values().next().value;
  const navCatalog = anyEntry?.catalogPath ?? null;
  const navLocale = fallbackEntry?.unit
    ? (activeLocale ?? undefined)
    : undefined;

  return (
    <button
      type="button"
      className="review-unit-row"
      style={{
        display: "flex",
        gap: 14,
        alignItems: "flex-start",
        cursor: navCatalog ? "pointer" : "default",
        borderRadius: 4,
        padding: "6px 4px",
        transition: "background 80ms",
        background: "transparent",
        border: "none",
        width: "100%",
        textAlign: "left",
      }}
      aria-label={`Unit ${unitIdText} — click to open in Translate`}
      disabled={!navCatalog}
      onClick={() => navCatalog && onClick(navCatalog, row.unitId, navLocale)}
      onMouseEnter={(e) =>
        ((e.currentTarget as HTMLButtonElement).style.background =
          "var(--color-bg-hover)")
      }
      onMouseLeave={(e) =>
        ((e.currentTarget as HTMLButtonElement).style.background =
          "transparent")
      }
    >
      {/* Left gutter — coverage dots, one per locale */}
      <div
        style={{
          flex: "0 0 18px",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          paddingTop: 4,
          gap: 4,
        }}
        aria-hidden="true"
      >
        {locales.map((l) => (
          <CoverageDot key={l} locale={l} state={dotState(l)} />
        ))}
      </div>

      {/* Source + meta + translations */}
      <div
        style={{ flex: 1, display: "flex", flexDirection: "column", gap: 6 }}
      >
        {/* Source line */}
        <div
          style={{
            display: "flex",
            alignItems: "baseline",
            gap: 8,
            flexWrap: "wrap",
          }}
        >
          <span
            style={{
              fontFamily: "monospace",
              fontSize: 15,
              fontWeight: 500,
              color: "var(--color-fg-primary)",
              lineHeight: 1.4,
            }}
          >
            {sourceText}
          </span>
          {row.isPlural && show.placeholders && (
            <MicroBadge tone="neutral">plural ×{row.pluralArity}</MicroBadge>
          )}
          {row.placeholderCount > 0 && show.placeholders && (
            <MicroBadge tone="neutral">
              {"{}"}×{row.placeholderCount}
            </MicroBadge>
          )}
          {row.flags.length > 0 && show.gateFlags && (
            <MicroBadge tone="soft">{row.flags[0]}</MicroBadge>
          )}
          <span style={{ flex: 1 }} />
          <span
            style={{
              fontFamily: "monospace",
              fontSize: 10.5,
              color: "var(--color-fg-tertiary)",
              whiteSpace: "nowrap",
            }}
          >
            {filledCount}/{totalLocales} translated
          </span>
        </div>

        {/* Unit ID */}
        {show.sourceIds && (
          <span
            style={{
              fontFamily: "monospace",
              fontSize: 10.5,
              color: "var(--color-fg-tertiary)",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
            title={unitIdText}
          >
            {unitIdText}
          </span>
        )}

        {/* Translation grid — one cell per project locale, each from the
            matching catalog (or `—` when that catalog isn't loaded or
            doesn't contain this unit). */}
        <div
          className="review-translation-grid"
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(280px, 1fr))",
            gap: "4px 20px",
            marginTop: 2,
            paddingLeft: 14,
            borderLeft: "2px solid var(--color-border-subtle)",
          }}
        >
          {locales.map((l) => {
            const text = targetText(l);
            if (!show.emptyTranslations && (text == null || text === "")) {
              return null;
            }
            return (
              <TranslationLine
                key={l}
                locale={l}
                text={text}
                flags={flagsFor(l)}
                showGateFlags={show.gateFlags}
              />
            );
          })}
        </div>
      </div>
    </button>
  );
}

// ── Proofread section — flat unit list across the whole project ─────────────
//
// The earlier per-catalog grouping conflated "catalog file" with "module".
// Multi-module projects need a manifest-level concept that doesn't exist
// yet, so for now the document body is one flat section. The side-nav
// still enumerates per-catalog navigation items for the translator's
// mental model (TODO: route those clicks to per-module anchors when the
// data model supports them).

function ProofreadSection({
  title,
  rows,
  locales,
  reports,
  show,
  activeFilters,
  activeLocale,
  onUnitClick,
}: {
  title: string;
  rows: ProofreadUnitRow[];
  locales: string[];
  reports: Record<UnitId, GateReport>;
  show: ShowToggles;
  activeFilters: Set<FilterChipKey>;
  activeLocale: string | null;
  onUnitClick: (catalogPath: string, unitId: UnitId, locale?: string) => void;
}) {
  const filteredRows = useMemo(() => {
    return rows.filter((row) => {
      if (activeFilters.has("flagged") && row.flags.length === 0) return false;
      if (activeFilters.has("untranslated")) {
        // "Untranslated" filter: any project locale missing or untranslated.
        const anyUntranslated = locales.some((l) => {
          const entry = row.byLocale.get(l);
          if (!entry) return true;
          return entry.unit.state === "untranslated";
        });
        if (!anyUntranslated) return false;
      }
      // Glossary filter: not represented in the wire type yet; skip silently.
      return true;
    });
  }, [rows, activeFilters, locales]);

  if (filteredRows.length === 0) return null;

  // Aggregate finished count across all locales for the header line.
  const total = rows.length;
  const finished = rows.filter((row) =>
    locales.every((l) => row.byLocale.get(l)?.unit.state === "finished"),
  ).length;

  return (
    <section className="review-module-section" aria-label={title}>
      <header
        className="review-module-header"
        style={{
          display: "flex",
          alignItems: "baseline",
          gap: 10,
          paddingBottom: 10,
          borderBottom: "1px solid var(--color-border-default)",
          marginBottom: 12,
        }}
      >
        <span style={{ color: "var(--color-fg-secondary)" }}>
          <FolderIcon size={13} />
        </span>
        <h2
          style={{
            margin: 0,
            fontFamily: "monospace",
            fontSize: 15,
            fontWeight: 600,
            color: "var(--color-fg-primary)",
            letterSpacing: 0,
          }}
        >
          {title}
        </h2>
        <span
          style={{
            fontFamily: "monospace",
            fontSize: 11,
            color: "var(--color-fg-tertiary)",
          }}
        >
          {total} units
        </span>
        <span
          style={{
            fontFamily: "monospace",
            fontSize: 10.5,
            color: "var(--color-fg-disabled)",
          }}
        >
          ·
        </span>
        <span
          style={{
            fontFamily: "monospace",
            fontSize: 10.5,
            color: "var(--color-fg-disabled)",
          }}
        >
          {finished}/{total} fully finished
        </span>
      </header>

      <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
        {filteredRows.map((row) => (
          <ReviewUnitRow
            key={row.unitId}
            row={row}
            locales={locales}
            reports={reports}
            show={show}
            activeLocale={activeLocale}
            onClick={onUnitClick}
          />
        ))}
      </div>
    </section>
  );
}

// ── Side navigation ───────────────────────────────────────────────────────────

function SideNav({
  catalogNavItems,
  show,
  onShowChange,
  activeFilters,
  onFilterToggle,
  onExport,
}: {
  catalogNavItems: CatalogNavItem[];
  show: ShowToggles;
  onShowChange: (key: keyof ShowToggles) => void;
  activeFilters: Set<FilterChipKey>;
  onFilterToggle: (key: FilterChipKey) => void;
  onExport: () => void;
}) {
  return (
    <aside
      className="review-panel-sidenav"
      style={{
        width: 232,
        flexShrink: 0,
        display: "flex",
        flexDirection: "column",
        gap: 16,
        background: "var(--color-bg-surface)",
        borderRight: "1px solid var(--color-border-subtle)",
        padding: "16px 14px",
        overflowY: "auto",
      }}
      aria-label="Review navigation"
    >
      {/* Eyebrow */}
      <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            padding: "0 4px 4px",
          }}
        >
          <span style={{ color: "var(--color-fg-secondary)" }}>
            <BookOpenIcon size={13} />
          </span>
          <span
            style={{
              fontSize: 11,
              fontWeight: 500,
              textTransform: "uppercase",
              letterSpacing: "0.08em",
              color: "var(--color-fg-tertiary)",
            }}
          >
            Final review
          </span>
        </div>
        <p
          style={{
            fontSize: 11.5,
            lineHeight: 1.5,
            color: "var(--color-fg-tertiary)",
            padding: "0 4px 4px",
            margin: 0,
          }}
        >
          Read your project end-to-end before sign-off. Filters and toggles
          control what&rsquo;s shown; nothing is editable here.
        </p>
      </div>

      {/* Contents */}
      <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        <span
          style={{
            fontSize: 11,
            fontWeight: 500,
            textTransform: "uppercase",
            letterSpacing: "0.08em",
            color: "var(--color-fg-tertiary)",
            padding: "0 4px 4px",
          }}
        >
          Contents
        </span>
        {catalogNavItems.map((nav) => (
          <a
            key={nav.catalogPath}
            href={`#catalog-${encodeURIComponent(nav.catalogPath)}`}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              padding: "6px 8px",
              borderRadius: 4,
              textDecoration: "none",
              color: "var(--color-fg-secondary)",
              fontSize: 12,
            }}
            title={`${nav.displayName} · ${nav.locale}`}
            onMouseEnter={(e) =>
              ((e.currentTarget as HTMLAnchorElement).style.background =
                "var(--color-bg-hover)")
            }
            onMouseLeave={(e) =>
              ((e.currentTarget as HTMLAnchorElement).style.background =
                "transparent")
            }
          >
            <span style={{ color: "var(--color-fg-secondary)", flexShrink: 0 }}>
              <FolderIcon size={12} />
            </span>
            <span
              style={{
                fontFamily: "monospace",
                flex: 1,
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
            >
              {nav.displayName}
              <span
                style={{
                  color: "var(--color-fg-tertiary)",
                  marginLeft: 4,
                }}
              >
                · {nav.locale}
              </span>
            </span>
            <span
              style={{
                fontFamily: "monospace",
                fontSize: 10.5,
                color: "var(--color-fg-disabled)",
              }}
            >
              {nav.total}
            </span>
            <ProgressBar
              total={nav.total}
              finished={nav.finished}
              proposed={0}
              height={4}
              width={22}
            />
          </a>
        ))}
      </div>

      {/* Show toggles */}
      <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        <span
          style={{
            fontSize: 11,
            fontWeight: 500,
            textTransform: "uppercase",
            letterSpacing: "0.08em",
            color: "var(--color-fg-tertiary)",
            padding: "0 4px 4px",
          }}
        >
          Show
        </span>
        {(
          [
            { key: "sourceIds", label: "Source IDs" },
            { key: "emptyTranslations", label: "Empty translations" },
            { key: "gateFlags", label: "Gate flags" },
            { key: "placeholders", label: "Placeholders" },
          ] as { key: keyof ShowToggles; label: string }[]
        ).map(({ key, label }) => (
          <label
            key={key}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              padding: "2px 4px",
              fontSize: 12,
              color: "var(--color-fg-secondary)",
              cursor: "pointer",
            }}
          >
            <input
              type="checkbox"
              checked={show[key]}
              onChange={() => onShowChange(key)}
              style={{ cursor: "pointer" }}
            />
            {label}
          </label>
        ))}
      </div>

      {/* Filter to chips */}
      <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        <span
          style={{
            fontSize: 11,
            fontWeight: 500,
            textTransform: "uppercase",
            letterSpacing: "0.08em",
            color: "var(--color-fg-tertiary)",
            padding: "0 4px 4px",
          }}
        >
          Filter to
        </span>
        {(
          [
            {
              key: "flagged",
              label: "Flagged",
              color: "var(--color-severity-soft)",
            },
            {
              key: "untranslated",
              label: "Untranslated",
              color: "var(--color-state-untranslated)",
            },
            {
              key: "glossary",
              label: "Glossary",
              color: "var(--color-severity-info)",
            },
          ] as { key: FilterChipKey; label: string; color: string }[]
        ).map(({ key, label, color }) => {
          const active = activeFilters.has(key);
          return (
            <button
              key={key}
              type="button"
              aria-pressed={active}
              onClick={() => onFilterToggle(key)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                padding: "5px 8px",
                fontSize: 12,
                borderRadius: 4,
                border: `1px solid ${active ? color : "var(--color-border-subtle)"}`,
                background: active ? `${color}18` : "transparent",
                cursor: "pointer",
                color: active ? color : "var(--color-fg-secondary)",
                textAlign: "left",
                width: "100%",
                transition: "all 80ms",
              }}
            >
              <span
                style={{
                  width: 6,
                  height: 6,
                  borderRadius: 999,
                  background: color,
                  flexShrink: 0,
                }}
              />
              <span style={{ flex: 1 }}>{label}</span>
            </button>
          );
        })}
      </div>

      {/* Export button */}
      <div style={{ marginTop: "auto", paddingTop: 8 }}>
        <button
          type="button"
          onClick={onExport}
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 6,
            width: "100%",
            padding: "7px 12px",
            borderRadius: 5,
            border: "1px solid var(--color-border-default)",
            background: "transparent",
            color: "var(--color-fg-secondary)",
            fontSize: 12,
            fontWeight: 500,
            cursor: "pointer",
            transition: "all 80ms",
          }}
          onMouseEnter={(e) => {
            const el = e.currentTarget as HTMLButtonElement;
            el.style.background = "var(--color-bg-hover)";
            el.style.color = "var(--color-fg-primary)";
          }}
          onMouseLeave={(e) => {
            const el = e.currentTarget as HTMLButtonElement;
            el.style.background = "transparent";
            el.style.color = "var(--color-fg-secondary)";
          }}
        >
          <ArrowRightIcon size={13} />
          Export&hellip;
        </button>
      </div>
    </aside>
  );
}

// ── Review header ─────────────────────────────────────────────────────────────

function ReviewHeader({
  summary,
  openCatalogs,
}: {
  summary: ProjectSummary;
  openCatalogs: Map<string, CatalogResponse>;
}) {
  // Derive per-locale finished counts from openCatalogs.
  const localeStats = useMemo(() => {
    return summary.locales.map((locale) => {
      const refs = summary.catalogs.filter((c) => c.locale === locale);
      let finished = 0;
      let total = 0;
      for (const ref of refs) {
        const resp = openCatalogs.get(ref.absolute_path);
        if (resp) {
          total += resp.units.length;
          finished += resp.units.filter((u) => u.state === "finished").length;
        }
      }
      return { locale, finished, total };
    });
  }, [summary, openCatalogs]);

  return (
    <div
      style={{
        flexShrink: 0,
        padding: "20px 32px 16px",
        borderBottom: "1px solid var(--color-border-subtle)",
        background: "var(--color-bg-base)",
      }}
    >
      <div style={{ display: "flex", alignItems: "flex-start", gap: 16 }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <span
            style={{
              fontSize: 11,
              fontWeight: 500,
              textTransform: "uppercase",
              letterSpacing: "0.08em",
              color: "var(--color-fg-tertiary)",
            }}
          >
            Project
          </span>
          <div
            style={{
              display: "flex",
              alignItems: "baseline",
              gap: 10,
            }}
          >
            <h1
              style={{
                margin: 0,
                fontSize: 22,
                fontWeight: 600,
                letterSpacing: "-0.015em",
                color: "var(--color-fg-primary)",
              }}
            >
              {summary.name}
            </h1>
            <span
              style={{
                fontFamily: "monospace",
                fontSize: 11.5,
                color: "var(--color-fg-tertiary)",
                padding: "1px 5px",
                borderRadius: 3,
                border: "1px solid var(--color-border-subtle)",
                background: "var(--color-bg-elevated)",
              }}
            >
              final review
            </span>
          </div>
        </div>
        <div
          style={{
            flex: 1,
            display: "flex",
            justifyContent: "flex-end",
            flexWrap: "wrap",
            gap: 6,
          }}
        >
          {localeStats.map(({ locale, finished, total }) => (
            <LocaleHeadStat
              key={locale}
              locale={locale}
              finished={finished}
              total={total}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

// ── Sign-off footer ───────────────────────────────────────────────────────────

function SignOffFooter({
  summary,
  openCatalogs,
  onMarkReviewed,
  onOpenHardFlags,
}: {
  summary: ProjectSummary;
  openCatalogs: Map<string, CatalogResponse>;
  onMarkReviewed: () => void;
  onOpenHardFlags: () => void;
}) {
  const stats = useMemo(() => {
    let totalUnits = 0;
    let finishedUnits = 0;
    let hardFlagged = 0;
    for (const [, resp] of openCatalogs) {
      totalUnits += resp.units.length;
      for (const u of resp.units) {
        if (u.state === "finished") finishedUnits++;
        if (u.flags.length > 0) hardFlagged++;
      }
    }
    return {
      catalogs: summary.catalogs.length,
      totalUnits,
      finishedUnits,
      hardFlagged,
    };
  }, [summary.catalogs.length, openCatalogs]);

  return (
    <div
      className="review-panel-signoff-footer"
      style={{
        marginTop: 24,
        padding: "16px 20px",
        border: "1px solid var(--color-border-subtle)",
        borderRadius: 8,
        background: "var(--color-bg-surface)",
        display: "flex",
        flexDirection: "column",
        gap: 10,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{ color: "var(--color-state-finished)" }}>
          <CheckIcon size={14} />
        </span>
        <span
          style={{
            fontSize: 11,
            fontWeight: 500,
            textTransform: "uppercase",
            letterSpacing: "0.08em",
            color: "var(--color-fg-tertiary)",
          }}
        >
          End of project
        </span>
      </div>
      <p
        style={{
          margin: 0,
          fontSize: 12.5,
          color: "var(--color-fg-secondary)",
          lineHeight: 1.5,
        }}
      >
        <span style={{ fontFamily: "monospace" }}>{stats.catalogs}</span>{" "}
        {stats.catalogs === 1 ? "catalog" : "catalogs"} &middot;{" "}
        <span style={{ fontFamily: "monospace" }}>{stats.totalUnits}</span>{" "}
        units &middot;{" "}
        <span style={{ fontFamily: "monospace" }}>{stats.finishedUnits}</span>{" "}
        of <span style={{ fontFamily: "monospace" }}>{stats.totalUnits}</span>{" "}
        translations finished
        {stats.hardFlagged > 0 && (
          <>
            {" "}
            &middot;{" "}
            <span
              style={{
                fontFamily: "monospace",
                color: "var(--color-severity-hard)",
              }}
            >
              {stats.hardFlagged}
            </span>{" "}
            flagged.
          </>
        )}
      </p>
      <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
        <button
          type="button"
          onClick={onMarkReviewed}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 6,
            height: 30,
            padding: "0 12px",
            borderRadius: 5,
            border: "1px solid var(--color-border-default)",
            background: "transparent",
            color: "var(--color-fg-secondary)",
            fontSize: 12,
            fontWeight: 500,
            cursor: "pointer",
            transition: "all 80ms",
          }}
          onMouseEnter={(e) => {
            const el = e.currentTarget as HTMLButtonElement;
            el.style.background = "var(--color-bg-hover)";
            el.style.color = "var(--color-fg-primary)";
            el.style.borderColor = "var(--color-border-strong)";
          }}
          onMouseLeave={(e) => {
            const el = e.currentTarget as HTMLButtonElement;
            el.style.background = "transparent";
            el.style.color = "var(--color-fg-secondary)";
            el.style.borderColor = "var(--color-border-default)";
          }}
        >
          <CheckIcon size={13} />
          Mark project reviewed
        </button>
        {stats.hardFlagged > 0 && (
          <button
            type="button"
            onClick={onOpenHardFlags}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 6,
              height: 30,
              padding: "0 12px",
              borderRadius: 5,
              border: "1px solid var(--color-border-subtle)",
              background: "transparent",
              color: "var(--color-fg-tertiary)",
              fontSize: 12,
              fontWeight: 500,
              cursor: "pointer",
              transition: "all 80ms",
            }}
            onMouseEnter={(e) => {
              const el = e.currentTarget as HTMLButtonElement;
              el.style.background = "var(--color-bg-hover)";
              el.style.color = "var(--color-fg-secondary)";
              el.style.borderColor = "var(--color-border-default)";
            }}
            onMouseLeave={(e) => {
              const el = e.currentTarget as HTMLButtonElement;
              el.style.background = "transparent";
              el.style.color = "var(--color-fg-tertiary)";
              el.style.borderColor = "var(--color-border-subtle)";
            }}
          >
            <ArrowRightIcon size={13} />
            Open remaining hard flags
          </button>
        )}
      </div>
    </div>
  );
}

// ── Export helper ─────────────────────────────────────────────────────────────

function buildMarkdown(
  rows: ProofreadUnitRow[],
  locales: string[],
  summary: ProjectSummary,
): string {
  const lines: string[] = [];
  lines.push(`# ${summary.name} — Final Review`);
  lines.push("");
  for (const row of rows) {
    lines.push(`### ${row.unitId}`);
    lines.push(`**Source:** \`${row.source}\``);
    lines.push("");
    for (const locale of locales) {
      const entry = row.byLocale.get(locale);
      let text = "—";
      if (entry) {
        const t = entry.unit.target;
        text =
          t.kind === "singular"
            ? (t.text ?? "—")
            : (t.forms.find((f) => f != null) ?? "—");
      }
      lines.push(`- **${locale}:** ${text}`);
    }
    lines.push("");
  }
  return lines.join("\n");
}

// ── ProofreadView ─────────────────────────────────────────────────────────────

export function ProofreadView({
  summary,
  openCatalogs,
  reports,
  onNavigateToUnit,
  onOpenHardFlags,
  onEnsureCatalogLoaded,
}: ProofreadViewProps) {
  const [show, setShow] = useState<ShowToggles>({
    sourceIds: false,
    emptyTranslations: true,
    gateFlags: true,
    placeholders: true,
  });
  const [activeFilters, setActiveFilters] = useState<Set<FilterChipKey>>(
    new Set(),
  );
  // The locale the user last "focused" by clicking a locale head stat card;
  // drives which locale is passed to onNavigateToUnit so the user lands in Focus.
  // setFocusedLocale will be wired to locale-card clicks in a future PR.
  const [focusedLocale] = useState<string | null>(null);

  // Fan-load every project catalog on mount so the manuscript renders with
  // real content instead of empty module sections. Mirrors MatrixView's
  // parallel-mount-once pattern (see MatrixView.tsx for the dependency
  // rationale): closing over openCatalogs in the dep list would re-trigger
  // on every cache update, which is wasted work.
  // biome-ignore lint/correctness/useExhaustiveDependencies: see comment above
  useEffect(() => {
    if (!onEnsureCatalogLoaded) return;
    const toLoad = summary.catalogs.filter(
      (ref) => !openCatalogs.has(ref.absolute_path),
    );
    if (toLoad.length === 0) return;
    void Promise.allSettled(
      toLoad.map((ref) => onEnsureCatalogLoaded(ref.absolute_path)),
    );
  }, [summary.catalogs, onEnsureCatalogLoaded]);

  const totalCatalogs = summary.catalogs.length;
  const loadedCatalogs = useMemo(
    () =>
      summary.catalogs.reduce(
        (acc, ref) => acc + (openCatalogs.has(ref.absolute_path) ? 1 : 0),
        0,
      ),
    [summary.catalogs, openCatalogs],
  );
  const allCatalogsLoaded =
    totalCatalogs === 0 || loadedCatalogs === totalCatalogs;

  const rows = useMemo(
    () => buildProofreadRows(summary, openCatalogs),
    [summary, openCatalogs],
  );

  const catalogNavItems = useMemo(
    () => buildCatalogNavItems(summary, openCatalogs),
    [summary, openCatalogs],
  );

  const handleShowChange = useCallback((key: keyof ShowToggles) => {
    setShow((prev) => ({ ...prev, [key]: !prev[key] }));
  }, []);

  const handleFilterToggle = useCallback((key: FilterChipKey) => {
    setActiveFilters((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  }, []);

  const handleUnitClick = useCallback(
    (catalogPath: string, unitId: UnitId, locale?: string) => {
      onNavigateToUnit(
        catalogPath,
        unitId,
        locale ?? focusedLocale ?? undefined,
      );
    },
    [onNavigateToUnit, focusedLocale],
  );

  const handleExport = useCallback(async () => {
    const md = buildMarkdown(rows, summary.locales, summary);
    try {
      await navigator.clipboard.writeText(md);
      // Could show a toast here, but ProofreadView has no direct toast access.
      // The action is self-evident; a future PR can wire flashInfo.
    } catch {
      // Clipboard API not available (non-secure context in some Tauri webviews).
      // Fallback: open a data URL.
      const blob = new Blob([md], { type: "text/markdown" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${summary.name}-review.md`;
      a.click();
      URL.revokeObjectURL(url);
    }
  }, [rows, summary]);

  const handleMarkReviewed = useCallback(() => {
    // No IPC for mark_project_reviewed exists yet — no-op stub.
    // A future PR will wire this to a Tauri command when the Rust side exposes it.
  }, []);

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        overflow: "hidden",
        minHeight: 0,
        background: "var(--color-bg-base)",
      }}
    >
      <SideNav
        catalogNavItems={catalogNavItems}
        show={show}
        onShowChange={handleShowChange}
        activeFilters={activeFilters}
        onFilterToggle={handleFilterToggle}
        onExport={handleExport}
      />

      <div
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
          minHeight: 0,
        }}
      >
        <ReviewHeader summary={summary} openCatalogs={openCatalogs} />

        <div
          className="review-document-scroll"
          style={{
            flex: 1,
            overflowY: "auto",
            padding: "24px 32px 48px",
          }}
        >
          <div
            className="review-document-col"
            style={{
              maxWidth: 1040,
              margin: "0 auto",
              width: "100%",
              display: "flex",
              flexDirection: "column",
              gap: 32,
            }}
          >
            {!allCatalogsLoaded ? (
              <div
                role="status"
                aria-live="polite"
                style={{
                  display: "flex",
                  flexDirection: "column",
                  alignItems: "center",
                  justifyContent: "center",
                  gap: 12,
                  padding: "64px 16px",
                  color: "var(--color-fg-tertiary)",
                }}
              >
                <span
                  aria-hidden="true"
                  className="animate-pulse"
                  style={{
                    display: "inline-block",
                    width: 10,
                    height: 10,
                    borderRadius: 999,
                    background: "var(--color-accent)",
                  }}
                />
                <p
                  style={{
                    margin: 0,
                    fontSize: 13,
                    fontWeight: 500,
                    color: "var(--color-fg-secondary)",
                  }}
                >
                  Loading project for review…
                </p>
                <p
                  style={{
                    margin: 0,
                    fontFamily: "monospace",
                    fontSize: 11,
                  }}
                >
                  {loadedCatalogs}/{totalCatalogs} catalogs ready
                </p>
              </div>
            ) : (
              <>
                {/* Anchor stubs for the side-nav "Contents" links. Until
                    multi-module grouping lands they all scroll to the top
                    of the manuscript; the IDs exist so the links don't
                    return 404 in the browser console. */}
                {catalogNavItems.map((nav) => (
                  <span
                    key={nav.catalogPath}
                    id={`catalog-${encodeURIComponent(nav.catalogPath)}`}
                    style={{ display: "block", height: 0 }}
                    aria-hidden="true"
                  />
                ))}

                <ProofreadSection
                  title={summary.name}
                  rows={rows}
                  locales={summary.locales}
                  reports={reports}
                  show={show}
                  activeFilters={activeFilters}
                  activeLocale={focusedLocale}
                  onUnitClick={handleUnitClick}
                />

                <SignOffFooter
                  summary={summary}
                  openCatalogs={openCatalogs}
                  onMarkReviewed={handleMarkReviewed}
                  onOpenHardFlags={onOpenHardFlags}
                />
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
