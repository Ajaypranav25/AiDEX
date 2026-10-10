import { expect, test, type Page } from "@playwright/test";

/** Uncaught exceptions and console errors the page reports while a test runs. */
function collectErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error.message}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });
  return errors;
}

const rail = (page: Page) => page.getByRole("navigation", { name: "Main navigation" });
const topBar = (page: Page) => page.getByRole("banner");

/** Leaves the launch screens through their sample-data paths, as a browser outside Tauri must. */
async function enterWorkspace(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("heading", { level: 1, name: "Choose your AiDEX drive" })).toBeVisible();
  await page.getByRole("button", { name: "Continue without a drive (sample data)" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Unlock your drive" })).toBeVisible();
  await page.getByRole("button", { name: "Continue", exact: true }).click();
  await expect(topBar(page)).toContainText("Chat");
}

/** Rail destinations and the page title the top bar shows for each. */
const destinations = [
  { rail: "Workspaces", title: "Workspaces" },
  { rail: "Chat", title: "Chat" },
  { rail: "History", title: "Session history" },
  { rail: "Remote access", title: "Remote access" },
  { rail: "Plugins", title: "Plugins" },
  { rail: "Agents", title: "Agents" },
  { rail: "Local models", title: "Models" },
  { rail: "Provider accounts", title: "Models" },
  { rail: "Security", title: "Security" },
  { rail: "Settings", title: "Settings" },
] as const;

test("every rail destination and each of its tabs renders without errors", async ({ page }) => {
  const errors = collectErrors(page);
  await enterWorkspace(page);

  await rail(page).getByRole("button", { name: "Home" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Welcome to AiDEX" })).toBeVisible();

  for (const destination of destinations) {
    const item = rail(page).getByRole("button", { name: destination.rail });
    await item.click();
    await expect(topBar(page)).toContainText(destination.title);
    await expect(item).toHaveAttribute("aria-current", "page");

    const tabs = page.getByRole("main").getByRole("tablist").first().getByRole("tab");
    const count = await tabs.count();
    for (let index = 0; index < count; index += 1) {
      await tabs.nth(index).click();
      await expect(tabs.nth(index)).toHaveAttribute("aria-selected", "true");
      await expect(page.getByRole("tabpanel").first()).toBeVisible();
    }
  }

  expect(errors).toEqual([]);
});

test("a drill-in page opens from its list and returns with Back", async ({ page }) => {
  const errors = collectErrors(page);
  await enterWorkspace(page);

  await rail(page).getByRole("button", { name: "Workspaces" }).click();
  await page.getByRole("main").getByRole("button", { name: "Add workspace" }).click();
  await expect(topBar(page)).toContainText("Add workspace");
  await expect(rail(page).getByRole("button", { name: "Workspaces" })).toHaveAttribute("aria-current", "page");

  await page.getByRole("button", { name: "Back", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Workspaces" })).toBeVisible();

  expect(errors).toEqual([]);
});

test("back and forward walk the visited screens by button, keyboard, and mouse", async ({ page }) => {
  const errors = collectErrors(page);
  await enterWorkspace(page);
  const back = page.getByRole("button", { name: "Back", exact: true });
  const forward = page.getByRole("button", { name: "Forward", exact: true });
  await rail(page).getByRole("button", { name: "Settings" }).click();
  await rail(page).getByRole("button", { name: "Security" }).click();
  await expect(topBar(page)).toContainText("Security");

  await back.click();
  await expect(topBar(page)).toContainText("Settings");

  await page.keyboard.press("Alt+ArrowLeft");
  await expect(topBar(page)).toContainText("Chat");
  await expect(back).toBeDisabled();

  await forward.click();
  await expect(topBar(page)).toContainText("Settings");

  await page.keyboard.press("Alt+ArrowRight");
  await expect(topBar(page)).toContainText("Security");
  await expect(forward).toBeDisabled();

  // Playwright's mouse has no side buttons, so dispatch what the webview sends for them.
  await page.locator("body").dispatchEvent("mouseup", { button: 3 });
  await expect(topBar(page)).toContainText("Settings");
  await page.locator("body").dispatchEvent("mouseup", { button: 4 });
  await expect(topBar(page)).toContainText("Security");

  expect(errors).toEqual([]);
});
