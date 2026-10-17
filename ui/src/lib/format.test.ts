import { describe, expect, it } from "vitest";

import { formatBytes, plural } from "./format";

describe("helpers", () => {
  it("pluralizes with grouping", () => {
    expect(plural(1, "string", "strings")).toBe("1 string");
    expect(plural(2556, "string", "strings")).toBe(`${(2556).toLocaleString()} strings`);
  });

  it("formats model sizes", () => {
    expect(formatBytes(9_700_000_000)).toBe("9.7 GB");
    expect(formatBytes(621_000_000)).toBe("621 MB");
    expect(formatBytes(null)).toBeNull();
  });
});
