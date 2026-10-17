import { describe, expect, it } from "vitest";
import { buildConflictReportMarkdown } from "../reuse";
import type { ReviewQueueItem } from "../types";

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
    state: "untranslated",
    reviewer_note: null,
    ...partial,
  };
}

describe("buildConflictReportMarkdown", () => {
  it("includes only conflict items and renders each candidate's references", () => {
    const note = JSON.stringify([
      {
        reference: "/refs/expert_de.ts",
        also_from: ["/refs/team_de.ts"],
        is_plural: false,
        text: "Hallo",
      },
      {
        reference: "/refs/legacy_de.ts",
        also_from: [],
        is_plural: false,
        text: "Hi",
      },
    ]);
    const items = [
      item({
        unit_id: "ctx::Greeting",
        review_status: "conflict",
        source_preview: "Hello",
        reviewer_note: note,
      }),
      // A non-conflict needs-review item must be excluded.
      item({ unit_id: "ctx::Other", review_status: "needs-review" }),
    ];

    const md = buildConflictReportMarkdown(items, "ES Project");

    expect(md).toContain("# Reference-translation conflicts — ES Project");
    expect(md).toContain("1 conflict");
    expect(md).toContain("## `ctx::Greeting`");
    expect(md).toContain("- **Source:** Hello");
    expect(md).toContain("Hallo");
    expect(md).toContain("/refs/expert_de.ts, /refs/team_de.ts");
    expect(md).toContain("Hi");
    // The non-conflict item is not in the report.
    expect(md).not.toContain("ctx::Other");
  });

  it("labels plural candidate forms with CLDR categories", () => {
    const note = JSON.stringify([
      {
        reference: "/refs/expert_de.ts",
        also_from: [],
        is_plural: true,
        text: "eine Datei\u{1F}{n} Dateien",
      },
    ]);
    const md = buildConflictReportMarkdown(
      [item({ review_status: "conflict", reviewer_note: note })],
      "P",
    );
    // Forms are labelled by position in CLDR canonical order, matching the
    // ConflictDetail rendering in the Review queue.
    expect(md).toContain("- **Zero:** eine Datei");
    expect(md).toContain("- **One:** {n} Dateien");
  });

  it("reports zero conflicts cleanly when there are none", () => {
    const md = buildConflictReportMarkdown(
      [item({ review_status: "needs-review" })],
      "P",
    );
    expect(md).toContain("0 conflicts need manual resolution");
  });
});
