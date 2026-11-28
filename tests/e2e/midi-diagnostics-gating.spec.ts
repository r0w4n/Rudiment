import { test, expect } from '@playwright/test';

// MIDI diagnostics must stay dormant while every diagnostics view is hidden:
// no diagnostic-list DOM mutation, no reactive publication, and no extra
// formatting while raw messages stream in. Musical messages (note-on scoring)
// must never be discarded for that. Reopening the view must show the traffic
// received while it was hidden, bounded to the ring length.

const MIDI_ENVIRONMENT = () => {
  const input = {
    id: 'fake-kit-1', name: 'Acceptance Kit', manufacturer: 'Rudiment Test', type: 'input',
    state: 'connected', connection: 'open',
    onmidimessage: null as null | ((event: unknown) => void), onstatechange: null as null | ((event: unknown) => void),
    open: async () => input, close: async () => input,
    addEventListener: () => {}, removeEventListener: () => {},
    dispatch: (data: number[], timestamp = performance.now()) => input.onmidimessage?.({ data: new Uint8Array(data), timeStamp: timestamp }),
  };
  const access = {
    inputs: new Map([[input.id, input]]), outputs: new Map(), sysexEnabled: false,
    onstatechange: null as null | ((event: unknown) => void),
    addEventListener: () => {}, removeEventListener: () => {},
  };
  Object.defineProperty(navigator, 'requestMIDIAccess', { configurable: true, value: async () => access });
  Object.defineProperty(navigator, 'permissions', { configurable: true, value: { query: async () => ({ state: 'prompt', onchange: null }) } });
  Object.assign(window, { __rudimentFakeInput: input });
};

const openDiagnostics = async (page: import('@playwright/test').Page) => {
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.locator('#settings-dialog').getByText('MIDI diagnostics').click();
  await expect(page.locator('#settings-dialog .diagnostics code').first()).toBeVisible();
};

test('hidden MIDI diagnostics perform no diagnostic-list mutations while messages stream', async ({ page }) => {
  await page.addInitScript(MIDI_ENVIRONMENT);
  await page.goto('/');
  await expect(page.locator('#settings-dialog .midi-state')).toContainText('Acceptance Kit · connected');

  const startsDelta = async (body: (input: { dispatch(data: number[], timestamp?: number): void }) => void, settle = 300) =>
    page.evaluate(async ({ bodySource, settle }) => {
      const input = (window as unknown as { __rudimentFakeInput: { dispatch(data: number[], timestamp?: number): void } }).__rudimentFakeInput;
      const audioDiagnostics = () => (window as unknown as { rudiment: { audioDiagnostics(): { snareStarts: number } } }).rudiment.audioDiagnostics();
      const startsBefore = audioDiagnostics().snareStarts;
      (0, eval)(bodySource)(input);
      await new Promise(resolve => setTimeout(resolve, settle));
      return audioDiagnostics().snareStarts - startsBefore;
    }, { bodySource: `(${body.toString()})`, settle });

  // Calibrate how many audio starts one note-on produces in this build, so
  // the assertion below is relative and stays exact.
  const perNoteOn = await startsDelta(input => input.dispatch([0x99, 38, 100]));
  expect(perNoteOn).toBeGreaterThan(0);

  // A mixed burst of note-on/off and control-change messages with the drawer
  // closed must not mutate either diagnostics view (drawer or modal), while
  // the note-ons still reach the scoring pipeline and the non-attacks stay
  // filtered out of scoring.
  const hidden = await page.evaluate(async ({ dispatchSource, perNoteOn }) => {
    const input = (window as unknown as { __rudimentFakeInput: { dispatch(data: number[], timestamp?: number): void } }).__rudimentFakeInput;
    const observers: MutationObserver[] = [];
    let mutations = 0;
    const record = (root: Element) => {
      const observer = new MutationObserver(records => { mutations += records.length; });
      observer.observe(root, { childList: true, subtree: true, characterData: true });
      observers.push(observer);
    };
    // Both hidden diagnostics consumers.
    document.querySelectorAll('.diagnostics').forEach(record);
    const audioDiagnostics = () => (window as unknown as { rudiment: { audioDiagnostics(): { snareStarts: number } } }).rudiment.audioDiagnostics();
    const startsBefore = audioDiagnostics().snareStarts;
    for (let index = 0; index < 20; index += 1) {
      input.dispatch([0x99, 38, 100]);
      input.dispatch([0x89, 38, 0]);
      input.dispatch([0xb9, 4, 90]);
    }
    await new Promise(resolve => setTimeout(resolve, 400));
    observers.forEach(observer => observer.disconnect());
    return { mutations, scoredHits: audioDiagnostics().snareStarts - startsBefore, perNoteOn };
  }, { dispatchSource: '', perNoteOn });
  expect(hidden.mutations).toBe(0);
  expect(hidden.scoredHits).toBe(20 * perNoteOn);

  // Control-change and note-off traffic alone never reaches scoring.
  const nonAttacks = await startsDelta(input => {
    for (let index = 0; index < 20; index += 1) {
      input.dispatch([0x89, 38, 0]);
      input.dispatch([0xb9, 4, 90]);
    }
  });
  expect(nonAttacks).toBe(0);
});

test('visible diagnostics stay bounded, show hidden traffic and republish while open', async ({ page }) => {
  await page.addInitScript(MIDI_ENVIRONMENT);
  await page.goto('/');
  await expect(page.locator('#settings-dialog .midi-state')).toContainText('Acceptance Kit · connected');

  // Traffic received while the view is hidden is retained in the bounded
  // non-reactive ring and appears when the view is opened.
  await page.evaluate(async () => {
    const input = (window as unknown as { __rudimentFakeInput: { dispatch(data: number[], timestamp?: number): void } }).__rudimentFakeInput;
    for (let index = 0; index < 15; index += 1) {
      input.dispatch([0x99, 38, 100 + index]);
    }
    await new Promise(resolve => setTimeout(resolve, 150));
  });
  await openDiagnostics(page);
  const rows = page.locator('#settings-dialog .diagnostics code');
  await expect(rows).toHaveCount(12);
  await expect(rows.first()).toContainText('velocity 114');

  // While visible, further messages keep appearing (bounded rate).
  await page.evaluate(() => {
    (window as unknown as { __rudimentFakeInput: { dispatch(data: number[]): void } }).__rudimentFakeInput.dispatch([0x99, 38, 55]);
  });
  await expect(rows.first()).toContainText('velocity 55');
  await expect(rows).toHaveCount(12);

  // Reopening after closing the disclosure shows the recent traffic again.
  await page.locator('#settings-dialog').getByText('MIDI diagnostics').click();
  await expect(page.locator('#settings-dialog .diagnostics code')).toHaveCount(0);
  await page.locator('#settings-dialog').getByText('MIDI diagnostics').click();
  await expect(page.locator('#settings-dialog .diagnostics code').first()).toContainText('velocity 55');
});
