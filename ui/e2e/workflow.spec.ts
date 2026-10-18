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
  await expect(files.getByRole("button", { name: /^App_sk_SK\.ts:/ })).toBeVisible();
  await expect(
    files.getByRole("heading", { name: "Slovak · from German · Qt Linguist" }),
  ).toBeVisible();
  // The German texts of a pivot catalog are work to do.
  await expect(page.getByRole("tab", { name: /^To do\s*46$/ })).toBeVisible();

  // The model picker lists what the server offers.
  await page.getByRole("combobox", { name: "Translation model" }).click();
  await page.getByRole("option", { name: /eurollm-9b-instruct/ }).click();
  if (shots) await page.screenshot({ path: `${shots}/workspace.png` });

  await page.getByRole("button", { name: "Translate 46 strings" }).click();
  await expect(page.getByRole("status")).toContainText("Finished. 46 translated", {
    timeout: 20_000,
  });
  await expect(page.getByRole("tab", { name: /^Check\s*46$/ })).toBeVisible();

  // Review one string: it was translated from the German text.
  await page.getByRole("tab", { name: /^Check/ }).click();
  await page.getByRole("cell", { name: /Filter Size/ }).click();
  const editor = page.getByRole("complementary", { name: "Translation" });
  await expect(editor.getByRole("textbox")).toHaveValue("SK·Filtergröße");
  if (shots) await page.screenshot({ path: `${shots}/review.png` });

  await editor.getByRole("textbox").fill("Veľkosť filtra");
  await editor.getByRole("button", { name: "Done" }).click();
  await expect(page.getByRole("tab", { name: /^Done\s*1$/ })).toBeVisible();

  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.getByText("Saved", { exact: true })).toBeVisible();
});

test("a translation that drops a placeholder is flagged", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  await page.getByRole("cell", { name: /Ignore<\/b> defects under/ }).click();
  const editor = page.getByRole("complementary", { name: "Translation" });
  await editor.getByRole("textbox").fill("<b>Ignorovať</b> chyby pod mm");
  await editor.getByRole("button", { name: "Keep as draft" }).click();
  await page.getByRole("tab", { name: /^Check/ }).click();
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
      .getByRole("region", { name: "Slovak · Qt Linguist" })
      .getByRole("button", { name: /^App_sk_SK\.ts:/ }),
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
  await expect(page.getByRole("combobox", { name: "Glossary term" })).toHaveText(/ParamsManager/);
  await expect(page.getByRole("cell", { name: /Filter Size/ })).toHaveCount(0);

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
  await files.getByRole("button", { name: /^App_sk_SK\.ts:/ }).click();
  await files.getByRole("button", { name: /^App_sk_SK\.ts:/ }).click();
  await expect(page.getByRole("cell", { name: /Filter Size/ })).toBeVisible();
});

test("the keyboard moves through the strings and into the editor", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  await page.getByRole("cell", { name: /Filter Size/ }).click();
  const editor = page.getByRole("complementary", { name: "Translation" });
  await expect(editor.getByText("Filter Size", { exact: true })).toBeVisible();

  // ↓ selects the next string; Enter moves the cursor into its translation.
  await page.keyboard.press("ArrowDown");
  await expect(editor.getByText("pix", { exact: true })).toBeVisible();
  await page.keyboard.press("ArrowUp");
  await expect(editor.getByText("Filter Size", { exact: true })).toBeVisible();
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
  await page
    .getByRole("navigation", { name: "Files" })
    .getByRole("button", { name: /^Viewer_sk_SK\.ts:/ })
    .click();
  await page.getByRole("tab", { name: /^All/ }).click();
  await page
    .getByRole("group", { name: "Show only" })
    .getByRole("button", { name: /^Warnings\s*1$/ })
    .click();
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
  await page
    .getByRole("navigation", { name: "Files" })
    .getByRole("button", { name: /^Viewer_sk_SK\.ts:/ })
    .click();
  await page.getByRole("tab", { name: /^All/ }).click();
  await page
    .getByRole("group", { name: "Show only" })
    .getByRole("button", { name: /^Differs\s*1$/ })
    .click();
  await expect(page.getByRole("cell", { name: "Priblížiť" })).toBeVisible();
  await expect(page.getByRole("cell", { name: /Filter Size/ })).toHaveCount(0);
});

test("strings to do are filled from what other files agree on", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  // No model is needed: nothing is translated, only copied.
  await page.getByRole("button", { name: "More translate options" }).click();
  await page.getByRole("menuitem", { name: "Fill from other files (1)" }).click();
  await expect(page.getByText("1 string filled from other files")).toBeVisible();
  // "Filter Size" is done in Viewer_sk_SK.ts, so it is done here too.
  await page.getByRole("tab", { name: /^Done/ }).click();
  await expect(page.getByRole("cell", { name: "Veľkosť filtra" })).toBeVisible();
});

test("a translation differing between files is resolved from the editor", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  await page
    .getByRole("navigation", { name: "Files" })
    .getByRole("button", { name: /^Viewer_sk_SK\.ts:/ })
    .click();
  await page.getByRole("tab", { name: /^All/ }).click();
  await page.getByRole("cell", { name: "Zoom in", exact: true }).click();
  const editor = page.getByRole("complementary", { name: "Translation" });
  const others = editor.getByRole("region", { name: "Other files" });
  await expect(others.getByText("Report_sk_SK.ts")).toBeVisible();
  await expect(others.getByText("Zväčšiť")).toBeVisible();

  // "Use this" takes the other file's text into the editor.
  await others.getByRole("button", { name: "Use this", exact: true }).click();
  await expect(editor.getByRole("textbox")).toHaveValue("Zväčšiť");
  await editor.getByRole("button", { name: "Undo" }).click();

  // Or this file's translation goes to every file.
  await others.getByRole("button", { name: "Use this translation in every file" }).click();
  await expect(page.getByText("Used in 1 other file")).toBeVisible();
  await expect(others.getByText("Priblížiť")).toBeVisible();
  // Nothing differs any more: the chip is gone.
  await expect(
    page.getByRole("group", { name: "Show only" }).getByRole("button", { name: /^Differs/ }),
  ).toHaveCount(0);
});

test("all files can be listed together", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  await page
    .getByRole("navigation", { name: "Files" })
    .getByRole("button", { name: /^All files:/ })
    .click();
  await page.getByRole("tab", { name: /^All/ }).click();
  await expect(page.getByRole("columnheader", { name: "File" })).toBeVisible();
  // Only the rows in view are drawn; the search brings a later file's string up.
  await page.getByRole("textbox", { name: "Search strings" }).fill("Print");
  await expect(page.getByRole("cell", { name: "Report_sk_SK.ts" })).toBeVisible();
  await page.getByRole("cell", { name: "Print", exact: true }).click();
  await expect(
    page.getByRole("complementary", { name: "Translation" }).getByText("Print", { exact: true }),
  ).toBeVisible();
});

test("the file summary shows where a file is and opens the strings behind each count", async ({
  page,
}) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  const files = page.getByRole("navigation", { name: "Files" });
  await expect(files.getByText("46 · 45 · 0")).toBeVisible();
  await files.getByRole("button", { name: /^Viewer_sk_SK\.ts:/ }).click();
  const summary = page.getByRole("complementary", { name: "Summary" });
  await expect(summary.getByText("/demo/translations/Viewer_sk_SK.ts")).toBeVisible();
  await summary.getByRole("button", { name: /Warnings/ }).click();
  await expect(page.getByRole("cell", { name: /Fit the image to the window/ })).toBeVisible();
  await expect(page.getByRole("cell", { name: /Zoom in/ })).toHaveCount(0);

  // A string replaces the summary; Esc twice (editor, then list) brings it back.
  await page.getByRole("cell", { name: /Fit the image to the window/ }).click();
  await expect(page.getByRole("complementary", { name: "Translation" })).toBeVisible();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("complementary", { name: "Summary" })).toBeVisible();
});

test("strings shared between files are compared and resolved across files", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  await page.getByRole("radio", { name: "Across files" }).click();
  const across = page.getByRole("region", { name: "Across files" });
  await expect(across.getByRole("tab", { name: /^Translated differently\s*1$/ })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await across.getByRole("cell", { name: "Zoom in" }).click();
  const detail = page.getByRole("complementary", { name: "Shared string" });
  await expect(detail.getByText("2 translations in 2 files")).toBeVisible();
  await expect(detail.getByText("Zväčšiť")).toBeVisible();

  await detail.getByRole("button", { name: "Use this in every file" }).first().click();
  await expect(page.getByText("Changed in 1 other file")).toBeVisible();
  await expect(across.getByRole("tab", { name: /^Translated differently$/ })).toBeVisible();

  // A file's copy opens in the strings view.
  await across.getByRole("tab", { name: /^All/ }).click();
  await across.getByRole("cell", { name: "Filter Size" }).click();
  await detail
    .getByRole("button", { name: /App_sk_SK\.ts/ })
    .first()
    .click();
  const editor = page.getByRole("complementary", { name: "Translation" });
  await expect(editor.getByText("Filter Size", { exact: true })).toBeVisible();
});

test("the glossary terms of a string are shown and filter the list", async ({ page }) => {
  await page.getByRole("button", { name: "Open folder…" }).click();
  await page.getByRole("cell", { name: "Open ParamsManager" }).click();
  const editor = page.getByRole("complementary", { name: "Translation" });
  const terms = editor.getByRole("region", { name: "Glossary" });
  await expect(terms.getByText("keep unchanged")).toBeVisible();
  await terms.getByRole("button", { name: "ParamsManager" }).click();
  await expect(
    page.getByRole("button", { name: "Remove the filter “ParamsManager”" }),
  ).toBeVisible();
  await expect(page.getByRole("cell", { name: /Filter Size/ })).toHaveCount(0);
  await expect(page.getByRole("cell", { name: "Open ParamsManager" })).toBeVisible();
});
