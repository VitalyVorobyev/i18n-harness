import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ProjectTopBar } from "../TopBar";

// Minimal required props that satisfy the interface without exercising
// locale-filter or theme logic.
const baseProps = {
  projectName: "Test Project",
  locales: ["de", "fr"],
  activeLocaleFilter: new Set<string>(),
  onLocaleFilterChange: vi.fn(),
  view: "translate" as const,
  onViewChange: vi.fn(),
  theme: "light" as const,
  onToggleTheme: vi.fn(),
  onCloseProject: vi.fn(),
};

describe("ProjectTopBar — Save button", () => {
  it("renders a Save button", () => {
    render(
      <ProjectTopBar
        {...baseProps}
        unsavedCount={0}
        saving={false}
        onSave={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: /save/i })).toBeInTheDocument();
  });

  it("is disabled when unsavedCount is 0 and not saving", () => {
    render(
      <ProjectTopBar
        {...baseProps}
        unsavedCount={0}
        saving={false}
        onSave={vi.fn()}
      />,
    );
    const btn = screen.getByRole("button", { name: /no unsaved changes/i });
    expect(btn).toBeDisabled();
  });

  it("is enabled when unsavedCount > 0", () => {
    render(
      <ProjectTopBar
        {...baseProps}
        unsavedCount={3}
        saving={false}
        onSave={vi.fn()}
      />,
    );
    const btn = screen.getByRole("button", { name: /save 3 unsaved/i });
    expect(btn).toBeEnabled();
  });

  it("shows the unsaved count pill when unsavedCount > 0", () => {
    render(
      <ProjectTopBar
        {...baseProps}
        unsavedCount={2}
        saving={false}
        onSave={vi.fn()}
      />,
    );
    // The pill text "2" should appear inside the button
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  it("does not show count pill when unsavedCount is 0", () => {
    const { container } = render(
      <ProjectTopBar
        {...baseProps}
        unsavedCount={0}
        saving={false}
        onSave={vi.fn()}
      />,
    );
    // There should be no tabular-nums pill element
    const pills = container.querySelectorAll(".tabular-nums");
    expect(pills.length).toBe(0);
  });

  it("shows a spinner (animate-spin svg) when saving is true", () => {
    const { container } = render(
      <ProjectTopBar
        {...baseProps}
        unsavedCount={1}
        saving={true}
        onSave={vi.fn()}
      />,
    );
    const spinner = container.querySelector(".animate-spin");
    expect(spinner).toBeInTheDocument();
  });

  it("does not show a spinner when saving is false", () => {
    const { container } = render(
      <ProjectTopBar
        {...baseProps}
        unsavedCount={0}
        saving={false}
        onSave={vi.fn()}
      />,
    );
    const spinner = container.querySelector(".animate-spin");
    expect(spinner).not.toBeInTheDocument();
  });

  it("calls onSave when clicked", async () => {
    const onSave = vi.fn();
    render(
      <ProjectTopBar
        {...baseProps}
        unsavedCount={1}
        saving={false}
        onSave={onSave}
      />,
    );
    const btn = screen.getByRole("button", { name: /save 1 unsaved/i });
    await userEvent.click(btn);
    expect(onSave).toHaveBeenCalledOnce();
  });

  it("sets title to 'No unsaved changes' when clean", () => {
    render(
      <ProjectTopBar
        {...baseProps}
        unsavedCount={0}
        saving={false}
        onSave={vi.fn()}
      />,
    );
    const btn = screen.getByRole("button", { name: /no unsaved changes/i });
    expect(btn).toHaveAttribute("title", "No unsaved changes");
  });

  it("sets title with catalog count and shortcut when dirty", () => {
    render(
      <ProjectTopBar
        {...baseProps}
        unsavedCount={2}
        saving={false}
        onSave={vi.fn()}
      />,
    );
    const btn = screen.getByRole("button", { name: /save 2 unsaved/i });
    expect(btn).toHaveAttribute("title", "Save 2 catalog(s) — ⌘S");
  });
});
