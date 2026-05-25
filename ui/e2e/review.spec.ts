// review.spec.ts — Review panel tests.

import { expect, openSampleProject, test } from "./fixture";

async function openReview(page: import("@playwright/test").Page) {
  await page.goto("/");
  await openSampleProject(page);
  await page
    .getByRole("tablist", { name: /project view/i })
    .getByRole("tab", { name: /^review$/i })
    .click();
  await page.waitForTimeout(300);
}

test.describe("Review panel", () => {
  test("the Review tab label reads 'Review'", async ({ page }) => {
    await page.goto("/");
    await openSampleProject(page);

    // The tab button should contain the text "Review" (not "Review queue").
    const reviewTab = page
      .getByRole("tablist", { name: /project view/i })
      .getByRole("tab", { name: /^review$/i });

    await expect(reviewTab).toBeVisible();
    // Verify the visible text is exactly "Review" (no extra label suffix).
    const label = await reviewTab.textContent();
    expect(label?.trim()).toMatch(/^Review/);
    expect(label?.trim()).not.toMatch(/queue/i);
  });

  test("Review panel sub-tab strip shows Queue and Proofread tabs", async ({
    page,
  }) => {
    await openReview(page);

    // The review panel has its own sub-tab strip.
    const queueTab = page.getByRole("tab", { name: /queue/i });
    const proofreadTab = page.getByRole("tab", { name: /proofread/i });

    await expect(queueTab).toBeVisible({ timeout: 3000 });
    await expect(proofreadTab).toBeVisible();
  });

  test("Queue sub-tab is active by default", async ({ page }) => {
    await openReview(page);

    const queueTab = page.getByRole("tab", { name: /queue/i });
    await expect(queueTab).toHaveAttribute("aria-selected", "true", {
      timeout: 3000,
    });
  });

  test("clicking Proofread switches to Proofread sub-view", async ({
    page,
  }) => {
    await openReview(page);

    await page.getByRole("tab", { name: /proofread/i }).click();
    await page.waitForTimeout(200);

    const proofreadTab = page.getByRole("tab", { name: /proofread/i });
    await expect(proofreadTab).toHaveAttribute("aria-selected", "true");
  });

  test("Proofread view renders after clicking the sub-tab", async ({
    page,
  }) => {
    await openReview(page);

    await page.getByRole("tab", { name: /proofread/i }).click();
    // Wait for the proofread panel to load (it eager-loads all catalogs).
    await page.waitForTimeout(1500);

    // The proofread panel should render content or an empty-state message.
    // It must not be completely blank — check that the tabpanel is present.
    const proofPanel = page.locator("[id='subtab-panel-proofread']");
    await expect(proofPanel).toBeVisible({ timeout: 5000 });
  });

  test("Queue panel shows 'Loading review queue' or items when Queue is active", async ({
    page,
  }) => {
    await openReview(page);

    // Queue is active by default. Panel content should be visible.
    const queuePanel = page.locator("[id='subtab-panel-queue']");
    await expect(queuePanel).toBeVisible({ timeout: 3000 });
  });
});
