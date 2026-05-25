// GlossaryPanel — AI-assist sparkle button tests.
//
// Covers the per-locale AI-assist button added to TranslationRow:
//   - Disabled when source term is empty
//   - Disabled when DNT flag is true
//   - Clicking sparkle calls translateGlossaryTerm and populates the field
//   - During the call the button shows a spinner (animate-spin)
//   - On error, the field is not modified and the button re-enables

import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { GlossaryPanel } from "../GlossaryPanel";

// ── Tauri module mock ─────────────────────────────────────────────────────────
//
// GlossaryPanel imports several tauri functions. We replace the whole module
// so the tests do not need a real Tauri runtime, then restore selected mocks
// to sensible defaults per-test via beforeEach.

vi.mock("../../../lib/tauri", () => ({
  listLocales: vi.fn(),
  loadGlossary: vi.fn(),
  saveGlossary: vi.fn(),
  pickGlossaryFile: vi.fn(),
  pickGlossarySaveLocation: vi.fn(),
  translateGlossaryTerm: vi.fn(),
}));

import {
  listLocales,
  loadGlossary,
  translateGlossaryTerm,
} from "../../../lib/tauri";

const mockListLocales = vi.mocked(listLocales);
const mockLoadGlossary = vi.mocked(loadGlossary);
const mockTranslateGlossaryTerm = vi.mocked(translateGlossaryTerm);

// ── Fixture factories ─────────────────────────────────────────────────────────

function makeTerm(overrides: {
  source?: string;
  do_not_translate?: boolean;
  translations?: Record<string, string>;
}) {
  return {
    source: overrides.source ?? "Settings",
    do_not_translate: overrides.do_not_translate ?? false,
    notes: null,
    translations: overrides.translations ?? {},
  };
}

function makeGlossaryPayload(
  terms: ReturnType<typeof makeTerm>[] = [makeTerm({})],
) {
  return {
    schema_version: 1,
    terms,
    locale_overrides: [],
  };
}

// ── Shared base props ─────────────────────────────────────────────────────────

const PROJECT_PATH = "/tmp/test-project";

function baseProps() {
  return {
    flashError: vi.fn(),
    flashInfo: vi.fn(),
    projectPath: PROJECT_PATH,
  };
}

// ── beforeEach: configure default mock behaviour ──────────────────────────────

beforeEach(() => {
  vi.clearAllMocks();

  // listLocales resolves with a single German locale by default.
  mockListLocales.mockResolvedValue([
    { id: "de_DE", register: "formal", script: "Latn", plural_arity: 2 },
  ]);

  // loadGlossary resolves with a glossary containing the "Settings" term.
  mockLoadGlossary.mockResolvedValue({
    path: `${PROJECT_PATH}/glossary.toml`,
    payload: makeGlossaryPayload(),
    warnings: [],
  });
});

// ── Helper: render with auto-loaded glossary ──────────────────────────────────

async function renderPanel(
  overrides: Partial<ReturnType<typeof baseProps>> = {},
) {
  const props = { ...baseProps(), ...overrides };
  // initialPath triggers the auto-load effect.
  render(
    <GlossaryPanel {...props} initialPath={`${PROJECT_PATH}/glossary.toml`} />,
  );
  // Wait for the glossary and locales to load.
  await waitFor(() =>
    expect(
      screen.getByLabelText("Propose AI translation for de_DE"),
    ).toBeInTheDocument(),
  );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("GlossaryPanel — AI-assist sparkle button", () => {
  // ── Disabled: source empty ──────────────────────────────────────────────────

  it("does not render sparkle button when source term is empty", async () => {
    mockLoadGlossary.mockResolvedValueOnce({
      path: `${PROJECT_PATH}/glossary.toml`,
      payload: makeGlossaryPayload([makeTerm({ source: "" })]),
      warnings: [],
    });

    render(
      <GlossaryPanel
        {...baseProps()}
        initialPath={`${PROJECT_PATH}/glossary.toml`}
      />,
    );

    // Wait for locale data to arrive (listLocales is still called).
    await waitFor(() => expect(mockListLocales).toHaveBeenCalled());
    // Give React a tick to apply all effect results.
    await waitFor(() => expect(mockLoadGlossary).toHaveBeenCalled());

    // The button must not be present because source is empty.
    expect(
      screen.queryByLabelText(/propose ai translation/i),
    ).not.toBeInTheDocument();
  });

  // ── Disabled: DNT flag ──────────────────────────────────────────────────────

  it("does not render sparkle button when do_not_translate is true", async () => {
    mockLoadGlossary.mockResolvedValueOnce({
      path: `${PROJECT_PATH}/glossary.toml`,
      payload: makeGlossaryPayload([makeTerm({ do_not_translate: true })]),
      warnings: [],
    });

    render(
      <GlossaryPanel
        {...baseProps()}
        initialPath={`${PROJECT_PATH}/glossary.toml`}
      />,
    );

    await waitFor(() => expect(mockLoadGlossary).toHaveBeenCalled());

    expect(
      screen.queryByLabelText(/propose ai translation/i),
    ).not.toBeInTheDocument();
  });

  // ── Not rendered without projectPath ───────────────────────────────────────

  it("does not render sparkle button when projectPath is absent", async () => {
    render(
      <GlossaryPanel
        flashError={vi.fn()}
        flashInfo={vi.fn()}
        // projectPath intentionally omitted
        initialPath={`${PROJECT_PATH}/glossary.toml`}
      />,
    );

    await waitFor(() => expect(mockLoadGlossary).toHaveBeenCalled());

    expect(
      screen.queryByLabelText(/propose ai translation/i),
    ).not.toBeInTheDocument();
  });

  // ── Happy path: click → populate field ─────────────────────────────────────

  it("clicking sparkle calls translateGlossaryTerm and populates the locale field", async () => {
    mockTranslateGlossaryTerm.mockResolvedValueOnce("Einstellungen");
    await renderPanel();

    const btn = screen.getByLabelText("Propose AI translation for de_DE");
    await userEvent.click(btn);

    // The shim must have been called with correct args.
    expect(mockTranslateGlossaryTerm).toHaveBeenCalledOnce();
    expect(mockTranslateGlossaryTerm).toHaveBeenCalledWith(
      PROJECT_PATH,
      "Settings",
      "de_DE",
    );

    // The input field for de_DE must now contain the proposed value.
    // findByDisplayValue waits until the input shows the translated text.
    const populated = await screen.findByDisplayValue("Einstellungen");
    expect(populated).toBeInTheDocument();
  });

  // ── Spinner during request ──────────────────────────────────────────────────

  it("shows a spinner in the button while the request is in flight", async () => {
    // Use a promise we control so we can inspect state mid-flight.
    let resolveTranslate!: (v: string) => void;
    const pendingTranslate = new Promise<string>((res) => {
      resolveTranslate = res;
    });
    mockTranslateGlossaryTerm.mockReturnValueOnce(pendingTranslate);

    await renderPanel();

    const btn = screen.getByLabelText("Propose AI translation for de_DE");
    await userEvent.click(btn);

    // While the promise is pending, the button label changes and a spinner appears.
    await waitFor(() => {
      expect(
        screen.getByLabelText("Requesting AI translation for de_DE…"),
      ).toBeInTheDocument();
    });

    // The button should be disabled during the request.
    const spinnerBtn = screen.getByLabelText(
      "Requesting AI translation for de_DE…",
    );
    expect(spinnerBtn).toBeDisabled();

    // The spinner SVG must be present inside the button.
    const spinner = spinnerBtn.querySelector(".animate-spin");
    expect(spinner).toBeInTheDocument();

    // Resolve the promise and verify the button returns to normal.
    resolveTranslate("Einstellungen");
    await waitFor(() =>
      expect(
        screen.getByLabelText("Propose AI translation for de_DE"),
      ).toBeInTheDocument(),
    );
  });

  // ── Error path: field unchanged, button re-enables ─────────────────────────

  it("on error the locale field is not modified and the button re-enables", async () => {
    mockTranslateGlossaryTerm.mockRejectedValueOnce(
      new Error("backend unavailable"),
    );
    const flashError = vi.fn();

    await renderPanel({ flashError });

    const btn = screen.getByLabelText("Propose AI translation for de_DE");

    // Capture the current placeholder to verify the field stays empty.
    const input = screen.getByPlaceholderText(/translate "Settings" to de_DE/i);
    expect((input as HTMLInputElement).value).toBe("");

    await userEvent.click(btn);

    // After the error, the button must be re-enabled.
    await waitFor(() =>
      expect(
        screen.getByLabelText("Propose AI translation for de_DE"),
      ).toBeEnabled(),
    );

    // The field must still be empty — not modified by the failed request.
    expect((input as HTMLInputElement).value).toBe("");

    // flashError must have been called with a message containing the error text.
    expect(flashError).toHaveBeenCalledOnce();
    const [firstArg] = flashError.mock.calls[0] as [string];
    expect(firstArg).toMatch(/backend unavailable/);
  });
});
