interface Props {
  total: number;
  finished: number;
  proposed: number;
  height?: number;
  width?: number | string;
}

export function ProgressBar({
  total,
  finished,
  proposed,
  height = 6,
  width,
}: Props) {
  const safeTotal = total > 0 ? total : 1;
  const finishedPct = Math.min(100, (finished / safeTotal) * 100);
  const proposedPct = Math.min(100 - finishedPct, (proposed / safeTotal) * 100);
  const remainderPct = 100 - finishedPct - proposedPct;

  const style: React.CSSProperties = {
    display: "flex",
    height,
    borderRadius: 999,
    overflow: "hidden",
    width:
      width !== undefined
        ? typeof width === "number"
          ? `${width}px`
          : width
        : "100%",
    flexShrink: 0,
  };

  return (
    <div
      style={style}
      role="img"
      aria-label={`${finished} finished, ${proposed} proposed of ${total} total`}
    >
      {finishedPct > 0 && (
        <div
          style={{
            width: `${finishedPct}%`,
            background: "var(--color-state-finished)",
          }}
        />
      )}
      {proposedPct > 0 && (
        <div
          style={{
            width: `${proposedPct}%`,
            background: "var(--color-state-proposed)",
          }}
        />
      )}
      {remainderPct > 0 && (
        <div
          style={{
            width: `${remainderPct}%`,
            background: "var(--color-bg-input)",
          }}
        />
      )}
    </div>
  );
}
