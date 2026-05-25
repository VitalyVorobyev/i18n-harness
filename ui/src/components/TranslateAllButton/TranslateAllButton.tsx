// TranslateAllButton — a dropdown button that starts a bulk translation job.
//
// Shows the count of units that will be translated under the selected scope.
// Hidden entirely when a batch is in flight (`batchActive`).
// The selected scope is session-only (not persisted); defaults to "untranslated".

import { useRef, useState } from "react";
import { cn } from "../../lib/cn";
import type { BatchScope, Unit } from "../../lib/types";

interface Props {
  units: Unit[];
  onStart: (scope: BatchScope) => void;
  /** True when any batch is in flight — the button hides entirely. */
  batchActive: boolean;
}

const SCOPE_LABELS: Record<BatchScope, string> = {
  untranslated: "Untranslated only",
  "untranslated-and-proposed": "Untranslated + Proposed",
};

function countForScope(units: Unit[], scope: BatchScope): number {
  if (scope === "untranslated") {
    return units.filter((u) => u.state === "untranslated").length;
  }
  return units.filter(
    (u) => u.state === "untranslated" || u.state === "proposed",
  ).length;
}

export function TranslateAllButton({ units, onStart, batchActive }: Props) {
  const [scope, setScope] = useState<BatchScope>("untranslated");
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);

  const count = countForScope(units, scope);
  const disabled = count === 0;

  // Hide entirely while a batch is running.
  if (batchActive) return null;

  function handleMainClick() {
    if (disabled) return;
    onStart(scope);
  }

  function handleScopeSelect(s: BatchScope) {
    setScope(s);
    setMenuOpen(false);
  }

  function handleChevronKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown") {
      e.preventDefault();
      setMenuOpen((prev) => !prev);
    }
    if (e.key === "Escape") {
      setMenuOpen(false);
    }
  }

  return (
    <div className="relative shrink-0" ref={menuRef}>
      <div className="flex items-center">
        {/* Main action button */}
        <button
          type="button"
          onClick={handleMainClick}
          disabled={disabled}
          aria-disabled={disabled}
          title={
            disabled
              ? "All units already translated"
              : `Translate ${count} unit${count === 1 ? "" : "s"} (${SCOPE_LABELS[scope]})`
          }
          className={cn(
            "h-6 pl-2.5 pr-2 rounded-l-md border-y border-l text-xs font-medium",
            "transition-colors duration-100 ease-out",
            "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent focus-visible:z-10",
            disabled
              ? "border-border-subtle text-fg-disabled cursor-not-allowed bg-transparent"
              : "border-border-default text-fg-secondary bg-transparent hover:bg-bg-hover hover:text-fg-primary hover:border-border-strong active:bg-bg-selected",
          )}
        >
          Translate all{" "}
          {!disabled && (
            <span className="tabular-nums text-accent">({count})</span>
          )}
        </button>

        {/* Chevron toggle for scope dropdown */}
        <button
          type="button"
          onClick={() => setMenuOpen((prev) => !prev)}
          onKeyDown={handleChevronKeyDown}
          aria-haspopup="listbox"
          aria-expanded={menuOpen}
          aria-label="Choose translation scope"
          title="Choose what to translate"
          className={cn(
            "h-6 w-6 flex items-center justify-center rounded-r-md border text-xs",
            "transition-colors duration-100 ease-out",
            "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent focus-visible:z-10",
            menuOpen
              ? "border-accent bg-accent-subtle text-fg-primary"
              : "border-border-default text-fg-tertiary bg-transparent hover:bg-bg-hover hover:text-fg-secondary hover:border-border-strong",
          )}
        >
          {/* Simple chevron-down glyph using Unicode */}
          <svg
            aria-hidden="true"
            width="10"
            height="10"
            viewBox="0 0 10 10"
            fill="currentColor"
          >
            <path
              d="M1.5 3.5L5 7l3.5-3.5"
              stroke="currentColor"
              strokeWidth="1.4"
              fill="none"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </button>
      </div>

      {/* Scope dropdown */}
      {menuOpen && (
        <div
          role="listbox"
          aria-label="Translation scope"
          className={cn(
            "absolute right-0 top-full mt-1 z-20 min-w-[200px]",
            "rounded-md border border-border-default bg-bg-elevated shadow-md",
            "py-1",
          )}
        >
          {(Object.entries(SCOPE_LABELS) as [BatchScope, string][]).map(
            ([s, label]) => (
              <button
                key={s}
                type="button"
                role="option"
                aria-selected={s === scope}
                onClick={() => handleScopeSelect(s)}
                className={cn(
                  "w-full text-left px-3 py-1.5 text-xs",
                  "transition-colors duration-100 ease-out",
                  "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-accent",
                  s === scope
                    ? "text-fg-primary bg-accent-subtle font-medium"
                    : "text-fg-secondary hover:bg-bg-hover hover:text-fg-primary",
                )}
              >
                {label}
                {s === scope && (
                  <span className="ml-2 text-accent" aria-hidden="true">
                    ✓
                  </span>
                )}
              </button>
            ),
          )}
        </div>
      )}

      {/* Click-outside to close */}
      {menuOpen && (
        <div
          className="fixed inset-0 z-10"
          aria-hidden="true"
          onClick={() => setMenuOpen(false)}
        />
      )}
    </div>
  );
}
