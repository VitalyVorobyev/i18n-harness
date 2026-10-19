import { toast } from "@vitavision/ui";
import { useState } from "react";

import type { FolderView } from "../api";
import { errorMessage, useApi } from "../api";
import { rememberFolder } from "../lib/recent";
import { Start } from "../screens/Start";
import { Workspace } from "../screens/Workspace";

/** Start screen until a folder is open, then the workspace. */
export function App() {
  const api = useApi();
  const [folder, setFolder] = useState<FolderView | null>(null);
  const [opening, setOpening] = useState(false);

  const open = async (path: string) => {
    setOpening(true);
    try {
      const view = await api.openFolder(path);
      rememberFolder(view.root);
      setFolder(view);
      for (const warning of view.warnings) {
        toast({ tone: "warn", title: "Opened with a warning", description: warning });
      }
    } catch (error) {
      toast({
        tone: "error",
        title: "Could not open the folder",
        description: errorMessage(error),
      });
    } finally {
      setOpening(false);
    }
  };

  const sample = async () => {
    try {
      await open(await api.createSample());
    } catch (error) {
      toast({
        tone: "error",
        title: "Could not create the sample",
        description: errorMessage(error),
      });
    }
  };

  const close = async () => {
    await api.closeFolder();
    setFolder(null);
  };

  if (folder === null) {
    return (
      <Start opening={opening} onOpen={(path) => void open(path)} onSample={() => void sample()} />
    );
  }
  return <Workspace folder={folder} onClose={() => void close()} />;
}
