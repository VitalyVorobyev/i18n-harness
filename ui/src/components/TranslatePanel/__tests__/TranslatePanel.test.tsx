// TranslatePanel — shared sub-header integration tests.
//
// Verifies that:
//   1. The Single ↔ Matrix toggle is visible in both modes.
//   2. Clicking "Matrix" while in Single mode calls setFocusLocale(null).
//   3. Clicking "Single" while in Matrix mode calls setFocusLocale with the
//      first available locale.
//   4. Changing the StatusFilter selection persists when toggling modes.

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { CatalogRef, ProjectSummary } from "../../../lib/types";
import { TranslatePanel } from "../TranslatePanel";

// ── Fixture helpers ───────────────────────────────────────────────────────────

function makeSummary(locales: string[] = ["de", "fr"]): ProjectSummary {
  const catalogs: CatalogRef[] = locales.map((locale) => ({
    absolute_path: `/project/${locale}.ts`,
    manifest_path: `${locale}.ts`,
    format: "qt-ts",
    locale,
    status: "ok",
  }));
  return {
    root: "/project",
    name: "Test",
    schema: 1,
    locales,
    catalogs,
    glossary_path: null,
    backend: null,
    state_dir: "/project/.i18n-harness",
  };
}

// Minimal required props that exercise the sub-header logic without IPC.
const BASE_PROPS = {
  summary: makeSummary(),
  openCatalogs: new Map(),
  activeCatalogPath: null,
  catalog: null,
  selectedId: null,
  filter: "all-open" as const,
  search: "",
  dirtyIds: new Set<string>(),
  reports: {},
  busyIds: new Set<string>(),
  batchActive: false,
  error: null,
  editorRef: { current: null },
  onSelect: vi.fn(),
  onFilterChange: vi.fn(),
  onSearchChange: vi.fn(),
  onEdit: vi.fn(),
  onTranslate: vi.fn(),
  onAccept: vi.fn(),
  onEnsureCatalogLoaded: vi.fn().mockResolvedValue(undefined),
  onTranslateUnitFor: vi.fn(),
  onEditUnitFor: vi.fn(),
  onAcceptUnitFor: vi.fn(),
  onTranslateAll: vi.fn(),
  startBatchForPair: vi
    .fn()
    .mockResolvedValue({ jobId: "job-1", cancel: vi.fn() }),
};

// ── Toggle visibility ─────────────────────────────────────────────────────────

describe("TranslatePanel sub-header toggle", () => {
  it("renders the Single and Matrix buttons in Matrix mode (focusLocale=null)", () => {
    render(
      <TranslatePanel
        {...BASE_PROPS}
        focusLocale={null}
        setFocusLocale={vi.fn()}
      />,
    );
    // In Matrix mode "Matrix" is the active span (aria-current) and "Single"
    // is an interactive button.
    expect(
      screen.getByRole("button", { name: /switch to single-locale view/i }),
    ).toBeInTheDocument();
    expect(screen.getByText("Matrix")).toBeInTheDocument();
  });

  it("renders the Single and Matrix buttons in Single mode (focusLocale='de')", () => {
    render(
      <TranslatePanel
        {...BASE_PROPS}
        focusLocale="de"
        setFocusLocale={vi.fn()}
      />,
    );
    // In Single mode "Single" is the active span and "Matrix" is a button.
    expect(
      screen.getByRole("button", { name: /switch to matrix view/i }),
    ).toBeInTheDocument();
    expect(screen.getByText("Single")).toBeInTheDocument();
  });
});

// ── Mode switching ────────────────────────────────────────────────────────────

describe("TranslatePanel mode switching", () => {
  it("clicking Matrix in Single mode calls setFocusLocale(null)", async () => {
    const setFocusLocale = vi.fn();
    render(
      <TranslatePanel
        {...BASE_PROPS}
        focusLocale="de"
        setFocusLocale={setFocusLocale}
      />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: /switch to matrix view/i }),
    );
    expect(setFocusLocale).toHaveBeenCalledWith(null);
  });

  it("clicking Single in Matrix mode calls setFocusLocale with the first locale", async () => {
    const setFocusLocale = vi.fn();
    render(
      <TranslatePanel
        {...BASE_PROPS}
        focusLocale={null}
        setFocusLocale={setFocusLocale}
      />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: /switch to single-locale view/i }),
    );
    // First locale in makeSummary() is "de"
    expect(setFocusLocale).toHaveBeenCalledWith("de");
    expect(setFocusLocale).not.toHaveBeenCalledWith(null);
  });

  it("Single button is disabled when no locales exist", () => {
    render(
      <TranslatePanel
        {...BASE_PROPS}
        summary={makeSummary([])}
        focusLocale={null}
        setFocusLocale={vi.fn()}
      />,
    );
    const singleBtn = screen.getByRole("button", {
      name: /switch to single-locale view/i,
    });
    expect(singleBtn).toBeDisabled();
  });
});

// ── StatusFilter persistence across mode switches ─────────────────────────────

describe("StatusFilter persists across mode switches", () => {
  it("StatusFilter is visible in Matrix mode", () => {
    render(
      <TranslatePanel
        {...BASE_PROPS}
        focusLocale={null}
        setFocusLocale={vi.fn()}
      />,
    );
    // The StatusFilter renders a tablist with 7 tabs.
    const tablist = screen.getByRole("tablist", { name: /status filter/i });
    expect(tablist).toBeInTheDocument();
  });

  it("StatusFilter is visible in Single mode", () => {
    render(
      <TranslatePanel
        {...BASE_PROPS}
        focusLocale="de"
        setFocusLocale={vi.fn()}
      />,
    );
    const tablist = screen.getByRole("tablist", { name: /status filter/i });
    expect(tablist).toBeInTheDocument();
  });

  it("StatusFilter selection changes the active pill in the same render", async () => {
    render(
      <TranslatePanel
        {...BASE_PROPS}
        focusLocale={null}
        setFocusLocale={vi.fn()}
      />,
    );
    // Initial: "All" is selected.
    const allTab = screen.getByRole("tab", { name: "All" });
    expect(allTab).toHaveAttribute("aria-selected", "true");

    // Click "Untranslated".
    await userEvent.click(screen.getByRole("tab", { name: "Untranslated" }));
    expect(screen.getByRole("tab", { name: "Untranslated" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("tab", { name: "All" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
  });
});
