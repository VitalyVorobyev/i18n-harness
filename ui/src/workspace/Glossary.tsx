import { Button, Checkbox, Input, Select, toast } from "@vitavision/ui";
import { FileUp, Plus, Search, Trash2, WandSparkles } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import type { GlossaryEntry, TermUsage } from "../api";
import { errorMessage, useApi } from "../api";
import { plural } from "../lib/format";

interface Row extends GlossaryEntry {
  rowId: number;
}

let nextRowId = 0;
const withId = (entry: GlossaryEntry): Row => ({ ...entry, rowId: nextRowId++ });
const withoutId = ({ rowId: _rowId, ...entry }: Row): GlossaryEntry => entry;

/** Edits wait this long for a pause before they are written. */
const SAVE_DELAY_MS = 600;

type SaveState = "saved" | "pending" | "saving" | "failed";

/**
 * The glossary of one target language: terms kept unchanged or translated a
 * fixed way, how the strings use them, and the run that applies them.
 * Edits are saved as you type.
 */
export function Glossary(props: {
  locale: string;
  language: string;
  /** Target languages of the folder, when there is more than one. */
  languages: { locale: string; language: string }[];
  onLocale: (locale: string) => void;
  /** Translated strings that break the glossary, over every file. */
  breaking: number;
  canApply: boolean;
  onApply: () => void;
  /** The glossary changed on disk: counts and issues need a refresh. */
  onChanged: () => void;
  /** Show the strings that use (or break) a term. */
  onShowStrings: (entry: GlossaryEntry, breaking: boolean) => void;
}) {
  const api = useApi();
  const { locale, onChanged } = props;
  const [rows, setRows] = useState<Row[] | null>(null);
  const [usage, setUsage] = useState<Map<string, TermUsage>>(new Map());
  const [state, setState] = useState<SaveState>("saved");
  const [search, setSearch] = useState("");
  // For the write on leaving the view, which runs after the last render.
  const latest = useRef<Row[] | null>(null);
  const pending = useRef(false);
  useEffect(() => {
    latest.current = rows;
    pending.current = state === "pending";
  }, [rows, state]);

  // Load the language's terms and their usage.
  useEffect(() => {
    let live = true;
    setRows(null);
    void Promise.all([api.getGlossary(locale), api.glossaryUsage(locale)])
      .then(([entries, used]) => {
        if (!live) return;
        setRows(entries.map(withId));
        setUsage(new Map(used.map((u) => [u.term, u])));
        setState("saved");
      })
      .catch((error: unknown) => {
        if (live)
          toast({
            tone: "error",
            title: "Could not load the glossary",
            description: errorMessage(error),
          });
      });
    return () => {
      live = false;
    };
  }, [api, locale]);

  const write = async (entries: Row[]) => {
    setState("saving");
    try {
      await api.setGlossary(locale, entries.filter((e) => e.term.trim() !== "").map(withoutId));
      setUsage(new Map((await api.glossaryUsage(locale)).map((u) => [u.term, u])));
      // A newer edit may have arrived while this one was being written.
      setState((s) => (s === "saving" ? "saved" : s));
      onChanged();
    } catch (error) {
      setState("failed");
      toast({
        tone: "error",
        title: "Could not save the glossary",
        description: errorMessage(error),
      });
    }
  };

  // Write edits after a pause in typing.
  useEffect(() => {
    if (state !== "pending" || rows === null) return;
    const timer = setTimeout(() => void write(rows), SAVE_DELAY_MS);
    return () => clearTimeout(timer);
    // write reads only state setters, api and locale.
    // oxlint-disable-next-line react/exhaustive-deps
  }, [rows, state]);

  // Leaving the view with an edit still waiting writes it.
  useEffect(
    () => () => {
      if (pending.current && latest.current) {
        void api.setGlossary(
          locale,
          latest.current.filter((e) => e.term.trim() !== "").map(withoutId),
        );
      }
    },
    [api, locale],
  );

  const edit = (next: Row[]) => {
    setRows(next);
    setState("pending");
  };
  const update = (rowId: number, patch: Partial<GlossaryEntry>) =>
    edit((rows ?? []).map((e) => (e.rowId === rowId ? { ...e, ...patch } : e)));

  const importTable = async () => {
    try {
      const path = await api.pickTable();
      if (path === null) return;
      const imported = await api.readGlossaryTable(path);
      let added = 0;
      let updated = 0;
      const next = [...(rows ?? [])];
      for (const entry of imported) {
        const i = next.findIndex((e) => e.term === entry.term);
        if (i >= 0) {
          next[i] = { ...entry, rowId: next[i]!.rowId };
          updated += 1;
        } else {
          next.push(withId(entry));
          added += 1;
        }
      }
      edit(next);
      toast({
        tone: "success",
        title: `${plural(added, "term", "terms")} added, ${updated} updated`,
      });
    } catch (error) {
      toast({
        tone: "error",
        title: "Could not import the terms",
        description: errorMessage(error),
      });
    }
  };

  const needle = search.trim().toLowerCase();
  const shown = (rows ?? []).filter(
    (e) =>
      needle === "" ||
      [e.term, e.translation ?? "", e.note ?? ""].some((t) => t.toLowerCase().includes(needle)),
  );

  return (
    <section aria-label="Glossary" className="flex h-full min-h-0 flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b border-line px-3 py-2">
        {props.languages.length > 1 ? (
          <div className="w-40">
            <Select
              aria-label="Language"
              value={locale}
              onValueChange={(v) => v && props.onLocale(v)}
              options={props.languages.map((l) => ({ value: l.locale, label: l.language }))}
            />
          </div>
        ) : (
          <span className="text-sm font-medium">{props.language}</span>
        )}
        <div className="relative min-w-48 flex-1">
          <Search
            aria-hidden
            size={14}
            className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-fg-subtle"
          />
          <Input
            aria-label="Search terms"
            placeholder="Search terms"
            className="pl-7"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <Button
          variant="secondary"
          icon={<Plus aria-hidden size={16} />}
          disabled={rows === null}
          onClick={() => {
            setSearch("");
            edit([
              ...(rows ?? []),
              withId({ term: "", keep: true, translation: null, note: null }),
            ]);
          }}
        >
          Add term
        </Button>
        <Button
          variant="secondary"
          icon={<FileUp aria-hidden size={16} />}
          disabled={rows === null}
          onClick={() => void importTable()}
        >
          Import from a spreadsheet…
        </Button>
        <Button
          variant="primary"
          icon={<WandSparkles aria-hidden size={16} />}
          disabled={!props.canApply || props.breaking === 0 || state !== "saved"}
          onClick={props.onApply}
        >
          {props.breaking > 0
            ? `Apply to ${plural(props.breaking, "string", "strings")}`
            : "Every translation follows it"}
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {rows !== null && rows.length === 0 ? (
          <div className="mx-auto max-w-xl space-y-2 p-6 text-sm text-fg-muted">
            <p>
              No terms yet. Add product names, class names and units the model must keep unchanged,
              or words that must always be translated the same way.
            </p>
            <p>
              Or import a spreadsheet saved as CSV or tab-separated text: term, {props.language}{" "}
              translation, note. A row without a translation keeps the term unchanged.
            </p>
          </div>
        ) : (
          <table className="w-full text-sm">
            <thead className="sticky top-0 z-10 bg-surface text-left text-xs text-fg-subtle">
              <tr>
                <th className="px-2 py-1.5 font-medium">Term</th>
                <th className="px-2 py-1.5 font-medium">Keep unchanged</th>
                <th className="px-2 py-1.5 font-medium">{props.language}</th>
                <th className="px-2 py-1.5 font-medium">Note</th>
                <th className="px-2 py-1.5 text-right font-medium">Used in</th>
                <th className="px-2 py-1.5 text-right font-medium">Breaks it</th>
                <th className="w-8" />
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {shown.map((entry) => {
                const used = usage.get(entry.term);
                return (
                  <tr key={entry.rowId}>
                    <td className="px-2 py-1">
                      <Input
                        aria-label="Term"
                        value={entry.term}
                        onChange={(e) => update(entry.rowId, { term: e.target.value })}
                      />
                    </td>
                    <td className="px-2 py-1">
                      <Checkbox
                        aria-label={`Keep ${entry.term || "this term"} unchanged`}
                        checked={entry.keep}
                        onCheckedChange={(keep) => update(entry.rowId, { keep })}
                      />
                    </td>
                    <td className="px-2 py-1">
                      <Input
                        aria-label={`${props.language} translation`}
                        value={entry.translation ?? ""}
                        disabled={entry.keep}
                        onChange={(e) =>
                          update(entry.rowId, { translation: e.target.value || null })
                        }
                      />
                    </td>
                    <td className="px-2 py-1">
                      <Input
                        aria-label="Note"
                        value={entry.note ?? ""}
                        onChange={(e) => update(entry.rowId, { note: e.target.value || null })}
                      />
                    </td>
                    <td className="px-2 py-1 text-right font-mono tabular-nums">
                      {used && used.strings > 0 ? (
                        <button
                          type="button"
                          className="rounded-control px-1 text-fg-muted underline-offset-2 hover:text-fg hover:underline"
                          aria-label={`Show the ${plural(used.strings, "string", "strings")} using ${entry.term}`}
                          onClick={() => props.onShowStrings(entry, false)}
                        >
                          {used.strings.toLocaleString()}
                        </button>
                      ) : (
                        <span className="text-fg-subtle">0</span>
                      )}
                    </td>
                    <td className="px-2 py-1 text-right font-mono tabular-nums">
                      {used && used.breaking > 0 ? (
                        <button
                          type="button"
                          className="rounded-control px-1 font-medium text-warn underline-offset-2 hover:underline"
                          aria-label={`Show the ${plural(used.breaking, "translation", "translations")} breaking ${entry.term}`}
                          onClick={() => props.onShowStrings(entry, true)}
                        >
                          {used.breaking.toLocaleString()}
                        </button>
                      ) : (
                        <span className="text-fg-subtle">0</span>
                      )}
                    </td>
                    <td className="px-1 py-1">
                      <Button
                        variant="ghost"
                        size="sm"
                        aria-label={`Remove ${entry.term || "term"}`}
                        icon={<Trash2 aria-hidden size={14} />}
                        onClick={() => edit((rows ?? []).filter((e) => e.rowId !== entry.rowId))}
                      />
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      <div className="flex items-center gap-3 border-t border-line px-3 py-1.5 text-xs text-fg-subtle">
        <span>{rows === null ? "…" : plural(rows.length, "term", "terms")}</span>
        <span className="ml-auto" aria-live="polite">
          {
            {
              saved: "Saved in glossary.toml",
              pending: "Editing…",
              saving: "Saving…",
              failed: "Not saved",
            }[state]
          }
        </span>
      </div>
    </section>
  );
}
