import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests',
  testMatch: 'browser.spec.ts',
  timeout: 60000,
  use: {
    baseURL: 'http://localhost:3000',
    viewport: { width: 1440, height: 960 },
  },
  webServer: {
    command: 'npm run dev',
    url: 'http://localhost:3000',
    reuseExistingServer: true,
  },
  reporter: 'list',
});
