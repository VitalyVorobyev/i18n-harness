import { describe, expect, it } from "vitest";

import type { CatalogSummary } from "../api";
import { groupFiles, shortName } from "./files";

const file = (relative_path: string, extra: Partial<CatalogSummary> = {}): CatalogSummary => ({
  path: `/demo/${relative_path}`,
  name: relative_path.split("/").at(-1) ?? relative_path,
  relative_path,
  format: "qt-ts",
  format_name: "Qt Linguist",
  locale: "sk_SK",
  language: "Slovak",
  pivot: "de_DE",
  pivot_language: "German",
  counts: {
    total: 1,
    to_do: 1,
    check: 0,
    done: 0,
    errors: 0,
    warnings: 0,
    inconsistent: 0,
    only_here: 1,
  },
  dirty: false,
  ...extra,
});

describe("groupFiles", () => {
  it("says the shared language, existing-text language and format once", () => {
    const groups = groupFiles([file("Acf_sk_SK.ts"), file("Rtv_sk_SK.ts")]);
    expect(groups.map((g) => [g.title, g.files.length])).toEqual([
      ["Slovak · from German · Qt Linguist", 2],
    ]);
  });

  it("separates formats, existing-text languages and folders", () => {
    const groups = groupFiles([
      file("a/App_sk_SK.ts"),
      file("a/Viewer_sk_SK.ts", { pivot: null, pivot_language: null }),
      file("po/messages.po", {
        format: "gettext-po",
        format_name: "gettext PO",
        pivot: null,
        pivot_language: null,
      }),
    ]);
    expect(groups.map((g) => g.title)).toEqual([
      "Slovak · from German · Qt Linguist · a",
      "Slovak · Qt Linguist · a",
      "Slovak · gettext PO · po",
    ]);
  });
});

describe("shortName", () => {
  it("drops the extension and a trailing locale", () => {
    expect(shortName(file("AcfSln_sk_SK.ts"))).toBe("AcfSln");
    expect(shortName(file("app-sk.ts"))).toBe("app");
    expect(shortName(file("messages.po"))).toBe("messages");
  });

  it("keeps a name that is only the locale", () => {
    expect(shortName(file("sk_SK.ts"))).toBe("sk_SK");
  });
});
