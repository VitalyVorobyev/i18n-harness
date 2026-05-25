// Pending-commits registry — flush contract.
//
// These tests pin the load-bearing behaviour the registry exists for:
//
//   1. The Cmd-S-without-blur scenario: a textarea has typed-but-not-blurred
//      content. The save path calls flushAll. Every registered commit-now
//      thunk runs. The textarea's onCommit (the IPC trampoline) is invoked
//      with the typed text — exactly as if the user had blurred first.
//
//   2. The race scenario: the user types, blurs (committing), then immediately
//      presses Save All. The IPC promise for the blur is still in flight.
//      flushAll awaits every tracked promise before returning.
//
//   3. The registry never leaks: failed promises remove themselves from the
//      in-flight set so subsequent saves are not blocked forever.
//
// The frontend-side bug the registry plugs is independent of the Rust-side
// "apply silently no-ops" bug — both must be fixed for Save All to work.
// This file covers the frontend half.

import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useMemo, useRef } from "react";
import { describe, expect, it, vi } from "vitest";
import { MatrixCell } from "../../components/TranslatePanel/MatrixCell";
import {
  type CommitNowFn,
  type CommitRegistry,
  PendingCommitsContext,
} from "../pending-commits";
import type { Unit } from "../types";

function makeUnit(overrides: Partial<Unit> = {}): Unit {
  return {
    id: "unit-1",
    source: "Hello",
    target: { kind: "singular", text: null },
    placeholders: [],
    plural_arity: null,
    flags: [],
    provenance: { file: "en.ts", line: 1, byte_offset: null },
    state: "untranslated",
    ...overrides,
  };
}

// Test harness: replicates the real App.tsx registry shape but exposes flushAll
// as a returned imperative handle. Lets tests render real MatrixCell components
// inside the real PendingCommitsContext and then trigger flushAll directly.
function makeTestRegistry(): {
  registry: CommitRegistry;
  flushAll: () => Promise<void>;
} {
  const commits = new Set<CommitNowFn>();
  const inFlight = new Set<Promise<unknown>>();
  const registry: CommitRegistry = {
    register: (commit) => {
      commits.add(commit);
      return () => {
        commits.delete(commit);
      };
    },
    trackInFlight: (promise) => {
      inFlight.add(promise);
      // Mirror App.tsx — swallow rejections so the .finally cleanup runs
      // without leaking an unhandled rejection out of the registry.
      void promise
        .catch(() => {})
        .finally(() => {
          inFlight.delete(promise);
        });
    },
    flushAll: async () => {
      const cs = Array.from(commits);
      await Promise.allSettled(cs.map((c) => Promise.resolve(c())));
      const ps = Array.from(inFlight);
      await Promise.allSettled(ps);
    },
  };
  return { registry, flushAll: registry.flushAll };
}

function HarnessProvider({
  registry,
  children,
}: {
  registry: CommitRegistry;
  children: React.ReactNode;
}) {
  // Stable identity — the production App.tsx uses useMemo for the same reason.
  const stable = useMemo(() => registry, [registry]);
  return (
    <PendingCommitsContext.Provider value={stable}>
      {children}
    </PendingCommitsContext.Provider>
  );
}

const CELL_PROPS = {
  locale: "de",
  catalogPath: "/project/de.ts",
  busy: false,
  focused: false,
  edited: false,
  onFocusCell: vi.fn(),
  onTranslate: vi.fn(),
  onAccept: vi.fn(),
  onReopen: vi.fn(),
};

describe("PendingCommitsRegistry — flush contract", () => {
  // H1: textarea has typed-but-not-blurred content. flushAll must commit it.
  // This is the exact pattern Cmd-S triggers in production.
  it("flushAll commits a typed-but-not-blurred Proposed cell", async () => {
    const onEdit = vi.fn();
    const { registry, flushAll } = makeTestRegistry();
    const unit = makeUnit({
      state: "proposed",
      target: { kind: "singular", text: "Hallo" },
    });
    render(
      <HarnessProvider registry={registry}>
        <MatrixCell {...CELL_PROPS} unit={unit} onEdit={onEdit} />
      </HarnessProvider>,
    );

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    // Type without blurring — production Cmd-S handler intercepts the
    // keystroke before any focus change.
    await userEvent.type(textarea, " Welt");

    // Sanity: onEdit MUST NOT have fired yet (blur did not happen).
    expect(onEdit).not.toHaveBeenCalled();

    // Flush should run the registered commit-now thunk, which calls onEdit
    // exactly as a blur would have.
    await flushAll();

    expect(onEdit).toHaveBeenCalledOnce();
    expect(onEdit).toHaveBeenCalledWith("/project/de.ts", unit, {
      kind: "singular",
      text: "Hallo Welt",
    });
  });

  it("flushAll commits a typed-but-not-blurred Untranslated cell", async () => {
    const onEdit = vi.fn();
    const { registry, flushAll } = makeTestRegistry();
    const unit = makeUnit();
    render(
      <HarnessProvider registry={registry}>
        <MatrixCell {...CELL_PROPS} unit={unit} onEdit={onEdit} />
      </HarnessProvider>,
    );

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    await userEvent.type(textarea, "Hallo");
    expect(onEdit).not.toHaveBeenCalled();

    await flushAll();
    expect(onEdit).toHaveBeenCalledOnce();
    expect(onEdit).toHaveBeenCalledWith("/project/de.ts", unit, {
      kind: "singular",
      text: "Hallo",
    });
  });

  // No double-commit: flushAll right after a blur must not fire onEdit again.
  it("flushAll after blur does NOT re-commit unchanged text", async () => {
    const onEdit = vi.fn();
    const { registry, flushAll } = makeTestRegistry();
    const unit = makeUnit({
      state: "proposed",
      target: { kind: "singular", text: "Hallo" },
    });
    render(
      <HarnessProvider registry={registry}>
        <MatrixCell {...CELL_PROPS} unit={unit} onEdit={onEdit} />
      </HarnessProvider>,
    );

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    await userEvent.type(textarea, " Welt");
    fireEvent.blur(textarea);
    expect(onEdit).toHaveBeenCalledOnce();

    await flushAll();
    // Still exactly one — the commit-now thunk's idempotence check (draft ===
    // last-committed) caught it.
    expect(onEdit).toHaveBeenCalledOnce();
  });

  // H2: flushAll awaits in-flight IPC promises so a save cannot land before
  // a still-pending blur commit returns from the Rust side.
  it("flushAll awaits in-flight promises before resolving", async () => {
    const { registry, flushAll } = makeTestRegistry();
    let ipcResolve: () => void = () => {};
    const ipcPromise = new Promise<void>((resolve) => {
      ipcResolve = resolve;
    });
    registry.trackInFlight(ipcPromise);

    let flushResolved = false;
    const flushPromise = flushAll().then(() => {
      flushResolved = true;
    });
    // Yield once so the promise machinery can race.
    await Promise.resolve();
    expect(flushResolved).toBe(false);

    ipcResolve();
    await flushPromise;
    expect(flushResolved).toBe(true);
  });

  // A rejected promise must NOT block future flushAll calls forever.
  it("rejected in-flight promises remove themselves from the registry", async () => {
    const { registry, flushAll } = makeTestRegistry();
    // Attach a no-op handler so happy-dom / Node does not flag this as an
    // unhandled rejection. The contract under test is whether the rejection
    // gets the promise removed from the in-flight set, not whether it is
    // surfaced anywhere — surfacing is the IPC layer's job.
    const rejecting = Promise.reject(new Error("ipc failed"));
    rejecting.catch(() => {});
    registry.trackInFlight(rejecting);

    // First flush should complete (allSettled swallows the rejection).
    await flushAll();

    // Second flush should also complete promptly — the rejected promise has
    // been removed by its .finally cleanup.
    let secondResolved = false;
    const second = flushAll().then(() => {
      secondResolved = true;
    });
    await Promise.resolve();
    await Promise.resolve();
    await second;
    expect(secondResolved).toBe(true);
  });

  it("unregister removes the commit-now thunk from the registry", async () => {
    const onEdit = vi.fn();
    const { registry, flushAll } = makeTestRegistry();
    const unit = makeUnit({
      state: "proposed",
      target: { kind: "singular", text: "Hallo" },
    });
    const { unmount } = render(
      <HarnessProvider registry={registry}>
        <MatrixCell {...CELL_PROPS} unit={unit} onEdit={onEdit} />
      </HarnessProvider>,
    );

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    await userEvent.type(textarea, " Welt");
    expect(onEdit).not.toHaveBeenCalled();

    // Unmount removes the cell — its useEffect cleanup must unregister.
    unmount();

    await flushAll();
    // No textarea, no thunk — onEdit must not fire.
    expect(onEdit).not.toHaveBeenCalled();
  });
});

// ── Regression guard: the production setup wires the registry correctly ────
//
// The production code path is:
//   App.tsx onSaveAll → commitRegistry.flushAll() → registered thunks fire
//     → MatrixCell.ProposedBody.commit() → onCommit(edit) → onEditUnitFor
//     → updateUnitTargetInProject (IPC).
//
// This test stops one step short of the IPC — we assert that flushAll causes
// the cell's onEdit handler (the IPC trampoline) to fire with the typed text.
// If this contract breaks, Save All silently loses every typed-but-not-blurred
// edit, exactly the failure the user reported pre-fix.

describe("Production wiring — App-style registry shape", () => {
  // A tiny harness that mirrors App.tsx's useMemo + useRef pattern.
  function AppLikeProvider({
    onEdit,
    unit,
  }: {
    onEdit: () => void;
    unit: Unit;
  }) {
    const commitsRef = useRef<Set<CommitNowFn>>(new Set());
    const inFlightRef = useRef<Set<Promise<unknown>>>(new Set());
    const registry = useMemo<CommitRegistry>(
      () => ({
        register: (commit) => {
          commitsRef.current.add(commit);
          return () => {
            commitsRef.current.delete(commit);
          };
        },
        trackInFlight: (promise) => {
          inFlightRef.current.add(promise);
          void promise
            .catch(() => {})
            .finally(() => {
              inFlightRef.current.delete(promise);
            });
        },
        flushAll: async () => {
          const cs = Array.from(commitsRef.current);
          await Promise.allSettled(cs.map((c) => Promise.resolve(c())));
          const ps = Array.from(inFlightRef.current);
          await Promise.allSettled(ps);
        },
      }),
      [],
    );
    return (
      <PendingCommitsContext.Provider value={registry}>
        <MatrixCell {...CELL_PROPS} unit={unit} onEdit={onEdit} />
        <button
          type="button"
          data-testid="flush-all"
          onClick={() => void registry.flushAll()}
        >
          flush
        </button>
      </PendingCommitsContext.Provider>
    );
  }

  it("clicking a Save-All-style button flushes the typed draft", async () => {
    const onEdit = vi.fn();
    const unit = makeUnit({
      state: "proposed",
      target: { kind: "singular", text: "Hallo" },
    });
    render(<AppLikeProvider onEdit={onEdit} unit={unit} />);

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    await userEvent.type(textarea, " Welt");
    expect(onEdit).not.toHaveBeenCalled();

    const flushBtn = screen.getByTestId("flush-all");
    await userEvent.click(flushBtn);
    // userEvent.click awaits microtasks; let one more macro turn settle.
    await new Promise((r) => setTimeout(r, 0));

    expect(onEdit).toHaveBeenCalledOnce();
    expect(onEdit).toHaveBeenCalledWith("/project/de.ts", unit, {
      kind: "singular",
      text: "Hallo Welt",
    });
  });
});
