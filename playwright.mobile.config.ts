import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: 'tests/mobile',
  timeout: 60_000,
  workers: 1,
  use: { baseURL: 'http://127.0.0.1:5174', channel: 'chrome' },
  webServer: {
    command: 'npm run dev:mobile -- --strictPort',
    url: 'http://127.0.0.1:5174',
    reuseExistingServer: !process.env.CI
  }
});
