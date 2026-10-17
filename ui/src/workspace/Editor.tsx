import { Badge, Button, Callout, Empty, Field, Kbd, Textarea } from "@vitavision/ui";
import { Check, Copy, Languages, Undo2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import type { CatalogSummary, SiblingView, UnitView } from "../api";
import { STATUS_LABEL } from "../lib/format";

/**
 * The selected string. Mount it with `key={unit.id}` so the draft resets when
 * the selection changes.
 */
export function Editor(props: {
  catalog: CatalogSummary;
  unit: UnitView | null;
  canTranslate: boolean;
  busy: boolean;
  /** The same string in other files of the language (`null`: not known yet). */
  siblings: SiblingView[] | null;
  /** Copy the saved translation to every other file. */
  onUseEverywhere: () => void;
  /** Take keyboard focus when shown (the string was chosen from the keyboard). */
  autoFocus: boolean;
  onSave: (forms: string[], done: boolean) => void;
  onTranslate: () => void;
  /** Esc: back to the list. */
  onLeave: () => void;
  /** Alt+↑ / Alt+↓: the previous or next string. */
  onStep: (step: 1 | -1) => void;
}) {
  const { unit } = props;
  const [draft, setDraft] = useState<string[]>(() => unit?.target ?? []);
  const first = useRef<HTMLTextAreaElement>(null);
  const shown = unit !== null;
  const { autoFocus } = props;

  useEffect(() => {
    if (shown && autoFocus) first.current?.focus();
  }, [shown, autoFocus]);

  if (unit === null) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <Empty>Select a string to edit its translation.</Empty>
      </div>
    );
  }

  const changed = draft.some((t, i) => t !== unit.target[i]);
  const complete = draft.every((t) => t.trim() !== "");
  const plural = unit.plural_slots.length > 0;

  const done = () => props.onSave(draft, true);
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && complete) {
      e.preventDefault();
      done();
    } else if (e.key === "Escape") {
      e.preventDefault();
      props.onLeave();
    } else if (e.altKey && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      e.preventDefault();
      props.onStep(e.key === "ArrowDown" ? 1 : -1);
    }
  };

  return (
    <div className="flex h-full flex-col overflow-y-auto" onKeyDown={onKeyDown}>
      <div className="space-y-4 p-4">
        <div className="flex items-center justify-between gap-2">
          <span className="truncate text-xs text-fg-subtle">
            {unit.context.group ?? "No context"}
          </span>
          <Badge
            tone={
              unit.status === "done" ? "normal" : unit.status === "check" ? "warning" : "neutral"
            }
          >
            {STATUS_LABEL[unit.status]}
          </Badge>
        </div>

        <section aria-label="Original" className="space-y-1">
          <h3 className="text-xs font-medium uppercase tracking-wide text-fg-subtle">Original</h3>
          <p className="whitespace-pre-wrap break-words text-base">{unit.source}</p>
          {unit.context.disambiguation && (
            <p className="text-sm text-fg-muted">Meaning: {unit.context.disambiguation}</p>
          )}
          {(unit.context.developer_notes ?? []).map((note) => (
            <p key={note} className="text-sm text-fg-muted">
              Note: {note}
            </p>
          ))}
          {(unit.context.translator_notes ?? []).map((note) => (
            <p key={note} className="text-sm text-fg-muted">
              Translator note: {note}
            </p>
          ))}
        </section>

        {props.catalog.pivot_language && unit.existing && (
          <section aria-label={`In ${props.catalog.pivot_language}`} className="space-y-1">
            <h3 className="text-xs font-medium uppercase tracking-wide text-fg-subtle">
              {props.catalog.pivot_language}
            </h3>
            {unit.existing
              .map((text, slot) => ({ slot, text }))
              .map((form) => (
                <p
                  key={`existing-${form.slot}`}
                  className="whitespace-pre-wrap break-words text-fg-muted"
                >
                  {form.text || <span className="italic text-fg-subtle">empty</span>}
                </p>
              ))}
          </section>
        )}

        <section aria-label="Translation" className="space-y-3">
          {draft
            .map((text, index) => ({ index, text }))
            .map(({ index: i, text }) => {
              const slot = unit.plural_slots[i];
              const label = plural
                ? `${props.catalog.language} — ${slot?.category ?? `form ${i + 1}`}${slot?.example !== null && slot?.example !== undefined ? ` (e.g. ${slot.example})` : ""}`
                : props.catalog.language;
              return (
                <Field key={slot?.category ?? `form-${String(i)}`} label={label}>
                  <Textarea
                    ref={i === 0 ? first : undefined}
                    className="text-base"
                    value={text}
                    minRows={plural ? 1 : 2}
                    maxRows={10}
                    disabled={!unit.editable || props.busy}
                    onChange={(e) => {
                      const next = [...draft];
                      next[i] = e.target.value;
                      setDraft(next);
                    }}
                  />
                </Field>
              );
            })}
        </section>

        {props.siblings && props.siblings.length > 0 && (
          <section aria-label="Other files" className="space-y-2">
            <h3 className="text-xs font-medium uppercase tracking-wide text-fg-subtle">
              Other files
            </h3>
            <ul className="divide-y divide-line rounded-panel border border-line">
              {props.siblings.map((s) => {
                const translated = s.target.some((t) => t !== "");
                const same = s.target.every((t, i) => t === unit.target[i]);
                return (
                  <li key={s.path} className="flex items-start gap-2 px-2.5 py-2 text-sm">
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-xs text-fg-subtle">{s.file}</span>
                      {translated ? (
                        <span className="block whitespace-pre-wrap break-words">
                          {s.target.join(" · ")}
                        </span>
                      ) : (
                        <span className="block italic text-fg-subtle">Not translated</span>
                      )}
                    </span>
                    <Badge
                      tone={
                        s.status === "done"
                          ? "normal"
                          : s.status === "check"
                            ? "warning"
                            : "neutral"
                      }
                    >
                      {STATUS_LABEL[s.status]}
                    </Badge>
                    {translated && !same && s.target.length === draft.length && (
                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={!unit.editable || props.busy}
                        onClick={() => setDraft(s.target)}
                      >
                        Use this
                      </Button>
                    )}
                  </li>
                );
              })}
            </ul>
            {(() => {
              const differ = props.siblings.some(
                (s) => !s.target.every((t, i) => t === unit.target[i]),
              );
              const savedText = unit.target.every((t) => t !== "");
              return (
                differ &&
                savedText && (
                  <Button
                    variant="secondary"
                    size="sm"
                    icon={<Copy aria-hidden size={14} />}
                    disabled={changed || props.busy}
                    title={changed ? "Save your edit first" : undefined}
                    onClick={props.onUseEverywhere}
                  >
                    Use this translation in every file
                  </Button>
                )
              );
            })()}
          </section>
        )}

        {unit.issues.length > 0 && (
          <section aria-label="Problems" className="space-y-2">
            {unit.issues.map((issue) => (
              <Callout
                key={issue.message}
                tone={
                  issue.severity === "error"
                    ? "error"
                    : issue.severity === "warning"
                      ? "warning"
                      : "info"
                }
              >
                {issue.message}
              </Callout>
            ))}
          </section>
        )}
        {unit.issues.some((i) => i.severity === "error") && (
          <p className="text-xs text-fg-subtle">
            A translation with a problem marked in red is not written to the file when you save; the
            file keeps its previous text until it is fixed.
          </p>
        )}
      </div>

      <div className="mt-auto flex flex-wrap items-center gap-2 border-t border-line p-3">
        <Button
          variant="primary"
          icon={<Check aria-hidden size={16} />}
          disabled={!unit.editable || !complete || props.busy}
          onClick={done}
        >
          Done
        </Button>
        <Button
          variant="secondary"
          disabled={!unit.editable || !changed || props.busy}
          onClick={() => props.onSave(draft, false)}
        >
          Keep as draft
        </Button>
        <Button
          variant="ghost"
          icon={<Languages aria-hidden size={16} />}
          disabled={!unit.editable || !props.canTranslate || props.busy}
          loading={props.busy}
          onClick={props.onTranslate}
        >
          Translate again
        </Button>
        {changed && (
          <Button
            variant="ghost"
            icon={<Undo2 aria-hidden size={16} />}
            onClick={() => setDraft(unit.target)}
            disabled={props.busy}
          >
            Undo
          </Button>
        )}
        <span className="ml-auto hidden text-xs text-fg-subtle lg:inline">
          <Kbd>Ctrl</Kbd>+<Kbd>Enter</Kbd> done and next
        </span>
      </div>
    </div>
  );
}
