import { Badge, Button, Callout, Empty, Field, Kbd, Textarea } from "@vitavision/ui";
import { Check, Languages, Undo2 } from "lucide-react";
import { useState } from "react";

import type { CatalogSummary, UnitView } from "../api";
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
  onSave: (forms: string[], done: boolean) => void;
  onTranslate: () => void;
}) {
  const { unit } = props;
  const [draft, setDraft] = useState<string[]>(() => unit?.target ?? []);

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
                    className="font-sans text-base"
                    value={text}
                    rows={plural ? 2 : 3}
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
