import type { ReactNode } from "react";

interface Props {
  children: ReactNode;
}

export function Eyebrow({ children }: Props) {
  return (
    <span
      style={{
        display: "block",
        fontSize: "11px",
        fontWeight: 500,
        textTransform: "uppercase",
        letterSpacing: "0.08em",
        color: "var(--color-fg-tertiary)",
        lineHeight: 1,
      }}
    >
      {children}
    </span>
  );
}
