import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ProgressBar } from "../ProgressBar";

describe("ProgressBar", () => {
  it("renders with correct aria-valuenow and aria-valuemax", () => {
    render(<ProgressBar value={30} max={100} />);
    const bar = screen.getByRole("progressbar");
    expect(bar).toHaveAttribute("aria-valuenow", "30");
    expect(bar).toHaveAttribute("aria-valuemax", "100");
    expect(bar).toHaveAttribute("aria-valuemin", "0");
  });

  it("fill width matches value/max * 100%", () => {
    render(<ProgressBar value={50} max={200} />);
    // The fill div is the first child of the progressbar element.
    const bar = screen.getByRole("progressbar");
    const fill = bar.firstElementChild as HTMLElement;
    expect(fill.style.width).toBe("25%");
  });

  it("fill is 0 when value is 0", () => {
    render(<ProgressBar value={0} max={100} />);
    const bar = screen.getByRole("progressbar");
    const fill = bar.firstElementChild as HTMLElement;
    expect(fill.style.width).toBe("0%");
  });

  it("fill is 100% when value equals max", () => {
    render(<ProgressBar value={100} max={100} />);
    const bar = screen.getByRole("progressbar");
    const fill = bar.firstElementChild as HTMLElement;
    expect(fill.style.width).toBe("100%");
  });

  it("clamps value below 0 to 0", () => {
    render(<ProgressBar value={-10} max={100} />);
    const bar = screen.getByRole("progressbar");
    expect(bar).toHaveAttribute("aria-valuenow", "0");
    const fill = bar.firstElementChild as HTMLElement;
    expect(fill.style.width).toBe("0%");
  });

  it("clamps value above max to max", () => {
    render(<ProgressBar value={150} max={100} />);
    const bar = screen.getByRole("progressbar");
    expect(bar).toHaveAttribute("aria-valuenow", "100");
    const fill = bar.firstElementChild as HTMLElement;
    expect(fill.style.width).toBe("100%");
  });

  it("variant=block applies a taller height class than inline", () => {
    const { container: blockContainer } = render(
      <ProgressBar value={50} max={100} variant="block" />,
    );
    const { container: inlineContainer } = render(
      <ProgressBar value={50} max={100} variant="inline" />,
    );
    const blockBar = blockContainer.querySelector("[role=progressbar]");
    const inlineBar = inlineContainer.querySelector("[role=progressbar]");

    // block uses h-2.5, inline uses h-1.5 — class strings differ.
    expect(blockBar?.className).toContain("h-2.5");
    expect(inlineBar?.className).toContain("h-1.5");
    expect(blockBar?.className).not.toContain("h-1.5");
  });

  it("renders aria-label when provided", () => {
    render(<ProgressBar value={10} max={50} label="Upload progress" />);
    const bar = screen.getByRole("progressbar");
    expect(bar).toHaveAttribute("aria-label", "Upload progress");
  });

  it("does not render percent text by default", () => {
    render(<ProgressBar value={75} max={100} />);
    expect(screen.queryByText(/\d+%/)).toBeNull();
  });

  it("renders percent text when showPercent is true", () => {
    render(<ProgressBar value={75} max={100} showPercent />);
    expect(screen.getByText("75%")).toBeTruthy();
  });

  it("shows rounded percent", () => {
    render(<ProgressBar value={1} max={3} showPercent />);
    // 33.33… rounds to 33
    expect(screen.getByText("33%")).toBeTruthy();
  });
});
