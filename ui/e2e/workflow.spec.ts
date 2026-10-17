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
    page
      .getByRole("navigation", { name: "Files" })
      .getByRole("button", { name: /App_sk_SK\.ts Slovak · Qt Linguist/ }),
  ).toBeVisible();
});

test("the glossary view edits terms and saves them as you type", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  await page.getByRole("radio", { name: "Glossary" }).click();
  const glossary = page.getByRole("region", { name: "Glossary" });
  await expect(glossary.getByRole("textbox", { name: "Term", exact: true }).first()).toHaveValue(
    "ParamsManager",
  );
  // ParamsManager occurs in one string; clicking the count lists it.
  await glossary.getByRole("button", { name: /Show the 1 string using ParamsManager/ }).click();
  await expect(page.getByRole("cell", { name: "Open ParamsManager" })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Search strings" })).toHaveValue("ParamsManager");

  await page.getByRole("radio", { name: "Glossary" }).click();
  await glossary.getByRole("button", { name: "Add term" }).click();
  await glossary.getByRole("textbox", { name: "Term", exact: true }).last().fill("Blob");
  await expect(glossary.getByText("Saved in glossary.toml")).toBeVisible();
  // Back and forth: the new term was written.
  await page.getByRole("radio", { name: "Strings" }).click();
  await page.getByRole("radio", { name: "Glossary" }).click();
  await expect(glossary.getByRole("textbox", { name: "Term", exact: true }).last()).toHaveValue(
    "Blob",
  );
});

test("a glossary imported later is applied to finished translations", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  await page.getByRole("combobox", { name: "Translation model" }).click();
  await page.getByRole("option", { name: /eurollm-9b-instruct/ }).click();

  await page.getByRole("radio", { name: "Glossary" }).click();
  const glossary = page.getByRole("region", { name: "Glossary" });
  await glossary.getByRole("button", { name: "Import from a spreadsheet…" }).click();
  await expect(page.getByText("2 terms added, 0 updated")).toBeVisible();
  await expect(glossary.getByRole("textbox", { name: "Term", exact: true }).last()).toHaveValue(
    "Blob",
  );
  // A required translation that a finished string ("Save" → "Uložiť") breaks.
  await glossary.getByRole("button", { name: "Add term" }).click();
  await glossary.getByRole("textbox", { name: "Term", exact: true }).last().fill("Save");
  await glossary.getByRole("checkbox", { name: "Keep Save unchanged" }).click();
  await glossary
    .getByRole("textbox", { name: /translation$/ })
    .last()
    .fill("Zachovať");
  await expect(
    glossary.getByRole("button", { name: "Show the 1 translation breaking Save" }),
  ).toBeVisible();

  await glossary.getByRole("button", { name: "Apply to 1 string" }).click();
  await expect(page.getByRole("status")).toContainText("Finished. 1 translated", {
    timeout: 20_000,
  });
  await expect(
    glossary.getByRole("button", { name: "Every translation follows it" }),
  ).toBeVisible();
});

test("choosing the file already shown keeps its strings", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  const files = page.getByRole("navigation", { name: "Files" });
  await files.getByText("App_sk_SK.ts").click();
  await files.getByText("App_sk_SK.ts").click();
  await expect(page.getByRole("cell", { name: /Filter Size/ })).toBeVisible();
});

test("the keyboard moves through the strings and into the editor", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  await page.getByRole("cell", { name: /Filter Size/ }).click();
  const editor = page.getByRole("complementary", { name: "Translation" });
  await expect(editor.getByText("Filter Size", { exact: true })).toBeVisible();

  // Enter on the row: the cursor goes into its translation.
  await page.keyboard.press("Enter");
  await expect(editor.getByRole("textbox")).toBeFocused();

  // Alt+↓ while editing: the next string, still in the editor.
  await page.keyboard.press("Alt+ArrowDown");
  await expect(editor.getByText("pix", { exact: true })).toBeVisible();
  await expect(editor.getByRole("textbox")).toBeFocused();

  await page.keyboard.press("Alt+ArrowDown");
  await expect(editor.getByText(/defects under %1 mm/)).toBeVisible();
  await page.keyboard.press("Alt+ArrowUp");
  await expect(editor.getByText("pix", { exact: true })).toBeVisible();

  // Esc: back to the list, on the selected row.
  await page.keyboard.press("Escape");
  await expect(page.locator('tr[aria-current="true"]')).toBeFocused();

  // Ctrl+F: the search field.
  await page.keyboard.press("Control+f");
  await expect(page.getByRole("textbox", { name: "Search strings" })).toBeFocused();
});

test("warnings can be listed and the length check tuned", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  await page.getByRole("navigation", { name: "Files" }).getByText("Viewer_sk_SK.ts").click();
  await page.getByRole("radio", { name: /All/ }).click();
  await page.getByRole("combobox", { name: "Which strings" }).click();
  await page.getByRole("option", { name: "Warnings (1)" }).click();
  await expect(page.getByRole("cell", { name: /Fit the image to the window/ })).toBeVisible();
  await expect(page.getByRole("cell", { name: /Zoom in/ })).toHaveCount(0);

  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  await dialog.getByRole("tab", { name: "Checks" }).click();
  const ratio = dialog.getByRole("spinbutton", { name: "Warn when a translation is longer than" });
  await ratio.fill("3");
  await ratio.blur();
  await dialog.getByRole("button", { name: "Save settings" }).click();
  await expect(page.getByText("No warnings in this file.")).toBeVisible();
});

test("strings translated differently in another file can be listed", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  await page.getByRole("navigation", { name: "Files" }).getByText("Viewer_sk_SK.ts").click();
  await page.getByRole("radio", { name: /All/ }).click();
  await page.getByRole("combobox", { name: "Which strings" }).click();
  await page.getByRole("option", { name: /Translated differently in other files \(1\)/ }).click();
  await expect(page.getByRole("cell", { name: "Priblížiť" })).toBeVisible();
  await expect(page.getByRole("cell", { name: /Filter Size/ })).toHaveCount(0);
});
