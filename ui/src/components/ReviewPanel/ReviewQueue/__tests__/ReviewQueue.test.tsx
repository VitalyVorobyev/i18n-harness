import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type {
  ReviewQueueItem,
  ReviewQueueResponse,
} from "../../../../lib/types";
import { ReviewQueue } from "../ReviewQueue";

function item(partial: Partial<ReviewQueueItem>): ReviewQueueItem {
  return {
    catalog_path: "/p/de.ts",
    catalog_manifest_path: "de.ts",
    locale: "de_DE",
    unit_id: "ctx::Key",
    source_preview: "Source",
    target_preview: "",
    flags: [],
    review_status: null,
    state: "proposed",
    reviewer_note: null,
    ...partial,
  };
}

function queue(items: ReviewQueueItem[]): ReviewQueueResponse {
  return {
    total_count: items.length,
    by_catalog: {},
    stats_by_catalog: {},
    items,
  };
}

describe("ReviewQueue conflicts-only filter", () => {
  const items = [
    item({ unit_id: "ctx::A", review_status: "needs-review" }),
    item({ unit_id: "ctx::B", review_status: "conflict" }),
    item({ unit_id: "ctx::C", state: "proposed" }),
  ];

  it("toggling the conflicts chip narrows the queue to conflicts", async () => {
    render(<ReviewQueue reviewQueue={queue(items)} onOpenItem={vi.fn()} />);

    // All three rows visible initially.
    expect(screen.getByText("ctx::A")).toBeInTheDocument();
    expect(screen.getByText("ctx::B")).toBeInTheDocument();
    expect(screen.getByText("ctx::C")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /1 conflict/i }));

    // Only the conflict row remains.
    expect(screen.queryByText("ctx::A")).not.toBeInTheDocument();
    expect(screen.getByText("ctx::B")).toBeInTheDocument();
    expect(screen.queryByText("ctx::C")).not.toBeInTheDocument();
  });

  it("renders the export button only when an export handler is provided", () => {
    const { rerender } = render(
      <ReviewQueue reviewQueue={queue(items)} onOpenItem={vi.fn()} />,
    );
    expect(
      screen.queryByRole("button", { name: /export conflict report/i }),
    ).not.toBeInTheDocument();

    rerender(
      <ReviewQueue
        reviewQueue={queue(items)}
        onOpenItem={vi.fn()}
        onExportConflicts={vi.fn()}
      />,
    );
    expect(
      screen.getByRole("button", { name: /export conflict report/i }),
    ).toBeInTheDocument();
  });

  it("pre-applies the conflicts-only filter when focusConflictsNonce fires", () => {
    render(
      <ReviewQueue
        reviewQueue={queue(items)}
        onOpenItem={vi.fn()}
        focusConflictsNonce={1}
      />,
    );
    // Deep-link arrived → only the conflict row is shown.
    expect(screen.queryByText("ctx::A")).not.toBeInTheDocument();
    expect(screen.getByText("ctx::B")).toBeInTheDocument();
  });
});
