// TranslatePanel — shared sub-header integration tests.
//
// Verifies that:
//   1. The Single ↔ Matrix toggle is visible in both modes.
//   2. Clicking "Matrix" while in Single mode calls setFocusLocale(null).
//   3. Clicking "Single" while in Matrix mode calls setFocusLocale with the
//      first available locale.
//   4. The StatusFilter multi-select pills are visible in both modes and
//      selections accumulate when pills are clicked.

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
    references: [],
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
  search: "",
  dirtyIds: new Set<string>(),
  reports: {},
  stats: null,
  busyIds: new Set<string>(),
  batchActive: false,
  error: null,
  editorRef: { current: null },
  onSelect: vi.fn(),
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

// ── FileFilter visibility + scoping ──────────────────────────────────────────

function summaryWithFiles(
  files: { path: string; locale: string }[],
): ProjectSummary {
  const catalogs: CatalogRef[] = files.map((f) => ({
    absolute_path: `/project/${f.path}`,
    manifest_path: f.path,
    format: "qt-ts",
    locale: f.locale,
    status: "ok",
  }));
  const locales = [...new Set(files.map((f) => f.locale))];
  return {
    root: "/project",
    name: "Test",
    schema: 1,
    locales,
    catalogs,
    references: [],
    glossary_path: null,
    backend: null,
    state_dir: "/project/.i18n-harness",
  };
}

describe("TranslatePanel file filter", () => {
  it("shows the file filter in Matrix mode when >1 catalog exists", () => {
    render(
      <TranslatePanel
        {...BASE_PROPS}
        focusLocale={null}
        setFocusLocale={vi.fn()}
      />,
    );
    const select = screen.getByRole("combobox", { name: /filter by file/i });
    const options = Array.from(select.querySelectorAll("option")).map(
      (o) => o.textContent,
    );
    expect(options).toEqual(["All files", "de.ts", "fr.ts"]);
  });

  it("hides the file filter in Single mode when the locale has one file", () => {
    render(
      <TranslatePanel
        {...BASE_PROPS}
        focusLocale="de"
        setFocusLocale={vi.fn()}
      />,
    );
    expect(
      screen.queryByRole("combobox", { name: /filter by file/i }),
    ).not.toBeInTheDocument();
  });

  it("shows the file filter in Single mode when the locale has multiple files", () => {
    const summary = summaryWithFiles([
      { path: "app_de.ts", locale: "de" },
      { path: "help_de.ts", locale: "de" },
      { path: "app_fr.ts", locale: "fr" },
    ]);
    render(
      <TranslatePanel
        {...BASE_PROPS}
        summary={summary}
        focusLocale="de"
        setFocusLocale={vi.fn()}
      />,
    );
    const select = screen.getByRole("combobox", { name: /filter by file/i });
    const options = Array.from(select.querySelectorAll("option")).map(
      (o) => o.textContent,
    );
    // Only the two de catalogs (plus the "All files" sentinel) are in scope.
    expect(options).toEqual(["All files", "app_de.ts", "help_de.ts"]);
  });
});

// ── StatusFilter visibility + multi-select behavior across modes ─────────────

describe("StatusFilter persists across mode switches", () => {
  it("StatusFilter renders exactly 3 pill buttons in Matrix mode", () => {
    render(
      <TranslatePanel
        {...BASE_PROPS}
        focusLocale={null}
        setFocusLocale={vi.fn()}
      />,
    );
    const toolbar = screen.getByRole("toolbar", { name: /status filter/i });
    expect(toolbar).toBeInTheDocument();
    const labels = Array.from(toolbar.querySelectorAll("button")).map(
      (b) => b.textContent,
    );
    expect(labels).toEqual(["Untranslated", "Proposed", "Finished"]);
  });

  it("StatusFilter renders exactly 3 pill buttons in Single mode", () => {
    render(
      <TranslatePanel
        {...BASE_PROPS}
        focusLocale="de"
        setFocusLocale={vi.fn()}
      />,
    );
    const toolbar = screen.getByRole("toolbar", { name: /status filter/i });
    expect(toolbar).toBeInTheDocument();
    const labels = Array.from(toolbar.querySelectorAll("button")).map(
      (b) => b.textContent,
    );
    expect(labels).toEqual(["Untranslated", "Proposed", "Finished"]);
  });

  it("clicking a pill toggles its aria-pressed state", async () => {
    render(
      <TranslatePanel
        {...BASE_PROPS}
        focusLocale={null}
        setFocusLocale={vi.fn()}
      />,
    );
    const toolbar = screen.getByRole("toolbar", { name: /status filter/i });
    const buttons = Array.from(toolbar.querySelectorAll("button"));
    const byLabel = (label: string) =>
      buttons.find((b) => b.textContent === label) as HTMLButtonElement;

    expect(byLabel("Proposed")).toHaveAttribute("aria-pressed", "false");
    await userEvent.click(byLabel("Proposed"));
    expect(byLabel("Proposed")).toHaveAttribute("aria-pressed", "true");
    // Other pills remain unpressed (multi-select, but only one was clicked).
    expect(byLabel("Untranslated")).toHaveAttribute("aria-pressed", "false");
    expect(byLabel("Finished")).toHaveAttribute("aria-pressed", "false");
  });
});
