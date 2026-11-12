import { expect, test } from '@playwright/test';
import { readFile } from 'node:fs/promises';

test('acoustic snare layers preload before the first audio interaction', async ({ page }) => {
  const requested:string[]=[];
  page.on('request',request=>{if(/\/audio\/snare-.*\.wav$/.test(new URL(request.url()).pathname))requested.push(request.url());});
  await page.goto('/');
  await expect.poll(()=>page.evaluate(()=>(window as any).rudiment.audioDiagnostics().sampleLayers)).toBe(4);
  expect(new Set(requested).size).toBe(4);
  const diagnostics=await page.evaluate(()=>(window as any).rudiment.audioDiagnostics());
  expect(diagnostics.snareStarts).toBe(0);
  expect(diagnostics.activeRun).toBe(false);
});

test('first-use workspace renders real notation and lifecycle controls', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Single Paradiddle').first()).toBeVisible();
  await expect(page.getByLabel('Tempo in BPM')).toHaveValue('60');
  await expect(page.getByText('Audio unlocks on first tap', { exact: true })).toHaveCount(0);
  await expect(page.getByText('Ready when you are', { exact: true })).toHaveCount(0);
  await expect(page.getByText('DEMO INPUT', { exact: true })).toHaveCount(0);
  await expect(page.locator('.feedback-strip')).toHaveCount(0);
  const scoreBottom=await page.locator('.score-card').evaluate(element=>element.getBoundingClientRect().bottom);
  const controlsTop=await page.locator('.transport').evaluate(element=>element.getBoundingClientRect().top);
  expect(controlsTop-scoreBottom).toBeLessThanOrEqual(20);
  await expect(page.locator('#notation svg')).toHaveCount(1);
  await expect(page.locator('.score-note')).toHaveCount(32);
  await expect(page.locator('#notation')).toContainText('R');
  await expect(page.locator('#notation')).toContainText('L');
  await expect(page.getByRole('button',{name:'MIDI connection'})).toBeVisible();
  await expect(page.getByRole('button',{name:'Import MIDI'})).toHaveCount(0);
  const startButton=page.getByRole('button',{name:/Start practice/});
  await expect(startButton.locator('.start-instruction')).toHaveText('Hit your drum or press J to start');
  await expect(page.getByText('Hit your drum or press J to start',{exact:true})).toHaveCount(1);
  await expect(page.locator('.tempo-control button svg.lucide')).toHaveCount(2);
  const tempoContentOffset=await page.locator('.tempo-control label').evaluate(label=>{
    const input=label.querySelector('input')!.getBoundingClientRect();
    const unit=label.querySelector('span')!.getBoundingClientRect();
    const bounds=label.getBoundingClientRect();
    return (input.left+unit.right)/2-(bounds.left+bounds.right)/2;
  });
  expect(Math.abs(tempoContentOffset)).toBeLessThan(1);
  const statsButton=page.getByRole('button',{name:'Stats'});
  await expect(statsButton).toBeVisible();
  await expect(statsButton.locator('svg')).toHaveCount(1);
  await expect(statsButton).toHaveText('');
  const settingsButton=page.getByRole('button',{name:'Open settings'});
  await expect(settingsButton.locator('svg.lucide')).toHaveCount(1);
  await expect(settingsButton).toHaveText('');
  const libraryButton=page.getByRole('button',{name:/MIDI library/});
  await expect(libraryButton.locator('svg.lucide')).toHaveCount(1);
  await expect(libraryButton.locator('.library-count')).toHaveText('11');
  await expect(page.locator('.score-meta #midi-library-trigger')).toHaveCount(0);
  const topbarButtons=await page.locator('.stats-trigger').evaluateAll(elements=>elements.map(element=>{
    const bounds=element.getBoundingClientRect(),style=getComputedStyle(element);
    return {width:bounds.width,height:bounds.height,background:style.backgroundColor,border:style.border,borderRadius:style.borderRadius};
  }));
  expect(topbarButtons).toHaveLength(3);
  expect(topbarButtons[1]).toEqual(topbarButtons[0]);
  expect(topbarButtons[2]).toEqual(topbarButtons[0]);
  await expect(page.getByRole('button',{name:'Play snare hit'})).toHaveCount(0);
  await expect(page.getByRole('button',{name:/Preview Acoustic snare/})).toHaveCount(0);
  await expect(page.getByRole('button',{name:'Connect MIDI drums'})).toHaveCount(0);
  await expect(page.locator('.live-assessment')).toHaveCount(0);
  await expect(page.locator('#visual-metronome')).toBeVisible();
  await expect(page.locator('.visual-metronome-beats i')).toHaveCount(4);
  await expect(page.locator('.visual-metronome-label')).toHaveText('READY');
  await expect(page.getByRole('button', { name: /Stop/ })).toHaveCount(0);
  const transportButtons=await page.locator('.transport').evaluate(element=>{
    const restart=element.querySelector('.restart-button')!.getBoundingClientRect();
    const start=element.querySelector('.start-button')!.getBoundingClientRect();
    return {restartWidth:restart.width,startWidth:start.width,restartBackground:getComputedStyle(element.querySelector('.restart-button')!).backgroundImage,startBackground:getComputedStyle(element.querySelector('.start-button')!).backgroundImage};
  });
  expect(transportButtons.restartWidth).toBeLessThan(transportButtons.startWidth);
  expect(transportButtons.restartBackground).toContain('rgb(32, 42, 38)');
  expect(transportButtons.startBackground).toContain('rgb(76, 243, 217)');
  await expect(page.locator('.restart-button svg.lucide')).toHaveCount(1);
  await expect(startButton.locator('svg.lucide')).toHaveCount(1);
  const timingKey=page.getByLabel('Timing feedback key');
  await expect(timingKey).toBeVisible();
  await expect(timingKey.locator('span')).toHaveText(['Perfect','Good','Early','Late','Missed']);
  const timingKeyColours=await timingKey.locator('i').evaluateAll(elements=>elements.map(element=>getComputedStyle(element).backgroundColor));
  expect(timingKeyColours).toEqual(['rgb(47, 207, 136)','rgb(115, 217, 147)','rgb(239, 138, 62)','rgb(168, 121, 221)','rgb(216, 61, 53)']);
  await page.waitForTimeout(450);
  await page.screenshot({ path: 'artifacts/first-run-practice.png', fullPage: true });

  await startButton.click();
  await expect(page.locator('.start-instruction')).toBeHidden();
  await expect.poll(() => page.evaluate(() => (window as unknown as {rudiment:{audioDiagnostics():{clickGain:number}}}).rudiment.audioDiagnostics().clickGain)).toBeGreaterThan(1);
  await expect(page.getByRole('button', { name: /Pause/ })).toBeVisible();
  await page.getByRole('button',{name:'Stats'}).click();
  await expect(page.getByRole('dialog',{name:'Performance history'})).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: /Start practice/ })).toBeVisible();
  await page.getByRole('button', { name: /Start practice/ }).click();
  await page.getByRole('button', { name: /Pause/ }).click();
  await expect(page.getByRole('button', { name: /Resume/ })).toBeVisible();
  await page.getByRole('button', { name: /Resume/ }).click();
  await page.getByRole('button', { name: /Restart/ }).click();
  await expect(page.getByRole('button', { name: /Start practice/ })).toBeVisible();
  await expect(page.getByRole('button', { name: /Pause/ })).toHaveCount(0);
  await expect(page.locator('#playhead')).not.toHaveClass(/visible/);
});

test('first-use timing defaults use the stricter practice profile', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button',{name:'Open settings'}).click();
  await expect(page.getByText('Perfect window · ±20 ms')).toBeVisible();
  await expect(page.getByText('Good window · ±60 ms')).toBeVisible();
  await expect(page.getByText(/Matching closes outside ±100 ms/)).toBeVisible();
  const timingSliders=page.locator('section').filter({hasText:'Timing feedback'}).locator('input[type="range"]');
  await expect(timingSliders.nth(0)).toHaveValue('20');
  await expect(timingSliders.nth(1)).toHaveValue('60');
});

test('restarting while audio loads cannot revive the practice transport', async ({ page }) => {
  await page.route('**/audio/snare-*.wav', async route => {
    await new Promise(resolve => setTimeout(resolve, 350));
    await route.continue();
  });
  await page.goto('/');
  await page.getByRole('button', { name: /Start practice/ }).click();
  await page.getByRole('button', { name: /Restart/ }).click();
  await expect(page.locator('.feedback-strip')).toHaveCount(0);
  await page.waitForTimeout(700);
  await expect(page.getByRole('button', { name: /Start practice/ })).toBeVisible();
  const diagnostics = await page.evaluate(() => (window as any).rudiment.audioDiagnostics());
  expect(diagnostics.activeRun).toBe(false);
  expect(diagnostics.transportTicks).toBe(0);
  await expect(page.locator('#playhead')).not.toHaveClass(/visible/);
});

test('Restart clears painted note feedback', async ({ page }) => {
  await page.goto('/');
  // Keep the automation round trip comfortably inside the first note's match
  // window; this test is about clearing feedback, not 60 BPM boundary timing.
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect(page.locator('#playhead')).toHaveClass(/visible/);
  await page.keyboard.press('j');
  await expect(page.locator('.score-note[data-judgement]')).toHaveCount(1);
  await page.getByRole('button', { name: /Restart/ }).click();
  await expect(page.locator('.score-note[data-judgement]')).toHaveCount(0);
  await expect(page.locator('.unmatched-hit-marker')).toHaveCount(0);
  await expect(page.locator('.live-assessment')).toHaveCount(0);
  await expect(page.locator('#playhead')).not.toHaveClass(/visible/);
});

test('percussion engraving aligns the snare, places sticking below without accent displacement, and gives every bar its own system', async ({ page }) => {
  await page.goto('/');
  const geometry = await page.locator('#notation').evaluate((notation) => {
    const svg = notation.querySelector('svg') as SVGSVGElement;
    const head = notation.querySelector('.score-note .vf-notehead > text') as SVGGraphicsElement;
    const bounds = head.getBBox();
    const matrix = head.getCTM()!;
    const headY = matrix.b * (bounds.x + bounds.width / 2) + matrix.d * (bounds.y + bounds.height / 2) + matrix.f;
    const stickingYPositions = [...notation.querySelectorAll('.score-note .vf-annotation > text')]
      .slice(0, 2)
      .map((sticking) => {
        const graphic = sticking as SVGGraphicsElement;
        const stickingBounds = graphic.getBBox();
        const stickingMatrix = graphic.getCTM()!;
        return stickingMatrix.b * (stickingBounds.x + stickingBounds.width / 2)
          + stickingMatrix.d * (stickingBounds.y + stickingBounds.height / 2) + stickingMatrix.f;
      });
    const staves = JSON.parse(svg.dataset.staffBounds || '[]') as Array<{top:number,bottom:number}>;
    return {
      headY,
      stickingYPositions,
      expectedSnareY: staves[0].top + ((staves[0].bottom - staves[0].top) / 4) * 1.5,
      systems: Number(svg.dataset.systemCount),
      barsPerLine: Number(svg.dataset.barsPerLine),
      systemMarkClearances: JSON.parse(svg.dataset.systemMarkClearances || '[]') as number[],
      rowHeight: Number(svg.dataset.systemRowHeight),
      svgHeight: svg.getBoundingClientRect().height,
      viewportHeight: notation.getBoundingClientRect().height,
    };
  });
  expect(Math.abs(geometry.headY - geometry.expectedSnareY)).toBeLessThan(2.5);
  expect(geometry.stickingYPositions).toHaveLength(2);
  expect(geometry.stickingYPositions[0]).toBeGreaterThan(geometry.headY + 10);
  expect(Math.abs(geometry.stickingYPositions[0] - geometry.stickingYPositions[1])).toBeLessThan(1);
  expect(geometry.systems).toBe(4);
  expect(geometry.barsPerLine).toBe(1);
  expect(geometry.systemMarkClearances).toHaveLength(geometry.systems - 1);
  expect(geometry.systemMarkClearances.every(clearance => clearance >= 23.5)).toBe(true);
  expect(geometry.viewportHeight).toBeGreaterThanOrEqual(geometry.rowHeight * 2);

  await page.setViewportSize({ width: 820, height: 1180 });
  await page.reload();
  const tablet = await page.locator('#notation').evaluate((notation) => ({
    systems: Number((notation.querySelector('svg') as SVGSVGElement).dataset.systemCount),
    barsPerLine: Number((notation.querySelector('svg') as SVGSVGElement).dataset.barsPerLine),
    systemMarkClearances: JSON.parse((notation.querySelector('svg') as SVGSVGElement).dataset.systemMarkClearances || '[]') as number[],
    rowHeight: Number((notation.querySelector('svg') as SVGSVGElement).dataset.systemRowHeight),
    svgHeight: (notation.querySelector('svg') as SVGSVGElement).getBoundingClientRect().height,
    viewportHeight: notation.getBoundingClientRect().height,
  }));
  expect(tablet.systems).toBe(4);
  expect(tablet.barsPerLine).toBe(1);
  expect(tablet.systemMarkClearances.every(clearance => clearance >= 23.5)).toBe(true);
  expect(tablet.viewportHeight).toBeGreaterThanOrEqual(tablet.rowHeight * 2);
});

test('live BPM changes retime the active transport without restarting practice', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.getByRole('button', { name: /Start practice/ }).click();
  await page.waitForTimeout(350);
  const before = await page.evaluate(() => (window as any).rudiment.audioDiagnostics());
  await page.getByLabel('Tempo in BPM').fill('120');
  await expect(page.getByRole('button', { name: /Pause/ })).toBeVisible();
  await expect.poll(() => page.evaluate(() => (window as any).rudiment.audioDiagnostics().transportBpm)).toBeCloseTo(120, 0);
  await page.waitForTimeout(250);
  const after = await page.evaluate(() => (window as any).rudiment.audioDiagnostics());
  expect(after.transportTicks).toBeGreaterThan(before.transportTicks);
  await page.getByRole('button', { name: /Restart/ }).click();
});

test('BPM can be replaced directly without partial values being clamped mid-entry',async({page})=>{
  await page.goto('/');
  const tempo=page.getByLabel('Tempo in BPM');

  await tempo.click();
  await page.keyboard.type('120');
  await expect(tempo).toHaveValue('120');

  await page.locator('.score-card').click({position:{x:10,y:10}});
  await tempo.click();
  await page.keyboard.type('12');
  await expect(tempo).toHaveValue('12');
  await page.keyboard.type('0');
  await expect(tempo).toHaveValue('120');

  await page.locator('.score-card').click({position:{x:10,y:10}});
  await tempo.click();
  await page.keyboard.type('999');
  await expect(tempo).toHaveValue('999');
  await page.locator('.score-card').click({position:{x:10,y:10}});
  await expect(tempo).toHaveValue('240');

  await tempo.click();
  await page.keyboard.type('90');
  await expect(tempo).toHaveValue('90');
  await tempo.press('ArrowUp');
  await expect(tempo).toHaveValue('91');
});

test('visual metronome pulses from the audible transport clock and marks the downbeat', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.evaluate(() => {
    const root = document.querySelector('#visual-metronome')!;
    (window as any).__visualPulses = [];
    new MutationObserver(() => {
      if (root.getAttribute('data-scheduled-at')) {
        (window as any).__visualPulses.push({ scheduled: Number(root.getAttribute('data-scheduled-at')), observed: performance.now() });
      }
    }).observe(root, { attributes: true, attributeFilter: ['data-pulse'] });
  });
  await page.getByRole('button', { name: /Start practice/ }).click();
  const visual = page.locator('#visual-metronome');
  await expect(visual).toHaveClass(/active/);
  await expect.poll(() => page.evaluate(() => (window as any).__visualPulses.length)).toBeGreaterThanOrEqual(2);
  const pulses = await page.evaluate(() => (window as any).__visualPulses as Array<{scheduled:number,observed:number}>);
  expect(Math.abs(pulses[0].observed - pulses[0].scheduled)).toBeLessThan(40);
  expect(Math.abs(pulses[1].observed - pulses[1].scheduled)).toBeLessThan(40);
  await expect(visual).toHaveAttribute('aria-label', /beat [1-4] of 4/);
  await expect(page.locator('.visual-metronome-beats i.current')).toHaveCount(1);
  await page.getByRole('button', { name: /Pause/ }).click();
  await expect(visual).not.toHaveClass(/active/);
  await expect(page.locator('.visual-metronome-label')).toHaveText('READY');
});

test('resuming between beats keeps the metronome accent on bar downbeats', async ({ page }) => {
  await page.addInitScript(() => {
    const original = AudioParam.prototype.setValueAtTime;
    (window as any).__clickFrequencies = [];
    AudioParam.prototype.setValueAtTime = function(value, time) {
      if (value === 1450 || value === 1080) (window as any).__clickFrequencies.push(value);
      return original.call(this, value, time);
    };
  });
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__clickFrequencies.length)).toBeGreaterThanOrEqual(2);
  await page.getByRole('button', { name: /Pause/ }).click();
  await page.evaluate(() => { (window as any).__clickFrequencies = []; });
  await page.getByRole('button', { name: /Resume/ }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__clickFrequencies.length)).toBeGreaterThanOrEqual(10);
  const clicks = await page.evaluate(() => (window as any).__clickFrequencies as number[]);
  expect(clicks).toContain(1450);
  // The oscillator and filter both set the click frequency; each beat appears twice.
  expect(clicks.slice(0, 10).filter(value => value === 1450)).toHaveLength(2);
  expect(clicks.slice(0, 10).every((value, index) => index % 2 === 0 || value === clicks[index - 1])).toBe(true);
});

test('navigation and configuration controls stop an active practice cleanly', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: /Start practice/ }).click();
  await page.getByRole('button', { name: 'Open settings' }).click();
  await expect(page.getByRole('button', { name: /Start practice/ })).toBeVisible();
  await expect(page.locator('.feedback-strip')).toHaveCount(0);
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.getByRole('button', { name: /Start practice/ }).click();
  await page.getByRole('button', { name: /MIDI library/ }).click();
  await expect(page.getByRole('button', { name: /Start practice/ })).toBeVisible();
  await expect(page.locator('.feedback-strip')).toHaveCount(0);
  await expect(page.getByRole('dialog', { name: 'MIDI library' })).toBeVisible();
});

test('a keyboard hit dismisses Statistics without starting practice', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Stats' }).click();
  const stats = page.getByRole('dialog', { name: 'Performance history' });
  await expect(stats).toBeVisible();
  await page.keyboard.press('Space');
  await expect(stats).toBeVisible();
  await expect(page.getByRole('button', { name: /Start practice/ })).toBeVisible();
  await page.keyboard.press('Shift+Tab');
  await expect.poll(() => page.evaluate(() => document.querySelector('#stats-dialog')?.contains(document.activeElement) ?? false)).toBe(true);
  await page.keyboard.press('j');
  await expect(stats).toHaveCount(0);
  await expect(page.locator('.feedback-strip')).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Start practice/ })).toBeVisible();
  await expect(page.getByRole('button', { name: /Pause/ })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Stats' })).toBeFocused();
  await page.keyboard.press('j');
  await expect(page.getByRole('button', { name: /Pause/ })).toBeVisible();
});

test('demo hits complete through scoring and persist locally', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await expect(page.getByLabel('Tempo in BPM')).toHaveValue('240');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();

  await page.getByRole('button', { name: /Start performance/ }).click();
  await page.evaluate(async () => {
    await new Promise(resolve => setTimeout(resolve, 165));
    for (let i = 0; i < 32; i++) {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true }));
      await new Promise(resolve => setTimeout(resolve, 125));
    }
  });
  const resultDialog=page.getByRole('dialog',{name:'Session Complete'});
  await expect(resultDialog).toBeVisible();
  await expect(resultDialog.getByText('SESSION SAVED · LOCALLY')).toHaveCount(0);
  await expect(resultDialog.getByRole('heading', { name: 'Session Complete' })).toBeVisible();
  await expect(resultDialog.locator('.score-summary-card')).toBeVisible();
  await expect(resultDialog.locator('.new-high-score')).toBeHidden();
  const breakdown=resultDialog.locator('.breakdown-row');
  await expect(breakdown).toHaveCount(5);
  await expect(breakdown.first()).toContainText('/ 32');
  for(const [index,label] of ['Perfect','Good','Early','Late','Missed'].entries())await expect(breakdown.nth(index).locator('span')).toContainText(label);
  await expect(resultDialog.locator('.progress-chart')).toBeVisible();
  await expect(resultDialog.locator('.progress-point')).toHaveCount(1);
  await expect(resultDialog.getByText(/Average offset|Mean absolute error|Timing variability|Focus for next time/i)).toHaveCount(0);
  const resultModalLayout=await resultDialog.evaluate(element=>({clientHeight:element.clientHeight,scrollHeight:element.scrollHeight,overflow:getComputedStyle(element).overflow}));
  expect(resultModalLayout.scrollHeight).toBeLessThanOrEqual(resultModalLayout.clientHeight);
  expect(resultModalLayout.overflow).toBe('hidden');
  const resultCloseChrome=await resultDialog.getByRole('button',{name:'Close session results'}).evaluate(element=>{const style=getComputedStyle(element);return {width:style.width,height:style.height,borderRadius:style.borderRadius,background:style.backgroundColor,borderColor:style.borderTopColor};});
  await expect(resultDialog.getByText('Single Paradiddle · 240 BPM', { exact:true })).toBeVisible();
  await expect(page.locator('.workspace')).toBeVisible();
  const overlayBounds=await page.evaluate(()=>{
    const overlay=document.querySelector('.result-backdrop')!.getBoundingClientRect();
    const modal=document.querySelector('.results-modal')!.getBoundingClientRect();
    return {viewport:{width:innerWidth,height:innerHeight},overlay:{x:overlay.x,y:overlay.y,width:overlay.width,height:overlay.height},modal:{x:modal.x,y:modal.y,width:modal.width,height:modal.height}};
  });
  expect(overlayBounds.overlay.x).toBe(0);
  expect(overlayBounds.overlay.y).toBe(0);
  expect(overlayBounds.overlay.width).toBe(overlayBounds.viewport.width);
  expect(overlayBounds.overlay.height).toBe(overlayBounds.viewport.height);
  expect(overlayBounds.modal.width).toBeLessThan(overlayBounds.viewport.width);
  expect(overlayBounds.modal.width).toBeLessThanOrEqual(900);
  await page.screenshot({ path: 'artifacts/completed-result.png', fullPage: true });
  await page.setViewportSize({width:412,height:915});
  const narrowResultLayout=await resultDialog.evaluate(element=>({clientWidth:element.clientWidth,scrollWidth:element.scrollWidth,clientHeight:element.clientHeight,scrollHeight:element.scrollHeight,rect:element.getBoundingClientRect().toJSON()}));
  expect(narrowResultLayout.scrollWidth).toBeLessThanOrEqual(narrowResultLayout.clientWidth);
  expect(narrowResultLayout.scrollHeight).toBeLessThanOrEqual(narrowResultLayout.clientHeight);
  expect(narrowResultLayout.rect.x).toBeGreaterThanOrEqual(5);
  expect(narrowResultLayout.rect.right).toBeLessThanOrEqual(407);
  await expect(resultDialog.locator('.summary-footer')).toBeVisible();
  await page.setViewportSize({width:2168,height:522});
  const shallowResultLayout=await resultDialog.evaluate(element=>{
    const modal=element.getBoundingClientRect();
    const actions=element.querySelector<HTMLElement>('.result-actions')!.getBoundingClientRect();
    return {
      clientHeight:element.clientHeight,
      scrollHeight:element.scrollHeight,
      actionsFit:actions.top>=modal.top&&actions.bottom<=modal.bottom,
    };
  });
  expect(shallowResultLayout.scrollHeight).toBeLessThanOrEqual(shallowResultLayout.clientHeight);
  expect(shallowResultLayout.actionsFit).toBe(true);
  await page.setViewportSize({width:915,height:412});
  const landscapeResultLayout=await resultDialog.evaluate(element=>({clientHeight:element.clientHeight,scrollHeight:element.scrollHeight,clientWidth:element.clientWidth,scrollWidth:element.scrollWidth}));
  expect(landscapeResultLayout.scrollHeight).toBeLessThanOrEqual(landscapeResultLayout.clientHeight);
  expect(landscapeResultLayout.scrollWidth).toBeLessThanOrEqual(landscapeResultLayout.clientWidth);
  await page.setViewportSize({width:1440,height:900});
  const pageUrl=page.url();
  await page.waitForTimeout(2_050);
  await page.keyboard.press('j');
  await expect(page.locator('.result-backdrop')).toBeHidden();
  await expect(page.getByRole('button',{name:/Start performance/})).toBeVisible();
  await expect(page.getByRole('button',{name:/Pause/})).toHaveCount(0);
  expect(page.url()).toBe(pageUrl);
  await page.keyboard.press('j');
  await expect(page.getByRole('button',{name:/Pause/})).toBeVisible();
  await expect(page.locator('.workspace')).toBeVisible();
  expect(page.url()).toBe(pageUrl);
  await page.getByRole('button',{name:/Restart/}).click();
  await expect(page.getByRole('button',{name:/Start performance/})).toBeVisible();
  await expect(page.getByRole('button',{name:/Pause/})).toHaveCount(0);
  await expect(page.locator('#playhead')).not.toHaveClass(/visible/);
  await page.reload();
  await page.evaluate(() => window.scrollTo(0, 0));

  await page.reload();
  await page.getByRole('button',{name:/MIDI library/}).click();
  const library=page.getByRole('dialog',{name:'MIDI library'});
  await expect(library).toBeVisible();
  const libraryCloseChrome=await library.getByRole('button',{name:'Close MIDI library'}).evaluate(element=>{const style=getComputedStyle(element);return {width:style.width,height:style.height,borderRadius:style.borderRadius,background:style.backgroundColor,borderColor:style.borderTopColor};});
  expect(resultCloseChrome).toEqual(libraryCloseChrome);
  await library.getByRole('button',{name:'Close MIDI library'}).click();
  await page.getByRole('button',{name:'Stats'}).click();
  const stats=page.getByRole('dialog',{name:'Performance history'});
  await expect(stats).toBeVisible();
  await expect(stats).toContainText('1');
  const statsMetrics=stats.locator('.stats-overview article');
  for(const [index,label] of ['PERFECT HITS','GOOD HITS','EARLY HITS','LATE HITS','MISSED NOTES'].entries())await expect(statsMetrics.nth(index+3).locator('span')).toHaveText(label);
  await expect(stats.locator('.stats-recent')).toContainText(/\d+ perfect, \d+ good, \d+ early, \d+ late, \d+ missed/);
  await expect(page.getByRole('button',{name:'Close statistics'})).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(stats).toHaveCount(0);
  await expect(page.getByRole('button',{name:'Stats'})).toBeFocused();
  await page.getByRole('button', { name: 'Open settings' }).click();
  await expect(page.getByText('1 completed performance(s) stored only in this browser.')).toBeVisible();
});

test('progress chart uses persisted scores for the same exercise and tempo',async({page})=>{
  await page.goto('/');
  await page.evaluate(async()=>new Promise<void>((resolve,reject)=>{
    const request=indexedDB.open('rudiment',2);
    request.onerror=()=>reject(request.error);
    request.onsuccess=()=>{
      const db=request.result,transaction=db.transaction('sessions','readwrite'),store=transaction.objectStore('sessions');
      for(const [index,score] of [42,58,67].entries())store.put({
        id:`seed-${index}`,endedAt:`2026-11-10T18:0${index}:00Z`,exerciseId:'single-paradiddle',exerciseTitle:'Single Paradiddle',exerciseHash:'builtin-paradiddle-v1',bpm:240,inputMode:'demo',scoringVersion:'v3',toleranceMs:60,latencyMs:0,
        summary:{score,matched:32,expected:32,missed:0,extras:0,wrong_instrument:0,mean_abs_error_ms:20,signed_mean_error_ms:0,timing_consistency_ms:10,scoring_version:'v3',results:[]},
        observed:[],schemaVersion:1,mode:'performance'
      });
      transaction.oncomplete=()=>resolve();transaction.onerror=()=>reject(transaction.error);
    };
  }));
  await page.reload();
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByRole('button',{name:'Open settings'}).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button',{name:'Close settings'}).first().click();
  await page.getByRole('button',{name:/Start performance/}).click();
  await page.evaluate(async()=>{
    await new Promise(resolve=>setTimeout(resolve,165));
    for(let index=0;index<32;index++){
      window.dispatchEvent(new KeyboardEvent('keydown',{key:'j',bubbles:true}));
      await new Promise(resolve=>setTimeout(resolve,125));
    }
  });
  const resultDialog=page.getByRole('dialog',{name:'Session Complete'});
  await expect(resultDialog.locator('.progress-point')).toHaveCount(4);
  await expect(resultDialog.locator('.progress-line')).toHaveAttribute('points',/\s/);
  await expect(resultDialog.locator('.progress-point.latest')).toHaveCount(1);
  const currentScore=Number(await resultDialog.locator('.summary-score strong').textContent());
  if(currentScore>67)await expect(resultDialog.locator('.new-high-score')).toBeVisible();
  else await expect(resultDialog.locator('.new-high-score')).toBeHidden();
});

test('one drum hit dismisses results and a second hit starts the next run',async({page})=>{
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByRole('button',{name:'Open settings'}).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button',{name:'Close settings'}).first().click();
  await page.getByRole('button',{name:/Start performance/}).click();
  await page.evaluate(async () => {
    await new Promise(resolve => setTimeout(resolve, 165));
    for (let i = 0; i < 32; i++) {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true }));
      await new Promise(resolve => setTimeout(resolve, 125));
    }
  });
  await expect(page.getByRole('dialog',{name:'Session Complete'})).toBeVisible();
  await page.keyboard.press('j');
  await expect(page.getByRole('dialog',{name:'Session Complete'})).toBeVisible();
  await page.waitForTimeout(2_050);
  await page.keyboard.press('j');
  await expect(page.locator('.result-backdrop')).toBeHidden();
  await expect(page.getByRole('button',{name:/Start performance/})).toBeVisible();
  await expect(page.getByRole('button',{name:/Pause/})).toHaveCount(0);
  await page.keyboard.press('j');
  await expect(page.getByRole('button',{name:/Pause/})).toBeVisible();
  await page.getByRole('button',{name:/Restart/}).click();
});

test('a drum hit starts practice from idle',async({page})=>{
  await page.goto('/');
  await page.getByRole('button',{name:'Open settings'}).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button',{name:'Close settings'}).first().click();
  await page.keyboard.press('j');
  await expect(page.getByRole('button',{name:/Pause/})).toBeVisible();
  await expect(page.locator('.result-backdrop')).toBeHidden();
  await page.getByRole('button',{name:/Restart/}).click();
  await expect(page.locator('.feedback-strip')).toHaveCount(0);
});

test('held keyboard auto-repeat is not recorded as evenly spaced drum strikes',async({page})=>{
  await page.goto('/');
  await page.evaluate(()=>{
    for(let index=0;index<4;index++)window.dispatchEvent(new KeyboardEvent('keydown',{key:'j',repeat:true,bubbles:true}));
  });
  await expect(page.getByRole('button',{name:/Start practice/})).toBeVisible();
  await expect(page.locator('.score-note[data-judgement]')).toHaveCount(0);
  await expect(page.locator('.unmatched-hit-marker')).toHaveCount(0);
  await page.evaluate(()=>window.dispatchEvent(new KeyboardEvent('keydown',{key:'j',bubbles:true})));
  await expect(page.getByRole('button',{name:/Pause/})).toBeVisible();
});

test('a run with no drum input never shows hit circles',async({page})=>{
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByRole('button',{name:'Open settings'}).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button',{name:'Close settings'}).first().click();
  await page.getByRole('button',{name:/Start performance/}).click();
  await expect(page.getByRole('button',{name:/Pause/})).toBeVisible();
  await expect(page.locator('.judgement-halo')).toHaveCount(0);
  await expect(page.locator('.score-note[data-judgement="missed"]')).toHaveCount(0);
  await expect(page.locator('.missed-note-marker')).toHaveCount(0);
  await expect(page.locator('.judgement-halo')).toHaveCount(0);
  await expect(page.getByRole('dialog',{name:'Session Complete'})).toBeVisible({timeout:10000});
  await expect(page.locator('.judgement-halo')).toHaveCount(0);
  await expect(page.locator('.missed-note-marker')).toHaveCount(0);
});

test('all built-in rudiments match the supplied timing and engraving reference', async ({ page }) => {
  await page.goto('/');
  const manifest = JSON.parse(await readFile('tests/fixtures/rudiment-reference/manifest.json', 'utf8')) as {pieces:Array<{
      id:string;
      title:string;
      midi_notes:number;
      total_bars:number;
      tempo_bpm:number;
      events:Array<{grace:boolean;accent:boolean;tuplet:number;hand:string}>;
    }>};
  expect(manifest.pieces).toHaveLength(11);
  const trigger = page.locator('#midi-library-trigger');
  await trigger.click();
  const dialog = page.getByRole('dialog', { name: 'MIDI library' });
  await expect(dialog.locator('.bundled-entry')).toHaveCount(6);
  await expect(dialog.locator('a')).toHaveCount(0);
  await page.keyboard.press('Tab');
  await expect.poll(() => page.evaluate(() => document.querySelector('#midi-library-dialog')?.contains(document.activeElement) ?? false)).toBe(true);
  await expect(dialog.getByText('Page 1 of 2')).toBeVisible();
  await dialog.getByRole('button', { name: 'Next library page' }).click();
  await expect(dialog.locator('.bundled-entry')).toHaveCount(5);
  await expect(dialog.getByText('Page 2 of 2')).toBeVisible();
  await dialog.getByRole('button', { name: 'Close MIDI library' }).click();

  for (const [index, piece] of manifest.pieces.entries()) {
    const reference = await readFile(`tests/fixtures/rudiment-reference/${piece.id}.png`);
    const referenceUrl = `data:image/png;base64,${reference.toString('base64')}`;
    const referenceGeometry = await page.evaluate(async url => {
      const response = await fetch(url);
      if (!response.ok) throw new Error(`Reference PNG failed to load: ${response.status}`);
      const bitmap = await createImageBitmap(await response.blob());
      const sourceWidth = bitmap.width;
      const sourceHeight = bitmap.height;
      const scale = 1;
      const canvas = document.createElement('canvas');
      canvas.width = Math.round(sourceWidth * scale);
      canvas.height = Math.round(sourceHeight * scale);
      const context = canvas.getContext('2d', {willReadFrequently:true})!;
      context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
      bitmap.close();
      const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
      const denseRows:number[] = [];
      const firstRow = Math.floor(canvas.height * 0.12);
      const lastRow = Math.floor(canvas.height * 0.92);
      for (let y = firstRow; y < lastRow; y += 1) {
        let run = 0;
        let longestRun = 0;
        let gap = 0;
        for (let x = 0; x < canvas.width; x += 1) {
          const offset = (y * canvas.width + x) * 4;
          if (pixels[offset] < 180 && pixels[offset + 1] < 180 && pixels[offset + 2] < 180) {
            run += 1;
            gap = 0;
            longestRun = Math.max(longestRun, run);
          } else if (run > 0 && gap < 40) {
            run += 1;
            gap += 1;
          } else {
            run = 0;
            gap = 0;
          }
        }
        if (longestRun > canvas.width * 0.55) denseRows.push(y);
      }
      const systemBands:number[][] = [];
      for (const row of denseRows) {
        const band = systemBands.at(-1);
        if (!band || row - band.at(-1)! > 4) systemBands.push([row]);
        else band.push(row);
      }
      return {width:sourceWidth,height:sourceHeight,systemRows:systemBands.map(rows => rows[Math.floor(rows.length / 2)])};
    }, referenceUrl);

    await trigger.click();
    if (index >= 6) await dialog.getByRole('button', { name: 'Next library page' }).click();
    const entry = dialog.locator('.bundled-entry').nth(index % 6);
    try {
      expect(referenceGeometry.width).toBe(2400);
      expect(referenceGeometry.height).toBe(piece.total_bars === 8 ? 3130 : 1900);
      expect(referenceGeometry.systemRows).toHaveLength(piece.total_bars);
      await expect(entry.locator('strong')).toHaveText(piece.title);
      await entry.locator('.library-load').click();
      await expect(page.locator('.exercise-heading strong')).toHaveText(piece.title);
      await expect(page.getByLabel('Tempo in BPM')).toHaveValue(String(piece.tempo_bpm));
      await expect(page.locator('.score-note')).toHaveCount(piece.midi_notes);
      await expect(page.locator('.score-note[data-grace="true"]')).toHaveCount(piece.events.filter(event => event.grace).length);
      await expect(page.locator('.score-note[data-accent="true"]')).toHaveCount(piece.events.filter(event => event.accent).length);
      await expect(page.locator('.score-note[data-tuplet="true"]')).toHaveCount(piece.events.filter(event => event.tuplet !== 0).length);
      await expect(page.locator('#notation svg')).toHaveAttribute('data-system-count', String(referenceGeometry.systemRows.length));
      await expect(page.locator('#notation svg')).toHaveAttribute('data-bars-per-line', '1');
      await expect(page.locator('#notation')).toContainText('R');
      await expect(page.locator('#notation')).toContainText('L');
      await expect(page.locator('.warning-row')).toHaveCount(0);
    } catch (error) {
      await test.info().attach(`${piece.id}-reference.png`, {body:reference,contentType:'image/png'});
      const score = page.locator('#notation svg');
      if (await score.count()) await test.info().attach(`${piece.id}-generated.png`, {body:await score.screenshot(),contentType:'image/png'});
      throw error;
    }
  }
});

test('built-in and personal MIDI import paths remain distinct and broken files explain failure', async ({ page }) => {
  await page.goto('/');
  const libraryTrigger = page.locator('#midi-library-trigger');
  const libraryDialog = page.getByRole('dialog', { name: 'MIDI library' });
  await libraryTrigger.click();
  await expect(libraryDialog.getByRole('button',{name:'Import MIDI'})).toBeVisible();
  await expect(libraryDialog.getByRole('button',{name:'Close MIDI library'})).toBeFocused();
  await expect(page.getByRole('button',{name:'Delete Single Paradiddle.mid'})).toHaveCount(0);
  await libraryDialog.locator('.bundled-entry .library-load').nth(5).click();
  await expect(libraryDialog).toHaveCount(0);
  await expect(page.locator('.exercise-heading strong')).toHaveText('Single Paradiddle');
  await expect(page.locator('.score-note')).toHaveCount(64);
  await expect(page.locator('#notation')).toContainText('R');
  await expect(page.locator('#notation')).toContainText('L');
  await expect(page.locator('.warning-row')).toHaveCount(0);
  await expect(page.locator('.score-meta .file-details')).toHaveCount(0);
  await page.locator('.score-file-info summary').click();
  await expect(page.locator('.score-file-info-popover')).toContainText('1 playable drum track');
  await expect(page.getByLabel('Drum track')).toBeHidden();
  await page.locator('.score-file-info summary').click();
  await libraryTrigger.click();
  await expect(libraryDialog.getByRole('button',{name:/File information/})).toHaveCount(0);
  await libraryDialog.getByRole('button',{name:'Close MIDI library'}).click();

  const quarterChooser=page.waitForEvent('filechooser');
  await libraryTrigger.click();
  await libraryDialog.getByRole('button',{name:'Import MIDI'}).click();
  await expect(libraryDialog).toBeVisible();
  await (await quarterChooser).setFiles('web/public/exercises/quarter-note-reference.mid');
  await expect(libraryDialog).toHaveCount(0);
  await expect(page.getByText('quarter-note-reference',{exact:true}).first()).toBeVisible();
  await expect(page.getByText('MIDI imported — review sticking and press Start',{exact:true})).toHaveCount(0);
  await expect(page.locator('.score-note[data-written-duration="q"]')).toHaveCount(8);
  await expect(page.locator('.warning-row')).toHaveCount(0);
  await expect(page.locator('.score-meta .file-details')).toHaveCount(0);
  await page.locator('.score-file-info summary').click();
  await expect(page.locator('.score-file-info-popover')).toContainText('1 playable drum track');
  await expect(page.getByLabel('Drum track')).toBeHidden();
  await page.locator('.score-file-info summary').click();
  await libraryTrigger.click();
  await libraryDialog.getByRole('tab',{name:/Your imports/}).click();
  await expect(libraryDialog.getByRole('button',{name:/File information/})).toHaveCount(0);
  await libraryDialog.getByRole('button',{name:'Close MIDI library'}).click();
  await expect.poll(() => page.evaluate(() => new Promise<number>((resolve, reject) => {
    const request = indexedDB.open('rudiment');
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const count = request.result.transaction('imports').objectStore('imports').count();
      count.onerror = () => reject(count.error);
      count.onsuccess = () => resolve(count.result);
    };
  }))).toBeGreaterThanOrEqual(1);
  await libraryTrigger.click();
  await libraryDialog.getByRole('tab',{name:/Your imports/}).click();
  await expect(libraryDialog.locator('.personal-library-card .library-load')).toContainText('quarter-note-reference.mid');
  await libraryDialog.getByRole('tab',{name:/Built-in/}).click();
  await expect(libraryDialog.locator('.bundled-entry .library-load')).toHaveCount(6);
  await page.locator('.library-backdrop').click({position:{x:5,y:5}});
  await expect(libraryDialog).toHaveCount(0);
  await libraryTrigger.click();
  await libraryDialog.locator('.bundled-entry .library-load').nth(5).click();
  await expect(page.locator('.score-note')).toHaveCount(64);
  await expect(libraryDialog).toHaveCount(0);
  await libraryTrigger.click();
  await libraryDialog.getByRole('tab',{name:/Your imports/}).click();
  await libraryDialog.locator('.personal-library-card .library-load').click();
  await expect(page.locator('.score-note[data-written-duration="q"]')).toHaveCount(8);
  await expect(libraryDialog).toHaveCount(0);
  await page.reload();
  await expect(page.locator('.score-note[data-written-duration="q"]')).toHaveCount(8);
  await expect.poll(() => page.evaluate(() => new Promise<number>((resolve, reject) => {
    const request = indexedDB.open('rudiment');
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const count = request.result.transaction('imports').objectStore('imports').count();
      count.onerror = () => reject(count.error);
      count.onsuccess = () => resolve(count.result);
    };
  }))).toBeGreaterThanOrEqual(1);
  await libraryTrigger.click();
  await libraryDialog.getByRole('tab',{name:/Your imports/}).click();
  page.once('dialog',dialog=>dialog.accept());
  await libraryDialog.getByRole('button',{name:'Delete quarter-note-reference.mid'}).click();
  await expect(libraryDialog.getByText('No personal imports yet')).toBeVisible();
  await expect(libraryTrigger).toContainText('11');
  await libraryDialog.getByRole('button', { name: 'Close MIDI library' }).click();

  const chooserPromise = page.waitForEvent('filechooser');
  await libraryTrigger.click();
  await libraryDialog.getByRole('button', { name: 'Import MIDI' }).click();
  await expect(libraryDialog).toBeVisible();
  const chooser = await chooserPromise;
  await chooser.setFiles({ name: 'broken.mid', mimeType: 'audio/midi', buffer: Buffer.from('not-midi') });
  await expect(libraryDialog).toHaveCount(0);
  await expect(page.locator('.error-row')).toContainText(/malformed|truncated/i);
});

test('score popovers stay inside the viewport and footer controls remain compact', async ({ page }) => {
  for (const viewport of [{ width: 1280, height: 720 }, { width: 390, height: 844 }]) {
    await page.setViewportSize(viewport);
    await page.goto('/');

    await page.locator('.score-file-info summary').click();
    await expect(page.locator('.score-file-info-popover')).toBeVisible();
    expect(await page.locator('.score-file-info-popover').evaluate(element => {
      const rect = element.getBoundingClientRect();
      return rect.left >= 0 && rect.right <= window.innerWidth && rect.top >= 0 && rect.bottom <= window.innerHeight;
    })).toBe(true);
    await page.locator('.score-file-info summary').click();

    await page.getByLabel('Select bars').click();
    await expect(page.locator('.bar-selection-popover')).toBeVisible();
    await page.getByLabel('First selected bar').selectOption('1');
    await page.getByLabel('Last selected bar').selectOption('2');
    await expect(page.locator('.bar-selection-popover')).toBeVisible();
    await expect(page.locator('.score-measure-number.selected-bar-number')).toHaveCount(2);
    expect(await page.locator('.bar-selection-popover').evaluate(element => {
      const rect = element.getBoundingClientRect();
      return rect.left >= 0 && rect.right <= window.innerWidth && rect.top >= 0 && rect.bottom <= window.innerHeight;
    })).toBe(true);
    await expect(page.locator('.practice-toolbar')).toHaveCount(0);
    await expect(page.locator('.mode-segment')).toHaveCount(0);

    await page.getByLabel('Close bar selection').click();
    await expect(page.locator('.bar-selection-popover')).toBeHidden();
    await page.getByLabel('Select bars').click();
    await page.keyboard.press('Escape');
    await expect(page.locator('.bar-selection-popover')).toBeHidden();
    await page.getByLabel('Select bars').click();
    await page.locator('#notation').click({ position: { x: 12, y: 12 } });
    await expect(page.locator('.bar-selection-popover')).toBeHidden();
    await page.getByLabel('Select bars').click();
    await page.getByLabel('Select bars').click();
    await expect(page.locator('.bar-selection-popover')).toBeHidden();
    await page.getByLabel('Select bars').click();
    await page.getByRole('button', { name: 'Open settings' }).click();
    await expect(page.locator('.bar-selection-popover')).toBeHidden();
    await page.getByRole('button', { name: 'Close settings' }).first().click();
    await page.getByLabel('Select bars').click();
    await page.getByRole('button', { name: 'All bars' }).click();
    await page.getByLabel('Close bar selection').click();
    await expect(page.locator('.start-button')).toContainText('Start performance');
  }
});

test('a large personal MIDI library paginates without overflowing the modal', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(async () => {
    const bytes = Array.from(new Uint8Array(await (await fetch('public/exercises/quarter-note-reference.mid')).arrayBuffer()));
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('rudiment');
      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(request.result);
    });
    await new Promise<void>((resolve, reject) => {
      const transaction = database.transaction('imports', 'readwrite');
      const store = transaction.objectStore('imports');
      for (let index = 1; index <= 13; index += 1) {
        store.put({hash:`pagination-${index}`,name:`Exercise ${String(index).padStart(2,'0')}.mid`,bytes});
      }
      transaction.oncomplete = () => resolve();
      transaction.onerror = () => reject(transaction.error);
    });
    database.close();
  });
  await page.reload();
  await page.getByRole('button', { name: /MIDI library/ }).click();
  const dialog = page.getByRole('dialog', { name: 'MIDI library' });
  await dialog.getByRole('tab', { name: /Your imports/ }).click();
  await expect(dialog.locator('.personal-library-card')).toHaveCount(6);
  await expect(dialog.getByText('Page 1 of 3')).toBeVisible();
  await dialog.getByRole('button', { name: 'Next library page' }).click();
  await expect(dialog.locator('.personal-library-card')).toHaveCount(6);
  await expect(dialog.getByText('Page 2 of 3')).toBeVisible();
  await dialog.getByRole('button', { name: 'Next library page' }).click();
  await expect(dialog.locator('.personal-library-card')).toHaveCount(1);
  await expect(dialog.getByText('Page 3 of 3')).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Next library page' })).toBeDisabled();
});

test('restoring the last MIDI exercise preserves the saved practice tempo', async ({ page }) => {
  await page.goto('/');
  const chooserPromise = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: /MIDI library/ }).click();
  const libraryDialog = page.getByRole('dialog', { name: 'MIDI library' });
  await libraryDialog.getByRole('button', { name: 'Import MIDI' }).click();
  await expect(libraryDialog).toBeVisible();
  await (await chooserPromise).setFiles('web/public/exercises/quarter-note-reference.mid');
  await expect(libraryDialog).toHaveCount(0);
  await expect(page.locator('.score-note[data-written-duration="q"]')).toHaveCount(8);
  await page.getByLabel('Tempo in BPM').fill('137');
  await expect.poll(() => page.evaluate(() => new Promise<number | undefined>((resolve, reject) => {
    const request = indexedDB.open('rudiment');
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const get = request.result.transaction('settings').objectStore('settings').get('app');
      get.onerror = () => reject(get.error);
      get.onsuccess = () => resolve(get.result?.value?.bpm);
    };
  }))).toBe(137);
  await page.reload();
  await expect(page.locator('.score-note[data-written-duration="q"]')).toHaveCount(8);
  await expect(page.getByLabel('Tempo in BPM')).toHaveValue('137');
});

test('simulated Web MIDI preserves message details and filters non-attacks', async ({ page }) => {
  await page.addInitScript(() => {
    const input = {
      id: 'fake-kit-1', name: 'Acceptance Kit', manufacturer: 'Rudiment Test', type: 'input',
      state: 'connected', connection: 'open',
      onmidimessage: null as null | ((event: unknown) => void), onstatechange: null as null | ((event: unknown) => void),
      open: async () => input, close: async () => input,
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatch: (data: number[], timestamp = performance.now()) => input.onmidimessage?.({ data: new Uint8Array(data), timeStamp: timestamp }),
    };
    const stateListeners: Array<(event: unknown) => void> = [];
    const access = {
      inputs: new Map([[input.id, input]]), outputs: new Map(), sysexEnabled: false,
      onstatechange: null as null | ((event: unknown) => void),
      addEventListener: (type: string, callback: (event: unknown) => void) => { if (type === 'statechange') stateListeners.push(callback); },
      removeEventListener: () => {},
      disconnect: () => { access.inputs.delete(input.id); input.state = 'disconnected'; input.connection = 'pending'; const event = { port: input, timeStamp: performance.now() }; access.onstatechange?.(event); stateListeners.forEach(callback => callback(event)); },
      reconnect: () => { access.inputs.set(input.id,input); input.state = 'connected'; input.connection = 'open'; const event = { port: input, timeStamp: performance.now() }; access.onstatechange?.(event); stateListeners.forEach(callback => callback(event)); },
    };
    Object.defineProperty(navigator, 'requestMIDIAccess', { configurable: true, value: async () => access });
    Object.assign(window, { __rudimentFakeInput: input, __rudimentFakeAccess: access });
  });
  await page.goto('/');
  await page.evaluate(() => { const prototype=AudioBufferSourceNode.prototype,original=prototype.start;(window as unknown as {__midiVoiceStarts:number}).__midiVoiceStarts=0;prototype.start=function(...args){(window as unknown as {__midiVoiceStarts:number}).__midiVoiceStarts++;return original.apply(this,args as [number]);}; });
  await page.getByRole('button', { name: 'MIDI connection' }).click();
  await expect(page.locator('.status-pill')).toContainText('Acceptance Kit · connected');
  await page.getByRole('button', { name: 'Stats' }).click();
  await page.evaluate(() => (window as unknown as { __rudimentFakeInput: { dispatch(data: number[]): void } }).__rudimentFakeInput.dispatch([0x99, 38, 100]));
  await expect(page.getByRole('dialog', { name: 'Practice statistics' })).toHaveCount(0);
  await expect(page.locator('.feedback-strip')).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Start practice/ })).toBeVisible();
  await expect(page.getByRole('button', { name: /Pause/ })).toHaveCount(0);
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByText('MIDI diagnostics').click();
  await page.evaluate(() => {
    const input = (window as unknown as { __rudimentFakeInput: { dispatch(data: number[],timestamp?:number): void } }).__rudimentFakeInput;
    input.dispatch([0x99, 38, 1],1234.567);
  });
  await expect(page.locator('.diagnostics')).toContainText('1234.567 ms');
  await expect(page.locator('.diagnostics')).toContainText('velocity 1');
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {__midiVoiceStarts:number}).__midiVoiceStarts)).toBeGreaterThanOrEqual(1);
  const startsAfterLow=await page.evaluate(()=>(window as unknown as {__midiVoiceStarts:number}).__midiVoiceStarts);
  await page.evaluate(() => {
    const input = (window as unknown as { __rudimentFakeInput: { dispatch(data: number[]): void } }).__rudimentFakeInput;
    input.dispatch([0x99, 38, 0]);
    input.dispatch([0xb9, 4, 90]);
  });
  await page.waitForTimeout(60);
  expect(await page.evaluate(()=>(window as unknown as {__midiVoiceStarts:number}).__midiVoiceStarts)).toBe(startsAfterLow);
  await page.evaluate(() => (window as unknown as { __rudimentFakeInput: { dispatch(data: number[]): void } }).__rudimentFakeInput.dispatch([0x99,38,100]));
  await expect(page.locator('.diagnostics')).toContainText('note_on');
  await expect(page.locator('.diagnostics')).toContainText('note_off');
  await expect(page.locator('.diagnostics')).toContainText('cc');
  await expect(page.locator('.diagnostics')).toContainText(/153,\s*38,\s*100/);
  await expect(page.locator('.live-assessment')).toHaveCount(0);
  await page.getByRole('button', { name: 'Learn snare' }).click();
  await page.evaluate(() => (window as unknown as { __rudimentFakeInput: { dispatch(data: number[]): void } }).__rudimentFakeInput.dispatch([0x99, 40, 112]));
  await expect(page.getByText('Current snare: note 40, channel 10')).toBeVisible();
  await page.waitForTimeout(400);
  await page.evaluate(() => (window as unknown as { __rudimentFakeAccess: { disconnect(): void } }).__rudimentFakeAccess.disconnect());
  await expect(page.locator('.midi-state')).toContainText('Drums disconnected');
  await page.evaluate(() => (window as unknown as { __rudimentFakeAccess: { reconnect(): void } }).__rudimentFakeAccess.reconnect());
  await expect(page.locator('.midi-state')).toContainText('Acceptance Kit · connected');
  await page.reload();
  await page.getByRole('button', { name: 'Open settings' }).click();
  await expect(page.getByText('Current snare: note 40, channel 10')).toBeVisible();
});

test('MIDI monitoring does not generate a substitute snare when sample decoding stalls', async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(BaseAudioContext.prototype,'decodeAudioData',{
      configurable:true,
      value:() => new Promise<AudioBuffer>(() => {})
    });
    const input = {
      id:'midiweb-kit',name:'MIDIWeb Kit',manufacturer:'Rudiment Test',type:'input',
      state:'connected',connection:'open',onmidimessage:null as null | ((event:unknown)=>void),onstatechange:null,
      open:async()=>input,close:async()=>input,addEventListener:()=>{},removeEventListener:()=>{},
      dispatch:(data:number[])=>input.onmidimessage?.({data:new Uint8Array(data),timeStamp:performance.now()})
    };
    const access={inputs:new Map([[input.id,input]]),outputs:new Map(),sysexEnabled:false,onstatechange:null,addEventListener:()=>{},removeEventListener:()=>{}};
    Object.defineProperty(navigator,'requestMIDIAccess',{configurable:true,value:async()=>access});
    Object.assign(window,{__rudimentMidiWebInput:input});
  });
  await page.goto('/');
  await page.getByRole('button',{name:'MIDI connection'}).click();
  await expect(page.locator('.status-pill')).toContainText('MIDIWeb Kit · connected');
  await page.evaluate(()=>(window as unknown as {__rudimentMidiWebInput:{dispatch(data:number[]):void}}).__rudimentMidiWebInput.dispatch([0x99,38,96]));
  await page.waitForTimeout(100);
  const diagnostics=await page.evaluate(()=>(window as any).rudiment.audioDiagnostics());
  expect(diagnostics.contextState).toBe('running');
  expect(diagnostics.sampleLayers).toBe(0);
  expect(diagnostics.snareGain).toBeGreaterThan(0);
  expect(diagnostics.snareStarts).toBe(0);
  expect(diagnostics.activeVoices).toBe(0);
  expect(diagnostics.scheduledVoices).toBe(0);
  expect(diagnostics.lastSnareError).toBeNull();
});

test('multiple MIDI inputs remain silent until one device is selected', async ({ page }) => {
  await page.addInitScript(() => {
    const makeInput = (id: string, name: string) => {
      const input = {
        id, name, manufacturer: 'Rudiment Test', type: 'input', state: 'connected', connection: 'open',
        onmidimessage: null as null | ((event: unknown) => void), onstatechange: null,
        open: async () => input, close: async () => input,
        addEventListener: () => {}, removeEventListener: () => {},
        dispatch: (data: number[]) => input.onmidimessage?.({ data: new Uint8Array(data), timeStamp: performance.now() }),
      };
      return input;
    };
    const first = makeInput('kit-a', 'Kit A'), second = makeInput('kit-b', 'Kit B');
    const access = { inputs: new Map([[first.id, first], [second.id, second]]), outputs: new Map(), sysexEnabled: false, onstatechange: null, addEventListener: () => {}, removeEventListener: () => {} };
    Object.defineProperty(navigator, 'requestMIDIAccess', { configurable: true, value: async () => access });
    Object.assign(window, { __rudimentMidiInputs: { first, second } });
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'MIDI connection' }).click();
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByText('MIDI diagnostics').click();
  await page.evaluate(() => {
    const { first, second } = (window as any).__rudimentMidiInputs;
    first.dispatch([0x99, 38, 90]); second.dispatch([0x99, 38, 100]);
  });
  await expect(page.locator('.diagnostics')).toContainText('No MIDI messages received yet.');
  await page.getByLabel('Input device').selectOption('kit-b');
  await page.evaluate(() => {
    const { first, second } = (window as any).__rudimentMidiInputs;
    first.dispatch([0x99, 38, 91]); second.dispatch([0x99, 38, 101]);
  });
  await expect(page.locator('.diagnostics')).toContainText('velocity 101');
  await expect(page.locator('.diagnostics')).not.toContainText('velocity 91');
});

test('previously authorised sole MIDI input reconnects automatically', async ({ page }) => {
  await page.addInitScript(() => {
    const input = { id:'returning-kit',name:'Returning Kit',manufacturer:'Test',type:'input',state:'connected',connection:'open',onmidimessage:null,onstatechange:null,open:async()=>input,close:async()=>input };
    const access={inputs:new Map([[input.id,input]]),outputs:new Map(),sysexEnabled:false,onstatechange:null};
    Object.defineProperty(navigator,'requestMIDIAccess',{configurable:true,value:async()=>access});
    Object.defineProperty(navigator,'permissions',{configurable:true,value:{query:async()=>({state:'granted',onchange:null})}});
    localStorage.setItem('rudiment-midi-input',input.id);
  });
  await page.goto('/');
  await expect(page.locator('.status-pill')).toContainText('Returning Kit · connected');
  await expect(page.locator('.mode-segment')).toHaveCount(0);
  await expect(page.getByLabel('Select bars')).toBeVisible();
});

test('last selected MIDI input reconnects automatically when several inputs are available', async ({ page }) => {
  await page.addInitScript(() => {
    const makeInput=(id:string,name:string)=>{
      const input={id,name,manufacturer:'Rudiment Test',type:'input',state:'connected',connection:'open',onmidimessage:null,onstatechange:null,open:async()=>input,close:async()=>input};
      return input;
    };
    const first=makeInput('kit-a','Kit A'),second=makeInput('kit-b','Kit B');
    const access={inputs:new Map([[first.id,first],[second.id,second]]),outputs:new Map(),sysexEnabled:false,onstatechange:null};
    Object.defineProperty(navigator,'requestMIDIAccess',{configurable:true,value:async()=>access});
    Object.defineProperty(navigator,'permissions',{configurable:true,value:{query:async()=>({state:'granted',onchange:null})}});
  });
  await page.goto('/');
  await expect(page.locator('.status-pill')).toContainText('No MIDI drums detected');
  await page.getByRole('button',{name:'Open settings'}).click();
  await page.getByLabel('Input device').selectOption('kit-b');
  await expect(page.locator('.midi-state')).toContainText('Kit B · connected');
  await page.reload();
  await expect(page.locator('.status-pill')).toContainText('Kit B · connected');
});

test('remembered MIDI input reconnects when the browser changes its port id', async ({ page }) => {
  await page.addInitScript(() => {
    const makeInput=(id:string,name:string,manufacturer:string)=>{
      const input={id,name,manufacturer,type:'input',state:'connected',connection:'open',onmidimessage:null,onstatechange:null,open:async()=>input,close:async()=>input};
      return input;
    };
    const remembered=makeInput('new-browser-id','Stage Drums','Acme'),other=makeInput('other-kit','Other Kit','Acme');
    const access={inputs:new Map([[remembered.id,remembered],[other.id,other]]),outputs:new Map(),sysexEnabled:false,onstatechange:null};
    Object.defineProperty(navigator,'requestMIDIAccess',{configurable:true,value:async()=>access});
    Object.defineProperty(navigator,'permissions',{configurable:true,value:{query:async()=>({state:'granted',onchange:null})}});
    localStorage.setItem('rudiment-midi-input','old-browser-id');
    localStorage.setItem('rudiment-midi-input-identity',JSON.stringify({name:'Stage Drums',manufacturer:'Acme'}));
  });
  await page.goto('/');
  await expect(page.locator('.status-pill')).toContainText('Stage Drums · connected');
  expect(await page.evaluate(()=>localStorage.getItem('rudiment-midi-input'))).toBe('new-browser-id');
});

test('auto-connect waits for the remembered MIDI input when a different input is present', async ({ page }) => {
  await page.addInitScript(() => {
    const input={id:'other-kit',name:'Other Kit',manufacturer:'Acme',type:'input',state:'connected',connection:'open',onmidimessage:null,onstatechange:null,open:async()=>input,close:async()=>input};
    const access={inputs:new Map([[input.id,input]]),outputs:new Map(),sysexEnabled:false,onstatechange:null};
    Object.defineProperty(navigator,'requestMIDIAccess',{configurable:true,value:async()=>access});
    Object.defineProperty(navigator,'permissions',{configurable:true,value:{query:async()=>({state:'granted',onchange:null})}});
    localStorage.setItem('rudiment-midi-input','remembered-kit');
    localStorage.setItem('rudiment-midi-input-identity',JSON.stringify({name:'Stage Drums',manufacturer:'Acme'}));
  });
  await page.goto('/');
  await expect(page.locator('.status-pill')).toContainText('No MIDI drums detected');
  expect(await page.evaluate(()=>localStorage.getItem('rudiment-midi-input'))).toBe('remembered-kit');
});

test('unplayed notes stay neutral and the playhead interpolates continuously', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByRole('button',{name:'Open settings'}).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button',{name:'Close settings'}).first().click();
  await page.getByRole('button',{name:/Start practice/}).click();
  // SVG lines have a zero-width bounding box, so Playwright correctly reports
  // no box even while the stroked playhead is visibly rendered.
  await expect(page.locator('#playhead')).toHaveClass(/visible/);
  const first=Number(await page.locator('#playhead').getAttribute('data-x'));
  await expect.poll(async()=>Number(await page.locator('#playhead').getAttribute('data-x')),{timeout:1500}).toBeGreaterThan(first);
  expect(Number(await page.locator('#playhead').getAttribute('x1'))).toBeGreaterThan(0);
  expect(await page.locator('#notation').evaluate(element=>getComputedStyle(element).scrollBehavior)).toBe('auto');
  expect(await page.locator('#notation').evaluate(element=>getComputedStyle(element).scrollbarWidth)).toBe('none');
  expect(await page.locator('#notation').evaluate(element=>getComputedStyle(element,'::-webkit-scrollbar').display)).toBe('none');
  await page.waitForTimeout(350);
  await expect(page.locator('.score-note[data-judgement="missed"]')).toHaveCount(0);
  expect(await page.locator('#notation').evaluate(element=>element.scrollLeft)).toBe(0);
  const frameStats=await page.evaluate(()=>new Promise<{median:number,p95:number}>(resolve=>{const deltas:number[]=[];let previous=performance.now();const sample=(now:number)=>{deltas.push(now-previous);previous=now;if(deltas.length<40)requestAnimationFrame(sample);else{const sorted=[...deltas.slice(1)].sort((a,b)=>a-b);resolve({median:sorted[Math.floor(sorted.length*.5)],p95:sorted[Math.floor(sorted.length*.95)]});}};requestAnimationFrame(sample);}));
  expect(frameStats.median).toBeLessThan(20);expect(frameStats.p95).toBeLessThan(40);
  await page.getByRole('button',{name:/Pause/}).click();const paused=await page.locator('#playhead').getAttribute('data-x');await page.waitForTimeout(120);expect(await page.locator('#playhead').getAttribute('data-x')).toBe(paused);
  await page.getByRole('button',{name:/Resume/}).click();
  await expect.poll(()=>page.locator('#playhead').getAttribute('data-x')).not.toBe(paused);
  await expect(page.locator('#playhead')).toHaveClass(/visible/);
});

test('playhead continues moving after entering bar two', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.evaluate(() => {
    (window as any).__barTwoMotion = new Promise(resolve => {
      let sawBarTwo = false;
      let minimum = Number.POSITIVE_INFINITY;
      let maximum = Number.NEGATIVE_INFINITY;
      const sample = () => {
        const playhead = document.querySelector('#playhead');
        const bar = playhead?.getAttribute('data-bar');
        const x = Number(playhead?.getAttribute('x1'));
        if (bar === '1') {
          sawBarTwo = true;
          minimum = Math.min(minimum, x);
          maximum = Math.max(maximum, x);
        }
        if (sawBarTwo && bar !== '1') {
          resolve({ minimum, maximum });
          return;
        }
        requestAnimationFrame(sample);
      };
      requestAnimationFrame(sample);
    });
  });
  await page.getByRole('button', { name: /Start practice/ }).click();

  const motion = await page.evaluate(() => (window as any).__barTwoMotion as Promise<{ minimum: number; maximum: number }>);
  expect(motion.maximum - motion.minimum).toBeGreaterThan(100);
  await expect(page.locator('#playhead')).not.toHaveAttribute('style', /transform/);
  await page.getByRole('button', { name: /Restart/ }).click();
});

test('count-in for a selected bar never traverses the preceding bar', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByLabel('Select bars').click();
  await page.getByLabel('First selected bar').selectOption('3');
  await page.getByLabel('Last selected bar').selectOption('3');
  await page.getByLabel('Close bar selection').click();
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByLabel('Count-in').selectOption('1');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await expect(page.getByLabel('Select bars')).toContainText('BAR 4 / 4');

  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect.poll(() => page.locator('#notation').evaluate(element => element.scrollTop)).toBeGreaterThan(0);
  await expect.poll(() => page.evaluate(() => (window as any).rudiment.audioDiagnostics().currentBeat)).toBeGreaterThan(8.25);
  expect(await page.evaluate(() => (window as any).rudiment.audioDiagnostics().currentBeat)).toBeLessThan(12);
  await expect(page.locator('#playhead')).not.toHaveClass(/visible/);

  await expect(page.locator('#playhead')).toHaveClass(/visible/, { timeout: 3_000 });
  await expect(page.locator('#playhead')).toHaveAttribute('data-bar', '3');
  await expect(page.getByText(/BAR 4 \/ 4/)).toBeVisible();
  await page.getByRole('button', { name: /Restart/ }).click();
});

test('early first downbeat and later loop hits stay on the selected bar', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByLabel('Select bars').click();
  await page.getByLabel('First selected bar').selectOption('1');
  await page.getByLabel('Last selected bar').selectOption('1');
  await page.getByLabel('Close bar selection').click();
  await page.getByLabel('Loops').selectOption('4');
  await page.getByRole('button', { name: /Start practice/ }).click();

  const firstHit = page.evaluate(() => new Promise<void>(resolve => {
    const sample = () => {
      const beat = (window as any).rudiment.audioDiagnostics().currentBeat;
      if (beat >= 3.82 && beat < 4) {
        window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true }));
        resolve();
      } else requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  }));
  await firstHit;
  await expect(page.locator('.score-note[data-beat="4"]')).toHaveAttribute('data-judgement', /perfect|good|early/);

  await page.evaluate(() => new Promise<void>(resolve => {
    const sample = () => {
      const beat = (window as any).rudiment.audioDiagnostics().currentBeat;
      if (beat >= 8.02 && beat < 8.3) {
        for (let index = 0; index < 6; index++)
          window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true }));
        resolve();
      } else requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  }));
  await expect(page.locator('.score-note[data-beat="4"]')).toHaveAttribute('data-judgement', /perfect|good|early|late/);
  await expect(page.locator('.unmatched-hit-marker').first()).toHaveAttribute('data-system-row', '1');
  await expect(page.locator('.score-note[data-score-bar="2"][data-judgement]')).toHaveCount(0);
  await page.evaluate(() => new Promise<void>(resolve => {
    const sample = () => {
      const beat = (window as any).rudiment.audioDiagnostics().currentBeat;
      if (beat >= 12.02 && beat < 12.3) {
        for (let index = 0; index < 6; index++)
          window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true }));
        resolve();
      } else requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  }));
  await expect(page.locator('.unmatched-hit-marker').first()).toHaveAttribute('data-system-row', '1');
  await page.getByRole('button', { name: /Restart/ }).click();
});

test('score keeps one bar per system and follows playback vertically without horizontal scrolling',async({page})=>{
  await page.goto('/');
  await page.evaluate(()=>{
    const hits=Array.from({length:64},(_,index)=>({beat:index*.5,notation_beat:index*.5,notation_duration_beats:.5,hand:index%2===0?'R':'L',accent:false}));
    (window as unknown as {rudiment:{renderScore(exercise:unknown):void}}).rudiment.renderScore({title:'Multi-line Test',meter_numerator:4,meter_denominator:4,hits});
  });
  const score=page.locator('#notation'),svg=score.locator('svg');
  await expect(svg).toHaveAttribute('aria-label',/Multi-line Test percussion notation/);
  await expect(svg).toHaveAttribute('data-staff-bounds',/^\[/);
  await expect.poll(()=>svg.getAttribute('data-bars-per-line')).toBe('1');
  await expect(svg).toHaveAttribute('data-staff-lines','5');
  const desktopRows=await page.locator('.score-note').evaluateAll(notes=>new Set(notes.map(note=>note.getAttribute('data-system-row'))).size);
  expect(desktopRows).toBeGreaterThan(1);
  const playheadCoversStaves=await page.evaluate(()=>{
    const app=(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment;
    const svg=document.querySelector<SVGSVGElement>('#notation svg')!,barsPerLine=Number(svg.dataset.barsPerLine),meter=4;
    const bounds=JSON.parse(svg.dataset.staffBounds!) as Array<{top:number;bottom:number}>;
    return bounds.map((staff,row)=>{
      app.updateScore(row*barsPerLine*meter+.1,[]);
      const line=document.querySelector('#playhead')!;
      return Number(line.getAttribute('y1'))<=staff.top&&Number(line.getAttribute('y2'))>=staff.bottom;
    });
  });
  expect(playheadCoversStaves.length).toBeGreaterThan(1);
  expect(playheadCoversStaves.every(Boolean)).toBe(true);
  const systemLayout=await page.evaluate(()=>{
    const center=(note:Element)=>{const bounds=(note.querySelector('.vf-notehead > text') as SVGGraphicsElement).getBBox();return bounds.x+bounds.width/2;};
    const firstBar=[...document.querySelectorAll('.score-note[data-score-bar="0"]')],secondBar=[...document.querySelectorAll('.score-note[data-score-bar="1"]')];
    const innerGaps=[...firstBar.slice(1).map((note,index)=>center(note)-center(firstBar[index])),...secondBar.slice(1).map((note,index)=>center(note)-center(secondBar[index]))];
    return {
      counts:JSON.parse(document.querySelector('#notation svg')?.getAttribute('data-system-bar-counts')||'[]'),
      firstStart:center(firstBar[0]),secondStart:center(secondBar[0]),
      gapSpread:Math.max(...innerGaps)-Math.min(...innerGaps),
    };
  });
  expect(systemLayout.counts.every((count:number)=>count===1)).toBe(true);
  expect(systemLayout.firstStart).toBeCloseTo(systemLayout.secondStart,1);
  expect(systemLayout.gapSpread).toBeLessThan(2);

  const cameraMotion=await page.evaluate(()=>{
    const app=(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment;
    const notation=document.querySelector<HTMLElement>('#notation')!;
    const positions=[3.99,4,4.25,4.5,4.75,5].map(beat=>{app.updateScore(beat,[]);return notation.scrollTop;});
    app.updateScore(4.5,[]);
    const repeated=notation.scrollTop;
    return {positions,repeated};
  });
  expect(Math.abs(cameraMotion.positions[1]-cameraMotion.positions[0])).toBeLessThan(1);
  expect(cameraMotion.positions.slice(1).every((position,index,items)=>index===0||position>=items[index-1])).toBe(true);
  expect(cameraMotion.positions[2]).toBeGreaterThan(cameraMotion.positions[1]);
  expect(cameraMotion.positions[4]).toBeLessThan(cameraMotion.positions[5]);
  expect(cameraMotion.repeated).toBeCloseTo(cameraMotion.positions[3],5);

  const nextSystemTransition=await page.evaluate(()=>{
    const app=(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment;
    app.updateScore(3.99,[]);const before=Number(document.querySelector('#playhead')?.getAttribute('data-x'));
    app.updateScore(4,[]);const after=Number(document.querySelector('#playhead')?.getAttribute('data-x'));
    return {before,after,row:document.querySelector('#playhead')?.getAttribute('data-row')};
  });
  expect(nextSystemTransition.row).toBe('1');
  expect(nextSystemTransition.after).toBeLessThan(nextSystemTransition.before);

  await page.setViewportSize({width:390,height:844});
  await expect.poll(()=>svg.getAttribute('data-bars-per-line')).toBe('1');
  await expect.poll(()=>score.evaluate(element=>element.querySelector('svg')?.getBoundingClientRect().width)).toBeLessThanOrEqual(390);
  expect(await score.evaluate(element=>element.scrollWidth<=element.clientWidth&&element.scrollLeft===0)).toBe(true);

  await page.evaluate(()=>{
    const app=(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment;
    app.updateScore(0,[]);const first=Number(document.querySelector('#playhead')?.getAttribute('data-x'));
    app.updateScore(.5,[]);const next=Number(document.querySelector('#playhead')?.getAttribute('data-x'));
    (window as unknown as {__scoreXProgresses:boolean}).__scoreXProgresses=next>first;
    app.updateScore(7.99,[]);const endOfBar=Number(document.querySelector('#playhead')?.getAttribute('data-x'));
    app.updateScore(8,[]);const nextBar=Number(document.querySelector('#playhead')?.getAttribute('data-x'));
    (window as unknown as {__scoreBarWraps:boolean}).__scoreBarWraps=nextBar<endOfBar;
    for(let frame=0;frame<30;frame++)app.updateScore(8,[]);
  });
  expect(await page.evaluate(()=>(window as unknown as {__scoreXProgresses:boolean}).__scoreXProgresses)).toBe(true);
  expect(await page.evaluate(()=>(window as unknown as {__scoreBarWraps:boolean}).__scoreBarWraps)).toBe(true);
  await expect(page.locator('#playhead')).toHaveAttribute('data-row','2');
  expect(await score.evaluate(element=>element.scrollTop)).toBeGreaterThan(0);
  expect(await score.evaluate(element=>element.scrollLeft)).toBe(0);
  expect(await score.evaluate(element=>{
    const viewport=element.getBoundingClientRect();
    return [2,3].every(bar=>{
      const note=element.querySelector(`.score-note[data-score-bar="${bar}"]`);
      if(!note)return false;
      const bounds=note.getBoundingClientRect();
      return bounds.top<viewport.bottom&&bounds.bottom>viewport.top;
    });
  })).toBe(true);

  await page.setViewportSize({width:1440,height:900});
  await expect.poll(()=>svg.getAttribute('data-bars-per-line')).toBe('1');
});

test('single-line snare notation is available only when explicitly selected',async({page})=>{
  await page.goto('/');
  await page.evaluate(()=>{
    const hits=Array.from({length:8},(_,index)=>({beat:index*.5,notation_beat:index*.5,notation_duration_beats:.5,instrument:'snare',note:38,hand:index%2?'L':'R'}));
    (window as unknown as {rudiment:{renderScore(exercise:unknown):void}}).rudiment.renderScore({title:'Snare Rudiment',meter_numerator:4,meter_denominator:4,hits,staff_mode:'single_line'});
  });
  await expect(page.locator('#notation svg')).toHaveAttribute('data-staff-lines','1');
});

test('kit MIDI maps to drum staff positions with mixed rhythms and one-bar systems',async({page})=>{
  await page.goto('/');
  await page.evaluate(()=>{
    const hits=[
      {beat:0,notation_beat:0,notation_duration_beats:.5,instrument:'kick',note:36,hand:'R',accent:true},
      {beat:0,notation_beat:0,notation_duration_beats:.5,instrument:'snare',note:38,hand:'R',accent:true},
      {beat:0,notation_beat:0,notation_duration_beats:.5,instrument:'closed_hat',note:42,hand:'R',accent:true},
      {beat:1.5,notation_beat:1.5,notation_duration_beats:.75,instrument:'snare',note:38,hand:'L',accent:false},
      {beat:2.25,notation_beat:2.25,notation_duration_beats:.25,instrument:'snare',note:38,hand:'R',accent:false},
      {beat:3,notation_beat:3,notation_duration_beats:.5,instrument:'snare',note:40,hand:'L',accent:false},
      {beat:4,notation_beat:4,notation_duration_beats:.5,instrument:'ride',note:51,hand:'R',accent:false},
      {beat:6,notation_beat:6,notation_duration_beats:.5,instrument:'tom',note:50,hand:'L',accent:false},
      {beat:7,notation_beat:7,notation_duration_beats:.5,instrument:'closed_hat',note:42,hand:'R',accent:false},
      {beat:9,notation_beat:9,notation_duration_beats:.5,instrument:'snare',note:38,hand:'L',accent:false},
      {beat:10,notation_beat:10,notation_duration_beats:.5,instrument:'crash',note:49,hand:'R',accent:true},
      {beat:12,notation_beat:12,notation_duration_beats:.5,instrument:'kick',note:36,hand:'R',accent:false},
      {beat:12.5,notation_beat:12.5,notation_duration_beats:.5,instrument:'snare',note:40,hand:'L',accent:false}
    ];
    (window as unknown as {rudiment:{renderScore(exercise:unknown):void}}).rudiment.renderScore({title:'Kit and mixed meter',meter_numerator:6,meter_denominator:8,hits});
  });
  const svg=page.locator('#notation svg');
  await expect(svg).toHaveAttribute('data-staff-lines','5');
  await expect(svg).toHaveAttribute('data-system-bar-counts');
  const layout=await svg.evaluate(element=>({
    counts:JSON.parse(element.getAttribute('data-system-bar-counts')!),
    widths:JSON.parse(element.getAttribute('data-system-widths')!),
    barsPerLine:Number(element.getAttribute('data-bars-per-line'))
  }));
  expect(layout.counts.reduce((sum:number,count:number)=>sum+count,0)).toBe(5);
  expect(layout.barsPerLine).toBe(1);
  expect(layout.counts.every((count:number)=>count===1)).toBe(true);
  expect(Math.max(...layout.widths)-Math.min(...layout.widths)).toBeLessThan(1);
  await expect(page.locator('.score-measure-number')).toHaveCount(5);
  await expect(page.locator('.score-barline')).toHaveCount(0);
  await expect(page.locator('[data-score-rest="true"]').first()).toBeVisible();
  await expect(page.locator('.score-note[data-beat="1.5"]')).toHaveAttribute('data-dotted','true');
  const chord=page.locator('.score-note[data-beat="0"]');
  await expect(chord).toHaveAttribute('data-midi-notes','36,38,42');
  const chordY=await chord.locator('.vf-notehead').evaluateAll(heads=>heads.map(head=>(head as SVGGraphicsElement).getBBox().y));
  expect(new Set(chordY).size).toBeGreaterThan(1);
});

test('triplet onsets render as grouped triplet notation',async({page})=>{
  await page.goto('/');
  await page.evaluate(()=>{
    const hits=Array.from({length:12},(_,index)=>({beat:index/3,notation_beat:index/3,notation_duration_beats:1/3,instrument:'snare',note:38,hand:index%2?'L':'R'}));
    (window as unknown as {rudiment:{renderScore(exercise:unknown):void}}).rudiment.renderScore({title:'Triplet Test',meter_numerator:4,meter_denominator:4,hits});
  });
  await expect(page.locator('.score-note[data-written-duration="8t"]')).toHaveCount(12);
  await expect(page.locator('.vf-tuplet')).toHaveCount(4);
});

test('playhead interpolates between noteheads and aligns with each note beat',async({page})=>{
  await page.goto('/');
  const hit=page.locator('.score-note[data-beat="1"]');
  const headX=await hit.locator('.vf-notehead > text').evaluate(head=>{
    const bounds=(head as SVGGraphicsElement).getBBox();return bounds.x+bounds.width/2;
  });
  const xAtBeat=async(beat:number)=>page.evaluate(value=>{
    (window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment.updateScore(value,[]);
    return Number(document.querySelector('#playhead')?.getAttribute('data-x'));
  },beat);
  const exact=await xAtBeat(1);
  const before=await xAtBeat(.99);
  const between=await xAtBeat(1.25);
  const after=await xAtBeat(1.5);
  expect(exact).toBeCloseTo(headX,1);
  expect(before).toBeLessThan(exact);
  expect(between).toBeGreaterThan(exact);
  expect(between).toBeLessThan(after);
});

test('playhead keeps an even beat spacing across measures with different rhythms',async({page})=>{
  await page.goto('/');
  await page.evaluate(()=>{
    const hits=[
      ...Array.from({length:4},(_,index)=>({beat:index,notation_beat:index,notation_duration_beats:1,instrument:'snare',note:38,hand:index%2?'L':'R'})),
      {beat:4,notation_beat:4,notation_duration_beats:2,instrument:'snare',note:38,hand:'R'},
      {beat:6,notation_beat:6,notation_duration_beats:2,instrument:'snare',note:38,hand:'L'},
      ...Array.from({length:8},(_,index)=>({beat:8+index*.5,notation_beat:8+index*.5,notation_duration_beats:.5,instrument:'snare',note:38,hand:index%2?'L':'R'}))
    ];
    (window as unknown as {rudiment:{renderScore(exercise:unknown):void}}).rudiment.renderScore({title:'Different rhythms',meter_numerator:4,meter_denominator:4,hits});
  });
  await expect(page.locator('#notation svg')).toHaveAttribute('data-bars-per-line','1');
  const beatSpacing=await page.evaluate(()=>{
    const app=(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment;
    return Array.from({length:3},(_,bar)=>Array.from({length:7},(_,step)=>{
      const start=bar*4+.25+step*.5;
      app.updateScore(start,[]);const before=Number(document.querySelector('#playhead')?.getAttribute('data-x'));
      app.updateScore(start+.5,[]);const after=Number(document.querySelector('#playhead')?.getAttribute('data-x'));
      return after-before;
    })).flat();
  });
  expect(Math.max(...beatSpacing)-Math.min(...beatSpacing)).toBeLessThan(beatSpacing.reduce((sum,value)=>sum+value,0)/beatSpacing.length*.2);
});

test('playhead keeps a constant visual speed through the end of the final bar',async({page})=>{
  await page.goto('/');
  const measureDistances=()=>page.evaluate(()=>{
    const app=(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment;
    const positions=[14.5,15,15.5,16].map(beat=>{
      app.updateScore(beat,[]);
      return Number(document.querySelector('#playhead')?.getAttribute('data-x'));
    });
    return positions.slice(1).map((x,index)=>x-positions[index]);
  });
  let distances=await measureDistances();
  const reference=(distances[0]+distances[1])/2;
  expect(distances[2]).toBeGreaterThan(reference*.9);
  expect(distances[2]).toBeLessThan(reference*1.1);
  await page.setViewportSize({width:390,height:844});
  await expect.poll(()=>page.locator('#notation svg').getAttribute('data-bars-per-line')).toBe('1');
  distances=await measureDistances();
  expect(Math.max(...distances)-Math.min(...distances)).toBeLessThan(1);
});

test('note glyphs keep their color and opacity as the playhead passes',async({page})=>{
  await page.goto('/');
  const first=page.locator('.score-note[data-beat="0"]');
  const before=await first.evaluate(note=>{
    const head=note.querySelector('.vf-notehead > text')!;
    return {fill:getComputedStyle(head).fill,opacity:getComputedStyle(note).opacity,filter:getComputedStyle(note).filter};
  });
  await page.evaluate(()=>{
    const update=(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment.updateScore;
    for(const beat of [0,1,2,3.5,4,8])update(beat,[]);
  });
  const after=await first.evaluate(note=>{
    const head=note.querySelector('.vf-notehead > text')!;
    return {fill:getComputedStyle(head).fill,opacity:getComputedStyle(note).opacity,filter:getComputedStyle(note).filter};
  });
  expect(after).toEqual(before);
  expect(after.fill).toBe('rgb(0, 0, 0)');
});

test('notehead feedback and unmatched strike markers stay at their own timestamps',async({page})=>{
  await page.goto('/');
  const hit=page.locator('.score-note[data-beat="1"]');
  const originalFill=await hit.locator(':scope > .vf-notehead > text').first().evaluate(element=>getComputedStyle(element).fill);
  const originalOpacity=await hit.evaluate(element=>getComputedStyle(element).opacity);
  await page.evaluate(()=>(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment.updateScore(1,[
    {expected_beat:1,actual_beat:1.01,judgement:'perfect',error_ms:4,excellent:true,event_id:'hit'},
    {expected_beat:null,actual_beat:1.25,judgement:'extra',error_ms:0,excellent:false,event_id:'extra'}
  ]));
  await expect(hit).toHaveAttribute('data-judgement','perfect');
  await expect(hit).toHaveClass(/excellent-flash/);
  expect(await hit.locator(':scope > .vf-notehead > text').first().evaluate(element=>getComputedStyle(element).fill)).toBe('rgb(13, 118, 83)');
  await expect(hit.locator(':scope > .vf-notehead').first()).toHaveAttribute('aria-label','perfect +4 milliseconds');
  const halo=hit.locator(':scope > .vf-notehead > .judgement-halo');
  await expect(halo).toHaveCount(1);
  expect(await halo.evaluate(element=>getComputedStyle(element).fill)).toBe('rgb(22, 132, 95)');
  await expect.poll(async()=>Number(await halo.evaluate(element=>getComputedStyle(element).opacity))).toBeGreaterThan(.6);
  const haloSize=await halo.evaluate(element=>{
    const head=element.parentElement!.querySelector(':scope > text') as SVGGraphicsElement,bounds=head.getBBox();
    return {radius:Number(element.getAttribute('r')),headWidth:bounds.width};
  });
  expect(haloSize.radius).toBeCloseTo(haloSize.headWidth*.84,5);
  expect(await halo.evaluate(element=>getComputedStyle(element).filter)).toContain('drop-shadow');
  expect(await halo.evaluate(element=>element.compareDocumentPosition(element.parentElement!.querySelector(':scope > text')!)&Node.DOCUMENT_POSITION_FOLLOWING)).toBeTruthy();
  const marker=page.locator('.unmatched-hit-marker');await expect(marker).toHaveCount(1);await expect(marker).toHaveAttribute('data-beat','1.25');
  const markerX=Number(await marker.getAttribute('cx')),expectedX=await hit.evaluate(element=>{const box=(element as SVGGElement).getBBox();return box.x+box.width/2;});expect(markerX).not.toBeCloseTo(expectedX,1);
  const unmatchedY=await marker.evaluate(element=>{
    const notes=[...document.querySelectorAll<SVGGElement>('.score-note')];
    const nearest=notes.reduce((best,note)=>Math.abs(Number(note.dataset.beat)-1.25)<Math.abs(Number(best.dataset.beat)-1.25)?note:best,notes[0]);
    const head=nearest.querySelector('.vf-notehead > text')!,markerRect=element.getBoundingClientRect(),headRect=head.getBoundingClientRect();
    return {marker:markerRect.y+markerRect.height/2,head:headRect.y+headRect.height/2};
  });
  expect(unmatchedY.marker).toBeCloseTo(unmatchedY.head,0);
  await page.evaluate(()=>(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment.updateScore(1.5,[{expected_beat:1,actual_beat:null,judgement:'missed',error_ms:0,excellent:false,event_id:null}]));
  await expect(hit).toHaveAttribute('data-judgement','missed');
  await expect(hit.locator('.judgement-halo')).toHaveCount(0);
  await expect(hit.locator('.missed-note-marker')).toHaveCount(0);
  await expect(hit.locator(':scope > .vf-notehead > circle')).toHaveCount(0);
  expect(await hit.locator(':scope > .vf-notehead > text').first().evaluate(element=>getComputedStyle(element).fill)).toBe(originalFill);

  await page.getByRole('button',{name:/Start practice/}).click();
  await page.keyboard.press('r');
  await expect(page.getByRole('button',{name:/Start practice/})).toBeVisible();
  await expect(page.locator('.feedback-strip')).toHaveCount(0);
  expect(await hit.evaluate(element=>getComputedStyle(element).opacity)).toBe(originalOpacity);
  await expect(marker).toHaveCount(0);
});

test('adding live feedback preserves existing notehead feedback nodes',async({page})=>{
  await page.goto('/');
  const preserved=await page.evaluate(()=>{
    const app=(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment;
    const first={expected_beat:0,actual_beat:0,judgement:'good',error_ms:20,excellent:false,event_id:'first'};
    const second={expected_beat:.5,actual_beat:.5,judgement:'perfect',error_ms:2,excellent:true,event_id:'second'};
    app.updateScore(.25,[first]);
    const original=document.querySelector('.score-note[data-beat="0"] .judgement-halo');
    app.updateScore(.75,[first,second]);
    return Boolean(original)&&original===document.querySelector('.score-note[data-beat="0"] .judgement-halo');
  });
  expect(preserved).toBe(true);
  await expect(page.locator('.judgement-halo')).toHaveCount(2);
});

test('unmatched marker positions preserve millisecond offsets without rhythmic snapping',async({page})=>{
  await page.goto('/');
  const offsets=[0,7,20,43,80,-35,-91];
  await page.evaluate(values=>{
    const baseBeat=1.5;
    (window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment.updateScore(2,values.map((offset,index)=>({
      expected_beat:null,actual_beat:baseBeat+offset/1000,judgement:'extra',error_ms:0,excellent:false,event_id:`precision-${index}`
    })));
  },offsets);
  const markers=await page.locator('.unmatched-hit-marker').evaluateAll(elements=>elements.map(element=>({
    beat:Number((element as SVGCircleElement).dataset.beat),x:Number(element.getAttribute('cx'))
  })));
  expect(markers).toHaveLength(offsets.length);
  expect(new Set(markers.map(marker=>marker.x)).size).toBe(offsets.length);
  for(let index=0;index<offsets.length;index++)expect(markers[index].beat).toBeCloseTo(1.5+offsets[index]/1000,10);
  const scale=(markers[4].x-markers[0].x)/.08;
  expect(scale).toBeGreaterThan(0);
  for(let index=0;index<offsets.length;index++)expect(markers[index].x).toBeCloseTo(markers[0].x+offsets[index]/1000*scale,5);
});

test('an unmatched strike before a bar line stays on the preceding system',async({page})=>{
  await page.goto('/');
  await page.evaluate(()=>(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment.updateScore(4,[{
    expected_beat:null,actual_beat:3.999,judgement:'extra',error_ms:0,excellent:false,event_id:'before-bar-line'
  }]));
  const marker=page.locator('.unmatched-hit-marker');
  await expect(marker).toHaveAttribute('data-system-row','0');
  const alignment=await marker.evaluate(element=>{
    const centre=(target:Element)=>{const box=target.getBoundingClientRect();return box.top+box.height/2;};
    const row0=document.querySelector('.score-note[data-system-row="0"] .vf-notehead > text')!;
    const row1=document.querySelector('.score-note[data-system-row="1"] .vf-notehead > text')!;
    const markerY=centre(element);
    return {row0:Math.abs(markerY-centre(row0)),row1:Math.abs(markerY-centre(row1))};
  });
  expect(alignment.row0).toBeLessThan(2);
  expect(alignment.row1).toBeGreaterThan(20);
});

test('restarting practice clears the previous session note colours',async({page})=>{
  await page.goto('/');
  const hit=page.locator('.score-note[data-beat="1"]');
  const originalFill=await hit.locator(':scope > .vf-notehead > text').first().evaluate(element=>getComputedStyle(element).fill);
  await page.getByRole('button',{name:/Start practice/}).click();
  await page.evaluate(()=>(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment.updateScore(1,[{expected_beat:1,actual_beat:1,judgement:'good',error_ms:30,excellent:false,event_id:'previous-session-hit'}]));
  await expect(hit).toHaveAttribute('data-judgement','good');
  await expect(hit.locator('.judgement-halo')).toHaveCount(1);
  expect(await hit.locator(':scope > .vf-notehead > text').first().evaluate(element=>getComputedStyle(element).fill)).toBe('rgb(36, 147, 87)');

  await page.getByRole('button',{name:/Restart/}).click();
  await expect(page.getByRole('button',{name:/Start practice/})).toBeVisible();
  await expect(page.getByText(/Count-in ·/)).toHaveCount(0);
  await expect(page.locator('.feedback-strip')).toHaveCount(0);
  await expect(hit).not.toHaveAttribute('data-judgement','good');
  await expect(hit.locator('.judgement-halo')).toHaveCount(0);
  expect(await hit.locator(':scope > .vf-notehead > text').first().evaluate(element=>getComputedStyle(element).fill)).toBe(originalFill);
});

test('all five timing judgements colour the notehead and expose text alternatives',async({page})=>{
  await page.goto('/');
  const missedHead=page.locator('.score-note[data-beat="2"] .vf-notehead > text').first();
  const missedOriginalFill=await missedHead.evaluate(element=>getComputedStyle(element).fill);
  await page.evaluate(()=>(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment.updateScore(2,[
    {expected_beat:0,actual_beat:0,judgement:'perfect',error_ms:0,excellent:true,note_score:100,event_id:'perfect'},
    {expected_beat:.5,actual_beat:.53,judgement:'good',error_ms:30,excellent:false,note_score:90,event_id:'good'},
    {expected_beat:1,actual_beat:.94,judgement:'early',error_ms:-60,excellent:false,note_score:53.7,event_id:'early'},
    {expected_beat:1.5,actual_beat:1.56,judgement:'late',error_ms:60,excellent:false,note_score:53.7,event_id:'late'},
    {expected_beat:2,actual_beat:null,judgement:'missed',error_ms:0,excellent:false,note_score:0,event_id:null}
  ]));
  const expected=[
    {beat:'0',judgement:'perfect',fill:'rgb(13, 118, 83)',label:'perfect +0 milliseconds'},
    {beat:'0.5',judgement:'good',fill:'rgb(36, 147, 87)',label:'good +30 milliseconds'},
    {beat:'1',judgement:'early',fill:'rgb(197, 91, 20)',label:'early -60 milliseconds'},
    {beat:'1.5',judgement:'late',fill:'rgb(112, 64, 167)',label:'late +60 milliseconds'},
    {beat:'2',judgement:'missed',fill:missedOriginalFill,label:'missed'}
  ];
  for(const item of expected){
    const note=page.locator(`.score-note[data-beat="${item.beat}"]`).first(),head=note.locator(':scope > .vf-notehead').first();
    await expect(note).toHaveAttribute('data-judgement',item.judgement);
    await expect(head).toHaveAttribute('aria-label',item.label);
    expect(await head.locator(':scope > text').first().evaluate(element=>getComputedStyle(element).fill)).toBe(item.fill);
  }
  await expect(page.locator('.score-note[data-beat="2"] .judgement-halo')).toHaveCount(0);
  await expect(page.locator('.score-note[data-beat="2"] .missed-note-marker')).toHaveCount(0);
});

test('missed notes keep their original notehead color',async({page})=>{
  await page.goto('/');
  const note=page.locator('.score-note[data-beat="2"]').first();
  const notehead=note.locator('.vf-notehead').first();
  const glyph=notehead.locator(':scope > text').first();
  const originalFill=await glyph.evaluate(element=>getComputedStyle(element).fill);
  await page.evaluate(()=>(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment.updateScore(2,[
    {expected_beat:2,actual_beat:null,judgement:'missed',error_ms:0,excellent:false,event_id:null}
  ]));
  await expect(note).toHaveAttribute('data-judgement','missed');
  await expect(notehead).toHaveAttribute('data-judgement','missed');
  expect(await glyph.evaluate(element=>getComputedStyle(element).fill)).toBe(originalFill);
  await expect(notehead.locator(':scope > .judgement-halo')).toHaveCount(0);
  await expect(notehead.locator(':scope > circle')).toHaveCount(0);
});

test('rapid repeated monitored strikes allocate an audio voice for every hit', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(() => { const prototype=AudioBufferSourceNode.prototype,original=prototype.start;(window as unknown as {__voiceStarts:number}).__voiceStarts=0;prototype.start=function(...args){(window as unknown as {__voiceStarts:number}).__voiceStarts++;return original.apply(this,args as [number]);}; });
  await page.evaluate(()=>(window as unknown as {rudiment:{ensureAudio():Promise<void>}}).rudiment.ensureAudio());
  await expect.poll(()=>page.evaluate(()=>(window as any).rudiment.audioDiagnostics().sampleLayers)).toBe(4);
  const levels=await page.evaluate(()=>(window as unknown as {rudiment:{audioDiagnostics():{layerPeaks:number[],layerGains:number[],velocityGains:number[],velocityLayers:Array<Array<{index:number,weight:number}>>},playSnare(velocity:number):boolean}}).rudiment.audioDiagnostics());
  expect(levels.layerPeaks).toHaveLength(4);
  expect(levels.layerGains).toHaveLength(4);
  for(let index=0;index<4;index++)expect(levels.layerPeaks[index]*levels.layerGains[index]).toBeCloseTo(.32,2);
  expect(levels.velocityGains).toHaveLength(127);expect(levels.velocityGains[0]).toBeGreaterThan(0);expect(levels.velocityGains[126]).toBeCloseTo(1,5);expect(levels.velocityGains[126]/levels.velocityGains[0]).toBeGreaterThan(30);
  for(let index=1;index<127;index++){expect(levels.velocityGains[index]).toBeGreaterThan(levels.velocityGains[index-1]);expect(levels.velocityGains[index]-levels.velocityGains[index-1]).toBeLessThan(.03);}
  for(const blend of levels.velocityLayers)expect(blend.reduce((sum,layer)=>sum+layer.weight*layer.weight,0)).toBeCloseTo(1,5);
  const beforeLow=await page.evaluate(()=>(window as unknown as {__voiceStarts:number}).__voiceStarts);expect(await page.evaluate(()=>(window as unknown as {rudiment:{playSnare(velocity:number):boolean}}).rudiment.playSnare(1))).toBe(true);await expect.poll(()=>page.evaluate(()=>(window as unknown as {__voiceStarts:number}).__voiceStarts)).toBeGreaterThan(beforeLow);
  const beforeZero=await page.evaluate(()=>(window as unknown as {__voiceStarts:number}).__voiceStarts);expect(await page.evaluate(()=>(window as unknown as {rudiment:{playSnare(velocity:number):boolean}}).rudiment.playSnare(0))).toBe(false);expect(await page.evaluate(()=>(window as unknown as {__voiceStarts:number}).__voiceStarts)).toBe(beforeZero);
  await page.evaluate(() => {for(let i=0;i<40;i++)window.dispatchEvent(new KeyboardEvent('keydown',{key:'j',bubbles:true}));});
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {__voiceStarts:number}).__voiceStarts)).toBeGreaterThanOrEqual(41);
});

test('reference snare can follow written notes instead of physical hits', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByLabel('Count-in').selectOption('0');
  const playbackMode = page.getByLabel('Reference snare timing');
  await playbackMode.selectOption('notes');
  await expect(playbackMode).toHaveValue('notes');
  await expect.poll(() => page.evaluate(() => new Promise<string | undefined>((resolve, reject) => {
    const request = indexedDB.open('rudiment');
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const get = request.result.transaction('settings').objectStore('settings').get('app');
      get.onerror = () => reject(get.error);
      get.onsuccess = () => resolve(get.result?.value?.snareReferenceMode);
    };
  }))).toBe('notes');
  await page.getByRole('button', { name: 'Close settings' }).first().click();

  const before = await page.evaluate(() => (window as any).rudiment.audioDiagnostics().snareStarts);
  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect.poll(() => page.evaluate(() => (window as any).rudiment.audioDiagnostics().referenceNoteCount)).toBe(32);
  await expect.poll(() => page.evaluate(() => (window as any).rudiment.audioDiagnostics().snareStarts)).toBeGreaterThan(before);

  await page.getByRole('button', { name: /Pause/ }).click();
  await page.waitForTimeout(100);
  const paused = await page.evaluate(() => (window as any).rudiment.audioDiagnostics().snareStarts);
  await page.keyboard.press('j');
  await page.waitForTimeout(150);
  expect(await page.evaluate(() => (window as any).rudiment.audioDiagnostics().snareStarts)).toBe(paused);

  await page.reload();
  await page.getByRole('button', { name: 'Open settings' }).click();
  await expect(page.getByLabel('Reference snare timing')).toHaveValue('notes');
});

test('loop and restart leave practice stopped until started again', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.getByLabel('Select bars').click();
  await page.getByLabel('First selected bar').selectOption('0');
  await page.getByLabel('Last selected bar').selectOption('0');
  await page.getByLabel('Close bar selection').click();
  await page.getByLabel('Loops').selectOption('2');
  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect(page.getByLabel('Loops')).toHaveValue('2');
  await expect(page.locator('#playhead')).toHaveClass(/visible/);
  await page.keyboard.press('j');
  await expect(page.locator('.score-note[data-judgement]')).toHaveCount(1);
  await expect.poll(() => page.evaluate(() => (window as any).rudiment.audioDiagnostics().currentBeat)).toBeGreaterThan(4.1);
  await expect(page.locator('.score-note[data-judgement]')).toHaveCount(0);
  await expect(page.locator('.unmatched-hit-marker')).toHaveCount(0);
  await page.getByRole('button', { name: /Restart/ }).click();
  await expect(page.getByRole('button', { name: /Start practice/ })).toBeVisible();
  await expect(page.locator('.feedback-strip')).toHaveCount(0);
});

test('repeat-until-good setting persists and retries a failed bar', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByLabel('Count-in').selectOption('0');
  const repeatBar = page.getByLabel('Repeat bar until Good');
  await repeatBar.check();
  await page.getByLabel('Repeat tolerance').selectOption('strict');
  await expect(page.getByLabel('Repeat tolerance')).toHaveValue('strict');
  await page.getByLabel('Repeat tolerance').selectOption('standard');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await expect.poll(() => page.evaluate(() => new Promise<boolean | undefined>((resolve, reject) => {
    const request = indexedDB.open('rudiment');
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const get = request.result.transaction('settings').objectStore('settings').get('app');
      get.onerror = () => reject(get.error);
      get.onsuccess = () => resolve(get.result?.value?.repeatBarUntilGood);
    };
  }))).toBe(true);
  await page.evaluate(() => {
    let started = false;
    const label = document.querySelector('.position-row span')!;
    new MutationObserver(() => {
      if (started || !label.textContent?.includes('TRY 2')) return;
      started = true;
      void (async () => {
        const firstHit = performance.now() + 20;
        for (let index = 0; index < 8; index += 1) {
          const delay = Math.max(0, firstHit + index * 125 - performance.now());
          await new Promise(resolve => setTimeout(resolve, delay));
          window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true }));
        }
      })();
    }).observe(label, { childList: true, subtree: true, characterData: true });
  });
  await page.getByRole('button', { name: /Start practice/ }).click();

  await expect(page.getByText(/BAR 1 \/ 4 · TRY 2/)).toBeVisible({ timeout: 4_000 });
  await expect(page.getByRole('button', { name: /Pause/ })).toBeVisible();
  await expect(page.getByRole('dialog', { name: 'Practice complete' })).toHaveCount(0);
  await expect(page.getByText(/BAR 2 \/ 4 · TRY 1/)).toBeVisible({ timeout: 3_000 });
  await page.getByRole('button', { name: /Restart/ }).click();

  await page.reload();
  await page.getByRole('button', { name: 'Open settings' }).click();
  await expect(page.getByLabel('Repeat bar until Good')).toBeChecked();
  await expect(page.getByLabel('Repeat tolerance')).toHaveValue('standard');
});

test('single-bar repetition preserves the visible score and reuses its SVG', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByLabel('Select bars').click();
  await page.getByLabel('First selected bar').selectOption('2');
  await page.getByLabel('Last selected bar').selectOption('2');
  await page.getByLabel('Close bar selection').click();
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByLabel('Repeat bar until Good').check();
  await page.getByRole('button', { name: 'Close settings' }).first().click();

  await page.evaluate(() => {
    const notation = document.querySelector<HTMLElement>('#notation')!;
    const originalSvg = notation.querySelector('svg')!;
    (window as any).__repeatViewport = { before: -1, after: -1, sameSvg: false, playheadBeat: -1 };
    const label = document.querySelector('.bar-selection-trigger span')!;
    new MutationObserver(() => {
      if (!label.textContent?.includes('TRY 2') || (window as any).__repeatViewport.before >= 0) return;
      (window as any).__repeatViewport.before = notation.scrollTop;
      requestAnimationFrame(() => requestAnimationFrame(() => {
        (window as any).__repeatViewport.after = notation.scrollTop;
        (window as any).__repeatViewport.sameSvg = notation.querySelector('svg') === originalSvg;
        (window as any).__repeatViewport.playheadBeat = (window as any).rudiment.audioDiagnostics().currentBeat;
      }));
    }).observe(label, { childList: true, subtree: true, characterData: true });
  });

  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect(page.getByText(/BAR 3 \/ 3 · TRY 2/)).toBeVisible({ timeout: 4_000 });
  await expect.poll(() => page.evaluate(() => (window as any).__repeatViewport.after)).toBeGreaterThanOrEqual(0);
  const viewport = await page.evaluate(() => (window as any).__repeatViewport as { before: number; after: number; sameSvg: boolean; playheadBeat: number });
  expect(viewport.before).toBeGreaterThan(0);
  expect(Math.abs(viewport.after - viewport.before)).toBeLessThan(2);
  expect(viewport.sameSvg).toBe(true);
  expect(viewport.playheadBeat).toBeGreaterThanOrEqual(8);
  expect(viewport.playheadBeat).toBeLessThan(9);
  await page.getByRole('button', { name: /Restart/ }).click();
});

test('bar retries stay on tempo and pause/resume preserves position without scoring paused hits', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('72');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByLabel('Repeat bar until Good').check();
  await page.getByRole('button', { name: 'Close settings' }).first().click();

  await page.evaluate(() => {
    (window as any).__attemptTimes = [];
    const label = document.querySelector('.position-row span')!;
    new MutationObserver(() => {
      const match = label.textContent?.match(/TRY (\d+)/);
      if (match) (window as any).__attemptTimes.push({ attempt: Number(match[1]), at: performance.now() });
    }).observe(label, { childList: true, subtree: true, characterData: true });
  });
  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect(page.getByText(/BAR 1 \/ 4 · TRY 5/)).toBeVisible({ timeout: 15_000 });
  const attempts = await page.evaluate(() => (window as any).__attemptTimes as { attempt: number; at: number }[]);
  for (let attempt = 3; attempt <= 5; attempt += 1) {
    const previous = attempts.find(item => item.attempt === attempt - 1)!;
    const current = attempts.find(item => item.attempt === attempt)!;
    expect(current.at - previous.at).toBeGreaterThan(3_200);
    expect(current.at - previous.at).toBeLessThan(3_450);
  }

  const diagnostics = () => page.evaluate(() => (window as any).rudiment.audioDiagnostics());
  await expect.poll(() => diagnostics().then(value => value.currentBeat)).toBeGreaterThan(1);
  const feedbackCount = await page.locator('.score-note[data-judgement]').count();
  await page.getByRole('button', { name: /Pause/ }).click();
  await expect(page.getByRole('button', { name: /Resume/ })).toBeVisible();
  const pausedBeat = await diagnostics().then(value => value.currentBeat);
  await page.waitForTimeout(200);
  expect((await diagnostics()).currentBeat).toBeCloseTo(pausedBeat, 2);
  await page.keyboard.press('j');
  await page.waitForTimeout(100);
  await expect(page.locator('.score-note[data-judgement]')).toHaveCount(feedbackCount);

  await page.getByRole('button', { name: /Resume/ }).click();
  await expect.poll(() => diagnostics().then(value => value.currentBeat)).toBeGreaterThanOrEqual(pausedBeat - 0.03);
  await expect.poll(() => diagnostics().then(value => value.currentBeat)).toBeGreaterThan(pausedBeat + 0.15);
  await page.getByRole('button', { name: /Restart/ }).click();
  expect((await diagnostics()).scheduledVoices).toBe(0);
});

test('top toolbar fits and preserves control order at mobile and desktop widths', async ({ page }) => {
  const widths=[320,360,375,390,430,520,1440];
  await page.goto('/');

  for(const width of widths){
    await page.setViewportSize({width,height:900});
    const layout=await page.locator('.topbar').evaluate((topbar,width)=>{
      const controls=[
        topbar.querySelector<HTMLElement>('#midi-library-trigger')!,
        topbar.querySelector<HTMLElement>('#stats-trigger')!,
        topbar.querySelector<HTMLElement>('.brand')!,
        topbar.querySelector<HTMLElement>('.tempo-control')!,
        topbar.querySelector<HTMLElement>('.settings-trigger')!,
      ].map(element=>element.getBoundingClientRect());
      const status=topbar.querySelector<HTMLElement>('.status-pill')!.getBoundingClientRect();
      const toolbar=topbar.getBoundingClientRect();
      const overlaps=(a:DOMRect,b:DOMRect)=>a.left<b.right&&a.right>b.left&&a.top<b.bottom&&a.bottom>b.top;
      return {
        horizontalOverflow:document.documentElement.scrollWidth>width||document.body.scrollWidth>width,
        controlsFit:controls.every(box=>box.left>=toolbar.left&&box.right<=toolbar.right&&box.top>=toolbar.top&&box.bottom<=toolbar.bottom),
        controlsOverlap:controls.some((box,index)=>controls.slice(index+1).some(other=>overlaps(box,other))),
        statusFits:status.left>=toolbar.left&&status.right<=toolbar.right&&status.top>=toolbar.top&&status.bottom<=toolbar.bottom,
        order:controls.map(box=>({left:box.left,top:box.top,centerY:(box.top+box.bottom)/2})),
        sourceOrder:[...topbar.children]
          .filter(element=>element.matches('#midi-library-trigger,#stats-trigger,.settings-trigger,.brand,.tempo-control'))
          .map(element=>element.getAttribute('aria-label')||'Tempo'),
        tempoWidth:controls[3].width,
      };
    },width);

    expect(layout.horizontalOverflow,`${width}px has horizontal overflow`).toBe(false);
    expect(layout.controlsFit,`${width}px clips a toolbar control`).toBe(true);
    expect(layout.controlsOverlap,`${width}px overlaps toolbar controls`).toBe(false);
    expect(layout.statusFits,`${width}px clips the MIDI status bar`).toBe(true);
    expect(layout.tempoWidth,`${width}px compresses the BPM control`).toBeGreaterThanOrEqual(149);
    expect(layout.sourceOrder).toEqual(['MIDI library · 11 exercises','Stats','Rudiment','Tempo','Open settings']);

    if(width<=360){
      const [library,stats,brand,tempo,settings]=layout.order;
      expect([library.top,stats.top,brand.top,settings.top],`${width}px primary controls do not share row one`).toEqual([library.top,library.top,library.top,library.top]);
      expect([library.left,stats.left,brand.left,settings.left],`${width}px changes primary control order`).toEqual([...([library.left,stats.left,brand.left,settings.left] as number[])].sort((a,b)=>a-b));
      expect(tempo.top,`${width}px does not place BPM on row two`).toBeGreaterThan(settings.top);
    }else if(width<=700){
      expect(layout.order.map(control=>control.left),`${width}px changes toolbar control order`).toEqual([...layout.order.map(control=>control.left)].sort((a,b)=>a-b));
      expect(Math.max(...layout.order.map(control=>control.centerY))-Math.min(...layout.order.map(control=>control.centerY)),`${width}px unnecessarily wraps toolbar controls`).toBeLessThanOrEqual(1);
    }
  }
});

test('permission denial and narrow layout remain usable', async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'requestMIDIAccess', { configurable: true, value: async () => { throw new Error('Permission denied by user'); } });
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await page.getByRole('button',{name:'MIDI connection'}).click();
  await page.getByRole('button', { name: 'Open settings' }).click();
  await expect(page.locator('.midi-state')).toContainText('MIDI error · Permission denied by user');
  await expect(page.getByRole('button',{name:'Connect / rescan MIDI'})).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Start practice/ })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Play snare hit' })).toHaveCount(0);
  await page.screenshot({ path: 'artifacts/narrow-practice.png', fullPage: true });
});

test('Pixel 8 Pro keeps MIDI connection visible and practice controls on-screen in both orientations',async({page})=>{
  await page.setViewportSize({width:448,height:998});
  await page.goto('/');
  const midi=page.getByRole('button',{name:'MIDI connection'});
  await expect(midi).toBeVisible();
  expect(await midi.evaluate(element=>{const box=element.getBoundingClientRect();return box.left>=0&&box.right<=innerWidth&&box.top>=0&&box.bottom<=innerHeight;})).toBe(true);
  const portraitFits=await page.evaluate(()=>({
    pageFits:document.documentElement.scrollHeight<=innerHeight&&document.body.scrollHeight<=innerHeight,
    controls:[...document.querySelectorAll('.topbar,.welcome,.score-card,.transport')].every(element=>{const box=element.getBoundingClientRect();return box.top>=0&&box.bottom<=innerHeight;}),
    horizontalFits:document.documentElement.scrollWidth<=innerWidth&&document.body.scrollWidth<=innerWidth,
  }));
  expect(portraitFits).toEqual({pageFits:true,controls:true,horizontalFits:true});

  await page.setViewportSize({width:998,height:448});
  await expect.poll(()=>page.locator('#notation svg').getAttribute('data-bars-per-line')).toBe('1');
  const layout=await page.evaluate(()=>({
    pageFits:document.documentElement.scrollHeight<=innerHeight&&document.body.scrollHeight<=innerHeight,
    controls:[...document.querySelectorAll('.topbar,.welcome,.score-card,.transport')].every(element=>{const box=element.getBoundingClientRect();return box.top>=0&&box.bottom<=innerHeight;}),
    horizontalFits:document.documentElement.scrollWidth<=innerWidth&&document.body.scrollWidth<=innerWidth,
  }));
  expect(layout).toEqual({pageFits:true,controls:true,horizontalFits:true});
  await expect(midi).toBeVisible();
  await expect(page.getByRole('button',{name:/Start practice/})).toBeVisible();
});

test('long scores scroll inside their panel while all practice controls remain on-screen',async({page})=>{
  await page.setViewportSize({width:1536,height:768});
  await page.goto('/');
  await page.getByRole('button',{name:'Dismiss welcome hint'}).click();
  await page.evaluate(()=>{
    const hits=Array.from({length:320},(_,index)=>({beat:index,notation_beat:index,notation_duration_beats:1,hand:index%2===0?'R':'L',accent:false}));
    (window as unknown as {rudiment:{renderScore(exercise:unknown):void}}).rudiment.renderScore({title:'Long MIDI',meter_numerator:4,meter_denominator:4,hits});
  });
  const layout=await page.evaluate(()=>({
    pageFits:document.documentElement.scrollHeight<=innerHeight&&document.body.scrollHeight<=innerHeight,
    controls:[...document.querySelectorAll('.topbar,.score-card,.transport')].every(element=>{const box=element.getBoundingClientRect();return box.top>=0&&box.bottom<=innerHeight;}),
    scoreScrollsInternally:(document.querySelector('#notation') as HTMLElement).scrollHeight>(document.querySelector('#notation') as HTMLElement).clientHeight,
  }));
  expect(layout).toEqual({pageFits:true,controls:true,scoreScrollsInternally:true});
});
