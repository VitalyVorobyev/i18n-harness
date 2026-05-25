import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Unit } from "../../../lib/types";
import {
  StatusFilter,
  type StatusFilterId,
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

// ── unitMatchesFilter ─────────────────────────────────────────────────────────

describe("unitMatchesFilter", () => {
  it("all — always true regardless of state", () => {
    for (const state of [
      "untranslated",
      "proposed",
      "finished",
      "vanished",
      "obsolete",
    ] as const) {
      expect(unitMatchesFilter(makeUnit({ state }), "all")).toBe(true);
    }
  });

  it("all-open — true for untranslated and proposed", () => {
    expect(
      unitMatchesFilter(makeUnit({ state: "untranslated" }), "all-open"),
    ).toBe(true);
    expect(unitMatchesFilter(makeUnit({ state: "proposed" }), "all-open")).toBe(
      true,
    );
  });

  it("all-open — false for finished, vanished, and obsolete", () => {
    expect(unitMatchesFilter(makeUnit({ state: "finished" }), "all-open")).toBe(
      false,
    );
    expect(unitMatchesFilter(makeUnit({ state: "vanished" }), "all-open")).toBe(
      false,
    );
    expect(unitMatchesFilter(makeUnit({ state: "obsolete" }), "all-open")).toBe(
      false,
    );
  });

  it("untranslated — true only when state is untranslated", () => {
    expect(
      unitMatchesFilter(makeUnit({ state: "untranslated" }), "untranslated"),
    ).toBe(true);
    expect(
      unitMatchesFilter(makeUnit({ state: "proposed" }), "untranslated"),
    ).toBe(false);
    expect(
      unitMatchesFilter(makeUnit({ state: "finished" }), "untranslated"),
    ).toBe(false);
  });

  it("proposed — true only when state is proposed", () => {
    expect(unitMatchesFilter(makeUnit({ state: "proposed" }), "proposed")).toBe(
      true,
    );
    expect(
      unitMatchesFilter(makeUnit({ state: "untranslated" }), "proposed"),
    ).toBe(false);
  });

  it("needs-review — true when unit has at least one soft flag", () => {
    // length-warn is soft
    const unit = makeUnit({ flags: ["length-warn"] });
    expect(unitMatchesFilter(unit, "needs-review")).toBe(true);
  });

  it("needs-review — false when only hard flags present", () => {
    const unit = makeUnit({ flags: ["placeholder-mismatch"] });
    expect(unitMatchesFilter(unit, "needs-review")).toBe(false);
  });

  it("needs-review — false when flags array is empty", () => {
    expect(unitMatchesFilter(makeUnit({ flags: [] }), "needs-review")).toBe(
      false,
    );
  });

  it("needs-review — true when both hard and soft flags are present", () => {
    const unit = makeUnit({
      flags: ["placeholder-mismatch", "length-warn"],
    });
    expect(unitMatchesFilter(unit, "needs-review")).toBe(true);
  });

  it("has-hard-flag — true when unit has at least one hard flag", () => {
    // placeholder-mismatch is hard
    const unit = makeUnit({ flags: ["placeholder-mismatch"] });
    expect(unitMatchesFilter(unit, "has-hard-flag")).toBe(true);
  });

  it("has-hard-flag — false when only soft flags present", () => {
    const unit = makeUnit({ flags: ["length-warn"] });
    expect(unitMatchesFilter(unit, "has-hard-flag")).toBe(false);
  });

  it("has-hard-flag — false when flags array is empty", () => {
    expect(unitMatchesFilter(makeUnit({ flags: [] }), "has-hard-flag")).toBe(
      false,
    );
  });

  it("proposed-by-model — true for proposed state", () => {
    expect(
      unitMatchesFilter(makeUnit({ state: "proposed" }), "proposed-by-model"),
    ).toBe(true);
  });

  it("proposed-by-model — false for non-proposed state", () => {
    expect(
      unitMatchesFilter(
        makeUnit({ state: "untranslated" }),
        "proposed-by-model",
      ),
    ).toBe(false);
    expect(
      unitMatchesFilter(makeUnit({ state: "finished" }), "proposed-by-model"),
    ).toBe(false);
  });
});

// ── StatusFilter component ────────────────────────────────────────────────────

describe("StatusFilter component", () => {
  it("renders exactly 7 pill buttons", () => {
    render(<StatusFilter value="all" onChange={() => {}} />);
    const tabs = screen.getAllByRole("tab");
    expect(tabs).toHaveLength(7);
  });

  it("clicking a pill calls onChange with the correct id", async () => {
    const onChange = vi.fn();
    render(<StatusFilter value="all" onChange={onChange} />);

    const pills: Array<[StatusFilterId, string]> = [
      ["all", "All"],
      ["all-open", "Open"],
      ["untranslated", "Untranslated"],
      ["proposed", "Proposed"],
      ["needs-review", "Needs review"],
      ["has-hard-flag", "Hard flag"],
      ["proposed-by-model", "Model proposal"],
    ];

    for (const [id, label] of pills) {
      const btn = screen.getByText(label);
      await userEvent.click(btn);
      expect(onChange).toHaveBeenLastCalledWith(id);
    }
  });

  it("active button has aria-selected=true", () => {
    render(<StatusFilter value="untranslated" onChange={() => {}} />);
    const tabs = screen.getAllByRole("tab");
    const activeTab = tabs.find(
      (t) => t.getAttribute("aria-selected") === "true",
    );
    expect(activeTab).toBeDefined();
    // biome-ignore lint/style/noNonNullAssertion: asserted defined above
    expect(activeTab!.textContent).toBe("Untranslated");
  });

  it("only the active button has aria-selected=true", () => {
    render(<StatusFilter value="proposed" onChange={() => {}} />);
    const tabs = screen.getAllByRole("tab");
    const activeTabs = tabs.filter(
      (t) => t.getAttribute("aria-selected") === "true",
    );
    expect(activeTabs).toHaveLength(1);
  });

  it("inactive buttons have aria-selected=false", () => {
    render(<StatusFilter value="all" onChange={() => {}} />);
    const tabs = screen.getAllByRole("tab");
    const inactiveTabs = tabs.filter(
      (t) => t.getAttribute("aria-selected") === "false",
    );
    expect(inactiveTabs).toHaveLength(6);
  });
});
