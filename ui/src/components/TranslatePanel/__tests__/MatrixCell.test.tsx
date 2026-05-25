// MatrixCell — unit tests for the untranslated-state editable draft surface.
//
// Verifies:
//   1. Untranslated state renders a textarea with data-matrix-cell-input.
//   2. Typing one character then blurring dispatches onEdit with { kind: "singular", text: "x" }.
//   3. Typing nothing and blurring does NOT dispatch onEdit.
//   4. The sparkle button calls onTranslate when clicked.
//   5. When busy=true the textarea is disabled and the sparkle shows a spinner.

import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Unit } from "../../../lib/types";
import { MatrixCell } from "../MatrixCell";

// ── Fixture helpers ───────────────────────────────────────────────────────────

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

const BASE_PROPS = {
  locale: "de",
  catalogPath: "/project/de.ts",
  busy: false,
  focused: false,
  edited: false,
  onFocusCell: vi.fn(),
  onTranslate: vi.fn(),
  onEdit: vi.fn(),
  onAccept: vi.fn(),
  onReopen: vi.fn(),
};

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("MatrixCell — untranslated state", () => {
  it("renders a textarea with data-matrix-cell-input attribute", () => {
    const unit = makeUnit();
    render(<MatrixCell {...BASE_PROPS} unit={unit} />);

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    expect(textarea).toBeInTheDocument();
    expect(textarea).toHaveAttribute("data-matrix-cell-input");
  });

  it("dispatches onEdit with { kind: 'singular', text: 'x' } after typing 'x' and blurring", async () => {
    const onEdit = vi.fn();
    const unit = makeUnit();
    render(<MatrixCell {...BASE_PROPS} unit={unit} onEdit={onEdit} />);

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    await userEvent.type(textarea, "x");
    fireEvent.blur(textarea);

    expect(onEdit).toHaveBeenCalledOnce();
    expect(onEdit).toHaveBeenCalledWith("/project/de.ts", unit, {
      kind: "singular",
      text: "x",
    });
  });

  it("does NOT dispatch onEdit when the user types nothing and blurs", () => {
    const onEdit = vi.fn();
    const unit = makeUnit();
    render(<MatrixCell {...BASE_PROPS} unit={unit} onEdit={onEdit} />);

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    fireEvent.blur(textarea);

    expect(onEdit).not.toHaveBeenCalled();
  });

  it("does NOT dispatch onEdit when the user types then clears then blurs", async () => {
    const onEdit = vi.fn();
    const unit = makeUnit();
    render(<MatrixCell {...BASE_PROPS} unit={unit} onEdit={onEdit} />);

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    await userEvent.type(textarea, "x");
    await userEvent.clear(textarea);
    fireEvent.blur(textarea);

    expect(onEdit).not.toHaveBeenCalled();
  });

  it("calls onTranslate when the sparkle button is clicked", async () => {
    const onTranslate = vi.fn();
    const unit = makeUnit();
    render(
      <MatrixCell {...BASE_PROPS} unit={unit} onTranslate={onTranslate} />,
    );

    const btn = screen.getByRole("button", { name: /translate with model/i });
    await userEvent.click(btn);

    expect(onTranslate).toHaveBeenCalledOnce();
    expect(onTranslate).toHaveBeenCalledWith("/project/de.ts", unit);
  });

  it("disables the textarea and sparkle button when busy=true", () => {
    const unit = makeUnit();
    render(<MatrixCell {...BASE_PROPS} unit={unit} busy={true} />);

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    expect(textarea).toBeDisabled();

    const btn = screen.getByRole("button", { name: /translate with model/i });
    expect(btn).toBeDisabled();
  });

  it("dispatches onEdit with form_index:0 for a plural unit", async () => {
    const onEdit = vi.fn();
    const unit = makeUnit({
      target: { kind: "plural", forms: [null, null] },
      plural_arity: 2,
    });
    render(<MatrixCell {...BASE_PROPS} unit={unit} onEdit={onEdit} />);

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    await userEvent.type(textarea, "eine");
    fireEvent.blur(textarea);

    expect(onEdit).toHaveBeenCalledOnce();
    expect(onEdit).toHaveBeenCalledWith("/project/de.ts", unit, {
      kind: "plural",
      form_index: 0,
      text: "eine",
    });
  });
});

describe("MatrixCell — proposed and finished states not broken", () => {
  it("renders a textarea in proposed state", () => {
    const unit = makeUnit({
      state: "proposed",
      target: { kind: "singular", text: "Hallo" },
    });
    render(<MatrixCell {...BASE_PROPS} unit={unit} />);

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    expect(textarea).toBeInTheDocument();
    expect(textarea).toHaveValue("Hallo");
    expect(textarea).toHaveAttribute("data-matrix-cell-input");
  });

  it("renders read-only text in finished state", () => {
    const unit = makeUnit({
      state: "finished",
      target: { kind: "singular", text: "Hallo" },
    });
    render(<MatrixCell {...BASE_PROPS} unit={unit} />);

    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(screen.getByText("Hallo")).toBeInTheDocument();
  });
});

// ── Spinner chain verification ────────────────────────────────────────────────
// These tests prove the busy prop actually renders a Spinner component in each
// state body. The Spinner is tagged with data-testid="translate-spinner".

describe("MatrixCell — spinner visibility (Bug 2 regression guard)", () => {
  it("untranslated + busy=true: Spinner is present in the sparkle button", () => {
    const unit = makeUnit({ state: "untranslated" });
    render(<MatrixCell {...BASE_PROPS} unit={unit} busy={true} />);
    expect(screen.getByTestId("translate-spinner")).toBeInTheDocument();
  });

  it("untranslated + busy=false: no Spinner rendered", () => {
    const unit = makeUnit({ state: "untranslated" });
    render(<MatrixCell {...BASE_PROPS} unit={unit} busy={false} />);
    expect(screen.queryByTestId("translate-spinner")).not.toBeInTheDocument();
  });

  it("proposed + busy=true: Spinner is present in the retry button", () => {
    const unit = makeUnit({
      state: "proposed",
      target: { kind: "singular", text: "Hallo" },
    });
    render(<MatrixCell {...BASE_PROPS} unit={unit} busy={true} />);
    expect(screen.getByTestId("translate-spinner")).toBeInTheDocument();
  });

  it("proposed + busy=true: retry button has accent colour and cursor-wait (not opacity-40)", () => {
    const unit = makeUnit({
      state: "proposed",
      target: { kind: "singular", text: "Hallo" },
    });
    render(<MatrixCell {...BASE_PROPS} unit={unit} busy={true} />);
    const retryBtn = screen.getByRole("button", {
      name: /retry translation with the model/i,
    });
    expect(retryBtn.className).toContain("text-accent");
    expect(retryBtn.className).toContain("cursor-wait");
    expect(retryBtn.className).not.toContain("opacity-40");
  });

  it("proposed + busy=false: no Spinner rendered", () => {
    const unit = makeUnit({
      state: "proposed",
      target: { kind: "singular", text: "Hallo" },
    });
    render(<MatrixCell {...BASE_PROPS} unit={unit} busy={false} />);
    expect(screen.queryByTestId("translate-spinner")).not.toBeInTheDocument();
  });

  it("proposed + busy=true: textarea is disabled", () => {
    const unit = makeUnit({
      state: "proposed",
      target: { kind: "singular", text: "Hallo" },
    });
    render(<MatrixCell {...BASE_PROPS} unit={unit} busy={true} />);
    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    expect(textarea).toBeDisabled();
  });
});
