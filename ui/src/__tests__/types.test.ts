// Smoke test: verify that the shared type utilities work correctly.
// These are pure-logic tests with no DOM or IPC dependencies.

import { describe, expect, it } from "vitest";
import type { Unit } from "../lib/types";
import { severityOf, unitRow } from "../lib/types";

describe("severityOf", () => {
  it("classifies placeholder-mismatch as hard", () => {
    expect(severityOf("placeholder-mismatch")).toBe("hard");
  });

  it("classifies length-warn as soft", () => {
    expect(severityOf("length-warn")).toBe("soft");
  });

  it("classifies low-confidence as semantic", () => {
    expect(severityOf("low-confidence")).toBe("semantic");
  });

  it("treats unknown flags as soft (conservative default)", () => {
    expect(severityOf("some-new-flag-we-do-not-know")).toBe("soft");
  });
});

describe("unitRow", () => {
  const baseUnit: Unit = {
    id: "MainWindow/Save",
    source: "Save",
    target: { kind: "singular", text: "Speichern" },
    placeholders: [],
    plural_arity: null,
    flags: [],
    provenance: { file: "src/mainwindow.cpp", line: 108, byte_offset: null },
    state: "finished",
    review_status: "reviewed",
    source_hash: null,
    confidence: null,
    flag_notes: null,
  };

  it("builds a row with correct source and state", () => {
    const row = unitRow(baseUnit);
    expect(row.id).toBe("MainWindow/Save");
    expect(row.source).toBe("Save");
    expect(row.state).toBe("finished");
    expect(row.flagCount).toBe(0);
    expect(row.needsReview).toBe(false);
  });

  it("marks needsReview when flags are present", () => {
    const unit = { ...baseUnit, flags: ["low-confidence"] };
    const row = unitRow(unit);
    expect(row.needsReview).toBe(true);
    expect(row.flagCount).toBe(1);
  });

  it("marks needsReview when review_status is needs-review", () => {
    const unit = { ...baseUnit, review_status: "needs-review" as const };
    const row = unitRow(unit);
    expect(row.needsReview).toBe(true);
  });

  it("handles plural units", () => {
    const plural: Unit = {
      ...baseUnit,
      id: "MainWindow/%n messages",
      source: "%n messages",
      target: { kind: "plural", forms: ["1 Nachricht", "Nachrichten"] },
      plural_arity: 2,
    };
    const row = unitRow(plural);
    expect(row.isPlural).toBe(true);
    expect(row.pluralTotal).toBe(2);
    expect(row.pluralFilled).toBe(2);
  });

  it("truncates long source text in preview", () => {
    const long = "A".repeat(100);
    const unit = { ...baseUnit, source: long };
    const row = unitRow(unit);
    expect(row.preview.length).toBeLessThanOrEqual(81); // 80 chars + ellipsis
  });
});
