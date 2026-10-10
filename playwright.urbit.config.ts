import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: 'tests/urbit',
  timeout: 90_000,
  workers: 1,
  use: {
    baseURL: process.env.NOCKSTER_TEST_URL || 'http://127.0.0.1:5186',
    viewport: { width: 1000, height: 850 },
    trace: 'retain-on-failure',
    actionTimeout: 15_000
  },
  webServer: process.env.NOCKSTER_TEST_URL
    ? undefined
    : {
        command: 'npm run preview --workspace @nockster/urbit',
        url: 'http://127.0.0.1:5186',
        reuseExistingServer: false
      }
});
