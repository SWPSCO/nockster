import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: 'tests/desktop',
  timeout: 60_000,
  workers: 1,
  use: {
    baseURL: 'http://127.0.0.1:5175',
    viewport: { width: 1120, height: 780 },
    trace: 'retain-on-failure'
  },
  webServer: {
    command: 'npm run dev:desktop:web',
    url: 'http://127.0.0.1:5175',
    reuseExistingServer: !process.env.CI
  }
});
