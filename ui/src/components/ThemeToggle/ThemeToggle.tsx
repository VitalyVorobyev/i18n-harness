/**
 * ThemeToggle — small icon button (sun / moon) that switches between
 * the "Technical Journal" light theme and the "Observatory" dark theme.
 *
 * No icon library dependency — plain inline SVG paths.
 */
import { cn } from "../../lib/cn";
import type { Theme } from "../../lib/theme";

interface Props {
  theme: Theme;
  onToggle: () => void;
  className?: string;
}

export function ThemeToggle({ theme, onToggle, className }: Props) {
  const isDark = theme === "dark";
  const label = isDark ? "Switch to light mode" : "Switch to dark mode";

  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onToggle}
      className={cn(
        "inline-flex items-center justify-center",
        "w-7 h-7 rounded-md border border-border-default",
        "text-fg-secondary bg-transparent",
        "hover:bg-bg-hover hover:text-fg-primary hover:border-border-strong",
        "active:bg-bg-selected",
        "transition-colors duration-100 ease-out",
        "focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-1",
        className,
      )}
    >
      {isDark ? <SunIcon /> : <MoonIcon />}
    </button>
  );
}

/* 16 x 16 sun icon — shows in dark mode (click to switch to light). */
function SunIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 16 16"
      fill="none"
      aria-hidden="true"
      xmlns="http://www.w3.org/2000/svg"
    >
      <circle cx="8" cy="8" r="3.25" stroke="currentColor" strokeWidth="1.5" />
      <line
        x1="8"
        y1="1"
        x2="8"
        y2="3"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
      <line
        x1="8"
        y1="13"
        x2="8"
        y2="15"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
      <line
        x1="1"
        y1="8"
        x2="3"
        y2="8"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
      <line
        x1="13"
        y1="8"
        x2="15"
        y2="8"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
      <line
        x1="2.93"
        y1="2.93"
        x2="4.34"
        y2="4.34"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
      <line
        x1="11.66"
        y1="11.66"
        x2="13.07"
        y2="13.07"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
      <line
        x1="13.07"
        y1="2.93"
        x2="11.66"
        y2="4.34"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
      <line
        x1="4.34"
        y1="11.66"
        x2="2.93"
        y2="13.07"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
    </svg>
  );
}

/* 16 x 16 crescent moon — shows in light mode (click to switch to dark). */
function MoonIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 16 16"
      fill="none"
      aria-hidden="true"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M13.5 10.5A6 6 0 0 1 5.5 2.5a6.002 6.002 0 1 0 8 8z"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
