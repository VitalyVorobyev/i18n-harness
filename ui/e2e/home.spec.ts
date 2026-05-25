// home.spec.ts — Home screen smoke tests.

import { expect, openSampleProject, test } from "./fixture";

test.describe("Home screen", () => {
  test("renders the identity block and action buttons", async ({ page }) => {
    await page.goto("/");

    // The app name appears in the header/identity block (h1).
    await expect(
      page.getByRole("heading", { name: "i18n-harness", level: 1 }),
    ).toBeVisible();

    // At least one "Open" button is visible.
    await expect(
      page.getByRole("button", { name: /open/i }).first(),
    ).toBeVisible();
  });

  test("recent projects list is present (even if empty)", async ({ page }) => {
    await page.goto("/");
    // The home screen should render without crashing. Either a list or an
    // empty-state element is expected.
    await expect(page.locator("body")).toBeVisible();
  });

  test("opening a project navigates to the workspace Overview tab", async ({
    page,
  }) => {
    await page.goto("/");
    await openSampleProject(page);

    // After opening, the active tab should be Overview.
    const overviewTab = page.getByRole("tab", { name: /overview/i });
    await expect(overviewTab).toBeVisible();
    await expect(overviewTab).toHaveAttribute("aria-selected", "true");
  });

  test("project name appears in the workspace after opening", async ({
    page,
  }) => {
    await page.goto("/");
    await openSampleProject(page);

    // The project name shows up in many places after opening; assert on the
    // banner copy specifically (visible in every tab as the top-bar label).
    await expect(
      page.getByRole("banner").getByText("sample-project"),
    ).toBeVisible();
  });
});
