import { test, expect } from './fixtures';
import type { Page } from '@playwright/test';

type Rudiment = {
  renderScore(exercise: unknown): void;
  updateScore(beat: number, items: unknown[], loopAware?: boolean, leadInBeat?: number | null): void;
  nextDownbeatScoreBeat(beat: number, options: Record<string, unknown>): number | null;
};

const renderRestTrailExercise = (page: Page) =>
  page.evaluate(() => {
    const hits = Array.from({ length: 3 }, (_, bar) => ([
      { beat: bar * 4, notation_beat: bar * 4, notation_duration_beats: 1, instrument: 'snare', note: 38, hand: 'R' },
      { beat: bar * 4 + 1, notation_beat: bar * 4 + 1, notation_duration_beats: 1, instrument: 'snare', note: 38, hand: 'L' },
    ])).flat();
    (window as unknown as { rudiment: Rudiment }).rudiment.renderScore({ title: 'System bounds', meter_numerator: 4, meter_denominator: 4, hits });
  });

const app = (page: Page) => page.evaluateHandle(() => (window as unknown as { rudiment: Rudiment }).rudiment);

test('playhead stays inside each system and ends at its barline', async ({ page }) => {
  await page.goto('/');
  await renderRestTrailExercise(page);
  const bounds = await page.evaluate(async (handle) => {
    const rudiment = handle as unknown as Rudiment;
    const svg = document.querySelector('#notation svg')!;
    const barlines = Array.from(svg.querySelectorAll('g.vf-stave')).map((group) => {
      const box = (group as SVGGraphicsElement).getBBox();
      return box.x + box.width;
    });
    const bars = [0, 1, 2].map((bar) => {
      let max = -Infinity;
      let end = 0;
      for (let step = 0; step < 100; step++) {
        const beat = bar * 4 + Math.min(3.999, step * 0.04);
        rudiment.updateScore(beat, [], true);
        const x = Number(document.querySelector('#playhead')?.getAttribute('data-x'));
        max = Math.max(max, x);
        end = x;
      }
      rudiment.updateScore(bar * 4 + 3.999, [], true);
      end = Number(document.querySelector('#playhead')?.getAttribute('data-x'));
      return { max, end };
    });
    return { barlines, bars };
  }, await app(page));
  for (const [bar, entry] of bounds.bars.entries()) {
    expect(entry.max, `bar ${bar} playhead must not pass its barline`).toBeLessThanOrEqual(bounds.barlines[bar] + .5);
    expect(entry.end, `bar ${bar} playhead must reach its barline at the final beat`).toBeCloseTo(bounds.barlines[bar], 0);
  }
});

test('lead-in preview marks the next downbeat without moving the playhead', async ({ page }) => {
  await page.goto('/');
  await renderRestTrailExercise(page);
  const preview = page.locator('.playhead-preview');
  // Mid-bar: no preview.
  await page.evaluate((handle) => (handle as unknown as Rudiment).updateScore(1.5, [], true, 4), await app(page));
  await expect(preview).not.toHaveClass(/visible/);
  // Final beat of the first system: the preview marks the next system's first
  // beat while the active playhead stays inside the current bar.
  const leadIn = await page.evaluate((handle) => {
    const rudiment = handle as unknown as Rudiment;
    // Capture where the playhead will appear at the downbeat...
    rudiment.updateScore(4, [], true, null);
    const playhead = document.querySelector('#playhead')!;
    const playheadX = playhead.getAttribute('data-x');
    const playheadRow = playhead.getAttribute('data-row');
    const lineY1 = playhead.getAttribute('y1');
    const lineY2 = playhead.getAttribute('y2');
    // ...then step back into the final beat of the previous system.
    rudiment.updateScore(3.2, [], true, 4);
    const element = document.querySelector('.playhead-preview')!;
    return {
      transform: (element as HTMLElement).style.transform,
      top: (element as HTMLElement).style.top,
      height: (element as HTMLElement).style.height,
      playheadX,
      playheadRow,
      lineY1,
      lineY2,
    };
  }, await app(page));
  await expect(preview).toHaveClass(/visible/);
  expect(leadIn.playheadRow).toBe('1');
  expect(Number(leadIn.transform.match(/translate3d\(([-\d.]+)px/)?.[1])).toBeCloseTo(Number(leadIn.playheadX), 3);
  expect(leadIn.top).toBe(`${leadIn.lineY1}px`);
  expect(leadIn.height).toBe(`${Number(leadIn.lineY2) - Number(leadIn.lineY1)}px`);
  // Once the downbeat arrives the preview disappears.
  await page.evaluate((handle) => (handle as unknown as Rudiment).updateScore(4.01, [], true, 8), await app(page));
  await expect(preview).not.toHaveClass(/visible/);
});

test('lead-in preview follows the played bar sequence across selections and loop wraps', async ({ page }) => {
  await page.goto('/');
  await renderRestTrailExercise(page);
  // The final system without a following downbeat shows no preview...
  await page.evaluate((handle) => (handle as unknown as Rudiment).updateScore(11.2, [], true, null), await app(page));
  await expect(page.locator('.playhead-preview')).not.toHaveClass(/visible/);
  // ...but a loop wrap leads back into the first system.
  const wrap = await page.evaluate((handle) => {
    const rudiment = handle as unknown as Rudiment;
    rudiment.updateScore(0, [], true, null);
    const playheadX = document.querySelector('#playhead')?.getAttribute('data-x');
    rudiment.updateScore(11.2, [], true, 0);
    return {
      transform: (document.querySelector('.playhead-preview') as HTMLElement).style.transform,
      playheadX,
    };
  }, await app(page));
  await expect(page.locator('.playhead-preview')).toHaveClass(/visible/);
  expect(Number(wrap.transform.match(/translate3d\(([-\d.]+)px/)?.[1])).toBeCloseTo(Number(wrap.playheadX), 3);
});

test('next downbeat resolution honours bar selections, loops and run ends', async ({ page }) => {
  await page.goto('/');
  const resolved = await page.evaluate((handle) => {
    const rudiment = handle as unknown as Rudiment;
    const options = { meter: 4, exerciseBeats: 8, displayRangeStart: 4, displayRangeBeats: 8, displayBars: [1, 2], totalBeats: 12, offsetBeat: 4 };
    return {
      nextOfFirstBar: rudiment.nextDownbeatScoreBeat(7, options),
      nextOfLastBarSinglePass: rudiment.nextDownbeatScoreBeat(11, options),
      looping: rudiment.nextDownbeatScoreBeat(11, { ...options, totalBeats: 20 }),
    };
  }, await app(page));
  expect(resolved.nextOfFirstBar).toBeCloseTo(8, 6);
  expect(resolved.nextOfLastBarSinglePass).toBeNull();
  expect(resolved.looping).toBeCloseTo(4, 6);
});
