import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Unit, UnitState } from "../../../lib/types";
import {
  EMPTY_STATUS_FILTER,
  StatusFilter,
  type StatusFilterState,
  unitMatchesFilter,
} from "../StatusFilter";

// ── Fixture factory ───────────────────────────────────────────────────────────

function makeUnit(overrides: Partial<Unit> = {}): Unit {
  return {
    id: "test/unit",
    source: "Hello",
    target: { kind: "singular", text: null },
    placeholders: [],
    plural_arity: null,
    flags: [],
    provenance: { file: "src/main.cpp", line: 1, byte_offset: null },
    state: "untranslated",
    review_status: null,
    source_hash: null,
    confidence: null,
    flag_notes: null,
    ...overrides,
  };
}

function set(...states: UnitState[]): StatusFilterState {
  return new Set(states);
}

// ── unitMatchesFilter ─────────────────────────────────────────────────────────

describe("unitMatchesFilter", () => {
  it("empty filter passes every UI-editable unit state", () => {
    for (const state of ["untranslated", "proposed", "finished"] as const) {
      expect(unitMatchesFilter(makeUnit({ state }), EMPTY_STATUS_FILTER)).toBe(
        true,
      );
    }
  });

  it("vanished/obsolete units never pass regardless of filter", () => {
    for (const state of ["vanished", "obsolete"] as const) {
      expect(unitMatchesFilter(makeUnit({ state }), EMPTY_STATUS_FILTER)).toBe(
        false,
      );
      expect(
        unitMatchesFilter(
          makeUnit({ state }),
          set("untranslated", "proposed", "finished"),
        ),
      ).toBe(false);
    }
  });

  it("single-state filter — only matching state passes", () => {
    expect(
      unitMatchesFilter(makeUnit({ state: "proposed" }), set("proposed")),
    ).toBe(true);
    expect(
      unitMatchesFilter(makeUnit({ state: "untranslated" }), set("proposed")),
    ).toBe(false);
    expect(
      unitMatchesFilter(makeUnit({ state: "finished" }), set("proposed")),
    ).toBe(false);
  });

  it("multi-state filter — any matching state passes", () => {
    const filter = set("untranslated", "finished");
    expect(unitMatchesFilter(makeUnit({ state: "untranslated" }), filter)).toBe(
      true,
    );
    expect(unitMatchesFilter(makeUnit({ state: "finished" }), filter)).toBe(
      true,
    );
    expect(unitMatchesFilter(makeUnit({ state: "proposed" }), filter)).toBe(
      false,
    );
  });

  it("all three states selected — every UI-editable unit passes", () => {
    const filter = set("untranslated", "proposed", "finished");
    for (const state of ["untranslated", "proposed", "finished"] as const) {
      expect(unitMatchesFilter(makeUnit({ state }), filter)).toBe(true);
    }
  });
});

// ── StatusFilter component ────────────────────────────────────────────────────

describe("StatusFilter component", () => {
  it("renders exactly 3 pill buttons", () => {
    render(<StatusFilter value={EMPTY_STATUS_FILTER} onChange={() => {}} />);
    const toolbar = screen.getByRole("toolbar", { name: /status filter/i });
    expect(toolbar.querySelectorAll("button")).toHaveLength(3);
  });

  it("renders the three unit-state labels", () => {
    render(<StatusFilter value={EMPTY_STATUS_FILTER} onChange={() => {}} />);
    expect(screen.getByText("Untranslated")).toBeInTheDocument();
    expect(screen.getByText("Proposed")).toBeInTheDocument();
    expect(screen.getByText("Finished")).toBeInTheDocument();
  });

  it("empty filter — every button has aria-pressed=false", () => {
    render(<StatusFilter value={EMPTY_STATUS_FILTER} onChange={() => {}} />);
    const toolbar = screen.getByRole("toolbar", { name: /status filter/i });
    for (const btn of toolbar.querySelectorAll("button")) {
      expect(btn).toHaveAttribute("aria-pressed", "false");
    }
  });

  it("aria-pressed reflects each pill's membership in the set", () => {
    render(<StatusFilter value={set("proposed")} onChange={() => {}} />);
    expect(screen.getByText("Proposed").closest("button")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByText("Untranslated").closest("button")).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(screen.getByText("Finished").closest("button")).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("clicking an inactive pill adds its state to the filter set", async () => {
    const onChange = vi.fn();
    render(<StatusFilter value={EMPTY_STATUS_FILTER} onChange={onChange} />);
    await userEvent.click(screen.getByText("Proposed"));
    expect(onChange).toHaveBeenCalledTimes(1);
    const next = onChange.mock.calls[0]?.[0] as StatusFilterState;
    expect(Array.from(next)).toEqual(["proposed"]);
  });

  it("clicking an active pill removes its state from the filter set", async () => {
    const onChange = vi.fn();
    render(
      <StatusFilter
        value={set("untranslated", "proposed")}
        onChange={onChange}
      />,
    );
    await userEvent.click(screen.getByText("Proposed"));
    expect(onChange).toHaveBeenCalledTimes(1);
    const next = onChange.mock.calls[0]?.[0] as StatusFilterState;
    expect(Array.from(next)).toEqual(["untranslated"]);
  });

  it("multi-select — selecting all three pills accumulates the set", async () => {
    let current: StatusFilterState = EMPTY_STATUS_FILTER;
    const onChange = vi.fn((next: StatusFilterState) => {
      current = next;
    });
    const { rerender } = render(
      <StatusFilter value={current} onChange={onChange} />,
    );
    for (const label of ["Untranslated", "Proposed", "Finished"]) {
      await userEvent.click(screen.getByText(label));
      rerender(<StatusFilter value={current} onChange={onChange} />);
    }
    expect(Array.from(current).sort()).toEqual(
      ["finished", "proposed", "untranslated"].sort(),
    );
  });
});
