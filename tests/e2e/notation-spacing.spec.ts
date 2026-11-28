import { test, expect, type Page } from '@playwright/test';

type Rudiment = {
  renderScore(exercise: unknown): void;
  updateScore(beat: number, items: unknown[], loopAware?: boolean, leadInBeat?: number | null): void;
};

type Hit = { beat: number; notation_beat: number; notation_duration_beats: number; instrument: string; note: number; hand: string };

const hit = (beat: number, duration: number, hand: 'R' | 'L' = 'R'): Hit => ({
  beat,
  notation_beat: beat,
  notation_duration_beats: duration,
  instrument: 'snare',
  note: 38,
  hand,
});

type GlyphBox = { x: number; y: number; width: number; height: number };

type RowGeometry = {
  heads: number[];
  timeSignatureRight: number;
  staveRight: number;
  restCentres: number[];
};

type Probe = { beat: number; x: number };

const within = (actual: number, expected: number, tolerance: number, message: string) =>
  expect(Math.abs(actual - expected), message).toBeLessThanOrEqual(tolerance);

// Render an exercise and collect per-system geometry: notehead centres,
// time-signature extent, stave extent, rest glyph centres, plus the playhead
// x for each probed beat.
const measure = async (page: Page, exercise: { title: string; meter_numerator: number; meter_denominator: number; hits: Hit[] }, beats: number[]): Promise<{ rows: RowGeometry[]; probes: Probe[] }> => {
  await page.evaluate(({ exercise }) => {
    (window as unknown as { rudiment: Rudiment }).rudiment.renderScore(exercise);
  }, { exercise });
  await page.waitForSelector('#notation svg .score-note');
  return page.evaluate(async ({ beats }) => {
    const rudiment = (window as unknown as { rudiment: Rudiment }).rudiment;
    const svg = document.querySelector('#notation svg')!;
    const rows = [...svg.querySelectorAll('g.vf-stave')].map((stave) => {
      const box = (stave as SVGGraphicsElement).getBBox();
      return { top: box.y, right: box.x + box.width };
    }).sort((a, b) => a.top - b.top);
    const rowFor = (y: number) => {
      let best = 0;
      for (let index = 1; index < rows.length; index++) {
        if (Math.abs(y - rows[index].top) < Math.abs(y - rows[best].top)) best = index;
      }
      return best;
    };
    const geometry: RowGeometry[] = rows.map(() => ({ heads: [], timeSignatureRight: 0, staveRight: 0, restCentres: [] }));
    rows.forEach((row, index) => {
      geometry[index].staveRight = row.right;
      const signature = [...svg.querySelectorAll('.vf-timesignature')]
        .map((element) => (element as SVGGraphicsElement).getBBox())
        .find((box) => rowFor(box.y + box.height / 2) === index);
      geometry[index].timeSignatureRight = signature ? signature.x + signature.width : 0;
    });
    for (const group of svg.querySelectorAll('.score-note')) {
      const head = group.querySelector('.vf-notehead > text') as SVGGraphicsElement | null;
      if (!head) continue;
      const box: GlyphBox = head.getBBox();
      geometry[Number((group as HTMLElement).dataset.systemRow ?? rowFor(box.y + box.height / 2))].heads.push(box.x + box.width / 2);
    }
    for (const group of svg.querySelectorAll('[data-score-rest]')) {
      const box: GlyphBox = (group as SVGGraphicsElement).getBBox();
      geometry[rowFor(box.y + box.height / 2)].restCentres.push(box.x + box.width / 2);
    }
    for (const row of geometry) {
      row.heads.sort((a, b) => a - b);
      row.restCentres.sort((a, b) => a - b);
    }
    const probes: Probe[] = [];
    for (const beat of beats) {
      rudiment.updateScore(beat, [], true);
      const x = Number(document.querySelector('#playhead')?.getAttribute('data-x'));
      probes.push({ beat, x });
    }
    return { rows: geometry, probes };
  }, { beats });
};

test('even subdivisions are evenly spaced and playhead-aligned in every bar', async ({ page }) => {
  await page.goto('/');
  const eighth = 0.5;
  const exercise = {
    title: 'Spacing grid',
    meter_numerator: 4,
    meter_denominator: 4,
    hits: [0, 1].flatMap((bar) => Array.from({ length: 8 }, (_, index) => hit(bar * 4 + index * eighth, eighth, index % 2 ? 'L' : 'R'))),
  };

  for (const viewport of [{ width: 1280, height: 900 }, { width: 420, height: 800 }]) {
    await page.setViewportSize(viewport);
    const { rows, probes } = await measure(page, exercise, [0, 1.5, 4, 5.5]);
    expect(rows.length, 'two 4/4 systems').toBe(2);
    for (const [bar, row] of rows.entries()) {
      await test.step(`viewport ${viewport.width}px, bar ${bar + 1}`, async () => {
        expect(row.heads.length, `eight eighths in bar ${bar + 1}`).toBe(8);
        const gaps = row.heads.slice(1).map((x, index) => x - row.heads[index]);
        // Equal rhythmic subdivisions: identical centre-to-centre spacing
        // within the bar, allowing only glyph-metrics jitter.
        expect(Math.max(...gaps) - Math.min(...gaps), `even eighth spacing in bar ${bar + 1}`).toBeLessThanOrEqual(1.5);
        // The introductory clef and time signature are a separate layout
        // area: the first onset keeps glyph clearance without overlap and
        // without distorting the bar's spacing.
        expect(row.timeSignatureRight, `time signature engraved in bar ${bar + 1}`).toBeGreaterThan(0);
        expect(row.heads[0] - 6 - row.timeSignatureRight, `clearance after the time signature in bar ${bar + 1}`).toBeGreaterThan(4);
        // The final eighth keeps half a beat of space before the barline:
        // tail == half of the beat-0..beat-3.5 onset span.
        const onsetSpan = row.heads[7] - row.heads[0];
        within(row.staveRight - row.heads[7], onsetSpan / 7, 3, `final-note space in bar ${bar + 1}`);
      });
    }
    // Consistent beat-relative positioning across bars: both bars start on
    // the same x and their eighth grids coincide.
    within(rows[1].heads[0], rows[0].heads[0], 1, 'bars share the beat-relative origin');
    within(rows[1].heads[7] - rows[1].heads[0], rows[0].heads[7] - rows[0].heads[0], 1.5, 'bars share the onset span');
    // Playhead alignment: the playhead passes through the engraved noteheads
    // at their onsets, including the first onset after the time signature.
    for (const [probeIndex, probe] of probes.entries()) {
      const bar = Math.floor(probe.beat / 4);
      const index = Math.round((probe.beat % 4) / eighth);
      within(probe.x, rows[bar].heads[index], 3, `playhead on beat ${probe.beat}`);
    }
  }
});

test('mixed subdivisions and rests stay on the same beat-relative grid', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto('/');
  // Bar 1: quarter, quarter rest, two eighths, two eighths.
  // Bar 2: quarter rest, quarter, quarter rest, quarter.
  const exercise = {
    title: 'Mixed spacing',
    meter_numerator: 4,
    meter_denominator: 4,
    hits: [hit(0, 1), hit(2, 0.5), hit(2.5, 0.5, 'L'), hit(3, 0.5), hit(3.5, 0.5, 'L'), hit(5, 1, 'L'), hit(7, 1, 'L')],
  };
  const { rows: [first, second] } = await measure(page, exercise, [0]);

  expect(first.heads.length, 'five onsets in bar 1').toBe(5);
  expect(second.heads.length, 'two onsets in bar 2').toBe(2);
  const eighthGap = first.heads[2] - first.heads[1];
  // The opening quarter is followed by a quarter rest: the next onset (beat 2)
  // sits four eighth intervals later on the same grid.
  within(first.heads[1] - first.heads[0], 4 * eighthGap, 3, 'onset after the quarter rest stays on the grid');
  // The remaining eighths share the same interval.
  within(first.heads[4] - first.heads[3], eighthGap, 2, 'eighths stay even');
  // The final eighth keeps half a beat (one eighth interval) before the barline.
  within(first.staveRight - first.heads[4], eighthGap, 3, 'final-note space before the barline');
  expect(first.heads[0] - 6 - first.timeSignatureRight, 'clearance after the time signature').toBeGreaterThan(4);

  // Bar 2: both quarter notes sit two beats apart on the same grid; the rests
  // at beats 4 and 6 are engraved on the same grid as the notes.
  within(second.heads[1] - second.heads[0], 4 * eighthGap, 3, 'quarters two beats apart');
  within(second.restCentres[0], second.heads[0] - 2 * eighthGap, 3, 'leading rest on the grid');
  within(second.restCentres[1], second.heads[1] - 2 * eighthGap, 3, 'interior rest on the grid');
  expect(second.heads[0] - 6 - second.timeSignatureRight, 'clearance after the time signature in bar 2').toBeGreaterThan(4);
});
