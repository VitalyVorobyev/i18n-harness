import type { Column } from "@vitavision/ui";
import {
  Button,
  Input,
  SegmentedControl,
  Skeleton,
  StatusDot,
  Table,
  ToggleChip,
} from "@vitavision/ui";
import { ChevronLeft, ChevronRight, Search } from "lucide-react";

import type { CatalogSummary, UnitPage, UnitStatus, UnitView } from "../api";
import { STATUS_LABEL } from "../lib/format";

export type StatusTab = UnitStatus | "all";
export const PAGE_SIZE = 100;

const DOT: Record<UnitStatus, "neutral" | "warning" | "normal"> = {
  "to-do": "neutral",
  check: "warning",
  done: "normal",
};

export function Strings(props: {
  catalog: CatalogSummary;
  tab: StatusTab;
  onTab: (tab: StatusTab) => void;
  search: string;
  onSearch: (search: string) => void;
  errorsOnly: boolean;
  onErrorsOnly: (on: boolean) => void;
  page: UnitPage | null;
  offset: number;
  onOffset: (offset: number) => void;
  activeId: string | null;
  onActivate: (unit: UnitView) => void;
}) {
  const { catalog, page } = props;
  const counts = catalog.counts;
  const tabs = [
    { value: "to-do", label: `To do ${counts.to_do.toLocaleString()}` },
    { value: "check", label: `Check ${counts.check.toLocaleString()}` },
    { value: "done", label: `Done ${counts.done.toLocaleString()}` },
    { value: "all", label: `All ${counts.total.toLocaleString()}` },
  ];

  const columns = columnsFor(catalog);

  const from = page && page.matching > 0 ? props.offset + 1 : 0;
  const to = page ? Math.min(props.offset + PAGE_SIZE, page.matching) : 0;

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
            aria-label="Search strings"
            placeholder="Search text, context or id"
            className="pl-7"
            value={props.search}
            onChange={(e) => props.onSearch(e.target.value)}
          />
        </div>
        <ToggleChip checked={props.errorsOnly} onCheckedChange={props.onErrorsOnly} size="sm">
          Problems only{counts.errors > 0 ? ` (${counts.errors})` : ""}
        </ToggleChip>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {page === null ? (
          <div className="space-y-2 p-3">
            <Skeleton className="h-8" />
            <Skeleton className="h-8" />
            <Skeleton className="h-8" />
          </div>
        ) : (
          <Table
            caption={`${catalog.name}: strings`}
            columns={columns}
            rows={page.units}
            rowKey={(u) => u.id}
            onRowClick={(u) => props.onActivate(u)}
            isRowActive={(u) => u.id === props.activeId}
            empty={emptyMessage(props.tab, props.search, props.errorsOnly)}
          />
        )}
      </div>

      {page !== null && page.matching > PAGE_SIZE && (
        <div className="flex items-center justify-end gap-2 border-t border-line px-3 py-1.5 text-sm text-fg-muted">
          <span>
            {from.toLocaleString()}–{to.toLocaleString()} of {page.matching.toLocaleString()}
          </span>
          <Button
            variant="ghost"
            size="sm"
            aria-label="Previous page"
            icon={<ChevronLeft aria-hidden size={16} />}
            disabled={props.offset === 0}
            onClick={() => props.onOffset(Math.max(0, props.offset - PAGE_SIZE))}
          />
          <Button
            variant="ghost"
            size="sm"
            aria-label="Next page"
            icon={<ChevronRight aria-hidden size={16} />}
            disabled={to >= page.matching}
            onClick={() => props.onOffset(props.offset + PAGE_SIZE)}
          />
        </div>
      )}
    </section>
  );
}

function columnsFor(catalog: CatalogSummary): Column<UnitView>[] {
  return [
    {
      key: "status",
      header: <span className="sr-only">Status</span>,
      width: "2rem",
      cell: statusCell,
    },
    {
      key: "source",
      header: "Original",
      cell: sourceCell,
    },
    ...(catalog.pivot_language
      ? [{ key: "existing", header: `From ${catalog.pivot_language}`, cell: existingCell }]
      : []),
    {
      key: "target",
      header: catalog.language,
      cell: targetCell,
    },
  ];
}

function statusCell(u: UnitView) {
  return (
    <StatusDot tone={DOT[u.status]}>
      <span className="sr-only">{STATUS_LABEL[u.status]}</span>
    </StatusDot>
  );
}

function sourceCell(u: UnitView) {
  return <Cell text={u.source} context={u.context.group} />;
}

function existingCell(u: UnitView) {
  return <Cell text={u.existing?.[0] ?? ""} muted />;
}

function targetCell(u: UnitView) {
  return (
    <span className="flex items-start gap-1.5">
      <Cell text={u.target[0] ?? ""} placeholder="Not translated" />
      {u.issues.some((i) => i.severity === "error") && (
        <span className="shrink-0 text-xs font-medium text-defect">Problem</span>
      )}
    </span>
  );
}

function Cell(props: {
  text: string;
  context?: string | undefined;
  muted?: boolean;
  placeholder?: string;
}) {
  return (
    <span className="block min-w-0">
      {props.text === "" ? (
        <span className="italic text-fg-subtle">{props.placeholder ?? "—"}</span>
      ) : (
        <span className={props.muted ? "line-clamp-2 text-fg-muted" : "line-clamp-2"}>
          {props.text}
        </span>
      )}
      {props.context && (
        <span className="block truncate text-xs text-fg-subtle">{props.context}</span>
      )}
    </span>
  );
}

function emptyMessage(tab: StatusTab, search: string, errorsOnly: boolean): string {
  if (search.trim() !== "") return "No string matches the search.";
  if (errorsOnly) return "No problems in this file.";
  switch (tab) {
    case "to-do":
      return "Nothing left to translate in this file.";
    case "check":
      return "Nothing waiting for a check.";
    case "done":
      return "No string is marked done yet.";
    case "all":
      return "This file has no strings.";
  }
}
