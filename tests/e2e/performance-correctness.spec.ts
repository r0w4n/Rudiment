import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';

const manifest=JSON.parse(readFileSync('web/public/exercises/performance/manifest.json','utf8')) as {fixtures:Record<string,{name:string,bars:number,subdivision:number,noteCount:number}>};

test('deterministic performance fixtures preserve every onset and simultaneous drum',async({page})=>{
  await page.goto('/');
  const medium=manifest.fixtures.medium,mediumOnsets=medium.bars*4*medium.subdivision;
  await page.evaluate(({name})=>(window as any).rudiment.loadBundledFromLibrary('public/exercises/performance/medium.mid',name),medium);
  await expect(page.locator('.score-note')).toHaveCount(mediumOnsets);
  await expect(page.locator('#notation svg')).toHaveAttribute('data-system-count',String(medium.bars));
  await expect(page.locator('.score-measure-number')).toHaveCount(medium.bars);

  const dense=manifest.fixtures.dense,denseOnsets=dense.bars*4*dense.subdivision;
  await page.evaluate(({name})=>(window as any).rudiment.loadBundledFromLibrary('public/exercises/performance/dense.mid',name),dense);
  await expect(page.locator('.score-note')).toHaveCount(denseOnsets);
  const simultaneous=page.locator('.score-note[data-midi-notes*="38,36"]');
  await expect(simultaneous).toHaveCount(denseOnsets/4);
  expect(await page.locator('.score-note').evaluateAll(notes=>notes.reduce((sum,note)=>sum+(note.getAttribute('data-midi-notes')?.split(',').length??0),0))).toBe(dense.noteCount);
});

async function installMidiKit(page: Page) {
  await page.addInitScript(()=>{
    const input={id:'perf-kit',name:'Performance Kit',manufacturer:'Rudiment',type:'input',state:'connected',connection:'open',onmidimessage:null as null|((event:unknown)=>void),onstatechange:null,open:async()=>input,close:async()=>input,addEventListener:()=>{},removeEventListener:()=>{},dispatch:(data:number[],timestamp=performance.now())=>input.onmidimessage?.({data:new Uint8Array(data),timeStamp:timestamp})};
    const access={inputs:new Map([[input.id,input]]),outputs:new Map(),sysexEnabled:false,onstatechange:null,addEventListener:()=>{},removeEventListener:()=>{}};
    Object.defineProperty(navigator,'requestMIDIAccess',{configurable:true,value:async()=>access});
    Object.defineProperty(navigator,'permissions',{configurable:true,value:{query:async()=>({state:'granted',onchange:null})}});
    Object.assign(window,{__performanceInput:input});
  });
}

test('buffered output and calibrated MIDI agree with the audible beat',async({page})=>{
  await installMidiKit(page);
  await page.addInitScript(()=>{
    // Model the hardware output independently of Rudiment's clock conversion.
    Object.defineProperty(AudioContext.prototype,'outputLatency',{get:()=>.24});
    AudioContext.prototype.getOutputTimestamp=function(){
      return this.currentTime<.24?{contextTime:0,performanceTime:0}
        :{contextTime:this.currentTime-.24,performanceTime:performance.now()};
    };
    let clicks=0;
    const start=AudioBufferSourceNode.prototype.start;
    AudioBufferSourceNode.prototype.start=function(...args){
      const app=(window as any).rudiment;
      if(app?.identifyMetronomeBuffer(this.buffer)&&++clicks===2){
        const audible=performance.now()+((args[0]??0)-this.context.currentTime+.24)*1000;
        setTimeout(()=>{(window as any).__beatError=app.currentAudioBeat()-1-(performance.now()-audible)/1000;},audible-performance.now());
        const strike=audible+150;
        setTimeout(()=>(window as any).__performanceInput.dispatch([0x99,38,100],strike),strike-performance.now());
      }
      return start.apply(this,args);
    };
  });
  await page.goto('/');
  await page.getByRole('button',{name:'Open settings'}).click();
  const settings=page.locator('#settings-dialog');
  await settings.getByLabel('Count-in').selectOption('0');
  await settings.getByLabel('Manual offset').fill('150');
  await page.getByRole('button',{name:'Close settings'}).first().click();
  await page.getByRole('button',{name:/Start practice/}).click();
  await expect.poll(()=>page.evaluate(()=>Math.abs((window as any).__beatError))).toBeLessThan(.035);
  await expect(page.locator('.score-note[data-beat="1"]')).toHaveAttribute('data-judgement','perfect');
});

test('high-rate MIDI feedback paints by the next frame and Restart clears it',async({page})=>{
  await installMidiKit(page);
  await page.goto('/');
  await page.getByRole('button',{name:'Open settings'}).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByRole('button',{name:'Close settings'}).first().click();
  await page.getByRole('button',{name:/Start practice/}).click();
  await expect(page.getByRole('button',{name:/Pause/})).toBeVisible();
  await expect.poll(()=>page.evaluate(()=>(window as any).rudiment.audioDiagnostics().activeRun)).toBe(true);
  await expect.poll(()=>page.evaluate(()=>(window as any).rudiment.currentAudioBeat())).toBeGreaterThan(0.05);
  await page.evaluate(()=>{const input=(window as any).__performanceInput;for(let index=0;index<64;index++)input.dispatch([0x99,index%8===0?36:38,40+index%88],performance.now()+index*.1);});
  await expect(page.locator('.score-note[data-judgement],.unmatched-hit-marker').first()).toBeVisible();
  await page.getByRole('button',{name:/Restart/}).click();
  await expect(page.locator('.score-note[data-judgement]')).toHaveCount(0);
  await expect(page.locator('.unmatched-hit-marker')).toHaveCount(0);
});

test('live hit feedback never rewinds the playhead within a bar',async({page})=>{
  await page.goto('/');
  await page.locator('#visual-metronome').click();
  await page.locator('#metronome-popover').getByLabel('Tempo in BPM').fill('120');
  await page.getByLabel('Close metronome settings').click();
  await page.getByRole('button',{name:'Open settings'}).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByRole('button',{name:'Close settings'}).first().click();
  await page.getByRole('button',{name:/Start practice/}).click();
  await expect(page.locator('#playhead')).toHaveClass(/visible/);
  await expect.poll(()=>page.evaluate(()=>(window as any).rudiment.currentAudioBeat())).toBeGreaterThan(0.05);
  const movement=await page.evaluate(()=>new Promise<{deltas:number[],feedbackCount:number}>(resolve=>{
    const playhead=document.querySelector('#playhead')!;
    const deltas:number[]=[];
    const observer=new MutationObserver(records=>{
      const current=Number(playhead.getAttribute('x1'));
      for(const record of records)if(record.oldValue!==null)deltas.push(current-Number(record.oldValue));
    });
    observer.observe(playhead,{attributes:true,attributeFilter:['x1'],attributeOldValue:true});
    const hits=setInterval(()=>window.dispatchEvent(new KeyboardEvent('keydown',{key:'j',code:'KeyJ',bubbles:true})),50);
    setTimeout(()=>{
      clearInterval(hits);observer.disconnect();
      resolve({deltas,feedbackCount:document.querySelectorAll('.score-note[data-judgement],.unmatched-hit-marker').length});
    },650);
  }));
  expect(movement.deltas.length).toBeGreaterThan(10);
  expect(movement.feedbackCount).toBeGreaterThan(0);
  expect(Math.min(...movement.deltas)).toBeGreaterThanOrEqual(-0.01);
});

test('dense MIDI traffic and a delayed frame recover both views to the audio timeline',async({page})=>{
  test.setTimeout(60_000);
  await page.addInitScript(() => { (window as any).__RUDIMENT_PERF__=true; });
  await page.goto('/');
  await page.evaluate(() => (window as any).rudiment.loadBundledFromLibrary('public/exercises/performance/dense.mid','Performance Dense'));
  await expect(page.locator('.score-note')).toHaveCount(manifest.fixtures.dense.bars*4*manifest.fixtures.dense.subdivision);
  await page.locator('#visual-metronome').click();
  await page.locator('#metronome-popover').getByLabel('Tempo in BPM').fill('240');
  await page.getByLabel('Close metronome settings').click();
  await page.getByRole('button',{name:'Open settings'}).click();
  await page.locator('#settings-dialog').getByLabel('Count-in').selectOption('0');
  await page.getByRole('button',{name:'Close settings'}).first().click();
  await page.getByRole('button',{name:/Start practice/}).click();
  await expect.poll(()=>page.evaluate(()=>(window as any).rudiment.currentAudioBeat())).toBeGreaterThan(0.05);
  for(const view of ['notation','highway'] as const){
    if(view==='highway')await page.getByRole('button',{name:'Highway',exact:true}).click();
    const result=await page.evaluate(async currentView=>{
      const app=(window as any).rudiment;
      const samples:{error:number;interval:number}[]=[];
      let previous=performance.now();
      const read=()=>{
        if(currentView==='notation')return Number(document.querySelector('#playhead')?.getAttribute('data-beat'));
        const svg=document.querySelector<SVGSVGElement>('.highway-svg')!;
        const translate=svg.querySelector<SVGGElement>('.highway-moving')!.transform.baseVal.consolidate()!.matrix.e;
        return (svg.viewBox.baseVal.width/2-translate)/Math.max(80,svg.viewBox.baseVal.width/10);
      };
      for(let index=0;index<32;index++){
        if(index%3===0){
          window.dispatchEvent(new KeyboardEvent('keydown',{key:'j',bubbles:true}));
          window.dispatchEvent(new KeyboardEvent('keydown',{key:'k',bubbles:true}));
        }
        await new Promise<void>(resolve=>requestAnimationFrame(()=>resolve()));
        const now=performance.now();
        samples.push({error:Math.abs(read()-app.currentAudioBeat()),interval:now-previous});
        previous=now;
      }
      const before=read(),start=performance.now();
      while(performance.now()-start<90){/* Simulate a blocked main thread. */}
      await new Promise<void>(resolve=>requestAnimationFrame(()=>resolve()));
      const after=read(),timeline=app.currentAudioBeat();
      return {before,after,timeline,samples};
    },view);
    expect(result.samples.filter(sample=>sample.error<0.12).length).toBeGreaterThan(24);
    expect(result.after-result.before).toBeGreaterThan(0.15);
    expect(Math.abs(result.after-result.timeline)).toBeLessThan(0.12);
  }
});
