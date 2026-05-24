import { useState, useMemo } from "react";
import { StateBadge } from "../StateBadge/StateBadge";
import { tokenize, type Token } from "../../lib/highlight";
import type { Unit } from "../../lib/types";
import styles from "./UnitEditor.module.css";

interface Props {
  unit: Unit;
}

export function UnitEditor({ unit }: Props) {
  const pluralTarget =
    unit.target.kind === "plural" ? unit.target : null;
  const isPlural = pluralTarget !== null && pluralTarget.forms.length > 1;
  const formCount = pluralTarget?.forms.length ?? 1;
  const [activeForm, setActiveForm] = useState(0);

  // Clamp the active tab when units change.
  const safeForm = Math.min(activeForm, Math.max(0, formCount - 1));

  const targetText = useMemo(() => {
    if (pluralTarget) return pluralTarget.forms[safeForm] ?? null;
    if (unit.target.kind === "singular") return unit.target.text;
    return null;
  }, [pluralTarget, safeForm, unit.target]);

  const provenance = formatProvenance(unit);

  return (
    <section className={styles.root}>
      <header className={styles.header}>
        <div className={styles.headerLeft}>
          <span className={styles.unitId} title={unit.id}>
            {unit.id}
          </span>
        </div>
        <div className={styles.headerRight}>
          <StateBadge state={unit.state} />
          {provenance && (
            <span className={styles.provenance}>{provenance}</span>
          )}
        </div>
      </header>

      <div className={styles.body}>
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

      <footer className={styles.footer}>
        <div className={styles.footnote}>
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
    <div className={styles.section}>
      <div className={styles.sectionHead}>
        <span className={styles.sectionLabel}>{label}</span>
        {right}
      </div>
      {children}
    </div>
  );
}

function TextBlock({ value }: { value: string }) {
  const tokens = tokenize(value);
  return (
    <div className={styles.textBlock}>
      {tokens.map((t, i) => (
        <TokenSpan key={i} token={t} />
      ))}
    </div>
  );
}

function EmptyTextBlock() {
  return (
    <div className={`${styles.textBlock} ${styles.textEmpty}`}>
      No target text yet.
    </div>
  );
}

function TokenSpan({ token }: { token: Token }) {
  if (token.kind === "placeholder") {
    return (
      <span className={styles.placeholder} title="Placeholder">
        {token.value}
      </span>
    );
  }
  if (token.kind === "accel") {
    return (
      <span className={styles.accel} title="Accelerator marker">
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
    <div className={styles.pluralTabs} role="tablist" aria-label="Plural form">
      {target.forms.map((form, i) => (
        <button
          key={i}
          type="button"
          role="tab"
          aria-selected={active === i}
          className={`${styles.pluralTab} ${active === i ? styles.pluralTabActive : ""}`}
          onClick={() => onChange(i)}
          title={form == null ? "Empty" : "Filled"}
        >
          <span className={styles.pluralTabIndex}>form {i}</span>
          <span
            className={`${styles.pluralTabDot} ${form != null ? styles.pluralTabDotFilled : ""}`}
            aria-hidden="true"
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
