import { expect, test } from '@playwright/test';

// Issue #34: first-run setup moved to a dedicated onboarding page shown from
// the first paint, and the delayed three-second startup MIDI modal is retired.

test('fresh profile shows onboarding immediately without waiting for MIDI', async ({ page }) => {
  await page.clock.install();
  await page.addInitScript(() => {
    (window as any).__midiRequests = 0;
    Object.defineProperty(navigator, 'requestMIDIAccess', {
      configurable: true,
      value: () => {
        (window as any).__midiRequests += 1;
        return new Promise(() => {});
      },
    });
  });

  await page.goto('/');
  // Onboarding is the first page: the heading is there before MIDI discovery
  // resolves and the workspace stays inert behind it.
  await expect(page.locator('#onboarding')).toBeVisible();
  await expect(page.locator('#onboarding-title')).toHaveText("Let's get you set up.");
  await expect(page.locator('main.app')).toHaveAttribute('inert', '');
  await expect(page.locator('.onboarding .midi-state')).toContainText('Checking for MIDI drums');
  await expect.poll(() => page.evaluate(() => (window as any).__midiRequests)).toBe(1);
  // The startup modal timer is retired: no MIDI connection dialog surfaces.
  await page.clock.fastForward(10_000);
  await expect(page.getByRole('dialog', { name: /MIDI connection/i })).toHaveCount(0);
  // Practice remains reachable without MIDI.
  await page.getByRole('button', { name: 'Enter practice workspace' }).click();
  await expect(page.locator('main.app')).not.toHaveAttribute('inert', '');
  await expect(page.getByRole('button', { name: /Start practice/ })).toBeVisible();
  await expect(page.locator('#onboarding')).toBeHidden();
});

test('a connected kit appears on onboarding without dismissing it or opening a modal', async ({ page }) => {
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
  await expect(page.locator('#onboarding')).toBeVisible();
  await expect(page.locator('.onboarding .midi-state')).toContainText('Auto Kit · connected');
  await page.clock.fastForward(10_000);
  await expect(page.getByRole('dialog', { name: /MIDI connection/i })).toHaveCount(0);
  await expect(page.locator('#onboarding')).toBeVisible();
});

test('display choice applies to the workspace and opting out persists', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('#onboarding')).toBeVisible();
  await page.locator('#onboarding').getByRole('button', { name: 'Highway', exact: true }).click();
  await expect(page.locator('#onboarding').getByRole('button', { name: 'Highway', exact: true })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await page.locator('.onboarding-skip input').check();
  await page.getByRole('button', { name: 'Enter practice workspace' }).click();
  await expect(page.locator('#highway svg')).toBeVisible();
  await expect(page.locator('#notation')).toBeHidden();
  expect(await page.evaluate(() => localStorage.getItem('rudiment-onboarding-skipped'))).toBe('true');
  expect(await page.evaluate(() => localStorage.getItem('rudiment-visualisation'))).toBe('highway');

  // Opted-out returning users never see onboarding again.
  await page.reload();
  await expect(page.locator('#onboarding')).toBeHidden();
  await expect(page.locator('#highway svg')).toBeVisible();
});

test('without opting out, dismissal is session-only and preferences persist', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('#onboarding')).toBeVisible();
  await page.locator('#onboarding').getByRole('button', { name: 'Notation', exact: true }).click();
  await page.getByRole('button', { name: 'Enter practice workspace' }).click();
  await expect(page.locator('#notation svg')).toBeVisible();
  expect(await page.evaluate(() => localStorage.getItem('rudiment-onboarding-skipped'))).toBeNull();

  // The display choice persists, but onboarding is offered again next session.
  await page.reload();
  await expect(page.locator('#onboarding')).toBeVisible();
  await expect(page.locator('#notation svg')).toBeVisible();
});

test('settings reopens onboarding and keeps manual MIDI configuration', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('rudiment-onboarding-skipped', 'true');
    Object.defineProperty(navigator, 'requestMIDIAccess', {
      configurable: true,
      value: () => new Promise(() => {}),
    });
  });
  await page.goto('/');
  await expect(page.locator('#onboarding')).toBeHidden();
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByRole('button', { name: 'Reopen setup page…' }).click();
  await expect(page.locator('#onboarding')).toBeVisible();
  await expect(page.locator('main.app')).toHaveAttribute('inert', '');
  await page.getByRole('button', { name: 'Enter practice workspace' }).click();
  await expect(page.locator('#onboarding')).toBeHidden();

  // Manual MIDI configuration stays reachable from Settings.
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByRole('button', { name: 'Open MIDI connection setup' }).click();
  const dialog = page.getByRole('dialog', { name: /MIDI connection/i });
  await expect(dialog).toBeVisible();
  await page.getByRole('button', { name: 'Continue without MIDI' }).click();
  await expect(dialog).toHaveCount(0);
});
