import {
  Button,
  Callout,
  Checkbox,
  Dialog,
  Field,
  Input,
  SegmentedControl,
  Tabs,
  toast,
} from "@vitavision/ui";
import { Plus, Trash2 } from "lucide-react";
import { useState } from "react";

import type { GlossaryEntry, ModelSettings, ServerKind } from "../api";
import { errorMessage, useApi } from "../api";
import { plural } from "../lib/format";

type Tab = "glossary" | "server";

interface Row extends GlossaryEntry {
  rowId: number;
}

let nextRowId = 0;
const withId = (entry: GlossaryEntry): Row => ({ ...entry, rowId: nextRowId++ });
const withoutId = ({ rowId: _rowId, ...entry }: Row): GlossaryEntry => entry;

/** One dialog for everything a project can be told: glossary and model server. */
export function SettingsDialog(props: {
  locale: string;
  language: string;
  glossary: GlossaryEntry[];
  settings: ModelSettings;
  onClose: () => void;
  onSaved: (glossary: GlossaryEntry[], settings: ModelSettings) => void;
}) {
  const api = useApi();
  const [tab, setTab] = useState<Tab>("glossary");
  const [entries, setEntries] = useState<Row[]>(() => props.glossary.map(withId));
  const [settings, setSettings] = useState<ModelSettings>(props.settings);
  const [testResult, setTestResult] = useState<{ ok: boolean; message: string } | null>(null);
  const [saving, setSaving] = useState(false);

  const update = (rowId: number, patch: Partial<GlossaryEntry>) =>
    setEntries(entries.map((e) => (e.rowId === rowId ? { ...e, ...patch } : e)));

  const test = async () => {
    setTestResult(null);
    try {
      const models = await api.listModels(settings);
      setTestResult({
        ok: true,
        message: `Connected: ${plural(models.length, "model", "models")} available.`,
      });
    } catch (error) {
      setTestResult({ ok: false, message: errorMessage(error) });
    }
  };

  const save = async () => {
    setSaving(true);
    try {
      const glossary = await api.setGlossary(
        props.locale,
        entries.filter((e) => e.term.trim() !== "").map(withoutId),
      );
      const saved = await api.setModelSettings(settings);
      props.onSaved(glossary, saved);
    } catch (error) {
      toast({
        tone: "error",
        title: "Could not save the settings",
        description: errorMessage(error),
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => !open && props.onClose()}
      title="Settings"
      className="w-[min(52rem,calc(100vw-2rem))]"
      footer={
        <>
          <Button variant="ghost" onClick={props.onClose}>
            Cancel
          </Button>
          <Button variant="primary" loading={saving} onClick={() => void save()}>
            Save settings
          </Button>
        </>
      }
    >
      <Tabs<Tab>
        label="Settings"
        active={tab}
        onSelect={setTab}
        items={[
          { id: "glossary", label: "Glossary", count: entries.length },
          { id: "server", label: "Model server" },
        ]}
      />

      {tab === "glossary" && (
        <div className="mt-4 space-y-3">
          <p className="text-sm text-fg-muted">
            Terms the model must keep unchanged (product names, class names such as ParamsManager,
            units such as mm) or always translate the same way. Translations here are for{" "}
            {props.language}.
          </p>
          <div className="max-h-[50vh] overflow-y-auto rounded-panel border border-line">
            <table className="w-full text-sm">
              <thead className="sticky top-0 bg-surface text-left text-xs text-fg-subtle">
                <tr>
                  <th className="px-2 py-1.5 font-medium">Term</th>
                  <th className="px-2 py-1.5 font-medium">Keep unchanged</th>
                  <th className="px-2 py-1.5 font-medium">{props.language}</th>
                  <th className="px-2 py-1.5 font-medium">Note</th>
                  <th className="w-8" />
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {entries.map((entry) => (
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
                    <td className="px-1 py-1">
                      <Button
                        variant="ghost"
                        size="sm"
                        aria-label={`Remove ${entry.term || "term"}`}
                        icon={<Trash2 aria-hidden size={14} />}
                        onClick={() => setEntries(entries.filter((e) => e.rowId !== entry.rowId))}
                      />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {entries.length === 0 && <p className="p-3 text-sm text-fg-subtle">No terms yet.</p>}
          </div>
          <Button
            variant="secondary"
            icon={<Plus aria-hidden size={16} />}
            onClick={() =>
              setEntries([
                ...entries,
                withId({ term: "", keep: true, translation: null, note: null }),
              ])
            }
          >
            Add term
          </Button>
        </div>
      )}

      {tab === "server" && (
        <div className="mt-4 space-y-4">
          <Field label="Kind of server" as="group">
            <SegmentedControl
              aria-label="Kind of server"
              value={settings.server}
              onValueChange={(server) =>
                setSettings({
                  ...settings,
                  server: server as ServerKind,
                  host: server === "ollama" ? "http://localhost:11434" : "http://localhost:1234/v1",
                  model: null,
                })
              }
              options={[
                { value: "ollama", label: "Ollama" },
                { value: "openai-compatible", label: "OpenAI-compatible" },
              ]}
            />
          </Field>
          <Field
            label="Address"
            description={
              settings.server === "ollama"
                ? "Where Ollama listens; the default is right unless you changed it."
                : "The API address up to /v1 (LM Studio, vLLM, llama.cpp server)."
            }
          >
            <Input
              value={settings.host}
              onChange={(e) => setSettings({ ...settings, host: e.target.value })}
            />
          </Field>
          <Field
            label="Context size (tokens)"
            description="Leave empty to use the model's default."
          >
            <Input
              inputMode="numeric"
              value={settings.num_ctx?.toString() ?? ""}
              onChange={(e) => {
                const n = Number.parseInt(e.target.value, 10);
                setSettings({ ...settings, num_ctx: Number.isFinite(n) && n > 0 ? n : null });
              }}
            />
          </Field>
          <div className="flex items-center gap-3">
            <Button variant="secondary" onClick={() => void test()}>
              Test connection
            </Button>
          </div>
          {testResult && (
            <Callout tone={testResult.ok ? "success" : "error"}>{testResult.message}</Callout>
          )}
        </div>
      )}
    </Dialog>
  );
}
