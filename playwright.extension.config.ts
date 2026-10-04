import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: 'tests/extension',
  timeout: 90_000,
  workers: 1,
  use: { baseURL: 'http://127.0.0.1:5175', channel: 'chrome' },
  webServer: {
    command: 'npm exec -- vite . --config apps/extension/vite.config.ts --mode test --host 127.0.0.1 --port 5175 --strictPort',
    url: 'http://127.0.0.1:5175/tests/extension/wallet-ui.html',
    reuseExistingServer: !process.env.CI
  }
});
