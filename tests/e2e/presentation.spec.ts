import { expect, test } from '@playwright/test';

test('Highway replaces notation, persists and keeps the transport running', async ({ page }, testInfo) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect.poll(() => page.evaluate(() => (window as any).rudiment.audioDiagnostics().currentBeat)).toBeGreaterThan(0.1);
  const before = await page.evaluate(() => (window as any).rudiment.audioDiagnostics().currentBeat);
  await page.getByRole('button', { name: 'Highway', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Highway', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('#highway svg')).toBeVisible();
  expect((await page.locator('#highway-timeline').boundingBox())!.height).toBeGreaterThan(100);
  await expect(page.locator('#notation')).toBeHidden();
  await expect(page.locator('#score-card')).toHaveCSS('background-color', 'rgb(23, 28, 25)');
  await expect.poll(() => page.evaluate(() => (window as any).rudiment.audioDiagnostics().currentBeat)).toBeGreaterThan(before);
  expect(await page.evaluate(() => (window as any).rudiment.audioDiagnostics().activeRun)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('highway-desktop.png') });
  await page.setViewportSize({ width: 1180, height: 820 });
  await expect.poll(async () => (await page.locator('#highway-timeline').boundingBox())!.width).toBeLessThan(1150);
  await page.screenshot({ path: testInfo.outputPath('highway-ipad-landscape.png') });
  await page.getByRole('button', { name: 'Notation', exact: true }).click();
  await expect(page.locator('#notation svg')).toBeVisible();
  await expect(page.locator('#highway')).toBeHidden();
  expect(await page.evaluate(() => (window as any).rudiment.audioDiagnostics().activeRun)).toBe(true);
  for (let index = 0; index < 3; index++) {
    await page.getByRole('button', { name: 'Highway', exact: true }).click();
    await expect(page.locator('#highway svg')).toHaveCount(1);
    await page.getByRole('button', { name: 'Notation', exact: true }).click();
    await expect(page.locator('#highway svg')).toHaveCount(0);
    expect(await page.evaluate(() => (window as any).rudiment.presentationActive())).toBe(false);
  }
  await page.getByRole('button', { name: 'Highway', exact: true }).click();
  await page.reload();
  await expect(page.locator('#highway svg')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Highway', exact: true })).toHaveAttribute('aria-pressed', 'true');
});

test('Highway uses the transport position beneath a fixed centre playhead', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.getByRole('button', { name: 'Highway', exact: true }).click();
  await expect(page.locator('.highway-note').first()).toBeVisible();
  await page.getByRole('button', { name: /Start practice/ }).click();
  const measure = () => page.evaluate(() => {
    const app = (window as any).rudiment;
    const viewport = document.querySelector('#highway-timeline')!.getBoundingClientRect();
    const line = document.querySelector('.highway-playhead line')!.getBoundingClientRect();
    const note = document.querySelector('.highway-note[data-note-id="p-0-2"]')!;
    const marker = note.querySelector('rect')!.getBoundingClientRect();
    return { beat: app.audioDiagnostics().currentBeat, width: viewport.width, centre: viewport.left + viewport.width / 2, line: line.left, note: marker.left + marker.width / 2, expected: Number(note.getAttribute('data-position')) };
  });
  const first = await measure();
  await page.waitForTimeout(300);
  const second = await measure();
  expect(Math.abs(first.line - first.centre)).toBeLessThan(1);
  expect(Math.abs(second.line - second.centre)).toBeLessThan(1);
  expect(second.note).toBeLessThan(first.note);
  expect(Math.abs(second.note - (second.centre + (second.expected - second.beat) * second.width / 10))).toBeLessThan(8);
  await page.getByRole('button', { name: /Pause/ }).click();
  const paused = await measure();
  await page.waitForTimeout(150);
  expect((await measure()).note).toBe(paused.note);
  await page.locator('#visual-metronome').click();
  await page.locator('#metronome-popover').getByLabel('Tempo in BPM').fill('120');
  await page.getByLabel('Close metronome settings').click();
  await page.getByRole('button', { name: /Resume/ }).click();
  await expect.poll(async () => (await measure()).note).toBeLessThan(paused.note);
  await expect.poll(() => page.evaluate(() => (window as any).rudiment.audioDiagnostics().transportBpm)).toBe(120);
  const faster = await measure();
  expect(Math.abs(faster.note - (faster.centre + (faster.expected - faster.beat) * faster.width / 10))).toBeLessThan(8);
});

test('Highway rests at the count-in position without jumping on start', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Highway', exact: true }).click();
  const position = () => page.evaluate(() => {
    const svg = document.querySelector<SVGSVGElement>('.highway-svg')!;
    const moving = svg.querySelector<SVGGElement>('.highway-moving')!;
    const width = svg.viewBox.baseVal.width;
    const x = moving.transform.baseVal.consolidate()!.matrix.e;
    return (width / 2 - x) / Math.max(80, width / 10);
  });
  await expect.poll(position).toBeCloseTo(-4, 3);
  await page.getByRole('button', { name: 'Open settings' }).click();
  const countIn = page.locator('#settings-dialog').getByLabel('Count-in');
  await countIn.selectOption('2');
  await expect.poll(position).toBeCloseTo(-8, 3);
  await countIn.selectOption('0');
  await expect.poll(position).toBeCloseTo(0, 3);
  await countIn.selectOption('1');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.getByLabel('Select bars').click();
  await page.getByLabel('Bar 3', { exact: true }).check();
  await page.getByLabel('Close bar selection').click();
  await expect.poll(position).toBeCloseTo(4, 3);
  await page.evaluate(() => {
    const svg = document.querySelector<SVGSVGElement>('.highway-svg')!;
    const moving = svg.querySelector<SVGGElement>('.highway-moving')!;
    const positions: number[] = [];
    (window as any).__highwayStartPositions = positions;
    const observer = new MutationObserver(() => {
      const width = svg.viewBox.baseVal.width;
      const x = moving.transform.baseVal.consolidate()!.matrix.e;
      positions.push((width / 2 - x) / Math.max(80, width / 10));
    });
    observer.observe(moving, { attributes: true, attributeFilter: ['transform'] });
    (window as any).__highwayStartObserver = observer;
  });
  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect.poll(() => page.evaluate(() => (window as any).rudiment.audioDiagnostics().activeRun)).toBe(true);
  await page.waitForTimeout(100);
  const observed = await page.evaluate(() => {
    (window as any).__highwayStartObserver.disconnect();
    return (window as any).__highwayStartPositions as number[];
  });
  expect(observed.length).toBeGreaterThan(0);
  expect(Math.min(...observed)).toBeGreaterThan(3.9);
  expect(Math.max(...observed)).toBeLessThan(5);
  await page.getByRole('button', { name: /Restart/ }).click();
  await expect.poll(position).toBeCloseTo(4, 3);
});

test('Highway colours the expected note directly without secondary hit markers', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Highway', exact: true }).click();
  const note = page.locator('.highway-note[data-note-id="p-0-0"][data-iteration="0"]');
  await expect(note).toBeAttached();
  const rect = note.locator('rect');
  await expect(rect).toBeAttached();
  const readStyle = () => rect.evaluate(element => {
    const style = getComputedStyle(element);
    return { fill: style.fill, stroke: style.stroke };
  });
  await expect.poll(readStyle).toEqual({
    fill: 'rgba(0, 0, 0, 0)',
    stroke: 'rgb(244, 240, 231)',
  });
  const expectedStyle = await readStyle();
  expect(expectedStyle.fill).toBe('rgba(0, 0, 0, 0)');
  expect(expectedStyle.stroke).toBe('rgb(244, 240, 231)');
  const fills: Record<string, string> = {
    perfect: 'rgb(47, 207, 136)',
    good: 'rgb(115, 217, 147)',
    early: 'rgb(239, 138, 62)',
    late: 'rgb(168, 121, 221)',
  };
  for (const judgement of ['perfect', 'good', 'early', 'late', 'missed']) {
    await page.evaluate(value => (window as any).rudiment.presentationJudgements([{ expected_id: 'p-0-0', iteration: 0, judgement: value }]), judgement);
    await expect(note).toHaveAttribute('data-judgement', judgement);
    await expect(note).toHaveAttribute('aria-label', new RegExp(judgement));
    await expect(note.locator('.highway-judgement')).toHaveCount(0);
    const style = await rect.evaluate(element => {
      const computed = getComputedStyle(element);
      return { fill: computed.fill, stroke: computed.stroke };
    });
    if (judgement === 'missed') expect(style).toEqual(expectedStyle);
    else expect(style).toEqual({ fill: fills[judgement], stroke: fills[judgement] });
  }
  await expect(note.locator('.highway-hand')).toHaveText('R');
});

test('authoritative misses remain neutral in Highway while Notation receives the same result', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.locator('#visual-metronome').click();
  await page.locator('#metronome-popover').getByLabel('Tempo in BPM').fill('240');
  await page.getByLabel('Close metronome settings').click();
  await page.getByLabel('Select bars').click();
  await page.getByLabel('Bar 1', { exact: true }).check();
  await page.getByLabel('Close bar selection').click();
  await page.getByRole('button', { name: 'Highway', exact: true }).click();
  await page.getByRole('button', { name: /Start practice/ }).click();

  const first = page.locator('.highway-note[data-note-id="p-0-0"][data-iteration="0"]');
  await expect(first).toHaveAttribute('data-judgement', 'missed', { timeout: 3000 });
  await expect(first).toHaveAttribute('aria-label', /missed/);
  await expect(first.locator('.highway-judgement')).toHaveCount(0);
  expect(await first.locator('rect').evaluate(element => {
    const style = getComputedStyle(element);
    return { fill: style.fill, stroke: style.stroke };
  })).toEqual({ fill: 'rgba(0, 0, 0, 0)', stroke: 'rgb(244, 240, 231)' });

  await page.getByRole('button', { name: 'Notation', exact: true }).click();
  await expect(page.locator('.vf-notehead[data-judgement="missed"]').first()).toBeAttached();
  expect(await page.evaluate(() => (window as any).rudiment.audioDiagnostics().activeRun)).toBe(true);
});

test('unmatched live strikes render as lane-aligned red dots and clear on restart', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.getByRole('button', { name: 'Highway', exact: true }).click();
  await page.getByRole('button', { name: /Start practice/ }).click();
  await page.evaluate(async () => {
    const app = (window as any).rudiment;
    while (app.audioDiagnostics().currentBeat < 0.24) await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true }));
    while (app.audioDiagnostics().currentBeat < 0.28) await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true }));
  });

  const dot = page.locator('.highway-unmatched-hit');
  await expect(dot).toHaveCount(2);
  await expect(dot.first()).toHaveAttribute('data-judgement', 'extra');
  await expect(dot.first()).toHaveAttribute('data-instrument', 'snare');
  await expect(dot.first()).toHaveCSS('fill', 'rgb(216, 61, 53)');
  const positions = await dot.evaluateAll(elements => elements.map(element => Number(element.getAttribute('data-position'))));
  expect(positions[0]).toBeGreaterThan(0.2);
  expect(positions[1]).toBeGreaterThan(positions[0]);
  expect(positions[1]).toBeLessThan(0.4);
  const lane = (await page.locator('.highway-lane[data-lane="snare"]').boundingBox())!;
  const marker = (await dot.first().boundingBox())!;
  expect(marker.y + marker.height / 2).toBeCloseTo(lane.y + lane.height / 2, 0);

  await page.getByRole('button', { name: 'Notation', exact: true }).click();
  await expect(page.locator('.unmatched-hit-marker')).toHaveCount(2);
  await page.getByRole('button', { name: 'Highway', exact: true }).click();
  await expect(dot).toHaveCount(2);
  await page.getByRole('button', { name: /Restart/ }).click();
  await expect(dot).toHaveCount(0);
  await expect(page.locator('.highway-note[data-judgement="expected"]')).not.toHaveCount(0);
});

test('Highway colours only the scored repetition and disposes when switched away', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.locator('#visual-metronome').click();
  await page.locator('#metronome-popover').getByLabel('Tempo in BPM').fill('240');
  await page.getByLabel('Close metronome settings').click();
  await page.getByLabel('Select bars').click();
  await page.getByLabel('Bar 1', { exact: true }).check();
  await page.getByLabel('Close bar selection').click();
  await page.getByLabel('Loops').selectOption('2');
  await page.getByRole('button', { name: 'Highway', exact: true }).click();
  await page.getByRole('button', { name: /Start practice/ }).click();
  const hit = async (target: number) => page.evaluate(async (beat) => {
    const app = (window as any).rudiment;
    while (app.audioDiagnostics().currentBeat < beat - 0.04) await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true }));
  }, target);
  await hit(1);
  const first = page.locator('.highway-note[data-note-id="p-0-2"][data-iteration="0"]');
  await expect(first).toHaveAttribute('data-judgement', /perfect|good|early|late/);
  await expect(first.locator('.highway-judgement')).toHaveCount(0);
  await expect(page.locator('.highway-unmatched-hit')).toHaveCount(0);
  await expect(first).toHaveAttribute('aria-label', /perfect|good|early|late/);
  const second = page.locator('.highway-note[data-note-id="p-0-2"][data-iteration="1"]');
  await expect(second).toHaveAttribute('data-judgement', 'expected');
  await hit(5);
  await expect(second).toHaveAttribute('data-judgement', /perfect|good|early|late/);
  const assessment = await second.getAttribute('data-judgement');
  await page.getByRole('button', { name: 'Notation', exact: true }).click();
  expect(await page.evaluate(() => (window as any).rudiment.presentationActive())).toBe(false);
  await expect(page.locator('#highway svg')).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).rudiment.audioDiagnostics().activeRun)).toBe(true);
  await page.getByRole('button', { name: 'Highway', exact: true }).click();
  await expect(second).toHaveAttribute('data-judgement', assessment!);
});

test('unsupported instruments are announced and a later snare exercise remounts', async ({ page }) => {
  const track = [0, 0x99, 51, 100, 0x83, 0x60, 0x99, 38, 100, 0x83, 0x60, 0xff, 0x2f, 0];
  const midi = Buffer.from([0x4d, 0x54, 0x68, 0x64, 0, 0, 0, 6, 0, 0, 0, 1, 1, 0xe0, 0x4d, 0x54, 0x72, 0x6b, 0, 0, 0, track.length, ...track]);
  await page.route('**/unsupported-highway.mid', route => route.fulfill({ status: 200, body: midi, contentType: 'audio/midi' }));
  await page.goto('/');
  await page.getByRole('button', { name: 'Highway', exact: true }).click();
  await expect(page.locator('#highway svg')).toBeVisible();
  await page.evaluate(() => (window as any).rudiment.loadBundledFromLibrary('unsupported-highway.mid', 'Unsupported drums'));
  await expect(page.locator('#highway-error')).toContainText('ride');
  await expect(page.locator('#highway-error')).toBeVisible();
  await expect(page.locator('#highway svg')).toHaveCount(0);
  await page.evaluate(() => (window as any).rudiment.loadBundledFromLibrary('public/exercises/rudiments/06_single_paradiddle.mid', 'Single Paradiddle'));
  await expect(page.locator('#highway-error')).toBeHidden();
  await expect(page.locator('#highway svg')).toBeVisible();
});

test('Highway renders simultaneous kit notes in kick, snare and hi-hat lanes', async ({ page }) => {
  const track = [
    0, 0x99, 36, 100,
    0, 0x99, 38, 100,
    0, 0x99, 42, 100,
    0x81, 0x70, 0x99, 46, 100,
    0, 0xff, 0x2f, 0,
  ];
  const midi = Buffer.from([0x4d, 0x54, 0x68, 0x64, 0, 0, 0, 6, 0, 0, 0, 1, 1, 0xe0, 0x4d, 0x54, 0x72, 0x6b, 0, 0, 0, track.length, ...track]);
  await page.route('**/kit-highway.mid', route => route.fulfill({ status: 200, body: midi, contentType: 'audio/midi' }));
  await page.goto('/');
  await page.getByRole('button', { name: 'Highway', exact: true }).click();
  await page.evaluate(() => (window as any).rudiment.loadBundledFromLibrary('kit-highway.mid', 'Kit Highway'));
  await expect(page.locator('#highway-error')).toBeHidden();
  await expect(page.locator('.highway-note[data-instrument="kick"]')).toHaveCount(1);
  await expect(page.locator('.highway-note[data-instrument="snare"]')).toHaveCount(1);
  await expect(page.locator('.highway-note[data-instrument="closed_hat"][data-lane="hi_hat"]')).toHaveCount(1);
  await expect(page.locator('.highway-note[data-instrument="open_hat"][data-articulation="open"]')).toHaveCount(1);
  const laneY = await page.locator('.highway-note').evaluateAll(notes => Object.fromEntries(notes.map(note => {
    const transform = note.getAttribute('transform') ?? '';
    return [note.getAttribute('data-instrument'), Number(transform.match(/ ([\d.]+)\)$/)?.[1])];
  })));
  expect(laneY.kick).not.toBe(laneY.snare);
  expect(laneY.snare).not.toBe(laneY.closed_hat);
});

test('Highway header and timing legend remain usable at narrow widths', async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Highway', exact: true }).click();
  const selector = (await page.locator('.visualisation-selector').boundingBox())!;
  const metronome = (await page.locator('#visual-metronome').boundingBox())!;
  expect(metronome.x + metronome.width).toBeLessThanOrEqual(selector.x);
  await expect(page.getByLabel('Timing feedback key')).toBeVisible();
  await expect(page.getByLabel('Timing feedback key')).toBeInViewport();
  expect((await page.getByRole('button', { name: 'Highway', exact: true }).boundingBox())!.height).toBeGreaterThanOrEqual(44);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(375);
});

test('Highway repositions stable note IDs when a noncontiguous selection changes', async ({ page }) => {
  await page.goto('/');
  // Keep the later selected bar in the visible range while testing mapping.
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.getByRole('button', { name: 'Highway', exact: true }).click();
  await page.getByLabel('Select bars').click();
  await page.getByLabel('Bar 1', { exact: true }).check();
  await page.getByLabel('Bar 3', { exact: true }).check();
  const note = page.locator('.highway-note[data-note-id="p-2-0"][data-iteration="0"]');
  await expect(note).toHaveAttribute('data-position', '4');
  await page.getByLabel('Bar 1', { exact: true }).uncheck();
  await expect(note).toHaveAttribute('data-position', '8');
  await page.getByLabel('Close bar selection').click();
});

test('grace strokes remain distinct without changing their onset positions', async ({ page }, testInfo) => {
  await page.goto('/');
  await page.evaluate(() => (window as any).rudiment.loadBundledFromLibrary('public/exercises/rudiments/05_hard_flams.mid', 'Hard Flams'));
  await expect(page.locator('.exercise-heading strong')).toHaveText('Hard Flams');
  await page.getByRole('button', { name: 'Highway', exact: true }).click();
  await expect(page.locator('.highway-note[data-grace="true"]').first()).toBeVisible();
  const pairs = await page.evaluate(() => {
    const notes = [...document.querySelectorAll('.highway-note')];
    return notes.flatMap((note, index) => {
      const next = notes[index + 1];
      if (note.getAttribute('data-grace') !== 'true' || !next || next.getAttribute('data-grace') !== 'false') return [];
      const gap = Number(next.getAttribute('data-position')) - Number(note.getAttribute('data-position'));
      if (gap <= 0 || gap >= 0.15) return [];
      const grace = note.querySelector('rect')!.getBoundingClientRect();
      const principal = next.querySelector('rect')!.getBoundingClientRect();
      return [{ gap, xGap: principal.x - grace.x, verticalGap: principal.top - grace.bottom }];
    });
  });
  expect(pairs.length).toBeGreaterThan(0);
  for (const pair of pairs) {
    expect(pair.xGap).toBeGreaterThan(0);
    expect(pair.verticalGap).toBeGreaterThanOrEqual(0);
  }
  const graceNote = page.locator('.highway-note[data-grace="true"]').first();
  const graceId = await graceNote.getAttribute('data-note-id');
  await page.evaluate(id => (window as any).rudiment.presentationJudgements([{ expected_id: id, iteration: 0, judgement: 'good' }]), graceId);
  await expect(graceNote).toHaveAttribute('data-judgement', 'good');
  await expect(graceNote.locator('.highway-judgement')).toHaveCount(0);
  await expect(graceNote.locator('rect')).toHaveCSS('fill', 'rgb(115, 217, 147)');
  await page.screenshot({ path: testInfo.outputPath('highway-flams.png') });
  await page.evaluate(() => (window as any).rudiment.loadBundledFromLibrary('public/exercises/rudiments/09_single_drag.mid', 'Single Drag'));
  await expect(page.locator('.exercise-heading strong')).toHaveText('Single Drag');
  await expect(page.locator('.highway-note[data-grace-row="2"]').first()).toBeVisible();
  const drag = await page.evaluate(() => {
    const notes = [...document.querySelectorAll('.highway-note[data-grace="true"]')];
    return notes.flatMap((note, index) => {
      const next = notes[index + 1];
      if (!next || Number(next.getAttribute('data-position')) - Number(note.getAttribute('data-position')) >= 0.15) return [];
      const first = note.querySelector('rect')!.getBoundingClientRect();
      const second = next.querySelector('rect')!.getBoundingClientRect();
      return [{ xGap: second.x - first.x, verticalGap: first.top - second.bottom }];
    });
  });
  expect(drag.length).toBeGreaterThan(0);
  for (const pair of drag) {
    expect(pair.xGap).toBeGreaterThan(0);
    expect(pair.verticalGap).toBeGreaterThanOrEqual(0);
  }
});

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
  // Leave enough wall-clock time between the loop boundary and its first note
  // for a contended CI renderer to observe the new iteration before striking.
  const tempo = page.locator('#metronome-popover').getByLabel('Tempo in BPM');
  await tempo.fill('60');
  await tempo.press('Enter');
  await page.getByLabel('Close metronome settings').click();
  await page.getByLabel('Select bars').click();
  await page.getByLabel('Bar 1', { exact: true }).check();
  await page.getByLabel('Bar 3', { exact: true }).check();
  await page.getByLabel('Close bar selection').click();
  await page.getByLabel('Loops').selectOption('2');
  await expect.poll(() => page.evaluate(() => (window as any).__presentation.model.timeline.bars)).toEqual([0, 2]);
  await expect.poll(() => page.evaluate(() => (window as any).__presentation.model.playback.tempoBpm)).toBe(60);
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
  await expect.poll(() => page.evaluate(() => (window as any).__presentation.model.timeline.iteration), { timeout: 10_000 }).toBe(1);
  // The first note in the new loop may already be a legitimate live miss;
  // feedback from iteration zero must not colour the still-future target.
  expect(await page.evaluate(() => (window as any).__presentation.model.notes.find((note: any) => note.id === 'p-0-2' && note.iteration === 1)?.judgement)).toBeNull();
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
