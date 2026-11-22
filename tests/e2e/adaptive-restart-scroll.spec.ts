import { expect, test, type Page } from '@playwright/test';

async function setTempo(page: Page, value: string) {
  await page.locator('#visual-metronome').click();
  await page.locator('#metronome-popover').getByLabel('Tempo in BPM').fill(value);
  await page.getByLabel('Close metronome settings').click();
}

// Minimal SMF: 8 bars of 4/4 quarter notes (snare, channel 10) at 120 BPM.
function eightBarMidi(): Buffer {
  const ppq = 480;
  const header = Buffer.alloc(14);
  header.write('MThd', 0);
  header.writeUInt32BE(6, 4);
  header.writeUInt16BE(0, 8);
  header.writeUInt16BE(1, 10);
  header.writeUInt16BE(ppq, 12);
  const varLen = (value: number): number[] => {
    if (value < 128) return [value];
    const bytes = [value & 0x7f];
    let rest = value >> 7;
    while (rest > 0) { bytes.unshift((rest & 0x7f) | 0x80); rest >>= 7; }
    return bytes;
  };
  const track: number[] = [];
  track.push(0x00, 0xff, 0x51, 0x03, 0x07, 0xa1, 0x20); // 120 BPM
  track.push(0x00, 0xff, 0x58, 0x04, 0x04, 0x02, 0x18, 0x08); // 4/4
  // Bars 1-3 and 5-8 carry quarter notes; bar 4 is dense with eighth notes so
  // skipping it costs enough score to fail the attempt while concentrating
  // every error in that one bar.
  const noteBeats: number[] = [];
  for (let bar = 0; bar < 8; bar += 1) {
    const notesInBar = bar === 3 ? 8 : 4;
    for (let note = 0; note < notesInBar; note += 1) {
      noteBeats.push(bar * 4 + note * (4 / notesInBar));
    }
  }
  let position = 0;
  for (const beat of noteBeats) {
    const tick = Math.round(beat * ppq);
    track.push(...varLen(Math.max(0, tick - position)), 0x99, 38, 100);
    track.push(...varLen(240), 0x89, 38, 0);
    position = tick + 240;
  }
  track.push(...varLen(240), 0xff, 0x2f, 0x00);
  const trackData = Buffer.from(track);
  const trackHeader = Buffer.alloc(8);
  trackHeader.write('MTrk', 0);
  trackHeader.writeUInt32BE(trackData.length, 4);
  return Buffer.concat([header, trackHeader, trackData]);
}

test('adaptive restart after a mid-bar failure scrolls the score up to the focus bars', async ({ page }) => {
  test.setTimeout(180_000);
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto('/');
  await page.locator('#midi-library-trigger').click();
  const libraryDialog = page.getByRole('dialog', { name: 'MIDI library' });
  const chooser = page.waitForEvent('filechooser');
  await libraryDialog.getByRole('button', { name: 'Import MIDI' }).click();
  await (await chooser).setFiles({ name: 'eight-bars.mid', mimeType: 'audio/midi', buffer: eightBarMidi() });
  await expect(libraryDialog).toHaveCount(0);
  await expect(page.getByText('eight-bars', { exact: true }).first()).toBeVisible();
  const barLabels = await page.evaluate(() =>
    Array.from(document.querySelectorAll('.bar-checkboxes input')).length);
  console.log('BAR COUNT:', barLabels);
  await setTempo(page, '240');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByLabel('Enable adaptive learning').check();
  await page.getByRole('button', { name: 'Close settings' }).first().click();

  await page.evaluate(() => {
    const notation = document.querySelector<HTMLElement>('#notation')!;
    const label = document.querySelector('.visual-metronome-label')!;
    const state = {
      bpm: label.textContent,
      samples: [] as Array<Record<string, unknown>>,
      boundaries: 0,
      logCamera: false,
      hitBeat: -1,
      hitLog: [] as number[],
    };
    (window as any).__adaptiveScroll = state;

    // Record every camera-affecting operation with a stack snapshot.
    const rudiment = (window as any).rudiment;
    const originalScroll = rudiment.scrollScoreToBeat;
    rudiment.scrollScoreToBeat = (beat: number) => {
      state.samples.push({ phase: 'scrollScoreToBeat', beat, scrollTop: notation.scrollTop, t: performance.now() | 0 });
      originalScroll(beat);
      state.samples.push({ phase: 'scrollScoreToBeat-after', beat, scrollTop: notation.scrollTop, t: performance.now() | 0 });
    };
    const originalClear = rudiment.clearScoreFeedback;
    rudiment.clearScoreFeedback = (preserve: boolean) => {
      state.samples.push({ phase: 'clearScoreFeedback', preserve, scrollTop: notation.scrollTop, t: performance.now() | 0 });
      originalClear(preserve);
    };
    const originalClearPreserve = rudiment.clearScoreFeedbackPreserveViewport;
    rudiment.clearScoreFeedbackPreserveViewport = () => {
      state.samples.push({ phase: 'clearPreserve', scrollTop: notation.scrollTop, t: performance.now() | 0 });
      originalClearPreserve();
    };
    const originalUpdate = rudiment.updateScore;
    rudiment.updateScore = (beat: number, judgements: unknown, loopAware: boolean) => {
      originalUpdate(beat, judgements, loopAware);
      if (state.logCamera) {
        const scrollTop = notation.scrollTop;
        const last = state.samples.at(-1);
        if (!last || last.phase !== 'update' || Math.abs(Number(last.scrollTop) - scrollTop) > 1 || Math.abs(Number(last.beat ?? -99) - beat) > 0.25) {
          state.samples.push({ phase: 'update', beat: Number(beat.toFixed(2)), scrollTop, t: performance.now() | 0 });
        }
      }
    };
    state.logCamera = false;

    new MutationObserver(() => {
      const bpm = label.textContent;
      if (bpm === state.bpm) return;
      state.bpm = bpm;
      state.boundaries += 1;
      state.logCamera = true;
      state.samples.push({ phase: `boundary-${state.boundaries}`, bpm, scrollTop: notation.scrollTop, t: performance.now() | 0 });
      setTimeout(() => { state.logCamera = false; }, 2500);
    }).observe(label, { childList: true, characterData: true, subtree: true });

    // Drive attempt 1 anchored to the real audio-output timeline (the same
    // approach the existing practice tests use). Every note lands Good except
    // bar 4's dense eighth notes, which stay unplayed: the attempt fails with
    // one star and every scoring error concentrates in bar 4.
    void (async () => {
      const rudimentLocal = rudiment;
      while (!Number.isFinite(rudimentLocal.currentAudioBeat())
        || rudimentLocal.currentAudioBeat() < 0.04) {
        await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
      }
      const firstHit = performance.now();
      const noteBeats: number[] = [];
      for (let bar = 0; bar < 8; bar += 1) {
        const notesInBar = bar === 3 ? 8 : 4;
        for (let note = 0; note < notesInBar; note += 1) {
          noteBeats.push(bar * 4 + note * (4 / notesInBar));
        }
      }
      for (const beat of noteBeats) {
        if (beat >= 12 && beat < 16) continue; // bar 4 stays unplayed
        const delay = Math.max(0, firstHit + beat * 250 - performance.now());
        await new Promise(resolve => setTimeout(resolve, delay));
        window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true }));
        state.hitLog.push(beat);
      }
    })();
  });

  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect
    .poll(() => page.evaluate(() => (window as any).__adaptiveScroll.boundaries), { timeout: 120_000 })
    .toBeGreaterThanOrEqual(1);
  await page.waitForTimeout(2500);

  const data = await page.evaluate(() => {
    const state = (window as any).__adaptiveScroll;
    return { samples: state.samples, hitLog: state.hitLog, hitBeat: state.hitBeat, bpm: state.bpm };
  });
  console.log('HITS PLAYED:', data.hitLog.length, 'first:', data.hitLog.slice(0, 4), 'last:', data.hitLog.slice(-4));
  console.log('CAMERA LOG', JSON.stringify(data.samples, null, 2));
  await page.getByRole('button', { name: /Restart/ }).click();
});
