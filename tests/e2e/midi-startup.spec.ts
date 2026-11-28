import { expect, test } from '@playwright/test';

test('unresolved automatic MIDI permission does not block the workspace', async ({ page }) => {
  await page.clock.install();
  await page.addInitScript(() => {
    (window as any).__midiRequests = 0;
    // The fallback timer is armed during app start-up; record that moment so
    // the fast-forward offsets below can be computed against it.
    (window as any).__appStart = Date.now();
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
  // Freeze the clock so the three-second fallback cannot fire while
  // assertions run; virtual time only moves via fastForward below. The
  // pause point is taken from the page's own clock with a generous margin:
  // virtual time advances while the page loads, so a small offset taken from
  // the test process can already lie in the past and pauseAt would refuse it.
  const pauseTarget = await page.evaluate(() => Date.now() + 500);
  await page.clock.pauseAt(pauseTarget);
  await expect.poll(() => page.evaluate(() => (window as any).__midiRequests)).toBe(1);
  await expect(page.locator('#notation svg')).toBeVisible();
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  // Just before the three-second fallback the workspace stays modal-free.
  // The virtual clock tracks real time while the page loads, so a slow first
  // load can already sit inside the final 500 ms window when the clock is
  // paused. Clamp the offset at zero so the virtual clock is never asked to
  // move backwards; every assertion is unchanged.
  const sinceAppStart = await page.evaluate(() => Date.now() - (window as any).__appStart);
  await page.clock.fastForward(Math.max(0, 3_000 - sinceAppStart - 500));
  await expect(page.getByRole('dialog', { name: /MIDI connection/i })).toHaveCount(0);
  // Past three seconds the shared MIDI connection modal appears.
  await page.clock.fastForward(6_000);
  await expect(page.getByRole('dialog', { name: /MIDI connection/i })).toBeVisible();
  // The workspace remains usable around it.
  await expect(page.getByRole('button', { name: /Start practice/ })).toBeAttached();
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
  await expect(page.locator('#settings-dialog .midi-state')).toContainText('Auto Kit · connected');
  await page.clock.fastForward(3_100);
  await expect(page.getByRole('dialog', { name: /MIDI connection/i })).toHaveCount(0);
  await expect(page.locator('#settings-dialog .midi-state')).toContainText('Auto Kit · connected');
});

test('dismissed startup modal stays dismissed but reopens through the settings control', async ({ page }) => {
  await page.clock.install();
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'requestMIDIAccess', {
      configurable: true,
      value: () => new Promise(() => {}),
    });
  });

  await page.goto('/');
  await page.clock.fastForward(3_100);
  const dialog = page.getByRole('dialog', { name: /MIDI connection/i });
  await expect(dialog).toBeVisible();
  // Dismiss with the real accessible close control.
  await page.getByRole('button', { name: 'Close MIDI connection setup' }).click();
  await expect(dialog).toHaveCount(0);
  // Practice stays usable and the modal is not shown again this session.
  await page.clock.fastForward(10_000);
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Start practice/ })).toBeVisible();
  // The settings "MIDI connection setup" control reopens it on demand.
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByRole('button', { name: 'Open MIDI connection setup' }).click();
  await expect(dialog).toBeVisible();
  // "Continue without MIDI" also dismisses it.
  await page.getByRole('button', { name: 'Continue without MIDI' }).click();
  await expect(dialog).toHaveCount(0);
});

test('a connection resolving after the modal appears dismisses it automatically', async ({ page }) => {
  await page.clock.install();
  await page.addInitScript(() => {
    const input = {
      id: 'late-resolve-kit',
      name: 'Late Resolve Kit',
      manufacturer: 'Test',
      type: 'input',
      state: 'connected',
      connection: 'open',
      onmidimessage: null,
      onstatechange: null,
      open: async () => input,
      close: async () => input,
    };
    let resolveAccess: (value: unknown) => void = () => {};
    const accessPromise = new Promise((resolve) => {
      resolveAccess = resolve;
    });
    (window as any).__resolveMidiAccess = () =>
      resolveAccess({
        inputs: new Map([[input.id, input]]),
        outputs: new Map(),
        sysexEnabled: false,
        onstatechange: null,
      });
    Object.defineProperty(navigator, 'requestMIDIAccess', { configurable: true, value: () => accessPromise });
  });

  await page.goto('/');
  await page.clock.fastForward(3_100);
  const dialog = page.getByRole('dialog', { name: /MIDI connection/i });
  await expect(dialog).toBeVisible();
  // The user grants permission while the modal is open; the app connects on
  // its own and the startup modal disappears without further interaction.
  await page.evaluate(() => (window as any).__resolveMidiAccess());
  await expect(dialog).toHaveCount(0);
  await expect(page.locator('#settings-dialog .midi-state')).toContainText('Late Resolve Kit · connected');
  await page.clock.fastForward(5_000);
  await expect(dialog).toHaveCount(0);
});

test('the startup modal selects one of several devices and closes on connection', async ({ page }) => {
  await page.clock.install();
  await page.addInitScript(() => {
    const makeInput = (id: string, name: string) => {
      const input = {
        id,
        name,
        manufacturer: 'Rudiment Test',
        type: 'input',
        state: 'connected',
        connection: 'open',
        onmidimessage: null as null | ((event: unknown) => void),
        onstatechange: null,
        open: async () => input,
        close: async () => input,
        addEventListener: () => {},
        removeEventListener: () => {},
      };
      return input;
    };
    const first = makeInput('kit-a', 'Kit A'),
      second = makeInput('kit-b', 'Kit B');
    const access = {
      inputs: new Map([
        [first.id, first],
        [second.id, second],
      ]),
      outputs: new Map(),
      sysexEnabled: false,
      onstatechange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
    };
    Object.defineProperty(navigator, 'requestMIDIAccess', { configurable: true, value: async () => access });
  });

  await page.goto('/');
  await page.clock.fastForward(3_100);
  const dialog = page.getByRole('dialog', { name: /MIDI connection/i });
  await expect(dialog).toBeVisible();
  // Device selection happens inside the modal itself.
  await dialog.getByLabel('Input device').selectOption('kit-b');
  await expect(dialog).toHaveCount(0);
  await expect(page.locator('#settings-dialog .midi-state')).toContainText('Kit B · connected');
  expect(await page.evaluate(() => localStorage.getItem('rudiment-midi-input'))).toBe('kit-b');
});
