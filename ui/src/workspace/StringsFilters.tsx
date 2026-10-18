import { Button, Input, Kbd, Select, Tabs, cn } from "@vitavision/ui";
import type { LucideIcon } from "lucide-react";
import {
  BookA,
  CircleAlert,
  Files,
  GitCompareArrows,
  Search,
  TriangleAlert,
  X,
} from "lucide-react";
import type { Ref } from "react";

import type { Show, StatusCounts } from "../api";
import { plural } from "../lib/format";
import type { StatusTab } from "./Strings";

const MAC = typeof navigator !== "undefined" && /Mac|iP(hone|ad)/.test(navigator.platform);

interface ChipSpec {
  show: Show;
  label: string;
  /** What the chip lists, in a sentence. */
  title: string;
  icon: LucideIcon;
  /** The icon's colour: the same as the row marker it stands for. */
  tone: string;
  count: number;
}

/**
 * The bar above the string list: status tabs and search on the first row;
 * on the second, one chip per kind of string worth singling out, each with
 * its count and the icon that marks such rows in the list.
 */
export function StringsFilters({
  searchRef,
  ...props
}: {
  counts: StatusCounts;
  /** The folder has several files: offer the cross-file chips. */
  multiFile: boolean;
  tab: StatusTab;
  onTab: (tab: StatusTab) => void;
  search: string;
  onSearch: (search: string) => void;
  searchRef: Ref<HTMLInputElement>;
  show: Show;
  onShow: (show: Show) => void;
  term: string | null;
  onTerm: (term: string | null) => void;
  /** The glossary terms some string of the language uses. */
  terms: string[];
  /** Rows listed, when known. */
  matching: number | null;
  /** Back to every string of the status tab. */
  onClear: () => void;
}) {
  const c = props.counts;
  const attention: ChipSpec[] = [
    {
      show: "problems",
      label: "Problems",
      title: "Translations that would break the application; they are not written until fixed",
      icon: CircleAlert,
      tone: "text-defect",
      count: c.errors,
    },
    {
      show: "warnings",
      label: "Warnings",
      title: "Translations worth a second look, for example much longer than the original",
      icon: TriangleAlert,
      tone: "text-warn",
      count: c.warnings,
    },
    {
      show: "glossary-broken",
      label: "Glossary breaks",
      title: "Translations that do not follow a glossary term of their original",
      icon: BookA,
      tone: "text-warn",
      count: c.glossary_broken,
    },
    ...(props.multiFile
      ? [
          {
            show: "inconsistent" as const,
            label: "Differs",
            title: "Translated differently in another file",
            icon: GitCompareArrows,
            tone: "text-fg-muted",
            count: c.inconsistent,
          },
        ]
      : []),
  ];
  const scope: ChipSpec[] = [
    {
      show: "glossary",
      label: "Glossary",
      title: "Strings whose original contains a glossary term",
      icon: BookA,
      tone: "text-fg-subtle",
      count: c.glossary,
    },
    ...(props.multiFile
      ? [
          {
            show: "shared" as const,
            label: "Shared",
            title: "Strings that also occur in another file",
            icon: Files,
            tone: "text-fg-subtle",
            count: c.total - c.only_here,
          },
        ]
      : []),
  ];
  const glossaryShown = props.show === "glossary" || props.show === "glossary-broken";
  const filtered = props.search.trim() !== "" || props.show !== "everything" || props.term !== null;
  // A chip with nothing to show is left out, unless it is the filter on.
  const offered = (specs: ChipSpec[]) =>
    specs.filter((spec) => spec.count > 0 || props.show === spec.show);
  const flagged = offered(attention);
  const scoped = offered(scope);
  const chip = (spec: ChipSpec) => (
    <Chip
      key={spec.show}
      spec={spec}
      pressed={props.show === spec.show}
      onClick={() => props.onShow(props.show === spec.show ? "everything" : spec.show)}
    />
  );

  return (
    <div className="space-y-2 border-b border-line px-3 py-2">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <Tabs<StatusTab>
          label="Status"
          active={props.tab}
          onSelect={props.onTab}
          items={[
            { id: "to-do", label: "To do", count: c.to_do },
            { id: "check", label: "Check", count: c.check },
            { id: "done", label: "Done", count: c.done },
            { id: "all", label: "All", count: c.total },
          ]}
        />
        <div className="relative min-w-52 flex-1">
          <Search
            aria-hidden
            size={14}
            className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-subtle"
          />
          <Input
            ref={searchRef}
            aria-label="Search strings"
            placeholder="Search text, context or id"
            className="pl-8 pr-16"
            value={props.search}
            onChange={(e) => props.onSearch(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape" && props.search !== "") {
                e.preventDefault();
                props.onSearch("");
              }
            }}
          />
          <span className="absolute right-2 top-1/2 flex -translate-y-1/2 items-center">
            {props.search !== "" ? (
              <button
                type="button"
                aria-label="Clear the search"
                className="rounded-control p-0.5 text-fg-subtle hover:bg-raised hover:text-fg"
                onClick={() => props.onSearch("")}
              >
                <X aria-hidden size={14} />
              </button>
            ) : (
              <span aria-hidden className="pointer-events-none text-fg-subtle">
                <Kbd>{MAC ? "⌘" : "Ctrl"}</Kbd>
                <Kbd>F</Kbd>
              </span>
            )}
          </span>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-1.5" role="group" aria-label="Show only">
        {flagged.length > 0 ? (
          flagged.map(chip)
        ) : (
          <span
            className="px-1 text-xs text-fg-subtle"
            title="No problems, warnings or glossary breaks"
          >
            Nothing flagged
          </span>
        )}
        {scoped.length > 0 && <span aria-hidden className="mx-1 h-4 w-px bg-line" />}
        {scoped.map(chip)}
        {glossaryShown && props.terms.length > 0 && (
          <div className="w-44">
            <Select
              aria-label="Glossary term"
              value={props.term ?? ""}
              onValueChange={(v) => props.onTerm(v === "" ? null : v)}
              options={[
                { value: "", label: "Any term" },
                ...props.terms.map((t) => ({ value: t, label: t })),
              ]}
            />
          </div>
        )}
        {!glossaryShown && props.term !== null && (
          <Chip
            spec={{
              show: "glossary",
              label: `“${props.term}”`,
              title: `Strings whose original contains "${props.term}"`,
              icon: BookA,
              tone: "text-fg-subtle",
              count: props.matching ?? 0,
            }}
            pressed
            removable
            onClick={() => props.onTerm(null)}
          />
        )}
        {filtered && (
          <span className="ml-auto flex items-center gap-2 text-xs text-fg-subtle">
            {props.matching !== null && plural(props.matching, "string", "strings")} shown
            <Button variant="ghost" size="sm" onClick={props.onClear}>
              Clear filters
            </Button>
          </span>
        )}
      </div>
    </div>
  );
}

/** A filter that is on or off; several in a row act as one choice. */
function Chip(props: {
  spec: ChipSpec;
  pressed: boolean;
  /** Clicking removes the filter: shows a cross. */
  removable?: boolean;
  onClick: () => void;
}) {
  const { spec } = props;
  const Icon = spec.icon;
  return (
    <button
      type="button"
      aria-pressed={props.removable ? undefined : props.pressed}
      aria-label={props.removable ? `Remove the filter ${spec.label}` : undefined}
      title={props.removable ? `${spec.title}. Click to show every string.` : spec.title}
      onClick={props.onClick}
      className={cn(
        "inline-flex h-7 items-center gap-1.5 rounded-full border px-2.5 text-xs transition-colors",
        "focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-signal",
        props.pressed
          ? "border-signal bg-signal/20 font-medium text-fg"
          : "border-line text-fg-muted hover:border-line-strong hover:bg-raised hover:text-fg",
      )}
    >
      <Icon aria-hidden size={13} className={cn("shrink-0", spec.tone)} />
      <span>{spec.label}</span>
      {!props.removable && (
        <span className={cn("font-mono tabular-nums", !props.pressed && "text-fg-subtle")}>
          {spec.count.toLocaleString()}
        </span>
      )}
      {props.removable && <X aria-hidden size={12} className="shrink-0" />}
    </button>
  );
}
