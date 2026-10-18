// An in-memory stand-in for the desktop shell, for `bun run dev:mock`, unit
// tests and the Playwright suite. It follows the session's rules closely
// enough to exercise the UI: statuses, pivot input, translation progress.

import type { Api } from "./api";
import type {
  CatalogSummary,
  CheckSettings,
  GlossaryEntry,
  ModelChoice,
  ModelSettings,
  SharedString,
  SiblingView,
  StatusCounts,
  TermHit,
  TranslateScope,
  TranslationEvent,
  UnitFilter,
  UnitRow,
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
    terms: [],
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
        path: "/demo/translations/Viewer_sk_SK.ts",
        name: "Viewer_sk_SK.ts",
        relative_path: "translations/Viewer_sk_SK.ts",
        format: "qt-ts",
        format_name: "Qt Linguist",
        locale: "sk_SK",
        language: "Slovak",
        pivot: null,
        pivot_language: null,
      },
      dirty: false,
      units: [
        unit("Settings::Filter Size", "Filter Size", "Veľkosť filtra", "Settings", {
          status: "done",
        }),
        unit("Viewer::Zoom in", "Zoom in", "Priblížiť", "Viewer", { status: "done" }),
        unit("Viewer::Zoom out", "Zoom out", null, "Viewer"),
        unit(
          "Viewer::Fit the image to the window",
          "Fit the image to the window",
          "Prispôsobiť veľkosť obrázka tak, aby sa celý zmestil do okna prehliadača",
          "Viewer",
          { status: "check", state: "proposed" },
        ),
      ],
    },
    {
      summary: {
        path: "/demo/translations/Report_sk_SK.ts",
        name: "Report_sk_SK.ts",
        relative_path: "translations/Report_sk_SK.ts",
        format: "qt-ts",
        format_name: "Qt Linguist",
        locale: "sk_SK",
        language: "Slovak",
        pivot: null,
        pivot_language: null,
      },
      dirty: false,
      units: [
        unit("Viewer::Zoom in", "Zoom in", "Zväčšiť", "Viewer", { status: "done" }),
        unit("Report::Print", "Print", null, "Report"),
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

const DEFAULT_CHECKS: CheckSettings = { length_ratio: 1.3, length_min_chars: 8 };

function issuesFor(u: MockUnit, pivot: boolean, checks: CheckSettings): UnitView["issues"] {
  const hasText = u.target.some((t) => t !== "");
  const edited = !sameList(u.target, u.original ?? []);
  // As the session does: untouched pivot text is input, not a translation.
  if (!hasText || (pivot && !edited)) return [];
  const want = placeholders(u.source);
  const errors = u.target.flatMap((t) => {
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
  const longest = Math.max(...u.target.map((t) => t.length));
  const long =
    u.source.length >= checks.length_min_chars && longest > u.source.length * checks.length_ratio;
  return long
    ? [
        ...errors,
        {
          severity: "warning" as const,
          message: `The translation is much longer than the original (${longest} vs ${u.source.length} characters) and may not fit`,
        },
      ]
    : errors;
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
  const checks = new Map<string, CheckSettings>();
  const checksFor = (locale: string) => checks.get(locale) ?? DEFAULT_CHECKS;
  let glossary: GlossaryEntry[] = [
    { term: "ParamsManager", keep: true, translation: null, note: "Class name" },
  ];
  const listeners = new Set<(event: TranslationEvent) => void>();
  let cancelled = false;

  // A required translation missing from a translated string, as the
  // session's glossary-issues scope sees it (without inflection tolerance).
  const missingTerms = (u: MockUnit): GlossaryEntry[] =>
    u.status === "to-do"
      ? []
      : glossary.filter((g) => {
          const source = u.source.toLowerCase();
          const target = u.target.join(" ").toLowerCase();
          if (g.keep) return u.source.includes(g.term) && !u.target.join(" ").includes(g.term);
          return (
            g.translation !== null &&
            source.includes(g.term.toLowerCase()) &&
            !target.includes(g.translation.toLowerCase())
          );
        });

  const termHits = (u: MockUnit): TermHit[] => {
    const broken = missingTerms(u);
    return glossary
      .filter((g) =>
        g.keep ? u.source.includes(g.term) : u.source.toLowerCase().includes(g.term.toLowerCase()),
      )
      .map((g) => ({
        term: g.term,
        keep: g.keep,
        translation: g.translation,
        broken: broken.includes(g),
      }));
  };

  const inScope = (scope: TranslateScope) => {
    const statuses =
      scope.statuses.length > 0
        ? scope.statuses
        : scope.glossary_issues
          ? (["to-do", "check", "done"] as UnitStatus[])
          : (["to-do"] as UnitStatus[]);
    return (catalogs ?? [])
      .filter((c) => scope.catalogs.length === 0 || scope.catalogs.includes(c.summary.path))
      .flatMap((c) => {
        refresh(c);
        return c.units
          .filter((u) => statuses.includes(u.status))
          .filter((u) => !scope.glossary_issues || missingTerms(u).length > 0)
          .map((u) => ({ c, u }));
      });
  };

  const refresh = (c: MockCatalog) => {
    const pivot = c.summary.pivot !== null;
    for (const u of c.units) {
      u.status = statusFor(u, pivot);
      u.issues = issuesFor(u, pivot, checksFor(c.summary.locale));
    }
  };
  // A translation, as the session judges one: text that is not untouched pivot input.
  const translation = (c: MockCatalog, u: MockUnit): string | null => {
    if (u.target.every((t) => t === "")) return null;
    if (c.summary.pivot !== null && sameList(u.target, u.original ?? [])) return null;
    return u.target.join("\u0000");
  };
  // The same string in other files: same language, id and original.
  const group = (c: MockCatalog, u: MockUnit) =>
    (catalogs ?? []).flatMap((other) =>
      other.summary.locale === c.summary.locale
        ? other.units
            .filter((x) => x.id === u.id && x.source === u.source)
            .map((x) => ({ c: other, u: x }))
        : [],
    );
  const siblingsOf = (c: MockCatalog, u: MockUnit) => group(c, u).filter((m) => m.u !== u);
  const relation = (c: MockCatalog, u: MockUnit) => {
    const group_ = group(c, u);
    const texts = new Set(
      group_.map((m) => translation(m.c, m.u)).filter((t): t is string => t !== null),
    );
    return { shared: group_.length > 1, inconsistent: group_.length > 1 && texts.size > 1 };
  };
  const summary = (c: MockCatalog): CatalogSummary => {
    refresh(c);
    const counts: StatusCounts = {
      total: c.units.length,
      to_do: 0,
      check: 0,
      done: 0,
      errors: 0,
      warnings: 0,
      inconsistent: 0,
      only_here: 0,
      glossary: 0,
      glossary_broken: 0,
    };
    for (const u of c.units) {
      const hits = termHits(u);
      if (hits.length > 0) counts.glossary += 1;
      if (hits.some((h) => h.broken)) counts.glossary_broken += 1;
      if (u.status === "to-do") counts.to_do += 1;
      if (u.status === "check") counts.check += 1;
      if (u.status === "done") counts.done += 1;
      if (u.issues.some((i) => i.severity === "error")) counts.errors += 1;
      if (u.issues.some((i) => i.severity === "warning")) counts.warnings += 1;
      const r = relation(c, u);
      if (!r.shared) counts.only_here += 1;
      if (r.inconsistent) counts.inconsistent += 1;
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
    const terms = termHits(u);
    return untouched ? { ...rest, terms, target: u.target.map(() => "") } : { ...rest, terms };
  };
  const member = (m: { c: MockCatalog; u: MockUnit }): SiblingView => {
    refresh(m.c);
    const text = translation(m.c, m.u);
    return {
      path: m.c.summary.path,
      file: m.c.summary.name,
      target: text === null ? m.u.target.map(() => "") : m.u.target,
      existing: m.c.summary.pivot !== null ? m.u.original : null,
      status: m.u.status,
    };
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
    listRows: async (path, filter: UnitFilter) => {
      const scope = path === null ? (catalogs ?? []) : [find(path)];
      const needle = filter.search?.trim().toLowerCase() ?? "";
      return scope.flatMap((c) => {
        refresh(c);
        const pivot = c.summary.pivot !== null;
        return c.units.flatMap((u): UnitRow[] => {
          const v = view(u, pivot);
          const r = relation(c, u);
          const error = u.issues.some((i) => i.severity === "error");
          const warning = u.issues.some((i) => i.severity === "warning");
          const glossaryBroken = v.terms.some((t) => t.broken);
          const shown =
            {
              everything: true,
              problems: error,
              warnings: warning,
              inconsistent: r.inconsistent,
              shared: r.shared,
              glossary: v.terms.length > 0,
              "glossary-broken": glossaryBroken,
            }[filter.show] &&
            (filter.term === null || v.terms.some((t) => t.term === filter.term));
          const matches =
            needle === "" ||
            [u.source, u.id, ...v.target, ...(u.existing ?? []), u.context.group ?? ""].some((t) =>
              t.toLowerCase().includes(needle),
            );
          if (
            !shown ||
            !matches ||
            (filter.statuses.length > 0 && !filter.statuses.includes(u.status))
          ) {
            return [];
          }
          return [
            {
              path: c.summary.path,
              file: c.summary.name,
              id: u.id,
              source: u.source,
              existing: u.existing?.[0] ?? null,
              target: v.target[0] ?? "",
              status: u.status,
              group: u.context.group ?? null,
              error,
              warning,
              ...r,
              terms: v.terms.length,
              glossary_broken: glossaryBroken,
            },
          ];
        });
      });
    },
    siblings: async (path, id) => {
      const c = find(path);
      const u = c.units.find((x) => x.id === id);
      if (!u) throw new MockError("unknown-unit", id);
      return siblingsOf(c, u).map(member);
    },
    sharedStrings: async () => {
      const seen = new Set<string>();
      const out: SharedString[] = [];
      for (const c of catalogs ?? []) {
        for (const u of c.units) {
          const key = `${c.summary.locale}\u0000${u.id}\u0000${u.source}`;
          const members = group(c, u);
          if (members.length < 2 || seen.has(key)) continue;
          seen.add(key);
          const texts = members.map((m) => translation(m.c, m.u));
          const existing = members
            .filter((m) => m.c.summary.pivot !== null && m.u.original !== null)
            .map((m) => m.u.original!.join("\u0000"));
          out.push({
            id: u.id,
            source: u.source,
            group: u.context.group ?? null,
            language: c.summary.language,
            members: members.map(member),
            translations: new Set(texts.filter((t) => t !== null)).size,
            missing: texts.filter((t) => t === null).length,
            existing_differs: new Set(existing).size > 1,
          });
        }
      }
      return out;
    },
    useEverywhere: async (path, id) => {
      const c = find(path);
      const u = c.units.find((x) => x.id === id);
      if (!u || translation(c, u) === null || u.target.some((t) => t === "")) {
        throw new MockError("invalid-input", `"${id}" has no complete translation to copy`);
      }
      refresh(c);
      const status = u.status === "done" ? "done" : "check";
      const others = siblingsOf(c, u);
      for (const m of others) {
        m.u.target = [...u.target];
        m.u.status = status;
        m.c.dirty = true;
      }
      return others.length;
    },
    fillFromOtherFiles: async (paths, apply) => {
      const report = { filled: 0, conflicting: 0 };
      for (const c of (catalogs ?? []).filter(
        (x) => paths.length === 0 || paths.includes(x.summary.path),
      )) {
        refresh(c);
        for (const u of c.units.filter((x) => x.status === "to-do")) {
          const sources = siblingsOf(c, u).filter(
            (m) => translation(m.c, m.u) !== null && m.u.target.every((t) => t !== ""),
          );
          if (sources.length === 0) continue;
          const texts = new Set(sources.map((m) => translation(m.c, m.u)));
          if (texts.size > 1) {
            report.conflicting += 1;
            continue;
          }
          report.filled += 1;
          if (apply) {
            u.target = [...sources[0]!.u.target];
            u.status = sources.some((m) => m.u.status === "done") ? "done" : "check";
            c.dirty = true;
          }
        }
      }
      return report;
    },
    getUnit: async (path, id) => {
      const c = find(path);
      const u = c.units.find((x) => x.id === id);
      if (!u) throw new MockError("unknown-unit", id);
      refresh(c);
      return view(u, c.summary.pivot !== null);
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
    glossaryUsage: async (locale) =>
      glossary.map((g) => {
        const units = (catalogs ?? [])
          .filter((c) => c.summary.locale === locale)
          .flatMap((c) => {
            refresh(c);
            return c.units;
          });
        const uses = (u: MockUnit) =>
          g.keep
            ? u.source.includes(g.term)
            : u.source.toLowerCase().includes(g.term.toLowerCase());
        return {
          term: g.term,
          strings: units.filter(uses).length,
          breaking: units.filter((u) => uses(u) && missingTerms(u).includes(g)).length,
        };
      }),
    setGlossary: async (_locale, entries) => {
      glossary = entries.filter((e) => e.term.trim() !== "");
      return glossary;
    },
    pickTable: async () => "/demo/terms.csv",
    readGlossaryTable: async () => [
      { term: "Threshold", keep: false, translation: "Prahová hodnota", note: null },
      { term: "Blob", keep: true, translation: null, note: "Technical term" },
    ],
    planSize: async (scope) => inScope(scope).length,
    getCheckSettings: async (locale) => checksFor(locale),
    setCheckSettings: async (locale, next) => {
      if (!(next.length_ratio > 0)) {
        throw new MockError("invalid-input", "the length ratio must be a positive number");
      }
      checks.set(locale, next);
      return next;
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
      const plan = inScope(scope);
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
          const missing = scope.glossary_issues ? missingTerms(u) : [];
          u.target = machine(u, c.summary.pivot !== null).map((form) =>
            [form, ...missing.map((g) => g.translation ?? g.term)].join(" "),
          );
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
