import { expect, test } from '@playwright/test';

test('unresolved automatic MIDI permission does not block the workspace', async ({ page }) => {
  await page.addInitScript(() => {
    (window as any).__midiRequests = 0;
    Object.defineProperty(navigator, 'requestMIDIAccess', {
      configurable: true,
      value: () => {
        (window as any).__midiRequests += 1;
        return new Promise(() => {});
      },
    });
    Object.defineProperty(navigator, 'permissions', {
      configurable: true,
      value: { query: async () => ({ state: 'granted', onchange: null }) },
    });
  });

  await page.goto('/');
  await expect.poll(() => page.evaluate(() => (window as any).__midiRequests)).toBe(1);
  await expect(page.locator('#notation svg')).toBeVisible();
  await page.getByRole('button', { name: 'Open settings' }).click();
  await expect(page.getByRole('button', { name: 'Close settings' }).first()).toBeVisible();
});

test('automatic MIDI connection remains connected after the modal delay', async ({ page }) => {
  await page.clock.install();
  await page.addInitScript(() => {
    const input = {
      id: 'auto-kit',
      name: 'Auto Kit',
      manufacturer: 'Test',
      type: 'input',
      state: 'connected',
      connection: 'open',
      onmidimessage: null,
      onstatechange: null,
      open: async () => input,
      close: async () => input,
    };
    const access = {
      inputs: new Map([[input.id, input]]),
      outputs: new Map(),
      sysexEnabled: false,
      onstatechange: null,
    };
    Object.defineProperty(navigator, 'requestMIDIAccess', { configurable: true, value: async () => access });
    Object.defineProperty(navigator, 'permissions', {
      configurable: true,
      value: { query: async () => ({ state: 'granted', onchange: null }) },
    });
  });

  await page.goto('/');
  await expect(page.locator('.status-pill')).toContainText('Auto Kit · connected');
  await page.clock.fastForward(3_100);
  await expect(page.getByRole('dialog', { name: /MIDI connection/i })).toHaveCount(0);
  await expect(page.locator('.status-pill')).toContainText('Auto Kit · connected');
});
