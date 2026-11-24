import { expect, test } from '@playwright/test';

// Regression tests for the visual metronome's beat-time correctness:
// - the sweep needle must never fall back to its CSS centre during the
//   scheduler look-ahead (the forward re-anchor seek produced a visible
//   centre-hold then snap at every even beat);
// - the beat-dot pulses must arm exactly on integer beats (subdivision
//   steps used to re-seek the same dot's pulse, sliding it up to three
//   quarters of a beat late at sixteenth-note subdivision);
// - the current-dot highlight must be pre-armed on the animation timeline
//   at the scheduled beat instead of toggled at frame time.

async function startPractice(page: any, options: { subdivision?: string; bpm?: string } = {}) {
  await page.goto('/');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.locator('#visual-metronome').click();
  if (options.bpm) {
    await page.locator('#metronome-popover').getByLabel('Tempo in BPM').fill(options.bpm);
  }
  if (options.subdivision) {
    await page.locator('#metronome-popover').getByLabel('Subdivision').selectOption(options.subdivision);
  }
  await page.getByLabel('Close metronome settings').click();
  await page.getByRole('button', { name: /Start practice/ }).click();
}

test('sweep needle never snaps back to centre between beats', async ({ page }) => {
  await startPractice(page, { bpm: '120' });
  await expect(page.locator('#visual-metronome')).toHaveClass(/active/);
  // Sample the needle's horizontal position every animation frame once the
  // pendulum is running (after the first scheduled beat). The defect held
  // the needle exactly at the CSS centre for the whole ~100 ms look-ahead
  // before every even beat (about a tenth of every cycle at this tempo); a
  // healthy pendulum only passes through the centre at speed.
  const samples = await page.evaluate(() => new Promise<number[]>(resolve => {
    const root = document.querySelector('#visual-metronome') as HTMLElement;
    const needle = document.querySelector('.visual-metronome-sweep i') as HTMLElement;
    const sweep = document.querySelector('.visual-metronome-sweep') as HTMLElement;
    const rect = sweep.getBoundingClientRect();
    const centre = rect.left + rect.width / 2;
    const waitRunning = () => {
      if (!root.getAttribute('data-beat')) { requestAnimationFrame(waitRunning); return; }
      // Skip the audio warm-up window: the very first scheduled beat can
      // straddle the AudioContext's cold start, which is unrelated to the
      // steady-state needle motion this test verifies.
      setTimeout(() => {
        const xs: number[] = [];
        const startedAt = performance.now();
        const tick = () => {
          if (performance.now() - startedAt >= 1800) { resolve(xs); return; }
          xs.push(needle.getBoundingClientRect().left + needle.getBoundingClientRect().width / 2 - centre);
          requestAnimationFrame(tick);
        };
        requestAnimationFrame(tick);
      }, 1200);
    };
    waitRunning();
  }));
  expect(samples.length).toBeGreaterThan(40);
  const travel = Math.max(...samples.map(Math.abs));
  expect(travel).toBeGreaterThan(8);
  const atCentre = samples.filter(x => Math.abs(x) <= 1).length;
  expect(atCentre / samples.length).toBeLessThan(0.02);
});

test('beat dot pulses arm exactly on integer beats under sixteenth subdivision', async ({ page }) => {
  await startPractice(page, { bpm: '120', subdivision: '4' });
  const result = await page.evaluate(() => new Promise(resolve => {
    const root = document.querySelector('#visual-metronome') as HTMLElement;
    const steps = new Map<number, number>();
    const observer = new MutationObserver(() => {
      steps.set(Number(root.getAttribute('data-pulse')), Number(root.getAttribute('data-scheduled-at')));
    });
    observer.observe(root, { attributes: true, attributeFilter: ['data-pulse'] });
    setTimeout(() => {
      observer.disconnect();
      const dots = [...root.querySelectorAll('.visual-metronome-beats i')] as (HTMLElement & { pulseAnimation?: Animation })[];
      const times = [...steps.entries()].sort((a, b) => a[0] - b[0]);
      resolve({ times, pulses: dots.map(dot => dot.pulseAnimation ? dot.pulseAnimation.startTime : null) });
    }, 2400);
  }));
  const { times, pulses } = result as { times: Array<[number, number]>; pulses: Array<number | null> };
  expect(times.length).toBeGreaterThan(8);
  expect(pulses.some(start => start !== null)).toBe(true);
  // One-beat duration from the STEADY-STATE beats (early beats can straddle
  // audio warm-up), using the median delta to reject outliers.
  const subdivision = 4;
  const beatTimes = times.filter(([step]) => step % subdivision === 0);
  expect(beatTimes.length).toBeGreaterThan(3);
  const deltas = beatTimes.slice(1).map(([, t], i) => t - beatTimes[i][1]).sort((a, b) => a - b);
  const beatMs = deltas[Math.floor(deltas.length / 2)];
  const firstBeatTime = beatTimes[beatTimes.length - 4][1];
  for (const startTime of pulses) {
    if (startTime === null) continue;
    const beats = (startTime - firstBeatTime) / beatMs;
    // The arm must land on a whole beat, within scheduling tolerance.
    expect(Math.abs(beats - Math.round(beats)) * beatMs).toBeLessThan(10);
  }
});

test('current-dot highlight is pre-armed at the scheduled beat', async ({ page }) => {
  await startPractice(page, { bpm: '120' });
  const result = await page.evaluate(() => new Promise(resolve => {
    const root = document.querySelector('#visual-metronome') as HTMLElement;
    const steps = new Map<number, number>();
    const observer = new MutationObserver(() => {
      steps.set(Number(root.getAttribute('data-pulse')), Number(root.getAttribute('data-scheduled-at')));
    });
    observer.observe(root, { attributes: true, attributeFilter: ['data-pulse'] });
    setTimeout(() => {
      observer.disconnect();
      const dots = [...root.querySelectorAll('.visual-metronome-beats i')] as (HTMLElement & { lightAnimation?: Animation })[];
      resolve({
        times: [...steps.entries()].sort((a, b) => a[0] - b[0]),
        lights: dots.map(dot => dot.lightAnimation ? dot.lightAnimation.startTime : null),
      });
    }, 2400);
  }));
  const { times, lights } = result as { times: Array<[number, number]>; lights: Array<number | null> };
  expect(lights.some(start => start !== null)).toBe(true);
  const beatTimes = times.filter(([step]) => step % 1 === 0);
  const deltas = beatTimes.slice(1).map(([, t], i) => t - beatTimes[i][1]).sort((a, b) => a - b);
  const beatMs = deltas[Math.floor(deltas.length / 2)];
  const anchorTime = beatTimes[beatTimes.length - 4][1];
  for (const startTime of lights) {
    if (startTime === null) continue;
    const beats = (startTime - anchorTime) / beatMs;
    expect(Math.abs(beats - Math.round(beats)) * beatMs).toBeLessThan(10);
  }
  // The highlight settles on exactly one dot, as before.
  await expect(page.locator('.visual-metronome-beats i.current')).toHaveCount(1);
});
