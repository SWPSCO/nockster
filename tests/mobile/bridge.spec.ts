import { test, expect } from '@playwright/test';
test('bridge encoding and split outputs agree with the independent Iris engine', async ({
  page
}) => {
  await page.route('https://**', route => route.abort());
  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.nocksterNative));
  const passed = await page.evaluate(
    async path => {
      const fixture = await import('/@fs' + path);
      return fixture.verify();
    },
    new URL('./bridge.fixture.ts', import.meta.url).pathname
  );
  expect(passed).toHaveLength(5);
});
