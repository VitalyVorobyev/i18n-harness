// Spinner — tiny inline SVG loading indicator.
//
// Wraps a circle-arc SVG in a span with Tailwind's animate-spin utility
// (Tailwind 4 ships this by default — no custom @keyframes needed).
// Size defaults to 12px to match the small icon buttons throughout the
// translate surface; pass a larger value for wider contexts.

interface SpinnerProps {
  size?: number;
}

export function Spinner({ size = 12 }: SpinnerProps) {
  const r = (size - 2) / 2; // radius — 1px inset for stroke
  const cx = size / 2;
  const cy = size / 2;
  const circumference = 2 * Math.PI * r;
  // Show ~75 % of the arc; the gap gives the rotation something to chase.
  const dashArray = `${circumference * 0.75} ${circumference * 0.25}`;

  return (
    <span
      data-testid="translate-spinner"
      className="animate-spin inline-flex shrink-0"
      style={{ width: size, height: size }}
    >
      <svg
        width={size}
        height={size}
        viewBox={`0 0 ${size} ${size}`}
        fill="none"
        stroke="currentColor"
        strokeWidth={1.75}
        strokeLinecap="round"
        aria-hidden="true"
      >
        <circle
          cx={cx}
          cy={cy}
          r={r}
          strokeDasharray={dashArray}
          strokeDashoffset={0}
        />
      </svg>
    </span>
  );
}
