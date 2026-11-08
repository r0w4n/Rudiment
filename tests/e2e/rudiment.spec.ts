import { expect, test } from '@playwright/test';

test('first-use workspace renders real notation and lifecycle controls', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Single Paradiddle').first()).toBeVisible();
  await expect(page.getByLabel('Tempo in BPM')).toHaveValue('60');
  await expect(page.getByText('Try without drums')).toBeVisible();
  await expect(page.locator('#notation svg')).toHaveCount(1);
  await expect(page.locator('.score-note')).toHaveCount(32);
  await expect(page.locator('#notation')).toContainText('R');
  await expect(page.locator('#notation')).toContainText('L');
  await page.waitForTimeout(450);
  await page.screenshot({ path: 'artifacts/first-run-practice.png', fullPage: true });

  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect(page.getByText(/Count-in ·/)).toBeVisible();
  await page.getByRole('button', { name: /Pause/ }).click();
  await expect(page.getByText('Paused', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: /Resume/ }).click();
  await page.getByRole('button', { name: /Stop/ }).click();
  await expect(page.getByText('Stopped — results were not saved')).toBeVisible();
});

test('demo hits complete through scoring and persist locally', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: /Preview Acoustic snare/ }).click();
  await expect(page.getByText(/Acoustic snare ready/)).toBeVisible();
  await page.getByLabel('Tempo in BPM').fill('240');
  await expect(page.getByLabel('Tempo in BPM')).toHaveValue('240');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();

  await page.getByRole('button', { name: /Start practice/ }).click();
  await page.evaluate(async () => {
    const pad = document.querySelector('[aria-label="Play snare hit"]');
    await new Promise(resolve => setTimeout(resolve, 165));
    for (let i = 0; i < 32; i++) {
      pad?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerType: 'mouse' }));
      await new Promise(resolve => setTimeout(resolve, 125));
    }
  });
  await expect(page.getByText('SESSION SAVED · LOCALLY')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Good work.' })).toBeVisible();
  await expect(page.locator('.result-metrics article').first()).toContainText(/(?:[1-9]|[12]\d|3[0-2]) \/ 32/);
  await expect(page.getByText('Single Paradiddle at 240 BPM · Demo input')).toBeVisible();
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({ path: 'artifacts/completed-result.png', fullPage: true });

  await page.reload();
  await page.getByRole('button', { name: 'Open settings' }).click();
  await expect(page.getByText('1 completed session(s) stored only in this browser.')).toBeVisible();
});

test('bundled MIDI imports through the visible import callback and broken files explain failure', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Load bundled .mid' }).click();
  await expect(page.getByText('single-paradiddle', { exact: true }).first()).toBeVisible();
  await expect(page.locator('.score-note')).toHaveCount(32);
  await expect(page.locator('#notation')).toContainText('R');
  await expect(page.locator('#notation')).toContainText('L');

  const quarterChooser=page.waitForEvent('filechooser');
  await page.getByRole('button',{name:'Import MIDI'}).click();
  await (await quarterChooser).setFiles('web/public/exercises/quarter-note-reference.mid');
  await expect(page.getByText('quarter-note-reference',{exact:true}).first()).toBeVisible();
  await expect(page.locator('.score-note[data-written-duration="q"]')).toHaveCount(8);
  await expect.poll(() => page.evaluate(() => new Promise<number>((resolve, reject) => {
    const request = indexedDB.open('rudiment');
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const count = request.result.transaction('imports').objectStore('imports').count();
      count.onerror = () => reject(count.error);
      count.onsuccess = () => resolve(count.result);
    };
  }))).toBeGreaterThanOrEqual(2);
  await page.reload();
  await expect.poll(() => page.evaluate(() => new Promise<number>((resolve, reject) => {
    const request = indexedDB.open('rudiment');
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const count = request.result.transaction('imports').objectStore('imports').count();
      count.onerror = () => reject(count.error);
      count.onsuccess = () => resolve(count.result);
    };
  }))).toBeGreaterThanOrEqual(2);

  const chooserPromise = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Import MIDI' }).click();
  const chooser = await chooserPromise;
  await chooser.setFiles({ name: 'broken.mid', mimeType: 'audio/midi', buffer: Buffer.from('not-midi') });
  await expect(page.locator('.error-row')).toContainText(/malformed|truncated/i);
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
      dispatch: (data: number[]) => input.onmidimessage?.({ data: new Uint8Array(data), timeStamp: performance.now() }),
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
  await page.getByRole('button', { name: 'Connect MIDI drums' }).click();
  await expect(page.locator('.status-pill')).toContainText('Acceptance Kit · connected');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByText('MIDI diagnostics').click();
  await page.evaluate(() => {
    const input = (window as unknown as { __rudimentFakeInput: { dispatch(data: number[]): void } }).__rudimentFakeInput;
    input.dispatch([0x99, 38, 100]);
    input.dispatch([0x99, 38, 0]);
    input.dispatch([0xb9, 4, 90]);
  });
  await expect(page.locator('.diagnostics')).toContainText('note_on');
  await expect(page.locator('.diagnostics')).toContainText('note_off');
  await expect(page.locator('.diagnostics')).toContainText('cc');
  await expect(page.locator('.diagnostics')).toContainText(/153,\s*38,\s*100/);
  await expect(page.getByText('0 received')).toBeVisible();
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
  await expect(page.locator('.mode-badge')).toHaveText('MIDI INPUT');
});

test('score feedback expires misses and playhead interpolates continuously', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByRole('button',{name:'Open settings'}).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button',{name:'Close settings'}).first().click();
  await page.getByRole('button',{name:/Start practice/}).click();
  // SVG lines have a zero-width bounding box, so Playwright correctly reports
  // no box even while the stroked playhead is visibly rendered.
  await expect(page.locator('#playhead')).toHaveClass(/visible/);
  const first=Number(await page.locator('#playhead').getAttribute('x1'));await page.waitForTimeout(70);const second=Number(await page.locator('#playhead').getAttribute('x1'));
  expect(second).toBeGreaterThan(first);
  await expect(page.locator('.score-note[data-judgement="missed"]').first()).toBeVisible();
  await page.getByRole('button',{name:/Pause/}).click();const paused=await page.locator('#playhead').getAttribute('x1');await page.waitForTimeout(120);expect(await page.locator('#playhead').getAttribute('x1')).toBe(paused);
  await page.getByRole('button',{name:/Resume/}).click();
  await expect.poll(()=>page.locator('#playhead').getAttribute('x1')).not.toBe(paused);
  await expect(page.locator('#playhead')).toHaveClass(/visible/);
});

test('rapid repeated monitored strikes allocate an audio voice for every hit', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(() => { const prototype=AudioBufferSourceNode.prototype,original=prototype.start;(window as unknown as {__voiceStarts:number}).__voiceStarts=0;prototype.start=function(...args){(window as unknown as {__voiceStarts:number}).__voiceStarts++;return original.apply(this,args as [number]);}; });
  await page.getByRole('button',{name:/Preview Acoustic snare/}).click();
  await expect(page.getByText('Acoustic snare ready')).toBeVisible();
  const levels=await page.evaluate(()=>(window as unknown as {rudiment:{audioDiagnostics():{layerPeaks:number[],layerGains:number[]}}}).rudiment.audioDiagnostics());
  expect(levels.layerPeaks).toHaveLength(4);
  expect(levels.layerGains).toHaveLength(4);
  for(let index=0;index<4;index++)expect(levels.layerPeaks[index]*levels.layerGains[index]).toBeCloseTo(.32,2);
  await page.evaluate(() => {const pad=document.querySelector('[aria-label="Play snare hit"]');for(let i=0;i<40;i++)pad?.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,pointerType:'mouse'}));});
  await expect.poll(()=>page.evaluate(()=>(window as unknown as {__voiceStarts:number}).__voiceStarts)).toBeGreaterThanOrEqual(41);
});

test('loop and restart keep one authoritative transport', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByLabel('Loops').selectOption('2');
  await page.getByRole('button', { name: 'Close settings' }).first().click();
  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect(page.getByText('2 / 2 LOOP')).toBeVisible({ timeout: 6_000 });
  await page.getByRole('button', { name: /Restart/ }).click();
  await expect(page.getByText('1 / 2 LOOP')).toBeVisible();
  await page.getByRole('button', { name: /Stop/ }).click();
  await expect(page.getByText('Stopped — results were not saved')).toBeVisible();
});

test('permission denial and narrow layout remain usable', async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'requestMIDIAccess', { configurable: true, value: async () => { throw new Error('Permission denied by user'); } });
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByRole('button', { name: 'Connect / rescan MIDI' }).click();
  await expect(page.locator('.midi-state')).toContainText('MIDI error · Permission denied by user');
  await expect(page.getByRole('button', { name: /Start practice/ })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Play snare hit' })).toBeVisible();
  await page.screenshot({ path: 'artifacts/narrow-practice.png', fullPage: true });
});
