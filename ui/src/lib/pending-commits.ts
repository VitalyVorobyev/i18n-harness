// Pending-commit registry — the contract a textarea must honour so that
// Save All can flush every uncommitted draft before persisting catalogs.
//
// Why this exists. Every translation editor (MatrixCell, FocusView,
// UntranslatedDraftEditor) commits its draft on `onBlur`. When the user
// presses Cmd-S without first leaving the textarea, the keystroke is captured
// at the window level, the browser does not synthesize a blur, so the draft
// stays in component-local React state and never reaches the IPC layer.
// Save All would then write the previous (stale) Rust-side state to disk.
//
// Two failure modes the registry plugs:
//   - H1: typed but never blurred — `commitNow` runs the same code path
//     `onBlur` would have run.
//   - H2: blurred, IPC is in flight, Save All races ahead — every IPC
//     promise launched by a commit is tracked and awaited by `flushAll`.
//
// The contract for a textarea:
//   1. On mount, call `register(commitNow)`. Store the returned `unregister`
//      and call it on unmount.
//   2. `commitNow` must read the latest draft (via a ref, not closed-over
//      state), compare against the last-committed value, and if changed,
//      run the same `onCommit(edit)` path that `onBlur` runs.
//   3. The component does NOT need to track IPC promises itself; the IPC
//      promise that `onCommit` returns is tracked by App.tsx's wrapper.
//
// The registry is intentionally a context, not a singleton: multiple App
// instances (tests) must not share state, and there is exactly one App
// instance at runtime.

import { createContext, useContext } from "react";

/// A textarea's "commit now" callback. Idempotent — calling it when nothing
/// has changed must be a no-op. May return a Promise if commit performs IPC;
/// `flushAll` awaits it.
export type CommitNowFn = () => Promise<void> | void;

export interface CommitRegistry {
  /** Register a commit-now callback. Returns an unregister function — call
   *  it on unmount or when the textarea identity (catalog,unit,form) changes. */
  register: (commit: CommitNowFn) => () => void;
  /** Track an IPC promise launched by an editor commit. App.tsx wraps
   *  `onEditUnitFor` to call this; editors do not call it directly. */
  trackInFlight: (promise: Promise<unknown>) => void;
  /** Run every registered commit-now, await every tracked IPC promise. The
   *  Save All button calls this before invoking `saveAllDirty`. */
  flushAll: () => Promise<void>;
}

// Null-object registry for when no provider is mounted (tests, storybook
// fixtures). Every method is a no-op so component-level rendering still works.
// trackInFlight still attaches a no-op rejection handler so callers can
// safely pass it a rejecting promise without leaking an unhandled rejection.
const noopRegistry: CommitRegistry = {
  register: () => () => {},
  trackInFlight: (promise) => {
    promise.catch(() => {});
  },
  flushAll: async () => {},
};

export const PendingCommitsContext =
  createContext<CommitRegistry>(noopRegistry);

export function usePendingCommits(): CommitRegistry {
  return useContext(PendingCommitsContext);
}
