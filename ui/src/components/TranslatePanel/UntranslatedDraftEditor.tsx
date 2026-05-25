// UntranslatedDraftEditor — shared textarea+sparkle surface for untranslated
// units. Used by both MatrixCell and FocusView so the commit-on-blur logic
// stays in one place.
//
// Behaviour:
//   - Empty textarea with data-matrix-cell-input (preserves keyboard nav).
//   - On blur: if the draft is non-empty, calls onCommit with a TargetEdit.
//   - Sparkle button beside the textarea calls onTranslate.
//   - When busy: textarea is disabled, sparkle shows a Spinner.
//   - Plural units: draft always commits to form_index 0 (matching
//     ProposedBody's behaviour in MatrixCell; full plural editing lives
//     in Focus mode's PluralEditor).

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { cn } from "../../lib/cn";
import { usePendingCommits } from "../../lib/pending-commits";
import type { TargetEdit, Unit } from "../../lib/types";
import { SparklesIcon, Spinner } from "../primitives";

interface Props {
  unit: Unit;
  busy: boolean;
  /** Called on blur when the draft is non-empty and changed. */
  onCommit: (edit: TargetEdit) => void;
  /** Called when the sparkle button is clicked. */
  onTranslate: () => void;
  /** Controls textarea size — "compact" for Matrix cells, "full" for Focus rows. */
  size?: "compact" | "full";
  /** Optional extra className applied to the textarea. */
  textareaClassName?: string;
}

export function UntranslatedDraftEditor({
  unit,
  busy,
  onCommit,
  onTranslate,
  size = "compact",
  textareaClassName,
}: Props) {
  const [draft, setDraft] = useState("");
  // Track the value at mount so we never commit "nothing changed".
  const committedRef = useRef("");
  // Mirror live values into refs so the commit-now thunk registered with
  // the pending-commits registry always sees the latest values without
  // having to re-register on every keystroke.
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const onCommitRef = useRef(onCommit);
  onCommitRef.current = onCommit;
  const unitRef = useRef(unit);
  unitRef.current = unit;

  const commit = () => {
    if (draftRef.current === committedRef.current) return;
    // Do not send an empty draft — the unit stays untranslated.
    if (draftRef.current.trim().length === 0) return;
    const text = draftRef.current;
    const edit: TargetEdit =
      unitRef.current.target.kind === "plural"
        ? { kind: "plural", form_index: 0, text }
        : { kind: "singular", text };
    committedRef.current = draftRef.current;
    onCommitRef.current(edit);
  };

  // Register a commit-now thunk so Save All flushes typed-but-not-blurred
  // drafts BEFORE asking the Rust side to persist. Without this, Cmd-S in
  // the middle of an untranslated cell typing would discard the new text.
  // The thunk reads everything via refs, so we register once at mount.
  const { register } = usePendingCommits();
  // biome-ignore lint/correctness/useExhaustiveDependencies: commit reads refs; register once
  useEffect(() => register(commit), [register]);

  const isCompact = size === "compact";

  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  // Auto-grow on content: keep the textarea tall enough for the current
  // draft so multi-line translations stay visible without manual resize.
  // The minimum (~2/4 rows) is enforced by `rows`; we only ever grow.
  // biome-ignore lint/correctness/useExhaustiveDependencies: dom resize needs to fire after each draft change
  useLayoutEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [draft]);

  return (
    <div className="flex items-start gap-1.5 w-full min-w-0">
      <textarea
        ref={textareaRef}
        value={draft}
        rows={isCompact ? 2 : 4}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        disabled={busy}
        spellCheck
        aria-label="Translation draft"
        data-matrix-cell-input
        placeholder={
          isCompact
            ? "Type a translation, or click ✨"
            : "Type the translation here…"
        }
        className={cn(
          "flex-1 min-w-0 px-2.5 py-1.5 rounded-md border resize-none",
          "font-mono leading-snug text-fg-primary",
          "bg-bg-input border-border-subtle whitespace-pre-wrap break-words",
          "focus:border-accent focus:outline-none focus-visible:outline-none",
          "disabled:bg-bg-surface disabled:text-fg-disabled disabled:cursor-not-allowed",
          "placeholder:text-fg-tertiary placeholder:not-italic",
          "max-h-[12rem] overflow-y-auto",
          // Declarative floor: matches rows={2|4} so the autosize
          // useLayoutEffect (which would otherwise set height to scrollHeight
          // ≈ one row for empty content) can never collapse the textarea
          // below its rows minimum on first render.
          isCompact ? "text-xs min-h-12" : "text-[13px] min-h-24",
          textareaClassName,
        )}
      />
      <button
        type="button"
        onClick={onTranslate}
        disabled={busy}
        aria-label="Translate with model"
        title={busy ? "Working…" : "Translate with model"}
        className={cn(
          "shrink-0 inline-flex items-center justify-center rounded-md border",
          "transition-colors duration-100",
          isCompact ? "h-7 w-7" : "h-8 w-8",
          busy
            ? "border-accent text-accent bg-accent-subtle cursor-wait"
            : null,
          busy
            ? null
            : "border-border-default text-fg-secondary bg-transparent",
          busy
            ? null
            : "enabled:hover:bg-bg-hover enabled:hover:text-fg-primary enabled:hover:border-border-strong",
          busy ? null : "disabled:opacity-40 disabled:cursor-not-allowed",
        )}
      >
        {busy ? (
          <Spinner size={isCompact ? 16 : 18} />
        ) : (
          <SparklesIcon size={isCompact ? 12 : 13} />
        )}
      </button>
    </div>
  );
}
