// An in-memory stand-in for the desktop shell, for `bun run dev:mock`, unit
// tests and the Playwright suite. It follows the session's rules closely
// enough to exercise the UI: statuses, pivot input, translation progress.

import type { Api } from "./api";
import type {
  CatalogSummary,
  GlossaryEntry,
  ModelChoice,
  ModelSettings,
  StatusCounts,
  TranslationEvent,
  UnitFilter,
  UnitStatus,
  UnitView,
} from "./types";

/** What the mock rejects with: the same `{ kind, message }` shape as the shell. */
export class MockError extends Error {
  constructor(
    readonly kind: string,
    message: string,
  ) {
    super(message);
  }
}

interface MockUnit extends UnitView {
  original: string[] | null;
}

interface MockCatalog {
  summary: Omit<CatalogSummary, "counts" | "dirty">;
  units: MockUnit[];
  dirty: boolean;
}

function unit(
  id: string,
  source: string,
  existing: string | string[] | null,
  context: string,
  extra: Partial<UnitView> = {},
): MockUnit {
  const forms = existing === null ? null : Array.isArray(existing) ? existing : [existing];
  const plural = Array.isArray(existing) || extra.plural_slots !== undefined;
  return {
    id,
    source,
    existing: forms,
    original: forms,
    target: forms ?? (plural ? ["", "", ""] : [""]),
    plural_slots: [],
    status: "to-do",
    state: forms ? "finished" : "untranslated",
    issues: [],
    context: { group: context },
    editable: true,
    ...extra,
  };
}

export function sampleProject(): MockCatalog[] {
  return [
    {
      summary: {
        path: "/demo/translations/App_sk_SK.ts",
        name: "App_sk_SK.ts",
        relative_path: "translations/App_sk_SK.ts",
        format: "qt-ts",
        format_name: "Qt Linguist",
        locale: "sk_SK",
        language: "Slovak",
        pivot: "de_DE",
        pivot_language: "German",
      },
      dirty: false,
      units: [
        unit("Settings::Filter Size", "Filter Size", "Filtergröße", "Settings"),
        unit("Settings::pix", "pix", null, "Settings"),
        unit(
          "Settings::<b>Ignore</b> defects under %1 mm",
          "<b>Ignore</b> defects under %1 mm",
          "<b>Ignoriere</b> Defekte unter %1 mm",
          "Settings",
          { context: { group: "Settings", developer_notes: ["Shown next to the size field."] } },
        ),
        unit(
          "Settings::<Plugin not loaded>",
          "<Plugin not loaded>",
          "<Plugin nicht geladen>",
          "Settings",
        ),
        unit("Import::Open ParamsManager", "Open ParamsManager", null, "Import"),
        unit(
          "Import::%n file(s) successfully imported.",
          "%n file(s) successfully imported.",
          ["%n Datei(en) erfolgreich importiert.", "", ""],
          "Import",
          {
            plural_slots: [
              { category: "one", example: 1 },
              { category: "few", example: 2 },
              { category: "other", example: 0 },
            ],
          },
        ),
        ...Array.from({ length: 40 }, (_, i) =>
          unit(
            `Batch::Step ${i + 1}`,
            `Step ${i + 1} of the inspection`,
            `Schritt ${i + 1} der Prüfung`,
            "Batch",
          ),
        ),
      ],
    },
    {
      summary: {
        path: "/demo/po/messages.po",
        name: "messages.po",
        relative_path: "po/messages.po",
        format: "gettext-po",
        format_name: "gettext PO",
        locale: "sk_SK",
        language: "Slovak",
        pivot: null,
        pivot_language: null,
      },
      dirty: false,
      units: [
        unit("Save", "Save", "Uložiť", "", { status: "done", state: "finished", context: {} }),
        unit("Cancel", "Cancel", null, "", { context: {} }),
        unit("Open %s", "Open %s", null, "", { context: {} }),
      ],
    },
  ];
}

const MODELS: ModelChoice[] = [
  {
    id: "eurollm-9b-instruct:q8",
    size_bytes: 9_700_000_000,
    parameter_size: "9.2B",
    quantization: "Q8_0",
    context_length: 32_768,
    prompt_style: "chat",
  },
  {
    id: "milmmt-46-4b:q8",
    size_bytes: 4_700_000_000,
    parameter_size: "3.9B",
    quantization: "Q8_0",
    context_length: 131_072,
    prompt_style: "completion",
  },
];

function placeholders(text: string): string[] {
  return (text.match(/%\d+|%n|%s|\{[^}]*\}/g) ?? []).sort();
}

function sameList(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((x, i) => x === b[i]);
}

function issuesFor(u: MockUnit): UnitView["issues"] {
  const hasText = u.target.some((t) => t !== "");
  const edited = !sameList(u.target, u.original ?? []);
  if (!hasText || !edited) return [];
  const want = placeholders(u.source);
  return u.target.flatMap((t) => {
    const got = placeholders(t);
    if (sameList(got, want)) return [];
    const missing = want.filter((p) => !got.includes(p));
    return [
      {
        severity: "error" as const,
        message: `Placeholders do not match the original: missing ${missing.join(", ") || "?"}`,
      },
    ];
  });
}

function statusFor(u: MockUnit, pivot: boolean): UnitStatus {
  if (u.target.some((t) => t === "")) return "to-do";
  if (u.status === "done" || u.status === "check") {
    if (!pivot || !sameList(u.target, u.original ?? [])) return u.status;
  }
  return pivot ? "to-do" : "done";
}

/** Build a fresh mock backend. `delayMs` slows translation down for demos. */
export function createMockApi(options: { delayMs?: number; ollamaRunning?: boolean } = {}): Api {
  const delayMs = options.delayMs ?? 0;
  let catalogs: MockCatalog[] | null = null;
  let settings: ModelSettings = {
    server: "ollama",
    host: "http://localhost:11434",
    model: null,
    num_ctx: null,
  };
  let glossary: GlossaryEntry[] = [
    { term: "ParamsManager", keep: true, translation: null, note: "Class name" },
  ];
  const listeners = new Set<(event: TranslationEvent) => void>();
  let cancelled = false;

  const refresh = (c: MockCatalog) => {
    const pivot = c.summary.pivot !== null;
    for (const u of c.units) {
      u.status = statusFor(u, pivot);
      u.issues = issuesFor(u);
    }
  };
  const summary = (c: MockCatalog): CatalogSummary => {
    refresh(c);
    const counts: StatusCounts = { total: c.units.length, to_do: 0, check: 0, done: 0, errors: 0 };
    for (const u of c.units) {
      if (u.status === "to-do") counts.to_do += 1;
      if (u.status === "check") counts.check += 1;
      if (u.status === "done") counts.done += 1;
      if (u.issues.some((i) => i.severity === "error")) counts.errors += 1;
    }
    return { ...c.summary, counts, dirty: c.dirty };
  };
  const find = (path: string): MockCatalog => {
    const c = catalogs?.find((x) => x.summary.path === path);
    if (!c) throw new MockError("unknown-catalog", `${path} is not open`);
    return c;
  };
  const view = (u: MockUnit, pivot: boolean): UnitView => {
    const { original, ...rest } = u;
    // As the session does: untouched pivot text is input, not a translation.
    const untouched = pivot && original !== null && sameList(u.target, original);
    return untouched ? { ...rest, target: u.target.map(() => "") } : rest;
  };
  const emit = (event: TranslationEvent) => {
    for (const l of listeners) l(event);
  };
  const machine = (u: MockUnit, pivot: boolean): string[] => {
    const input = pivot && u.original?.every((t) => t !== "") ? u.original : [u.source];
    if (u.plural_slots.length > 0) return ["%n súbor", "%n súbory", "%n súborov"];
    return input.map((t) => `SK·${t}`);
  };
  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

  return {
    appInfo: async () => ({ version: "0.1.0-mock", cldr: "48.2.0" }),
    listLanguages: async () => [
      { id: "cs", name: "Czech" },
      { id: "de_DE", name: "German" },
      { id: "en", name: "English" },
      { id: "sk_SK", name: "Slovak" },
    ],
    pickFolder: async () => "/demo",
    openFolder: async (path) => {
      catalogs = sampleProject();
      return { root: path, catalogs: catalogs.map(summary), warnings: [] };
    },
    closeFolder: async () => {
      catalogs = null;
    },
    listCatalogs: async () => (catalogs ?? []).map(summary),
    listUnits: async (path, filter: UnitFilter, offset, limit) => {
      const c = find(path);
      refresh(c);
      const needle = filter.search?.trim().toLowerCase() ?? "";
      const all = c.units.filter(
        (u) =>
          (filter.statuses.length === 0 || filter.statuses.includes(u.status)) &&
          (!filter.errors_only || u.issues.some((i) => i.severity === "error")) &&
          (needle === "" ||
            [u.source, u.id, ...u.target, ...(u.existing ?? []), u.context.group ?? ""].some((t) =>
              t.toLowerCase().includes(needle),
            )),
      );
      return {
        units: all.slice(offset, offset + limit).map((u) => view(u, c.summary.pivot !== null)),
        matching: all.length,
      };
    },
    setTranslation: async (path, id, forms, done) => {
      const c = find(path);
      const u = c.units.find((x) => x.id === id);
      if (!u) throw new MockError("unknown-unit", id);
      u.target = forms;
      u.status = done ? "done" : "check";
      c.dirty = true;
      refresh(c);
      return view(u, c.summary.pivot !== null);
    },
    setDone: async (path, ids, done) => {
      const c = find(path);
      const out: UnitView[] = [];
      for (const u of c.units.filter((x) => ids.includes(x.id))) {
        if (u.target.some((t) => t === "")) continue;
        u.status = done ? "done" : "check";
        if (u.original && sameList(u.target, u.original) && c.summary.pivot) {
          // Confirming the pivot text as is: keep it, mark it.
          u.original = null;
        }
        c.dirty = true;
        out.push(view(u, c.summary.pivot !== null));
      }
      refresh(c);
      return out;
    },
    save: async (path) => {
      const written: string[] = [];
      let held = 0;
      for (const c of catalogs ?? []) {
        if (!c.dirty || (path !== null && c.summary.path !== path)) continue;
        refresh(c);
        held += c.units.filter((u) => u.issues.some((i) => i.severity === "error")).length;
        c.dirty = held > 0;
        written.push(c.summary.path);
      }
      return { written, held_back: held };
    },
    setPivot: async (path, pivot) => {
      const c = find(path);
      c.summary.pivot = pivot;
      c.summary.pivot_language = pivot === null ? null : pivot.startsWith("de") ? "German" : pivot;
      return summary(c);
    },
    getGlossary: async () => glossary,
    setGlossary: async (_locale, entries) => {
      glossary = entries.filter((e) => e.term.trim() !== "");
      return glossary;
    },
    getModelSettings: async () => settings,
    setModelSettings: async (next) => {
      settings = next;
      return settings;
    },
    listModels: async () => {
      if (options.ollamaRunning === false) {
        throw new MockError(
          "backend",
          "network error talking to backend `ollama`: Connection refused",
        );
      }
      return MODELS;
    },
    translateOne: async (path, id) => {
      const c = find(path);
      const u = c.units.find((x) => x.id === id);
      if (!u) throw new MockError("unknown-unit", id);
      if (!settings.model) throw new MockError("needs-model", "no translation model is selected");
      u.target = machine(u, c.summary.pivot !== null);
      u.status = "check";
      c.dirty = true;
      refresh(c);
      return view(u, c.summary.pivot !== null);
    },
    startTranslation: async (scope) => {
      if (!settings.model) throw new MockError("needs-model", "no translation model is selected");
      const statuses = scope.statuses.length > 0 ? scope.statuses : ["to-do"];
      const plan = (catalogs ?? [])
        .filter((c) => scope.catalogs.length === 0 || scope.catalogs.includes(c.summary.path))
        .flatMap((c) => {
          refresh(c);
          return c.units.filter((u) => statuses.includes(u.status)).map((u) => ({ c, u }));
        });
      cancelled = false;
      const total = plan.length;
      void (async () => {
        emit({ progress: { event: "started", total }, unit: null, catalog: null });
        let done = 0;
        for (const { c, u } of plan) {
          if (cancelled) {
            emit({
              progress: { event: "finished", translated: done, failed: 0, cancelled: true },
              unit: null,
              catalog: null,
            });
            return;
          }
          await sleep(delayMs);
          u.target = machine(u, c.summary.pivot !== null);
          u.status = "check";
          c.dirty = true;
          done += 1;
          emit({
            progress: {
              event: "unit",
              catalog: c.summary.path,
              id: u.id,
              done,
              total,
              translated: true,
              failure: null,
            },
            unit: view(u, c.summary.pivot !== null),
            catalog: summary(c),
          });
        }
        emit({
          progress: { event: "finished", translated: done, failed: 0, cancelled: false },
          unit: null,
          catalog: null,
        });
      })();
      return total;
    },
    cancelTranslation: async () => {
      cancelled = true;
    },
    onTranslation: async (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
