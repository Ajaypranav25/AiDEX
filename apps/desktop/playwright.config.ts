import { defineConfig, devices } from "@playwright/test";

/** The dev port from CLAUDE.md; Next.js binds it to 127.0.0.1 only. */
const PORT = 3311;
const CI = Boolean(process.env.CI);

/**
 * Smoke tests against the Next.js dev server in a plain browser. Outside Tauri every bridge command
 * fails with `bridge_unavailable`, which is the app's labelled sample-data mode.
 */
export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: CI,
  retries: CI ? 1 : 0,
  reporter: CI ? [["github"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [
    {
      name: "chromium",
      // The Tauri window opens at 1440×920.
      use: { ...devices["Desktop Chrome"], viewport: { width: 1440, height: 920 } },
    },
  ],
  webServer: {
    command: "bun run dev",
    url: `http://127.0.0.1:${PORT}`,
    reuseExistingServer: !CI,
    timeout: 120_000,
  },
});
