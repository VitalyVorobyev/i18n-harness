import type { Page } from "@playwright/test";
import { expect, test } from "@playwright/test";

// Screenshots for the user guide in `book/`. Runs only when BOOK_SCREENSHOTS
// names the output folder:
//
//   BOOK_SCREENSHOTS=../book/src/images bunx playwright test e2e/book-screenshots.spec.ts

const out = process.env.BOOK_SCREENSHOTS;

test.skip(!out, "set BOOK_SCREENSHOTS=<dir> to capture the book's screenshots");
test.use({ viewport: { width: 1280, height: 800 } });

const shot = (page: Page, name: string) => page.screenshot({ path: `${out}/${name}.png` });

async function openFolder(page: Page) {
  await page.goto("/");
  await page.getByRole("button", { name: "Open folder…" }).click();
  await expect(page.getByRole("navigation", { name: "Files" })).toBeVisible();
}

async function chooseModel(page: Page) {
  await page.getByRole("combobox", { name: "Translation model" }).click();
  await page.getByRole("option", { name: /eurollm-9b-instruct/ }).click();
}

test("start screen", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Ollama is running")).toBeVisible();
  await shot(page, "start");
});

test("workspace", async ({ page }) => {
  await openFolder(page);
  await page.getByRole("combobox", { name: "Translation model" }).click();
  await expect(page.getByRole("option", { name: /milmmt/ })).toBeVisible();
  await shot(page, "model-picker");
  await page.getByRole("option", { name: /eurollm-9b-instruct/ }).click();
  await page.getByRole("cell", { name: /Filter Size/ }).click();
  await shot(page, "workspace");
});

test("translation in progress and review", async ({ page }) => {
  await openFolder(page);
  await chooseModel(page);
  await page.getByRole("button", { name: "Translate 46 strings" }).click();
  await expect(page.getByRole("status")).toContainText(/Translating (1\d|2\d) of 46/, {
    timeout: 10_000,
  });
  await shot(page, "translating");
  await expect(page.getByRole("status")).toContainText("Finished. 46 translated", {
    timeout: 20_000,
  });
  await page.getByRole("radio", { name: /Check/ }).click();
  await page.getByRole("cell", { name: /Filter Size/ }).click();
  await expect(
    page.getByRole("complementary", { name: "Translation" }).getByRole("textbox"),
  ).toHaveValue("SK·Filtergröße");
  await shot(page, "review");
});

test("editor with a problem", async ({ page }) => {
  await openFolder(page);
  await page.getByRole("cell", { name: /Ignore<\/b> defects under/ }).click();
  const editor = page.getByRole("complementary", { name: "Translation" });
  await editor.getByRole("textbox").fill("<b>Ignorovať</b> chyby pod mm");
  await editor.getByRole("button", { name: "Keep as draft" }).click();
  await page.getByRole("radio", { name: /Check/ }).click();
  await page.getByRole("cell", { name: /Ignore<\/b> defects under/ }).click();
  await expect(
    editor.getByText(/Placeholders do not match the original: missing %1/),
  ).toBeVisible();
  await shot(page, "editor-problem");
});

test("pivot dialog", async ({ page }) => {
  await openFolder(page);
  await page.getByRole("button", { name: "More for App_sk_SK.ts" }).click();
  await page.getByRole("menuitem", { name: "Language of the existing text…" }).click();
  const dialog = page.getByRole("dialog", { name: "Language of the existing text" });
  await expect(dialog).toBeVisible();
  await shot(page, "pivot-dialog");
});

test("settings", async ({ page }) => {
  await openFolder(page);
  await page.getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  await expect(dialog.getByRole("textbox", { name: "Term" }).first()).toHaveValue("ParamsManager");
  await dialog.getByRole("button", { name: "Add term" }).click();
  await dialog.getByRole("textbox", { name: "Term" }).last().fill("filter");
  await dialog.getByRole("checkbox", { name: "Keep filter unchanged" }).click();
  await dialog.getByRole("textbox", { name: "Slovak translation" }).last().fill("filter");
  await dialog.getByRole("textbox", { name: "Note" }).last().fill("Image filter");
  await shot(page, "settings-glossary");
  await dialog.getByRole("tab", { name: "Model server" }).click();
  await expect(dialog.getByText("Context size (tokens)")).toBeVisible();
  await shot(page, "settings-server");
});
