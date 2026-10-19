import { beforeEach, describe, expect, it } from "vitest";

import { hideSample, sampleHidden } from "./sample";

describe("sample offer", () => {
  beforeEach(() => localStorage.clear());

  it("is shown until the viewer hides it, and stays hidden", () => {
    expect(sampleHidden()).toBe(false);
    hideSample();
    expect(sampleHidden()).toBe(true);
    expect(sampleHidden()).toBe(true);
  });
});
