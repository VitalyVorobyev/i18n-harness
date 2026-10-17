import type { ModelChoice, UnitStatus } from "../api";

export const STATUS_LABEL: Record<UnitStatus, string> = {
  "to-do": "To do",
  check: "Check",
  done: "Done",
};

export function plural(n: number, one: string, many: string): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
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
