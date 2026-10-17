import { Button, Callout, Panel, Skeleton, ThemeToggle } from "@vitavision/ui";
import { FolderOpen, X } from "lucide-react";
import { useEffect, useState } from "react";

import type { ModelChoice } from "../api";
import { DEFAULT_MODEL_SETTINGS, errorMessage, useApi } from "../api";
import { plural } from "../lib/format";
import { folderName, forgetFolder, recentFolders } from "../lib/recent";

type ServerState =
  | { kind: "checking" }
  | { kind: "ready"; models: ModelChoice[] }
  | { kind: "down"; message: string };

export function Start(props: { opening: boolean; onOpen: (path: string) => void }) {
  const api = useApi();
  const [recent, setRecent] = useState(recentFolders);
  const [server, setServer] = useState<ServerState>({ kind: "checking" });

  useEffect(() => {
    let live = true;
    api
      .listModels(DEFAULT_MODEL_SETTINGS)
      .then((models) => live && setServer({ kind: "ready", models }))
      .catch((error: unknown) => live && setServer({ kind: "down", message: errorMessage(error) }));
    return () => {
      live = false;
    };
  }, [api]);

  const pick = async () => {
    const path = await api.pickFolder();
    if (path) props.onOpen(path);
  };

  return (
    <div className="flex min-h-dvh flex-col bg-ground text-fg">
      <div className="flex justify-end p-4">
        <ThemeToggle />
      </div>
      <main className="mx-auto flex w-full max-w-xl flex-1 flex-col gap-6 px-4 pb-16 pt-8">
        <header className="space-y-2">
          <h1 className="text-2xl font-semibold">i18n-harness</h1>
          <p className="text-fg-muted">
            Translate your application's text with a language model that runs on this computer. Open
            the folder that holds the translation files.
          </p>
        </header>

        <Panel>
          <div className="flex flex-col gap-3">
            <Button
              variant="primary"
              icon={<FolderOpen aria-hidden size={16} />}
              loading={props.opening}
              onClick={() => void pick()}
            >
              Open folder…
            </Button>
            <p className="text-sm text-fg-subtle">
              Qt Linguist (.ts), gettext (.po) and ICU message (.json) files are found
              automatically, with their languages.
            </p>
          </div>
        </Panel>

        {recent.length > 0 && (
          <section aria-labelledby="recent-heading" className="space-y-2">
            <h2 id="recent-heading" className="text-sm font-medium text-fg-muted">
              Recent folders
            </h2>
            <ul className="divide-y divide-line rounded-panel border border-line bg-surface">
              {recent.map((path) => (
                <li key={path} className="flex items-center gap-2 px-3 py-2">
                  <button
                    type="button"
                    className="min-w-0 flex-1 text-left"
                    onClick={() => props.onOpen(path)}
                    disabled={props.opening}
                  >
                    <span className="block truncate font-medium">{folderName(path)}</span>
                    <span className="block truncate text-xs text-fg-subtle">{path}</span>
                  </button>
                  <Button
                    variant="ghost"
                    size="sm"
                    aria-label={`Remove ${folderName(path)} from recent folders`}
                    icon={<X aria-hidden size={14} />}
                    onClick={() => {
                      forgetFolder(path);
                      setRecent(recentFolders());
                    }}
                  />
                </li>
              ))}
            </ul>
          </section>
        )}

        <section aria-label="Translation model" className="space-y-2">
          {server.kind === "checking" && <Skeleton className="h-14" />}
          {server.kind === "ready" && server.models.length > 0 && (
            <Callout tone="success" title="Ollama is running">
              {plural(server.models.length, "model", "models")} available:{" "}
              {server.models.map((m) => m.id.split("/").at(-1)).join(", ")}.
            </Callout>
          )}
          {server.kind === "ready" && server.models.length === 0 && (
            <Callout tone="warning" title="Ollama has no models">
              Download a model in Ollama to translate (for example a translation model from Hugging
              Face). You can still open files and edit translations.
            </Callout>
          )}
          {server.kind === "down" && (
            <Callout tone="warning" title="Ollama is not running">
              Start Ollama to translate with a local model. You can still open files and edit
              translations by hand.
              <span className="mt-1 block text-xs text-fg-subtle">{server.message}</span>
            </Callout>
          )}
        </section>
      </main>
    </div>
  );
}
