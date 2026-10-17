import {
  Button,
  DropdownMenu,
  MenuItem,
  SegmentedControl,
  Select,
  ThemeToggle,
  Tooltip,
} from "@vitavision/ui";
import { ChevronDown, FolderClosed, Languages, Save, Settings } from "lucide-react";

import type { ModelChoice } from "../api";
import { modelLabel, modelNote, plural } from "../lib/format";
import { folderName } from "../lib/recent";

/** What the main area shows. */
export type View = "strings" | "glossary";

export function Toolbar(props: {
  root: string;
  view: View;
  onView: (view: View) => void;
  models: ModelChoice[] | null;
  modelsError: string | null;
  model: string | null;
  onModelChange: (id: string) => void;
  toDoInFile: number;
  toDoInFolder: number;
  /** Translated strings that break the glossary. */
  glossaryIssues: number;
  /** Strings to do that other files translate the same way. */
  fillable: number;
  onFill: () => void;
  running: boolean;
  dirty: boolean;
  onTranslateFile: () => void;
  onTranslateFolder: () => void;
  onApplyGlossary: () => void;
  onSave: () => void;
  onSettings: () => void;
  onClose: () => void;
}) {
  const modelOptions = (props.models ?? []).map((m) => ({
    value: m.id,
    label: modelLabel(m),
    note: modelNote(m),
  }));
  const canTranslate = props.model !== null && !props.running;

  return (
    <div className="flex items-center gap-3 border-b border-line bg-surface px-3 py-2">
      <div className="flex min-w-0 items-center gap-2">
        <FolderClosed aria-hidden size={16} className="shrink-0 text-fg-subtle" />
        <span className="truncate font-medium" title={props.root}>
          {folderName(props.root)}
        </span>
      </div>
      <SegmentedControl
        aria-label="View"
        size="md"
        value={props.view}
        onValueChange={(v) => props.onView(v as View)}
        options={[
          { value: "strings", label: "Strings" },
          { value: "glossary", label: "Glossary" },
        ]}
      />
      <div className="flex-1" />

      <div className="w-64">
        {props.modelsError === null ? (
          <Select
            aria-label="Translation model"
            value={props.model ?? ""}
            onValueChange={props.onModelChange}
            options={modelOptions}
            placeholder={props.models === null ? "Looking for models…" : "Choose a model"}
            disabled={props.running || props.models === null}
          />
        ) : (
          <Tooltip content={props.modelsError}>
            <span className="block truncate text-sm text-warn">Model server not reachable</span>
          </Tooltip>
        )}
      </div>

      <div className="flex">
        <Button
          variant="primary"
          icon={<Languages aria-hidden size={16} />}
          disabled={!canTranslate || props.toDoInFile === 0}
          className="rounded-r-none"
          onClick={props.onTranslateFile}
        >
          {props.toDoInFile > 0
            ? `Translate ${plural(props.toDoInFile, "string", "strings")}`
            : "Nothing to translate"}
        </Button>
        <DropdownMenu
          align="end"
          trigger={
            <Button
              variant="primary"
              aria-label="More translate options"
              disabled={props.running}
              className="rounded-l-none border-l border-signal-fg/30 px-2"
              icon={<ChevronDown aria-hidden size={16} />}
            />
          }
        >
          <MenuItem
            disabled={!canTranslate || props.toDoInFolder === 0}
            onSelect={props.onTranslateFolder}
          >
            Translate all files ({props.toDoInFolder.toLocaleString()} to do)
          </MenuItem>
          <MenuItem disabled={props.fillable === 0} onSelect={props.onFill}>
            {props.fillable > 0
              ? `Fill from other files (${props.fillable.toLocaleString()})`
              : "Nothing to fill from other files"}
          </MenuItem>
          <MenuItem
            disabled={!canTranslate || props.glossaryIssues === 0}
            onSelect={props.onApplyGlossary}
          >
            {props.glossaryIssues > 0
              ? `Apply the glossary (${plural(props.glossaryIssues, "string breaks", "strings break")} it)`
              : "Every translation follows the glossary"}
          </MenuItem>
        </DropdownMenu>
      </div>

      <Button
        variant="secondary"
        icon={<Save aria-hidden size={16} />}
        disabled={!props.dirty}
        onClick={props.onSave}
        title="Save all files (Ctrl+S)"
      >
        Save
      </Button>
      <Button
        variant="ghost"
        aria-label="Settings"
        icon={<Settings aria-hidden size={16} />}
        onClick={props.onSettings}
      />
      <ThemeToggle />
      <Button variant="ghost" onClick={props.onClose}>
        Close
      </Button>
    </div>
  );
}
