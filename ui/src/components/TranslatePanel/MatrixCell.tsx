// MatrixCell — one locale cell inside a matrix card. Owns the per-state
// affordances (Translate / Accept+Retry+textarea / Read-only+Edit) and the
// hard-flag gating on Accept.
//
// The cell's identity in the matrix is (catalogPath, unitId). The cell is
// purely controlled — it dispatches IPC callbacks the container provides
// and re-renders when the unit shape on disk changes.

import { useEffect, useRef, useState } from "react";
import { cn } from "../../lib/cn";
import type { TargetEdit, Unit } from "../../lib/types";
import { severityOf } from "../../lib/types";
import { LocaleTag } from "../primitives";
import { StateBadge } from "../StateBadge/StateBadge";

interface Props {
  locale: string;
  /** The unit on disk for this (catalog, id) — null when the catalog for
   *  this locale has not yet been loaded into the cache. */
  unit: Unit | null;
  /** Absolute catalog path; needed by the IPC layer. Null when the locale
   *  has no matching catalog (rare; manifest typo). */
  catalogPath: string | null;
  /** True while a per-unit IPC (translate / accept) is in flight. */
  busy: boolean;
  /** Cell currently owns the keyboard / inspector focus. */
  focused: boolean;
  /** Cell has an in-memory edit not yet flushed to disk. */
  edited: boolean;
  /** Cell is registered as the keyboard target. */
  onFocusCell: () => void;
  /** Fire Translate-this-unit-to-this-locale via the per-unit IPC. */
  onTranslate: (catalogPath: string, unit: Unit) => void;
  /** Commit a draft edit on blur. */
  onEdit: (catalogPath: string, unit: Unit, edit: TargetEdit) => void;
  /** Mark the unit reviewed / finished via the per-unit IPC. */
  onAccept: (catalogPath: string, unit: Unit) => void;
  /** Reopen a Finished unit for further edits (transitions to Proposed). */
  onReopen: (catalogPath: string, unit: Unit) => void;
}

export function MatrixCell({
  locale,
  unit,
  catalogPath,
  busy,
  focused,
  edited,
  onFocusCell,
  onTranslate,
  onEdit,
  onAccept,
  onReopen,
}: Props) {
  // ── Inactive cell — catalog not loaded yet or missing locale entirely ───
  if (!unit || !catalogPath) {
    return (
      <div
        className={cn(
          "flex flex-col gap-2 px-3.5 py-3 min-h-[132px]",
          "border-r border-b border-border-subtle",
        )}
      >
        <div className="flex items-center gap-2">
          <LocaleTag locale={locale} tone="muted" />
        </div>
        <p className="m-0 text-xs italic text-fg-tertiary">loading…</p>
      </div>
    );
  }

  const hardFlag =
    Array.isArray(unit.flags) &&
    unit.flags.some((flag) => severityOf(flag) === "hard");
  const anyFlag = Array.isArray(unit.flags) && unit.flags.length > 0;
  const state = unit.state;

  return (
    <div
      onClick={onFocusCell}
      onFocus={onFocusCell}
      className={cn(
        "flex flex-col gap-2 px-3.5 py-3 min-h-[132px]",
        "border-r border-b border-border-subtle transition-colors duration-100",
        focused ? "bg-bg-selected" : "bg-transparent",
      )}
    >
      <div className="flex items-center gap-2">
        <LocaleTag locale={locale} tone="default" />
        <StateBadge state={state} variant="dot" />
        <span
          className={cn(
            "inline-flex items-center h-4 px-1.5 rounded-pill border",
            "text-[9.5px] font-medium uppercase tracking-loose whitespace-nowrap",
            state === "untranslated" &&
              "bg-state-untranslated-bg border-state-untranslated-border text-state-untranslated",
            state === "proposed" &&
              "bg-state-proposed-bg border-state-proposed-border text-state-proposed",
            state === "finished" &&
              "bg-state-finished-bg border-state-finished-border text-state-finished",
            (state === "vanished" || state === "obsolete") &&
              "bg-state-vanished-bg border-state-vanished-border text-state-vanished",
          )}
        >
          {state}
        </span>
        {edited && (
          <span
            className={cn(
              "inline-flex items-center h-4 px-1.5 rounded-pill border",
              "text-[9.5px] font-medium uppercase tracking-loose whitespace-nowrap",
              "bg-accent-subtle border-accent-subtle-border text-accent",
            )}
            title="Has unsaved local edits"
          >
            edited
          </span>
        )}
        {anyFlag && (
          <span
            role="img"
            aria-label={`Flags: ${unit.flags.join(", ")}`}
            className="ml-auto text-severity-hard inline-flex"
            title={unit.flags.join(", ")}
          >
            <AlertTriangleIcon size={13} />
          </span>
        )}
      </div>

      {state === "untranslated" && (
        <UntranslatedBody
          locale={locale}
          busy={busy}
          onTranslate={() => onTranslate(catalogPath, unit)}
        />
      )}
      {state === "proposed" && (
        <ProposedBody
          unit={unit}
          busy={busy}
          hardFlag={hardFlag}
          onCommit={(edit) => onEdit(catalogPath, unit, edit)}
          onAccept={() => onAccept(catalogPath, unit)}
          onRetry={() => onTranslate(catalogPath, unit)}
        />
      )}
      {state === "finished" && (
        <FinishedBody
          unit={unit}
          busy={busy}
          onReopen={() => onReopen(catalogPath, unit)}
        />
      )}
      {(state === "vanished" || state === "obsolete") && <VanishedBody />}
    </div>
  );
}

// ── State-specific bodies ───────────────────────────────────────────────────

function UntranslatedBody({
  locale,
  busy,
  onTranslate,
}: {
  locale: string;
  busy: boolean;
  onTranslate: () => void;
}) {
  return (
    <>
      <div
        className={cn(
          "flex-1 flex items-center justify-center rounded-md",
          "border border-dashed border-border-default",
          "px-3 py-2.5 italic text-fg-tertiary",
        )}
        style={{ fontSize: 11.5 }}
      >
        no translation
      </div>
      <button
        type="button"
        onClick={onTranslate}
        disabled={busy}
        className={cn(
          "w-full inline-flex items-center justify-center gap-1.5 h-7 px-3",
          "rounded-md text-xs font-medium transition-colors duration-100",
          "text-accent-fg bg-accent",
          "enabled:hover:bg-accent-hover enabled:active:bg-accent-active",
          "disabled:bg-accent-subtle disabled:text-fg-disabled disabled:cursor-not-allowed",
        )}
      >
        <SparklesIcon size={12} />
        {busy ? "Translating…" : `Translate to ${locale}`}
      </button>
    </>
  );
}

function ProposedBody({
  unit,
  busy,
  hardFlag,
  onCommit,
  onAccept,
  onRetry,
}: {
  unit: Unit;
  busy: boolean;
  hardFlag: boolean;
  onCommit: (edit: TargetEdit) => void;
  onAccept: () => void;
  onRetry: () => void;
}) {
  // Local draft state: typing feels native; the IPC is hit on blur.
  // Plural support in Matrix is intentionally minimal — pluraled units fall
  // through to the singular-shaped draft, edits go to form_index 0; users
  // who need per-form editing land in Focus mode (PR 5).
  const initial = readSingular(unit);
  const [draft, setDraft] = useState(initial);
  const initialRef = useRef(initial);
  useEffect(() => {
    setDraft(initial);
    initialRef.current = initial;
  }, [initial]);

  const commit = () => {
    if (draft === initialRef.current) return;
    const text = draft.length === 0 ? null : draft;
    const edit: TargetEdit =
      unit.target.kind === "plural"
        ? { kind: "plural", form_index: 0, text }
        : { kind: "singular", text };
    initialRef.current = draft;
    onCommit(edit);
  };

  const acceptDisabled = hardFlag || busy;

  return (
    <>
      <textarea
        value={draft}
        rows={2}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        disabled={busy}
        spellCheck
        aria-label="Translation draft"
        data-matrix-cell-input
        className={cn(
          "w-full px-2.5 py-1.5 rounded-md border resize-none",
          "font-mono text-xs leading-snug text-fg-primary",
          "bg-bg-input border-border-subtle whitespace-pre-wrap break-words",
          "focus:border-accent focus:outline-none focus-visible:outline-none",
          "disabled:bg-bg-surface disabled:text-fg-disabled disabled:cursor-not-allowed",
        )}
      />
      <div className="flex items-center gap-1">
        <button
          type="button"
          onClick={onAccept}
          disabled={acceptDisabled}
          title={
            hardFlag
              ? "Resolve the hard gate flag before accepting"
              : busy
                ? "Working…"
                : "Accept and mark Finished"
          }
          className={cn(
            "flex-1 inline-flex items-center justify-center gap-1.5 h-7 px-3",
            "rounded-md text-xs font-medium transition-colors duration-100",
            "text-accent-fg bg-accent",
            "enabled:hover:bg-accent-hover enabled:active:bg-accent-active",
            "disabled:bg-accent-subtle disabled:text-fg-disabled disabled:cursor-not-allowed",
          )}
        >
          <CheckIcon size={12} />
          Accept
        </button>
        <button
          type="button"
          onClick={onRetry}
          disabled={busy}
          aria-label="Retry translation with the model"
          title="Retry with model"
          className={cn(
            "inline-flex items-center justify-center h-7 w-7 rounded-md border",
            "transition-colors duration-100",
            "border-border-default text-fg-secondary bg-transparent",
            "enabled:hover:bg-bg-hover enabled:hover:text-fg-primary enabled:hover:border-border-strong",
            "disabled:opacity-40 disabled:cursor-not-allowed",
          )}
        >
          <SparklesIcon size={12} />
        </button>
      </div>
    </>
  );
}

function FinishedBody({
  unit,
  busy,
  onReopen,
}: {
  unit: Unit;
  busy: boolean;
  onReopen: () => void;
}) {
  const text = readSingular(unit) || "—";
  return (
    <>
      <div
        className={cn(
          "flex-1 font-mono text-xs leading-snug text-fg-primary",
          "px-2.5 py-1.5 rounded-r-sm bg-state-finished-bg",
          "border-l-2 border-state-finished whitespace-pre-wrap break-words",
        )}
      >
        {text}
      </div>
      <button
        type="button"
        onClick={onReopen}
        disabled={busy}
        title="Reopen for editing — returns to Proposed"
        className={cn(
          "self-start inline-flex items-center gap-1.5 h-6 px-2",
          "rounded-md text-[11px] font-medium transition-colors duration-100",
          "text-fg-secondary bg-transparent border border-transparent",
          "enabled:hover:bg-bg-hover enabled:hover:text-fg-primary",
          "disabled:opacity-40 disabled:cursor-not-allowed",
        )}
      >
        <PencilIcon size={11} />
        Edit
      </button>
    </>
  );
}

function VanishedBody() {
  return (
    <div className="flex-1 flex items-center text-xs italic text-fg-tertiary">
      out of scope
    </div>
  );
}

// ── Helpers ────────────────────────────────────────────────────────────────

function readSingular(unit: Unit): string {
  if (unit.target.kind === "singular") return unit.target.text ?? "";
  return unit.target.forms[0] ?? "";
}

// ── Inline SVG icons (no external library) ─────────────────────────────────

function SparklesIcon({ size = 12 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.582a.5.5 0 0 1 0 .962L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z" />
      <path d="M20 3v4" />
      <path d="M22 5h-4" />
      <path d="M4 17v2" />
      <path d="M5 18H3" />
    </svg>
  );
}

function CheckIcon({ size = 12 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M20 6 9 17l-5-5" />
    </svg>
  );
}

function PencilIcon({ size = 11 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z" />
      <path d="m15 5 4 4" />
    </svg>
  );
}

function AlertTriangleIcon({ size = 13 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" />
      <path d="M12 9v4" />
      <path d="M12 17h.01" />
    </svg>
  );
}
