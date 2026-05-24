// Settings view — manifest editor for the currently-open project.
//
// Four sections: Project meta (read-only), Locales, Catalogs, Backend.
// Every mutation auto-persists via the Tauri command surface; there is no
// Save button. A transient "Saved" badge confirms each write.

import { useCallback, useEffect, useRef, useState } from "react";
import {
  addCatalogToProject,
  pickCatalogFileForProject,
  removeCatalogFromProject,
  removeLocaleFromProject,
  setBackendInProject,
  updateLocaleInProject,
} from "../../lib/tauri";
import type {
  BackendConfig,
  BackendKind,
  CatalogFormat,
  LocaleConfig,
  ProjectOpenResponse,
  ProjectSummary,
  RegisterOverride,
} from "../../lib/types";

// ── Prop types ────────────────────────────────────────────────────────────────

interface Props {
  summary: ProjectSummary;
  onMutation: (response: ProjectOpenResponse) => void;
  flashError: (msg: string) => void;
  flashInfo: (msg: string) => void;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  return String(e);
}

function shortenPath(p: string, segments = 2): string {
  const parts = p.replace(/\\/g, "/").split("/");
  if (parts.length <= segments) return p;
  return `…/${parts.slice(-segments).join("/")}`;
}

/** Infer a CatalogFormat from a file extension. Falls back to "qt-ts". */
function guessFormat(filePath: string): CatalogFormat {
  const lower = filePath.toLowerCase();
  if (lower.endsWith(".po") || lower.endsWith(".pot")) return "gettext-po";
  if (lower.endsWith(".json")) return "icu-json";
  return "qt-ts";
}

/** Compute manifest-relative path when the file is under project root. */
function relativize(absPath: string, projectRoot: string): string {
  const normAbs = absPath.replace(/\\/g, "/");
  const normRoot = projectRoot.replace(/\\/g, "/").replace(/\/?$/, "/");
  if (normAbs.startsWith(normRoot)) {
    return normAbs.slice(normRoot.length);
  }
  return absPath;
}

// ── Saved badge — transient per-card confirmation ─────────────────────────────

function useSavedBadge(): [boolean, () => void] {
  const [visible, setVisible] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const flash = useCallback(() => {
    setVisible(true);
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = setTimeout(() => setVisible(false), 2000);
  }, []);

  useEffect(
    () => () => {
      if (timer.current !== null) clearTimeout(timer.current);
    },
    [],
  );

  return [visible, flash];
}

// ── Card wrapper ──────────────────────────────────────────────────────────────

function Card({
  title,
  saved,
  children,
}: {
  title: string;
  saved: boolean;
  children: React.ReactNode;
}) {
  return (
    <section className="rounded-lg border border-border-default bg-bg-surface p-5 flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-fg-primary">{title}</h2>
        {saved && (
          <span
            className="text-xs text-state-finished font-medium animate-fade-in"
            aria-live="polite"
          >
            Saved
          </span>
        )}
      </div>
      {children}
    </section>
  );
}

// ── Inline label + input row ──────────────────────────────────────────────────

function FieldRow({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center gap-3">
      <span className="w-32 shrink-0 text-xs text-fg-tertiary">{label}</span>
      <div className="flex-1 min-w-0">{children}</div>
    </div>
  );
}

// ── Base input styles ─────────────────────────────────────────────────────────

const inputCls =
  "h-7 w-full rounded border border-border-default bg-bg-base px-2 text-xs text-fg-primary focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent placeholder:text-fg-disabled";

const selectCls =
  "h-7 rounded border border-border-default bg-bg-base px-1.5 text-xs text-fg-primary focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent";

const btnCls =
  "h-7 px-3 rounded border border-border-default bg-transparent text-xs font-medium text-fg-secondary hover:bg-bg-hover hover:text-fg-primary hover:border-border-strong active:bg-bg-selected transition-colors duration-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent";

const btnPrimaryCls =
  "h-7 px-3 rounded border border-accent bg-accent/10 text-xs font-medium text-accent hover:bg-accent/20 active:bg-accent/30 transition-colors duration-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent";

const btnDangerCls =
  "h-7 px-3 rounded border border-border-default bg-transparent text-xs font-medium text-severity-hard hover:bg-severity-hard-bg hover:border-severity-hard-border active:bg-severity-hard-bg/60 transition-colors duration-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent";

// ── Section 1 — Project meta (read-only) ──────────────────────────────────────

function ProjectMetaCard({ summary }: { summary: ProjectSummary }) {
  return (
    <Card title="Project" saved={false}>
      <FieldRow label="Name">
        <span className="text-xs text-fg-secondary">{summary.name}</span>
      </FieldRow>
      <FieldRow label="Schema version">
        <span className="text-xs text-fg-secondary">v{summary.schema}</span>
      </FieldRow>
      <FieldRow label="Root path">
        <span
          className="text-xs text-fg-secondary font-mono"
          title={summary.root}
        >
          {shortenPath(summary.root)}
        </span>
      </FieldRow>
      {summary.glossary_path && (
        <FieldRow label="Glossary">
          <span
            className="text-xs text-fg-secondary font-mono"
            title={summary.glossary_path}
          >
            {shortenPath(summary.glossary_path)}
          </span>
        </FieldRow>
      )}
    </Card>
  );
}

// ── Section 2 — Locales ───────────────────────────────────────────────────────

type LocaleRow = {
  id: string;
  register: RegisterOverride | null;
  variant: string;
  lengthWarnRatio: string;
};

function emptyLocaleRow(): LocaleRow {
  return { id: "", register: null, variant: "", lengthWarnRatio: "" };
}

function toLocaleConfig(row: LocaleRow): LocaleConfig {
  const ratio = parseFloat(row.lengthWarnRatio);
  return {
    register: row.register ?? undefined,
    variant: row.variant || undefined,
    length_warn_ratio: Number.isFinite(ratio) ? ratio : undefined,
  };
}

function LocalesCard({
  summary,
  onMutation,
  flashError,
}: {
  summary: ProjectSummary;
  onMutation: (r: ProjectOpenResponse) => void;
  flashError: (m: string) => void;
}) {
  const [saved, flashSaved] = useSavedBadge();
  const [addRow, setAddRow] = useState<LocaleRow | null>(null);
  const [busy, setBusy] = useState(false);

  const handleUpdate = useCallback(
    async (id: string, config: LocaleConfig) => {
      setBusy(true);
      try {
        const resp = await updateLocaleInProject(id, config);
        onMutation(resp);
        flashSaved();
      } catch (e) {
        flashError(`Could not update locale: ${formatError(e)}`);
      } finally {
        setBusy(false);
      }
    },
    [onMutation, flashError, flashSaved],
  );

  const handleRemove = useCallback(
    async (id: string) => {
      // Client-side guard: refuse to remove the last locale.
      if (summary.locales.length <= 1) {
        flashError("A project needs at least one locale.");
        return;
      }
      // Guard: refuse if any catalog uses this locale.
      const inUse = summary.catalogs.filter((c) => c.locale === id);
      if (inUse.length > 0) {
        flashError(
          `Locale "${id}" is in use by ${inUse.length} catalog(s). Remove or reassign them first.`,
        );
        return;
      }
      const ok = window.confirm(`Remove locale "${id}" from the project?`);
      if (!ok) return;
      setBusy(true);
      try {
        const resp = await removeLocaleFromProject(id);
        onMutation(resp);
        flashSaved();
      } catch (e) {
        flashError(`Could not remove locale: ${formatError(e)}`);
      } finally {
        setBusy(false);
      }
    },
    [
      summary.locales.length,
      summary.catalogs,
      onMutation,
      flashError,
      flashSaved,
    ],
  );

  const handleAddSubmit = useCallback(async () => {
    if (!addRow?.id.trim()) {
      flashError("Locale id is required.");
      return;
    }
    setBusy(true);
    try {
      const resp = await updateLocaleInProject(
        addRow.id.trim(),
        toLocaleConfig(addRow),
      );
      onMutation(resp);
      flashSaved();
      setAddRow(null);
    } catch (e) {
      flashError(`Could not add locale: ${formatError(e)}`);
    } finally {
      setBusy(false);
    }
  }, [addRow, onMutation, flashError, flashSaved]);

  return (
    <Card title="Locales" saved={saved}>
      <div className="overflow-x-auto -mx-1">
        <table className="w-full text-xs border-collapse">
          <thead>
            <tr className="text-fg-tertiary border-b border-border-subtle">
              <th className="text-left py-1.5 px-1 font-medium">ID</th>
              <th className="text-left py-1.5 px-1 font-medium">Register</th>
              <th className="text-left py-1.5 px-1 font-medium">Variant</th>
              <th className="text-left py-1.5 px-1 font-medium w-24">
                Length ratio
              </th>
              <th className="py-1.5 px-1" aria-label="Actions" />
            </tr>
          </thead>
          <tbody>
            {summary.locales.map((id) => (
              <LocaleTableRow
                key={id}
                id={id}
                disabled={busy}
                onUpdate={handleUpdate}
                onRemove={handleRemove}
              />
            ))}
          </tbody>
        </table>
      </div>

      {/* Add locale form */}
      {addRow === null ? (
        <button
          type="button"
          className={btnCls}
          disabled={busy}
          onClick={() => setAddRow(emptyLocaleRow())}
        >
          Add locale
        </button>
      ) : (
        <AddLocaleForm
          row={addRow}
          onChange={setAddRow}
          onSubmit={handleAddSubmit}
          onCancel={() => setAddRow(null)}
          disabled={busy}
        />
      )}
    </Card>
  );
}

function LocaleTableRow({
  id,
  disabled,
  onUpdate,
  onRemove,
}: {
  id: string;
  disabled: boolean;
  onUpdate: (id: string, config: LocaleConfig) => Promise<void>;
  onRemove: (id: string) => Promise<void>;
}) {
  const [register, setRegister] = useState<RegisterOverride | "">("");
  const [variant, setVariant] = useState("");
  const [lengthRatio, setLengthRatio] = useState("");

  const commit = useCallback(
    (r: RegisterOverride | "", v: string, lr: string) => {
      const ratio = parseFloat(lr);
      onUpdate(id, {
        register: r || undefined,
        variant: v || undefined,
        length_warn_ratio: Number.isFinite(ratio) ? ratio : undefined,
      }).catch(() => {
        // error already surfaced by parent
      });
    },
    [id, onUpdate],
  );

  return (
    <tr className="border-b border-border-subtle last:border-0 hover:bg-bg-hover/40">
      <td className="py-1.5 px-1 font-mono text-fg-primary">{id}</td>
      <td className="py-1.5 px-1">
        <select
          aria-label={`Register for ${id}`}
          value={register}
          disabled={disabled}
          className={`${selectCls} w-full`}
          onChange={(e) => {
            const v = e.target.value as RegisterOverride | "";
            setRegister(v);
          }}
          onBlur={() => commit(register, variant, lengthRatio)}
        >
          <option value="">—</option>
          <option value="formal">formal</option>
          <option value="informal">informal</option>
          <option value="neutral">neutral</option>
        </select>
      </td>
      <td className="py-1.5 px-1">
        <input
          aria-label={`Variant for ${id}`}
          type="text"
          value={variant}
          disabled={disabled}
          placeholder="e.g. es_419"
          className={inputCls}
          onChange={(e) => setVariant(e.target.value)}
          onBlur={() => commit(register, variant, lengthRatio)}
        />
      </td>
      <td className="py-1.5 px-1">
        <input
          aria-label={`Length warn ratio for ${id}`}
          type="number"
          step="0.05"
          min="1"
          max="3"
          value={lengthRatio}
          disabled={disabled}
          placeholder="1.3"
          className={inputCls}
          onChange={(e) => setLengthRatio(e.target.value)}
          onBlur={() => commit(register, variant, lengthRatio)}
        />
      </td>
      <td className="py-1.5 px-1">
        <button
          type="button"
          aria-label={`Remove locale ${id}`}
          className={btnDangerCls}
          disabled={disabled}
          onClick={() => void onRemove(id)}
        >
          Remove
        </button>
      </td>
    </tr>
  );
}

function AddLocaleForm({
  row,
  onChange,
  onSubmit,
  onCancel,
  disabled,
}: {
  row: LocaleRow;
  onChange: (r: LocaleRow) => void;
  onSubmit: () => void;
  onCancel: () => void;
  disabled: boolean;
}) {
  return (
    <fieldset
      disabled={disabled}
      className="border border-border-subtle rounded-md p-3 flex flex-col gap-2"
    >
      <legend className="px-1 text-xs text-fg-tertiary font-medium">
        Add locale
      </legend>
      <div className="flex flex-wrap gap-2">
        <div className="flex flex-col gap-0.5">
          <label className="text-xs text-fg-tertiary" htmlFor="add-locale-id">
            ID *
          </label>
          <input
            id="add-locale-id"
            type="text"
            required
            placeholder="de_DE"
            value={row.id}
            className={`${inputCls} w-28`}
            onChange={(e) => onChange({ ...row, id: e.target.value })}
          />
        </div>
        <div className="flex flex-col gap-0.5">
          <label
            className="text-xs text-fg-tertiary"
            htmlFor="add-locale-register"
          >
            Register
          </label>
          <select
            id="add-locale-register"
            value={row.register ?? ""}
            className={selectCls}
            onChange={(e) =>
              onChange({
                ...row,
                register: (e.target.value as RegisterOverride) || null,
              })
            }
          >
            <option value="">—</option>
            <option value="formal">formal</option>
            <option value="informal">informal</option>
            <option value="neutral">neutral</option>
          </select>
        </div>
        <div className="flex flex-col gap-0.5">
          <label
            className="text-xs text-fg-tertiary"
            htmlFor="add-locale-variant"
          >
            Variant
          </label>
          <input
            id="add-locale-variant"
            type="text"
            placeholder="es_419"
            value={row.variant}
            className={`${inputCls} w-24`}
            onChange={(e) => onChange({ ...row, variant: e.target.value })}
          />
        </div>
        <div className="flex flex-col gap-0.5">
          <label
            className="text-xs text-fg-tertiary"
            htmlFor="add-locale-ratio"
          >
            Length ratio
          </label>
          <input
            id="add-locale-ratio"
            type="number"
            step="0.05"
            min="1"
            max="3"
            placeholder="1.3"
            value={row.lengthWarnRatio}
            className={`${inputCls} w-20`}
            onChange={(e) =>
              onChange({ ...row, lengthWarnRatio: e.target.value })
            }
          />
        </div>
      </div>
      <div className="flex gap-2 justify-end">
        <button type="button" className={btnCls} onClick={onCancel}>
          Cancel
        </button>
        <button
          type="button"
          className={btnPrimaryCls}
          onClick={onSubmit}
          disabled={!row.id.trim()}
        >
          Add
        </button>
      </div>
    </fieldset>
  );
}

// ── Section 3 — Catalogs ──────────────────────────────────────────────────────

type AddCatalogFormState = {
  path: string;
  format: CatalogFormat;
  locale: string;
};

function emptyAddCatalogForm(
  defaultLocale: string,
  filePath?: string,
  projectRoot?: string,
): AddCatalogFormState {
  const relPath =
    filePath && projectRoot
      ? relativize(filePath, projectRoot)
      : (filePath ?? "");
  return {
    path: relPath,
    format: filePath ? guessFormat(filePath) : "qt-ts",
    locale: defaultLocale,
  };
}

function CatalogsCard({
  summary,
  onMutation,
  flashError,
  flashInfo,
}: {
  summary: ProjectSummary;
  onMutation: (r: ProjectOpenResponse) => void;
  flashError: (m: string) => void;
  flashInfo: (m: string) => void;
}) {
  const [saved, flashSaved] = useSavedBadge();
  const [addForm, setAddForm] = useState<AddCatalogFormState | null>(null);
  const [busy, setBusy] = useState(false);

  const handleRemove = useCallback(
    async (manifestPath: string) => {
      const ok = window.confirm(
        `Remove catalog "${manifestPath}" from the project manifest?\n\nThe file on disk will not be deleted.`,
      );
      if (!ok) return;
      setBusy(true);
      try {
        const resp = await removeCatalogFromProject(manifestPath);
        onMutation(resp);
        flashSaved();
        flashInfo(`Removed "${manifestPath}" from the manifest.`);
      } catch (e) {
        flashError(`Could not remove catalog: ${formatError(e)}`);
      } finally {
        setBusy(false);
      }
    },
    [onMutation, flashError, flashInfo, flashSaved],
  );

  const handlePickFile = useCallback(async () => {
    try {
      const picked = await pickCatalogFileForProject();
      if (!picked) return;
      setAddForm(
        emptyAddCatalogForm(summary.locales[0] ?? "", picked, summary.root),
      );
    } catch (e) {
      flashError(`File picker failed: ${formatError(e)}`);
    }
  }, [summary.locales, summary.root, flashError]);

  const handleAddSubmit = useCallback(async () => {
    if (!addForm) return;
    if (!addForm.path.trim()) {
      flashError("Catalog path is required.");
      return;
    }
    if (!addForm.locale.trim()) {
      flashError(
        "Locale is required. Add a locale first if the list is empty.",
      );
      return;
    }
    setBusy(true);
    try {
      const resp = await addCatalogToProject({
        path: addForm.path.trim(),
        format: addForm.format,
        locale: addForm.locale.trim(),
      });
      onMutation(resp);
      flashSaved();
      flashInfo(`Added "${addForm.path}" to the manifest.`);
      setAddForm(null);
    } catch (e) {
      flashError(`Could not add catalog: ${formatError(e)}`);
    } finally {
      setBusy(false);
    }
  }, [addForm, onMutation, flashError, flashInfo, flashSaved]);

  return (
    <Card title="Catalogs" saved={saved}>
      {summary.catalogs.length === 0 ? (
        <p className="text-xs text-fg-tertiary">No catalogs registered yet.</p>
      ) : (
        <div className="overflow-x-auto -mx-1">
          <table className="w-full text-xs border-collapse">
            <thead>
              <tr className="text-fg-tertiary border-b border-border-subtle">
                <th className="text-left py-1.5 px-1 font-medium">Path</th>
                <th className="text-left py-1.5 px-1 font-medium">Format</th>
                <th className="text-left py-1.5 px-1 font-medium">Locale</th>
                <th className="text-left py-1.5 px-1 font-medium">Status</th>
                <th className="py-1.5 px-1" aria-label="Actions" />
              </tr>
            </thead>
            <tbody>
              {summary.catalogs.map((c) => (
                <tr
                  key={c.absolute_path}
                  className="border-b border-border-subtle last:border-0 hover:bg-bg-hover/40"
                >
                  <td
                    className="py-1.5 px-1 font-mono text-fg-primary max-w-[220px] truncate"
                    title={c.manifest_path}
                  >
                    {c.manifest_path}
                  </td>
                  <td className="py-1.5 px-1 text-fg-secondary">{c.format}</td>
                  <td className="py-1.5 px-1 text-fg-secondary">{c.locale}</td>
                  <td className="py-1.5 px-1">
                    <CatalogStatusBadge status={c.status} />
                  </td>
                  <td className="py-1.5 px-1">
                    <button
                      type="button"
                      aria-label={`Remove catalog ${c.manifest_path}`}
                      className={btnDangerCls}
                      disabled={busy}
                      onClick={() => void handleRemove(c.manifest_path)}
                    >
                      Remove
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Add catalog affordance */}
      {addForm === null ? (
        <div className="flex gap-2">
          <button
            type="button"
            className={btnCls}
            disabled={busy}
            onClick={() => void handlePickFile()}
          >
            Pick file…
          </button>
          <button
            type="button"
            className={btnCls}
            disabled={busy}
            onClick={() =>
              setAddForm(
                emptyAddCatalogForm(
                  summary.locales[0] ?? "",
                  undefined,
                  summary.root,
                ),
              )
            }
          >
            Add manually
          </button>
        </div>
      ) : (
        <AddCatalogForm
          form={addForm}
          projectLocales={summary.locales}
          onChange={setAddForm}
          onSubmit={handleAddSubmit}
          onCancel={() => setAddForm(null)}
          disabled={busy}
        />
      )}
    </Card>
  );
}

function CatalogStatusBadge({ status }: { status: string }) {
  const styles: Record<string, string> = {
    ok: "bg-state-finished/20 text-state-finished border-state-finished/40",
    missing:
      "bg-severity-hard-bg text-severity-hard border-severity-hard-border",
    "format-mismatch":
      "bg-state-proposed/20 text-state-proposed border-state-proposed/40",
  };
  const cls =
    styles[status] ?? "bg-bg-hover text-fg-tertiary border-border-subtle";
  return (
    <span
      className={`inline-flex items-center px-1.5 py-0.5 rounded border text-xs font-medium ${cls}`}
    >
      {status}
    </span>
  );
}

function AddCatalogForm({
  form,
  projectLocales,
  onChange,
  onSubmit,
  onCancel,
  disabled,
}: {
  form: AddCatalogFormState;
  projectLocales: string[];
  onChange: (f: AddCatalogFormState) => void;
  onSubmit: () => void;
  onCancel: () => void;
  disabled: boolean;
}) {
  return (
    <fieldset
      disabled={disabled}
      className="border border-border-subtle rounded-md p-3 flex flex-col gap-2"
    >
      <legend className="px-1 text-xs text-fg-tertiary font-medium">
        Add catalog
      </legend>
      <div className="flex flex-col gap-1.5">
        <div className="flex flex-col gap-0.5">
          <label className="text-xs text-fg-tertiary" htmlFor="add-cat-path">
            Path (manifest-relative or absolute) *
          </label>
          <input
            id="add-cat-path"
            type="text"
            required
            placeholder="translations/app_de.ts"
            value={form.path}
            className={inputCls}
            onChange={(e) => {
              const p = e.target.value;
              onChange({
                ...form,
                path: p,
                format: guessFormat(p),
              });
            }}
          />
        </div>
        <div className="flex gap-3 flex-wrap">
          <div className="flex flex-col gap-0.5">
            <label
              className="text-xs text-fg-tertiary"
              htmlFor="add-cat-format"
            >
              Format
            </label>
            <select
              id="add-cat-format"
              value={form.format}
              className={selectCls}
              onChange={(e) =>
                onChange({ ...form, format: e.target.value as CatalogFormat })
              }
            >
              <option value="qt-ts">Qt Linguist (.ts)</option>
              <option value="gettext-po">Gettext PO (.po)</option>
              <option value="icu-json">ICU MessageFormat JSON (.json)</option>
            </select>
          </div>
          <div className="flex flex-col gap-0.5">
            <label
              className="text-xs text-fg-tertiary"
              htmlFor="add-cat-locale"
            >
              Locale *
            </label>
            {projectLocales.length > 0 ? (
              <select
                id="add-cat-locale"
                value={form.locale}
                className={selectCls}
                onChange={(e) => onChange({ ...form, locale: e.target.value })}
              >
                {projectLocales.map((l) => (
                  <option key={l} value={l}>
                    {l}
                  </option>
                ))}
              </select>
            ) : (
              <p className="text-xs text-fg-disabled italic">
                No locales yet — add a locale first.
              </p>
            )}
          </div>
        </div>
      </div>
      <div className="flex gap-2 justify-end">
        <button type="button" className={btnCls} onClick={onCancel}>
          Cancel
        </button>
        <button
          type="button"
          className={btnPrimaryCls}
          disabled={!form.path.trim() || !form.locale.trim()}
          onClick={onSubmit}
        >
          Add
        </button>
      </div>
    </fieldset>
  );
}

// ── Section 4 — Backend ───────────────────────────────────────────────────────

function BackendCard({
  summary,
  onMutation,
  flashError,
  flashInfo,
}: {
  summary: ProjectSummary;
  onMutation: (r: ProjectOpenResponse) => void;
  flashError: (m: string) => void;
  flashInfo: (m: string) => void;
}) {
  const [saved, flashSaved] = useSavedBadge();
  const existing = summary.backend;

  const [kind, setKind] = useState<BackendKind>(existing?.kind ?? "ollama");
  const [model, setModel] = useState(existing?.model ?? "");
  const [host, setHost] = useState(existing?.host ?? "http://localhost:11434");
  const [numCtx, setNumCtx] = useState(existing?.num_ctx?.toString() ?? "");
  const [busy, setBusy] = useState(false);

  const handleSave = useCallback(async () => {
    setBusy(true);
    const config: BackendConfig = {
      kind,
      model: model || undefined,
      host: host || undefined,
      num_ctx: numCtx ? parseInt(numCtx, 10) || undefined : undefined,
    };
    try {
      const resp = await setBackendInProject(config);
      onMutation(resp);
      flashSaved();
      flashInfo("Backend configuration saved.");
    } catch (e) {
      flashError(`Could not save backend: ${formatError(e)}`);
    } finally {
      setBusy(false);
    }
  }, [
    kind,
    model,
    host,
    numCtx,
    onMutation,
    flashError,
    flashInfo,
    flashSaved,
  ]);

  return (
    <Card title="Backend" saved={saved}>
      {!existing && (
        <p className="text-xs text-fg-tertiary">
          No backend configured. Fill in the form below and click "Save
          backend".
        </p>
      )}
      <div className="flex flex-col gap-2">
        <FieldRow label="Kind">
          <select
            aria-label="Backend kind"
            value={kind}
            disabled={busy}
            className={selectCls}
            onChange={(e) => setKind(e.target.value as BackendKind)}
          >
            <option value="ollama">Ollama (local)</option>
            <option value="manual">Manual (no model)</option>
            <option value="open-ai-compatible" disabled>
              OpenAI-compatible (M5)
            </option>
            <option value="agent" disabled>
              Claude Agent (M5)
            </option>
          </select>
        </FieldRow>
        {kind === "ollama" && (
          <>
            <FieldRow label="Model">
              <input
                type="text"
                aria-label="Model identifier"
                placeholder="gemma4:e2b"
                value={model}
                disabled={busy}
                className={inputCls}
                onChange={(e) => setModel(e.target.value)}
              />
            </FieldRow>
            <FieldRow label="Host">
              <input
                type="text"
                aria-label="Ollama host URL"
                placeholder="http://localhost:11434"
                value={host}
                disabled={busy}
                className={inputCls}
                onChange={(e) => setHost(e.target.value)}
              />
            </FieldRow>
            <FieldRow label="Context (tokens)">
              <input
                type="number"
                aria-label="Context window size"
                placeholder="8192"
                min="512"
                step="512"
                value={numCtx}
                disabled={busy}
                className={inputCls}
                onChange={(e) => setNumCtx(e.target.value)}
              />
            </FieldRow>
          </>
        )}
      </div>
      <div className="flex justify-end">
        <button
          type="button"
          className={btnPrimaryCls}
          disabled={busy}
          onClick={() => void handleSave()}
        >
          Save backend
        </button>
      </div>
    </Card>
  );
}

// ── Section 5 — Prompts (placeholder) ────────────────────────────────────────

function PromptsCard() {
  return (
    <Card title="Prompt templates" saved={false}>
      <p className="text-xs text-fg-disabled italic">
        Prompt template editing UI — coming in M4.10.
      </p>
    </Card>
  );
}

// ── Root component ────────────────────────────────────────────────────────────

export function ProjectSettings({
  summary,
  onMutation,
  flashError,
  flashInfo,
}: Props) {
  return (
    <section
      aria-label="Project settings"
      className="flex-1 overflow-y-auto p-5 flex flex-col gap-4 bg-bg-base"
    >
      <h1 className="text-sm font-semibold text-fg-secondary uppercase tracking-wide">
        Settings
      </h1>
      <ProjectMetaCard summary={summary} />
      <LocalesCard
        summary={summary}
        onMutation={onMutation}
        flashError={flashError}
      />
      <CatalogsCard
        summary={summary}
        onMutation={onMutation}
        flashError={flashError}
        flashInfo={flashInfo}
      />
      <BackendCard
        summary={summary}
        onMutation={onMutation}
        flashError={flashError}
        flashInfo={flashInfo}
      />
      <PromptsCard />
    </section>
  );
}
