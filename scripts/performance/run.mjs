import { chromium } from 'playwright';
import { execFileSync, spawn } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

const argument=(name,fallback)=>{const index=process.argv.indexOf(name);return index>=0?process.argv[index+1]:fallback;};
const flag=name=>process.argv.includes(name);
const tier=argument('--tier','fast');
const output=path.resolve(argument('--output',`performance-results/${tier}.json`));
const baseURL=argument('--base-url','http://127.0.0.1:4183');
const durationMs=Number(argument('--duration-ms',tier==='endurance'?'900000':'30000'));
const repeats=Number(argument('--repeats',tier==='fast'?'3':'1'));
const trace=flag('--trace');
const fixtureManifest=JSON.parse(await readFile('web/public/exercises/performance/manifest.json','utf8'));
const tierFixtures={fast:['small','medium'],full:['small','medium','large','extreme','dense'],endurance:['medium']};
if(!tierFixtures[tier])throw new Error(`Unknown tier: ${tier}`);

const percentile=(values,p)=>{
  if(!values.length)return null;
  const sorted=[...values].sort((a,b)=>a-b),index=(sorted.length-1)*p,low=Math.floor(index),high=Math.ceil(index);
  return sorted[low]+(sorted[high]-sorted[low])*(index-low);
};
const summary=values=>({samples:values.length,p50:percentile(values,.5),p95:percentile(values,.95),p99:percentile(values,.99),max:values.length?Math.max(...values):null});
const sleep=milliseconds=>new Promise(resolve=>setTimeout(resolve,milliseconds));
const git=(...args)=>{try{return execFileSync('git',args,{encoding:'utf8'}).trim();}catch{return 'unknown';}};

async function waitForServer(url,timeout=15_000){
  const deadline=Date.now()+timeout;
  while(Date.now()<deadline){try{const response=await fetch(url);if(response.ok)return;}catch{}await sleep(150);}
  throw new Error(`Timed out waiting for ${url}`);
}

const server=spawn('python3',['-m','http.server','4183','--bind','127.0.0.1','--directory','dist'],{stdio:'ignore'});
try{await waitForServer(baseURL);}catch(error){server.kill('SIGTERM');throw error;}

const browser=await chromium.launch({channel:'chrome',headless:true,args:['--enable-precise-memory-info']});
const browserVersion=browser.version();
const results=[];
const startedAt=new Date().toISOString();

async function createContext(label){
  const context=await browser.newContext({viewport:{width:1440,height:900}});
  await context.addInitScript(() => {
    window.__RUDIMENT_PERF__=true;
    const input={
      id:'perf-kit',name:'Performance Kit',manufacturer:'Rudiment',type:'input',state:'connected',connection:'open',onmidimessage:null,onstatechange:null,
      open:async()=>input,close:async()=>input,addEventListener:()=>{},removeEventListener:()=>{},
      dispatch:(data,timestamp=performance.now())=>input.onmidimessage?.({data:new Uint8Array(data),timeStamp:timestamp}),
    };
    const access={inputs:new Map([[input.id,input]]),outputs:new Map(),sysexEnabled:false,onstatechange:null,addEventListener:()=>{},removeEventListener:()=>{}};
    Object.defineProperty(navigator,'requestMIDIAccess',{configurable:true,value:async()=>access});
    Object.defineProperty(navigator,'permissions',{configurable:true,value:{query:async()=>({state:'granted',onchange:null})}});
    Object.assign(window,{__rudimentPerfInput:input,__rudimentObserved:{longTasks:[],frames:[],eventToPaint:[],keyboardDispatch:[]}});
    new PerformanceObserver(list=>{for(const entry of list.getEntries())window.__rudimentObserved.longTasks.push(entry.duration);}).observe({type:'longtask',buffered:true});
    let previous=performance.now();
    const frame=now=>{window.__rudimentObserved.frames.push(now-previous);previous=now;requestAnimationFrame(frame);};
    requestAnimationFrame(frame);
  });
  if(trace)await context.tracing.start({screenshots:true,snapshots:true,sources:true,title:label});
  return context;
}

async function loadFixture(page,id){
  const fixture=fixtureManifest.fixtures[id],expectedOnsets=fixture.bars*4*fixture.subdivision;
  await page.evaluate(()=>{window.rudiment.resetPerformanceDiagnostics();window.__rudimentObserved.longTasks.length=0;window.__rudimentObserved.frames.length=0;});
  const heapBefore=await page.evaluate(()=>performance.memory?.usedJSHeapSize??null);
  const started=await page.evaluate(()=>performance.now());
  await page.evaluate(({id,name})=>window.rudiment.loadBundledFromLibrary(`public/exercises/performance/${id}.mid`,name),{id,name:fixture.name});
  await page.waitForFunction(expected=>document.querySelectorAll('.score-note').length===expected,expectedOnsets,{timeout:id==='extreme'?180_000:90_000});
  await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
  const finished=await page.evaluate(()=>performance.now());
  const heapAfter=await page.evaluate(()=>performance.memory?.usedJSHeapSize??null);
  const observed=await page.evaluate(()=>window.__rudimentObserved);
  const diagnostics=await page.evaluate(()=>window.rudiment.performanceDiagnostics());
  return {
    fixture:{...fixture,id,expectedOnsets},
    measurements:{
      import_to_render_ms:[finished-started],
      notation_render_ms:diagnostics.notation_render_ms??[],
      long_task_ms:observed.longTasks,
      frame_interval_ms:observed.frames.filter(value=>value<1000),
      heap_growth_bytes:heapBefore===null||heapAfter===null?[]:[heapAfter-heapBefore],
    },
  };
}

async function configurePractice(page){
  await page.getByRole('button',{name:'Open settings'}).click();
  await page.getByLabel('Count-in').selectOption('0');
  await page.getByRole('button',{name:'Close settings'}).first().click();
  await page.getByRole('button',{name:/Start practice/}).click();
  await page.getByRole('button',{name:/Pause/}).waitFor({timeout:15_000});
}

async function measurePaintLatency(page,dispatch){
  return page.evaluate(async ({dispatch})=>{
    const root=document.querySelector('#notation'),input=window.__rudimentPerfInput;
    return new Promise(resolve=>{
      const start=performance.now();let settled=false;
      const finish=()=>{if(settled)return;settled=true;observer.disconnect();resolve(performance.now()-start);};
      const observer=new MutationObserver(finish);observer.observe(root,{subtree:true,childList:true,attributes:true,attributeFilter:['data-judgement']});
      if(dispatch==='midi')input.dispatch([0x99,38,96]);
      else window.dispatchEvent(new KeyboardEvent('keydown',{key:'j',code:'KeyJ',bubbles:true}));
      setTimeout(finish,250);
    });
  },{dispatch});
}

async function runInteractions(page,fixtureId){
  await configurePractice(page);
  await page.evaluate(()=>window.rudiment.resetPerformanceDiagnostics());
  const midiPaint=[];
  for(let index=0;index<12;index++)midiPaint.push(await measurePaintLatency(page,'midi'));
  const keyboardPaint=[];
  for(let index=0;index<12;index++)keyboardPaint.push(await measurePaintLatency(page,'keyboard'));
  await page.evaluate(()=>{for(let index=0;index<250;index++)window.__rudimentPerfInput.dispatch([0x99,index%9===0?36:38,40+index%88]);});
  await page.waitForTimeout(150);
  const scroll=await page.evaluate(()=>{
    const values=[];for(let index=0;index<120;index++){const start=performance.now();window.rudiment.scrollScoreToBeat((index*17)%480);values.push(performance.now()-start);}return values;
  });
  const transition=[];
  for(const name of [/Pause/,/Resume/,/Pause/,/Restart/]){
    const start=Date.now();await page.getByRole('button',{name}).click();transition.push(Date.now()-start);
  }
  const modal=[];
  for(let index=0;index<4;index++){
    let start=Date.now();await page.getByRole('button',{name:'Stats'}).click();await page.getByRole('dialog',{name:'Performance history'}).waitFor();modal.push(Date.now()-start);
    start=Date.now();await page.keyboard.press('Escape');await page.getByRole('dialog',{name:'Performance history'}).waitFor({state:'hidden'});modal.push(Date.now()-start);
  }
  const diagnostics=await page.evaluate(()=>window.rudiment.performanceDiagnostics());
  const observed=await page.evaluate(()=>window.__rudimentObserved);
  return {fixtureId,measurements:{
    midi_event_to_paint_ms:midiPaint,keyboard_event_to_paint_ms:keyboardPaint,scroll_call_ms:scroll,
    transition_response_ms:transition,modal_response_ms:modal,midi_callback_ms:diagnostics.midi_callback_ms??[],
    score_update_ms:diagnostics.score_update_ms??[],feedback_paint_work_ms:diagnostics.feedback_paint_work_ms??[],
    audio_scheduling_ms:diagnostics.audio_scheduling_ms??[],animation_frame_interval_ms:diagnostics.animation_frame_interval_ms??[],
    long_task_ms:observed.longTasks,
  }};
}

async function runEndurance(page){
  await page.getByLabel('Loops').selectOption('0');
  await configurePractice(page);
  const samples=[];const deadline=Date.now()+durationMs;let cycle=0;
  while(Date.now()<deadline){
    await page.evaluate(()=>{for(let index=0;index<64;index++)window.__rudimentPerfInput.dispatch([0x99,38,96]);});
    if(cycle%5===0){await page.getByRole('button',{name:/Pause/}).click();await page.getByRole('button',{name:/Resume/}).click();}
    samples.push(await page.evaluate(()=>({time:performance.now(),heap:performance.memory?.usedJSHeapSize??null,notes:document.querySelectorAll('.score-note').length,markers:document.querySelectorAll('.unmatched-hit-marker').length,audio:window.rudiment.audioDiagnostics()})));
    cycle++;await page.waitForTimeout(1000);
  }
  await page.getByRole('button',{name:/Restart/}).click();
  await page.waitForTimeout(3000);
  const afterStop=await page.evaluate(()=>({time:performance.now(),heap:performance.memory?.usedJSHeapSize??null,notes:document.querySelectorAll('.score-note').length,markers:document.querySelectorAll('.unmatched-hit-marker').length,audio:window.rudiment.audioDiagnostics()}));
  return {durationMs,samples,afterStop};
}

try{
  for(const fixtureId of tierFixtures[tier]){
    for(let repeat=0;repeat<repeats;repeat++){
      const context=await createContext(`${tier}-${fixtureId}-${repeat+1}`),page=await context.newPage();
      await page.goto(baseURL,{waitUntil:'networkidle'});await page.locator('.score-note').first().waitFor();
      const loaded=await loadFixture(page,fixtureId);loaded.repeat=repeat+1;results.push(loaded);
      if((tier==='fast'&&fixtureId==='medium'&&repeat===0)||(tier==='full'&&['large','extreme','dense'].includes(fixtureId)))results.push(await runInteractions(page,fixtureId));
      if(tier==='endurance'){results.push({fixtureId,endurance:await runEndurance(page)});}
      if(trace){await mkdir(path.dirname(output),{recursive:true});await context.tracing.stop({path:path.join(path.dirname(output),`${tier}-${fixtureId}-${repeat+1}.zip`)});}
      await context.close();
    }
  }
}finally{await browser.close();server.kill('SIGTERM');}

const metrics={};
for(const result of results)for(const [name,values] of Object.entries(result.measurements??{})){metrics[name]??=[];metrics[name].push(...values);}
const report={
  schemaVersion:1,label:argument('--label',tier),tier,startedAt,finishedAt:new Date().toISOString(),
  methodology:{observed:'Browser Performance API, Long Tasks API, requestAnimationFrame, synchronous callback timings, DOM mutation latency and Chromium precise heap size.',estimated:'Dropped-frame counts are estimated from frame intervals above 16.7 ms. Audio output and USB/driver latency are not measured.',repeats,durationMs:tier==='endurance'?durationMs:null},
  environment:{platform:process.platform,release:os.release(),arch:os.arch(),cpus:os.cpus().length,node:process.version,browser:{channel:'chrome',version:browserVersion},commit:git('rev-parse','HEAD'),dirty:Boolean(git('status','--porcelain'))},
  scenarios:results,
  metrics:Object.fromEntries(Object.entries(metrics).map(([name,values])=>[name,{...summary(values),values}])),
};
report.metrics.estimated_dropped_frames={samples:metrics.frame_interval_ms?.length??0,count:(metrics.frame_interval_ms??[]).filter(value=>value>16.7).length,method:'Frame intervals above 16.7 ms; this is an estimate, not compositor telemetry.'};
await mkdir(path.dirname(output),{recursive:true});await writeFile(output,`${JSON.stringify(report,null,2)}\n`);
console.log(`Performance report: ${output}`);
for(const [name,metric] of Object.entries(report.metrics))if('p95'in metric)console.log(`${name}: p95=${metric.p95?.toFixed?.(2)??'n/a'}${name.endsWith('_bytes')?' bytes':' ms'}, n=${metric.samples}`);
