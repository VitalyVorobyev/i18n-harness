// BatchProgressWidget — progress bar, ETA, and cancel-button tests.
//
// Covers:
//   - ProgressBar renders with correct ARIA attributes (role, valuenow, valuemax)
//   - ETA is hidden when completed === 0 (no rate signal yet)
//   - ETA is visible after at least one unit has completed
//   - ETA is hidden when batch is fully complete
//   - Cancel button fires onCancel callback
//   - Recent activity list renders when present

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { type ActiveBatch, BatchProgressWidget } from "../BatchProgressWidget";

/** Build a minimal ActiveBatch fixture, overriding any fields via partial. */
function makeBatch(overrides: Partial<ActiveBatch> = {}): ActiveBatch {
  return {
    jobId: "job-1",
    catalogPath: "/project/app_de.ts",
    catalogName: "app_de.ts",
    completed: 0,
    total: 10,
    recent: [],
    startedAt: Date.now() - 5_000, // 5 s ago
    ...overrides,
  };
}

describe("BatchProgressWidget", () => {
  it("renders a progressbar with correct ARIA attributes", () => {
    render(
      <BatchProgressWidget
        batch={makeBatch({ completed: 3, total: 10 })}
        onCancel={vi.fn()}
      />,
    );
    const bar = screen.getByRole("progressbar");
    expect(bar).toHaveAttribute("aria-valuenow", "3");
    expect(bar).toHaveAttribute("aria-valuemax", "10");
    expect(bar).toHaveAttribute("aria-valuemin", "0");
  });

  it("progressbar aria-label describes translation progress", () => {
    render(
      <BatchProgressWidget
        batch={makeBatch({ completed: 2, total: 8 })}
        onCancel={vi.fn()}
      />,
    );
    const bar = screen.getByRole("progressbar");
    expect(bar).toHaveAttribute("aria-label", "2 of 8 units translated");
  });

  it("hides ETA when completed is 0 (no rate signal)", () => {
    render(
      <BatchProgressWidget
        batch={makeBatch({ completed: 0 })}
        onCancel={vi.fn()}
      />,
    );
    // No element whose text contains "remaining"
    expect(screen.queryByText(/remaining/i)).toBeNull();
  });

  it("shows ETA once at least one unit has completed", () => {
    // 1 unit done in 5 s; 9 remaining → ETA ~45 s
    render(
      <BatchProgressWidget
        batch={makeBatch({
          completed: 1,
          total: 10,
          startedAt: Date.now() - 5_000,
        })}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText(/remaining/i)).toBeTruthy();
  });

  it("hides ETA when batch is fully complete (completed === total)", () => {
    render(
      <BatchProgressWidget
        batch={makeBatch({
          completed: 10,
          total: 10,
          startedAt: Date.now() - 20_000,
        })}
        onCancel={vi.fn()}
      />,
    );
    // At 100 % there is nothing remaining — completed < total is false
    expect(screen.queryByText(/remaining/i)).toBeNull();
  });

  it("cancel button calls onCancel when clicked", async () => {
    const onCancel = vi.fn();
    render(<BatchProgressWidget batch={makeBatch()} onCancel={onCancel} />);
    await userEvent.click(screen.getByRole("button", { name: /cancel/i }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("cancel button is always visible", () => {
    render(<BatchProgressWidget batch={makeBatch()} onCancel={vi.fn()} />);
    expect(screen.getByRole("button", { name: /cancel/i })).toBeTruthy();
  });

  it("renders recent activity list when recent ids are present", () => {
    render(
      <BatchProgressWidget
        batch={makeBatch({ recent: ["unit-1", "unit-2"] })}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText(/unit-1/)).toBeTruthy();
    expect(screen.getByText(/unit-2/)).toBeTruthy();
  });

  it("does not render recent activity section when recent is empty", () => {
    const { container } = render(
      <BatchProgressWidget
        batch={makeBatch({ recent: [] })}
        onCancel={vi.fn()}
      />,
    );
    // No "Last:" label present
    expect(container.textContent).not.toContain("Last:");
  });

  it("ETA text reads '<1s' for sub-second remainders", () => {
    // 9 units done in 450 ms → rate = 50 ms/unit → 1 unit left → ~50 ms
    render(
      <BatchProgressWidget
        batch={makeBatch({
          completed: 9,
          total: 10,
          startedAt: Date.now() - 450,
        })}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText(/remaining/i).textContent).toContain("<1s");
  });
});
