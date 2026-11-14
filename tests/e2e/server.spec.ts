import { expect, test } from '@playwright/test';
import { readFile } from 'node:fs/promises';

test('browser tests use the current production build', async ({ request, baseURL }) => {
  expect(baseURL).toBe(process.env.RUDIMENT_E2E_BASE_URL);
  const response = await request.get('/');
  expect(response.status()).toBe(200);
  expect(await response.text()).toBe(await readFile('dist/index.html', 'utf8'));
});
