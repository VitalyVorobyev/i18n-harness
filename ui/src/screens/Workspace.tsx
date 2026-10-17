import { toast } from "@vitavision/ui";
import { AppShell } from "@vitavision/workbench";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type {
  CatalogSummary,
  CheckSettings,
  FolderView,
  GlossaryEntry,
  Language,
  ModelChoice,
  ModelSettings,
  Show,
  TranslationEvent,
  TranslateScope,
  UnitRow,
  UnitView,
} from "../api";
import { errorKind, errorMessage, useApi } from "../api";
import { plural } from "../lib/format";
import { Editor } from "../workspace/Editor";
import { FileList } from "../workspace/FileList";
import { PivotDialog } from "../workspace/PivotDialog";
import type { RunState } from "../workspace/RunBar";
import { RunBar } from "../workspace/RunBar";
import { SettingsDialog } from "../workspace/SettingsDialog";
import type { StatusTab } from "../workspace/Strings";
import { Strings, rowKey } from "../workspace/Strings";
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
  const [glossary, setGlossary] = useState<GlossaryEntry[] | null>(null);
  const [checks, setChecks] = useState<CheckSettings | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [pivotFor, setPivotFor] = useState<CatalogSummary | null>(null);
  const [glossaryIssues, setGlossaryIssues] = useState(0);

  const catalog = catalogs.find((c) => c.path === selected) ?? null;
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
      .listRows(selected, { statuses: tab === "all" ? [] : [tab], show, search: query || null })
      .then((next) => live && setRows(next))
      .catch((error: unknown) => live && fail("Could not load the strings", error));
    return () => {
      live = false;
    };
  }, [api, selected, tab, show, query, reload, fail]);

  // The full view of the selected string, for the editor.
  useEffect(() => {
    if (active === null) {
      setActiveUnit(null);
      return;
    }
    let live = true;
    api
      .getUnit(active.path, active.id)
      .then((unit) => live && setActiveUnit(unit))
      .catch((error: unknown) => live && fail("Could not load the string", error));
    return () => {
      live = false;
    };
  }, [api, active, fail]);

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
    return () => {
      live = false;
    };
  }, [api, catalogs, running, reload]);

  const toDoInFolder = useMemo(() => catalogs.reduce((n, c) => n + c.counts.to_do, 0), [catalogs]);

  const openSettings = async () => {
    if (!catalog) return;
    try {
      setGlossary(await api.getGlossary(catalog.locale));
      setChecks(await api.getCheckSettings(catalog.locale));
      setSettingsOpen(true);
    } catch (error) {
      fail("Could not load the glossary", error);
    }
  };

  const setView = (next: Partial<{ tab: StatusTab; show: Show; selected: string }>) => {
    // Choosing the file already shown changes nothing (and must not blank
    // the list: the loading effect would not run again).
    if (next.selected === selected && next.tab === undefined && next.show === undefined) return;
    setActive(null);
    if (next.tab !== undefined) setTab(next.tab);
    if (next.show !== undefined) setShow(next.show);
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
            models={models}
            modelsError={modelsError}
            model={settings?.model ?? null}
            onModelChange={(id) => void chooseModel(id)}
            toDoInFile={catalog?.counts.to_do ?? 0}
            toDoInFolder={toDoInFolder}
            running={running}
            dirty={dirty}
            onTranslateFile={() => catalog && void translate([catalog.path])}
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
            onSelect={(path) => setView({ selected: path })}
            onSetPivot={setPivotFor}
          />
        }
        leftLabel="Files"
        leftSize={{ defaultSize: 280, minSize: 200, maxSize: "40%" }}
        main={
          catalog ? (
            <Strings
              catalog={catalog}
              tab={tab}
              onTab={(t) => setView({ tab: t })}
              search={search}
              onSearch={setSearch}
              searchRef={searchRef}
              show={show}
              onShow={(s) => setView({ show: s })}
              rows={rows}
              activeKey={activeKey}
              onActivate={(row, keyboard) => activate(row, keyboard)}
            />
          ) : (
            <p className="p-6 text-fg-muted">This folder has no translation files.</p>
          )
        }
        right={
          catalog ? (
            <Editor
              key={`${active?.path ?? ""}::${activeUnit?.id ?? ""}::${activeUnit?.target.join("\u0000") ?? ""}`}
              catalog={catalog}
              unit={activeUnit !== null && activeUnit.id === active?.id ? activeUnit : null}
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
        rightLabel="Translation"
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

      {settingsOpen && catalog && settings && glossary && checks && (
        <SettingsDialog
          locale={catalog.locale}
          language={catalog.language}
          glossary={glossary}
          settings={settings}
          checks={checks}
          onClose={() => setSettingsOpen(false)}
          onSaved={(_glossary, saved) => {
            setSettingsOpen(false);
            setSettings(saved);
            // Warnings and their counts depend on the check settings.
            void refreshCatalogs();
            // Issues shown in the list depend on the glossary.
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
