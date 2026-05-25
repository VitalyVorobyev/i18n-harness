// Tauri event listener helpers for M4.8 bulk translate.
//
// The Tauri 2 `UnlistenFn` is synchronous (`() => void`), not async.
// We wrap all three batch event subscriptions into a single cleanup
// function so callers only need to call one function to tear down.

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { BatchProgressPayload, BatchTerminalPayload } from "./types";

/**
 * Subscribe to all three batch-event channels for a given job.
 *
 * Returns a combined unlisten function: calling it unregisters all
 * three listeners in one step. The listeners are already established
 * before this promise resolves, so the caller can start the batch
 * command after `await listenBatchProgress(...)` without racing against
 * a fast first-unit emit.
 *
 * Prefer to `await listenBatchProgress` BEFORE calling
 * `translateBatchInProject` — the comment in `tauri.ts` documents why.
 */
export async function listenBatchProgress(
  jobId: string,
  onProgress: (p: BatchProgressPayload) => void,
  onTerminal: (p: BatchTerminalPayload, status: "completed" | "failed") => void,
): Promise<UnlistenFn> {
  const u1 = await listen<BatchProgressPayload>(
    `batch-progress-${jobId}`,
    (e) => onProgress(e.payload),
  );
  const u2 = await listen<BatchTerminalPayload>(
    `batch-completed-${jobId}`,
    (e) => onTerminal(e.payload, "completed"),
  );
  const u3 = await listen<BatchTerminalPayload>(`batch-failed-${jobId}`, (e) =>
    onTerminal(e.payload, "failed"),
  );

  return () => {
    u1();
    u2();
    u3();
  };
}
