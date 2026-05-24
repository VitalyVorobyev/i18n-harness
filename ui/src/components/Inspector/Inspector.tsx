import type { Unit } from "../../lib/types";
import styles from "./Inspector.module.css";

interface Props {
  unit: Unit;
}

export function Inspector({ unit }: Props) {
  const isPlural = unit.plural_arity != null;
  const placeholders = unit.placeholders ?? [];
  const placeholderCount = Array.isArray(placeholders) ? placeholders.length : 0;
  const provenance = unit.provenance;

  return (
    <aside className={`${styles.root} app-chrome`}>
      <header className={styles.head}>
        <span className={styles.heading}>Inspector</span>
      </header>

      <dl className={styles.list}>
        <Item label="State">
          <code className={styles.code}>{unit.state}</code>
        </Item>

        <Item label="Plural arity">
          {isPlural ? (
            <code className={styles.code}>
              {unit.plural_arity}&nbsp;forms
            </code>
          ) : (
            <span className={styles.muted}>singular</span>
          )}
        </Item>

        <Item label="Placeholders">
          {placeholderCount > 0 ? (
            <code className={styles.code}>{placeholderCount}</code>
          ) : (
            <span className={styles.muted}>none</span>
          )}
        </Item>

        <Item label="Source location">
          {provenance.file ? (
            <code className={styles.code}>
              {provenance.file}
              {provenance.line ? `:${provenance.line}` : ""}
            </code>
          ) : (
            <span className={styles.muted}>—</span>
          )}
        </Item>
      </dl>

      <div className={styles.findings}>
        <div className={styles.findingsHead}>Findings</div>
        <div className={styles.findingsBody}>
          <span className={styles.muted}>
            No findings. The gate runs when this unit is translated; later
            milestones surface its hard and soft flags here.
          </span>
        </div>
      </div>
    </aside>
  );
}

function Item({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className={styles.item}>
      <dt className={styles.itemLabel}>{label}</dt>
      <dd className={styles.itemValue}>{children}</dd>
    </div>
  );
}
