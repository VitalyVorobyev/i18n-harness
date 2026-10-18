import { toast } from "@vitavision/ui";
import { AppShell } from "@vitavision/workbench";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type {
  CatalogSummary,
  CheckSettings,
  FillReport,
  FolderView,
  GlossaryEntry,
  Language,
  ModelChoice,
  ModelSettings,
  SharedString,
  Show,
  SiblingView,
  TranslationEvent,
  TranslateScope,
  UnitRow,
  UnitView,
} from "../api";
import { errorKind, errorMessage, useApi } from "../api";
import { plural } from "../lib/format";
import type { SharedKind } from "../workspace/Across";
import { Across, SharedDetail, firstKind, sharedKey } from "../workspace/Across";
import { Editor } from "../workspace/Editor";
import { FileSummary } from "../workspace/FileSummary";
import { ALL_FILES, FileList, allFilesSummary } from "../workspace/FileList";
import { Glossary } from "../workspace/Glossary";
import { PivotDialog } from "../workspace/PivotDialog";
import type { RunState } from "../workspace/RunBar";
import { RunBar } from "../workspace/RunBar";
import { SettingsDialog } from "../workspace/SettingsDialog";
import type { StatusTab } from "../workspace/Strings";
import { Strings, rowKey } from "../workspace/Strings";
import type { View } from "../workspace/Toolbar";
import { Toolbar } from "../workspace/Toolbar";

/** Every translated string whose text breaks the glossary. */
const GLOSSARY_ISSUES: TranslateScope = {
  catalogs: [],
  statuses: [],
  ids: null,
  glossary_issues: true,
};

export function Workspace(props: { folder: FolderView; onClose: () => void }) {
  const api = useApi();
  const [catalogs, setCatalogs] = useState<CatalogSummary[]>(props.folder.catalogs);
  const [selected, setSelected] = useState<string | null>(props.folder.catalogs[0]?.path ?? null);
  const [tab, setTab] = useState<StatusTab>("to-do");
  const [search, setSearch] = useState("");
  const [query, setQuery] = useState("");
  const [show, setShow] = useState<Show>("everything");
  const [term, setTerm] = useState<string | null>(null);
  const [rows, setRows] = useState<UnitRow[] | null>(null);
  const [reload, setReload] = useState(0);
  const [active, setActive] = useState<UnitRow | null>(null);
  const [activeUnit, setActiveUnit] = useState<UnitView | null>(null);
  // The row whose editor should take keyboard focus once shown (chosen from
  // the keyboard rather than with the mouse).
  const [focusKey, setFocusKey] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const [settings, setSettings] = useState<ModelSettings | null>(null);
  const [models, setModels] = useState<ModelChoice[] | null>(null);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [run, setRun] = useState<RunState>({ kind: "idle" });
  const [busyUnit, setBusyUnit] = useState(false);
  const [languages, setLanguages] = useState<Language[]>([]);
  const [view, setMainView] = useState<View>("strings");
  const [glossaryLocale, setGlossaryLocale] = useState<string | null>(null);
  const [checks, setChecks] = useState<CheckSettings | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [pivotFor, setPivotFor] = useState<CatalogSummary | null>(null);
  const [glossaryIssues, setGlossaryIssues] = useState(0);
  const [siblings, setSiblings] = useState<SiblingView[] | null>(null);
  const [fillable, setFillable] = useState<FillReport>({ filled: 0, conflicting: 0 });
  // Strings to do in the shown file (or all files) that other files agree on.
  const [fillableHere, setFillableHere] = useState(0);
  const [shared, setShared] = useState<SharedString[] | null>(null);
  const [sharedKind, setSharedKind] = useState<SharedKind | null>(null);
  const [activeShared, setActiveShared] = useState<string | null>(null);
  const [sharedBusy, setSharedBusy] = useState(false);
  const [usedTerms, setUsedTerms] = useState<string[]>([]);
  // A string to select once the rows of its file arrive.
  const [pendingOpen, setPendingOpen] = useState<{ path: string; id: string } | null>(null);

  const allFiles = selected === ALL_FILES;
  const catalog = catalogs.find((c) => c.path === selected) ?? null;
  // What the strings view shows: one file, or all of them together.
  const shown = useMemo(
    () => (allFiles ? allFilesSummary(catalogs) : catalog),
    [allFiles, catalogs, catalog],
  );
  const activeCatalog = active ? (catalogs.find((c) => c.path === active.path) ?? null) : null;
  const settingsCatalog = catalog ?? activeCatalog ?? catalogs[0] ?? null;
  const activeKey = active ? rowKey(active) : null;
  const dirty = catalogs.some((c) => c.dirty);
  const running = run.kind === "running";

  const fail = useCallback((title: string, error: unknown) => {
    toast({ tone: "error", title, description: errorMessage(error) });
  }, []);

  // Search waits for a pause in typing.
  useEffect(() => {
    const t = setTimeout(() => setQuery(search), 200);
    return () => clearTimeout(t);
  }, [search]);

  // The strings of the selected file. A newer request supersedes an older
  // one; the rows on screen stay until the new ones arrive.
  useEffect(() => {
    if (selected === null) return;
    let live = true;
    api
      .listRows(allFiles ? null : selected, {
        statuses: tab === "all" ? [] : [tab],
        show,
        term,
        search: query || null,
      })
      .then((next) => live && setRows(next))
      .catch((error: unknown) => live && fail("Could not load the strings", error));
    return () => {
      live = false;
    };
  }, [api, selected, allFiles, tab, show, term, query, reload, fail]);

  // A string opened from another view is selected once its file's rows are in.
  useEffect(() => {
    if (pendingOpen === null || rows === null) return;
    const row = rows.find((r) => r.path === pendingOpen.path && r.id === pendingOpen.id);
    if (!row) return;
    setPendingOpen(null);
    setActive(row);
    setFocusKey(rowKey(row));
  }, [rows, pendingOpen]);

  // The full view of the selected string and the same string in other
  // files, for the editor.
  const [siblingsVersion, setSiblingsVersion] = useState(0);
  useEffect(() => {
    if (active === null) {
      setActiveUnit(null);
      setSiblings(null);
      return;
    }
    let live = true;
    api
      .getUnit(active.path, active.id)
      .then((unit) => live && setActiveUnit(unit))
      .catch((error: unknown) => live && fail("Could not load the string", error));
    api
      .siblings(active.path, active.id)
      .then((list) => live && setSiblings(list))
      .catch(() => live && setSiblings(null));
    return () => {
      live = false;
    };
  }, [api, active, siblingsVersion, fail]);

  // Model settings and the models the server offers.
  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const s = await api.getModelSettings();
        if (!live) return;
        setSettings(s);
        const list = await api.listModels(s);
        if (!live) return;
        setModels(list);
        setModelsError(null);
      } catch (error) {
        if (live) setModelsError(errorMessage(error));
      }
    })();
    api
      .listLanguages()
      .then((l) => live && setLanguages(l))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [api]);

  // Translation progress.
  const onEvent = useRef<(event: TranslationEvent) => void>(() => undefined);
  onEvent.current = (event) => {
    const { progress } = event;
    if (event.catalog) {
      const summary = event.catalog;
      setCatalogs((all) => all.map((c) => (c.path === summary.path ? summary : c)));
    }
    if (event.unit && event.catalog) {
      updateRow(event.catalog.path, event.unit);
    }
    if (progress.event === "started") {
      setRun({ kind: "running", done: 0, total: progress.total, failed: 0, cancelling: false });
    } else if (progress.event === "unit") {
      setRun((r) =>
        r.kind === "running"
          ? { ...r, done: progress.done, failed: r.failed + (progress.translated ? 0 : 1) }
          : r,
      );
    } else {
      setRun({
        kind: "finished",
        translated: progress.translated,
        failed: progress.failed,
        cancelled: progress.cancelled,
      });
      setReload((n) => n + 1);
    }
  };
  useEffect(() => {
    let unsubscribe: (() => void) | null = null;
    let live = true;
    void api
      .onTranslation((e) => onEvent.current(e))
      .then((u) => {
        if (live) unsubscribe = u;
        else u();
      });
    return () => {
      live = false;
      unsubscribe?.();
    };
  }, [api]);

  const refreshCatalogs = async () => {
    try {
      setCatalogs(await api.listCatalogs());
    } catch (error) {
      fail("Could not refresh the files", error);
    }
  };

  const chooseModel = async (id: string) => {
    if (!settings) return;
    try {
      setSettings(await api.setModelSettings({ ...settings, model: id }));
    } catch (error) {
      fail("Could not select the model", error);
    }
  };

  const translate = async (paths: string[], glossaryIssuesOnly = false) => {
    try {
      const total = await api.startTranslation(
        glossaryIssuesOnly
          ? GLOSSARY_ISSUES
          : { catalogs: paths, statuses: ["to-do"], ids: null, glossary_issues: false },
        "auto",
      );
      if (total === 0) toast({ tone: "info", title: "Nothing to translate" });
    } catch (error) {
      if (errorKind(error) === "needs-model") {
        toast({
          tone: "warn",
          title: "Choose a model first",
          description: "Pick a translation model in the toolbar.",
        });
      } else {
        fail("Could not start translating", error);
      }
    }
  };

  const save = useCallback(async () => {
    try {
      const report = await api.save(null);
      await refreshCatalogs();
      toast(
        report.held_back > 0
          ? {
              tone: "warn",
              title: "Saved, with problems held back",
              description: `${plural(report.held_back, "translation has", "translations have")} a problem and kept the previous text in the file.`,
            }
          : { tone: "success", title: report.written.length > 0 ? "Saved" : "Nothing to save" },
      );
    } catch (error) {
      fail("Could not save", error);
    }
    // refreshCatalogs only reads state setters and api.
    // oxlint-disable-next-line react/exhaustive-deps
  }, [api, fail]);

  // Ctrl/Cmd+S saves; Ctrl/Cmd+F goes to the search field.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void save();
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [save]);

  // Bring a row in line with a changed string, keeping its place in the list.
  function updateRow(path: string, unit: UnitView) {
    const patch = (r: UnitRow): UnitRow =>
      r.path === path && r.id === unit.id
        ? {
            ...r,
            target: unit.target[0] ?? "",
            status: unit.status,
            error: unit.issues.some((i) => i.severity === "error"),
            warning: unit.issues.some((i) => i.severity === "warning"),
          }
        : r;
    setRows((all) => all?.map(patch) ?? all);
    setActive((a) => (a ? patch(a) : a));
    setActiveUnit((u) => (u && active?.path === path && u.id === unit.id ? unit : u));
  }

  const neighbour = (step: 1 | -1): UnitRow | null => {
    const list = rows ?? [];
    const i = active ? list.findIndex((r) => rowKey(r) === rowKey(active)) : -1;
    return list[i + step] ?? null;
  };

  const activate = (row: UnitRow | null, focus: boolean) => {
    setActive(row);
    setFocusKey(focus && row ? rowKey(row) : null);
  };

  const saveUnit = async (forms: string[], done: boolean) => {
    if (!active) return;
    try {
      const after = neighbour(1);
      const unit = await api.setTranslation(active.path, active.id, forms, done);
      updateRow(active.path, unit);
      if (done) activate(after, true);
      await refreshCatalogs();
    } catch (error) {
      fail("Could not change the translation", error);
    }
  };

  const copyEverywhere = async () => {
    if (!active) return;
    try {
      const changed = await api.useEverywhere(active.path, active.id);
      toast({
        tone: "success",
        title: `Used in ${plural(changed, "other file", "other files")}`,
        description: "Save to write the files.",
      });
      setSiblingsVersion((n) => n + 1);
      setReload((n) => n + 1);
      await refreshCatalogs();
    } catch (error) {
      fail("Could not copy the translation", error);
    }
  };

  const fillFromOtherFiles = async (paths: string[] = []) => {
    try {
      const report = await api.fillFromOtherFiles(paths, true);
      toast({
        tone: report.filled > 0 ? "success" : "info",
        title: `${plural(report.filled, "string", "strings")} filled from other files`,
        description:
          report.conflicting > 0
            ? `${plural(report.conflicting, "string is", "strings are")} translated differently in other files and left to do: choose "Translated differently in other files" to see them.`
            : undefined,
      });
      setReload((n) => n + 1);
      setSiblingsVersion((n) => n + 1);
      await refreshCatalogs();
    } catch (error) {
      fail("Could not fill from other files", error);
    }
  };

  const translateUnit = async () => {
    if (!active) return;
    setBusyUnit(true);
    try {
      updateRow(active.path, await api.translateOne(active.path, active.id, "auto"));
      await refreshCatalogs();
    } catch (error) {
      fail("Could not translate the string", error);
    } finally {
      setBusyUnit(false);
    }
  };

  // Recount after every change, but not per string during a run: the
  // count checks every translation and the run holds the session.
  useEffect(() => {
    if (running) return;
    let live = true;
    void api
      .planSize(GLOSSARY_ISSUES)
      .then((n) => live && setGlossaryIssues(n))
      .catch(() => live && setGlossaryIssues(0));
    void api
      .fillFromOtherFiles([], false)
      .then((report) => live && setFillable(report))
      .catch(() => live && setFillable({ filled: 0, conflicting: 0 }));
    void api
      .sharedStrings()
      .then((list) => live && setShared(list))
      .catch(() => live && setShared([]));
    return () => {
      live = false;
    };
  }, [api, catalogs, running, reload]);

  useEffect(() => {
    if (running || selected === null) return;
    let live = true;
    void api
      .fillFromOtherFiles(selected === ALL_FILES ? [] : [selected], false)
      .then((report) => live && setFillableHere(report.filled))
      .catch(() => live && setFillableHere(0));
    return () => {
      live = false;
    };
  }, [api, catalogs, running, reload, selected]);

  // Different strings over every file: a string in several files counts once.
  const distinct = useMemo(() => {
    if (shared === null) return null;
    const total = catalogs.reduce((n, c) => n + c.counts.total, 0);
    return shared.reduce((n, s) => n - (s.members.length - 1), total);
  }, [shared, catalogs]);
  const pivotLanguage = catalogs.find((c) => c.pivot_language !== null)?.pivot_language ?? null;
  const kind = sharedKind ?? (shared ? firstKind(shared) : "differ");
  const activeSharedString = shared?.find((x) => sharedKey(x) === activeShared) ?? null;

  const toDoInFolder = useMemo(() => catalogs.reduce((n, c) => n + c.counts.to_do, 0), [catalogs]);
  const targetLanguages = useMemo(() => {
    const seen = new Map<string, string>();
    for (const c of catalogs) seen.set(c.locale, c.language);
    return [...seen].map(([locale, language]) => ({ locale, language }));
  }, [catalogs]);
  const shownLocale = glossaryLocale ?? catalog?.locale ?? targetLanguages[0]?.locale ?? null;
  const termsLocale = catalog?.locale ?? targetLanguages[0]?.locale ?? null;

  // The terms some string uses, for the term filter.
  useEffect(() => {
    if (running || termsLocale === null) return;
    let live = true;
    void api
      .glossaryUsage(termsLocale)
      .then((usage) => live && setUsedTerms(usage.filter((u) => u.strings > 0).map((u) => u.term)))
      .catch(() => live && setUsedTerms([]));
    return () => {
      live = false;
    };
  }, [api, termsLocale, running, reload]);

  // A count in the glossary opens the strings, over every file, that use
  // or break that term (the glossary counts them over every file).
  const showTermStrings = (entry: GlossaryEntry, breaking: boolean) => {
    setMainView("strings");
    setActive(null);
    setTab("all");
    setShow(breaking ? "glossary-broken" : "glossary");
    setTerm(entry.term);
    setSearch("");
    if (catalogs.length > 1 && selected !== ALL_FILES) {
      setSelected(ALL_FILES);
      setRows(null);
    }
  };

  // Open one file's copy of a shared string in the strings view.
  const openInStrings = (path: string, id: string) => {
    setMainView("strings");
    setTab("all");
    setShow("everything");
    setTerm(null);
    setSearch("");
    setActive(null);
    if (selected !== path) {
      setSelected(path);
      setRows(null);
    }
    setPendingOpen({ path, id });
  };

  const afterSharedChange = async (changed: number) => {
    toast({
      tone: "success",
      title: `Changed in ${plural(changed, "other file", "other files")}`,
      description: "Save to write the files.",
    });
    setReload((n) => n + 1);
    setSiblingsVersion((n) => n + 1);
    await refreshCatalogs();
  };

  const copySharedVersion = async (s: SharedString, member: SiblingView) => {
    setSharedBusy(true);
    try {
      await afterSharedChange(await api.useEverywhere(member.path, s.id));
    } catch (error) {
      fail("Could not copy the translation", error);
    } finally {
      setSharedBusy(false);
    }
  };

  const writeShared = async (s: SharedString, text: string) => {
    const first = s.members[0];
    if (!first) return;
    setSharedBusy(true);
    try {
      await api.setTranslation(first.path, s.id, [text], true);
      await afterSharedChange(await api.useEverywhere(first.path, s.id));
    } catch (error) {
      fail("Could not change the translation", error);
    } finally {
      setSharedBusy(false);
    }
  };

  const openSettings = async () => {
    if (!settingsCatalog) return;
    try {
      setChecks(await api.getCheckSettings(settingsCatalog.locale));
      setSettingsOpen(true);
    } catch (error) {
      fail("Could not load the settings", error);
    }
  };

  const setView = (next: Partial<{ tab: StatusTab; show: Show; selected: string }>) => {
    setActive(null);
    // Choosing the file already shown only goes back to its summary (and
    // must not blank the list: the loading effect would not run again).
    if (next.selected === selected && next.tab === undefined && next.show === undefined) return;
    if (next.tab !== undefined) setTab(next.tab);
    if (next.show !== undefined) {
      setShow(next.show);
      if (next.show !== "glossary" && next.show !== "glossary-broken") setTerm(null);
    }
    if (next.selected !== undefined && next.selected !== selected) {
      setSelected(next.selected);
      setRows(null);
    }
  };

  // Esc in the editor returns to the selected row of the list.
  const focusList = () => {
    document.querySelector<HTMLElement>('section[aria-label="Strings"] tr[aria-current]')?.focus();
  };

  return (
    <>
      <AppShell
        storageKey="i18n-harness:workspace"
        header={
          <Toolbar
            root={props.folder.root}
            view={view}
            onView={setMainView}
            models={models}
            modelsError={modelsError}
            model={settings?.model ?? null}
            onModelChange={(id) => void chooseModel(id)}
            toDoInFile={shown?.counts.to_do ?? 0}
            toDoInFolder={toDoInFolder}
            running={running}
            dirty={dirty}
            onTranslateFile={() => shown && void translate(allFiles ? [] : [shown.path])}
            fillable={fillable.filled}
            onFill={() => void fillFromOtherFiles()}
            onTranslateFolder={() => void translate([])}
            glossaryIssues={glossaryIssues}
            onApplyGlossary={() => void translate([], true)}
            onSave={() => void save()}
            onSettings={() => void openSettings()}
            onClose={props.onClose}
          />
        }
        left={
          <FileList
            catalogs={catalogs}
            selected={selected}
            distinct={distinct}
            onSelect={(path) => {
              if (view === "glossary") setMainView("strings");
              setView({ selected: path });
            }}
            onSetPivot={setPivotFor}
          />
        }
        leftLabel="Files"
        leftSize={{ defaultSize: 300, minSize: 200, maxSize: "40%" }}
        main={
          view === "glossary" && shownLocale !== null ? (
            <Glossary
              locale={shownLocale}
              language={
                targetLanguages.find((l) => l.locale === shownLocale)?.language ?? shownLocale
              }
              languages={targetLanguages}
              onLocale={setGlossaryLocale}
              breaking={glossaryIssues}
              canApply={settings?.model != null && !running}
              onApply={() => void translate([], true)}
              onChanged={() => {
                setReload((n) => n + 1);
                void refreshCatalogs();
              }}
              onShowStrings={showTermStrings}
            />
          ) : view === "across" ? (
            <Across
              shared={shared}
              pivotLanguage={pivotLanguage}
              kind={kind}
              onKind={(k) => {
                setSharedKind(k);
                setActiveShared(null);
              }}
              activeKey={activeShared}
              onActivate={(s) => setActiveShared(sharedKey(s))}
              fillable={fillable.filled}
              running={running}
              onFill={() => void fillFromOtherFiles()}
            />
          ) : shown ? (
            <Strings
              catalog={shown}
              allFiles={allFiles}
              tab={tab}
              onTab={(t) => setView({ tab: t })}
              search={search}
              onSearch={setSearch}
              searchRef={searchRef}
              show={show}
              onShow={(s) => setView({ show: s })}
              term={term}
              onTerm={setTerm}
              terms={usedTerms}
              multiFile={catalogs.length > 1}
              onClear={() => {
                setActive(null);
                setShow("everything");
                setTerm(null);
                setSearch("");
              }}
              rows={rows}
              activeKey={activeKey}
              onActivate={(row, keyboard) => activate(row, keyboard)}
              onDeselect={() => setActive(null)}
            />
          ) : (
            <p className="p-6 text-fg-muted">This folder has no translation files.</p>
          )
        }
        right={
          view === "across" ? (
            <SharedDetail
              key={activeShared ?? ""}
              shared={activeSharedString}
              pivotLanguage={pivotLanguage}
              busy={sharedBusy || running}
              onUse={(member) =>
                activeSharedString && void copySharedVersion(activeSharedString, member)
              }
              onWrite={(text) => activeSharedString && void writeShared(activeSharedString, text)}
              onOpen={(member) =>
                activeSharedString && openInStrings(member.path, activeSharedString.id)
              }
            />
          ) : view === "strings" && shown && active === null ? (
            <FileSummary
              summary={shown}
              allFiles={allFiles}
              fileCount={catalogs.length}
              root={props.folder.root}
              distinct={distinct}
              fillable={fillableHere}
              running={running}
              onTab={(t) => setView({ tab: t, show: "everything" })}
              onShow={(s) => setView({ show: s, tab: "all" })}
              onFill={() => void fillFromOtherFiles(allFiles ? [] : [shown.path])}
              onAcross={() => setMainView("across")}
              onSave={() => void save()}
              onSetPivot={allFiles || !catalog ? null : () => setPivotFor(catalog)}
            />
          ) : view === "strings" && shown ? (
            <Editor
              key={`${active?.path ?? ""}::${activeUnit?.id ?? ""}::${activeUnit?.target.join("\u0000") ?? ""}`}
              catalog={activeCatalog ?? shown}
              unit={activeUnit !== null && activeUnit.id === active?.id ? activeUnit : null}
              siblings={siblings}
              onUseEverywhere={() => void copyEverywhere()}
              onTerm={(t) => {
                setTerm(t);
                setShow("everything");
                setTab("all");
              }}
              canTranslate={settings?.model != null && !running}
              busy={busyUnit}
              autoFocus={focusKey !== null && focusKey === activeKey}
              onSave={(forms, done) => void saveUnit(forms, done)}
              onTranslate={() => void translateUnit()}
              onLeave={focusList}
              onStep={(step) => activate(neighbour(step), true)}
            />
          ) : null
        }
        rightLabel={
          view === "across" ? "Shared string" : active === null ? "Summary" : "Translation"
        }
        rightSize={{ defaultSize: 420, minSize: 300, maxSize: "55%" }}
        bottom={
          <RunBar
            run={run}
            onCancel={() => {
              setRun((r) => (r.kind === "running" ? { ...r, cancelling: true } : r));
              void api.cancelTranslation();
            }}
            onDismiss={() => setRun({ kind: "idle" })}
          />
        }
      />

      {pivotFor && (
        <PivotDialog
          catalog={pivotFor}
          languages={languages}
          onClose={() => setPivotFor(null)}
          onApply={(pivot) => {
            const target = pivotFor;
            setPivotFor(null);
            void api
              .setPivot(target.path, pivot)
              .then((summary) => {
                setCatalogs((all) => all.map((c) => (c.path === summary.path ? summary : c)));
                setReload((n) => n + 1);
              })
              .catch((error: unknown) => fail("Could not change the language", error));
          }}
        />
      )}

      {settingsOpen && settingsCatalog && settings && checks && (
        <SettingsDialog
          locale={settingsCatalog.locale}
          language={settingsCatalog.language}
          settings={settings}
          checks={checks}
          onClose={() => setSettingsOpen(false)}
          onSaved={(saved) => {
            setSettingsOpen(false);
            setSettings(saved);
            // Warnings and their counts depend on the check settings.
            void refreshCatalogs();
            setReload((n) => n + 1);
            toast({ tone: "success", title: "Settings saved" });
            void api
              .listModels(saved)
              .then((list) => {
                setModels(list);
                setModelsError(null);
              })
              .catch((error: unknown) => setModelsError(errorMessage(error)));
          }}
        />
      )}
    </>
  );
}
