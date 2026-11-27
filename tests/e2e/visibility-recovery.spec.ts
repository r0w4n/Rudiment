import { expect, test, type Page } from '@playwright/test';

// Regression coverage for issue #29: backgrounding during playback could leave
// a run that was nominally playing but silent and visually frozen after
// returning. Mobile browsers suspend the output AudioContext in the
// background; because the transport clock derives from that context, a
// returned page kept the Pause button while nothing progressed. The
// visibility policy under test:
// - hiding the page pauses a playing run cleanly;
// - returning to a page whose output context no longer runs pauses instead of
//   staying stuck, and pause/resume stay functional afterwards;
// - returning after a finite exercise's expected end reaches the normal
//   completion state instead of a lingering Pause button.

type Diagnostics = { activeRun: boolean; contextState: string };

const diagnostics = (page: Page) =>
  page.evaluate(() => (window as unknown as { rudiment: { audioDiagnostics(): Diagnostics } }).rudiment.audioDiagnostics());

// Captures the app's real AudioContext so a test can reproduce the OS-level
// suspension applied while backgrounded. Only instrumented for the page under
// test via addInitScript; it records every real-time context node creation so
// no production hook is needed.
const CAPTURE_OUTPUT_CONTEXT = `
  (() => {
    window.__rudimentOutputContexts = [];
    const prototype = window.AudioContext.prototype;
    const original = prototype.createGain;
    prototype.createGain = function (...args) {
      const contexts = window.__rudimentOutputContexts;
      if (!contexts.includes(this)) contexts.push(this);
      return original.apply(this, args);
    };
  })()
`;

// document.visibilityState is read-only, so tests shadow it with an instance
// property before dispatching the event the page reacts to. This reproduces
// the browser's transition faithfully, including the case where the event is
// never delivered (a swallowed hide event) while the state itself changed.
async function simulateVisibility(page: Page, state: 'hidden' | 'visible', deliverEvent: boolean) {
  await page.evaluate(
    ([next, deliver]) => {
      Object.defineProperty(document, 'visibilityState', { value: next, configurable: true });
      if (deliver) document.dispatchEvent(new Event('visibilitychange'));
    },
    [state, deliverEvent],
  );
}

async function startRun(page: Page) {
  await page.getByRole('button', { name: /Start (practice|performance)/ }).click();
  await expect.poll(() => diagnostics(page).then(value => value.activeRun)).toBe(true);
}

test('backgrounding during playback pauses with a defined resume position', async ({ page }) => {
  await page.addInitScript(CAPTURE_OUTPUT_CONTEXT);
  await page.goto('/');
  await startRun(page);

  await simulateVisibility(page, 'hidden', true);

  await expect(page.getByRole('button', { name: 'Pause' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Resume' })).toBeVisible();
  await expect.poll(() => diagnostics(page).then(value => value.activeRun)).toBe(false);

  // Returning must not disturb the paused state, and the run must stay
  // resumable from where it paused rather than needing a fresh start.
  await simulateVisibility(page, 'visible', true);
  await expect(page.getByRole('button', { name: 'Resume' })).toBeVisible();
  await page.getByRole('button', { name: 'Resume' }).click();
  await expect.poll(() => diagnostics(page).then(value => value.activeRun)).toBe(true);
});

test('foregrounding a run the OS suspended pauses it cleanly and playback resumes', async ({ page }) => {
  await page.addInitScript(CAPTURE_OUTPUT_CONTEXT);
  await page.goto('/');
  await startRun(page);
  await expect.poll(async () => (await diagnostics(page)).contextState).toBe('running');

  // Reproduce the mobile backgrounding failure: the OS suspended the output
  // context while the run stayed nominally playing and no visibilitychange
  // pause was deliverable. The transport clock derives from this context, so
  // the run can no longer progress on its own.
  await page.evaluate(async () => {
    const contexts = (window as unknown as { __rudimentOutputContexts: AudioContext[] }).__rudimentOutputContexts;
    await contexts.at(-1)!.suspend();
  });
  await expect.poll(async () => (await diagnostics(page)).contextState).toBe('suspended');

  // The foreground moment: a visible document whose run is stuck.
  await simulateVisibility(page, 'visible', true);

  await expect(page.getByRole('button', { name: 'Pause' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Resume' })).toBeVisible();
  await expect.poll(() => diagnostics(page).then(value => value.activeRun)).toBe(false);

  // Pause, resume and stop must remain functional after foregrounding.
  await page.getByRole('button', { name: 'Resume' }).click();
  await expect.poll(() => diagnostics(page).then(value => value.activeRun)).toBe(true);
  await expect.poll(async () => (await diagnostics(page)).contextState).toBe('running');
});

test('returning after a finite exercise ended reaches the normal completion state', async ({ page }) => {
  await page.goto('/');
  await page.locator('#visual-metronome').click();
  await page.locator('#metronome-popover').getByLabel('Tempo in BPM').fill('240');
  await page.getByLabel('Close metronome settings').click();
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();

  await page.getByRole('button', { name: /Start performance/ }).click();
  await expect.poll(() => diagnostics(page).then(value => value.activeRun)).toBe(true);

  // Background with the hide event swallowed, then let the finite exercise
  // pass its expected end while nominally backgrounded.
  await simulateVisibility(page, 'hidden', false);
  const resultDialog = page.getByRole('dialog', { name: 'Session Complete' });
  await expect(resultDialog).toBeVisible({ timeout: 15_000 });

  // Foregrounding must present the normal completion state: no Pause button,
  // no resumed run, and the transport back at its start action.
  await simulateVisibility(page, 'visible', true);
  await expect(resultDialog).toBeVisible();
  await expect(page.getByRole('button', { name: 'Pause' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Resume' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Start performance/ })).toBeVisible();
  await expect.poll(() => diagnostics(page).then(value => value.activeRun)).toBe(false);
});
