interface Props {
  value: string | number;
  label: string;
  tone?: "default" | "finished" | "proposed" | "untranslated" | "severityHard";
}

const TONE_COLOR: Record<NonNullable<Props["tone"]>, string> = {
  default: "var(--color-fg-primary)",
  finished: "var(--color-state-finished)",
  proposed: "var(--color-state-proposed)",
  untranslated: "var(--color-state-untranslated)",
  severityHard: "var(--color-severity-hard)",
};

export function Stat({ value, label, tone = "default" }: Props) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
      <span
        style={{
          fontFamily: "var(--font-mono)",
          fontSize: "var(--text-md)",
          fontWeight: 600,
          lineHeight: 1,
          color: TONE_COLOR[tone],
          letterSpacing: "-0.01em",
        }}
      >
        {value}
      </span>
      <span
        style={{
          fontSize: "10.5px",
          fontWeight: 400,
          lineHeight: 1,
          color: "var(--color-fg-tertiary)",
          letterSpacing: "0.02em",
        }}
      >
        {label}
      </span>
    </div>
  );
}
