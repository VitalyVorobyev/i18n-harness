// matrix-textarea-height.spec.ts — regression test for the first-render
// textarea collapse in Matrix view. The autosize effect previously set the
// inline height to `scrollHeight` which, for empty / single-line content,
// equals one row — overriding the rows={2} attribute and collapsing every
// cell to ~16 px on entry. Declarative `min-h-*` classes now enforce the
// rows={2|4} floor regardless of what scrollHeight returns.

import { expect, openSampleProject, test } from "./fixture";

test.describe("Translate Matrix — textarea height on first render", () => {
  test("every matrix textarea is at least 40px tall on first paint", async ({
    page,
  }) => {
    await page.goto("/");
    await openSampleProject(page);
    await page
      .getByRole("tablist", { name: /project view/i })
      .getByRole("tab", { name: /^translate$/i })
      .click();

    // Wait for catalogs to load and the matrix cells to mount.
    const cells = page.locator("textarea[data-matrix-cell-input]");
    await expect(cells.first()).toBeVisible({ timeout: 5000 });
    await page.waitForTimeout(300);

    const count = await cells.count();
    expect(count).toBeGreaterThan(0);

    // Each textarea's bounding box should be at least min-h-12 (3rem = 48px)
    // tall. Allow a 6 px tolerance to absorb sub-pixel rounding across DPIs.
    const FLOOR_PX = 42;
    for (let i = 0; i < count; i++) {
      const box = await cells.nth(i).boundingBox();
      expect(box, `textarea #${i} has a bounding box`).not.toBeNull();
      expect(box!.height, `textarea #${i} height ≥ ${FLOOR_PX} px`).toBeGreaterThanOrEqual(
        FLOOR_PX,
      );
    }

    // Save a screenshot so the visual can be eyeballed if a CI fails.
    await page.screenshot({
      path: "test-results/matrix-textarea-height-first-render.png",
      fullPage: false,
    });
  });
});
