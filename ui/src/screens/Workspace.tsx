import { toast } from "@vitavision/ui";
import { AppShell } from "@vitavision/workbench";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type {
  CatalogSummary,
  FolderView,
  GlossaryEntry,
  Language,
  ModelChoice,
  ModelSettings,
  TranslationEvent,
  UnitPage,
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
import { PAGE_SIZE, Strings } from "../workspace/Strings";
import { Toolbar } from "../workspace/Toolbar";

export function Workspace(props: { folder: FolderView; onClose: () => void }) {
  const api = useApi();
  const [catalogs, setCatalogs] = useState<CatalogSummary[]>(props.folder.catalogs);
  const [selected, setSelected] = useState<string | null>(props.folder.catalogs[0]?.path ?? null);
  const [tab, setTab] = useState<StatusTab>("to-do");
  const [search, setSearch] = useState("");
  const [query, setQuery] = useState("");
  const [errorsOnly, setErrorsOnly] = useState(false);
  const [offset, setOffset] = useState(0);
  const [page, setPage] = useState<UnitPage | null>(null);
  const [reload, setReload] = useState(0);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [settings, setSettings] = useState<ModelSettings | null>(null);
  const [models, setModels] = useState<ModelChoice[] | null>(null);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [run, setRun] = useState<RunState>({ kind: "idle" });
  const [busyUnit, setBusyUnit] = useState(false);
  const [languages, setLanguages] = useState<Language[]>([]);
  const [glossary, setGlossary] = useState<GlossaryEntry[] | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [pivotFor, setPivotFor] = useState<CatalogSummary | null>(null);

  const catalog = catalogs.find((c) => c.path === selected) ?? null;
  const activeUnit = page?.units.find((u) => u.id === activeId) ?? null;
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

  // The current page of strings.
  useEffect(() => {
    if (selected === null) return;
    let live = true;
    api
      .listUnits(
        selected,
        { statuses: tab === "all" ? [] : [tab], errors_only: errorsOnly, search: query || null },
        offset,
        PAGE_SIZE,
      )
      .then((next) => live && setPage(next))
      .catch((error: unknown) => live && fail("Could not load the strings", error));
    return () => {
      live = false;
    };
  }, [api, selected, tab, errorsOnly, query, offset, reload, fail]);

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
    if (event.unit) {
      const unit = event.unit;
      setPage((p) => (p ? { ...p, units: p.units.map((u) => (u.id === unit.id ? unit : u)) } : p));
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

  const translate = async (paths: string[]) => {
    try {
      const total = await api.startTranslation(
        { catalogs: paths, statuses: ["to-do"], ids: null },
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

  // Ctrl/Cmd+S saves.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void save();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [save]);

  const replaceUnit = (unit: UnitView) =>
    setPage((p) => (p ? { ...p, units: p.units.map((u) => (u.id === unit.id ? unit : u)) } : p));

  const nextId = (after: string): string | null => {
    const units = page?.units ?? [];
    const i = units.findIndex((u) => u.id === after);
    return units[i + 1]?.id ?? null;
  };

  const saveUnit = async (forms: string[], done: boolean) => {
    if (!catalog || !activeUnit) return;
    try {
      const unit = await api.setTranslation(catalog.path, activeUnit.id, forms, done);
      replaceUnit(unit);
      if (done) setActiveId(nextId(unit.id));
      await refreshCatalogs();
    } catch (error) {
      fail("Could not change the translation", error);
    }
  };

  const translateUnit = async () => {
    if (!catalog || !activeUnit) return;
    setBusyUnit(true);
    try {
      replaceUnit(await api.translateOne(catalog.path, activeUnit.id, "auto"));
      await refreshCatalogs();
    } catch (error) {
      fail("Could not translate the string", error);
    } finally {
      setBusyUnit(false);
    }
  };

  const toDoInFolder = useMemo(() => catalogs.reduce((n, c) => n + c.counts.to_do, 0), [catalogs]);

  const openSettings = async () => {
    if (!catalog) return;
    try {
      setGlossary(await api.getGlossary(catalog.locale));
      setSettingsOpen(true);
    } catch (error) {
      fail("Could not load the glossary", error);
    }
  };

  const setView = (next: Partial<{ tab: StatusTab; errorsOnly: boolean; selected: string }>) => {
    setOffset(0);
    setActiveId(null);
    if (next.tab !== undefined) setTab(next.tab);
    if (next.errorsOnly !== undefined) setErrorsOnly(next.errorsOnly);
    if (next.selected !== undefined) {
      setSelected(next.selected);
      setPage(null);
    }
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
              onSearch={(s) => {
                setOffset(0);
                setSearch(s);
              }}
              errorsOnly={errorsOnly}
              onErrorsOnly={(on) => setView({ errorsOnly: on })}
              page={page}
              offset={offset}
              onOffset={setOffset}
              activeId={activeId}
              onActivate={(u) => setActiveId(u.id)}
            />
          ) : (
            <p className="p-6 text-fg-muted">This folder has no translation files.</p>
          )
        }
        right={
          catalog ? (
            <Editor
              key={`${catalog.path}::${activeUnit?.id ?? ""}::${activeUnit?.target.join("\u0000") ?? ""}`}
              catalog={catalog}
              unit={activeUnit}
              canTranslate={settings?.model != null && !running}
              busy={busyUnit}
              onSave={(forms, done) => void saveUnit(forms, done)}
              onTranslate={() => void translateUnit()}
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

      {settingsOpen && catalog && settings && glossary && (
        <SettingsDialog
          locale={catalog.locale}
          language={catalog.language}
          glossary={glossary}
          settings={settings}
          onClose={() => setSettingsOpen(false)}
          onSaved={(_glossary, saved) => {
            setSettingsOpen(false);
            setSettings(saved);
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
