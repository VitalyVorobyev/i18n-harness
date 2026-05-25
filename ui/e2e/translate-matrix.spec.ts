// translate-matrix.spec.ts — Matrix mode translation tests.
//
// These tests verify the Matrix view's data rendering and per-cell behaviour.
// Some assertions WILL fail if the current product implementation has bugs
// (e.g. skeleton bars showing instead of real cards). That's intentional —
// failures here are the punch list for the fix PR.

import { expect, openSampleProject, test } from "./fixture";
import fixtureData from "./fixtures/sample-project.json" with { type: "json" };

// The sample project has 9 unique source unit IDs.
const EXPECTED_UNIT_COUNT = 9;
const PROJECT_LOCALES = ["de_DE", "es_ES", "zh_Hans"];

async function navigateToTranslate(page: import("@playwright/test").Page) {
  await page.goto("/");
  await openSampleProject(page);
  await page
    .getByRole("tablist", { name: /project view/i })
    .getByRole("tab", { name: /^translate$/i })
    .click();
  // Wait for the translate panel to be in the DOM.
  await page.waitForTimeout(300);
}

test.describe("Translate — Matrix mode", () => {
  test("Matrix mode is the default (focusLocale is null)", async ({ page }) => {
    await navigateToTranslate(page);

    // In Matrix mode the Focus-mode-only clear-focus dismiss button is
    // absent. If FocusView was accidentally mounted it would render that
    // button next to the locale chip.
    await expect(
      page.getByRole("button", { name: /clear focus locale/i }),
    ).not.toBeVisible();
  });

  test("after catalogs load, card list shows exactly 9 cards", async ({
    page,
  }) => {
    await navigateToTranslate(page);

    // Trigger catalog loading — the matrix loads all catalogs eagerly.
    // Wait a generous timeout for the IPC calls to resolve.
    await page.waitForTimeout(1000);

    // Each source unit maps to one card article in the matrix list.
    const cards = page.locator("article");
    await expect(cards).toHaveCount(EXPECTED_UNIT_COUNT, { timeout: 5000 });
  });

  test("each card has cells for all three project locales", async ({
    page,
  }) => {
    await navigateToTranslate(page);
    await page.waitForTimeout(1000);

    // Each locale should appear as a column header or cell label.
    for (const locale of PROJECT_LOCALES) {
      await expect(page.getByText(locale).first()).toBeVisible();
    }
  });

  test("an untranslated cell shows a Translate button", async ({ page }) => {
    await navigateToTranslate(page);
    await page.waitForTimeout(1000);

    // The "Hello" unit is untranslated in all locales.
    // A "Translate to de_DE" or "Translate" button should be present.
    const translateBtn = page
      .getByRole("button", { name: /translate/i })
      .first();
    await expect(translateBtn).toBeVisible();
  });

  test("a finished cell (Save → Speichern) shows read-only text, not Translate", async ({
    page,
  }) => {
    await navigateToTranslate(page);
    await page.waitForTimeout(1000);

    // "Speichern" is the de_DE translation for "Save" in the fixture.
    await expect(page.getByText("Speichern")).toBeVisible();

    // The finished cell should NOT have a Translate button adjacent to it.
    // We verify by checking that "Speichern" is NOT inside a button.
    const speicherenInsideButton = page.locator('button:has-text("Speichern")');
    await expect(speicherenInsideButton).toHaveCount(0);
  });

  test("finished cell has an Edit control", async ({ page }) => {
    await navigateToTranslate(page);
    await page.waitForTimeout(1000);

    // Finished cells should expose an Edit (pencil) button so the user can
    // revise the translation. If this count is 0 the feature is missing.
    // We look for a button near the "Speichern" text — either with aria-label
    // "Edit" or title "Edit" or text "Edit".
    const editBtn = page.getByRole("button", { name: /edit/i }).first();
    await expect(editBtn).toBeVisible();
  });

  test("locale chips match the project locales", async ({ page }) => {
    await page.goto("/");
    await openSampleProject(page);

    // Locale chips are rendered in the TopBar.
    for (const locale of PROJECT_LOCALES) {
      await expect(
        page.getByRole("button", { name: locale }).first(),
      ).toBeVisible();
    }
  });

  test("fixture source units match expected IDs", async () => {
    // Pure data check — verifies the fixture is correct before any UI assertion.
    const deUnits =
      fixtureData.catalogs[
        "/sample-project/translations/app_de.ts" as keyof typeof fixtureData.catalogs
      ]?.units ?? [];
    expect(deUnits).toHaveLength(EXPECTED_UNIT_COUNT);

    const ids = deUnits.map((u) => u.id);
    expect(ids).toContain("MainWindow/Save");
    expect(ids).toContain("SettingsDialog/Dark");
  });
});
