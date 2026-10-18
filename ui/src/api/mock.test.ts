import { describe, expect, it } from "vitest";

import type { TranslationEvent } from "./types";
import { createMockApi } from "./mock";

const QT = "/demo/translations/App_sk_SK.ts";
const filter = { statuses: [], show: "everything" as const, term: null, search: null };

// The mock stands in for the session in dev mode and e2e, so it must follow
// the same rules the UI relies on.
describe("mock backend", () => {
  it("treats untouched pivot text as input, not as a translation", async () => {
    const api = createMockApi();
    await api.openFolder("/demo");
    const row = (await api.listRows(QT, filter)).find((r) => r.source === "Filter Size");
    const unit = await api.getUnit(QT, row?.id ?? "");
    expect(unit?.existing).toEqual(["Filtergröße"]);
    expect(unit?.target).toEqual([""]);
    expect(unit?.status).toBe("to-do");
  });

  it("refuses to translate without a model", async () => {
    const api = createMockApi();
    await api.openFolder("/demo");
    await expect(
      api.startTranslation(
        { catalogs: [QT], statuses: [], ids: null, glossary_issues: false },
        "auto",
      ),
    ).rejects.toMatchObject({ kind: "needs-model" });
  });

  it("streams progress and leaves results to check", async () => {
    const api = createMockApi();
    await api.openFolder("/demo");
    const settings = await api.getModelSettings();
    await api.setModelSettings({ ...settings, model: "milmmt-46-4b:q8" });
    const events: TranslationEvent[] = [];
    const finished = new Promise<void>((resolve) => {
      void api.onTranslation((e) => {
        events.push(e);
        if (e.progress.event === "finished") resolve();
      });
    });
    const total = await api.startTranslation(
      { catalogs: [QT], statuses: [], ids: null, glossary_issues: false },
      "auto",
    );
    await finished;
    expect(events[0]?.progress).toEqual({ event: "started", total });
    const [summary] = (await api.listCatalogs()).filter((c) => c.path === QT);
    expect(summary?.counts.check).toBe(total);
    expect(summary?.dirty).toBe(true);
  });

  it("flags a translation that loses a placeholder", async () => {
    const api = createMockApi();
    await api.openFolder("/demo");
    const unit = (await api.listRows(QT, filter)).find((r) => r.source.includes("%1"));
    const edited = await api.setTranslation(QT, unit?.id ?? "", ["bez zástupného symbolu"], false);
    expect(edited.issues[0]?.severity).toBe("error");
  });
});

describe("strings across files", () => {
  it("lists every file's strings and marks shared and inconsistent ones", async () => {
    const api = createMockApi();
    await api.openFolder("/demo");
    const all = await api.listRows(null, filter);
    expect(new Set(all.map((r) => r.file)).size).toBe(4);
    const inconsistent = await api.listRows(null, { ...filter, show: "inconsistent" });
    expect(inconsistent.map((r) => [r.file, r.target])).toEqual([
      ["Viewer_sk_SK.ts", "Priblížiť"],
      ["Report_sk_SK.ts", "Zväčšiť"],
    ]);
    const viewer = (await api.listCatalogs()).find((c) => c.name === "Viewer_sk_SK.ts");
    expect(viewer?.counts.inconsistent).toBe(1);
    // Filter Size is shared with App_sk_SK.ts; Zoom in with Report_sk_SK.ts.
    expect(viewer?.counts.only_here).toBe(2);
  });

  it("warns about long translations as the check settings say", async () => {
    const api = createMockApi();
    await api.openFolder("/demo");
    const viewer = "/demo/translations/Viewer_sk_SK.ts";
    const warned = () => api.listRows(viewer, { ...filter, show: "warnings" });
    expect((await warned()).map((r) => r.source)).toEqual(["Fit the image to the window"]);
    await api.setCheckSettings("sk_SK", { length_ratio: 3, length_min_chars: 8 });
    expect(await warned()).toEqual([]);
  });
});

describe("glossary added later", () => {
  it("counts the translations that break it and applies it to them", async () => {
    const api = createMockApi({ delayMs: 0 });
    await api.openFolder("/demo");
    const settings = await api.getModelSettings();
    await api.setModelSettings({ ...settings, model: "milmmt-46-4b:q8" });
    const run = async (glossary_issues: boolean) => {
      const finished = new Promise<void>((resolve) => {
        void api.onTranslation((e) => {
          if (e.progress.event === "finished") resolve();
        });
      });
      const total = await api.startTranslation(
        { catalogs: [], statuses: [], ids: null, glossary_issues },
        "source",
      );
      await finished;
      return total;
    };
    await run(false);
    const scope = { catalogs: [], statuses: [], ids: null, glossary_issues: true };
    expect(await api.planSize(scope)).toBe(0);

    const terms = await api.readGlossaryTable("/demo/terms.csv");
    await api.setGlossary("sk_SK", [
      ...(await api.getGlossary("sk_SK")),
      { term: "Filter Size", keep: false, translation: "Veľkosť filtra", note: null },
      ...terms,
    ]);
    expect(await api.planSize(scope)).toBe(1);
    expect(await run(true)).toBe(1);
    expect(await api.planSize(scope)).toBe(0);
  });
});
