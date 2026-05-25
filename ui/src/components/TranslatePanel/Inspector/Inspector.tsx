// Inspector — right-panel context sidebar, shared between MatrixView and
// FocusView. Refactored into four collapsible sections:
//
//   1. Gate findings    — soft-tinted finding cards; colleague-notes aesthetic
//   2. Glossary hits    — matching terms for the active locale (muted when empty)
//   3. Metadata         — key/value rows (state, arity, placeholders, location,
//                         backend, confidence)
//   4. History          — unit correction-log entries (collapsed by default)
//
// Optional `peerLocales` section renders only in Focus mode when the caller
// provides peer locale data.

import { useState } from "react";
import { cn } from "../../../lib/cn";
import type {
  AnyFlag,
  Finding,
  GateReport,
  ModelFlag,
  Unit,
  UnitState,
} from "../../../lib/types";
import { humanizeFlag, severityOf } from "../../../lib/types";

// ── Peer locale shape (Focus mode only) ────────────────────────────────────

export interface PeerLocaleEntry {
  locale: string;
  text: string | null;
  state: UnitState;
}

// ── Props ──────────────────────────────────────────────────────────────────

interface Props {
  unit: Unit;
  report: GateReport | null;
  activeCatalogPath: string | null;
  busyIds: Set<string>;
  onAccept: (unitId: string) => Promise<void>;
  /** Focus-mode-only: peer locale translations for the selected unit. */
  peerLocales?: PeerLocaleEntry[];
  /** Called when the user clicks a peer locale chip to jump to it. */
  onJumpToLocale?: (locale: string) => void;
  /** Override width (default 320px / w-80). FocusView passes 260. */
  width?: number | string;
  /** When true, render only a thin 36px vertical strip with an "Inspector"
   *  label + expand button. Hosting view owns the state. */
  collapsed?: boolean;
  /** Toggle the collapsed state. Required when `collapsed` is wired. */
  onToggleCollapsed?: () => void;
}

// ── Helpers ────────────────────────────────────────────────────────────────

function asModelFlag(flag: AnyFlag): ModelFlag | null {
  const MODEL_FLAGS: readonly ModelFlag[] = [
    "ambiguous-source",
    "idiom",
    "insufficient-context",
    "low-confidence",
    "brand-term",
    "tone-mismatch",
  ];
  return (MODEL_FLAGS as readonly string[]).includes(flag)
    ? (flag as ModelFlag)
    : null;
}

const DOT_COLOR: Record<UnitState, string> = {
  untranslated: "var(--color-state-untranslated)",
  proposed: "var(--color-state-proposed)",
  finished: "var(--color-state-finished)",
  vanished: "var(--color-state-vanished)",
  obsolete: "var(--color-state-vanished)",
};

// ── Component ──────────────────────────────────────────────────────────────

export function Inspector({
  unit,
  report,
  activeCatalogPath,
  busyIds,
  onAccept,
  peerLocales,
  onJumpToLocale,
  width = 320,
  collapsed = false,
  onToggleCollapsed,
}: Props) {
  // All hooks must be called unconditionally, BEFORE any early return — the
  // collapsed-strip branch below would otherwise change the hook call order
  // between renders and break React's reconciliation.
  const [gateOpen, setGateOpen] = useState(true);
  const [glossOpen, setGlossOpen] = useState(true);
  const [metaOpen, setMetaOpen] = useState(true);
  const [histOpen, setHistOpen] = useState(false);

  // ── Collapsed strip ──────────────────────────────────────────────────
  if (collapsed) {
    return (
      <aside
        className="app-chrome shrink-0 flex flex-col items-center bg-bg-surface border-l border-border-subtle"
        style={{ width: 36 }}
        aria-label="Unit inspector (collapsed)"
      >
        <button
          type="button"
          onClick={onToggleCollapsed}
          aria-label="Expand inspector"
          title="Expand inspector"
          className={cn(
            "w-full flex items-center justify-center py-3",
            "text-fg-tertiary hover:bg-bg-hover hover:text-fg-secondary",
            "transition-colors duration-100",
            "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-accent",
          )}
        >
          <ChevronLeftIcon />
        </button>
        <button
          type="button"
          onClick={onToggleCollapsed}
          aria-label="Expand inspector"
          title="Expand inspector"
          className={cn(
            "flex-1 w-full flex items-start justify-center pt-2",
            "text-fg-tertiary hover:bg-bg-hover hover:text-fg-secondary",
            "transition-colors duration-100",
            "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-accent",
          )}
        >
          <span
            className="text-xs font-semibold uppercase tracking-loose"
            style={{
              writingMode: "vertical-rl",
              transform: "rotate(180deg)",
              letterSpacing: "0.12em",
            }}
          >
            Inspector
          </span>
        </button>
      </aside>
    );
  }

  const isPlural = unit.plural_arity != null;
  const placeholders = Array.isArray(unit.placeholders)
    ? unit.placeholders.length
    : 0;
  const provenance = unit.provenance;
  const findings = report?.findings ?? [];
  const confidence = unit.confidence ?? null;
  const busy = busyIds.has(unit.id);

  const modelFlags: ModelFlag[] = Array.isArray(unit.flags)
    ? unit.flags.map(asModelFlag).filter((f): f is ModelFlag => f !== null)
    : [];
  const flagNotes: Record<string, string> = unit.flag_notes ?? {};

  const hasPeerLocales = peerLocales && peerLocales.length > 0;

  return (
    <aside
      className="app-chrome shrink-0 flex flex-col overflow-hidden bg-bg-surface border-l border-border-subtle"
      style={{ width }}
      aria-label="Unit inspector"
    >
      {/* Header */}
      <header className="px-4 py-3 border-b border-border-subtle flex items-center justify-between gap-2 shrink-0">
        <span className="text-xs font-semibold uppercase tracking-loose text-fg-tertiary">
          Inspector
        </span>
        <div className="flex items-center gap-1.5">
          {unit.review_status === "needs-review" && (
            <ReviewStatusChip label="Needs review" tone="soft" />
          )}
          {unit.review_status === "reviewed" && (
            <ReviewStatusChip label="Reviewed" tone="finished" />
          )}
          {onToggleCollapsed && (
            <button
              type="button"
              onClick={onToggleCollapsed}
              aria-label="Collapse inspector"
              title="Collapse inspector"
              className={cn(
                "inline-flex items-center justify-center w-6 h-6 rounded-sm",
                "text-fg-tertiary hover:bg-bg-hover hover:text-fg-secondary",
                "transition-colors duration-100",
                "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
              )}
            >
              <ChevronRightIcon />
            </button>
          )}
        </div>
      </header>

      <div className="flex-1 overflow-y-auto">
        {/* ── 1. Gate findings ─────────────────────────────────────────────── */}
        <CollapsibleSection
          title="Gate findings"
          count={findings.length + modelFlags.length}
          open={gateOpen}
          onToggle={() => setGateOpen((v) => !v)}
        >
          {findings.length === 0 && modelFlags.length === 0 ? (
            <Muted>
              {report
                ? "Gate clean — no findings on this unit."
                : "No translation run yet. Findings appear after Translate."}
            </Muted>
          ) : (
            <ul className="flex flex-col gap-2" aria-label="Gate findings">
              {modelFlags.map((flag) => (
                <ModelFlagCard
                  key={flag}
                  flag={flag}
                  note={flagNotes[flag] ?? null}
                />
              ))}
              {findings.map((f, i) => (
                <FindingCard key={i} finding={f} />
              ))}
            </ul>
          )}
          {/* Accept button — shown when there are flags */}
          {(findings.length > 0 || modelFlags.length > 0) && (
            <button
              type="button"
              disabled={busy || !activeCatalogPath}
              onClick={() => void onAccept(unit.id)}
              aria-label="Accept translation and mark as reviewed"
              className={cn(
                "mt-2 w-full h-7 px-3 rounded-md border text-xs font-medium",
                "transition-colors duration-100 ease-out",
                "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
                busy || !activeCatalogPath
                  ? "opacity-40 cursor-not-allowed border-border-subtle text-fg-tertiary bg-transparent"
                  : "border-border-default text-fg-primary bg-bg-elevated hover:bg-bg-hover hover:border-border-strong active:bg-bg-selected",
              )}
            >
              {busy ? "Accepting…" : "Accept"}
            </button>
          )}
        </CollapsibleSection>

        {/* ── 2. Glossary hits ─────────────────────────────────────────────── */}
        <CollapsibleSection
          title="Glossary hits"
          open={glossOpen}
          onToggle={() => setGlossOpen((v) => !v)}
        >
          <Muted>No glossary terms in this unit.</Muted>
        </CollapsibleSection>

        {/* ── 3. Metadata ──────────────────────────────────────────────────── */}
        <CollapsibleSection
          title="Metadata"
          open={metaOpen}
          onToggle={() => setMetaOpen((v) => !v)}
        >
          <dl className="m-0 flex flex-col gap-2">
            <MetaItem label="State">
              <CodeText>{unit.state}</CodeText>
            </MetaItem>

            <MetaItem label="Plural arity">
              {isPlural ? (
                <CodeText>{unit.plural_arity}&nbsp;forms</CodeText>
              ) : (
                <Muted>singular</Muted>
              )}
            </MetaItem>

            <MetaItem label="Placeholders">
              {placeholders > 0 ? (
                <CodeText>{placeholders}</CodeText>
              ) : (
                <Muted>none</Muted>
              )}
            </MetaItem>

            <MetaItem label="Source location">
              {provenance.file ? (
                <CodeText>
                  {provenance.file}
                  {provenance.line ? `:${provenance.line}` : ""}
                </CodeText>
              ) : (
                <Muted>—</Muted>
              )}
            </MetaItem>

            {confidence !== null && (
              <MetaItem label="Confidence">
                <ConfidenceBar value={confidence} />
              </MetaItem>
            )}
          </dl>
        </CollapsibleSection>

        {/* ── 4. History (collapsed by default) ───────────────────────────── */}
        <CollapsibleSection
          title="History"
          open={histOpen}
          onToggle={() => setHistOpen((v) => !v)}
        >
          <Muted>No correction log entries for this unit.</Muted>
        </CollapsibleSection>

        {/* ── Peer locales (Focus mode only) ───────────────────────────────── */}
        {hasPeerLocales && onJumpToLocale && (
          <CollapsibleSection
            title="Other locales"
            open={true}
            onToggle={() => {}}
          >
            <ul
              className="flex flex-col gap-1"
              aria-label="Peer locale translations"
            >
              {peerLocales?.map((p) => (
                <li key={p.locale}>
                  <button
                    type="button"
                    onClick={() => onJumpToLocale(p.locale)}
                    title={`Jump to ${p.locale} focus`}
                    className={cn(
                      "w-full flex items-center gap-2 text-left py-1 px-1 rounded",
                      "hover:bg-bg-hover transition-colors duration-100",
                      "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
                    )}
                  >
                    <span
                      style={{
                        display: "inline-block",
                        fontFamily: "var(--font-mono)",
                        fontSize: "11px",
                        letterSpacing: "0.04em",
                        lineHeight: 1,
                        padding: "2px 6px",
                        borderRadius: 4,
                        whiteSpace: "nowrap",
                        background: "var(--color-bg-elevated)",
                        border: "1px solid var(--color-border-default)",
                        color: "var(--color-fg-secondary)",
                        flexShrink: 0,
                        minWidth: 52,
                        textAlign: "center",
                      }}
                    >
                      {p.locale}
                    </span>
                    <span
                      style={{
                        display: "inline-block",
                        width: 6,
                        height: 6,
                        borderRadius: 999,
                        background: DOT_COLOR[p.state],
                        flexShrink: 0,
                      }}
                      aria-hidden="true"
                    />
                    <span className="font-mono text-[11.5px] text-fg-primary truncate flex-1">
                      {p.text ?? (
                        <span className="italic text-fg-tertiary">
                          untranslated
                        </span>
                      )}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </CollapsibleSection>
        )}
      </div>
    </aside>
  );
}

// ── Collapsible section wrapper ────────────────────────────────────────────

function CollapsibleSection({
  title,
  count,
  open,
  onToggle,
  children,
}: {
  title: string;
  count?: number;
  open: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}) {
  return (
    <div className="border-b border-border-subtle">
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={open}
        className={cn(
          "w-full flex items-center gap-2 px-4 py-2.5 text-left",
          "hover:bg-bg-hover transition-colors duration-100",
          "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-accent",
        )}
      >
        <span className="flex-1 text-xs font-semibold uppercase tracking-loose text-fg-tertiary">
          {title}
        </span>
        {count !== undefined && count > 0 && (
          <span className="font-mono text-[11px] text-fg-tertiary tabular-nums">
            {count}
          </span>
        )}
        <ChevronIcon open={open} />
      </button>
      {open && <div className="px-4 pb-3 flex flex-col gap-2">{children}</div>}
    </div>
  );
}

function ChevronIcon({ open }: { open: boolean }) {
  return (
    <svg
      width={12}
      height={12}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      style={{
        flexShrink: 0,
        transform: open ? "rotate(180deg)" : "none",
        transition: "transform 150ms ease",
        color: "var(--color-fg-tertiary)",
      }}
    >
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

// ── Finding card (gate-produced) ───────────────────────────────────────────

function FindingCard({ finding }: { finding: Finding }) {
  const sev = severityOf(finding.flag);
  const summary = summarizeDetail(finding.detail);

  // Severity icon character — inline SVG alternatives available but text
  // characters keep the density down and match the "colleague notes" tone.
  const SevIcon =
    sev === "hard" ? HardIcon : sev === "soft" ? SoftIcon : InfoIcon;

  return (
    <li
      className={cn(
        "flex flex-col gap-1.5 px-3 py-2.5 rounded-md border",
        sev === "hard" && "bg-severity-hard-bg border-severity-hard-border",
        sev === "soft" && "bg-severity-soft-bg border-state-proposed-border",
        sev === "semantic" && "bg-severity-info-bg border-border-default",
      )}
    >
      <div className="flex items-center gap-1.5">
        <SevIcon sev={sev} />
        <span
          className={cn(
            "font-mono leading-none",
            sev === "hard" && "text-severity-hard",
            sev === "soft" && "text-severity-soft",
            sev === "semantic" && "text-severity-info",
          )}
          style={{ fontSize: 10.5 }}
        >
          {finding.flag}
        </span>
        <span className="ml-auto text-[10px] font-semibold uppercase tracking-loose text-fg-tertiary">
          {sev}
        </span>
      </div>
      <p
        className="m-0 leading-snug text-fg-secondary"
        style={{ fontSize: 12 }}
      >
        {summary}
      </p>
    </li>
  );
}

// ── Model-flag card (semantic flags from the model) ────────────────────────

function ModelFlagCard({
  flag,
  note,
}: {
  flag: ModelFlag;
  note: string | null;
}) {
  return (
    <li className="flex flex-col gap-1.5 px-3 py-2.5 rounded-md border bg-severity-info-bg border-border-default">
      <div className="flex items-center gap-1.5">
        <InfoIcon sev="semantic" />
        <span
          className="font-medium text-severity-info"
          style={{ fontSize: 11 }}
        >
          {humanizeFlag(flag)}
        </span>
        <span className="ml-auto text-[10px] font-semibold uppercase tracking-loose text-fg-tertiary">
          semantic
        </span>
      </div>
      {note && (
        <p className="m-0 text-xs italic text-fg-tertiary leading-snug">
          {note}
        </p>
      )}
    </li>
  );
}

// ── Small severity icons ────────────────────────────────────────────────────

function HardIcon({ sev }: { sev: string }) {
  return (
    <svg
      width={13}
      height={13}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={cn(
        "shrink-0",
        sev === "hard"
          ? "text-severity-hard"
          : sev === "soft"
            ? "text-severity-soft"
            : "text-severity-info",
      )}
    >
      <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" />
      <path d="M12 9v4" />
      <path d="M12 17h.01" />
    </svg>
  );
}

function SoftIcon({ sev }: { sev: string }) {
  return (
    <svg
      width={13}
      height={13}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={cn(
        "shrink-0",
        sev === "hard"
          ? "text-severity-hard"
          : sev === "soft"
            ? "text-severity-soft"
            : "text-severity-info",
      )}
    >
      <circle cx={12} cy={12} r={10} />
      <path d="M12 16v-4" />
      <path d="M12 8h.01" />
    </svg>
  );
}

function InfoIcon({ sev }: { sev: string }) {
  return (
    <svg
      width={13}
      height={13}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={cn(
        "shrink-0",
        sev === "hard"
          ? "text-severity-hard"
          : sev === "soft"
            ? "text-severity-soft"
            : "text-severity-info",
      )}
    >
      <circle cx={12} cy={12} r={10} />
      <path d="M12 16v-4" />
      <path d="M12 8h.01" />
    </svg>
  );
}

// ── Confidence bar ────────────────────────────────────────────────────────

function ConfidenceBar({ value }: { value: number }) {
  const pct = Math.round(value * 100);
  const colorClass =
    value < 0.5
      ? "text-severity-soft"
      : value > 0.85
        ? "text-state-finished"
        : "text-fg-secondary";
  const barColor =
    value < 0.5
      ? "bg-severity-soft"
      : value > 0.85
        ? "bg-state-finished"
        : "bg-accent";

  return (
    <span className="flex items-center gap-2">
      <span
        className={cn("font-mono text-sm tabular-nums shrink-0", colorClass)}
        title={`Model confidence: ${pct}%`}
        style={{ width: 32 }}
      >
        {pct}%
      </span>
      <span
        className="h-1.5 rounded-pill bg-bg-elevated overflow-hidden"
        role="img"
        aria-label={`Confidence bar: ${pct}%`}
        style={{ width: 80, display: "block" }}
      >
        <span
          className={cn("block h-full rounded-pill", barColor)}
          style={{ width: `${pct}%` }}
        />
      </span>
    </span>
  );
}

// ── Review status chip ────────────────────────────────────────────────────

function ReviewStatusChip({
  label,
  tone,
}: {
  label: string;
  tone: "soft" | "finished";
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center h-[18px] px-2 rounded-pill border",
        "text-xs font-medium uppercase tracking-loose whitespace-nowrap",
        tone === "soft" &&
          "border-severity-soft-border bg-severity-soft-bg text-severity-soft",
        tone === "finished" &&
          "border-state-finished-border bg-state-finished-bg text-state-finished",
      )}
      title={label}
    >
      {label}
    </span>
  );
}

// ── Shared sub-components ─────────────────────────────────────────────────

function MetaItem({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-[110px_1fr] gap-2 items-baseline">
      <dt className="m-0 text-xs uppercase tracking-loose text-fg-tertiary">
        {label}
      </dt>
      <dd className="m-0 text-sm text-fg-secondary [overflow-wrap:anywhere]">
        {children}
      </dd>
    </div>
  );
}

function CodeText({ children }: { children: React.ReactNode }) {
  return <span className="font-mono text-sm text-fg-primary">{children}</span>;
}

function Muted({ children }: { children: React.ReactNode }) {
  return <span className="text-xs text-fg-tertiary">{children}</span>;
}

// ── Finding detail → human-readable summary ───────────────────────────────

function summarizeDetail(detail: Finding["detail"]): string {
  const { rule, ...rest } = detail;
  switch (rule) {
    case "placeholder-mismatch":
      return formatMissingExtra(rest);
    case "plural-arity-mismatch":
      return `expected ${rest.expected ?? "?"} forms, found ${rest.found ?? "?"}${
        rest.wrong_variant ? " (singular/plural variant wrong)" : ""
      }`;
    case "icu-parse-error":
      return String(rest.message ?? "ICU parse error");
    case "empty-target-when-finished":
      return `slot ${rest.slot ?? 0}: state is Finished but the target is empty.`;
    case "accel-mismatch":
      return `source has ${rest.source_count ?? "?"} accel marker(s); target has ${
        rest.target_count ?? "?"
      }.`;
    case "length-warn": {
      const ratio =
        typeof rest.ratio === "number" ? rest.ratio.toFixed(2) : "?";
      const threshold =
        typeof rest.threshold === "number" ? rest.threshold.toFixed(2) : "?";
      return `target is ${ratio}× the source length (threshold ${threshold}×).`;
    }
    case "markup-tag-mismatch":
      return formatMissingExtra(rest, "tag");
    case "placeholder-agreement-risk":
      return `placeholder ${rest.placeholder ?? ""} follows determiner "${
        rest.determiner ?? ""
      }".`;
    case "cjk-punctuation-tolerated": {
      const chars = Array.isArray(rest.characters) ? rest.characters : [];
      return chars.length > 0
        ? `ASCII punctuation where CJK convention is full-width: ${chars.join(" ")}`
        : "CJK script with ASCII punctuation.";
    }
    case "backend-malformed-response": {
      const reason = typeof rest.reason === "string" ? rest.reason : "";
      return reason
        ? `Malformed response from backend: ${reason}`
        : "Backend returned a response that could not be parsed. Check the prompt template.";
    }
    default:
      return rule;
  }
}

function formatMissingExtra(
  rest: Record<string, unknown>,
  noun: string = "placeholder",
): string {
  const missing = Array.isArray(rest.missing) ? (rest.missing as string[]) : [];
  const extra = Array.isArray(rest.extra) ? (rest.extra as string[]) : [];
  const parts: string[] = [];
  if (missing.length > 0) {
    parts.push(`missing ${noun}(s): ${missing.map(q).join(", ")}`);
  }
  if (extra.length > 0) {
    parts.push(`extra ${noun}(s): ${extra.map(q).join(", ")}`);
  }
  if (parts.length === 0) return "mismatch";
  return parts.join("; ");
}

function q(s: string): string {
  return `"${s}"`;
}

function ChevronRightIcon() {
  return (
    <svg
      width={14}
      height={14}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="m9 18 6-6-6-6" />
    </svg>
  );
}

function ChevronLeftIcon() {
  return (
    <svg
      width={14}
      height={14}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="m15 18-6-6 6-6" />
    </svg>
  );
}
