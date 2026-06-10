// ProofreadView — Copy / Save As export button tests.
//
// Covers:
//   - Two distinct accessible export controls are rendered
//   - "Copy" calls onToast with kind="info" on clipboard success
//   - "Copy" calls onToast with kind="error" on clipboard failure
//   - "Save .md" calls the save-dialog helper and toasts on confirm
//   - "Save .md" does NOT toast when the user cancels the dialog

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjectSummary } from "../../../lib/types";
import { ProofreadView } from "../ProofreadView";

// ── Tauri module mock ─────────────────────────────────────────────────────────
//
// ProofreadView imports pickReviewSaveLocation and writeTextFile from
// lib/tauri. We replace the entire module so tests do not need a real
// Tauri runtime.

vi.mock("../../../lib/tauri", () => ({
  pickReviewSaveLocation: vi.fn(),
  writeTextFile: vi.fn(),
}));

import { pickReviewSaveLocation, writeTextFile } from "../../../lib/tauri";

const mockPickReviewSaveLocation = vi.mocked(pickReviewSaveLocation);
const mockWriteTextFile = vi.mocked(writeTextFile);

// ── Minimal fixture ───────────────────────────────────────────────────────────

function makeMinimalSummary(): ProjectSummary {
  return {
    root: "/tmp/test-project",
    name: "Test Project",
    schema: 1,
    locales: ["de_DE"],
    catalogs: [],
    references: [],
    glossary_path: null,
    backend: null,
    state_dir: "/tmp/test-project/.i18n-harness",
  };
}

const baseProps = {
  summary: makeMinimalSummary(),
  openCatalogs: new Map(),
  reports: {},
  onNavigateToUnit: vi.fn(),
  onOpenHardFlags: vi.fn(),
};

// ── Tests ─────────────────────────────────────────────────────────────────────

// Clipboard mock — happy-dom does not expose navigator.clipboard as a writable
// property, so we use Object.defineProperty to install a mock once and
// replace the writeText spy per-test.
const clipboardWriteText = vi.fn().mockResolvedValue(undefined);

Object.defineProperty(navigator, "clipboard", {
  value: { writeText: clipboardWriteText },
  writable: true,
  configurable: true,
});

describe("ProofreadView — export controls", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Reset the clipboard mock to resolve successfully before each test.
    clipboardWriteText.mockResolvedValue(undefined);
  });

  it("renders two distinct export buttons", () => {
    render(<ProofreadView {...baseProps} onToast={vi.fn()} />);

    const copyBtn = screen.getByRole("button", {
      name: /copy review report to clipboard/i,
    });
    const saveBtn = screen.getByRole("button", {
      name: /save review report as markdown/i,
    });

    expect(copyBtn).toBeInTheDocument();
    expect(saveBtn).toBeInTheDocument();
    // The two buttons must be distinct elements.
    expect(copyBtn).not.toBe(saveBtn);
  });

  it("Copy button has visible label text 'Copy'", () => {
    render(<ProofreadView {...baseProps} onToast={vi.fn()} />);
    const btn = screen.getByRole("button", {
      name: /copy review report to clipboard/i,
    });
    expect(btn.textContent).toMatch(/Copy/);
  });

  it("Save button has visible label text 'Save .md'", () => {
    render(<ProofreadView {...baseProps} onToast={vi.fn()} />);
    const btn = screen.getByRole("button", {
      name: /save review report as markdown/i,
    });
    expect(btn.textContent).toMatch(/Save .md/);
  });

  // ── Copy: success ─────────────────────────────────────────────────────────

  it("Copy calls onToast with kind='info' on clipboard success", async () => {
    const onToast = vi.fn();
    render(<ProofreadView {...baseProps} onToast={onToast} />);

    const btn = screen.getByRole("button", {
      name: /copy review report to clipboard/i,
    });
    await userEvent.click(btn);

    expect(clipboardWriteText).toHaveBeenCalledOnce();
    expect(onToast).toHaveBeenCalledOnce();
    const [message, kind] = onToast.mock.calls[0] as [string, string];
    expect(kind).toBe("info");
    expect(message).toMatch(/copied/i);
  });

  // ── Copy: failure ─────────────────────────────────────────────────────────

  it("Copy calls onToast with kind='error' on clipboard failure", async () => {
    clipboardWriteText.mockRejectedValueOnce(new Error("permission denied"));
    const onToast = vi.fn();
    render(<ProofreadView {...baseProps} onToast={onToast} />);

    const btn = screen.getByRole("button", {
      name: /copy review report to clipboard/i,
    });
    await userEvent.click(btn);

    expect(onToast).toHaveBeenCalledOnce();
    const [, kind] = onToast.mock.calls[0] as [string, string];
    expect(kind).toBe("error");
  });

  // ── Save As: confirmed ────────────────────────────────────────────────────

  it("Save .md calls pickReviewSaveLocation, writes file, and toasts on confirm", async () => {
    mockPickReviewSaveLocation.mockResolvedValueOnce(
      "/tmp/test-project-review.md",
    );
    mockWriteTextFile.mockResolvedValueOnce(undefined);
    const onToast = vi.fn();
    render(<ProofreadView {...baseProps} onToast={onToast} />);

    const btn = screen.getByRole("button", {
      name: /save review report as markdown/i,
    });
    await userEvent.click(btn);

    expect(mockPickReviewSaveLocation).toHaveBeenCalledOnce();
    expect(mockWriteTextFile).toHaveBeenCalledOnce();
    expect(onToast).toHaveBeenCalledOnce();
    const [message, kind] = onToast.mock.calls[0] as [string, string];
    expect(kind).toBe("info");
    expect(message).toContain("/tmp/test-project-review.md");
  });

  // ── Save As: cancelled ────────────────────────────────────────────────────

  it("Save .md does not toast when the save dialog is cancelled", async () => {
    mockPickReviewSaveLocation.mockResolvedValueOnce(null);
    const onToast = vi.fn();
    render(<ProofreadView {...baseProps} onToast={onToast} />);

    const btn = screen.getByRole("button", {
      name: /save review report as markdown/i,
    });
    await userEvent.click(btn);

    expect(mockPickReviewSaveLocation).toHaveBeenCalledOnce();
    expect(mockWriteTextFile).not.toHaveBeenCalled();
    expect(onToast).not.toHaveBeenCalled();
  });

  // ── Graceful without onToast ──────────────────────────────────────────────

  it("does not throw when onToast is absent and Copy succeeds", async () => {
    render(<ProofreadView {...baseProps} />);
    const btn = screen.getByRole("button", {
      name: /copy review report to clipboard/i,
    });
    // Should not throw even though onToast is not provided.
    await expect(userEvent.click(btn)).resolves.not.toThrow();
  });
});
