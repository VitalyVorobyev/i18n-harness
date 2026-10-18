import type { Column } from "@vitavision/ui";
import {
  Badge,
  Button,
  Empty,
  Field,
  Input,
  SegmentedControl,
  Skeleton,
  Table,
  Textarea,
} from "@vitavision/ui";
import { ArrowUpRight, Copy, Search } from "lucide-react";
import { useMemo, useState } from "react";

import type { SharedString, SiblingView } from "../api";
import { STATUS_LABEL, plural } from "../lib/format";

/** Which shared strings to list. */
export type SharedKind = "differ" | "missing" | "existing" | "all";

/** Identifies a shared string across reloads. */
export function sharedKey(s: SharedString): string {
  return `${s.language}\u0000${s.id}\u0000${s.source}`;
}

const isKind = (s: SharedString, kind: SharedKind): boolean => {
  switch (kind) {
    case "differ":
      return s.translations > 1;
    case "missing":
      return s.translations === 1 && s.missing > 0;
    case "existing":
      return s.existing_differs;
    case "all":
      return true;
  }
};

/** The kind to open with: the first one that has something to look at. */
export function firstKind(shared: SharedString[]): SharedKind {
  for (const kind of ["differ", "missing", "existing"] as const) {
    if (shared.some((s) => isKind(s, kind))) return kind;
  }
  return "all";
}

/**
 * Every string that occurs in more than one file of its language (the same
 * id: Qt context and text, or the same key), with how the files translate
 * it. Strings the files translate differently, and strings some files have
 * still to do, are listed first.
 */
export function Across(props: {
  shared: SharedString[] | null;
  /** Some file holds its text in another language (a pivot): the name of it. */
  pivotLanguage: string | null;
  kind: SharedKind;
  onKind: (kind: SharedKind) => void;
  activeKey: string | null;
  onActivate: (s: SharedString, keyboard: boolean) => void;
  fillable: number;
  running: boolean;
  onFill: () => void;
}) {
  const [search, setSearch] = useState("");
  const shared = props.shared;
  const count = (kind: SharedKind) => shared?.filter((s) => isKind(s, kind)).length ?? 0;
  const rows = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return (shared ?? []).filter(
      (s) =>
        isKind(s, props.kind) &&
        (needle === "" ||
          [s.source, s.id, s.group ?? "", ...s.members.flatMap((m) => m.target)].some((t) =>
            t.toLowerCase().includes(needle),
          )),
    );
  }, [shared, props.kind, search]);

  const kinds: { value: SharedKind; label: string }[] = [
    { value: "differ", label: `Translated differently ${count("differ").toLocaleString()}` },
    { value: "missing", label: `Missing in some files ${count("missing").toLocaleString()}` },
    ...(props.pivotLanguage
      ? [
          {
            value: "existing" as const,
            label: `${props.pivotLanguage} differs ${count("existing").toLocaleString()}`,
          },
        ]
      : []),
    { value: "all", label: `All ${count("all").toLocaleString()}` },
  ];

  return (
    <section aria-label="Across files" className="flex h-full min-h-0 flex-col">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-line px-3 py-2">
        <p className="text-sm text-fg-muted">
          {shared === null
            ? "Looking for strings that occur in several files…"
            : `${plural(shared.length, "string occurs", "strings occur")} in more than one file. A string is the same when its id matches: the Qt context and text, or the key.`}
        </p>
        <Button
          variant="secondary"
          size="sm"
          className="ml-auto"
          icon={<Copy aria-hidden size={14} />}
          disabled={props.fillable === 0 || props.running}
          onClick={props.onFill}
          title="Give strings to do the translation the other files agree on"
        >
          {props.fillable > 0
            ? `Fill ${plural(props.fillable, "string", "strings")} from other files`
            : "Nothing to fill from other files"}
        </Button>
      </div>
      <div className="flex flex-wrap items-center gap-2 border-b border-line px-3 py-2">
        <SegmentedControl
          aria-label="Which shared strings"
          size="sm"
          value={props.kind}
          options={kinds}
          onValueChange={(v) => props.onKind(v as SharedKind)}
        />
        <div className="relative min-w-48 flex-1">
          <Search
            aria-hidden
            size={14}
            className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-fg-subtle"
          />
          <Input
            aria-label="Search shared strings"
            placeholder="Search text, context or id"
            className="pl-7"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
      </div>
      <div className="min-h-0 flex-1">
        {shared === null ? (
          <div className="space-y-2 p-3">
            <Skeleton className="h-8" />
            <Skeleton className="h-8" />
          </div>
        ) : (
          <Table
            caption="Strings in several files"
            columns={columns(props.pivotLanguage)}
            rows={rows}
            rowKey={sharedKey}
            className="h-full"
            windowed={{ rowHeight: 36 }}
            selectionFollowsFocus
            onRowClick={(row, _index, event) =>
              props.onActivate(row, "key" in event && (event.key === "Enter" || event.key === " "))
            }
            isRowActive={(row) => sharedKey(row) === props.activeKey}
            empty={emptyMessage(props.kind, search)}
          />
        )}
      </div>
    </section>
  );
}

function columns(pivotLanguage: string | null): Column<SharedString>[] {
  return [
    {
      key: "source",
      header: <span className="pl-3">Original</span>,
      cell: (s) => (
        <span
          className="block truncate pl-3"
          title={s.group ? `${s.group}: ${s.source}` : s.source}
        >
          {s.source}
        </span>
      ),
    },
    {
      key: "files",
      header: "Files",
      width: "5rem",
      numeric: true,
      cell: (s) => <span title={s.members.map((m) => m.file).join("\n")}>{s.members.length}</span>,
    },
    {
      key: "state",
      header: "Translations",
      width: "13rem",
      cell: (s) => (
        <span className="flex items-center gap-1.5">
          {s.translations > 1 ? (
            <Badge tone="warning">{s.translations} versions</Badge>
          ) : s.translations === 1 ? (
            <Badge tone="normal">Same</Badge>
          ) : (
            <Badge tone="neutral">Not translated</Badge>
          )}
          {s.missing > 0 && s.translations > 0 && <Badge tone="neutral">{s.missing} to do</Badge>}
          {pivotLanguage && s.existing_differs && (
            <span className="text-xs text-fg-muted">{pivotLanguage} differs</span>
          )}
        </span>
      ),
    },
  ];
}

function emptyMessage(kind: SharedKind, search: string): string {
  if (search.trim() !== "") return "No shared string matches the search.";
  switch (kind) {
    case "differ":
      return "Every string shared between files is translated the same way in each.";
    case "missing":
      return "No shared string is translated in one file and missing in another.";
    case "existing":
      return "The existing text of every shared string is the same in each file.";
    case "all":
      return "No string occurs in more than one file.";
  }
}

/** The files of a shared string grouped by their translation, most used first. */
function versions(s: SharedString): { text: string[]; members: SiblingView[] }[] {
  const out: { text: string[]; members: SiblingView[] }[] = [];
  for (const m of s.members) {
    if (m.target.every((t) => t === "")) continue;
    const same = out.find((v) => v.text.join("\u0000") === m.target.join("\u0000"));
    if (same) same.members.push(m);
    else out.push({ text: m.target, members: [m] });
  }
  return out.sort((a, b) => b.members.length - a.members.length);
}

/**
 * One shared string: each version of its translation with the files that
 * use it, and the way to make every file use one of them (or a new text).
 */
export function SharedDetail(props: {
  shared: SharedString | null;
  pivotLanguage: string | null;
  busy: boolean;
  /** Copy the translation of `member` to every other file. */
  onUse: (member: SiblingView) => void;
  /** Give every file this text, marked done. */
  onWrite: (text: string) => void;
  /** Open the string in that file's list. */
  onOpen: (member: SiblingView) => void;
}) {
  const s = props.shared;
  const [draft, setDraft] = useState("");
  if (s === null) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <Empty>
          Select a string to compare its translations and choose the one every file should use.
        </Empty>
      </div>
    );
  }
  const found = versions(s);
  const missing = s.members.filter((m) => m.target.every((t) => t === ""));
  const plural_ = (s.members[0]?.target.length ?? 1) > 1;
  const existing = props.pivotLanguage
    ? groupBy(
        s.members.filter((m) => m.existing !== null),
        (m) => (m.existing ?? []).join(" · "),
      )
    : [];

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      <div className="space-y-5 p-4">
        <section aria-label="Original" className="space-y-1">
          <span className="block truncate text-xs text-fg-subtle">{s.group ?? "No context"}</span>
          <p className="whitespace-pre-wrap break-words text-base">{s.source}</p>
          <p className="break-all font-mono text-xs text-fg-subtle" title="Id">
            {s.id}
          </p>
        </section>

        <section aria-label="Translations" className="space-y-2">
          <h3 className="text-xs font-medium uppercase tracking-wide text-fg-subtle">
            {found.length > 1
              ? `${found.length} translations in ${s.members.length} files`
              : `${s.language} in ${s.members.length} files`}
          </h3>
          {found.map((v) => (
            <div
              key={v.text.join("\u0000")}
              className="space-y-2 rounded-panel border border-line p-3"
            >
              <p className="whitespace-pre-wrap break-words">{v.text.join(" · ")}</p>
              <MemberList members={v.members} onOpen={props.onOpen} />
              {(found.length > 1 || missing.length > 0) && (
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={props.busy}
                  onClick={() => props.onUse(v.members[0]!)}
                >
                  Use this in every file
                </Button>
              )}
            </div>
          ))}
          {missing.length > 0 && (
            <div className="space-y-2 rounded-panel border border-dashed border-line p-3">
              <p className="italic text-fg-subtle">Not translated</p>
              <MemberList members={missing} onOpen={props.onOpen} />
            </div>
          )}
        </section>

        {!plural_ && (
          <section aria-label="New translation" className="space-y-2">
            <Field
              label="Or one new translation for every file"
              description="Every file gets this text, marked done."
            >
              <Textarea
                className="text-base"
                minRows={1}
                maxRows={6}
                value={draft}
                disabled={props.busy}
                onChange={(e) => setDraft(e.target.value)}
              />
            </Field>
            <Button
              variant="secondary"
              size="sm"
              disabled={props.busy || draft.trim() === ""}
              onClick={() => props.onWrite(draft)}
            >
              Use in every file
            </Button>
          </section>
        )}

        {existing.length > 0 && (
          <section aria-label={`In ${props.pivotLanguage}`} className="space-y-2">
            <h3 className="text-xs font-medium uppercase tracking-wide text-fg-subtle">
              {props.pivotLanguage}
              {existing.length > 1 ? `: ${existing.length} versions` : ""}
            </h3>
            {existing.map(([text, members]) => (
              <div key={text} className="space-y-1 rounded-panel border border-line p-3">
                <p className="whitespace-pre-wrap break-words text-fg-muted">{text}</p>
                <MemberList members={members} onOpen={props.onOpen} />
              </div>
            ))}
          </section>
        )}
      </div>
    </div>
  );
}

function MemberList(props: { members: SiblingView[]; onOpen: (m: SiblingView) => void }) {
  return (
    <ul className="flex flex-col gap-0.5">
      {props.members.map((m) => (
        <li key={m.path} className="flex items-center gap-2 text-sm">
          <button
            type="button"
            className="flex min-w-0 flex-1 items-center gap-1 truncate text-left text-fg-muted hover:text-fg hover:underline"
            title={`Open in ${m.file}`}
            onClick={() => props.onOpen(m)}
          >
            <span className="truncate">{m.file}</span>
            <ArrowUpRight aria-hidden size={12} className="shrink-0" />
          </button>
          <Badge
            tone={m.status === "done" ? "normal" : m.status === "check" ? "warning" : "neutral"}
          >
            {STATUS_LABEL[m.status]}
          </Badge>
        </li>
      ))}
    </ul>
  );
}

function groupBy<T>(items: T[], key: (item: T) => string): [string, T[]][] {
  const out = new Map<string, T[]>();
  for (const item of items) {
    const k = key(item);
    out.set(k, [...(out.get(k) ?? []), item]);
  }
  return [...out];
}
