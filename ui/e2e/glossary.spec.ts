// glossary.spec.ts — Glossary editor tests.
//
// The fixture glossary has 3 terms:
//   - "Save":   de_DE + es_ES present, zh_Hans missing
//   - "Cancel": de_DE + es_ES present, zh_Hans missing
//   - "Theme":  de_DE + es_ES present, zh_Hans missing
//
// So missingCount = 3 (all terms are missing zh_Hans).
// The "Missing translation" filter should show exactly 3 items.

import { expect, openSampleProject, test } from "./fixture";

async function openGlossary(page: import("@playwright/test").Page) {
  await page.goto("/");
  await openSampleProject(page);
  await page
    .getByRole("tablist", { name: /project view/i })
    .getByRole("tab", { name: /^glossary$/i })
    .click();
  // Wait for the glossary to auto-load from the project glossary_path.
  await page.waitForTimeout(800);
}

test.describe("Glossary editor", () => {
  test("the glossary panel loads and shows 3 terms", async ({ page }) => {
    await openGlossary(page);

    // The term list should show all 3 terms.
    await expect(page.getByText("Save").first()).toBeVisible({ timeout: 5000 });
    await expect(page.getByText("Cancel").first()).toBeVisible();
    await expect(page.getByText("Theme").first()).toBeVisible();
  });

  test("the 'Missing translation' filter chip shows count 3", async ({
    page,
  }) => {
    await openGlossary(page);

    // The nav rail has a "Missing translation" link with a count.
    const missingLink = page.getByText(/missing translation/i).first();
    await expect(missingLink).toBeVisible({ timeout: 5000 });

    // The count "3" should appear near the link.
    // The link renders a count span right after the label.
    await expect(page.getByText("3").first()).toBeVisible();
  });

  test("clicking Missing translation filter shows exactly 3 items", async ({
    page,
  }) => {
    await openGlossary(page);

    // Click the "Missing translation" filter button.
    await page
      .getByText(/missing translation/i)
      .first()
      .click();
    await page.waitForTimeout(200);

    // After filtering, the term list should show 3 items.
    // Each term is rendered as a listbox option or a named row.
    // We check that exactly 3 terms are visible — currently BROKEN:
    // the filter shows 0 items even though count says 3.
    const termRows = page.locator('[role="option"]');
    await expect(termRows).toHaveCount(3, { timeout: 3000 });
  });

  test("adding a term creates an entry and focuses the source field", async ({
    page,
  }) => {
    await openGlossary(page);

    // Click the Add term button (+ icon).
    await page
      .getByRole("button", { name: /add term/i })
      .first()
      .click();
    await page.waitForTimeout(300);

    // The source input should gain focus.
    const sourceInput = page.getByRole("textbox", { name: /term source/i });
    await expect(sourceInput).toBeFocused({ timeout: 2000 });
  });

  test("editing a translation field marks the panel dirty", async ({
    page,
  }) => {
    await openGlossary(page);

    // Select the first term (Save).
    const firstRow = page.locator('[role="option"]').first();
    await firstRow.click();
    await page.waitForTimeout(200);

    // Type in the zh_Hans translation field.
    const zhInput = page.getByRole("textbox").filter({ hasText: "" }).last();
    await zhInput.fill("保存");
    await zhInput.blur();
    await page.waitForTimeout(200);

    // The "Unsaved changes" indicator or Save button should appear.
    const dirty = page
      .getByText(/unsaved changes/i)
      .or(page.getByRole("button", { name: /save/i }))
      .first();
    await expect(dirty).toBeVisible({ timeout: 3000 });
  });
});
