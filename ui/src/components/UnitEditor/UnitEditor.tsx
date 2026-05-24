import { useMemo, useState } from "react";
import { cn } from "../../lib/cn";
import { type Token, tokenize } from "../../lib/highlight";
import type { Unit } from "../../lib/types";
import { StateBadge } from "../StateBadge/StateBadge";

interface Props {
  unit: Unit;
}

export function UnitEditor({ unit }: Props) {
  const pluralTarget = unit.target.kind === "plural" ? unit.target : null;
  const isPlural = pluralTarget !== null && pluralTarget.forms.length > 1;
  const formCount = pluralTarget?.forms.length ?? 1;
  const [activeForm, setActiveForm] = useState(0);
  const safeForm = Math.min(activeForm, Math.max(0, formCount - 1));

  const targetText = useMemo(() => {
    if (pluralTarget) return pluralTarget.forms[safeForm] ?? null;
    if (unit.target.kind === "singular") return unit.target.text;
    return null;
  }, [pluralTarget, safeForm, unit.target]);

  const provenance = formatProvenance(unit);

  return (
    <section className="flex-1 flex flex-col overflow-hidden min-w-0 bg-bg-base">
      <header className="shrink-0 flex items-center justify-between gap-4 px-5 py-3 border-b border-border-subtle bg-bg-surface">
        <div className="flex-1 min-w-0">
          <span
            className="block font-mono text-sm text-fg-primary truncate select-text"
            title={unit.id}
          >
            {unit.id}
          </span>
        </div>
        <div className="shrink-0 flex items-center gap-3">
          <StateBadge state={unit.state} />
          {provenance && (
            <span className="font-mono text-xs text-fg-tertiary">
              {provenance}
            </span>
          )}
        </div>
      </header>

      <div className="flex-1 overflow-y-auto p-5 flex flex-col gap-5">
        <Section label="Source">
          <TextBlock value={unit.source} />
        </Section>

        <Section
          label="Target"
          right={
            isPlural && pluralTarget ? (
              <PluralTabs
                target={pluralTarget}
                active={safeForm}
                onChange={setActiveForm}
              />
            ) : null
          }
        >
          {targetText == null || targetText.length === 0 ? (
            <EmptyTextBlock />
          ) : (
            <TextBlock value={targetText} />
          )}
        </Section>
      </div>

      <footer className="shrink-0 px-5 py-3 border-t border-border-subtle bg-bg-surface">
        <div className="text-xs text-fg-tertiary">
          Editing &amp; save arrive in the next milestone. This view is
          read-only.
        </div>
      </footer>
    </section>
  );
}

function Section({
  label,
  right,
  children,
}: {
  label: string;
  right?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2">
      <div className="min-h-[22px] flex items-center justify-between gap-3">
        <span className="text-xs font-semibold uppercase tracking-loose text-fg-tertiary">
          {label}
        </span>
        {right}
      </div>
      {children}
    </div>
  );
}

function TextBlock({ value }: { value: string }) {
  const tokens = tokenize(value);
  return (
    <div
      className={cn(
        "p-4 rounded-md border border-border-subtle bg-bg-surface",
        "font-mono text-md leading-[1.65] text-fg-primary",
        "whitespace-pre-wrap break-words select-text",
      )}
    >
      {tokens.map((t, i) => (
        <TokenSpan key={i} token={t} />
      ))}
    </div>
  );
}

function EmptyTextBlock() {
  return (
    <div
      className={cn(
        "p-4 rounded-md border border-border-subtle bg-bg-surface",
        "font-sans text-md leading-[1.65] text-fg-disabled italic",
      )}
    >
      No target text yet.
    </div>
  );
}

function TokenSpan({ token }: { token: Token }) {
  if (token.kind === "placeholder") {
    return (
      <span
        title="Placeholder"
        className={cn(
          "inline mx-px px-1 py-px rounded-sm border",
          "font-mono text-[0.94em] text-accent-hover",
          "bg-accent-subtle border-accent-subtle-border",
        )}
      >
        {token.value}
      </span>
    );
  }
  if (token.kind === "accel") {
    return (
      <span
        title="Accelerator marker"
        className="inline text-state-proposed font-semibold"
      >
        {token.value}
      </span>
    );
  }
  return <>{token.value}</>;
}

function PluralTabs({
  target,
  active,
  onChange,
}: {
  target: { kind: "plural"; forms: (string | null)[] };
  active: number;
  onChange: (i: number) => void;
}) {
  return (
    <div role="tablist" aria-label="Plural form" className="flex gap-1">
      {target.forms.map((form, i) => (
        <button
          key={i}
          type="button"
          role="tab"
          aria-selected={active === i}
          onClick={() => onChange(i)}
          title={form == null ? "Empty" : "Filled"}
          className={cn(
            "inline-flex items-center gap-2 h-[22px] px-2 rounded-sm border",
            "text-xs tracking-loose transition-colors duration-100 ease-out",
            active === i
              ? "text-fg-primary bg-accent-subtle border-accent-subtle-border"
              : "text-fg-tertiary bg-transparent border-transparent hover:text-fg-secondary hover:bg-bg-hover",
          )}
        >
          <span className="font-mono uppercase">form {i}</span>
          <span
            aria-hidden="true"
            className={cn(
              "w-1.5 h-1.5 rounded-pill",
              form != null ? "bg-state-finished" : "bg-border-strong",
            )}
          />
        </button>
      ))}
    </div>
  );
}

function formatProvenance(unit: Unit): string | null {
  const { file, line } = unit.provenance;
  if (!file) return null;
  return line ? `${file}:${line}` : file;
}
