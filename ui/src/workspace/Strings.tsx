import type { Column } from "@vitavision/ui";
import {
  Button,
  Input,
  Kbd,
  SegmentedControl,
  Select,
  Skeleton,
  StatusDot,
  Table,
} from "@vitavision/ui";
import { BookA, CircleAlert, GitCompareArrows, Search, TriangleAlert, X } from "lucide-react";
import type { Ref } from "react";

import type { CatalogSummary, Show, UnitRow, UnitStatus } from "../api";
import { STATUS_LABEL, plural } from "../lib/format";

export type StatusTab = UnitStatus | "all";

/** Height of one row of the windowed list, in pixels. */
const ROW_HEIGHT = 36;

const DOT: Record<UnitStatus, "neutral" | "warning" | "normal"> = {
  "to-do": "neutral",
  check: "warning",
  done: "normal",
};

export function Strings(props: {
  catalog: CatalogSummary;
  /** The strings of every file together: adds a File column. */
  allFiles: boolean;
  tab: StatusTab;
  onTab: (tab: StatusTab) => void;
  search: string;
  onSearch: (search: string) => void;
  searchRef: Ref<HTMLInputElement>;
  show: Show;
  onShow: (show: Show) => void;
  /** Only strings using this glossary term. */
  term: string | null;
  onTerm: (term: string | null) => void;
  /** The glossary terms some string of the language uses. */
  terms: string[];
  rows: UnitRow[] | null;
  activeKey: string | null;
  /** `keyboard`: chosen with Enter or Space, so the editor should take focus. */
  onActivate: (row: UnitRow, keyboard: boolean) => void;
  /** Esc in the list: back to the file summary. */
  onDeselect: () => void;
}) {
  const { catalog, rows, searchRef } = props;
  const counts = catalog.counts;
  const tabs = [
    { value: "to-do", label: `To do ${counts.to_do.toLocaleString()}` },
    { value: "check", label: `Check ${counts.check.toLocaleString()}` },
    { value: "done", label: `Done ${counts.done.toLocaleString()}` },
    { value: "all", label: `All ${counts.total.toLocaleString()}` },
  ];
  const shared = counts.total - counts.only_here;
  const showOptions: { value: Show; label: string }[] = [
    { value: "everything", label: "Everything" },
    { value: "problems", label: `Problems (${counts.errors.toLocaleString()})` },
    { value: "warnings", label: `Warnings (${counts.warnings.toLocaleString()})` },
    {
      value: "inconsistent",
      label: `Translated differently in other files (${counts.inconsistent.toLocaleString()})`,
    },
    { value: "shared", label: `Also in other files (${shared.toLocaleString()})` },
    { value: "glossary", label: `Use a glossary term (${counts.glossary.toLocaleString()})` },
    {
      value: "glossary-broken",
      label: `Do not follow the glossary (${counts.glossary_broken.toLocaleString()})`,
    },
  ];
  const termOptions = [
    { value: "", label: "Any term" },
    ...props.terms.map((t) => ({ value: t, label: t })),
  ];
  const glossaryShown = props.show === "glossary" || props.show === "glossary-broken";
  const filtered = props.search.trim() !== "" || props.show !== "everything" || props.term !== null;

  return (
    <section aria-label="Strings" className="flex h-full min-h-0 flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b border-line px-3 py-2">
        <SegmentedControl
          aria-label="Show strings"
          value={props.tab}
          options={tabs}
          onValueChange={(v) => props.onTab(v as StatusTab)}
          size="sm"
        />
        <div className="relative min-w-48 flex-1">
          <Search
            aria-hidden
            size={14}
            className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-fg-subtle"
          />
          <Input
            ref={searchRef}
            aria-label="Search strings"
            placeholder="Search text, context or id"
            className="pl-7"
            value={props.search}
            onChange={(e) => props.onSearch(e.target.value)}
          />
        </div>
        <div className="w-44">
          <Select
            aria-label="Which strings"
            value={props.show}
            onValueChange={(v) => props.onShow((v || "everything") as Show)}
            options={showOptions}
          />
        </div>
        {glossaryShown && props.terms.length > 0 && (
          <div className="w-44">
            <Select
              aria-label="Glossary term"
              value={props.term ?? ""}
              onValueChange={(v) => props.onTerm(v === "" ? null : v)}
              options={termOptions}
            />
          </div>
        )}
        {!glossaryShown && props.term !== null && (
          <Button
            variant="secondary"
            size="sm"
            icon={<X aria-hidden size={14} />}
            aria-label={`Stop showing only strings with "${props.term}"`}
            onClick={() => props.onTerm(null)}
          >
            Term: {props.term}
          </Button>
        )}
      </div>

      <div
        className="min-h-0 flex-1"
        onKeyDown={(e) => {
          if (e.key === "Escape" && props.activeKey !== null) {
            e.preventDefault();
            props.onDeselect();
          }
        }}
      >
        {rows === null ? (
          <div className="space-y-2 p-3">
            <Skeleton className="h-8" />
            <Skeleton className="h-8" />
            <Skeleton className="h-8" />
          </div>
        ) : (
          <Table
            caption={`${catalog.name}: strings`}
            columns={columnsFor(catalog, props.allFiles)}
            rows={rows}
            rowKey={rowKey}
            className="h-full"
            windowed={{ rowHeight: ROW_HEIGHT }}
            // The arrow keys move the selection; Enter or Space also moves
            // the cursor into the editor.
            selectionFollowsFocus
            onRowClick={(row, _index, event) =>
              props.onActivate(row, "key" in event && (event.key === "Enter" || event.key === " "))
            }
            isRowActive={(row) => rowKey(row) === props.activeKey}
            empty={emptyMessage(props.tab, props.search, props.show, props.term, props.allFiles)}
          />
        )}
      </div>

      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-line px-3 py-1.5 text-xs text-fg-subtle">
        {filtered && rows !== null && (
          <span>{plural(rows.length, "string matches", "strings match")}</span>
        )}
        <span className="ml-auto flex flex-wrap items-center gap-x-3 gap-y-1">
          <span>
            <Kbd>↑</Kbd>
            <Kbd>↓</Kbd> move
          </span>
          <span>
            <Kbd>Enter</Kbd> edit
          </span>
          <span>
            <Kbd>Esc</Kbd> back
          </span>
          <span>
            <Kbd>Alt</Kbd>+<Kbd>↑</Kbd>
            <Kbd>↓</Kbd> previous / next
          </span>
          <span>
            <Kbd>Ctrl</Kbd>+<Kbd>F</Kbd> search
          </span>
        </span>
      </div>
    </section>
  );
}

export function rowKey(row: UnitRow): string {
  return `${row.path}\u0000${row.id}`;
}

function columnsFor(catalog: CatalogSummary, allFiles: boolean): Column<UnitRow>[] {
  return [
    {
      key: "status",
      header: <span className="sr-only">Status</span>,
      width: "2.25rem",
      cell: (r) => (
        <span className="flex pl-3">
          <StatusDot tone={DOT[r.status]}>
            <span className="sr-only">{STATUS_LABEL[r.status]}</span>
          </StatusDot>
        </span>
      ),
    },
    ...(allFiles
      ? [
          {
            key: "file",
            header: "File",
            width: "9rem",
            cell: (r: UnitRow) => <Text text={r.file} muted />,
          },
        ]
      : []),
    {
      key: "source",
      header: "Original",
      cell: (r) => <Text text={r.source} title={r.group ? `${r.group}: ${r.source}` : r.source} />,
    },
    ...(catalog.pivot_language
      ? [
          {
            key: "existing",
            header: `From ${catalog.pivot_language}`,
            cell: (r: UnitRow) => <Text text={r.existing ?? ""} muted />,
          },
        ]
      : []),
    {
      key: "target",
      header: catalog.language,
      cell: (r) => <Text text={r.target} placeholder="Not translated" />,
    },
    {
      key: "markers",
      header: <span className="sr-only">Remarks</span>,
      width: "5.5rem",
      cell: markers,
    },
  ];
}

function markers(r: UnitRow) {
  return (
    <span className="flex items-center gap-1.5">
      {r.error && (
        <span title="Problem: not written to the file until fixed" className="text-defect">
          <CircleAlert aria-hidden size={14} />
          <span className="sr-only">Problem</span>
        </span>
      )}
      {r.warning && (
        <span title="Warning" className="text-warn">
          <TriangleAlert aria-hidden size={14} />
          <span className="sr-only">Warning</span>
        </span>
      )}
      {r.terms > 0 && (
        <span
          title={
            r.glossary_broken
              ? "Does not follow the glossary"
              : `Uses ${r.terms === 1 ? "a glossary term" : `${r.terms} glossary terms`}`
          }
          className={r.glossary_broken ? "text-warn" : "text-fg-subtle"}
        >
          <BookA aria-hidden size={14} />
          <span className="sr-only">
            {r.glossary_broken ? "Does not follow the glossary" : "Uses a glossary term"}
          </span>
        </span>
      )}
      {r.inconsistent && (
        <span title="Translated differently in another file" className="text-fg-muted">
          <GitCompareArrows aria-hidden size={14} />
          <span className="sr-only">Translated differently in another file</span>
        </span>
      )}
    </span>
  );
}

function Text(props: { text: string; title?: string; muted?: boolean; placeholder?: string }) {
  if (props.text === "") {
    return <span className="italic text-fg-subtle">{props.placeholder ?? "—"}</span>;
  }
  return (
    <span
      className={props.muted ? "block truncate text-fg-muted" : "block truncate"}
      title={props.title ?? props.text}
    >
      {props.text}
    </span>
  );
}

function emptyMessage(
  tab: StatusTab,
  search: string,
  show: Show,
  term: string | null,
  allFiles: boolean,
): string {
  const here = allFiles ? "in any file" : "in this file";
  if (search.trim() !== "") return "No string matches the search.";
  if (term !== null) {
    return show === "glossary-broken"
      ? `Every translation ${here} follows "${term}".`
      : `No string ${here} uses "${term}".`;
  }
  switch (show) {
    case "problems":
      return `No problems ${here}.`;
    case "warnings":
      return `No warnings ${here}.`;
    case "inconsistent":
      return "Every string shared between files is translated the same way in each.";
    case "shared":
      return allFiles
        ? "No string occurs in more than one file."
        : "No string of this file occurs in another file.";
    case "glossary":
      return `No string ${here} uses a glossary term.`;
    case "glossary-broken":
      return `Every translation ${here} follows the glossary.`;
    case "everything":
      break;
  }
  switch (tab) {
    case "to-do":
      return `Nothing left to translate ${here}.`;
    case "check":
      return "Nothing waiting for a check.";
    case "done":
      return "No string is marked done yet.";
    case "all":
      return allFiles ? "The folder has no strings." : "This file has no strings.";
  }
}
