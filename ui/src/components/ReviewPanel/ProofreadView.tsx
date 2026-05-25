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

import { useCallback, useMemo, useState } from "react";
import type {
  CatalogRef,
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

// ── CatalogModule — groups units from one catalog ─────────────────────────────

interface CatalogModuleData {
  catalogRef: CatalogRef;
  units: Unit[];
  /** Display name derived from the manifest path basename. */
  displayName: string;
  /** Short path hint for the tertiary line under the heading. */
  pathHint: string;
}

function buildCatalogModules(
  summary: ProjectSummary,
  openCatalogs: Map<string, CatalogResponse>,
): CatalogModuleData[] {
  return summary.catalogs.map((ref) => {
    const response = openCatalogs.get(ref.absolute_path);
    const units = response?.units ?? [];
    const parts = ref.manifest_path.replace(/\\/g, "/").split("/");
    const basename = parts[parts.length - 1] ?? ref.manifest_path;
    const displayName = basename.replace(/\.[^.]+$/, ""); // strip extension
    const pathHint =
      parts.length > 2 ? `…/${parts.slice(-2).join("/")}` : ref.manifest_path;
    return { catalogRef: ref, units, displayName, pathHint };
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

function ReviewUnitRow({
  unit,
  locales,
  reports,
  show,
  activeLocale,
  onClick,
}: {
  unit: Unit;
  locales: string[];
  reports: Record<UnitId, GateReport>;
  show: ShowToggles;
  /** The locale the user has "active" in the side-nav, if any. */
  activeLocale: string | null;
  onClick: (unitId: UnitId, locale?: string) => void;
}) {
  const report = reports[unit.id];
  const hardFlagSet = new Set<string>(
    report?.findings
      .filter((f) => {
        const s = f.flag?.toLowerCase() ?? "";
        return (
          s.includes("placeholder") ||
          s.includes("plural-arity") ||
          s.includes("icu-parse") ||
          s.includes("empty-target") ||
          s.includes("backend-malformed")
        );
      })
      .map((f) => f.flag) ?? [],
  );

  // Per-locale target text helper.
  // Each CatalogResponse is locale-specific, so unit.target is the target for
  // the catalog's own locale. We ignore the locale argument here; a future
  // cross-catalog view would need the catalog map to look up by locale.
  function targetText(_locale: string): string | null {
    if (unit.target.kind === "singular") return unit.target.text;
    // For plural — surface first non-null form as preview.
    return unit.target.forms.find((f) => f != null) ?? null;
  }

  // Per-locale state for coverage dot.
  function dotState(locale: string): DotState {
    const text = targetText(locale);
    if (hardFlagSet.size > 0) return "hard";
    if (!text) return "empty";
    return "finished";
  }

  // Per-locale flags for TranslationLine (from the gate report).
  function flagsFor(): string[] {
    return report?.findings.map((f) => f.flag) ?? [];
  }

  const isPlural = unit.plural_arity != null;
  const hasPlaceholders = unit.placeholders.length > 0;
  const sourceText = unit.source;
  const unitIdText = unit.id;

  // For this unit, count "filled" = target text is non-null.
  const filledCount =
    unit.target.kind === "plural"
      ? unit.target.forms.filter((f) => f != null).length
      : unit.target.text != null
        ? 1
        : 0;
  const totalLocales = locales.length || 1;

  return (
    <button
      type="button"
      className="review-unit-row"
      style={{
        display: "flex",
        gap: 14,
        alignItems: "flex-start",
        cursor: "pointer",
        borderRadius: 4,
        padding: "6px 4px",
        transition: "background 80ms",
        background: "transparent",
        border: "none",
        width: "100%",
        textAlign: "left",
      }}
      aria-label={`Unit ${unitIdText} — click to open in Translate`}
      onClick={() => onClick(unit.id, activeLocale ?? undefined)}
      onMouseEnter={(e) =>
        ((e.currentTarget as HTMLButtonElement).style.background =
          "var(--color-bg-hover)")
      }
      onMouseLeave={(e) =>
        ((e.currentTarget as HTMLButtonElement).style.background =
          "transparent")
      }
    >
      {/* Left gutter — coverage dots */}
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
          {isPlural && show.placeholders && (
            <MicroBadge tone="neutral">plural ×{unit.plural_arity}</MicroBadge>
          )}
          {hasPlaceholders && show.placeholders && (
            <MicroBadge tone="neutral">
              {"{}"}×{unit.placeholders.length}
            </MicroBadge>
          )}
          {unit.flags.length > 0 && show.gateFlags && (
            <MicroBadge tone="soft">{unit.flags[0]}</MicroBadge>
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

        {/* Translation grid */}
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
                flags={flagsFor()}
                showGateFlags={show.gateFlags}
              />
            );
          })}
        </div>
      </div>
    </button>
  );
}

// ── Module section ────────────────────────────────────────────────────────────

function ModuleSection({
  mod,
  locales,
  reports,
  show,
  activeFilters,
  activeLocale,
  onUnitClick,
}: {
  mod: CatalogModuleData;
  locales: string[];
  reports: Record<UnitId, GateReport>;
  show: ShowToggles;
  activeFilters: Set<FilterChipKey>;
  activeLocale: string | null;
  onUnitClick: (catalogPath: string, unitId: UnitId, locale?: string) => void;
}) {
  const filteredUnits = useMemo(() => {
    return mod.units.filter((u) => {
      if (activeFilters.has("flagged") && u.flags.length === 0) return false;
      if (activeFilters.has("untranslated") && u.state !== "untranslated")
        return false;
      // Glossary filter: keep only units whose id contains glossary indicator
      // (no glossary metadata in the wire type yet; skip silently).
      return true;
    });
  }, [mod.units, activeFilters]);

  if (filteredUnits.length === 0) return null;

  const finished = mod.units.filter((u) => u.state === "finished").length;
  const total = mod.units.length;

  return (
    <section
      className="review-module-section"
      aria-label={`Catalog: ${mod.displayName}`}
    >
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
          {mod.displayName}
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
          {finished}/{total} finished
        </span>
        <span style={{ flex: 1 }} />
        <span
          style={{
            fontSize: 11,
            color: "var(--color-fg-tertiary)",
            maxWidth: 280,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
          title={mod.catalogRef.manifest_path}
        >
          {mod.pathHint} · {mod.catalogRef.locale}
        </span>
      </header>

      {mod.units.length === 0 ? (
        <p
          style={{
            fontSize: 12,
            color: "var(--color-fg-disabled)",
            fontStyle: "italic",
            padding: "4px 0",
          }}
        >
          Catalog not yet loaded — open it in Translate to view units here.
        </p>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          {filteredUnits.map((u) => (
            <ReviewUnitRow
              key={u.id}
              unit={u}
              locales={locales}
              reports={reports}
              show={show}
              activeLocale={activeLocale}
              onClick={(uid, locale) =>
                onUnitClick(mod.catalogRef.absolute_path, uid, locale)
              }
            />
          ))}
        </div>
      )}
    </section>
  );
}

// ── Side navigation ───────────────────────────────────────────────────────────

function SideNav({
  modules,
  show,
  onShowChange,
  activeFilters,
  onFilterToggle,
  onExport,
}: {
  modules: CatalogModuleData[];
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
        {modules.map((mod) => {
          const finished = mod.units.filter(
            (u) => u.state === "finished",
          ).length;
          const total = mod.units.length;
          return (
            <a
              key={mod.catalogRef.absolute_path}
              href={`#catalog-${encodeURIComponent(mod.catalogRef.absolute_path)}`}
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
              onMouseEnter={(e) =>
                ((e.currentTarget as HTMLAnchorElement).style.background =
                  "var(--color-bg-hover)")
              }
              onMouseLeave={(e) =>
                ((e.currentTarget as HTMLAnchorElement).style.background =
                  "transparent")
              }
            >
              <span
                style={{ color: "var(--color-fg-secondary)", flexShrink: 0 }}
              >
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
                {mod.displayName}
              </span>
              <span
                style={{
                  fontFamily: "monospace",
                  fontSize: 10.5,
                  color: "var(--color-fg-disabled)",
                }}
              >
                {total}
              </span>
              <ProgressBar
                total={total}
                finished={finished}
                proposed={0}
                height={4}
                width={22}
              />
            </a>
          );
        })}
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
  modules: CatalogModuleData[],
  locales: string[],
  summary: ProjectSummary,
): string {
  const lines: string[] = [];
  lines.push(`# ${summary.name} — Final Review`);
  lines.push("");
  for (const mod of modules) {
    lines.push(`## ${mod.displayName}`);
    lines.push(`_${mod.pathHint}_`);
    lines.push("");
    for (const u of mod.units) {
      lines.push(`### ${u.id}`);
      lines.push(`**Source:** \`${u.source}\``);
      lines.push("");
      for (const locale of locales) {
        const text =
          u.target.kind === "singular"
            ? (u.target.text ?? "—")
            : (u.target.forms.find((f) => f != null) ?? "—");
        lines.push(`- **${locale}:** ${text}`);
      }
      lines.push("");
    }
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

  const modules = useMemo(
    () => buildCatalogModules(summary, openCatalogs),
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
    const md = buildMarkdown(modules, summary.locales, summary);
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
  }, [modules, summary]);

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
        modules={modules}
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
            {modules.map((mod) => (
              <div
                key={mod.catalogRef.absolute_path}
                id={`catalog-${encodeURIComponent(mod.catalogRef.absolute_path)}`}
              >
                <ModuleSection
                  mod={mod}
                  locales={summary.locales}
                  reports={reports}
                  show={show}
                  activeFilters={activeFilters}
                  activeLocale={focusedLocale}
                  onUnitClick={handleUnitClick}
                />
              </div>
            ))}

            <SignOffFooter
              summary={summary}
              openCatalogs={openCatalogs}
              onMarkReviewed={handleMarkReviewed}
              onOpenHardFlags={onOpenHardFlags}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
