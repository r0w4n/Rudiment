import { expect, test } from '@playwright/test';

test('real transport, noncontiguous selection and Rust feedback reach a mounted renderer', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('#notation svg')).toBeVisible();
  await page.evaluate(() => {
    const app = (window as any).rudiment;
    const state = { updates: 0, disposed: 0, initialized: 0, model: null as any, handle: null as any };
    state.handle = app.mountPresentationRenderer({
      initialize() { state.initialized++; },
      update(model: unknown) { state.model = model; state.updates++; },
      resize() {},
      dispose() { state.disposed++; },
    });
    (window as any).__presentation = state;
  });
  await expect.poll(() => page.evaluate(() => (window as any).__presentation.model?.notes.length)).toBeGreaterThan(0);
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.locator('#visual-metronome').click();
  await page.locator('#metronome-popover').getByLabel('Tempo in BPM').fill('120');
  await page.getByLabel('Close metronome settings').click();
  await page.getByLabel('Select bars').click();
  await page.getByLabel('Bar 1', { exact: true }).check();
  await page.getByLabel('Bar 3', { exact: true }).check();
  await page.getByLabel('Close bar selection').click();
  await page.getByLabel('Loops').selectOption('2');
  await expect.poll(() => page.evaluate(() => (window as any).__presentation.model.timeline.bars)).toEqual([0, 2]);
  await expect.poll(() => page.evaluate(() => (window as any).__presentation.model.playback.tempoBpm)).toBe(120);
  const mapped = await page.evaluate(() => (window as any).__presentation.model.notes.find((note: any) => note.id === 'p-2-0'));
  expect(mapped.beat).toBe(8);
  expect(mapped.position).toBe(4);
  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__presentation.model.playback.phase)).toBe('running');
  const hit = await page.evaluate(async () => {
    const app = (window as any).rudiment, state = (window as any).__presentation;
    const next = () => new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
    while (app.audioDiagnostics().currentBeat < 0.96) await next();
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true }));
    for (let frame = 0; frame < 20; frame++) {
      await next();
      const note = state.model.notes.find((item: any) => item.id === 'p-0-2' && item.iteration === 0);
      const notation = document.querySelector('.score-note[data-beat="1"]')?.getAttribute('data-judgement');
      if (note?.judgement && notation) return { note, notation };
    }
    return null;
  });
  expect(hit).not.toBeNull();
  if (!hit) throw new Error('No matching live feedback');
  expect(hit.note.judgement).toMatch(/perfect|good|early|late/);
  expect(hit.note.judgement).toBe(hit.notation);
  await expect.poll(() => page.evaluate(() => (window as any).__presentation.model.timeline.iteration), { timeout: 6000 }).toBe(1);
  expect(await page.evaluate(() => (window as any).__presentation.model.notes.some((note: any) => note.iteration === 1 && note.judgement))).toBe(false);
  const repeatedHit = await page.evaluate(async () => {
    const app = (window as any).rudiment, state = (window as any).__presentation;
    const next = () => new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
    while (app.audioDiagnostics().currentBeat < 8.96) await next();
    if (app.audioDiagnostics().currentBeat > 9.15) throw new Error('Missed repeated hit window');
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true }));
    for (let frame = 0; frame < 20; frame++) {
      await next();
      const note = state.model.notes.find((item: any) => item.id === 'p-0-2' && item.iteration === 1);
      const notation = document.querySelector('.score-note[data-beat="1"]')?.getAttribute('data-judgement');
      if (note?.judgement && notation) return { note, notation };
    }
    return null;
  });
  expect(repeatedHit).not.toBeNull();
  if (!repeatedHit) throw new Error('No matching repeated-loop feedback');
  expect(repeatedHit.note.judgement).toMatch(/perfect|good|early|late/);
  expect(repeatedHit.note.judgement).toBe(repeatedHit.notation);
  await page.getByRole('button', { name: /Pause/ }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__presentation.model.playback.phase)).toBe('paused');
  const paused = await page.evaluate(() => (window as any).__presentation.model.position);
  await page.waitForTimeout(150);
  expect(await page.evaluate(() => (window as any).__presentation.model.position)).toBe(paused);
  await page.getByRole('button', { name: /Resume/ }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__presentation.model.position)).toBeGreaterThan(paused);
  await page.evaluate(() => (window as any).__presentation.handle.dispose());
  const updates = await page.evaluate(() => (window as any).__presentation.updates);
  await page.waitForTimeout(150);
  expect(await page.evaluate(() => (window as any).__presentation.updates)).toBe(updates);
  expect(await page.evaluate(() => (window as any).__presentation.disposed)).toBe(1);
  expect(await page.evaluate(() => (window as any).rudiment.audioDiagnostics().activeRun)).toBe(true);
});

test('a late-mounted or failing renderer does not own the transport', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect.poll(() => page.evaluate(() => (window as any).rudiment.audioDiagnostics().currentBeat)).toBeGreaterThan(0.1);
  const result = await page.evaluate(() => {
    const app = (window as any).rudiment;
    const state = { model: null as any, failed: false, disposed: 0, viewport: null as any };
    (window as any).__latePresentation = state;
    const handle = app.mountPresentationRenderer({
      initialize() {},
      update(model: unknown) { if (state.failed) throw new Error('test renderer failure'); state.model = model; },
      resize(viewport: unknown) { state.viewport = viewport; },
      dispose() { state.disposed++; },
    });
    handle.resize({ width: 800, height: 300 });
    const beat = app.audioDiagnostics().currentBeat;
    const position = state.model.position;
    let duplicateRejected = false;
    try { app.mountPresentationRenderer({}); } catch { duplicateRejected = true; }
    state.failed = true;
    return { beat, position, duplicateRejected };
  });
  expect(Math.abs(result.position - result.beat)).toBeLessThan(0.1);
  expect(result.duplicateRejected).toBe(true);
  await expect.poll(() => page.evaluate(() => (window as any).__latePresentation.disposed)).toBe(1);
  expect(await page.evaluate(() => (window as any).__latePresentation.viewport)).toEqual({ width: 800, height: 300 });
  expect(await page.evaluate(() => (window as any).rudiment.presentationActive())).toBe(false);
  await expect.poll(() => page.evaluate(() => (window as any).rudiment.audioDiagnostics().currentBeat)).toBeGreaterThan(result.beat);
  expect(await page.evaluate(() => (window as any).rudiment.audioDiagnostics().activeRun)).toBe(true);
});

test('paused selection seeks through the normal app flow and restart resets the presentation', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('#notation svg')).toBeVisible();
  await page.evaluate(() => {
    const state = { model: null as any };
    (window as any).__presentation = state;
    (window as any).rudiment.mountPresentationRenderer({ initialize() {}, update(model: unknown) { state.model = model; }, resize() {}, dispose() {} });
  });
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__presentation.model.position)).toBeGreaterThan(0.1);
  await page.getByRole('button', { name: /Pause/ }).click();
  await page.getByLabel('Select bars').click();
  await page.getByLabel('Bar 3', { exact: true }).check();
  await page.getByLabel('Close bar selection').click();
  await page.getByRole('button', { name: 'Start selected bars' }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__presentation.model.position)).toBeGreaterThanOrEqual(8);
  await expect.poll(() => page.evaluate(() => (window as any).__presentation.model.timeline.bars)).toEqual([2]);
  await page.getByRole('button', { name: /Restart/ }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__presentation.model.playback.phase)).toBe('idle');
  expect(await page.evaluate(() => (window as any).__presentation.model.position)).toBe(8);
  expect(await page.evaluate(() => (window as any).__presentation.model.notes.some((note: any) => note.judgement))).toBe(false);
});

test('simulated Web MIDI still reaches the shared scorer and notation while a renderer is mounted', async ({ page }) => {
  await page.addInitScript(() => {
    const input = {
      id: 'presentation-kit', name: 'Presentation Kit', manufacturer: 'Rudiment', type: 'input', state: 'connected', connection: 'open',
      onmidimessage: null as null | ((event: unknown) => void), onstatechange: null,
      open: async () => input, close: async () => input, addEventListener: () => {}, removeEventListener: () => {},
      dispatch: (data: number[]) => input.onmidimessage?.({ data: new Uint8Array(data), timeStamp: performance.now() }),
    };
    const access = { inputs: new Map([[input.id, input]]), outputs: new Map(), sysexEnabled: false, onstatechange: null, addEventListener: () => {}, removeEventListener: () => {} };
    Object.defineProperty(navigator, 'requestMIDIAccess', { configurable: true, value: async () => access });
    Object.defineProperty(navigator, 'permissions', { configurable: true, value: { query: async () => ({ state: 'granted', onchange: null }) } });
    (window as any).__presentationInput = input;
  });
  await page.goto('/');
  await expect(page.locator('#notation svg')).toBeVisible();
  await page.evaluate(() => {
    const state = { model: null as any };
    (window as any).__presentation = state;
    (window as any).rudiment.mountPresentationRenderer({ initialize() {}, update(model: unknown) { state.model = model; }, resize() {}, dispose() {} });
  });
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await expect.poll(() => page.evaluate(() => typeof (window as any).__presentationInput.onmidimessage)).toBe('function');
  await page.getByRole('button', { name: /Start practice/ }).click();
  const result = await page.evaluate(async () => {
    const app = (window as any).rudiment, state = (window as any).__presentation;
    const next = () => new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
    while (app.audioDiagnostics().currentBeat < 0.96) await next();
    (window as any).__presentationInput.dispatch([0x99, 38, 100]);
    for (let frame = 0; frame < 20; frame++) {
      await next();
      const note = state.model.notes.find((item: any) => item.id === 'p-0-2' && item.iteration === 0);
      const notation = document.querySelector('.score-note[data-beat="1"]')?.getAttribute('data-judgement');
      if (note?.judgement && notation) return { judgement: note.judgement, notation };
    }
    return null;
  });
  expect(result).not.toBeNull();
  if (!result) throw new Error('No matching Web MIDI feedback');
  expect(result.judgement).toMatch(/perfect|good|early|late/);
  expect(result.judgement).toBe(result.notation);
});
