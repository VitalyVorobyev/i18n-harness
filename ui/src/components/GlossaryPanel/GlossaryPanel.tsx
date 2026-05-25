import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
import { Eyebrow } from "../primitives/Eyebrow";
import { LocaleTag } from "../primitives/LocaleTag";
import { ProgressBar } from "../primitives/ProgressBar";

const REGISTERS = ["formal", "informal", "neutral"] as const;
type Register = (typeof REGISTERS)[number];

type ViewFilter = "all" | "dnt" | "missing" | "overrides";

interface Props {
  flashError: (msg: string) => void;
  flashInfo: (msg: string) => void;
  /** When set, the panel will auto-load this glossary path on mount (one-shot). */
  initialPath?: string;
  /** Navigate to Translate panel, select a unit. */
  onOpenInTranslate?: (unitId: string) => void;
}

// ── Top-level loader — always renders hooks unconditionally ───────────────────

export function GlossaryPanel({
  flashError,
  flashInfo,
  initialPath,
  onOpenInTranslate,
}: Props) {
  const [locales, setLocales] = useState<LocaleInfo[]>([]);
  const [path, setPath] = useState<string | null>(null);
  const [payload, setPayload] = useState<GlossaryPayload | null>(null);
  const [original, setOriginal] = useState<string | null>(null);
  const [warnings, setWarnings] = useState<string[]>([]);

  useEffect(() => {
    listLocales()
      .then(setLocales)
      .catch((e) => flashError(`Could not list locales: ${formatError(e)}`));
  }, [flashError]);

  // Auto-load: when the project declares a glossary, load it immediately.
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

  const open = useCallback(
    async (_currentPayload: GlossaryPayload | null, isDirty: boolean) => {
      if (isDirty) {
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
    },
    [flashInfo, flashError],
  );

  const save = useCallback(
    async (currentPayload: GlossaryPayload, currentPath: string | null) => {
      try {
        const target = currentPath ?? (await pickGlossarySaveLocation());
        if (!target) return;
        const res = await saveGlossary(target, currentPayload);
        setPath(res.path);
        setOriginal(JSON.stringify(currentPayload));
        setWarnings(res.warnings);
        flashInfo(
          res.warnings.length === 0
            ? `Saved to ${shortPath(res.path)}`
            : `Saved with ${res.warnings.length} warning(s).`,
        );
      } catch (e) {
        flashError(`Save glossary failed: ${formatError(e)}`);
      }
    },
    [flashInfo, flashError],
  );

  if (!payload) {
    return (
      <EmptyGlossary
        onOpen={() => void open(null, dirty)}
        onCreate={initEmpty}
        flashError={flashError}
      />
    );
  }

  return (
    <GlossaryEditor
      locales={locales}
      path={path}
      payload={payload}
      warnings={warnings}
      dirty={dirty}
      onPayloadChange={setPayload}
      onOpen={() => void open(payload, dirty)}
      onSave={() => void save(payload, path)}
      onOpenInTranslate={onOpenInTranslate}
    />
  );
}

// ── Editor — rendered only when payload is non-null ───────────────────────────

interface EditorProps {
  locales: LocaleInfo[];
  path: string | null;
  payload: GlossaryPayload;
  warnings: string[];
  dirty: boolean;
  onPayloadChange: (p: GlossaryPayload) => void;
  onOpen: () => void;
  onSave: () => void;
  onOpenInTranslate?: (unitId: string) => void;
}

function GlossaryEditor({
  locales,
  path,
  payload,
  warnings,
  dirty,
  onPayloadChange,
  onOpen,
  onSave,
  onOpenInTranslate,
}: EditorProps) {
  const [search, setSearch] = useState("");
  const [viewFilter, setViewFilter] = useState<ViewFilter>("all");
  const [selectedIndex, setSelectedIndex] = useState<number | null>(
    payload.terms.length > 0 ? 0 : null,
  );
  const sourceInputRef = useRef<HTMLInputElement | null>(null);

  const knownLocaleIds = useMemo(() => locales.map((l) => l.id), [locales]);
  const allLocaleIds = useMemo(
    () => collectLocaleIds(payload, knownLocaleIds),
    [payload, knownLocaleIds],
  );

  const filteredTerms = useMemo(
    () => buildFilteredTerms(payload.terms, search, viewFilter, allLocaleIds),
    [payload.terms, search, viewFilter, allLocaleIds],
  );

  // Clamp selectedIndex when filter shrinks the list
  const safeSelectedIndex: number | null = useMemo(() => {
    if (selectedIndex !== null && selectedIndex < filteredTerms.length)
      return selectedIndex;
    return filteredTerms.length > 0 ? 0 : null;
  }, [selectedIndex, filteredTerms.length]);

  const selectedEntry: FilteredTerm | null = useMemo(
    () =>
      safeSelectedIndex !== null
        ? (filteredTerms[safeSelectedIndex] ?? null)
        : null,
    [safeSelectedIndex, filteredTerms],
  );

  const updatePayload = useCallback(
    (updater: (p: GlossaryPayload) => GlossaryPayload) => {
      onPayloadChange(updater(payload));
    },
    [payload, onPayloadChange],
  );

  const updateSelectedTerm = useCallback(
    (updater: (t: TermEntry) => TermEntry) => {
      if (selectedEntry === null) return;
      const realIndex = selectedEntry.index;
      updatePayload((p) => ({
        ...p,
        terms: p.terms.map((t, i) => (i === realIndex ? updater(t) : t)),
      }));
    },
    [selectedEntry, updatePayload],
  );

  const deleteTerm = useCallback(
    (realIndex: number) => {
      const ok = window.confirm("Delete this term from the glossary?");
      if (!ok) return;
      updatePayload((p) => ({
        ...p,
        terms: p.terms.filter((_, i) => i !== realIndex),
      }));
      setSelectedIndex((prev) => {
        if (prev === null) return null;
        if (prev >= filteredTerms.length - 1) return Math.max(0, prev - 1);
        return prev;
      });
    },
    [updatePayload, filteredTerms.length],
  );

  const addTerm = useCallback(() => {
    updatePayload((p) => ({
      ...p,
      terms: [
        { source: "", do_not_translate: false, translations: {} },
        ...p.terms,
      ],
    }));
    setSearch("");
    setViewFilter("all");
    setSelectedIndex(0);
    setTimeout(() => {
      sourceInputRef.current?.focus();
      sourceInputRef.current?.select();
    }, 30);
  }, [updatePayload]);

  const updateLocaleOverride = useCallback(
    (updater: (prev: LocaleOverrideEntry[]) => LocaleOverrideEntry[]) => {
      updatePayload((p) => ({
        ...p,
        locale_overrides: updater(p.locale_overrides),
      }));
    },
    [updatePayload],
  );

  // ⌘S / Ctrl+S
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        onSave();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onSave]);

  // Locale coverage stats (over all terms, not filtered)
  const localeCoverage = useMemo(
    () =>
      allLocaleIds.map((id) => {
        const translatable = payload.terms.filter(
          (t) => !t.do_not_translate,
        ).length;
        const covered = payload.terms.filter(
          (t) => !t.do_not_translate && !!t.translations[id],
        ).length;
        return { id, covered, total: translatable };
      }),
    [payload.terms, allLocaleIds],
  );

  const dntCount = useMemo(
    () => payload.terms.filter((t) => t.do_not_translate).length,
    [payload.terms],
  );
  const missingCount = useMemo(
    () =>
      payload.terms.filter(
        (t) =>
          !t.do_not_translate && allLocaleIds.some((id) => !t.translations[id]),
      ).length,
    [payload.terms, allLocaleIds],
  );
  const overridesCount = payload.locale_overrides.length;

  return (
    <section
      style={{ display: "flex", flex: 1, overflow: "hidden", minWidth: 0 }}
    >
      {/* ── Glossary nav rail (240px) ──────────────────────────────────────── */}
      <GlossaryNavRail
        path={path}
        viewFilter={viewFilter}
        onViewFilter={setViewFilter}
        termCount={payload.terms.length}
        dntCount={dntCount}
        missingCount={missingCount}
        overridesCount={overridesCount}
        localeCoverage={localeCoverage}
        dirty={dirty}
        onOpen={onOpen}
        warnings={warnings}
      />

      {/* ── Term list (280px) ──────────────────────────────────────────────── */}
      <GlossaryTermList
        terms={filteredTerms}
        totalCount={payload.terms.length}
        allLocaleIds={allLocaleIds}
        search={search}
        onSearchChange={setSearch}
        selectedIndex={safeSelectedIndex}
        onSelect={setSelectedIndex}
        onAddTerm={addTerm}
      />

      {/* ── Term detail (flex-1) ───────────────────────────────────────────── */}
      {selectedEntry !== null ? (
        <SelectedTermDetail
          entry={selectedEntry}
          allLocaleIds={allLocaleIds}
          localeOverrides={payload.locale_overrides}
          sourceInputRef={sourceInputRef}
          onUpdateTerm={updateSelectedTerm}
          onDeleteTerm={deleteTerm}
          onUpdateLocaleOverride={updateLocaleOverride}
          onOpenInTranslate={onOpenInTranslate}
          dirty={dirty}
          onSave={onSave}
        />
      ) : (
        <GlossaryDetailEmpty
          hasTerms={payload.terms.length > 0}
          onAddTerm={addTerm}
        />
      )}
    </section>
  );
}

// ── Thin wrapper to keep JSX in GlossaryEditor clean ─────────────────────────

function SelectedTermDetail({
  entry,
  allLocaleIds,
  localeOverrides,
  sourceInputRef,
  onUpdateTerm,
  onDeleteTerm,
  onUpdateLocaleOverride,
  onOpenInTranslate,
  dirty,
  onSave,
}: {
  entry: FilteredTerm;
  allLocaleIds: string[];
  localeOverrides: LocaleOverrideEntry[];
  sourceInputRef: React.RefObject<HTMLInputElement | null>;
  onUpdateTerm: (updater: (t: TermEntry) => TermEntry) => void;
  onDeleteTerm: (realIndex: number) => void;
  onUpdateLocaleOverride: (
    updater: (prev: LocaleOverrideEntry[]) => LocaleOverrideEntry[],
  ) => void;
  onOpenInTranslate?: (unitId: string) => void;
  dirty: boolean;
  onSave: () => void;
}) {
  return (
    <GlossaryTermDetail
      term={entry.entry}
      allLocaleIds={allLocaleIds}
      localeOverrides={localeOverrides}
      sourceInputRef={sourceInputRef}
      onUpdateTerm={onUpdateTerm}
      onDeleteTerm={() => onDeleteTerm(entry.index)}
      onUpdateLocaleOverride={onUpdateLocaleOverride}
      onOpenInTranslate={onOpenInTranslate}
      dirty={dirty}
      onSave={onSave}
    />
  );
}

// ── Glossary nav rail ──────────────────────────────────────────────────────────

function GlossaryNavRail({
  path,
  viewFilter,
  onViewFilter,
  termCount,
  dntCount,
  missingCount,
  overridesCount,
  localeCoverage,
  dirty,
  onOpen,
  warnings,
}: {
  path: string | null;
  viewFilter: ViewFilter;
  onViewFilter: (v: ViewFilter) => void;
  termCount: number;
  dntCount: number;
  missingCount: number;
  overridesCount: number;
  localeCoverage: { id: string; covered: number; total: number }[];
  dirty: boolean;
  onOpen: () => void;
  warnings: string[];
}) {
  return (
    <aside
      style={{
        width: 240,
        display: "flex",
        flexDirection: "column",
        flexShrink: 0,
        background: "var(--color-bg-surface)",
        borderRight: "1px solid var(--color-border-subtle)",
        overflow: "hidden",
      }}
    >
      {/* Identity block */}
      <div style={{ padding: "14px 14px 10px" }}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            marginBottom: 4,
          }}
        >
          <BookOpenIcon />
          <span
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: "var(--color-fg-primary)",
            }}
          >
            Glossary
          </span>
        </div>
        {path ? (
          <span
            title={path}
            style={{
              display: "block",
              fontFamily: "var(--font-mono)",
              fontSize: 10.5,
              color: "var(--color-fg-tertiary)",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              paddingLeft: 2,
              marginBottom: 8,
            }}
          >
            {shortPath(path)}
          </span>
        ) : (
          <span
            style={{
              display: "block",
              fontSize: 10.5,
              fontStyle: "italic",
              color: "var(--color-fg-tertiary)",
              paddingLeft: 2,
              marginBottom: 8,
            }}
          >
            New glossary
          </span>
        )}

        {/* View links */}
        <ViewLink
          icon={<ListIcon />}
          label="All terms"
          count={termCount}
          active={viewFilter === "all"}
          onClick={() => onViewFilter("all")}
        />
        <ViewLink
          icon={<FlagIcon />}
          label="Do-not-translate"
          count={dntCount}
          active={viewFilter === "dnt"}
          onClick={() => onViewFilter("dnt")}
        />
        <ViewLink
          icon={<AlertIcon />}
          label="Missing translation"
          count={missingCount}
          active={viewFilter === "missing"}
          onClick={() => onViewFilter("missing")}
          tint={missingCount > 0 ? "warn" : "default"}
        />
        <ViewLink
          icon={<GlobeIcon />}
          label="Per-locale overrides"
          count={overridesCount}
          active={viewFilter === "overrides"}
          onClick={() => onViewFilter("overrides")}
        />
      </div>

      <div style={{ borderTop: "1px solid var(--color-border-subtle)" }} />

      {/* Locale coverage */}
      <div
        style={{
          padding: "12px 14px",
          flex: 1,
          overflow: "auto",
        }}
      >
        <div style={{ marginBottom: 8 }}>
          <Eyebrow>Locale coverage</Eyebrow>
        </div>
        {localeCoverage.map(({ id, covered, total }) => (
          <div
            key={id}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              padding: "4px 0",
            }}
          >
            <LocaleTag locale={id} tone="muted" />
            <div style={{ flex: 1, minWidth: 0 }}>
              <ProgressBar
                total={total}
                finished={covered}
                proposed={0}
                height={3}
              />
            </div>
            <span
              style={{
                fontFamily: "var(--font-mono)",
                fontSize: 10.5,
                color: "var(--color-fg-tertiary)",
                whiteSpace: "nowrap",
              }}
            >
              {covered}/{total}
            </span>
          </div>
        ))}
      </div>

      {/* Footer: open / dirty signal */}
      <div
        style={{
          marginTop: "auto",
          padding: "10px 14px",
          borderTop: "1px solid var(--color-border-subtle)",
          display: "flex",
          flexDirection: "column",
          gap: 6,
        }}
      >
        {dirty && (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 5,
              fontSize: 11.5,
              color: "var(--color-state-proposed)",
            }}
          >
            <span
              aria-hidden="true"
              style={{
                width: 7,
                height: 7,
                borderRadius: 999,
                background: "var(--color-state-proposed)",
                flexShrink: 0,
                display: "inline-block",
              }}
            />
            Unsaved changes
          </div>
        )}
        {warnings.length > 0 && (
          <div
            style={{
              fontSize: 11,
              color: "var(--color-severity-soft)",
            }}
          >
            {warnings.length} warning{warnings.length !== 1 ? "s" : ""}
          </div>
        )}
        <button
          type="button"
          onClick={onOpen}
          style={{
            alignSelf: "flex-start",
            height: 26,
            paddingInline: 10,
            border: "1px solid var(--color-border-default)",
            borderRadius: 5,
            background: "transparent",
            fontSize: 12,
            color: "var(--color-fg-secondary)",
            cursor: "pointer",
          }}
        >
          Open other…
        </button>
      </div>
    </aside>
  );
}

function ViewLink({
  icon,
  label,
  count,
  active,
  onClick,
  tint = "default",
}: {
  icon: React.ReactNode;
  label: string;
  count: number;
  active: boolean;
  onClick: () => void;
  tint?: "default" | "warn";
}) {
  const fg = active ? "var(--color-fg-primary)" : "var(--color-fg-secondary)";
  const iconColor = active
    ? "var(--color-accent)"
    : tint === "warn"
      ? "var(--color-severity-soft)"
      : "var(--color-fg-tertiary)";

  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        width: "100%",
        height: 26,
        padding: "0 8px",
        borderRadius: 4,
        border: "none",
        background: active ? "var(--color-bg-selected)" : "transparent",
        cursor: "pointer",
        textAlign: "left",
      }}
      aria-current={active ? "page" : undefined}
    >
      <span
        style={{ color: iconColor, flexShrink: 0, display: "flex" }}
        aria-hidden="true"
      >
        {icon}
      </span>
      <span
        style={{
          flex: 1,
          fontSize: 12.5,
          color: fg,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {label}
      </span>
      <span
        style={{
          fontFamily: "var(--font-mono)",
          fontSize: 10.5,
          color: "var(--color-fg-tertiary)",
        }}
      >
        {count}
      </span>
    </button>
  );
}

// ── Term list (280px) ──────────────────────────────────────────────────────────

function GlossaryTermList({
  terms,
  totalCount,
  allLocaleIds,
  search,
  onSearchChange,
  selectedIndex,
  onSelect,
  onAddTerm,
}: {
  terms: FilteredTerm[];
  totalCount: number;
  allLocaleIds: string[];
  search: string;
  onSearchChange: (v: string) => void;
  selectedIndex: number | null;
  onSelect: (i: number) => void;
  onAddTerm: () => void;
}) {
  return (
    <div
      style={{
        width: 280,
        flexShrink: 0,
        display: "flex",
        flexDirection: "column",
        background: "var(--color-bg-surface)",
        borderRight: "1px solid var(--color-border-subtle)",
        overflow: "hidden",
      }}
    >
      {/* Search + add strip */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "10px 12px",
          borderBottom: "1px solid var(--color-border-subtle)",
        }}
      >
        <div style={{ position: "relative", flex: 1 }}>
          <span
            style={{
              position: "absolute",
              left: 8,
              top: "50%",
              transform: "translateY(-50%)",
              color: "var(--color-fg-tertiary)",
              display: "flex",
              pointerEvents: "none",
            }}
            aria-hidden="true"
          >
            <SearchIcon />
          </span>
          <input
            type="search"
            placeholder="Filter…"
            value={search}
            onChange={(e) => onSearchChange(e.target.value)}
            aria-label="Filter terms"
            spellCheck={false}
            style={{
              width: "100%",
              height: 28,
              paddingLeft: 28,
              paddingRight: 8,
              borderRadius: 5,
              border: "1px solid var(--color-border-default)",
              background: "var(--color-bg-input)",
              color: "var(--color-fg-primary)",
              fontSize: 12.5,
              outline: "none",
              boxSizing: "border-box",
            }}
          />
        </div>
        <button
          type="button"
          onClick={onAddTerm}
          title="Add term"
          aria-label="Add term"
          style={{
            flexShrink: 0,
            width: 28,
            height: 28,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            border: "1px solid var(--color-border-default)",
            borderRadius: 5,
            background: "transparent",
            color: "var(--color-fg-secondary)",
            cursor: "pointer",
          }}
        >
          <PlusIcon />
        </button>
      </div>

      {/* Eyebrow + count */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "8px 14px",
        }}
      >
        <Eyebrow>Terms</Eyebrow>
        <span
          style={{
            fontFamily: "var(--font-mono)",
            fontSize: 10.5,
            color: "var(--color-fg-tertiary)",
            marginLeft: "auto",
          }}
        >
          {terms.length}
          {terms.length !== totalCount && `/${totalCount}`}
        </span>
      </div>

      {/* Scrollable rows */}
      <div
        style={{ flex: 1, overflowY: "auto", overflowX: "hidden" }}
        role="listbox"
        aria-label="Terms"
      >
        {terms.length === 0 && (
          <div
            style={{
              padding: "12px 14px",
              fontSize: 12,
              fontStyle: "italic",
              color: "var(--color-fg-tertiary)",
            }}
          >
            No terms match this filter.
          </div>
        )}
        {terms.map((item, i) => (
          <TermListRow
            key={item.index}
            term={item.entry}
            allLocaleIds={allLocaleIds}
            selected={selectedIndex === i}
            onClick={() => onSelect(i)}
          />
        ))}
      </div>
    </div>
  );
}

function TermListRow({
  term,
  allLocaleIds,
  selected,
  onClick,
}: {
  term: TermEntry;
  allLocaleIds: string[];
  selected: boolean;
  onClick: () => void;
}) {
  const covered = allLocaleIds.filter(
    (id) => !!term.translations[id] || term.do_not_translate,
  ).length;
  const total = allLocaleIds.length;
  const missingLocales = term.do_not_translate
    ? []
    : allLocaleIds.filter((id) => !term.translations[id]);

  return (
    <div
      role="option"
      aria-selected={selected}
      onClick={onClick}
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick();
        }
      }}
      style={{
        padding: "9px 14px",
        borderLeft: selected
          ? "2px solid var(--color-accent)"
          : "2px solid transparent",
        background: selected ? "var(--color-bg-selected)" : "transparent",
        borderBottom: "1px solid var(--color-border-subtle)",
        cursor: "pointer",
        outline: "none",
      }}
    >
      {/* Source + DNT + coverage */}
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          gap: 6,
          marginBottom: 4,
        }}
      >
        <span
          style={{
            fontFamily: "var(--font-mono)",
            fontSize: 13,
            fontWeight: 500,
            color: "var(--color-fg-primary)",
            flex: 1,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {term.source || (
            <span
              style={{
                color: "var(--color-fg-tertiary)",
                fontStyle: "italic",
              }}
            >
              (empty)
            </span>
          )}
        </span>
        {term.do_not_translate && (
          <span
            style={{
              fontSize: 9,
              fontWeight: 600,
              letterSpacing: "0.05em",
              padding: "1px 4px",
              borderRadius: 3,
              background: "var(--color-severity-hard-bg)",
              border: "1px solid var(--color-severity-hard-border)",
              color: "var(--color-severity-hard)",
              textTransform: "uppercase",
              flexShrink: 0,
            }}
          >
            DNT
          </span>
        )}
        <span
          style={{
            fontFamily: "var(--font-mono)",
            fontSize: 10.5,
            color: "var(--color-fg-tertiary)",
            flexShrink: 0,
          }}
        >
          {covered}/{total}
        </span>
      </div>

      {/* Per-locale dots */}
      {allLocaleIds.length > 0 && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 3,
            marginBottom: 3,
            flexWrap: "wrap",
          }}
        >
          {allLocaleIds.map((id) => {
            const filled = term.do_not_translate || !!term.translations[id];
            return (
              <span
                key={id}
                title={id}
                aria-hidden="true"
                style={{
                  width: 6,
                  height: 6,
                  borderRadius: 999,
                  background: filled
                    ? "var(--color-state-finished)"
                    : "var(--color-state-untranslated)",
                  flexShrink: 0,
                }}
              />
            );
          })}
          {missingLocales.length > 0 && (
            <span
              style={{
                fontSize: 10.5,
                color: "var(--color-fg-tertiary)",
                marginLeft: 2,
                fontFamily: "var(--font-mono)",
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
                maxWidth: 160,
              }}
              title={`missing: ${missingLocales.join(", ")}`}
            >
              missing: {missingLocales.join(", ")}
            </span>
          )}
        </div>
      )}

      {/* Note (italic, truncated) */}
      {term.notes && (
        <div
          style={{
            fontSize: 11,
            fontStyle: "italic",
            color: "var(--color-fg-tertiary)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {term.notes}
        </div>
      )}
    </div>
  );
}

// ── Term detail ────────────────────────────────────────────────────────────────

function GlossaryTermDetail({
  term,
  allLocaleIds,
  localeOverrides,
  sourceInputRef,
  onUpdateTerm,
  onDeleteTerm,
  onUpdateLocaleOverride,
  onOpenInTranslate,
  dirty,
  onSave,
}: {
  term: TermEntry;
  allLocaleIds: string[];
  localeOverrides: LocaleOverrideEntry[];
  sourceInputRef: React.RefObject<HTMLInputElement | null>;
  onUpdateTerm: (updater: (t: TermEntry) => TermEntry) => void;
  onDeleteTerm: () => void;
  onUpdateLocaleOverride: (
    updater: (prev: LocaleOverrideEntry[]) => LocaleOverrideEntry[],
  ) => void;
  onOpenInTranslate?: (unitId: string) => void;
  dirty: boolean;
  onSave: () => void;
}) {
  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        overflow: "auto",
        background: "var(--color-bg-base)",
        minWidth: 0,
      }}
    >
      {/* Term header */}
      <div
        style={{
          padding: "18px 28px",
          borderBottom: "1px solid var(--color-border-subtle)",
          background: "var(--color-bg-surface)",
          display: "flex",
          alignItems: "flex-start",
          gap: 16,
        }}
      >
        <div style={{ flex: 1, minWidth: 0 }}>
          <Eyebrow>Term</Eyebrow>
          <div
            style={{
              display: "flex",
              alignItems: "baseline",
              gap: 12,
              marginTop: 4,
              flexWrap: "wrap",
            }}
          >
            <input
              ref={sourceInputRef}
              type="text"
              value={term.source}
              onChange={(e) =>
                onUpdateTerm((t) => ({ ...t, source: e.target.value }))
              }
              spellCheck={false}
              aria-label="Term source"
              placeholder="Enter term…"
              style={{
                fontFamily: "var(--font-mono)",
                fontSize: 22,
                fontWeight: 500,
                color: "var(--color-fg-primary)",
                background: "transparent",
                border: "none",
                outline: "none",
                padding: 0,
                minWidth: 0,
                width: "auto",
                maxWidth: 340,
              }}
            />
            <span
              style={{
                fontFamily: "var(--font-mono)",
                fontSize: 11,
                color: "var(--color-fg-tertiary)",
              }}
            >
              {/* refs placeholder — no IPC for references yet */}— refs not
              loaded
            </span>
          </div>
        </div>

        {/* Controls */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 10,
            flexShrink: 0,
          }}
        >
          <label
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              fontSize: 12.5,
              color: "var(--color-fg-secondary)",
              cursor: "pointer",
              userSelect: "none",
            }}
          >
            <input
              type="checkbox"
              checked={term.do_not_translate}
              onChange={(e) =>
                onUpdateTerm((t) => ({
                  ...t,
                  do_not_translate: e.target.checked,
                }))
              }
              style={{
                accentColor: "var(--color-accent)",
                width: 14,
                height: 14,
              }}
            />
            Do not translate
          </label>

          <button
            type="button"
            onClick={onDeleteTerm}
            aria-label="Delete term"
            style={{
              display: "flex",
              alignItems: "center",
              gap: 5,
              height: 28,
              paddingInline: 10,
              border: "1px solid var(--color-border-default)",
              borderRadius: 5,
              background: "transparent",
              fontSize: 12,
              color: "var(--color-severity-hard)",
              cursor: "pointer",
            }}
          >
            <TrashIcon />
            Delete
          </button>

          {dirty && (
            <button
              type="button"
              onClick={onSave}
              aria-label="Save glossary (⌘S)"
              title="Save (⌘S)"
              style={{
                display: "flex",
                alignItems: "center",
                gap: 5,
                height: 28,
                paddingInline: 10,
                border: "none",
                borderRadius: 5,
                background: "var(--color-accent)",
                color: "var(--color-accent-fg)",
                fontSize: 12,
                fontWeight: 500,
                cursor: "pointer",
              }}
            >
              Save
            </button>
          )}
        </div>
      </div>

      {/* Detail body */}
      <div
        style={{
          padding: "20px 28px 40px",
          maxWidth: 720,
          display: "flex",
          flexDirection: "column",
          gap: 24,
        }}
      >
        {/* Note field */}
        <DetailField label="Note">
          <input
            type="text"
            value={term.notes ?? ""}
            onChange={(e) =>
              onUpdateTerm((t) => ({
                ...t,
                notes: e.target.value.length === 0 ? null : e.target.value,
              }))
            }
            placeholder="Contextual note for the translator…"
            spellCheck={false}
            style={{
              width: "100%",
              height: 32,
              paddingInline: 10,
              borderRadius: 5,
              border: "1px solid var(--color-border-default)",
              background: "var(--color-bg-input)",
              color: "var(--color-fg-primary)",
              fontSize: 13,
              outline: "none",
              boxSizing: "border-box",
            }}
          />
        </DetailField>

        {/* Translations block */}
        <div>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              marginBottom: 10,
            }}
          >
            <Eyebrow>Translations</Eyebrow>
            <span
              style={{
                fontSize: 12,
                color: "var(--color-fg-tertiary)",
              }}
            >
              One row per project locale.
            </span>
          </div>
          {allLocaleIds.length === 0 ? (
            <div
              style={{
                fontSize: 12.5,
                fontStyle: "italic",
                color: "var(--color-fg-tertiary)",
              }}
            >
              No locales configured in this project.
            </div>
          ) : (
            <div
              style={{
                border: "1px solid var(--color-border-subtle)",
                borderRadius: 7,
                background: "var(--color-bg-surface)",
                overflow: "hidden",
              }}
            >
              {allLocaleIds.map((id, i) => (
                <TranslationRow
                  key={id}
                  locale={id}
                  value={term.translations[id] ?? ""}
                  disabled={term.do_not_translate}
                  sourceTerm={term.source}
                  last={i === allLocaleIds.length - 1}
                  onChange={(v) =>
                    onUpdateTerm((t) => {
                      const next = { ...t.translations };
                      if (v.length === 0) delete next[id];
                      else next[id] = v;
                      return { ...t, translations: next };
                    })
                  }
                />
              ))}
            </div>
          )}
        </div>

        {/* Per-locale overrides block */}
        <div>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              marginBottom: 10,
            }}
          >
            <Eyebrow>Per-locale overrides</Eyebrow>
            <button
              type="button"
              onClick={() => {
                const existing = new Set(localeOverrides.map((o) => o.locale));
                const nextLocale =
                  allLocaleIds.find((id) => !existing.has(id)) ??
                  allLocaleIds[0] ??
                  "";
                onUpdateLocaleOverride((prev) => [
                  ...prev,
                  { locale: nextLocale, register: null, variant: null },
                ]);
              }}
              style={{
                marginLeft: "auto",
                height: 24,
                paddingInline: 8,
                border: "1px solid var(--color-border-default)",
                borderRadius: 4,
                background: "transparent",
                fontSize: 11.5,
                color: "var(--color-fg-secondary)",
                cursor: "pointer",
              }}
            >
              + Add override
            </button>
          </div>
          {localeOverrides.length === 0 ? (
            <div
              style={{
                padding: "12px 14px",
                border: "1px dashed var(--color-border-default)",
                borderRadius: 7,
                fontSize: 12.5,
                fontStyle: "italic",
                color: "var(--color-fg-tertiary)",
              }}
            >
              No per-locale overrides. Defaults from the workspace locales table
              apply.
            </div>
          ) : (
            <div
              style={{
                border: "1px solid var(--color-border-subtle)",
                borderRadius: 7,
                background: "var(--color-bg-surface)",
                overflow: "hidden",
              }}
            >
              {localeOverrides.map((o, i) => (
                <OverrideRow
                  key={`override-${i}`}
                  override={o}
                  last={i === localeOverrides.length - 1}
                  allLocaleIds={allLocaleIds}
                  onChange={(updated) =>
                    onUpdateLocaleOverride((prev) =>
                      prev.map((x, j) => (j === i ? updated : x)),
                    )
                  }
                  onDelete={() =>
                    onUpdateLocaleOverride((prev) =>
                      prev.filter((_, j) => j !== i),
                    )
                  }
                />
              ))}
            </div>
          )}
        </div>

        {/* References block */}
        <div>
          <div style={{ marginBottom: 10 }}>
            <Eyebrow>References</Eyebrow>
          </div>
          <div
            style={{
              padding: "12px 14px",
              border: "1px dashed var(--color-border-default)",
              borderRadius: 7,
              fontSize: 12.5,
              fontStyle: "italic",
              color: "var(--color-fg-tertiary)",
            }}
          >
            {onOpenInTranslate
              ? "No reference data available. References will appear here once the catalog index is built."
              : "Open this glossary from within a project to see which units reference this term."}
          </div>
        </div>
      </div>
    </div>
  );
}

function TranslationRow({
  locale,
  value,
  disabled,
  sourceTerm,
  last,
  onChange,
}: {
  locale: string;
  value: string;
  disabled: boolean;
  sourceTerm: string;
  last: boolean;
  onChange: (v: string) => void;
}) {
  const missing = !disabled && value.length === 0;
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        padding: "10px 14px",
        borderBottom: last ? "none" : "1px solid var(--color-border-subtle)",
      }}
    >
      <LocaleTag locale={locale} />
      <span
        style={{
          fontSize: 10,
          fontWeight: 600,
          letterSpacing: "0.05em",
          textTransform: "uppercase",
          padding: "1px 5px",
          borderRadius: 3,
          background: missing
            ? "var(--color-state-untranslated-bg)"
            : "var(--color-state-finished-bg)",
          border: `1px solid ${missing ? "var(--color-state-untranslated-border)" : "var(--color-state-finished-border)"}`,
          color: missing
            ? "var(--color-state-untranslated)"
            : "var(--color-state-finished)",
          flexShrink: 0,
        }}
      >
        {missing ? "empty" : "set"}
      </span>
      <input
        type="text"
        value={disabled ? "" : value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
        placeholder={
          disabled
            ? "(do not translate)"
            : missing
              ? `translate "${sourceTerm}" to ${locale}…`
              : ""
        }
        spellCheck={false}
        style={{
          flex: 1,
          height: 30,
          paddingInline: 10,
          borderRadius: 4,
          border: "1px solid var(--color-border-default)",
          background: "var(--color-bg-input)",
          color: "var(--color-fg-primary)",
          fontFamily: "var(--font-mono)",
          fontSize: 13,
          outline: "none",
          opacity: disabled ? 0.45 : 1,
          cursor: disabled ? "not-allowed" : "text",
          boxSizing: "border-box",
          minWidth: 0,
        }}
      />
    </div>
  );
}

function OverrideRow({
  override,
  last,
  allLocaleIds,
  onChange,
  onDelete,
}: {
  override: LocaleOverrideEntry;
  last: boolean;
  allLocaleIds: string[];
  onChange: (o: LocaleOverrideEntry) => void;
  onDelete: () => void;
}) {
  const inputStyle: React.CSSProperties = {
    height: 28,
    paddingInline: 8,
    borderRadius: 4,
    border: "1px solid var(--color-border-default)",
    background: "var(--color-bg-input)",
    color: "var(--color-fg-primary)",
    fontSize: 12.5,
    outline: "none",
    fontFamily: "var(--font-mono)",
    boxSizing: "border-box",
  };

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "10px 14px",
        borderBottom: last ? "none" : "1px solid var(--color-border-subtle)",
      }}
    >
      <input
        type="text"
        value={override.locale}
        list="known-locales-override"
        onChange={(e) => onChange({ ...override, locale: e.target.value })}
        placeholder="locale…"
        spellCheck={false}
        style={{ ...inputStyle, width: 110 }}
        aria-label="Locale"
      />
      <datalist id="known-locales-override">
        {allLocaleIds.map((id) => (
          <option key={id} value={id} />
        ))}
      </datalist>
      <select
        value={override.register ?? ""}
        onChange={(e) =>
          onChange({
            ...override,
            register:
              e.target.value === "" ? null : (e.target.value as Register),
          })
        }
        style={{
          ...inputStyle,
          width: 120,
          fontFamily: "inherit",
        }}
        aria-label="Register"
      >
        <option value="">— default</option>
        {REGISTERS.map((r) => (
          <option key={r} value={r}>
            {r}
          </option>
        ))}
      </select>
      <input
        type="text"
        value={override.variant ?? ""}
        onChange={(e) =>
          onChange({
            ...override,
            variant: e.target.value.length === 0 ? null : e.target.value,
          })
        }
        placeholder="variant…"
        spellCheck={false}
        style={{ ...inputStyle, flex: 1, minWidth: 0 }}
        aria-label="Variant"
      />
      <button
        type="button"
        onClick={onDelete}
        aria-label="Remove override"
        title="Remove override"
        style={{
          flexShrink: 0,
          border: "none",
          background: "transparent",
          color: "var(--color-fg-tertiary)",
          cursor: "pointer",
          padding: 4,
          borderRadius: 4,
          fontSize: 16,
          lineHeight: 1,
          display: "flex",
          alignItems: "center",
        }}
      >
        ×
      </button>
    </div>
  );
}

function GlossaryDetailEmpty({
  hasTerms,
  onAddTerm,
}: {
  hasTerms: boolean;
  onAddTerm: () => void;
}) {
  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        color: "var(--color-fg-tertiary)",
        fontSize: 13,
        flexDirection: "column",
        gap: 14,
        padding: 40,
      }}
    >
      {hasTerms ? (
        <p style={{ margin: 0 }}>Select a term from the list to edit it.</p>
      ) : (
        <>
          <p style={{ margin: 0 }}>No terms yet.</p>
          <button
            type="button"
            onClick={onAddTerm}
            style={{
              height: 32,
              paddingInline: 16,
              border: "1px solid var(--color-border-default)",
              borderRadius: 6,
              background: "transparent",
              fontSize: 13,
              color: "var(--color-fg-secondary)",
              cursor: "pointer",
            }}
          >
            + Add first term
          </button>
        </>
      )}
    </div>
  );
}

// ── Empty state (no file loaded) ───────────────────────────────────────────────

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
    <section
      style={{
        flex: 1,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "var(--color-bg-base)",
        padding: 48,
      }}
    >
      <div
        style={{
          width: "100%",
          maxWidth: 520,
          borderRadius: 8,
          border: "1px solid var(--color-border-subtle)",
          background: "var(--color-bg-surface)",
          padding: 32,
        }}
      >
        <div style={{ marginBottom: 8 }}>
          <Eyebrow>Glossary</Eyebrow>
        </div>
        <h1
          style={{
            margin: "0 0 8px",
            fontSize: 22,
            fontWeight: 600,
            letterSpacing: "-0.01em",
            color: "var(--color-fg-primary)",
          }}
        >
          Open or create a glossary
        </h1>
        <p
          style={{
            margin: "0 0 24px",
            fontSize: 13.5,
            color: "var(--color-fg-secondary)",
            lineHeight: 1.65,
          }}
        >
          A glossary is a TOML file with per-term translations and
          do-not-translate markers. The harness uses it to constrain the model's
          vocabulary and to surface gate warnings when target text drifts from
          the project's standard wording.
        </p>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 10 }}>
          <button
            type="button"
            onClick={onOpen}
            style={{
              height: 36,
              paddingInline: 16,
              borderRadius: 6,
              border: "none",
              background: "var(--color-accent)",
              color: "var(--color-accent-fg)",
              fontSize: 13.5,
              fontWeight: 500,
              cursor: "pointer",
            }}
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
            style={{
              height: 36,
              paddingInline: 16,
              borderRadius: 6,
              border: "1px solid var(--color-border-default)",
              background: "transparent",
              color: "var(--color-fg-secondary)",
              fontSize: 13.5,
              cursor: "pointer",
            }}
          >
            New glossary
          </button>
        </div>
      </div>
    </section>
  );
}

// ── Detail field wrapper ───────────────────────────────────────────────────────

function DetailField({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <Eyebrow>{label}</Eyebrow>
      {children}
    </div>
  );
}

// ── Inline SVG icons ───────────────────────────────────────────────────────────

function BookOpenIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 16 16"
      fill="none"
      aria-hidden="true"
      style={{ color: "var(--color-accent)" }}
    >
      <path
        d="M1 3.5A1.5 1.5 0 0 1 2.5 2h4.25A1.25 1.25 0 0 1 8 3.25v9.5A1.25 1.25 0 0 1 6.75 14H2.5A1.5 1.5 0 0 1 1 12.5v-9Z"
        stroke="currentColor"
        strokeWidth="1.2"
        fill="none"
      />
      <path
        d="M15 3.5A1.5 1.5 0 0 0 13.5 2H9.25A1.25 1.25 0 0 0 8 3.25v9.5A1.25 1.25 0 0 0 9.25 14H13.5A1.5 1.5 0 0 0 15 12.5v-9Z"
        stroke="currentColor"
        strokeWidth="1.2"
        fill="none"
      />
    </svg>
  );
}

function ListIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 14 14"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M2 3h10M2 7h10M2 11h10"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinecap="round"
      />
    </svg>
  );
}

function FlagIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 14 14"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M2 1.5v11M2 1.5h8l-2 3.5 2 3.5H2"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function AlertIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 14 14"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M7 2 L12.5 12 H1.5 Z"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinejoin="round"
        fill="none"
      />
      <path
        d="M7 6v2.5M7 10.5v.5"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
    </svg>
  );
}

function GlobeIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 14 14"
      fill="none"
      aria-hidden="true"
    >
      <circle cx="7" cy="7" r="5.5" stroke="currentColor" strokeWidth="1.2" />
      <path
        d="M7 1.5c-2 1.5-2 9 0 11M7 1.5c2 1.5 2 9 0 11M1.5 7h11"
        stroke="currentColor"
        strokeWidth="1.2"
      />
    </svg>
  );
}

function SearchIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 14 14"
      fill="none"
      aria-hidden="true"
    >
      <circle cx="5.5" cy="5.5" r="4" stroke="currentColor" strokeWidth="1.3" />
      <path
        d="M8.5 8.5l3 3"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinecap="round"
      />
    </svg>
  );
}

function PlusIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 14 14"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M7 2v10M2 7h10"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
    </svg>
  );
}

function TrashIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 14 14"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M1.5 3.5h11M5 3.5V2.5a.5.5 0 0 1 .5-.5h3a.5.5 0 0 1 .5.5v1M11.5 3.5l-.75 8.5H3.25L2.5 3.5"
        stroke="currentColor"
        strokeWidth="1.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

// ── Helpers ────────────────────────────────────────────────────────────────────

interface FilteredTerm {
  entry: TermEntry;
  index: number;
}

function buildFilteredTerms(
  terms: TermEntry[],
  search: string,
  view: ViewFilter,
  allLocaleIds: string[],
): FilteredTerm[] {
  const lower = search.trim().toLowerCase();

  return terms
    .map((entry, index) => ({ entry, index }))
    .filter(({ entry }) => {
      // View filter
      if (view === "dnt" && !entry.do_not_translate) return false;
      if (view === "missing") {
        if (entry.do_not_translate) return false;
        // Match the count predicate exactly: a term is "missing" when any
        // project locale lacks a translation. Iterating over project
        // locales (rather than the term's own translation keys) catches
        // the "key never set" case the count includes.
        const missingAny = allLocaleIds.some((id) => !entry.translations[id]);
        if (!missingAny) return false;
      }
      // Text search
      if (lower) {
        if (entry.source.toLowerCase().includes(lower)) return true;
        if (entry.notes?.toLowerCase().includes(lower)) return true;
        return Object.values(entry.translations).some((v) =>
          v.toLowerCase().includes(lower),
        );
      }
      return true;
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
