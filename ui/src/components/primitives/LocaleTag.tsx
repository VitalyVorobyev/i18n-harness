interface Props {
  locale: string;
  tone?: "default" | "accent" | "muted";
}

const STYLE: Record<NonNullable<Props["tone"]>, React.CSSProperties> = {
  default: {
    background: "var(--color-bg-elevated)",
    border: "1px solid var(--color-border-default)",
    color: "var(--color-fg-secondary)",
  },
  accent: {
    background: "var(--color-accent-subtle)",
    border: "1px solid var(--color-accent-subtle-border)",
    color: "var(--color-accent)",
  },
  muted: {
    background: "transparent",
    border: "1px solid var(--color-border-subtle)",
    color: "var(--color-fg-tertiary)",
  },
};

export function LocaleTag({ locale, tone = "default" }: Props) {
  return (
    <span
      style={{
        display: "inline-block",
        fontFamily: "var(--font-mono)",
        fontSize: "var(--text-xs)",
        letterSpacing: "0.04em",
        lineHeight: 1,
        padding: "1px 6px",
        borderRadius: 4,
        whiteSpace: "nowrap",
        ...STYLE[tone],
      }}
    >
      {locale}
    </span>
  );
}
