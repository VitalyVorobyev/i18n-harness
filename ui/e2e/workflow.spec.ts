import { expect, test } from "@playwright/test";

// The translator's main path against the mock backend: open a folder, see
// what is to do, translate with a model, check and confirm, save.

const shots = process.env.SCREENSHOTS;

test.beforeEach(async ({ page }) => {
  await page.goto("/");
});

test("start screen offers to open a folder and reports the model server", async ({ page }) => {
  await expect(page.getByRole("heading", { name: "i18n-harness" })).toBeVisible();
  await expect(page.getByText("Ollama is running")).toBeVisible();
  if (shots) await page.screenshot({ path: `${shots}/start.png` });
});

test("open, translate, confirm and save", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();

  const files = page.getByRole("navigation", { name: "Files" });
  await expect(files.getByText("App_sk_SK.ts")).toBeVisible();
  await expect(files.getByText("Slovak · from German · Qt Linguist")).toBeVisible();
  // The German texts of a pivot catalog are work to do.
  await expect(page.getByRole("radio", { name: /To do 46/ })).toBeVisible();

  // The model picker lists what the server offers.
  await page.getByRole("combobox", { name: "Translation model" }).click();
  await page.getByRole("option", { name: /eurollm-9b-instruct/ }).click();
  if (shots) await page.screenshot({ path: `${shots}/workspace.png` });

  await page.getByRole("button", { name: "Translate 46 strings" }).click();
  await expect(page.getByRole("status")).toContainText("Finished. 46 translated", {
    timeout: 20_000,
  });
  await expect(page.getByRole("radio", { name: /Check 46/ })).toBeVisible();

  // Review one string: it was translated from the German text.
  await page.getByRole("radio", { name: /Check/ }).click();
  await page.getByRole("cell", { name: /Filter Size/ }).click();
  const editor = page.getByRole("complementary", { name: "Translation" });
  await expect(editor.getByRole("textbox")).toHaveValue("SK·Filtergröße");
  if (shots) await page.screenshot({ path: `${shots}/review.png` });

  await editor.getByRole("textbox").fill("Veľkosť filtra");
  await editor.getByRole("button", { name: "Done" }).click();
  await expect(page.getByRole("radio", { name: /Done 1/ })).toBeVisible();

  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.getByText("Saved", { exact: true })).toBeVisible();
});

test("a translation that drops a placeholder is flagged", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  await page.getByRole("cell", { name: /Ignore<\/b> defects under/ }).click();
  const editor = page.getByRole("complementary", { name: "Translation" });
  await editor.getByRole("textbox").fill("<b>Ignorovať</b> chyby pod mm");
  await editor.getByRole("button", { name: "Keep as draft" }).click();
  await page.getByRole("radio", { name: /Check/ }).click();
  await page.getByRole("cell", { name: /Ignore<\/b> defects under/ }).click();
  await expect(
    editor.getByText(/Placeholders do not match the original: missing %1/),
  ).toBeVisible();
});

test("the language of the existing text can be changed", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  await page.getByRole("button", { name: "More for App_sk_SK.ts" }).click();
  await page.getByRole("menuitem", { name: "Language of the existing text…" }).click();
  const dialog = page.getByRole("dialog", { name: "Language of the existing text" });
  await dialog.getByRole("combobox").click();
  await page.getByRole("option", { name: /Slovak \(already translated\)/ }).click();
  await dialog.getByRole("button", { name: "Apply" }).click();
  await expect(
    page.getByRole("navigation", { name: "Files" }).getByText("Slovak · Qt Linguist"),
  ).toBeVisible();
});

test("settings edit the glossary", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  await expect(dialog.getByRole("textbox", { name: "Term" }).first()).toHaveValue("ParamsManager");
  await dialog.getByRole("button", { name: "Add term" }).click();
  await dialog.getByRole("textbox", { name: "Term" }).last().fill("Blob");
  if (shots) await page.screenshot({ path: `${shots}/settings.png` });
  await dialog.getByRole("button", { name: "Save settings" }).click();
  await expect(page.getByText("Settings saved")).toBeVisible();
});
