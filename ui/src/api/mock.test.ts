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
      api.startTranslation({ catalogs: [QT], statuses: [], ids: null }, "auto"),
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
    const total = await api.startTranslation({ catalogs: [QT], statuses: [], ids: null }, "auto");
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
