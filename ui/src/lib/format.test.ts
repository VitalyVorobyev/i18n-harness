import { describe, expect, it } from "vitest";

import { countsLine, doneFraction, formatBytes, plural } from "./format";

const counts = (to_do: number, check: number, done: number, errors = 0) => ({
  total: to_do + check + done,
  to_do,
  check,
  done,
  errors,
});

describe("countsLine", () => {
  it("lists only what is not zero", () => {
    expect(countsLine(counts(312, 20, 5))).toBe("312 to do · 20 to check");
    expect(countsLine(counts(0, 3, 5, 1))).toBe("3 to check · 1 problem");
  });

  it("says when everything is done", () => {
    expect(countsLine(counts(0, 0, 7))).toBe("All done");
    expect(countsLine(counts(0, 0, 0))).toBe("No strings");
  });
});

describe("helpers", () => {
  it("pluralizes with grouping", () => {
    expect(plural(1, "string", "strings")).toBe("1 string");
    expect(plural(2556, "string", "strings")).toBe(`${(2556).toLocaleString()} strings`);
  });

  it("treats an empty catalog as complete", () => {
    expect(doneFraction(counts(0, 0, 0))).toBe(1);
    expect(doneFraction(counts(1, 0, 3))).toBe(0.75);
  });

  it("formats model sizes", () => {
    expect(formatBytes(9_700_000_000)).toBe("9.7 GB");
    expect(formatBytes(621_000_000)).toBe("621 MB");
    expect(formatBytes(null)).toBeNull();
  });
});
