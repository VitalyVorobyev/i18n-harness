import {
  Button,
  Callout,
  Dialog,
  Field,
  Input,
  NumberInput,
  SegmentedControl,
  Tabs,
  toast,
} from "@vitavision/ui";
import { useState } from "react";

import type { CheckSettings, ModelSettings, ServerKind } from "../api";
import { errorMessage, useApi } from "../api";
import { plural } from "../lib/format";

type Tab = "checks" | "server";

/** How the project's strings are checked, and which model server translates them. */
export function SettingsDialog(props: {
  locale: string;
  language: string;
  settings: ModelSettings;
  checks: CheckSettings;
  onClose: () => void;
  onSaved: (settings: ModelSettings) => void;
}) {
  const api = useApi();
  const [tab, setTab] = useState<Tab>("checks");
  const [settings, setSettings] = useState<ModelSettings>(props.settings);
  const [checks, setChecks] = useState<CheckSettings>(props.checks);
  const [testResult, setTestResult] = useState<{ ok: boolean; message: string } | null>(null);
  const [saving, setSaving] = useState(false);

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
      await api.setCheckSettings(props.locale, checks);
      props.onSaved(await api.setModelSettings(settings));
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
          { id: "checks", label: "Checks" },
          { id: "server", label: "Model server" },
        ]}
      />

      {tab === "checks" && (
        <div className="mt-4 space-y-4">
          <p className="text-sm text-fg-muted">
            When a {props.language} translation is flagged as too long to fit. A warning never stops
            a translation from being saved.
          </p>
          <Field
            label="Warn when a translation is longer than"
            description="Times the length of the original. Translations often run 20–40% longer than English."
          >
            {/* A field with a unit is sized through its container. */}
            <div className="w-32">
              <NumberInput
                value={checks.length_ratio}
                precision={2}
                min={0.5}
                max={10}
                step={0.1}
                unit="×"
                onValueChange={(length_ratio) => setChecks({ ...checks, length_ratio })}
              />
            </div>
          </Field>
          <Field
            label="Skip originals shorter than"
            description="Short labels have too little text for a ratio to mean anything."
          >
            <div className="w-48">
              <NumberInput
                value={checks.length_min_chars}
                precision={0}
                min={0}
                max={1000}
                step={1}
                unit="characters"
                onValueChange={(n) => setChecks({ ...checks, length_min_chars: Math.round(n) })}
              />
            </div>
          </Field>
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
