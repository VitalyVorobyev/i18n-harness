// UntranslatedDraftEditor — unit tests for the shared draft surface that
// appears in both MatrixCell and FocusView when a unit is untranslated.
//
// Verifies the FocusView ("full" size) contract in particular:
//   1. Renders a textarea with data-matrix-cell-input attribute.
//   2. Typing a character then blurring calls onCommit with { kind: "singular", text }.
//   3. Blurring an empty textarea does NOT call onCommit.
//   4. Typing then clearing then blurring does NOT call onCommit.
//   5. The sparkle button calls onTranslate when clicked.
//   6. When busy=true the textarea is disabled and the button is disabled.
//   7. Plural unit: commit uses form_index:0.

import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Unit } from "../../../lib/types";
import { UntranslatedDraftEditor } from "../UntranslatedDraftEditor";

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

// ── Tests — compact size (MatrixCell usage) ───────────────────────────────────

describe("UntranslatedDraftEditor — compact (MatrixCell)", () => {
  it("renders a textarea with data-matrix-cell-input", () => {
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={false}
        onCommit={vi.fn()}
        onTranslate={vi.fn()}
        size="compact"
      />,
    );

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    expect(textarea).toBeInTheDocument();
    expect(textarea).toHaveAttribute("data-matrix-cell-input");
  });

  it("calls onCommit after typing 'x' and blurring", async () => {
    const onCommit = vi.fn();
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={false}
        onCommit={onCommit}
        onTranslate={vi.fn()}
        size="compact"
      />,
    );

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    await userEvent.type(textarea, "x");
    fireEvent.blur(textarea);

    expect(onCommit).toHaveBeenCalledOnce();
    expect(onCommit).toHaveBeenCalledWith({ kind: "singular", text: "x" });
  });

  it("does NOT call onCommit on empty blur", () => {
    const onCommit = vi.fn();
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={false}
        onCommit={onCommit}
        onTranslate={vi.fn()}
        size="compact"
      />,
    );

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    fireEvent.blur(textarea);

    expect(onCommit).not.toHaveBeenCalled();
  });

  it("does NOT call onCommit when user types then clears then blurs", async () => {
    const onCommit = vi.fn();
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={false}
        onCommit={onCommit}
        onTranslate={vi.fn()}
        size="compact"
      />,
    );

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    await userEvent.type(textarea, "x");
    await userEvent.clear(textarea);
    fireEvent.blur(textarea);

    expect(onCommit).not.toHaveBeenCalled();
  });

  it("calls onTranslate when the sparkle button is clicked", async () => {
    const onTranslate = vi.fn();
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={false}
        onCommit={vi.fn()}
        onTranslate={onTranslate}
        size="compact"
      />,
    );

    const btn = screen.getByRole("button", { name: /translate with model/i });
    await userEvent.click(btn);

    expect(onTranslate).toHaveBeenCalledOnce();
  });

  it("disables textarea and button when busy=true", () => {
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={true}
        onCommit={vi.fn()}
        onTranslate={vi.fn()}
        size="compact"
      />,
    );

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    expect(textarea).toBeDisabled();

    const btn = screen.getByRole("button", { name: /translate with model/i });
    expect(btn).toBeDisabled();
  });

  it("commits with form_index:0 for plural units", async () => {
    const onCommit = vi.fn();
    const unit = makeUnit({
      target: { kind: "plural", forms: [null, null] },
      plural_arity: 2,
    });
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={false}
        onCommit={onCommit}
        onTranslate={vi.fn()}
        size="compact"
      />,
    );

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    await userEvent.type(textarea, "eine");
    fireEvent.blur(textarea);

    expect(onCommit).toHaveBeenCalledOnce();
    expect(onCommit).toHaveBeenCalledWith({
      kind: "plural",
      form_index: 0,
      text: "eine",
    });
  });
});

// ── Tests — full size (FocusView usage) ──────────────────────────────────────

describe("UntranslatedDraftEditor — full (FocusView)", () => {
  it("renders a textarea with data-matrix-cell-input in full size", () => {
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={false}
        onCommit={vi.fn()}
        onTranslate={vi.fn()}
        size="full"
      />,
    );

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    expect(textarea).toBeInTheDocument();
    expect(textarea).toHaveAttribute("data-matrix-cell-input");
  });

  it("calls onCommit with singular edit after typing and blurring", async () => {
    const onCommit = vi.fn();
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={false}
        onCommit={onCommit}
        onTranslate={vi.fn()}
        size="full"
      />,
    );

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    await userEvent.type(textarea, "Hallo Welt");
    fireEvent.blur(textarea);

    expect(onCommit).toHaveBeenCalledOnce();
    expect(onCommit).toHaveBeenCalledWith({
      kind: "singular",
      text: "Hallo Welt",
    });
  });

  it("does NOT call onCommit when blurred without typing", () => {
    const onCommit = vi.fn();
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={false}
        onCommit={onCommit}
        onTranslate={vi.fn()}
        size="full"
      />,
    );

    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    fireEvent.blur(textarea);

    expect(onCommit).not.toHaveBeenCalled();
  });

  it("calls onTranslate from the sparkle button in full size", async () => {
    const onTranslate = vi.fn();
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={false}
        onCommit={vi.fn()}
        onTranslate={onTranslate}
        size="full"
      />,
    );

    const btn = screen.getByRole("button", { name: /translate with model/i });
    await userEvent.click(btn);

    expect(onTranslate).toHaveBeenCalledOnce();
  });
});

// ── Spinner chain (Bug 2 regression guard) ───────────────────────────────────
// Spinner renders as <span data-testid="translate-spinner"> wrapping aria-hidden SVG.

describe("UntranslatedDraftEditor — spinner chain", () => {
  it("compact + busy=true: Spinner is present in the sparkle button", () => {
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={true}
        onCommit={vi.fn()}
        onTranslate={vi.fn()}
        size="compact"
      />,
    );
    expect(screen.getByTestId("translate-spinner")).toBeInTheDocument();
  });

  it("compact + busy=false: no Spinner rendered", () => {
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={false}
        onCommit={vi.fn()}
        onTranslate={vi.fn()}
        size="compact"
      />,
    );
    expect(screen.queryByTestId("translate-spinner")).not.toBeInTheDocument();
  });

  it("compact + busy=true: textarea is disabled", () => {
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={true}
        onCommit={vi.fn()}
        onTranslate={vi.fn()}
        size="compact"
      />,
    );
    const textarea = screen.getByRole("textbox", {
      name: /translation draft/i,
    });
    expect(textarea).toBeDisabled();
  });

  it("compact + busy=true: button has accent colour and cursor-wait (not opacity-40)", () => {
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={true}
        onCommit={vi.fn()}
        onTranslate={vi.fn()}
        size="compact"
      />,
    );
    const btn = screen.getByRole("button", { name: /translate with model/i });
    expect(btn.className).toContain("text-accent");
    expect(btn.className).toContain("cursor-wait");
    // Must NOT show the disabled-ghost treatment
    expect(btn.className).not.toContain("opacity-40");
  });

  it("full + busy=true: Spinner is present in the sparkle button", () => {
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={true}
        onCommit={vi.fn()}
        onTranslate={vi.fn()}
        size="full"
      />,
    );
    expect(screen.getByTestId("translate-spinner")).toBeInTheDocument();
  });

  it("full + busy=false: no Spinner rendered", () => {
    const unit = makeUnit();
    render(
      <UntranslatedDraftEditor
        unit={unit}
        busy={false}
        onCommit={vi.fn()}
        onTranslate={vi.fn()}
        size="full"
      />,
    );
    expect(screen.queryByTestId("translate-spinner")).not.toBeInTheDocument();
  });
});
