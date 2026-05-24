import { cn } from "../../lib/cn";
import type {
  AnyFlag,
  Finding,
  GateReport,
  ModelFlag,
  Unit,
} from "../../lib/types";
import { humanizeFlag, severityOf } from "../../lib/types";

interface Props {
  unit: Unit;
  report: GateReport | null;
  activeCatalogPath: string | null;
  busyIds: Set<string>;
  onAccept: (unitId: string) => Promise<void>;
}

// Narrows a generic flag string to ModelFlag (the enumerated semantic variants).
// Returns null for gate-produced flags that are not model-supplied.
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

export function Inspector({
  unit,
  report,
  activeCatalogPath,
  busyIds,
  onAccept,
}: Props) {
  const isPlural = unit.plural_arity != null;
  const placeholders = unit.placeholders ?? [];
  const placeholderCount = Array.isArray(placeholders)
    ? placeholders.length
    : 0;
  const provenance = unit.provenance;
  const findings = report?.findings ?? [];

  // Model-supplied flags from Unit.flags (semantic subset).
  const modelFlags: ModelFlag[] = Array.isArray(unit.flags)
    ? unit.flags.map(asModelFlag).filter((f): f is ModelFlag => f !== null)
    : [];
  const flagNotes: Record<string, string> = unit.flag_notes ?? {};

  const confidence = unit.confidence ?? null;
  const reviewStatus = unit.review_status ?? null;
  const busy = busyIds.has(unit.id);

  const hasFlags = modelFlags.length > 0;

  return (
    <aside className="app-chrome shrink-0 w-80 min-w-[240px] flex flex-col overflow-hidden bg-bg-surface border-l border-border-subtle">
      <header className="px-4 py-3 border-b border-border-subtle flex items-center justify-between gap-2">
        <span className="text-xs font-semibold uppercase tracking-loose text-fg-tertiary">
          Inspector
        </span>
        {/* Review-status chip — only rendered for "needs-review" and "reviewed" */}
        {reviewStatus === "needs-review" && (
          <span
            className="inline-flex items-center h-[18px] px-2 rounded-pill border border-severity-soft-border bg-severity-soft-bg text-severity-soft text-xs font-medium uppercase tracking-loose whitespace-nowrap"
            title="This unit has been flagged for human review"
          >
            Needs review
          </span>
        )}
        {reviewStatus === "reviewed" && (
          <span
            className="inline-flex items-center h-[18px] px-2 rounded-pill border border-state-finished-border bg-state-finished-bg text-state-finished text-xs font-medium uppercase tracking-loose whitespace-nowrap"
            title="This unit has been reviewed and accepted"
          >
            Reviewed
          </span>
        )}
      </header>

      <dl className="m-0 px-4 py-3 flex flex-col gap-3 border-b border-border-subtle">
        <Item label="State">
          <CodeText>{unit.state}</CodeText>
        </Item>

        <Item label="Plural arity">
          {isPlural ? (
            <CodeText>{unit.plural_arity}&nbsp;forms</CodeText>
          ) : (
            <Muted>singular</Muted>
          )}
        </Item>

        <Item label="Placeholders">
          {placeholderCount > 0 ? (
            <CodeText>{placeholderCount}</CodeText>
          ) : (
            <Muted>none</Muted>
          )}
        </Item>

        <Item label="Source location">
          {provenance.file ? (
            <CodeText>
              {provenance.file}
              {provenance.line ? `:${provenance.line}` : ""}
            </CodeText>
          ) : (
            <Muted>—</Muted>
          )}
        </Item>

        {/* Confidence indicator — rendered only when the backend reported one */}
        {confidence !== null && (
          <Item label="Confidence">
            <ConfidenceBar value={confidence} />
          </Item>
        )}
      </dl>

      {/* Model flags section — shown when the unit carries model-supplied flags */}
      {hasFlags && (
        <div className="px-4 py-3 border-b border-border-subtle flex flex-col gap-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-semibold uppercase tracking-loose text-fg-tertiary">
              Model flags
            </span>
            <span className="text-xs font-mono text-fg-tertiary tabular-nums">
              {modelFlags.length}
            </span>
          </div>
          <ul className="flex flex-col gap-2" aria-label="Model flags">
            {modelFlags.map((flag) => (
              <ModelFlagChip
                key={flag}
                flag={flag}
                note={flagNotes[flag] ?? null}
              />
            ))}
          </ul>
          {/* Accept button — visible only when flags are present */}
          <button
            type="button"
            disabled={busy || !activeCatalogPath}
            onClick={() => void onAccept(unit.id)}
            aria-label="Accept translation and mark as reviewed"
            className={cn(
              "mt-1 w-full h-7 px-3 rounded-md border text-xs font-medium",
              "transition-colors duration-100 ease-out",
              "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
              busy || !activeCatalogPath
                ? "opacity-40 cursor-not-allowed border-border-subtle text-fg-tertiary bg-transparent"
                : "border-border-default text-fg-primary bg-bg-elevated hover:bg-bg-hover hover:border-border-strong active:bg-bg-selected",
            )}
          >
            {busy ? "Accepting…" : "Accept"}
          </button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto px-4 py-3 flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <span className="text-xs font-semibold uppercase tracking-loose text-fg-tertiary">
            Findings
          </span>
          {findings.length > 0 && (
            <span className="text-xs font-mono text-fg-tertiary tabular-nums">
              {findings.length}
            </span>
          )}
        </div>
        {findings.length === 0 ? (
          <Muted>
            {report
              ? "Gate clean — no findings on this unit."
              : "No translation run yet. Findings appear after Translate."}
          </Muted>
        ) : (
          <ul className="flex flex-col gap-2">
            {findings.map((f, i) => (
              <FindingRow key={i} finding={f} />
            ))}
          </ul>
        )}
      </div>
    </aside>
  );
}

// ── Sub-components ─────────────────────────────────────────────────────────────

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
        className={cn("font-mono text-sm tabular-nums", colorClass)}
        title={`Model confidence: ${pct}%`}
      >
        {pct}%
      </span>
      <span
        className="flex-1 h-1.5 rounded-pill bg-bg-elevated overflow-hidden"
        role="img"
        aria-label={`Confidence bar: ${pct}%`}
      >
        <span
          className={cn("block h-full rounded-pill", barColor)}
          style={{ width: `${pct}%` }}
        />
      </span>
    </span>
  );
}

function ModelFlagChip({
  flag,
  note,
}: {
  flag: ModelFlag;
  note: string | null;
}) {
  return (
    <li className="px-3 py-2 rounded-md border bg-severity-info-bg border-border-default">
      <div className="flex items-center justify-between gap-2 mb-px">
        <span className="font-medium text-xs text-severity-info" title={flag}>
          {humanizeFlag(flag)}
        </span>
        <span className="text-[10px] font-semibold uppercase tracking-loose text-fg-tertiary">
          semantic
        </span>
      </div>
      {note && (
        <p className="mt-1 text-xs italic text-fg-tertiary leading-snug">
          {note}
        </p>
      )}
    </li>
  );
}

function FindingRow({ finding }: { finding: Finding }) {
  const sev = severityOf(finding.flag);
  return (
    <li
      className={cn(
        "px-3 py-2 rounded-md border",
        sev === "hard" && "bg-severity-hard-bg border-severity-hard-border",
        sev === "soft" && "bg-severity-soft-bg border-state-proposed-border",
        sev === "semantic" && "bg-severity-info-bg border-border-default",
      )}
    >
      <div className="flex items-center justify-between gap-2 mb-1">
        <span
          className={cn(
            "font-mono text-xs",
            sev === "hard" && "text-severity-hard",
            sev === "soft" && "text-severity-soft",
            sev === "semantic" && "text-severity-info",
          )}
        >
          {finding.flag}
        </span>
        <span className="text-[10px] font-semibold uppercase tracking-loose text-fg-tertiary">
          {sev}
        </span>
      </div>
      <div className="text-xs text-fg-secondary leading-snug">
        {summarizeDetail(finding.detail)}
      </div>
    </li>
  );
}

function summarizeDetail(detail: Finding["detail"]): string {
  // Best-effort one-line summary. Field names mirror the Rust
  // `*Detail` structs in `crates/gate/src/report.rs`. When a new rule
  // lands there, add a case here.
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
    // M4.6.1: backend returned a non-parseable response. The detail carries
    // a `reason` field from `BackendMalformedResponseDetail`.
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
    parts.push(`missing ${noun}(s): ${missing.map(quote).join(", ")}`);
  }
  if (extra.length > 0) {
    parts.push(`extra ${noun}(s): ${extra.map(quote).join(", ")}`);
  }
  if (parts.length === 0) return "mismatch";
  return parts.join("; ");
}

function quote(s: string): string {
  return `"${s}"`;
}

function Item({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-[110px_1fr] gap-3 items-baseline">
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
  return <span className="text-fg-tertiary">{children}</span>;
}
