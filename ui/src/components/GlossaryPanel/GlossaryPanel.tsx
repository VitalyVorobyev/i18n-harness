import { useCallback, useEffect, useMemo, useState } from "react";
import { cn } from "../../lib/cn";
import {
  listLocales,
  loadGlossary,
  pickGlossaryFile,
  pickGlossarySaveLocation,
  saveGlossary,
} from "../../lib/tauri";
import type {
  GlossaryPayload,
  LocaleInfo,
  LocaleOverrideEntry,
  TermEntry,
} from "../../lib/types";

const REGISTERS = ["formal", "informal", "neutral"] as const;
type Register = (typeof REGISTERS)[number];

interface Props {
  flashError: (msg: string) => void;
  flashInfo: (msg: string) => void;
  /** When set, the panel will auto-load this glossary path on mount (one-shot). */
  initialPath?: string;
}

export function GlossaryPanel({ flashError, flashInfo, initialPath }: Props) {
  const [locales, setLocales] = useState<LocaleInfo[]>([]);
  const [path, setPath] = useState<string | null>(null);
  const [payload, setPayload] = useState<GlossaryPayload | null>(null);
  const [original, setOriginal] = useState<string | null>(null);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [search, setSearch] = useState("");

  useEffect(() => {
    listLocales()
      .then(setLocales)
      .catch((e) => flashError(`Could not list locales: ${formatError(e)}`));
  }, [flashError]);

  // Auto-load: when the project declares a glossary, load it immediately
  // so the translator does not have to click "Open glossary.toml…".
  // The panel is remounted each time a new project is opened, so this effect
  // fires once per project mount. The cleanup flag prevents a slow in-flight
  // promise from overwriting a glossary the user opened manually while the
  // auto-load was still pending.
  useEffect(() => {
    if (!initialPath) return;
    let cancelled = false;
    loadGlossary(initialPath)
      .then((res) => {
        if (cancelled) return;
        setPayload(res.payload);
        setOriginal(JSON.stringify(res.payload));
        setPath(res.path);
        setWarnings(res.warnings);
      })
      .catch((e) => {
        if (cancelled) return;
        flashError(`Could not auto-load glossary: ${formatError(e)}`);
      });
    return () => {
      cancelled = true;
    };
  }, [initialPath, flashError]);

  const dirty = useMemo(() => {
    if (!payload) return false;
    return JSON.stringify(payload) !== original;
  }, [payload, original]);

  const initEmpty = useCallback(() => {
    const fresh: GlossaryPayload = {
      schema_version: 1,
      terms: [],
      locale_overrides: [],
    };
    setPayload(fresh);
    setOriginal(JSON.stringify(fresh));
    setPath(null);
    setWarnings([]);
  }, []);

  const open = useCallback(async () => {
    if (dirty) {
      const ok = window.confirm(
        "Discard unsaved glossary edits and open another file?",
      );
      if (!ok) return;
    }
    try {
      const picked = await pickGlossaryFile();
      if (!picked) return;
      const res = await loadGlossary(picked);
      setPayload(res.payload);
      setOriginal(JSON.stringify(res.payload));
      setPath(res.path);
      setWarnings(res.warnings);
      flashInfo(
        `Loaded ${res.payload.terms.length} term(s) from ${shortPath(res.path)}`,
      );
    } catch (e) {
      flashError(`Open glossary failed: ${formatError(e)}`);
    }
  }, [dirty, flashInfo, flashError]);

  const save = useCallback(async () => {
    if (!payload) return;
    try {
      const target = path ?? (await pickGlossarySaveLocation());
      if (!target) return;
      const res = await saveGlossary(target, payload);
      setPath(res.path);
      setOriginal(JSON.stringify(payload));
      setWarnings(res.warnings);
      flashInfo(
        res.warnings.length === 0
          ? `Saved to ${shortPath(res.path)}`
          : `Saved with ${res.warnings.length} warning(s).`,
      );
    } catch (e) {
      flashError(`Save glossary failed: ${formatError(e)}`);
    }
  }, [payload, path, flashInfo, flashError]);

  const discard = useCallback(() => {
    if (!original) return;
    if (!dirty) return;
    const ok = window.confirm("Discard glossary edits?");
    if (!ok) return;
    setPayload(JSON.parse(original) as GlossaryPayload);
    flashInfo("Reverted glossary edits.");
  }, [original, dirty, flashInfo]);

  const updatePayload = useCallback(
    (updater: (p: GlossaryPayload) => GlossaryPayload) => {
      setPayload((prev) => (prev ? updater(prev) : prev));
    },
    [],
  );

  if (!payload) {
    return (
      <EmptyGlossary
        onOpen={open}
        onCreate={initEmpty}
        flashError={flashError}
      />
    );
  }

  const filteredTerms = filterTerms(payload.terms, search);
  const knownLocaleIds = locales.map((l) => l.id);
  const allLocaleIds = collectLocaleIds(payload, knownLocaleIds);

  return (
    <section className="flex-1 flex flex-col overflow-hidden min-w-0 bg-bg-base">
      <header className="shrink-0 px-5 py-3 border-b border-border-subtle bg-bg-surface flex items-center justify-between gap-4">
        <div className="flex items-center gap-3 min-w-0">
          <span className="text-xs font-semibold uppercase tracking-loose text-fg-tertiary">
            Glossary
          </span>
          {path ? (
            <span
              className="font-mono text-xs text-fg-secondary truncate max-w-[420px]"
              title={path}
            >
              {shortPath(path)}
              {dirty && (
                <span
                  role="img"
                  className="ml-1 text-state-proposed"
                  aria-label="Unsaved changes"
                  title="Unsaved changes"
                >
                  •
                </span>
              )}
            </span>
          ) : (
            <span className="text-xs text-fg-tertiary italic">
              New glossary{dirty ? " · unsaved" : ""}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <Btn onClick={open} title="Open a glossary .toml">
            Open…
          </Btn>
          <Btn
            onClick={save}
            disabled={!dirty}
            primary={dirty}
            title={dirty ? "Save glossary to disk" : "No unsaved changes"}
          >
            Save
          </Btn>
          <Btn
            onClick={discard}
            disabled={!dirty}
            title={dirty ? "Discard unsaved edits" : "No unsaved changes"}
          >
            Discard
          </Btn>
        </div>
      </header>

      <div className="flex-1 overflow-y-auto p-5 flex flex-col gap-5">
        <SectionHead title="Terms" count={payload.terms.length}>
          <input
            type="search"
            placeholder="Filter terms…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            aria-label="Filter terms"
            spellCheck={false}
            className={cn(
              "h-[28px] w-[260px] px-3 rounded-md border bg-bg-input",
              "border-border-default text-sm text-fg-primary",
              "placeholder:text-fg-tertiary transition-colors duration-100 ease-out",
              "focus:border-accent focus:outline-none focus-visible:outline-none",
            )}
          />
          <Btn
            onClick={() =>
              updatePayload((p) => ({
                ...p,
                terms: [
                  { source: "", do_not_translate: false, translations: {} },
                  ...p.terms,
                ],
              }))
            }
            title="Add a new term"
            primary
          >
            + Add term
          </Btn>
        </SectionHead>

        <TermsTable
          terms={filteredTerms}
          localeIds={allLocaleIds}
          knownLocaleIds={knownLocaleIds}
          totalCount={payload.terms.length}
          onChange={(updater) =>
            updatePayload((p) => ({ ...p, terms: updater(p.terms) }))
          }
        />

        <SectionHead
          title="Locale overrides"
          count={payload.locale_overrides.length}
        >
          <Btn
            onClick={() => {
              const existing = new Set(
                payload.locale_overrides.map((o) => o.locale),
              );
              const nextLocale =
                knownLocaleIds.find((id) => !existing.has(id)) ??
                knownLocaleIds[0] ??
                "";
              if (!nextLocale) {
                flashError("No locales available — workspace table is empty.");
                return;
              }
              updatePayload((p) => ({
                ...p,
                locale_overrides: [
                  ...p.locale_overrides,
                  { locale: nextLocale, register: null, variant: null },
                ],
              }));
            }}
            primary
            title="Add a per-locale register / variant override"
          >
            + Add override
          </Btn>
        </SectionHead>

        <OverridesTable
          overrides={payload.locale_overrides}
          knownLocaleIds={knownLocaleIds}
          onChange={(updater) =>
            updatePayload((p) => ({
              ...p,
              locale_overrides: updater(p.locale_overrides),
            }))
          }
        />

        {warnings.length > 0 && (
          <div className="rounded-md border border-state-proposed-border bg-state-proposed-bg p-3">
            <div className="text-xs font-semibold uppercase tracking-loose text-state-proposed mb-1">
              Warnings ({warnings.length})
            </div>
            <ul className="flex flex-col gap-1 text-xs text-fg-secondary">
              {warnings.map((w, i) => (
                <li key={i} className="font-mono">
                  {w}
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </section>
  );
}

function EmptyGlossary({
  onOpen,
  onCreate,
  flashError,
}: {
  onOpen: () => void;
  onCreate: () => void;
  flashError: (msg: string) => void;
}) {
  return (
    <section className="flex-1 flex items-center justify-center bg-bg-base p-12">
      <div className="w-full max-w-[520px] rounded-lg border border-border-subtle bg-bg-surface p-8">
        <div className="text-xs font-semibold uppercase tracking-loose text-accent mb-2">
          Glossary
        </div>
        <h1 className="m-0 mb-2 text-2xl font-semibold tracking-tight text-fg-primary">
          Open or create a glossary
        </h1>
        <p className="m-0 mb-6 text-sm text-fg-secondary leading-[1.65]">
          A glossary is a TOML file with per-term translations and
          do-not-translate markers. The harness uses it to constrain the model's
          vocabulary and to surface gate warnings when target text drifts from
          the project's standard wording.
        </p>
        <div className="flex flex-wrap gap-3">
          <button
            type="button"
            onClick={() => {
              onOpen();
            }}
            className="inline-flex items-center h-9 px-4 rounded-md text-md font-medium text-accent-fg bg-accent transition-colors duration-100 ease-out hover:bg-accent-hover"
          >
            Open glossary.toml…
          </button>
          <button
            type="button"
            onClick={() => {
              try {
                onCreate();
              } catch (e) {
                flashError(`Could not create glossary: ${formatError(e)}`);
              }
            }}
            className="inline-flex items-center h-9 px-4 rounded-md text-md font-medium text-fg-secondary border border-border-default bg-transparent transition-colors duration-100 ease-out hover:bg-bg-hover hover:text-fg-primary"
          >
            New glossary
          </button>
        </div>
      </div>
    </section>
  );
}

function TermsTable({
  terms,
  localeIds,
  knownLocaleIds,
  totalCount,
  onChange,
}: {
  terms: { entry: TermEntry; index: number }[];
  localeIds: string[];
  knownLocaleIds: string[];
  totalCount: number;
  onChange: (updater: (prev: TermEntry[]) => TermEntry[]) => void;
}) {
  if (totalCount === 0) {
    return (
      <div className="text-sm text-fg-tertiary italic">
        No terms yet. Use “+ Add term” above.
      </div>
    );
  }
  if (terms.length === 0) {
    return (
      <div className="text-sm text-fg-tertiary italic">
        No terms match this filter.
      </div>
    );
  }
  return (
    <div className="overflow-x-auto rounded-md border border-border-subtle">
      <table className="w-full border-collapse text-sm">
        <thead>
          <tr className="bg-bg-elevated text-fg-tertiary text-xs uppercase tracking-loose">
            <Th>Source</Th>
            <Th className="w-[60px] text-center">DNT</Th>
            <Th className="w-[200px]">Notes</Th>
            {localeIds.map((id) => (
              <Th key={id} className="min-w-[160px]">
                <span className="font-mono normal-case tracking-normal text-fg-secondary">
                  {id}
                </span>
                {!knownLocaleIds.includes(id) && (
                  <span
                    className="ml-1 text-state-proposed"
                    title="Locale not declared in the workspace locales table"
                  >
                    ?
                  </span>
                )}
              </Th>
            ))}
            <Th className="w-[40px]" />
          </tr>
        </thead>
        <tbody>
          {terms.map(({ entry, index }) => (
            <TermRow
              key={index}
              term={entry}
              localeIds={localeIds}
              onChange={(updater) =>
                onChange((prev) =>
                  prev.map((t, i) => (i === index ? updater(t) : t)),
                )
              }
              onDelete={() =>
                onChange((prev) => prev.filter((_, i) => i !== index))
              }
            />
          ))}
        </tbody>
      </table>
    </div>
  );
}

function TermRow({
  term,
  localeIds,
  onChange,
  onDelete,
}: {
  term: TermEntry;
  localeIds: string[];
  onChange: (updater: (prev: TermEntry) => TermEntry) => void;
  onDelete: () => void;
}) {
  return (
    <tr className="border-t border-border-subtle align-top">
      <Td>
        <TextInput
          value={term.source}
          onChange={(source) => onChange((t) => ({ ...t, source }))}
          mono
        />
      </Td>
      <Td className="text-center">
        <input
          type="checkbox"
          checked={term.do_not_translate}
          onChange={(e) =>
            onChange((t) => ({ ...t, do_not_translate: e.target.checked }))
          }
          className="w-4 h-4 accent-accent cursor-pointer"
          aria-label="Do not translate"
        />
      </Td>
      <Td>
        <TextInput
          value={term.notes ?? ""}
          onChange={(notes) =>
            onChange((t) => ({
              ...t,
              notes: notes.length === 0 ? null : notes,
            }))
          }
        />
      </Td>
      {localeIds.map((id) => (
        <Td key={id}>
          <TextInput
            value={term.translations[id] ?? ""}
            disabled={term.do_not_translate}
            onChange={(v) =>
              onChange((t) => {
                const next = { ...t.translations };
                if (v.length === 0) delete next[id];
                else next[id] = v;
                return { ...t, translations: next };
              })
            }
          />
        </Td>
      ))}
      <Td className="text-center">
        <button
          type="button"
          onClick={onDelete}
          aria-label="Delete term"
          title="Delete term"
          className="text-fg-tertiary hover:text-severity-hard transition-colors duration-100 ease-out p-1 rounded-sm"
        >
          ×
        </button>
      </Td>
    </tr>
  );
}

function OverridesTable({
  overrides,
  knownLocaleIds,
  onChange,
}: {
  overrides: LocaleOverrideEntry[];
  knownLocaleIds: string[];
  onChange: (
    updater: (prev: LocaleOverrideEntry[]) => LocaleOverrideEntry[],
  ) => void;
}) {
  if (overrides.length === 0) {
    return (
      <div className="text-sm text-fg-tertiary italic">
        No per-locale overrides. Defaults from the workspace locales table
        apply.
      </div>
    );
  }
  return (
    <div className="overflow-x-auto rounded-md border border-border-subtle">
      <table className="w-full border-collapse text-sm">
        <thead>
          <tr className="bg-bg-elevated text-fg-tertiary text-xs uppercase tracking-loose">
            <Th className="w-[140px]">Locale</Th>
            <Th className="w-[160px]">Register</Th>
            <Th className="min-w-[160px]">Variant</Th>
            <Th className="w-[40px]" />
          </tr>
        </thead>
        <tbody>
          {overrides.map((o, i) => (
            <tr key={i} className="border-t border-border-subtle align-top">
              <Td>
                <TextInput
                  value={o.locale}
                  onChange={(locale) =>
                    onChange((prev) =>
                      prev.map((x, j) => (j === i ? { ...x, locale } : x)),
                    )
                  }
                  list="known-locales"
                  mono
                />
                <datalist id="known-locales">
                  {knownLocaleIds.map((id) => (
                    <option key={id} value={id} />
                  ))}
                </datalist>
              </Td>
              <Td>
                <select
                  value={o.register ?? ""}
                  onChange={(e) =>
                    onChange((prev) =>
                      prev.map((x, j) =>
                        j === i
                          ? {
                              ...x,
                              register:
                                e.target.value === ""
                                  ? null
                                  : (e.target.value as Register),
                            }
                          : x,
                      ),
                    )
                  }
                  className={cn(
                    "w-full h-[28px] px-2 rounded-sm border bg-bg-input text-sm",
                    "border-border-default text-fg-primary focus:border-accent focus:outline-none",
                  )}
                  aria-label="Register"
                >
                  <option value="">— (default)</option>
                  {REGISTERS.map((r) => (
                    <option key={r} value={r}>
                      {r}
                    </option>
                  ))}
                </select>
              </Td>
              <Td>
                <TextInput
                  value={o.variant ?? ""}
                  onChange={(variant) =>
                    onChange((prev) =>
                      prev.map((x, j) =>
                        j === i
                          ? {
                              ...x,
                              variant: variant.length === 0 ? null : variant,
                            }
                          : x,
                      ),
                    )
                  }
                />
              </Td>
              <Td className="text-center">
                <button
                  type="button"
                  onClick={() =>
                    onChange((prev) => prev.filter((_, j) => j !== i))
                  }
                  aria-label="Delete override"
                  title="Delete override"
                  className="text-fg-tertiary hover:text-severity-hard transition-colors duration-100 ease-out p-1 rounded-sm"
                >
                  ×
                </button>
              </Td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function SectionHead({
  title,
  count,
  children,
}: {
  title: string;
  count?: number;
  children?: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-3">
      <div className="flex items-center gap-2">
        <span className="text-xs font-semibold uppercase tracking-loose text-fg-tertiary">
          {title}
        </span>
        {typeof count === "number" && (
          <span className="font-mono text-xs text-fg-tertiary tabular-nums">
            {count}
          </span>
        )}
      </div>
      <div className="flex items-center gap-2">{children}</div>
    </div>
  );
}

function Th({
  children,
  className,
}: {
  children?: React.ReactNode;
  className?: string;
}) {
  return (
    <th
      className={cn(
        "text-left font-semibold px-3 py-2 border-b border-border-subtle",
        className,
      )}
    >
      {children}
    </th>
  );
}

function Td({
  children,
  className,
}: {
  children?: React.ReactNode;
  className?: string;
}) {
  return <td className={cn("px-2 py-1.5 align-top", className)}>{children}</td>;
}

function TextInput({
  value,
  onChange,
  disabled,
  mono,
  list,
}: {
  value: string;
  onChange: (v: string) => void;
  disabled?: boolean;
  mono?: boolean;
  list?: string;
}) {
  return (
    <input
      type="text"
      value={value}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
      list={list}
      spellCheck={false}
      className={cn(
        "w-full h-[28px] px-2 rounded-sm border bg-bg-input text-sm",
        "border-border-default text-fg-primary",
        "transition-colors duration-100 ease-out",
        "focus:border-accent focus:outline-none focus-visible:outline-none",
        "disabled:bg-bg-surface disabled:text-fg-disabled disabled:cursor-not-allowed",
        mono && "font-mono",
      )}
    />
  );
}

function Btn({
  children,
  onClick,
  disabled,
  primary,
  title,
}: {
  children: React.ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  primary?: boolean;
  title?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={cn(
        "inline-flex items-center gap-2 h-7 px-3 rounded-md text-sm font-medium border",
        "transition-colors duration-100 ease-out",
        primary
          ? "text-accent-fg bg-accent border-transparent enabled:hover:bg-accent-hover enabled:active:bg-accent-active"
          : "text-fg-secondary border-border-default bg-transparent enabled:hover:bg-bg-hover enabled:hover:text-fg-primary",
        "disabled:text-fg-disabled disabled:border-border-subtle disabled:cursor-not-allowed disabled:bg-transparent",
      )}
    >
      {children}
    </button>
  );
}

function filterTerms(
  terms: TermEntry[],
  search: string,
): { entry: TermEntry; index: number }[] {
  const lower = search.trim().toLowerCase();
  return terms
    .map((entry, index) => ({ entry, index }))
    .filter(({ entry }) => {
      if (!lower) return true;
      if (entry.source.toLowerCase().includes(lower)) return true;
      if (entry.notes?.toLowerCase().includes(lower)) return true;
      return Object.values(entry.translations).some((v) =>
        v.toLowerCase().includes(lower),
      );
    });
}

function collectLocaleIds(
  payload: GlossaryPayload,
  knownLocaleIds: string[],
): string[] {
  const set = new Set<string>(knownLocaleIds);
  for (const term of payload.terms) {
    for (const id of Object.keys(term.translations)) {
      set.add(id);
    }
  }
  for (const ov of payload.locale_overrides) {
    set.add(ov.locale);
  }
  return Array.from(set).sort();
}

function shortPath(p: string): string {
  if (p.length <= 60) return p;
  const parts = p.split(/[\\/]/);
  if (parts.length <= 3) return p;
  return `…/${parts.slice(-3).join("/")}`;
}

function formatError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  return String(e);
}
