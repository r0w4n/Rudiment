import { expect, test } from '@playwright/test';

test('first-use workspace renders real notation and lifecycle controls', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Single Paradiddle').first()).toBeVisible();
  await expect(page.getByLabel('Tempo in BPM')).toHaveValue('60');
  await expect(page.getByText('Hit your drum or press J to start.', { exact: true }).last()).toBeVisible();
  await expect(page.locator('#notation svg')).toHaveCount(1);
  await expect(page.locator('.score-note')).toHaveCount(32);
  await expect(page.locator('#notation')).toContainText('R');
  await expect(page.locator('#notation')).toContainText('L');
  await expect(page.getByRole('button',{name:'MIDI connection'})).toBeVisible();
  await expect(page.getByRole('button',{name:'Stats'})).toBeVisible();
  await expect(page.getByRole('button',{name:'Play snare hit'})).toHaveCount(0);
  await expect(page.getByRole('button',{name:/Preview Acoustic snare/})).toHaveCount(0);
  await expect(page.getByRole('button',{name:'Connect MIDI drums'})).toHaveCount(0);
  await page.waitForTimeout(450);
  await page.screenshot({ path: 'artifacts/first-run-practice.png', fullPage: true });

  await page.getByRole('button', { name: /Start practice/ }).click();
  await expect(page.getByText(/Count-in ·/)).toBeVisible();
  await page.getByRole('button',{name:'Stats'}).click();
  await expect(page.getByRole('dialog',{name:'Practice statistics'})).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByText(/Count-in ·/)).toBeVisible();
  await page.getByRole('button', { name: /Pause/ }).click();
  await expect(page.getByText('Paused', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: /Resume/ }).click();
  await page.getByRole('button', { name: /Stop/ }).click();
  await expect(page.getByText('Stopped — results were not saved')).toBeVisible();
});

test('demo hits complete through scoring and persist locally', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await expect(page.getByLabel('Tempo in BPM')).toHaveValue('240');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button', { name: 'Close settings' }).first().click();

  await page.getByRole('button', { name: /Start practice/ }).click();
  await page.evaluate(async () => {
    await new Promise(resolve => setTimeout(resolve, 165));
    for (let i = 0; i < 32; i++) {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true }));
      await new Promise(resolve => setTimeout(resolve, 125));
    }
  });
  const resultDialog=page.getByRole('dialog',{name:'Good work.'});
  await expect(resultDialog).toBeVisible();
  await expect(resultDialog.getByText('SESSION SAVED · LOCALLY')).toHaveCount(0);
  await expect(resultDialog.getByRole('heading', { name: 'Good work.' })).toBeVisible();
  await expect(resultDialog.locator('.results-score-summary .score-ring')).toHaveCSS('width','76px');
  await expect(resultDialog.locator('.result-metrics article').first()).toContainText(/(?:[1-9]|[12]\d|3[0-2]) \/ 32/);
  const sessionMetrics=resultDialog.locator('.result-metrics article');
  for(const [index,label] of ['PERFECT','GOOD','EARLY','LATE','MISSED'].entries())await expect(sessionMetrics.nth(index+4).locator('span')).toHaveText(label);
  await expect(sessionMetrics.nth(0).locator('strong')).toHaveCSS('color','rgb(244, 240, 231)');
  const resultModalLayout=await resultDialog.evaluate(element=>({clientHeight:element.clientHeight,scrollHeight:element.scrollHeight,overflow:getComputedStyle(element).overflow}));
  expect(resultModalLayout.scrollHeight).toBeLessThanOrEqual(resultModalLayout.clientHeight);
  expect(resultModalLayout.overflow).toBe('hidden');
  await expect(resultDialog.getByText('Single Paradiddle at 240 BPM · Demo input')).toBeVisible();
  await expect(page.locator('.workspace')).toBeVisible();
  const overlayBounds=await page.evaluate(()=>{
    const score=document.querySelector('#score-card')!.getBoundingClientRect();
    const overlay=document.querySelector('.result-backdrop')!.getBoundingClientRect();
    const modal=document.querySelector('.results-modal')!.getBoundingClientRect();
    return {score:{x:score.x,y:score.y,width:score.width,height:score.height},overlay:{x:overlay.x,y:overlay.y,width:overlay.width,height:overlay.height},modal:{x:modal.x,y:modal.y,width:modal.width,height:modal.height}};
  });
  expect(Math.abs(overlayBounds.overlay.x-overlayBounds.score.x)).toBeLessThanOrEqual(2);
  expect(Math.abs(overlayBounds.overlay.y-overlayBounds.score.y)).toBeLessThanOrEqual(2);
  expect(Math.abs(overlayBounds.overlay.width-overlayBounds.score.width)).toBeLessThanOrEqual(2);
  expect(Math.abs(overlayBounds.overlay.height-overlayBounds.score.height)).toBeLessThanOrEqual(2);
  expect(overlayBounds.score.width-overlayBounds.modal.width).toBeGreaterThan(100);
  const pageUrl=page.url();
  await page.keyboard.press('j');
  await expect(page.locator('.result-backdrop')).toBeHidden();
  await expect(page.getByRole('button',{name:/Start practice/})).toBeVisible();
  await expect(page.getByRole('button',{name:/Pause/})).toHaveCount(0);
  expect(page.url()).toBe(pageUrl);
  await page.keyboard.press('j');
  await expect(page.getByRole('button',{name:/Pause/})).toBeVisible();
  await expect(page.locator('.workspace')).toBeVisible();
  expect(page.url()).toBe(pageUrl);
  await page.getByRole('button',{name:/Stop/}).click();
  await expect(page.getByText('Stopped — results were not saved')).toBeVisible();
  await page.reload();
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({ path: 'artifacts/completed-result.png', fullPage: true });

  await page.reload();
  await page.getByRole('button',{name:'Stats'}).click();
  const stats=page.getByRole('dialog',{name:'Practice statistics'});
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
  await expect(page.getByText('1 completed session(s) stored only in this browser.')).toBeVisible();
});

test('one drum hit dismisses results and a second hit starts the next run',async({page})=>{
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByRole('button',{name:'Open settings'}).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button',{name:'Close settings'}).first().click();
  await page.getByRole('button',{name:/Start practice/}).click();
  await page.evaluate(async () => {
    await new Promise(resolve => setTimeout(resolve, 165));
    for (let i = 0; i < 32; i++) {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true }));
      await new Promise(resolve => setTimeout(resolve, 125));
    }
  });
  await expect(page.getByRole('dialog',{name:'Good work.'})).toBeVisible();
  await page.keyboard.press('j');
  await expect(page.locator('.result-backdrop')).toBeHidden();
  await expect(page.getByRole('button',{name:/Start practice/})).toBeVisible();
  await expect(page.getByRole('button',{name:/Pause/})).toHaveCount(0);
  await page.keyboard.press('j');
  await expect(page.getByRole('button',{name:/Pause/})).toBeVisible();
  await page.getByRole('button',{name:/Stop/}).click();
});

test('a drum hit starts practice from idle',async({page})=>{
  await page.goto('/');
  await page.getByRole('button',{name:'Open settings'}).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button',{name:'Close settings'}).first().click();
  await page.keyboard.press('j');
  await expect(page.getByRole('button',{name:/Pause/})).toBeVisible();
  await expect(page.locator('.result-backdrop')).toBeHidden();
  await page.getByRole('button',{name:/Stop/}).click();
  await expect(page.getByText('Stopped — results were not saved')).toBeVisible();
});

test('a run with no drum input never shows hit circles',async({page})=>{
  await page.goto('/');
  await page.getByLabel('Tempo in BPM').fill('240');
  await page.getByRole('button',{name:'Open settings'}).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button',{name:'Close settings'}).first().click();
  await page.getByRole('button',{name:/Start practice/}).click();
  await expect(page.getByRole('button',{name:/Pause/})).toBeVisible();
  await expect(page.locator('.judgement-halo')).toHaveCount(0);
  await expect(page.locator('.score-note[data-judgement="missed"]').first()).toBeVisible();
  await expect(page.locator('.missed-note-marker')).toHaveCount(0);
  await expect(page.locator('.judgement-halo')).toHaveCount(0);
  await expect(page.getByRole('dialog',{name:'Good work.'})).toBeVisible({timeout:10000});
  await expect(page.locator('.judgement-halo')).toHaveCount(0);
  await expect(page.locator('.missed-note-marker')).toHaveCount(0);
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
  await page.locator('.midi-library summary').click();
  await expect(page.getByRole('button',{name:'quarter-note-reference.mid',exact:true})).toBeVisible();
  await expect(page.getByRole('button',{name:'single-paradiddle.mid',exact:true})).toBeVisible();
  await page.getByRole('button',{name:'single-paradiddle.mid',exact:true}).click();
  await expect(page.locator('.score-note')).toHaveCount(32);
  const quarterLibrary=page.locator('.midi-library').getByRole('button',{name:'quarter-note-reference.mid',exact:true});
  await quarterLibrary.click();
  await expect(page.locator('.score-note[data-written-duration="q"]')).toHaveCount(8);
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
  }))).toBeGreaterThanOrEqual(2);
  await page.locator('.midi-library summary').click();
  page.once('dialog',dialog=>dialog.accept());
  await page.locator('.midi-library').getByRole('button',{name:'Delete quarter-note-reference.mid'}).click();
  await expect(page.locator('.midi-library summary')).toContainText('1');

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
  await page.evaluate(() => { const prototype=AudioBufferSourceNode.prototype,original=prototype.start;(window as unknown as {__midiVoiceStarts:number}).__midiVoiceStarts=0;prototype.start=function(...args){(window as unknown as {__midiVoiceStarts:number}).__midiVoiceStarts++;return original.apply(this,args as [number]);}; });
  await page.getByRole('button', { name: 'MIDI connection' }).click();
  await expect(page.locator('.status-pill')).toContainText('Acceptance Kit · connected');
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.getByText('MIDI diagnostics').click();
  await page.evaluate(() => {
    const input = (window as unknown as { __rudimentFakeInput: { dispatch(data: number[]): void } }).__rudimentFakeInput;
    input.dispatch([0x99, 38, 1]);
  });
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
  await expect(page.locator('.live-assessment')).toContainText('SCORE —%');
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
  const first=Number(await page.locator('#playhead').getAttribute('data-x'));
  await expect.poll(async()=>Number(await page.locator('#playhead').getAttribute('data-x')),{timeout:1500}).toBeGreaterThan(first);
  expect(await page.locator('#playhead').evaluate(element=>(element as SVGElement).style.transform)).toContain('translate3d');
  expect(await page.locator('#notation').evaluate(element=>getComputedStyle(element).scrollBehavior)).toBe('auto');
  expect(await page.locator('#notation').evaluate(element=>getComputedStyle(element).scrollbarWidth)).toBe('none');
  expect(await page.locator('#notation').evaluate(element=>getComputedStyle(element,'::-webkit-scrollbar').display)).toBe('none');
  await expect(page.locator('.score-note[data-judgement="missed"]').first()).toBeVisible();
  expect(await page.locator('#notation').evaluate(element=>element.scrollLeft)).toBe(0);
  const frameStats=await page.evaluate(()=>new Promise<{median:number,p95:number}>(resolve=>{const deltas:number[]=[];let previous=performance.now();const sample=(now:number)=>{deltas.push(now-previous);previous=now;if(deltas.length<40)requestAnimationFrame(sample);else{const sorted=[...deltas.slice(1)].sort((a,b)=>a-b);resolve({median:sorted[Math.floor(sorted.length*.5)],p95:sorted[Math.floor(sorted.length*.95)]});}};requestAnimationFrame(sample);}));
  expect(frameStats.median).toBeLessThan(25);expect(frameStats.p95).toBeLessThan(55);
  await page.getByRole('button',{name:/Pause/}).click();const paused=await page.locator('#playhead').getAttribute('data-x');await page.waitForTimeout(120);expect(await page.locator('#playhead').getAttribute('data-x')).toBe(paused);
  await page.getByRole('button',{name:/Resume/}).click();
  await expect.poll(()=>page.locator('#playhead').getAttribute('data-x')).not.toBe(paused);
  await expect(page.locator('#playhead')).toHaveClass(/visible/);
});

test('score wraps responsively and follows playback vertically without horizontal scrolling',async({page})=>{
  await page.goto('/');
  await page.evaluate(()=>{
    const hits=Array.from({length:64},(_,index)=>({beat:index*.5,notation_beat:index*.5,notation_duration_beats:.5,hand:index%2===0?'R':'L',accent:false}));
    (window as unknown as {rudiment:{renderScore(exercise:unknown):void}}).rudiment.renderScore({title:'Multi-line Test',meter_numerator:4,meter_denominator:4,hits});
  });
  const score=page.locator('#notation'),svg=score.locator('svg');
  await expect.poll(()=>svg.getAttribute('data-bars-per-line')).toBe('3');
  await expect(svg).toHaveAttribute('data-staff-lines','5');
  const desktopRows=await page.locator('.score-note').evaluateAll(notes=>new Set(notes.map(note=>note.getAttribute('data-system-row'))).size);
  expect(desktopRows).toBeGreaterThan(1);
  const playheadCoversStaves=await page.evaluate(()=>{
    const app=(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment;
    const svg=document.querySelector('svg')!,barsPerLine=Number(svg.dataset.barsPerLine),meter=4;
    const bounds=JSON.parse(svg.dataset.staffBounds!) as Array<{top:number;bottom:number}>;
    return bounds.map((staff,row)=>{
      app.updateScore(row*barsPerLine*meter+.1,[]);
      const line=document.querySelector('#playhead')!;
      return Number(line.getAttribute('y1'))<=staff.top&&Number(line.getAttribute('y2'))>=staff.bottom;
    });
  });
  expect(playheadCoversStaves.length).toBeGreaterThan(1);
  expect(playheadCoversStaves.every(Boolean)).toBe(true);
  const barSpacing=await page.evaluate(()=>{
    const center=(note:Element)=>{const bounds=(note.querySelector('.vf-notehead > text') as SVGGraphicsElement).getBBox();return bounds.x+bounds.width/2;};
    const firstBar=[...document.querySelectorAll('.score-note[data-score-bar="0"]')],secondBar=[...document.querySelectorAll('.score-note[data-score-bar="1"]')];
    const innerGaps=[...firstBar.slice(1).map((note,index)=>center(note)-center(firstBar[index])),...secondBar.slice(1).map((note,index)=>center(note)-center(secondBar[index]))];
    return {boundary:center(secondBar[0])-center(firstBar.at(-1)!),inner:innerGaps.sort((a,b)=>a-b)[Math.floor(innerGaps.length/2)]};
  });
  expect(barSpacing.boundary).toBeGreaterThan(barSpacing.inner*.7);
  expect(barSpacing.boundary).toBeLessThan(barSpacing.inner*1.4);

  const sameLineTransition=await page.evaluate(()=>{
    const app=(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment;
    app.updateScore(3.99,[]);const before=Number(document.querySelector('#playhead')?.getAttribute('data-x'));
    app.updateScore(4,[]);const after=Number(document.querySelector('#playhead')?.getAttribute('data-x'));
    return {before,after,row:document.querySelector('#playhead')?.getAttribute('data-row')};
  });
  expect(sameLineTransition.row).toBe('0');
  expect(sameLineTransition.after-sameLineTransition.before).toBeGreaterThanOrEqual(0);
  expect(sameLineTransition.after-sameLineTransition.before).toBeLessThan(5);

  await page.setViewportSize({width:390,height:844});
  await expect.poll(()=>svg.getAttribute('data-bars-per-line')).toBe('1');
  expect(await score.evaluate(element=>element.scrollWidth<=element.clientWidth&&element.scrollLeft===0)).toBe(true);

  await page.evaluate(()=>{
    const app=(window as unknown as {rudiment:{updateScore(beat:number,items:unknown[]):void}}).rudiment;
    app.updateScore(0,[]);const first=Number(document.querySelector('#playhead')?.getAttribute('data-x'));
    app.updateScore(.5,[]);const next=Number(document.querySelector('#playhead')?.getAttribute('data-x'));
    (window as unknown as {__scoreXProgresses:boolean}).__scoreXProgresses=next>first;
    app.updateScore(3.99,[]);const endOfBar=Number(document.querySelector('#playhead')?.getAttribute('data-x'));
    app.updateScore(4,[]);const nextBar=Number(document.querySelector('#playhead')?.getAttribute('data-x'));
    (window as unknown as {__scoreBarWraps:boolean}).__scoreBarWraps=nextBar<endOfBar;
    for(let frame=0;frame<30;frame++)app.updateScore(4,[]);
  });
  expect(await page.evaluate(()=>(window as unknown as {__scoreXProgresses:boolean}).__scoreXProgresses)).toBe(true);
  expect(await page.evaluate(()=>(window as unknown as {__scoreBarWraps:boolean}).__scoreBarWraps)).toBe(true);
  await expect(page.locator('#playhead')).toHaveAttribute('data-row','1');
  expect(await score.evaluate(element=>element.scrollTop)).toBeGreaterThan(0);
  expect(await score.evaluate(element=>element.scrollLeft)).toBe(0);
  expect(await score.evaluate(element=>{
    const viewport=element.getBoundingClientRect();
    return [1,2].every(bar=>{
      const note=element.querySelector(`.score-note[data-score-bar="${bar}"]`);
      if(!note)return false;
      const bounds=note.getBoundingClientRect();
      return bounds.top<viewport.bottom&&bounds.bottom>viewport.top;
    });
  })).toBe(true);

  await page.setViewportSize({width:1440,height:900});
  await expect.poll(()=>svg.getAttribute('data-bars-per-line')).toBe('3');
});

test('single-line snare notation is available only when explicitly selected',async({page})=>{
  await page.goto('/');
  await page.evaluate(()=>{
    const hits=Array.from({length:8},(_,index)=>({beat:index*.5,notation_beat:index*.5,notation_duration_beats:.5,instrument:'snare',note:38,hand:index%2?'L':'R'}));
    (window as unknown as {rudiment:{renderScore(exercise:unknown):void}}).rudiment.renderScore({title:'Snare Rudiment',meter_numerator:4,meter_denominator:4,hits,staff_mode:'single_line'});
  });
  await expect(page.locator('#notation svg')).toHaveAttribute('data-staff-lines','1');
});

test('kit MIDI maps to drum staff positions with mixed rhythms and compact final system',async({page})=>{
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
  expect(layout.counts.at(-1)).toBeLessThan(layout.barsPerLine);
  expect(layout.widths.at(-1)).toBeLessThan(layout.widths[0]);
  await expect(page.locator('.score-measure-number')).toHaveCount(5);
  await expect(page.locator('.score-barline')).toHaveCount(3);
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
    const head=element.parentElement!.querySelector(':scope > text')!,bounds=head.getBBox();
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
  expect(await hit.locator(':scope > .vf-notehead > text').first().evaluate(element=>getComputedStyle(element).fill)).toBe('rgb(194, 62, 53)');
  expect(await hit.evaluate(element=>getComputedStyle(element).opacity)).toBe(originalOpacity);
  await expect(marker).toHaveCount(0);
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
  await expect(hit).not.toHaveAttribute('data-judgement','good');
  await expect(hit.locator('.judgement-halo')).toHaveCount(0);
  expect(await hit.locator(':scope > .vf-notehead > text').first().evaluate(element=>getComputedStyle(element).fill)).toBe(originalFill);
});

test('all five timing judgements colour the notehead and expose text alternatives',async({page})=>{
  await page.goto('/');
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
    {beat:'2',judgement:'missed',fill:'rgb(194, 62, 53)',label:'missed'}
  ];
  for(const item of expected){
    const note=page.locator(`.score-note[data-beat="${item.beat}"]`).first(),head=note.locator(':scope > .vf-notehead').first();
    await expect(note).toHaveAttribute('data-judgement',item.judgement);
    await expect(head).toHaveAttribute('aria-label',item.label);
    expect(await head.locator(':scope > text').first().evaluate(element=>getComputedStyle(element).fill)).toBe(item.fill);
  }
});

test('rapid repeated monitored strikes allocate an audio voice for every hit', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(() => { const prototype=AudioBufferSourceNode.prototype,original=prototype.start;(window as unknown as {__voiceStarts:number}).__voiceStarts=0;prototype.start=function(...args){(window as unknown as {__voiceStarts:number}).__voiceStarts++;return original.apply(this,args as [number]);}; });
  await page.evaluate(()=>(window as unknown as {rudiment:{ensureAudio():Promise<void>}}).rudiment.ensureAudio());
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

  await page.setViewportSize({width:998,height:448});
  await expect.poll(()=>page.locator('#notation svg').getAttribute('data-bars-per-line')).toBe('4');
  const layout=await page.evaluate(()=>({
    pageFits:document.documentElement.scrollHeight<=innerHeight&&document.body.scrollHeight<=innerHeight,
    controls:[...document.querySelectorAll('.topbar,.welcome,.score-card,.feedback-strip,.transport')].every(element=>{const box=element.getBoundingClientRect();return box.top>=0&&box.bottom<=innerHeight;}),
    horizontalFits:document.documentElement.scrollWidth<=innerWidth&&document.body.scrollWidth<=innerWidth,
  }));
  expect(layout).toEqual({pageFits:true,controls:true,horizontalFits:true});
  await expect(midi).toBeVisible();
  await expect(page.getByRole('button',{name:/Start practice/})).toBeVisible();
});
