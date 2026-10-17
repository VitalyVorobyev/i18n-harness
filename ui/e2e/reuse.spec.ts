// reuse.spec.ts — reference reuse / remainder / merge UI surfaces.
//
// Drives the three surfaces against the mock IPC: the Settings reference-files
// section, the per-catalog actions menu in the sidebar, and the conflict view
// scaffolding in the review queue.

import { expect, openSampleProject, test } from "./fixture";

test.describe("Reference reuse surfaces", () => {
  test("Settings shows a Reference files section with an add control", async ({
    page,
  }) => {
    await page.goto("/");
    await openSampleProject(page);

    await page
      .getByRole("tablist", { name: /project view/i })
      .getByRole("tab", { name: /settings/i })
      .click();

    const heading = page.getByRole("heading", { name: /reference files/i });
    await expect(heading).toBeVisible({ timeout: 3000 });

    const addBtn = page.getByRole("button", {
      name: /add reference files/i,
    });
    await expect(addBtn).toBeVisible();
  });

  test("the per-catalog actions menu opens and lists reuse actions", async ({
    page,
  }) => {
    await page.goto("/");
    await openSampleProject(page);

    // The Overview view renders the project sidebar with per-catalog rows.
    const actionsTrigger = page
      .getByRole("button", { name: /reuse actions for/i })
      .first();

    // The trigger is hover-revealed; force the click so the opacity transition
    // does not block the interaction in a headless run.
    await actionsTrigger.click({ force: true });

    const menu = page.getByRole("menu", { name: /reuse actions for/i });
    await expect(menu).toBeVisible({ timeout: 2000 });

    await expect(
      menu.getByRole("menuitem", { name: /apply references/i }),
    ).toBeVisible();
    await expect(
      menu.getByRole("menuitem", { name: /export remainder/i }),
    ).toBeVisible();
    await expect(
      menu.getByRole("menuitem", { name: /merge translated/i }),
    ).toBeVisible();
  });

  test("the actions menu closes on Escape", async ({ page }) => {
    await page.goto("/");
    await openSampleProject(page);

    const actionsTrigger = page
      .getByRole("button", { name: /reuse actions for/i })
      .first();
    await actionsTrigger.click({ force: true });

    const menu = page.getByRole("menu", { name: /reuse actions for/i });
    await expect(menu).toBeVisible();

    await page.keyboard.press("Escape");
    await expect(menu).toBeHidden();
  });

  test("Apply references surfaces a conflict that the Review view can resolve", async ({
    page,
  }) => {
    await page.goto("/");
    await openSampleProject(page);

    // Open the first catalog so its units are cached (the mock reuse mutates
    // the cached catalog).
    await page
      .getByRole("button", { name: /Reuse actions for/i })
      .first()
      .click({ force: true });
    await page.getByRole("menuitem", { name: /apply references/i }).click();

    // The reuse ran — assert on the durable progress reflected in the sidebar
    // (one unit copied to finished) rather than the transient result toast.
    await expect(
      page.getByRole("img", { name: /1 finished/i }).first(),
    ).toBeVisible({ timeout: 4000 });

    // Go to Review → Queue; the conflict row should be present and expandable.
    await page
      .getByRole("tablist", { name: /project view/i })
      .getByRole("tab", { name: /^review$/i })
      .click();
    await page.waitForTimeout(300);

    const expandBtn = page
      .getByRole("button", { name: /show conflict candidates/i })
      .first();
    await expect(expandBtn).toBeVisible({ timeout: 3000 });
    await expandBtn.click();

    // Two candidates and a "Use this" affordance render.
    const useButtons = page.getByRole("button", {
      name: /use this candidate/i,
    });
    await expect(useButtons.first()).toBeVisible();
    expect(await useButtons.count()).toBeGreaterThanOrEqual(2);

    // Picking a candidate applies it via the edit path (toast confirms, then
    // auto-dismisses — assert it appears within its lifetime).
    await useButtons.first().click();
    await expect(page.getByText(/Applied candidate/i)).toBeVisible({
      timeout: 3000,
    });
  });
});
