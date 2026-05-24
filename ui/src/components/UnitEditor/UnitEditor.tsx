import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { cn } from "../../lib/cn";
import { type Token, tokenize } from "../../lib/highlight";
import type { TargetEdit, Unit, UnitId } from "../../lib/types";
import { StateBadge } from "../StateBadge/StateBadge";

/// Imperative handle exposed by UnitEditor — App calls this before
/// saving to make sure the textarea's in-progress edit lands in the
/// catalog before save_catalog runs. Without it, ⌘S while the
/// textarea is still focused would persist stale content.
export interface UnitEditorHandle {
  /** Commit any pending draft edit (no-op when the draft matches the
   *  unit on disk). Resolves after the underlying IPC completes. */
  flushPendingEdit: () => Promise<void>;
}

interface Props {
  unit: Unit;
  busy: boolean;
  hasOllama: boolean;
  onEdit: (id: UnitId, edit: TargetEdit) => Promise<void>;
  onTranslate: (id: UnitId) => void;
}

export const UnitEditor = forwardRef<UnitEditorHandle, Props>(
  function UnitEditor(
    { unit, busy, hasOllama, onEdit, onTranslate }: Props,
    ref,
  ) {
    const pluralTarget = unit.target.kind === "plural" ? unit.target : null;
    const isPlural = pluralTarget !== null && pluralTarget.forms.length > 1;
    const formCount = pluralTarget?.forms.length ?? 1;
    const [activeForm, setActiveForm] = useState(0);
    const safeForm = Math.min(activeForm, Math.max(0, formCount - 1));

    const provenance = formatProvenance(unit);
    const writable = unit.state === "untranslated" || unit.state === "proposed";

    // Local draft state so typing feels native; commits on blur and on
    // explicit flush. The server replies with the canonical unit.
    const initialDraft = useMemo(
      () => readTarget(unit, safeForm),
      [unit, safeForm],
    );
    const [draft, setDraft] = useState(initialDraft);
    useEffect(() => {
      setDraft(initialDraft);
    }, [initialDraft]);

    // Keep refs current so flushPendingEdit can read the latest values
    // even when called outside React's render cycle (via the ref).
    const draftRef = useRef(draft);
    const initialDraftRef = useRef(initialDraft);
    const safeFormRef = useRef(safeForm);
    const pluralTargetRef = useRef(pluralTarget);
    const unitIdRef = useRef(unit.id);
    useEffect(() => {
      draftRef.current = draft;
    }, [draft]);
    useEffect(() => {
      initialDraftRef.current = initialDraft;
    }, [initialDraft]);
    useEffect(() => {
      safeFormRef.current = safeForm;
    }, [safeForm]);
    useEffect(() => {
      pluralTargetRef.current = pluralTarget;
    }, [pluralTarget]);
    useEffect(() => {
      unitIdRef.current = unit.id;
    }, [unit.id]);

    const commit = useCallback(async () => {
      const current = draftRef.current;
      if (current === initialDraftRef.current) return;
      const text = current.length === 0 ? null : current;
      const edit: TargetEdit =
        pluralTargetRef.current != null
          ? { kind: "plural", form_index: safeFormRef.current, text }
          : { kind: "singular", text };
      // Optimistically advance the local "initial" so a repeat flush
      // (e.g. ⌘S → blur → ⌘S) does not re-fire the same edit.
      initialDraftRef.current = current;
      await onEdit(unitIdRef.current, edit);
    }, [onEdit]);

    useImperativeHandle(
      ref,
      () => ({
        flushPendingEdit: commit,
      }),
      [commit],
    );

    return (
      <section className="flex-1 flex flex-col overflow-hidden min-w-0 bg-bg-base">
        <header className="shrink-0 flex items-center justify-between gap-4 px-5 py-3 border-b border-border-subtle bg-bg-surface">
          <div className="flex-1 min-w-0">
            <span
              className="block font-mono text-sm text-fg-primary truncate select-text"
              title={unit.id}
            >
              {unit.id}
            </span>
          </div>
          <div className="shrink-0 flex items-center gap-3">
            <StateBadge state={unit.state} />
            {provenance && (
              <span className="font-mono text-xs text-fg-tertiary">
                {provenance}
              </span>
            )}
          </div>
        </header>

        <div className="flex-1 overflow-y-auto p-5 flex flex-col gap-5">
          <Section label="Source">
            <TextBlock value={unit.source} />
          </Section>

          <Section
            label="Target"
            right={
              isPlural && pluralTarget ? (
                <PluralTabs
                  target={pluralTarget}
                  active={safeForm}
                  onChange={setActiveForm}
                />
              ) : null
            }
          >
            <TargetEditor
              value={draft}
              disabled={!writable || busy}
              onChange={setDraft}
              onBlur={() => void commit()}
            />
          </Section>
        </div>

        <footer className="shrink-0 px-5 py-3 border-t border-border-subtle bg-bg-surface flex items-center gap-3">
          <button
            type="button"
            onClick={() => onTranslate(unit.id)}
            disabled={!writable || busy || !hasOllama}
            title={
              !hasOllama
                ? "Catalog has no <TS language=…> — translation needs a locale"
                : busy
                  ? "Translation in progress…"
                  : "Translate via the Ollama backend"
            }
            className={cn(
              "inline-flex items-center gap-2 h-8 px-3 rounded-md text-sm font-medium",
              "transition-colors duration-100 ease-out",
              "text-accent-fg bg-accent",
              "enabled:hover:bg-accent-hover enabled:active:bg-accent-active",
              "disabled:bg-accent-subtle disabled:text-fg-disabled disabled:cursor-not-allowed",
            )}
          >
            {busy ? (
              <>
                <Spinner />
                Translating…
              </>
            ) : (
              <>Translate</>
            )}
          </button>
          <div className="text-xs text-fg-tertiary">
            {writable
              ? "Edit the target; blur to save in memory. ⌘S persists to disk."
              : "Vanished and obsolete units are not writable."}
          </div>
        </footer>
      </section>
    );
  },
);

function Section({
  label,
  right,
  children,
}: {
  label: string;
  right?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2">
      <div className="min-h-[22px] flex items-center justify-between gap-3">
        <span className="text-xs font-semibold uppercase tracking-loose text-fg-tertiary">
          {label}
        </span>
        {right}
      </div>
      {children}
    </div>
  );
}

function TextBlock({ value }: { value: string }) {
  const tokens = tokenize(value);
  return (
    <div
      className={cn(
        "p-4 rounded-md border border-border-subtle bg-bg-surface",
        "font-mono text-md leading-[1.65] text-fg-primary",
        "whitespace-pre-wrap break-words select-text",
      )}
    >
      {tokens.map((t, i) => (
        <TokenSpan key={i} token={t} />
      ))}
    </div>
  );
}

function TargetEditor({
  value,
  disabled,
  onChange,
  onBlur,
}: {
  value: string;
  disabled: boolean;
  onChange: (v: string) => void;
  onBlur: () => void;
}) {
  return (
    <textarea
      value={value}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
      onBlur={onBlur}
      placeholder={disabled ? "" : "Type the translation here…"}
      spellCheck
      className={cn(
        "w-full min-h-[120px] p-4 rounded-md border resize-vertical",
        "font-mono text-md leading-[1.65] text-fg-primary",
        "bg-bg-input border-border-subtle whitespace-pre-wrap break-words",
        "placeholder:text-fg-disabled placeholder:italic placeholder:font-sans",
        "transition-colors duration-100 ease-out",
        "focus:border-accent focus:outline-none focus-visible:outline-none",
        "disabled:bg-bg-surface disabled:text-fg-disabled disabled:cursor-not-allowed",
      )}
    />
  );
}

function TokenSpan({ token }: { token: Token }) {
  if (token.kind === "placeholder") {
    return (
      <span
        title="Placeholder"
        className={cn(
          "inline mx-px px-1 py-px rounded-sm border",
          "font-mono text-[0.94em] text-accent-hover",
          "bg-accent-subtle border-accent-subtle-border",
        )}
      >
        {token.value}
      </span>
    );
  }
  if (token.kind === "accel") {
    return (
      <span
        title="Accelerator marker"
        className="inline text-state-proposed font-semibold"
      >
        {token.value}
      </span>
    );
  }
  return <>{token.value}</>;
}

function PluralTabs({
  target,
  active,
  onChange,
}: {
  target: { kind: "plural"; forms: (string | null)[] };
  active: number;
  onChange: (i: number) => void;
}) {
  return (
    <div role="tablist" aria-label="Plural form" className="flex gap-1">
      {target.forms.map((form, i) => (
        <button
          key={i}
          type="button"
          role="tab"
          aria-selected={active === i}
          onClick={() => onChange(i)}
          title={form == null ? "Empty" : "Filled"}
          className={cn(
            "inline-flex items-center gap-2 h-[22px] px-2 rounded-sm border",
            "text-xs tracking-loose transition-colors duration-100 ease-out",
            active === i
              ? "text-fg-primary bg-accent-subtle border-accent-subtle-border"
              : "text-fg-tertiary bg-transparent border-transparent hover:text-fg-secondary hover:bg-bg-hover",
          )}
        >
          <span className="font-mono uppercase">form {i}</span>
          <span
            aria-hidden="true"
            className={cn(
              "w-1.5 h-1.5 rounded-pill",
              form != null ? "bg-state-finished" : "bg-border-strong",
            )}
          />
        </button>
      ))}
    </div>
  );
}

function Spinner() {
  return (
    <span
      aria-hidden="true"
      className="w-3 h-3 border-2 border-white/40 border-t-white rounded-pill animate-spin"
    />
  );
}

function readTarget(unit: Unit, formIndex: number): string {
  if (unit.target.kind === "singular") return unit.target.text ?? "";
  return unit.target.forms[formIndex] ?? "";
}

function formatProvenance(unit: Unit): string | null {
  const { file, line } = unit.provenance;
  if (!file) return null;
  return line ? `${file}:${line}` : file;
}
