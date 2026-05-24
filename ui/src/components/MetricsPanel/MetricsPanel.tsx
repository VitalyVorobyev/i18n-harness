import { useCallback, useMemo, useState } from "react";
import { cn } from "../../lib/cn";
import { loadMetrics, pickMetricsFile } from "../../lib/tauri";
import type { MetricEvent, MetricsResponse } from "../../lib/types";
import { severityOf } from "../../lib/types";

interface Props {
  flashError: (msg: string) => void;
  flashInfo: (msg: string) => void;
}

export function MetricsPanel({ flashError, flashInfo }: Props) {
  const [response, setResponse] = useState<MetricsResponse | null>(null);
  const [loading, setLoading] = useState(false);

  const open = useCallback(async () => {
    setLoading(true);
    try {
      const picked = await pickMetricsFile();
      if (!picked) return;
      const res = await loadMetrics(picked);
      setResponse(res);
      flashInfo(
        `Loaded ${res.events.length} event(s) from ${shortPath(res.path)}` +
          (res.error_count > 0
            ? ` (${res.error_count} unparseable line(s))`
            : ""),
      );
    } catch (e) {
      flashError(`Open metrics failed: ${formatError(e)}`);
    } finally {
      setLoading(false);
    }
  }, [flashInfo, flashError]);

  const reload = useCallback(async () => {
    if (!response) return;
    setLoading(true);
    try {
      const res = await loadMetrics(response.path);
      setResponse(res);
      flashInfo(`Reloaded ${res.events.length} event(s).`);
    } catch (e) {
      flashError(`Reload failed: ${formatError(e)}`);
    } finally {
      setLoading(false);
    }
  }, [response, flashInfo, flashError]);

  if (!response) {
    return <EmptyMetrics onOpen={open} loading={loading} />;
  }

  return (
    <Loaded
      response={response}
      loading={loading}
      onOpen={open}
      onReload={reload}
    />
  );
}

function EmptyMetrics({
  onOpen,
  loading,
}: {
  onOpen: () => void;
  loading: boolean;
}) {
  return (
    <section className="flex-1 flex items-center justify-center bg-bg-base p-12">
      <div className="w-full max-w-[520px] rounded-lg border border-border-subtle bg-bg-surface p-8">
        <div className="text-xs font-semibold uppercase tracking-loose text-accent mb-2">
          Metrics
        </div>
        <h1 className="m-0 mb-2 text-2xl font-semibold tracking-tight text-fg-primary">
          Open a metrics file
        </h1>
        <p className="m-0 mb-6 text-sm text-fg-secondary leading-[1.65]">
          The harness appends one JSONL line per gate finding to
          <CodeChip>.i18n-harness/metrics.jsonl</CodeChip>. Open one to see
          per-rule histograms and per-(backend, locale) breakdowns of what the
          gate flagged across runs.
        </p>
        <div className="flex gap-3">
          <button
            type="button"
            onClick={onOpen}
            disabled={loading}
            className="inline-flex items-center h-9 px-4 rounded-md text-md font-medium text-accent-fg bg-accent transition-colors duration-100 ease-out enabled:hover:bg-accent-hover disabled:opacity-60"
          >
            {loading ? "Loading…" : "Open metrics.jsonl…"}
          </button>
        </div>
      </div>
    </section>
  );
}

function Loaded({
  response,
  loading,
  onOpen,
  onReload,
}: {
  response: MetricsResponse;
  loading: boolean;
  onOpen: () => void;
  onReload: () => void;
}) {
  const events = response.events;
  const summary = useMemo(() => summarise(events), [events]);
  const byRule = useMemo(() => groupByRule(events), [events]);
  const byBackendLocale = useMemo(() => groupByBackendLocale(events), [events]);

  return (
    <section className="flex-1 flex flex-col overflow-hidden min-w-0 bg-bg-base">
      <header className="shrink-0 px-5 py-3 border-b border-border-subtle bg-bg-surface flex items-center justify-between gap-4">
        <div className="flex items-center gap-3 min-w-0">
          <span className="text-xs font-semibold uppercase tracking-loose text-fg-tertiary">
            Metrics
          </span>
          <span
            className="font-mono text-xs text-fg-secondary truncate max-w-[460px]"
            title={response.path}
          >
            {shortPath(response.path)}
          </span>
          <span className="text-fg-disabled">·</span>
          <span className="text-xs text-fg-tertiary tracking-loose whitespace-nowrap">
            {events.length} event{events.length === 1 ? "" : "s"}
          </span>
          {response.error_count > 0 && (
            <span
              className="text-xs text-severity-hard tracking-loose"
              title={`${response.error_count} unparseable line(s)`}
            >
              · {response.error_count} bad line(s)
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <ActionBtn
            onClick={onReload}
            disabled={loading}
            title="Re-read the file from disk"
          >
            Reload
          </ActionBtn>
          <ActionBtn
            onClick={onOpen}
            disabled={loading}
            title="Open another file"
          >
            Open…
          </ActionBtn>
        </div>
      </header>

      <div className="flex-1 overflow-y-auto p-5 flex flex-col gap-6">
        <SummaryCards summary={summary} />
        <RuleTable rows={byRule} totalEvents={events.length} />
        <BackendLocaleTable rows={byBackendLocale} />
        {events.length === 0 && (
          <div className="text-sm text-fg-tertiary italic">
            File parsed but no events found. (Did the gate run produce findings
            yet?)
          </div>
        )}
      </div>
    </section>
  );
}

function SummaryCards({ summary }: { summary: Summary }) {
  return (
    <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
      <Card label="Events" value={summary.totalEvents.toString()} />
      <Card label="Backends" value={summary.backends.join(", ") || "—"} />
      <Card label="Locales" value={summary.locales.join(", ") || "—"} />
      <Card
        label="Time range"
        value={
          summary.first && summary.last
            ? `${formatTs(summary.first)} → ${formatTs(summary.last)}`
            : "—"
        }
        mono
      />
    </div>
  );
}

function Card({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="rounded-md border border-border-subtle bg-bg-surface px-4 py-3">
      <div className="text-xs font-semibold uppercase tracking-loose text-fg-tertiary mb-1">
        {label}
      </div>
      <div
        className={cn(
          "text-sm text-fg-primary truncate",
          mono ? "font-mono" : "",
        )}
        title={value}
      >
        {value}
      </div>
    </div>
  );
}

function RuleTable({
  rows,
  totalEvents,
}: {
  rows: RuleRow[];
  totalEvents: number;
}) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <span className="text-xs font-semibold uppercase tracking-loose text-fg-tertiary">
          By rule
        </span>
        <span className="text-xs text-fg-tertiary tabular-nums">
          {rows.length} rule{rows.length === 1 ? "" : "s"}
        </span>
      </div>
      {rows.length === 0 ? (
        <Muted>No rule events recorded.</Muted>
      ) : (
        <div className="overflow-x-auto rounded-md border border-border-subtle">
          <table className="w-full border-collapse text-sm">
            <thead>
              <tr className="bg-bg-elevated text-fg-tertiary text-xs uppercase tracking-loose">
                <Th>Rule</Th>
                <Th className="w-[100px]">Severity</Th>
                <Th className="w-[100px] text-right">Count</Th>
                <Th className="w-[160px]">Share</Th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr
                  key={r.rule}
                  className="border-t border-border-subtle align-top"
                >
                  <Td>
                    <span className="font-mono text-fg-primary">{r.rule}</span>
                  </Td>
                  <Td>
                    <SeverityChip severity={r.severity} />
                  </Td>
                  <Td className="text-right font-mono text-fg-primary tabular-nums">
                    {r.count}
                  </Td>
                  <Td>
                    <Bar
                      ratio={totalEvents === 0 ? 0 : r.count / totalEvents}
                    />
                  </Td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function BackendLocaleTable({ rows }: { rows: BackendLocaleRow[] }) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <span className="text-xs font-semibold uppercase tracking-loose text-fg-tertiary">
          By backend × locale
        </span>
        <span className="text-xs text-fg-tertiary tabular-nums">
          {rows.length} pair{rows.length === 1 ? "" : "s"}
        </span>
      </div>
      {rows.length === 0 ? (
        <Muted>No backend/locale events recorded.</Muted>
      ) : (
        <div className="overflow-x-auto rounded-md border border-border-subtle">
          <table className="w-full border-collapse text-sm">
            <thead>
              <tr className="bg-bg-elevated text-fg-tertiary text-xs uppercase tracking-loose">
                <Th>Backend</Th>
                <Th>Locale</Th>
                <Th className="w-[80px] text-right">Events</Th>
                <Th className="w-[80px] text-right">Hard</Th>
                <Th className="w-[80px] text-right">Soft</Th>
                <Th className="w-[80px] text-right">Other</Th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr
                  key={`${r.backend}::${r.locale}`}
                  className="border-t border-border-subtle align-top"
                >
                  <Td className="font-mono text-fg-primary">{r.backend}</Td>
                  <Td className="font-mono text-fg-primary">{r.locale}</Td>
                  <Td className="text-right font-mono text-fg-primary tabular-nums">
                    {r.total}
                  </Td>
                  <Td className="text-right font-mono text-severity-hard tabular-nums">
                    {r.hard}
                  </Td>
                  <Td className="text-right font-mono text-severity-soft tabular-nums">
                    {r.soft}
                  </Td>
                  <Td className="text-right font-mono text-fg-tertiary tabular-nums">
                    {r.other}
                  </Td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function SeverityChip({ severity }: { severity: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center h-[18px] px-2 rounded-pill border text-xs uppercase tracking-loose",
        severity === "hard" &&
          "bg-severity-hard-bg border-severity-hard-border text-severity-hard",
        severity === "soft" &&
          "bg-severity-soft-bg border-state-proposed-border text-severity-soft",
        severity === "semantic" &&
          "bg-severity-info-bg border-border-default text-severity-info",
      )}
    >
      {severity}
    </span>
  );
}

function Bar({ ratio }: { ratio: number }) {
  const pct = Math.round(ratio * 100);
  return (
    <div
      className="relative h-2 rounded-pill bg-bg-elevated overflow-hidden"
      title={`${pct}%`}
    >
      <div
        className="h-full bg-accent"
        style={{ width: `${Math.max(2, pct)}%` }}
      />
    </div>
  );
}

function ActionBtn({
  children,
  onClick,
  disabled,
  title,
}: {
  children: React.ReactNode;
  onClick: () => void;
  disabled?: boolean;
  title?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={cn(
        "inline-flex items-center h-7 px-3 rounded-md border text-sm font-medium",
        "text-fg-secondary border-border-default bg-transparent",
        "transition-colors duration-100 ease-out",
        "enabled:hover:bg-bg-hover enabled:hover:text-fg-primary",
        "disabled:text-fg-disabled disabled:border-border-subtle disabled:cursor-not-allowed",
      )}
    >
      {children}
    </button>
  );
}

function Th({
  children,
  className,
}: {
  children?: React.ReactNode;
  className?: string;
}) {
  return (
    <th
      className={cn(
        "text-left font-semibold px-3 py-2 border-b border-border-subtle",
        className,
      )}
    >
      {children}
    </th>
  );
}

function Td({
  children,
  className,
}: {
  children?: React.ReactNode;
  className?: string;
}) {
  return <td className={cn("px-3 py-2 align-top", className)}>{children}</td>;
}

function Muted({ children }: { children: React.ReactNode }) {
  return <div className="text-sm text-fg-tertiary italic">{children}</div>;
}

function CodeChip({ children }: { children: React.ReactNode }) {
  return (
    <code className="px-1 py-px font-mono text-[0.9em] bg-bg-elevated border border-border-subtle rounded-sm text-fg-primary">
      {children}
    </code>
  );
}

interface Summary {
  totalEvents: number;
  backends: string[];
  locales: string[];
  first: string | null;
  last: string | null;
}

function summarise(events: MetricEvent[]): Summary {
  const backends = new Set<string>();
  const locales = new Set<string>();
  let first: string | null = null;
  let last: string | null = null;
  for (const e of events) {
    if (e.backend) backends.add(e.backend);
    if (e.locale) locales.add(e.locale);
    if (e.ts) {
      if (first === null || e.ts < first) first = e.ts;
      if (last === null || e.ts > last) last = e.ts;
    }
  }
  return {
    totalEvents: events.length,
    backends: Array.from(backends).sort(),
    locales: Array.from(locales).sort(),
    first,
    last,
  };
}

interface RuleRow {
  rule: string;
  severity: string;
  count: number;
}

function groupByRule(events: MetricEvent[]): RuleRow[] {
  const counts = new Map<string, number>();
  for (const e of events) {
    if (!e.rule) continue;
    counts.set(e.rule, (counts.get(e.rule) ?? 0) + 1);
  }
  return Array.from(counts.entries())
    .map(([rule, count]) => ({
      rule,
      severity: severityOf(rule),
      count,
    }))
    .sort((a, b) => b.count - a.count || a.rule.localeCompare(b.rule));
}

interface BackendLocaleRow {
  backend: string;
  locale: string;
  total: number;
  hard: number;
  soft: number;
  other: number;
}

function groupByBackendLocale(events: MetricEvent[]): BackendLocaleRow[] {
  const rows = new Map<string, BackendLocaleRow>();
  for (const e of events) {
    const key = `${e.backend}::${e.locale}`;
    let row = rows.get(key);
    if (!row) {
      row = {
        backend: e.backend || "—",
        locale: e.locale || "—",
        total: 0,
        hard: 0,
        soft: 0,
        other: 0,
      };
      rows.set(key, row);
    }
    row.total += 1;
    if (e.rule) {
      const sev = severityOf(e.rule);
      if (sev === "hard") row.hard += 1;
      else if (sev === "soft") row.soft += 1;
      else row.other += 1;
    } else {
      row.other += 1;
    }
  }
  return Array.from(rows.values()).sort(
    (a, b) =>
      b.total - a.total ||
      a.backend.localeCompare(b.backend) ||
      a.locale.localeCompare(b.locale),
  );
}

function shortPath(p: string): string {
  if (p.length <= 60) return p;
  const parts = p.split(/[\\/]/);
  if (parts.length <= 3) return p;
  return `…/${parts.slice(-3).join("/")}`;
}

function formatTs(ts: string): string {
  // Trim microseconds and Z for a denser display: 2026-05-24T09:04:47Z
  const match = ts.match(/^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2}:\d{2})/);
  if (!match) return ts;
  return `${match[1]} ${match[2]}`;
}

function formatError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  return String(e);
}
