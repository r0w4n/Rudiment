import { expect, test } from '@playwright/test';
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

test('high-rate MIDI feedback paints by the next frame and Restart clears it',async({page})=>{
  await page.addInitScript(()=>{
    const input={id:'perf-kit',name:'Performance Kit',manufacturer:'Rudiment',type:'input',state:'connected',connection:'open',onmidimessage:null as null|((event:unknown)=>void),onstatechange:null,open:async()=>input,close:async()=>input,addEventListener:()=>{},removeEventListener:()=>{},dispatch:(data:number[],timestamp=performance.now())=>input.onmidimessage?.({data:new Uint8Array(data),timeStamp:timestamp})};
    const access={inputs:new Map([[input.id,input]]),outputs:new Map(),sysexEnabled:false,onstatechange:null,addEventListener:()=>{},removeEventListener:()=>{}};
    Object.defineProperty(navigator,'requestMIDIAccess',{configurable:true,value:async()=>access});
    Object.defineProperty(navigator,'permissions',{configurable:true,value:{query:async()=>({state:'granted',onchange:null})}});
    Object.assign(window,{__performanceInput:input});
  });
  await page.goto('/');
  await page.getByRole('button',{name:'Open settings'}).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button',{name:'Close settings'}).first().click();
  await page.getByRole('button',{name:/Start practice/}).click();
  await expect(page.getByRole('button',{name:/Pause/})).toBeVisible();
  await page.evaluate(()=>{const input=(window as any).__performanceInput;for(let index=0;index<64;index++)input.dispatch([0x99,index%8===0?36:38,40+index%88],performance.now()+index*.1);});
  await expect(page.locator('.score-note[data-judgement],.unmatched-hit-marker').first()).toBeVisible();
  await page.getByRole('button',{name:/Restart/}).click();
  await expect(page.locator('.score-note[data-judgement]')).toHaveCount(0);
  await expect(page.locator('.unmatched-hit-marker')).toHaveCount(0);
});
