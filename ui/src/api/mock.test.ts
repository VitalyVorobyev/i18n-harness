import { describe, expect, it } from "vitest";

import type { TranslationEvent } from "./types";
import { createMockApi } from "./mock";

const QT = "/demo/translations/App_sk_SK.ts";
const filter = { statuses: [], errors_only: false, search: null };

// The mock stands in for the session in dev mode and e2e, so it must follow
// the same rules the UI relies on.
describe("mock backend", () => {
  it("treats untouched pivot text as input, not as a translation", async () => {
    const api = createMockApi();
    await api.openFolder("/demo");
    const page = await api.listUnits(QT, filter, 0, 100);
    const unit = page.units.find((u) => u.source === "Filter Size");
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
    const unit = (await api.listUnits(QT, filter, 0, 100)).units.find((u) =>
      u.source.includes("%1"),
    );
    const edited = await api.setTranslation(QT, unit?.id ?? "", ["bez zástupného symbolu"], false);
    expect(edited.issues[0]?.severity).toBe("error");
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
