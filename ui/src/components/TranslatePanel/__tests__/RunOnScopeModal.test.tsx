// RunOnScopeModal — unit tests.
//
// Verifies:
//   1. Preview renders rows for all pairs; skipped (count=0) rows carry "skipped" label.
//   2. Clicking Start invokes startPair only for non-skipped pairs.
//   3. Per-pair progress bars update when progress callbacks fire.
//   4. Cancel button calls the cancel function returned by startPair.
//   5. Modal transitions to Close button after all pairs reach terminal state.
//   6. Active pairs run sequentially — second pair not started until first fires terminal.
//   7. Cancel drops the remaining queued pairs after cancelling the active one.

import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type {
  PairJobHandle,
  PairProgress,
  PairTerminal,
  ScopePair,
  StartPairFn,
} from "../RunOnScopeModal";
import { RunOnScopeModal } from "../RunOnScopeModal";

// ── Fixture helpers ───────────────────────────────────────────────────────────

function makePair(locale: string, count: number): ScopePair {
  return {
    catalogPath: `/project/${locale}.ts`,
    catalogName: `${locale}.ts`,
    locale,
    untranslatedCount: count,
  };
}

// A startPair stub that captures the callbacks and exposes them for test control.
// Each invocation gets its own cancel mock and callback slots.
function makeStartPairStub() {
  let _onProgress: ((p: PairProgress) => void) | null = null;
  let _onTerminal: ((t: PairTerminal) => void) | null = null;
  const cancel = vi.fn();

  const stub: StartPairFn = vi.fn(async (_pair, onProgress, onTerminal) => {
    _onProgress = onProgress;
    _onTerminal = onTerminal;
    const handle: PairJobHandle = { jobId: "job-test", cancel };
    return handle;
  });

  return {
    stub,
    cancel,
    fireProgress: (p: PairProgress) => _onProgress?.(p),
    fireTerminal: (t: PairTerminal) => _onTerminal?.(t),
  };
}

// Multi-invocation stub: tracks callbacks per call index for sequential tests.
function makeMultiStartPairStub() {
  const calls: Array<{
    pair: ScopePair;
    onProgress: (p: PairProgress) => void;
    onTerminal: (t: PairTerminal) => void;
    cancel: ReturnType<typeof vi.fn>;
  }> = [];

  const stub: StartPairFn = vi.fn(async (pair, onProgress, onTerminal) => {
    const cancel = vi.fn();
    calls.push({ pair, onProgress, onTerminal, cancel });
    return { jobId: `job-${calls.length}`, cancel };
  });

  return {
    stub,
    calls,
    fireTerminalForCall: (idx: number, t: PairTerminal) =>
      calls[idx]?.onTerminal(t),
  };
}

const BASE_CLOSE = vi.fn();

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("RunOnScopeModal — preview phase", () => {
  it("renders a row for each pair", () => {
    const pairs = [makePair("de", 5), makePair("fr", 3)];
    render(
      <RunOnScopeModal
        open={true}
        pairs={pairs}
        onClose={BASE_CLOSE}
        startPair={vi.fn()}
      />,
    );
    expect(screen.getByText("de.ts")).toBeInTheDocument();
    expect(screen.getByText("fr.ts")).toBeInTheDocument();
  });

  it("renders a 'skipped' label for pairs with 0 untranslated units", () => {
    const pairs = [makePair("de", 5), makePair("zh", 0)];
    render(
      <RunOnScopeModal
        open={true}
        pairs={pairs}
        onClose={BASE_CLOSE}
        startPair={vi.fn()}
      />,
    );
    // The "0 units" pair should show "skipped".
    expect(screen.getByText("skipped")).toBeInTheDocument();
  });

  it("renders correct headline unit count", () => {
    const pairs = [makePair("de", 5), makePair("fr", 3), makePair("zh", 0)];
    render(
      <RunOnScopeModal
        open={true}
        pairs={pairs}
        onClose={BASE_CLOSE}
        startPair={vi.fn()}
      />,
    );
    // 5 + 3 = 8 units total; 2 pairs active.
    expect(screen.getByText("2")).toBeInTheDocument();
    expect(screen.getByText("8")).toBeInTheDocument();
  });

  it("does not render when open=false", () => {
    const pairs = [makePair("de", 5)];
    render(
      <RunOnScopeModal
        open={false}
        pairs={pairs}
        onClose={BASE_CLOSE}
        startPair={vi.fn()}
      />,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("calls onClose when Cancel is clicked in preview phase", async () => {
    const onClose = vi.fn();
    render(
      <RunOnScopeModal
        open={true}
        pairs={[makePair("de", 5)]}
        onClose={onClose}
        startPair={vi.fn()}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: /cancel/i }));
    expect(onClose).toHaveBeenCalledOnce();
  });
});

describe("RunOnScopeModal — Start / running phase", () => {
  it("clicking Start calls startPair only for non-skipped pairs", async () => {
    const { stub } = makeStartPairStub();
    const pairs = [makePair("de", 5), makePair("fr", 0)];
    render(
      <RunOnScopeModal
        open={true}
        pairs={pairs}
        onClose={vi.fn()}
        startPair={stub}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: /^start$/i }));

    await waitFor(() => {
      expect(stub).toHaveBeenCalledTimes(1);
    });
    expect(stub).toHaveBeenCalledWith(
      pairs[0],
      expect.any(Function),
      expect.any(Function),
    );
  });

  it("per-pair progress bar updates when progress callback fires", async () => {
    const { stub, fireProgress } = makeStartPairStub();
    const pairs = [makePair("de", 10)];
    render(
      <RunOnScopeModal
        open={true}
        pairs={pairs}
        onClose={vi.fn()}
        startPair={stub}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: /^start$/i }));

    // Wait for startPair to be called
    await waitFor(() => expect(stub).toHaveBeenCalled());

    // Fire progress event
    act(() => {
      fireProgress({ completed: 4, total: 10 });
    });

    // Progress bar should reflect 4/10
    await waitFor(() => {
      const bar = screen.getByRole("progressbar", { name: /progress for de/i });
      expect(bar).toHaveAttribute("aria-valuenow", "4");
      expect(bar).toHaveAttribute("aria-valuemax", "10");
    });
  });

  it("Cancel translation button calls the cancel function", async () => {
    const { stub, cancel } = makeStartPairStub();
    const pairs = [makePair("de", 5)];
    render(
      <RunOnScopeModal
        open={true}
        pairs={pairs}
        onClose={vi.fn()}
        startPair={stub}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: /^start$/i }));
    await waitFor(() => expect(stub).toHaveBeenCalled());

    const cancelBtn = await screen.findByRole("button", {
      name: /cancel translation/i,
    });
    await userEvent.click(cancelBtn);

    expect(cancel).toHaveBeenCalled();
  });
});

describe("RunOnScopeModal — terminal phase", () => {
  it("shows Done status and Close button after terminal event", async () => {
    const { stub, fireTerminal } = makeStartPairStub();
    const pairs = [makePair("de", 3)];
    render(
      <RunOnScopeModal
        open={true}
        pairs={pairs}
        onClose={vi.fn()}
        startPair={stub}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: /^start$/i }));
    await waitFor(() => expect(stub).toHaveBeenCalled());

    act(() => {
      fireTerminal({
        completed: 3,
        total: 3,
        cancelled: false,
        failedReason: null,
      });
    });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /^close$/i }),
      ).toBeInTheDocument();
    });
    expect(screen.getByText(/done/i)).toBeInTheDocument();
  });

  it("shows Failed status when terminal fires with a reason", async () => {
    const { stub, fireTerminal } = makeStartPairStub();
    const pairs = [makePair("de", 3)];
    render(
      <RunOnScopeModal
        open={true}
        pairs={pairs}
        onClose={vi.fn()}
        startPair={stub}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: /^start$/i }));
    await waitFor(() => expect(stub).toHaveBeenCalled());

    act(() => {
      fireTerminal({
        completed: 1,
        total: 3,
        cancelled: false,
        failedReason: "ollama not running",
      });
    });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /^close$/i }),
      ).toBeInTheDocument();
    });
    expect(screen.getByText(/failed/i)).toBeInTheDocument();
  });

  it("calls onClose when Close is clicked after terminal", async () => {
    const { stub, fireTerminal } = makeStartPairStub();
    const onClose = vi.fn();
    const pairs = [makePair("de", 2)];
    render(
      <RunOnScopeModal
        open={true}
        pairs={pairs}
        onClose={onClose}
        startPair={stub}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: /^start$/i }));
    await waitFor(() => expect(stub).toHaveBeenCalled());

    act(() => {
      fireTerminal({
        completed: 2,
        total: 2,
        cancelled: false,
        failedReason: null,
      });
    });

    const closeBtn = await screen.findByRole("button", { name: /^close$/i });
    await userEvent.click(closeBtn);
    expect(onClose).toHaveBeenCalled();
  });
});

describe("RunOnScopeModal — sequential execution", () => {
  it("does not start the second pair until the first fires its terminal event", async () => {
    const { stub, calls, fireTerminalForCall } = makeMultiStartPairStub();
    const pairs = [makePair("de", 3), makePair("fr", 2)];

    render(
      <RunOnScopeModal
        open={true}
        pairs={pairs}
        onClose={vi.fn()}
        startPair={stub}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: /^start$/i }));

    // Only the first pair should have been started.
    await waitFor(() => expect(stub).toHaveBeenCalledTimes(1));
    expect(calls.length).toBe(1);

    // Fire terminal for the first pair — the second pair should start now.
    act(() => {
      fireTerminalForCall(0, {
        completed: 3,
        total: 3,
        cancelled: false,
        failedReason: null,
      });
    });

    await waitFor(() => expect(stub).toHaveBeenCalledTimes(2));
    expect(calls.length).toBe(2);
    expect(calls[1]?.pair.locale).toBe("fr");
  });

  it("shows Queued label for pairs that have not started yet", async () => {
    const { stub } = makeMultiStartPairStub();
    const pairs = [makePair("de", 3), makePair("fr", 2)];

    render(
      <RunOnScopeModal
        open={true}
        pairs={pairs}
        onClose={vi.fn()}
        startPair={stub}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: /^start$/i }));

    // Wait for the first pair to start.
    await waitFor(() => expect(stub).toHaveBeenCalledTimes(1));

    // The fr pair should show a queued label while de is running.
    expect(screen.getByText(/queued/i)).toBeInTheDocument();
  });

  it("cancel stops the active pair and skips remaining queued pairs", async () => {
    const { stub, calls, fireTerminalForCall } = makeMultiStartPairStub();
    const pairs = [makePair("de", 3), makePair("fr", 2)];

    render(
      <RunOnScopeModal
        open={true}
        pairs={pairs}
        onClose={vi.fn()}
        startPair={stub}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: /^start$/i }));
    await waitFor(() => expect(stub).toHaveBeenCalledTimes(1));

    // Click Cancel — should call the active pair's cancel fn.
    const cancelBtn = await screen.findByRole("button", {
      name: /cancel translation/i,
    });
    await userEvent.click(cancelBtn);
    expect(calls[0]?.cancel).toHaveBeenCalled();

    // Fire terminal for the first pair (cancelled).
    act(() => {
      fireTerminalForCall(0, {
        completed: 1,
        total: 3,
        cancelled: true,
        failedReason: null,
      });
    });

    // The second pair should never start since cancelledRef is true.
    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /^close$/i }),
      ).toBeInTheDocument();
    });
    expect(stub).toHaveBeenCalledTimes(1);
  });
});
