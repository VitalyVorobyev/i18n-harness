import { Button, ProgressBar } from "@vitavision/ui";

export type RunState =
  | { kind: "idle" }
  | { kind: "running"; done: number; total: number; failed: number; cancelling: boolean }
  | { kind: "finished"; translated: number; failed: number; cancelled: boolean };

/** Progress of a translation run, along the bottom of the window. */
export function RunBar(props: { run: RunState; onCancel: () => void; onDismiss: () => void }) {
  const { run } = props;
  if (run.kind === "idle") return null;

  if (run.kind === "running") {
    const fraction = run.total === 0 ? 1 : run.done / run.total;
    return (
      <div
        role="status"
        className="flex items-center gap-3 border-t border-line bg-surface px-3 py-2 text-sm"
      >
        <span className="shrink-0 font-medium">
          Translating {run.done.toLocaleString()} of {run.total.toLocaleString()}
        </span>
        <ProgressBar className="flex-1" fraction={fraction} aria-label="Translation progress" />
        {run.failed > 0 && <span className="shrink-0 text-warn">{run.failed} failed</span>}
        <Button variant="secondary" size="sm" disabled={run.cancelling} onClick={props.onCancel}>
          {run.cancelling ? "Stopping…" : "Stop"}
        </Button>
      </div>
    );
  }

  return (
    <div
      role="status"
      className="flex items-center gap-3 border-t border-line bg-surface px-3 py-2 text-sm"
    >
      <span className="flex-1">
        {run.cancelled ? "Stopped. " : "Finished. "}
        {run.translated.toLocaleString()} translated
        {run.failed > 0 ? `, ${run.failed.toLocaleString()} failed` : ""}. Review them under{" "}
        <strong>Check</strong> and save when ready.
      </span>
      <Button variant="ghost" size="sm" onClick={props.onDismiss}>
        Dismiss
      </Button>
    </div>
  );
}
