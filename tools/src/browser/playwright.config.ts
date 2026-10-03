import { defineConfig, devices } from "@playwright/test";

const PORT = Number(process.env["PORT"] ?? 4174);

export default defineConfig({
  testDir: import.meta.dirname,
  testMatch: "*.spec.js",
  outputDir: "../../../test-results",
  fullyParallel: true,
  forbidOnly: !!process.env["CI"],
  reporter: process.env["CI"] ? [["list"], ["github"]] : "list",
  use: { baseURL: `http://127.0.0.1:${PORT}` },
  // Desktop engines. No real mobile device is attached in CI, and a mobile
  // viewport on a desktop engine says nothing about mobile WASM, so none is
  // claimed.
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "firefox", use: { ...devices["Desktop Firefox"] } },
    { name: "webkit", use: { ...devices["Desktop Safari"] } },
  ],
  webServer: {
    command: `node ${import.meta.dirname}/server.js`,
    // The repository root, so SCOLTA_CORE_TARBALLS paths resolve as CI writes them.
    cwd: `${import.meta.dirname}/../../..`,
    url: `http://127.0.0.1:${PORT}/routes.json`,
    env: { PORT: String(PORT) },
    reuseExistingServer: false,
    stdout: "pipe",
  },
});
