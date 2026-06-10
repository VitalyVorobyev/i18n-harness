// CatalogActions — a small, keyboard-accessible actions menu for one catalog
// row. Hosts the reference-reuse / export-remainder / merge affordances so the
// per-catalog row stays uncluttered while keeping the actions discoverable.
//
// Rendered as a sibling of the catalog nav button (never nested — nesting
// interactive controls inside the row <button> would be invalid).

import { useCallback, useEffect, useId, useRef, useState } from "react";
import { cn } from "../../lib/cn";
import { Spinner } from "../primitives";

export interface CatalogActionsProps {
  /** Absolute path of the catalog this menu acts on. */
  catalogPath: string;
  /** Manifest-relative path, for accessible labels. */
  displayName: string;
  /** True while a reuse/split/merge IPC call for this catalog is in flight. */
  busy: boolean;
  /** Only Qt `.ts` catalogs support reuse/split/merge today. */
  enabled: boolean;
  onApplyReferences: (catalogPath: string) => void;
  onExportRemainder: (catalogPath: string) => void;
  onMerge: (catalogPath: string) => void;
}

export function CatalogActions({
  catalogPath,
  displayName,
  busy,
  enabled,
  onApplyReferences,
  onExportRemainder,
  onMerge,
}: CatalogActionsProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const menuId = useId();

  const close = useCallback(() => setOpen(false), []);

  // Close on outside click and on Escape; restore focus to the trigger.
  useEffect(() => {
    if (!open) return;
    function onDocClick(e: MouseEvent) {
      if (!rootRef.current?.contains(e.target as Node)) close();
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.stopPropagation();
        close();
        triggerRef.current?.focus();
      }
    }
    document.addEventListener("mousedown", onDocClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDocClick);
      document.removeEventListener("keydown", onKey);
    };
  }, [open, close]);

  const run = useCallback(
    (action: (p: string) => void) => {
      close();
      action(catalogPath);
    },
    [close, catalogPath],
  );

  if (!enabled) return null;

  return (
    <div ref={rootRef} className="relative shrink-0">
      <button
        ref={triggerRef}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        aria-label={`Reuse actions for ${displayName}`}
        title="Reuse / split / merge"
        disabled={busy}
        onClick={(e) => {
          e.stopPropagation();
          setOpen((v) => !v);
        }}
        className={cn(
          "inline-flex items-center justify-center w-5 h-5 rounded-sm",
          "text-fg-tertiary hover:text-fg-primary hover:bg-bg-hover",
          "transition-colors duration-75",
          "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent",
          "disabled:opacity-40 disabled:cursor-not-allowed",
          open && "bg-bg-selected text-fg-primary",
        )}
      >
        {busy ? <Spinner size={12} /> : <DotsIcon />}
      </button>

      {open && (
        <div
          id={menuId}
          role="menu"
          aria-label={`Reuse actions for ${displayName}`}
          className={cn(
            "absolute right-0 top-6 z-40 min-w-[180px] py-1",
            "rounded-md border border-border-default bg-bg-elevated",
            "shadow-md",
          )}
        >
          <MenuItem onClick={() => run(onApplyReferences)}>
            Apply references…
          </MenuItem>
          <MenuItem onClick={() => run(onExportRemainder)}>
            Export remainder…
          </MenuItem>
          <MenuItem onClick={() => run(onMerge)}>Merge translated…</MenuItem>
        </div>
      )}
    </div>
  );
}

function MenuItem({
  onClick,
  children,
}: {
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className={cn(
        "w-full text-left px-3 py-1.5 text-xs text-fg-secondary",
        "hover:bg-bg-hover hover:text-fg-primary",
        "focus-visible:outline-none focus-visible:bg-bg-hover focus-visible:text-fg-primary",
        "transition-colors duration-75",
      )}
    >
      {children}
    </button>
  );
}

function DotsIcon() {
  return (
    <svg
      width={14}
      height={14}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
    >
      <circle cx={12} cy={5} r={1.6} />
      <circle cx={12} cy={12} r={1.6} />
      <circle cx={12} cy={19} r={1.6} />
    </svg>
  );
}
