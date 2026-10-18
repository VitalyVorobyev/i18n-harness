import type { BreakdownSegment } from "@vitavision/ui";
import { Badge, Breakdown, Button, cn } from "@vitavision/ui";
import { ChevronRight, Save } from "lucide-react";

import type { CatalogSummary, Show } from "../api";
import { plural } from "../lib/format";
import type { StatusTab } from "./Strings";

/**
 * What the right panel shows while no string is selected: where the file
 * is, how far its translation is, what the checks found, and how its
 * strings relate to the other files. Every count opens the strings behind it.
 */
export function FileSummary(props: {
  summary: CatalogSummary;
  /** The summary stands for every file of the folder. */
  allFiles: boolean;
  fileCount: number;
  root: string;
  /** Different strings over every file, once known (all-files view). */
  distinct: number | null;
  /** Strings to do here that other files translate the same way. */
  fillable: number;
  running: boolean;
  onTab: (tab: StatusTab) => void;
  onShow: (show: Show) => void;
  onFill: () => void;
  onAcross: () => void;
  onSave: () => void;
  onSetPivot: (() => void) | null;
}) {
  const { summary, allFiles } = props;
  const c = summary.counts;
  const segments: BreakdownSegment[] = [
    { value: c.done, tone: "normal", label: "done" },
    { value: c.check, tone: "warn", label: "to check" },
  ];
  const translated = c.total - c.to_do;
  const percent = c.total === 0 ? 100 : Math.floor((translated / c.total) * 100);
  const location = allFiles ? props.root : joinPath(props.root, summary.relative_path);

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      <div className="space-y-5 p-4">
        <header className="space-y-1">
          <div className="flex items-center justify-between gap-2">
            <h2 className="truncate text-base font-medium" title={summary.name}>
              {allFiles ? `All ${props.fileCount} files` : summary.name}
            </h2>
            {summary.dirty && <Badge tone="warning">Unsaved</Badge>}
          </div>
          <p
            className="select-text break-all font-mono text-xs text-fg-subtle"
            title="Location on disk"
          >
            {location}
          </p>
          <p className="flex flex-wrap items-center gap-x-1.5 text-sm text-fg-muted">
            <span>{summary.format_name}</span>
            <span aria-hidden>·</span>
            <span>{summary.language}</span>
            {summary.pivot_language && (
              <>
                <span aria-hidden>·</span>
                <span>from {summary.pivot_language}</span>
              </>
            )}
            {props.onSetPivot && (
              <Button variant="ghost" size="sm" className="ml-auto" onClick={props.onSetPivot}>
                Language of the existing text…
              </Button>
            )}
          </p>
        </header>

        <Group title="Translation">
          <div className="space-y-1.5 px-2 pb-1">
            <div className="flex items-baseline justify-between text-sm">
              <span className="text-fg-muted">
                {translated.toLocaleString()} of {plural(c.total, "string", "strings")} translated
              </span>
              <span className="font-mono tabular-nums">{percent}%</span>
            </div>
            <Breakdown segments={segments} total={c.total} remainderLabel="to do" />
          </div>
          <Stat label="Done" value={c.done} tone="normal" onClick={() => props.onTab("done")} />
          <Stat label="To check" value={c.check} tone="warn" onClick={() => props.onTab("check")} />
          <Stat label="To do" value={c.to_do} onClick={() => props.onTab("to-do")} />
          {allFiles && props.distinct !== null && (
            <Stat
              label="Different strings"
              value={props.distinct}
              hint="Counting a string once however many files have it"
            />
          )}
        </Group>

        <Group title="Checks">
          <Stat
            label="Problems"
            value={c.errors}
            tone="defect"
            hint="Not written to the file until fixed"
            onClick={() => props.onShow("problems")}
          />
          <Stat label="Warnings" value={c.warnings} onClick={() => props.onShow("warnings")} />
          <Stat
            label="Use a glossary term"
            value={c.glossary}
            onClick={() => props.onShow("glossary")}
          />
          <Stat
            label="Do not follow the glossary"
            value={c.glossary_broken}
            tone="warn"
            onClick={() => props.onShow("glossary-broken")}
          />
        </Group>

        {props.fileCount > 1 && (
          <Group title="Across files">
            <Stat label={allFiles ? "In one file only" : "Only in this file"} value={c.only_here} />
            <Stat
              label="Also in other files"
              value={c.total - c.only_here}
              onClick={() => props.onShow("shared")}
            />
            <Stat
              label="Translated differently"
              value={c.inconsistent}
              tone="warn"
              onClick={() => props.onShow("inconsistent")}
            />
            <div className="flex flex-wrap gap-2 px-2 pt-2">
              <Button
                variant="secondary"
                size="sm"
                disabled={props.fillable === 0 || props.running}
                onClick={props.onFill}
              >
                {props.fillable > 0
                  ? `Fill ${plural(props.fillable, "string", "strings")} from other files`
                  : "Nothing to fill from other files"}
              </Button>
              <Button variant="ghost" size="sm" onClick={props.onAcross}>
                Compare across files
              </Button>
            </div>
          </Group>
        )}

        {summary.dirty && (
          <Button
            variant="secondary"
            icon={<Save aria-hidden size={16} />}
            onClick={props.onSave}
            title="Save all files (Ctrl+S)"
          >
            Save changes
          </Button>
        )}

        <p className="text-xs text-fg-subtle">Select a string to edit its translation.</p>
      </div>
    </div>
  );
}

function Group(props: { title: string; children: React.ReactNode }) {
  return (
    <section aria-label={props.title} className="space-y-1">
      <h3 className="px-2 text-xs font-medium uppercase tracking-wide text-fg-subtle">
        {props.title}
      </h3>
      <div className="flex flex-col">{props.children}</div>
    </section>
  );
}

const TONE = {
  normal: "text-normal",
  warn: "text-warn",
  defect: "text-defect",
} as const;

/** A labelled count; with `onClick` and something to show, a button that shows it. */
function Stat(props: {
  label: string;
  value: number;
  tone?: keyof typeof TONE;
  hint?: string;
  onClick?: () => void;
}) {
  const active = props.onClick !== undefined && props.value > 0;
  const body = (
    <>
      <span className="min-w-0 flex-1 truncate">{props.label}</span>
      <span
        className={cn(
          "font-mono tabular-nums",
          props.value === 0 ? "text-fg-subtle" : props.tone && TONE[props.tone],
        )}
      >
        {props.value.toLocaleString()}
      </span>
      {/* Keeps the numbers of every row in one column. */}
      <ChevronRight
        aria-hidden
        size={14}
        className={cn("shrink-0 text-fg-subtle", !active && "invisible")}
      />
    </>
  );
  const className = "flex items-center gap-2 rounded-control px-2 py-1 text-left text-sm";
  if (!active) {
    return (
      <div className={className} title={props.hint}>
        {body}
      </div>
    );
  }
  return (
    <button
      type="button"
      className={cn(className, "hover:bg-raised")}
      title={props.hint ?? `Show these ${props.value.toLocaleString()}`}
      onClick={props.onClick}
    >
      {body}
    </button>
  );
}

function joinPath(root: string, relative: string): string {
  if (relative === "") return root;
  const sep = root.includes("\\") && !root.includes("/") ? "\\" : "/";
  return root.endsWith(sep) ? root + relative : `${root}${sep}${relative}`;
}
