// workspace.spec.ts — Project workspace tab navigation tests.

import { expect, openSampleProject, test } from "./fixture";

test.describe("Workspace TopBar", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await openSampleProject(page);
  });

  test("shows all six tabs in the correct order", async ({ page }) => {
    const tabList = page.getByRole("tablist", { name: /project view/i });
    await expect(tabList).toBeVisible();

    const tabs = tabList.getByRole("tab");
    const expectedOrder = [
      "Overview",
      "Translate",
      "Glossary",
      "Settings",
      "Quality",
      "Review",
    ];

    const count = await tabs.count();
    expect(count).toBe(expectedOrder.length);

    for (let i = 0; i < expectedOrder.length; i++) {
      await expect(tabs.nth(i)).toContainText(expectedOrder[i] as string);
    }
  });

  test("default active tab is Overview", async ({ page }) => {
    const overviewTab = page
      .getByRole("tablist", { name: /project view/i })
      .getByRole("tab", { name: /overview/i });
    await expect(overviewTab).toHaveAttribute("aria-selected", "true");
  });

  test("clicking Translate switches to Translate view", async ({ page }) => {
    await page
      .getByRole("tablist", { name: /project view/i })
      .getByRole("tab", { name: /^translate$/i })
      .click();
    await expect(
      page
        .getByRole("tablist", { name: /project view/i })
        .getByRole("tab", { name: /^translate$/i }),
    ).toHaveAttribute("aria-selected", "true");
  });

  test("clicking Glossary switches to Glossary view", async ({ page }) => {
    await page
      .getByRole("tablist", { name: /project view/i })
      .getByRole("tab", { name: /^glossary$/i })
      .click();
    await expect(
      page
        .getByRole("tablist", { name: /project view/i })
        .getByRole("tab", { name: /^glossary$/i }),
    ).toHaveAttribute("aria-selected", "true");
  });

  test("clicking Settings switches to Settings view", async ({ page }) => {
    await page
      .getByRole("tablist", { name: /project view/i })
      .getByRole("tab", { name: /settings/i })
      .click();
    await expect(
      page
        .getByRole("tablist", { name: /project view/i })
        .getByRole("tab", { name: /settings/i }),
    ).toHaveAttribute("aria-selected", "true");
  });

  test("clicking Quality switches to Quality view", async ({ page }) => {
    await page
      .getByRole("tablist", { name: /project view/i })
      .getByRole("tab", { name: /quality/i })
      .click();
    await expect(
      page
        .getByRole("tablist", { name: /project view/i })
        .getByRole("tab", { name: /quality/i }),
    ).toHaveAttribute("aria-selected", "true");
  });

  test("clicking Review switches to Review view", async ({ page }) => {
    await page
      .getByRole("tablist", { name: /project view/i })
      .getByRole("tab", { name: /review/i })
      .click();
    await expect(
      page
        .getByRole("tablist", { name: /project view/i })
        .getByRole("tab", { name: /review/i }),
    ).toHaveAttribute("aria-selected", "true");
  });
});

test.describe("Workspace sidebar visibility", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await openSampleProject(page);
  });

  // The ProjectSidebar should be present in overview, settings, quality views.
  // It should be absent in translate, glossary, review views.

  test("sidebar is visible in Overview", async ({ page }) => {
    // Overview is the default tab — sidebar should be present.
    // The sidebar has a known element — the "Review queue" or catalog list.
    // We'll check for a nav/aside landmark that the sidebar renders.
    const sidebar = page.locator("nav, aside").first();
    await expect(sidebar).toBeVisible();
  });

  test("sidebar is hidden in Translate", async ({ page }) => {
    await page
      .getByRole("tablist", { name: /project view/i })
      .getByRole("tab", { name: /^translate$/i })
      .click();

    // Translate has its own left rail, not the ProjectSidebar.
    // The ProjectSidebar catalog list should not be present.
    // We check that the ProjectSidebar's specific "Catalogs" heading is gone.
    // This is the regression net for the reported bug.
    const sidebarCatalogHeading = page.getByText(/^catalogs$/i).first();
    await expect(sidebarCatalogHeading).not.toBeVisible();
  });

  test("sidebar is hidden in Glossary", async ({ page }) => {
    await page
      .getByRole("tablist", { name: /project view/i })
      .getByRole("tab", { name: /^glossary$/i })
      .click();

    const sidebarCatalogHeading = page.getByText(/^catalogs$/i).first();
    await expect(sidebarCatalogHeading).not.toBeVisible();
  });

  test("sidebar is hidden in Review", async ({ page }) => {
    await page
      .getByRole("tablist", { name: /project view/i })
      .getByRole("tab", { name: /^review$/i })
      .click();

    const sidebarCatalogHeading = page.getByText(/^catalogs$/i).first();
    await expect(sidebarCatalogHeading).not.toBeVisible();
  });

  test("sidebar is visible in Settings", async ({ page }) => {
    await page
      .getByRole("tablist", { name: /project view/i })
      .getByRole("tab", { name: /settings/i })
      .click();

    // Settings keeps the sidebar, so a nav/aside should be present.
    const sidebar = page.locator("nav, aside").first();
    await expect(sidebar).toBeVisible();
  });

  test("sidebar is visible in Quality", async ({ page }) => {
    await page
      .getByRole("tablist", { name: /project view/i })
      .getByRole("tab", { name: /quality/i })
      .click();

    const sidebar = page.locator("nav, aside").first();
    await expect(sidebar).toBeVisible();
  });
});
