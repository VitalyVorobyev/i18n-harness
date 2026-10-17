import type { ModelChoice, StatusCounts, UnitStatus } from "../api";

export const STATUS_LABEL: Record<UnitStatus, string> = {
  "to-do": "To do",
  check: "Check",
  done: "Done",
};

export function plural(n: number, one: string, many: string): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

/** "312 to do · 20 to check" — only the parts that are not zero. */
export function countsLine(counts: StatusCounts): string {
  const parts: string[] = [];
  if (counts.to_do > 0) parts.push(`${counts.to_do.toLocaleString()} to do`);
  if (counts.check > 0) parts.push(`${counts.check.toLocaleString()} to check`);
  if (counts.errors > 0) parts.push(plural(counts.errors, "problem", "problems"));
  if (parts.length === 0) return counts.total > 0 ? "All done" : "No strings";
  return parts.join(" · ");
}

export function doneFraction(counts: StatusCounts): number {
  return counts.total === 0 ? 1 : counts.done / counts.total;
}

export function formatBytes(bytes: number | null): string | null {
  if (bytes === null) return null;
  const gb = bytes / 1_000_000_000;
  return gb >= 1 ? `${gb.toFixed(1)} GB` : `${Math.round(bytes / 1_000_000)} MB`;
}

/** "EuroLLM-9B-Instruct · 9.2B · 9.7 GB" style label for a model. */
export function modelLabel(model: ModelChoice): string {
  const name = model.id.split("/").at(-1) ?? model.id;
  return name;
}

export function modelNote(model: ModelChoice): string {
  return [
    model.parameter_size,
    formatBytes(model.size_bytes),
    model.prompt_style === "completion" ? "translation model" : null,
  ]
    .filter(Boolean)
    .join(" · ");
}
