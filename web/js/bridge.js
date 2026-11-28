import * as Tone from 'tone';
import { WebMidi } from 'webmidi';
import { openDB } from 'idb';
import { Renderer, Stave, StaveNote, GraceNote, GraceNoteGroup, Voice, Formatter, Beam, Annotation, AnnotationVerticalJustify, Articulation, BarlineType, Dot, Fraction, Tuplet, Metrics } from 'vexflow';
import { createPresentationHost } from './presentation-host.mjs';
import { createHighwaySvg } from './highway-svg.mjs';

const DB_NAME = 'rudiment';
const DB_VERSION = 2;
const STICKING_PLACEMENT = AnnotationVerticalJustify.BOTTOM;
const SYSTEM_MARK_BASELINE_CLEARANCE = 24;
const bridgeUrl = document.currentScript?.src || document.querySelector('script[src*="bridge.js"]')?.src || document.baseURI;
const publicAsset = path => new URL(path, bridgeUrl).toString();
// Build identity for the diagnostic drawer and HUD. Bump together with the
// bridge cache-busting query in web/index.html.
const BRIDGE_BUILD='20261128-1';
// Session-only diagnostic switches for the playhead-smoothness investigation.
// URL parameters seed them so existing links and browser tests keep working;
// the Settings drawer toggles them at runtime so iPad experiments do not
// require URL editing. Nothing here persists: a reload always restores normal
// operation, and diagnostic mode never alters scoring, saving or playback
// unless the switch itself says so (the scoring/UI tick, which is also the
// only switch that suppresses result saving).
const diagnosticsParams = new URLSearchParams(location.search);
const diagnosticsDefault = {
  referenceOverlay: diagnosticsParams.get('playhead-debug') === '1',
  visualFeedback: !['0','off','false'].includes((diagnosticsParams.get('visual-hits')||(diagnosticsParams.get('feedback')||'')).toLowerCase()),
  metronomeVisual: !['0','off','false'].includes((diagnosticsParams.get('metronome-animation')||'').toLowerCase()),
  metronomeNeedle: true,
  metronomePulses: true,
  stateTick: true,
  autoScroll: true,
  perfCounters: globalThis.__RUDIMENT_PERF__===true,
};
const diagnostics = { ...diagnosticsDefault };
if(diagnostics.referenceOverlay)document.documentElement.dataset.playheadDebug='1';
const dbPromise = openDB(DB_NAME, DB_VERSION, {
  upgrade(db) {
    if (!db.objectStoreNames.contains('sessions')) db.createObjectStore('sessions', { keyPath: 'id' });
    if (!db.objectStoreNames.contains('settings')) db.createObjectStore('settings', { keyPath: 'key' });
    if (!db.objectStoreNames.contains('imports')) db.createObjectStore('imports', { keyPath: 'hash' });
    if (!db.objectStoreNames.contains('profiles')) db.createObjectStore('profiles', { keyPath: 'id' });
  }
});
let midiLoadGeneration=0;
async function loadSavedImport(hash){const generation=++midiLoadGeneration;try{const item=await (await dbPromise).get('imports',hash);if(generation!==midiLoadGeneration)return;if(!item)throw new Error('This MIDI file is no longer in the local library.');if(!midiFileCallback)throw new Error('MIDI import is not ready yet.');midiFileCallback(new Uint8Array(item.bytes),item.name,'');}catch(error){if(generation===midiLoadGeneration)throw error;}}
async function saveLastImport(hash){const db=await dbPromise,current=await db.get('settings','app');await db.put('settings',{key:'app',value:{...(current?.value||{}),lastImportHash:hash||null}});}
const put = async (store, value) => (await dbPromise).put(store, value);
const all = async store => (await dbPromise).getAll(store);
const clear = async store => (await dbPromise).clear(store);
// Practice duration is wall-clock time only while the musical transport is
// playing. Checkpoints are persisted independently of completed performances,
// so an interrupted run still contributes without re-counting on resume.
const practiceTime={
  since:null,timer:null,pending:0,error:null,writes:Promise.resolve(),
  checkpoint(){
    if(this.since===null)return;
    const now=performance.now();this.pending+=Math.max(0,now-this.since);this.since=now;
    this.writes=this.writes.catch(()=>{}).then(async()=>{
      const increment=this.pending;if(!increment)return;
      const db=await dbPromise,tx=db.transaction('settings','readwrite');
      const saved=await tx.store.get('practiceTime');
      const total=Number(saved?.value)||0;
      await tx.store.put({key:'practiceTime',value:total+increment});await tx.done;
      this.pending-=increment;this.error=null;
    }).catch(error=>{this.error=error;});
  },
  // iPadOS Safari commits IndexedDB readwrite transactions on the main
  // thread's schedule, so a 5 s cadence put one on that schedule during
  // otherwise idle playback; an iPad field report of lock-step playhead
  // hitches matched that periodic signature. stop() (pause, navigation,
  // visibility change, pagehide) still persists immediately, so only a hard
  // crash can lose up to 30 s of wall-clock duration — a counter, not scored
  // history. Desktop WebKit pacing measurements do not reproduce the hitch,
  // so on-device confirmation is still required.
  start(){if(this.since!==null)return;this.since=performance.now();this.timer=setInterval(()=>this.checkpoint(),30000);},
  stop(){if(this.since===null)return;this.checkpoint();this.since=null;clearInterval(this.timer);this.timer=null;},
  async read(){if(this.since!==null)this.checkpoint();await this.writes;if(this.error)throw this.error;return Number((await (await dbPromise).get('settings','practiceTime'))?.value)||0;},
  async clearHistory(){this.stop();await this.writes;if(this.error)throw this.error;const db=await dbPromise,tx=db.transaction(['sessions','settings'],'readwrite');await tx.objectStore('sessions').clear();await tx.objectStore('settings').delete('practiceTime');await tx.done;}
};
document.addEventListener('visibilitychange',()=>{if(document.visibilityState==='hidden')practiceTime.stop();});
window.addEventListener('pagehide',()=>practiceTime.stop());
const clampVelocity = velocity => Math.max(1,Math.min(127,Number(velocity)||1));
const velocityGain = velocity => {
  const normalised=(clampVelocity(velocity)-1)/126;
  return .025+.975*Math.pow(normalised,1.7);
};
const velocityLayers = velocity => {
  const position=(clampVelocity(velocity)-1)/42;
  const lower=Math.min(3,Math.floor(position)),upper=Math.min(3,Math.ceil(position)),mix=position-lower;
  if(lower===upper)return [{index:lower,weight:1}];
  return [{index:lower,weight:Math.cos(mix*Math.PI/2)},{index:upper,weight:Math.sin(mix*Math.PI/2)}];
};
const audioErrorMessage = error => error instanceof Error ? error.message : String(error ?? 'Unknown audio error');
const metronomeSampleFiles = {
  woodblock: { accent:'audio/woodblock-accent.wav', regular:'audio/woodblock-regular.wav' },
  crisp: { accent:'audio/click-accent.wav', regular:'audio/click-regular.wav' },
  cowbell: { accent:'audio/cowbell-accent.wav', regular:'audio/cowbell-regular.wav' }
};
const drumSampleFiles = {
  kick:'audio/kick.wav',
  closed_hat:'audio/closed-hat.wav',
  open_hat:'audio/open-hat.wav',
};
const metronomeSampleName = path => path.slice('audio/'.length,-'.wav'.length);
const PERFORMANCE_SAMPLE_CAP=4096;
const performanceProfile={
  enabled:globalThis.__RUDIMENT_PERF__===true,
  samples:new Map(),
  // Bounded history: endurance runs must not accumulate unbounded arrays whose
  // GC pauses skew the very timings being measured. Dropping the oldest
  // quarter once the cap is reached keeps recording amortised O(1); the cap
  // retains minutes of per-frame samples, far above the QA budgets' minimums.
  record(name,value){if(!this.enabled||!Number.isFinite(value))return;const values=this.samples.get(name)??[];if(values.length>=PERFORMANCE_SAMPLE_CAP)values.splice(0,PERFORMANCE_SAMPLE_CAP>>2);values.push(value);this.samples.set(name,values);},
  reset(){this.samples.clear();},
  snapshot(){return Object.fromEntries([...this.samples].map(([name,values])=>[name,[...values]]));}
};
// Tone's implicit "now" includes a 100 ms scheduler look-ahead. That is
// correct for scheduling clicks, but ahead of what the listener hears NOW.
// Read the same transport at the audio output's current context time instead.
function elapsedTransportBeats(run,now){
  const outputTime=audio.outputTime(now);
  // The output can still be playing the preceding run during look-ahead.
  if(outputTime<run.transportStart)return 0;
  const transport=Tone.getTransport();
  return transport.getTicksAtTime(outputTime)/transport.PPQ;
}
const transportBeat=(run,now=performance.now())=>run.offset+elapsedTransportBeats(run,now)-run.countBeats;
const presentation=createPresentationHost(()=>audio.activeRun?{beat:Math.min(audio.activeRun.totalBeats,transportBeat(audio.activeRun)),tempoBpm:Tone.getTransport().bpm.value}:null,highwayError);
let highwayHandle=null,highwayObserver=null,highwaySelected=false;
function highwayError(error){
  highwayObserver?.disconnect();highwayObserver=null;highwayHandle=null;
  const message=document.getElementById('highway-error');
  if(message){message.textContent=`Highway unavailable: ${error instanceof Error?error.message:String(error)}. Switch to Notation to view this exercise.`;message.classList.remove('hidden');}
}
function mountHighway(){
  const viewport=document.getElementById('highway-timeline'),message=document.getElementById('highway-error');
  if(!viewport||highwayHandle)return;
  if(message){message.textContent='';message.classList.add('hidden');}
  try{
    highwayHandle=presentation.mount(createHighwaySvg(viewport,performanceProfile.enabled?value=>performanceProfile.record('highway_render_ms',value):null));
    const resize=()=>{const rect=viewport.getBoundingClientRect();highwayHandle?.resize({width:rect.width,height:rect.height});};
    highwayObserver=new ResizeObserver(resize);highwayObserver.observe(viewport);resize();
  }catch(error){highwayHandle?.dispose();highwayError(error);}
}
function setHighwayMode(selected){
  highwaySelected=selected;
  if(selected)mountHighway();
  else{highwayObserver?.disconnect();highwayObserver=null;highwayHandle?.dispose();highwayHandle=null;}
}
function highwayExerciseChanged(){
  presentation.exerciseChanged();
  if(highwaySelected&&!presentation.isActive())mountHighway();
}
function savedHighwayMode(){try{return localStorage.getItem('rudiment-visualisation')==='highway';}catch{return false;}}
function saveHighwayMode(selected){try{localStorage.setItem('rudiment-visualisation',selected?'highway':'notation');}catch{/* Storage is optional. */}}
function savedNotationTheme(){try{return localStorage.getItem('rudiment-notation-theme')!=='light';}catch{return true;}}
// First-run onboarding opt-out. Only a checked "Don't show me this again" is
// persisted; an unchecked dismissal stays session-only, so the flag's absence
// means onboarding should be offered again next session.
function savedOnboardingSkip(){try{return localStorage.getItem('rudiment-onboarding-skipped')==='true';}catch{return false;}}
function saveOnboardingSkip(skip){try{localStorage.setItem('rudiment-onboarding-skipped',skip?'true':'false');}catch{/* Storage is optional. */}}
function saveNotationTheme(dark){try{localStorage.setItem('rudiment-notation-theme',dark?'dark':'light');}catch{/* Storage is optional. */}}
function tempoAtBeat(beat,map,initialBpm){let bpm=initialBpm;for(const change of map){if(change.beat>beat)break;bpm=change.bpm;}return bpm;}
function transportPositionAtBeat(beat,meter){const bars=Math.floor(beat/meter),within=beat-bars*meter;return `${bars}:${within}:0`;}

function positionMetronomePopover(){
  const popup=document.getElementById('metronome-popover');
  if(!popup||popup.classList.contains('hidden'))return;
  popup.style.setProperty('--metronome-offset-y','0px');
  const bounds=popup.getBoundingClientRect(),scale=bounds.width/popup.offsetWidth;
  const offset=Math.max(8-bounds.top,Math.min(0,window.innerHeight-8-bounds.bottom));
  popup.style.setProperty('--metronome-offset-y',`${offset/scale}px`);
}
let metronomePopoverObserver;
document.addEventListener('click',event=>{
  if(!(event.target instanceof Element)||!event.target.closest('#visual-metronome'))return;
  requestAnimationFrame(()=>{
    const popup=document.getElementById('metronome-popover');
    if(popup&&!metronomePopoverObserver){
      metronomePopoverObserver=new ResizeObserver(positionMetronomePopover);
      metronomePopoverObserver.observe(popup);
    }
    positionMetronomePopover();
  });
});
window.addEventListener('resize',positionMetronomePopover);
// A resize can change the sweep width (responsive CSS); drop the cached
// needle so the next frame re-measures its travel.
window.addEventListener('resize',()=>{visualMetronome.needle=null;});

// Native selects can match :focus-visible even when opened by a pointer.
// Keep keyboard focus visible, but do not retain that ring after mouse/touch.
document.addEventListener('pointerdown',event=>{
  if(event.target instanceof Element)event.target.closest('.loop-control')?.setAttribute('data-pointer-focus','');
},true);
document.addEventListener('keydown',event=>{
  if(event.key==='Tab'||event.target instanceof Element&&event.target.closest('.loop-control'))document.querySelector('.loop-control')?.removeAttribute('data-pointer-focus');
},true);

// A glanceable drummer-style display: the sweep borrows the left/right motion
// of a digital needle, while the LEDs show beat position and a stronger
// downbeat. Pulses are queued from the same Tone callback and output timestamp
// as the audible click, so there is no independent visual timer to drift.
// The pulse animations are PRE-ARMED at schedule time (the transport knows
// each beat ~100 ms ahead): each dot's animation and its glow child get a
// future WAAPI startTime and run on the compositor, so the beat moment itself
// carries no style writes, paints or animation restarts - the main thread
// only records the data-pulse timing attributes the metronome e2e observes.
const visualMetronome = {
  pending: [], lastStep: null, lastArmedStep: null, lastDisplayBeat: null,
  schedule(performanceTime,beat,meter,subdivision,beatMs){
    // The three metronome switches are INDEPENDENT enables: the master
    // 'beat lights' controls labels and the current-dot highlight, while the
    // needle and pulse sub-toggles enable their own elements even when the
    // master is off. Scheduling runs when any element is enabled.
    if(!(diagnostics.metronomeVisual||diagnostics.metronomeNeedle||diagnostics.metronomePulses))return;
    const step=Math.round(beat*Math.max(1,subdivision));
    const event={performanceTime,beat,meter:Math.max(1,Math.round(meter)),step};
    // The pending queue is the pulse observation channel only, so it is
    // pushed and drained under the pulse switch alone.
    if(diagnostics.metronomePulses)this.pending.push(event);
    if(step!==this.lastArmedStep){
      this.lastArmedStep=step;
      // Only integer beats arm the per-beat visuals. Subdivision steps must
      // not re-arm the beat dot's pulse: every re-seek slid the 180 ms pulse
      // forward to the latest sub-step's schedule time, so at subdivision 4
      // the pulse fired up to three quarters of a beat late and was
      // restarted mid-flight two to four times per beat.
      const onBeat=Number.isInteger(beat);
      if(diagnostics.metronomePulses&&onBeat)this.armPulse(event);
      if(diagnostics.metronomeVisual&&onBeat)this.armLights(event);
      // Even integer beats begin a needle sweep. Re-arming from the
      // authoritative schedule time keeps the compositor animation
      // phase-locked to the audible clock (drift corrected every two beats)
      // and absorbs tempo changes within one cycle.
      if(diagnostics.metronomeNeedle&&onBeat&&Math.abs(beat)%2===0){
        this.armNeedle(performanceTime,beatMs);
      }
    }
  },
  armNeedle(performanceTime,beatMs){
    const root=document.getElementById('visual-metronome');if(!root)return;
    // Needle-only bisection mode still highlights the needle colour.
    root.classList.add('active');
    if(!this.needle||!this.needle.isConnected||!this.needleAnimation){
      this.needle=root.querySelector('.visual-metronome-sweep i');
      const sweep=root.querySelector('.visual-metronome-sweep');
      this.needleTravel=Math.max(4,(sweep?.clientWidth??58)/2-6);
      this.needleAnimation?.cancel();
      this.needleAnimation=this.needle?.animate([
        {transform:`translate3d(${-this.needleTravel}px,0,0)`},
        {transform:`translate3d(${this.needleTravel}px,0,0)`}
      ],{duration:beatMs,iterations:Infinity,direction:'alternate',easing:'ease-in-out',fill:'backwards'});
      if(this.needleAnimation)this.needleAnimation.startTime=performanceTime;
      return;
    }
    // Correct a running sweep only when the tempo or its audible phase changes.
    const cycle=2*beatMs,animation=this.needleAnimation;
    const phase=Math.abs((performanceTime-animation.startTime)%cycle);
    if(animation.effect.getTiming().duration===beatMs&&Math.min(phase,cycle-phase)<2)return;
    animation.effect.updateTiming({duration:beatMs});
    animation.startTime=performanceTime-cycle;
  },
  armPulse(event){
    const root=document.getElementById('visual-metronome');if(!root)return;
    const dots=this.beatDots(root);
    const index=((Math.floor(event.beat)%event.meter)+event.meter)%event.meter;
    const dot=dots[index];if(!dot)return;
    // One animation per dot, re-used by seeking: a future startTime discards
    // any in-flight iteration and starts the keyframes exactly at the beat's
    // output time on the animation timeline.
    if(!dot.pulseAnimation){
      dot.pulseAnimation=dot.animate([
        {transform:'scale(1)'},
        {transform:index===0?'scale(1.9)':'scale(1.65)'},
        {transform:'scale(1)'}
      ],{duration:180,easing:'cubic-bezier(.2,.8,.2,1)'});
    }
    dot.pulseAnimation.startTime=event.performanceTime;
    // The glow is a permanently-mounted child with a static shadow; fading
    // its opacity replaces the previous per-beat inline box-shadow writes
    // and their repaints.
    const glow=dot.firstElementChild;
    if(glow&&!glow.glowAnimation){
      glow.glowAnimation=glow.animate([{opacity:0},{opacity:1,offset:.45},{opacity:0}],{duration:180,easing:'linear'});
    }
    if(glow)glow.glowAnimation.startTime=event.performanceTime;
  },
  beatDots(root){
    if(!this.dots?.[0]?.isConnected){
      this.dots=[...root.querySelectorAll('.visual-metronome-beats i')];
      this.lastLitIndex=null;
    }
    return this.dots;
  },
  armLights(event){
    const root=document.getElementById('visual-metronome');if(!root)return;
    const dots=this.beatDots(root);
    const index=((event.beat%event.meter)+event.meter)%event.meter;
    if(!dots[index]||index===this.lastLitIndex)return;
    if(this.lastLitIndex!=null)this.lightTransition(dots[this.lastLitIndex],false,event.performanceTime);
    this.lightTransition(dots[index],true,event.performanceTime);
    this.lastLitIndex=index;
  },
  lightTransition(dot,lit,startTime){
    // Only the opacity of a static highlight (including its shadow) changes.
    // Backwards fill holds the previous appearance during audio look-ahead.
    dot.classList.toggle('current',lit);
    if(!dot.lightAnimation){
      dot.lightAnimation=dot.lastElementChild.animate([{opacity:0},{opacity:1}],
        {duration:90,fill:'backwards'});
    }
    dot.lightAnimation.effect.updateTiming({direction:lit?'normal':'reverse'});
    dot.lightAnimation.startTime=startTime;
  },
  pulse(event){
    const root=document.getElementById('visual-metronome');if(!root)return;
    performanceProfile.record('metronome_visual_jitter_ms',performance.now()-event.performanceTime);
    // Observation channel only: the visual pulse was pre-armed at schedule
    // time and runs on the compositor, so the beat moment carries no style
    // or animation work - just these style-free data attributes the
    // visual-metronome e2e uses as its scheduled-vs-observed timing channel.
    root.dataset.pulse=String(event.step);root.dataset.scheduledAt=String(event.performanceTime);
  },
  render(beat,meter,subdivision,now){
    if(!(diagnostics.metronomeVisual||diagnostics.metronomeNeedle||diagnostics.metronomePulses))return;
    const root=document.getElementById('visual-metronome');if(!root)return;
    meter=Math.max(1,Math.round(meter));subdivision=Math.max(1,subdivision);
    const whole=Math.floor(beat);
    const beatInBar=((whole%meter)+meter)%meter;
    const step=Math.floor(beat*subdivision);
    // The needle is NOT written here: it is a compositor-driven WAAPI
    // animation re-armed at every even scheduled beat (see schedule/
    // armNeedle), so beat lights add zero per-frame main-thread work. The
    // displayed labels and the current-dot highlight belong to the MASTER
    // beat-lights switch; needle-only and pulses-only modes show their
    // element alone for the on-device bisection.
    if(diagnostics.metronomeVisual){
      const last=this.lastDisplayBeat;
      if(!last||last.count!==beat<0||last.beatInBar!==beatInBar||last.step!==step||last.meter!==meter){
        this.lastDisplayBeat={count:beat<0,beatInBar,step,meter};
        root.classList.add('active');root.classList.toggle('count-in',beat<0);
        root.dataset.beat=String(beatInBar+1);root.dataset.step=String(step);
        // The current-dot highlight is pre-armed at schedule time (armLights);
        // only the non-painting label/aria bookkeeping stays on the frame.
        root.setAttribute('aria-label',`Metronome settings, ${beat<0?'COUNT IN':'PLAY'}, beat ${beatInBar+1} of ${meter}`);
      }
    }
    while(diagnostics.metronomePulses&&this.pending.length&&this.pending[0].performanceTime<=now+8){
      const event=this.pending.shift();if(event.step!==this.lastStep){this.lastStep=event.step;this.pulse(event);}
    }
  },
  // Labels/current-dot teardown for the master switch alone: leaves needle
  // and pulse animations running when only the labels are switched off. The
  // sweep needle's highlighted colour rides the same 'active' class, so it
  // stays while the needle switch is on.
  clearLabels(){
    const root=document.getElementById('visual-metronome');if(!root)return;
    root.classList.remove('count-in');
    if(!diagnostics.metronomeNeedle)root.classList.remove('active');
    this.lastDisplayBeat=null;
    delete root.dataset.beat;delete root.dataset.step;
    if(this.dots)this.dots.forEach(dot=>{
      dot.classList.remove('current');
      dot.lightAnimation?.cancel();dot.lightAnimation=null;
    });
    this.lastLitIndex=null;
    root.setAttribute('aria-label','Metronome settings');
  },
  stop(){
    this.pending.length=0;this.lastStep=null;this.lastArmedStep=null;this.lastDisplayBeat=null;this.dots=null;this.lastLitIndex=null;this.needle=null;
    const root=document.getElementById('visual-metronome');if(!root)return;
    root.classList.remove('active','count-in');
    // The needle animation is compositor-driven; cancel it so the needle
    // returns to its centred CSS position.
    this.needleAnimation?.cancel();this.needleAnimation=null;
    delete root.dataset.beat;delete root.dataset.step;delete root.dataset.pulse;delete root.dataset.scheduledAt;
    root.querySelectorAll('.visual-metronome-beats i').forEach(dot=>{
      dot.classList.remove('current');
      dot.pulseAnimation?.cancel();dot.pulseAnimation=null;
      dot.lightAnimation?.cancel();dot.lightAnimation=null;
      const glow=dot.firstElementChild;
      glow?.glowAnimation?.cancel();if(glow)glow.glowAnimation=null;
    });
  }
};

// Deferred scoring/UI tick: the transport frame decides when scoring work is
// due, but the work itself runs as a separate macrotask instead of inside the
// animation-frame callback batch that the browser must follow with that
// frame's style, layout and paint steps. A zero-delay timeout is NOT a
// presentation fence: it typically lands after the dispatching frame's
// rendering steps, but it can be clamped or run directly before the next
// frame's callbacks, so scoring work still shares the interval between two
// rendering opportunities and can delay the next playhead commit. The
// deferral removes the tick from the same-frame callback batch; it does not
// and cannot guarantee an isolated budget. Scoring watermarks are monotonic,
// so when ticks queue up faster than they run, keeping only the newest
// pending tick cannot lose scoring state - the newer tick's advance subsumes
// everything the older one would have confirmed.
let pendingTick=null,pendingTickTimer=0;
function deferTick(fn){
  pendingTick=fn;
  if(!pendingTickTimer)pendingTickTimer=setTimeout(flushPendingTick,0);
}
function flushPendingTick(){
  if(pendingTickTimer){clearTimeout(pendingTickTimer);pendingTickTimer=0;}
  const fn=pendingTick;pendingTick=null;
  if(fn)fn();
}
// Deferred-tick cost, recorded into the HUD's preallocated ring so the
// measurement itself allocates nothing. Only collected while a diagnostic
// mode is active, per the project's opt-in profiling policy.
function recordTickDuration(ms){
  if(!(diagnostics.referenceOverlay||performanceProfile.enabled))return;
  playheadHud.ticks[playheadHud.tickIndex]=ms;
  playheadHud.tickIndex=(playheadHud.tickIndex+1)%PLAYHEAD_HUD_WINDOW;
  playheadHud.tickCount=Math.min(playheadHud.tickCount+1,PLAYHEAD_HUD_WINDOW);
}
// On-device readout for the playhead-smoothness investigation: live frame
// pacing and deferred-tick cost. Deliberately independent of the reference
// overlay: it samples the PRODUCTION transport frame's own cadence and is
// shown whenever any measurement mode is active (reference overlay or
// performance counters), so production pacing can be measured without the
// overlay's visual overhead distorting perception or the GPU load.
//
// The readout itself must not disturb what it measures: on the iPad the
// frame budget sits close to its edge, and an earlier version of this HUD
// visibly jittered playback by re-sorting copied arrays, filtering and
// spreading twice a second. Statistics are now computed from preallocated
// ring buffers in a single allocation-free pass, and the DOM text is only
// written when it changes. The thresholds are 60 Hz-aware: a frame that runs
// 18.5-24 ms is late for its vsync and visible as micro-jitter even though
// older p95/>25 ms gauges read clean.
const PLAYHEAD_HUD_WINDOW=128,PLAYHEAD_HUD_VSYNC_MS=18.5,PLAYHEAD_HUD_BUCKETS=132;
const playheadHud={
  element:null,lastPaint:0,lastText:'',
  frames:new Float64Array(PLAYHEAD_HUD_WINDOW),frameCount:0,frameIndex:0,
  ticks:new Float64Array(PLAYHEAD_HUD_WINDOW),tickCount:0,tickIndex:0,
  histogram:new Uint32Array(PLAYHEAD_HUD_BUCKETS),
  active(){return diagnostics.referenceOverlay||performanceProfile.enabled;},
  syncVisibility(){ if(this.active())this.show(); else this.hide(); },
  show(){
    if(this.element?.isConnected)return;
    this.element=document.createElement('div');
    this.element.className='playhead-debug-hud';
    this.element.setAttribute('aria-hidden','true');
    document.body.appendChild(this.element);
  },
  hide(){this.element?.remove();this.lastText='';},
  reset(){this.frameCount=0;this.frameIndex=0;this.tickCount=0;this.tickIndex=0;this.lastPaint=0;},
  sample(frameIntervalMs){
    this.frames[this.frameIndex]=frameIntervalMs;
    this.frameIndex=(this.frameIndex+1)%PLAYHEAD_HUD_WINDOW;
    this.frameCount=Math.min(this.frameCount+1,PLAYHEAD_HUD_WINDOW);
  },
  // Single allocation-free pass over a ring buffer: max, threshold counts and
  // a 0.5 ms histogram from which percentiles are read. The histogram array
  // is reused across calls; nothing here allocates on the measurement path.
  scan(values,count,index){
    this.histogram.fill(0);
    let max=0,overVsync=0,over25=0;
    for(let seen=0;seen<count;seen++){
      const position=(index+seen)%PLAYHEAD_HUD_WINDOW,value=values[position];
      if(value>max)max=value;
      if(value>PLAYHEAD_HUD_VSYNC_MS)overVsync++;
      if(value>25)over25++;
      this.histogram[value>=65.5?PLAYHEAD_HUD_BUCKETS-1:Math.min(PLAYHEAD_HUD_BUCKETS-1,Math.round(value*2))]++;
    }
    return {max,overVsync,over25,count};
  },
  percentileOf(scanResult,p){
    const total=scanResult.count;
    if(!total)return Number.NaN;
    let accumulated=0;
    for(let bucket=0;bucket<PLAYHEAD_HUD_BUCKETS;bucket++){
      accumulated+=this.histogram[bucket];
      if(accumulated>=total*p)return bucket/2;
    }
    return Number.NaN;
  },
  render(now){
    if(now-this.lastPaint<500)return;
    this.lastPaint=now;
    if(!this.element?.isConnected){
      // Self-healing creation so URL-seeded diagnostic pages get the HUD
      // without an explicit init call at module scope (TDZ ordering).
      if(!this.active())return;
      this.show();
    }
    const frameStats=this.scan(this.frames,this.frameCount,this.frameIndex);
    const frameP95=this.percentileOf(frameStats,.95);
    const frameP99=this.percentileOf(frameStats,.99);
    // Reuse the same histogram for the tick window.
    const tickStats=this.scan(this.ticks,this.tickCount,this.tickIndex);
    const tickP95=this.percentileOf(tickStats,.95);
    const fixed=value=>Number.isFinite(value)?value.toFixed(1):'—';
    const text=`build ${BRIDGE_BUILD} · rAF p95 ${fixed(frameP95)} / p99 ${fixed(frameP99)} / max ${fixed(frameStats.max)} ms · >${PLAYHEAD_HUD_VSYNC_MS} ms: ${frameStats.overVsync} · >25 ms: ${frameStats.over25} · tick p95 ${fixed(tickP95)} / max ${fixed(tickStats.max)} ms`;
    if(text!==this.lastText){this.lastText=text;this.element.textContent=text;}
  },
  snapshot(){
    const frameStats=this.scan(this.frames,this.frameCount,this.frameIndex);
    const frameP95=this.percentileOf(frameStats,.95);
    const frameP99=this.percentileOf(frameStats,.99);
    const tickStats=this.scan(this.ticks,this.tickCount,this.tickIndex);
    return {
      build:BRIDGE_BUILD,
      frame_p95:frameP95,
      frame_p99:frameP99,
      frame_max:frameStats.max,
      frame_over18_5:frameStats.overVsync,
      frame_over25:frameStats.over25,
      frame_n:frameStats.count,
      tick_p95:this.percentileOf(tickStats,.95),
      tick_max:tickStats.max,
      tick_n:tickStats.count,
    };
  },
};

// Maps a transport beat to the notated score beat shown to the user. Count-in
// beats are transport time, not playable exercise beats: for a later selected
// range they numerically overlap the preceding bar, so the playhead stays
// hidden (-1) until the selected playback offset begins.
function scoreBeatForDisplay(musicalBeat, offset, options){
  if(musicalBeat<offset)return -1;
  const rangeStart=Number(options.displayRangeStart)||0,rangeBeats=Math.max(.001,Number(options.displayRangeBeats)||options.exerciseBeats);
  const position=((musicalBeat-rangeStart)%rangeBeats+rangeBeats)%rangeBeats;
  const bars=options.displayBars,barIndex=Math.floor(position/options.meter);
  return Array.isArray(bars)&&bars.length?Number(bars[Math.min(barIndex,bars.length-1)])*options.meter+position-barIndex*options.meter:rangeStart+position;
}

// Score position of the next display downbeat after a musical beat, so the
// notation playhead can preview the next system during its lead-in. A bar
// selection or loop continues on the display sequence, and a run whose final
// pass ends at that boundary has no further downbeat to lead into (null).
function nextDownbeatScoreBeat(musicalBeat, options){
  const meter=options.meter,rangeStart=Number(options.displayRangeStart)||0;
  const rangeBeats=Math.max(.001,Number(options.displayRangeBeats)||options.exerciseBeats);
  const rel=musicalBeat-rangeStart;
  if(!(rel>=0))return null;
  const pass=Math.floor(rel/rangeBeats),within=rel-pass*rangeBeats;
  const barIndex=Math.floor(within/meter);
  const nextDownbeat=(barIndex+1)*meter<rangeBeats-.000001
    ?rangeStart+pass*rangeBeats+(barIndex+1)*meter
    :rangeStart+(pass+1)*rangeBeats;
  return nextDownbeat<options.totalBeats-.000001?scoreBeatForDisplay(nextDownbeat,options.offsetBeat||0,options):null;
}

// Tone.Transport is the single musical transport; source nodes use callback audio time.
const audio = {
  raw: null, snareGain: null, clickGain: null, master: null, buffers: [], buffersPromise: null, drumBuffers: {}, drumBuffersPromise: null, drumSampleErrors: {}, drumStarts: {kick:0,closed_hat:0,open_hat:0}, openHatVoices:new Set(), metronomeBuffers: {}, metronomeBuffersPromise: null, metronomeSampleErrors: {}, layerGains: [], layerPeaks: [], sampleErrors: [], snareStarts: 0, lastSnareError: null, lastMetronomeError:null, voices:new Set(), scheduledVoices:new Set(), raf: 0, generation: 0, state: 'locked', activeRun:null, lastBeat:0,
  preload() {
    this.state = 'loading';
    this.raw ??= Tone.getContext().rawContext;
    // Tone 15 wraps the native AudioContext with standardized-audio-context,
    // which exposes currentTime but omits the output timestamp and latency.
    // Limit access to the pinned wrapper's backing context to this adapter.
    this.outputContext ??= this.raw._nativeAudioContext ?? this.raw;
    if (!this.master) {
      this.master = this.raw.createGain(); this.master.gain.value = .84;
      this.snareGain = this.raw.createGain(); this.snareGain.gain.value = .72;
      this.clickGain = this.raw.createGain(); this.clickGain.gain.value = 1.08;
      const limiter = this.raw.createDynamicsCompressor(); limiter.threshold.value = -3; limiter.ratio.value = 12;
      this.snareGain.connect(this.master); this.clickGain.connect(this.master); this.master.connect(limiter); limiter.connect(this.raw.destination);
    }
    if(!this.buffersPromise){
      // PCM WAV avoids the optional AAC decoder used by several embedded Web
      // MIDI browsers while preserving the same four acoustic recordings.
      const urls = ['audio/snare-soft.wav','audio/snare-medium.wav','audio/snare-hard.wav','audio/snare-accent.wav'].map(publicAsset);
      // Sample loading must never hold the audio context or practice transport
      // hostage. Some Web MIDI wrapper browsers expose Web Audio but leave audio
      // decodeAudioData pending forever. Until an acoustic layer has decoded,
      // snare monitoring stays silent rather than substituting a generated sound.
      this.buffersPromise = Promise.all(urls.map(async (url,index) => {
        try {
          const response = await fetch(url); if (!response.ok) throw new Error(String(response.status));
          const buffer = await this.raw.decodeAudioData(await response.arrayBuffer());
          let peak = 0;
          for (let channel=0;channel<buffer.numberOfChannels;channel++) {
            const data=buffer.getChannelData(channel);
            for (let sample=0;sample<data.length;sample++) peak=Math.max(peak,Math.abs(data[sample]));
          }
          // The source recordings vary by almost 28 dB. Normalise their
          // transients after decoding so velocity changes timbre/dynamics
          // without making a successfully triggered layer sound missing.
          if (!Number.isFinite(peak) || peak < .00001) throw new Error('Decoded sample is silent');
          this.layerPeaks[index]=peak;
          this.layerGains[index]=peak>0?Math.min(24,Math.max(.5,.32/peak)):1;
          this.buffers[index]=buffer;
          return buffer;
        }
        catch(error) { this.sampleErrors[index]=audioErrorMessage(error); return null; }
      })).then(buffers => {
        this.buffers=buffers;
        this.updateSampleState();
        return buffers;
      });
    }
    if(!this.metronomeBuffersPromise){
      const entries=Object.entries(metronomeSampleFiles).flatMap(([,layers])=>
        Object.values(layers).map(path=>({name:metronomeSampleName(path),url:publicAsset(path)}))
      );
      this.metronomeBuffersPromise=Promise.all(entries.map(async({name,url})=>{
        try{
          const response=await fetch(url);if(!response.ok)throw new Error(String(response.status));
          const buffer=await this.raw.decodeAudioData(await response.arrayBuffer());
          let peak=0;
          for(let channel=0;channel<buffer.numberOfChannels;channel++){
            const data=buffer.getChannelData(channel);
            for(let sample=0;sample<data.length;sample++)peak=Math.max(peak,Math.abs(data[sample]));
          }
          if(!Number.isFinite(peak)||peak<.00001)throw new Error('Decoded sample is silent');
          return {name,buffer};
        }catch(error){
          this.metronomeSampleErrors[name]=audioErrorMessage(error);
          return {name,buffer:null};
        }
      })).then(entries=>{
        this.metronomeBuffers=Object.fromEntries(entries.map(({name,buffer})=>[name,buffer]));
        this.updateSampleState();
        return this.metronomeBuffers;
      });
    }
    if(!this.drumBuffersPromise){
      this.drumBuffersPromise=Promise.all(Object.entries(drumSampleFiles).map(async([name,path])=>{
        try{
          const response=await fetch(publicAsset(path));if(!response.ok)throw new Error(String(response.status));
          return [name,await this.raw.decodeAudioData(await response.arrayBuffer())];
        }catch(error){
          this.drumSampleErrors[name]=audioErrorMessage(error);
          return [name,null];
        }
      })).then(entries=>{
        this.drumBuffers=Object.fromEntries(entries);
        this.updateSampleState();
        return this.drumBuffers;
      });
    }
    return Promise.all([this.buffersPromise,this.metronomeBuffersPromise,this.drumBuffersPromise]);
  },
  updateSampleState(){
    const drumsReady=this.buffers.some(Boolean)||Object.values(this.drumBuffers).some(Boolean),metronomeReady=Object.values(this.metronomeBuffers).some(Boolean);
    this.state=drumsReady||metronomeReady?'ready':'unavailable';
  },
  async waitForMetronome(timbre){
    const layers=metronomeSampleFiles[timbre];
    if(!layers)throw new Error(`Unknown metronome preset: ${timbre}`);
    await this.metronomeBuffersPromise;
    const missing=Object.entries(layers).filter(([,path])=>!this.metronomeBuffers[metronomeSampleName(path)]);
    if(missing.length){
      const details=missing.map(([,path])=>`${metronomeSampleName(path)}: ${this.metronomeSampleErrors[metronomeSampleName(path)]||'sample unavailable'}`).join('; ');
      throw new Error(`Unable to load metronome samples (${details})`);
    }
    return true;
  },
  async ensure() {
    // Start/resume the context immediately while the Connect click still has
    // mobile browser user activation. MIDI permission UI may otherwise consume
    // that activation before the asynchronous sample decode has completed.
    const startPromise=Tone.start();
    this.preload();
    try { await startPromise; }
    catch(error) { this.state='locked'; this.lastSnareError=audioErrorMessage(error); throw error; }
    if(this.buffers.length||Object.keys(this.drumBuffers).length||Object.keys(this.metronomeBuffers).length)this.updateSampleState();
    return { state: this.state, sampleLayers: this.buffers.filter(Boolean).length, drumSampleCount:Object.values(this.drumBuffers).filter(Boolean).length, metronomeSampleCount:Object.values(this.metronomeBuffers).filter(Boolean).length };
  },
  // Both projections use the same output-clock correlation. A fresh device
  // timestamp can trail the rendering clock by hundreds of milliseconds.
  outputTime(now) {
    const context=this.outputContext,time=context.getOutputTimestamp?.();
    if(time&&time.performanceTime>0&&Number.isFinite(time.contextTime)&&time.contextTime>=0
      &&Math.abs(now-time.performanceTime)<1000){
      return time.contextTime+(now-time.performanceTime)/1000;
    }
    return context.currentTime-(context.outputLatency||0);
  },
  perfAt(audioTime) {
    const now=performance.now();
    return now+(audioTime-this.outputTime(now))*1000;
  },
  playSnare(velocity = 105, when = null, articulation = 'center') {
    const profileStart=performanceProfile.enabled?performance.now():0;
    if (!this.raw || this.raw.state !== 'running') { this.state='locked'; return false; }
    if(Number(velocity)<=0)return false;
    const time = when ?? this.raw.currentTime, scheduled=time>this.raw.currentTime+.001, response=velocityGain(velocity);
    const layers=velocityLayers(velocity).filter(layer=>this.buffers[layer.index]);
    let started=0;
    if (layers.length) {
      for(const layer of layers){
        try {
          const source = this.raw.createBufferSource(), gain = this.raw.createGain(); source.buffer = this.buffers[layer.index];
          const articulationRate=articulation==='rim'?1.16:articulation==='snare_wires_off' ? .88 : 1;
          const articulationGain=articulation==='rim' ? .82 : articulation==='snare_wires_off' ? .72 : 1;
          source.playbackRate.value = articulationRate * (.99 + Math.random() * .02);
          gain.gain.value = (this.layerGains[layer.index]||1) * response * layer.weight * articulationGain;
          source.connect(gain); gain.connect(this.snareGain);this.voices.add(source);
          source.onended=()=>{this.voices.delete(source);this.scheduledVoices.delete(source);source.disconnect();gain.disconnect();};
          if(scheduled)this.scheduledVoices.add(source);
          source.start(time);started++;
        } catch(error) { this.lastSnareError=audioErrorMessage(error); }
      }
    }
    this.snareStarts+=started;
    performanceProfile.record('audio_scheduling_ms',performance.now()-profileStart);
    return started>0;
  },
  monitorSnare(velocity = 105) {
    if(Number(velocity)<=0)return false;
    if(this.playSnare(velocity))return true;
    // The context is already available, so a false result here only means the
    // acoustic layer is still loading or unavailable. Do not ask Tone to unlock
    // again for every hit while preloading is in flight.
    if(this.raw?.state==='running')return true;
    // A permission sheet, screen rotation or mobile power policy can suspend a
    // previously unlocked context. The Connect gesture has already authorised
    // audio, so make the sample graph ready for the next physical strike.
    // Auto-reconnected MIDI devices can deliver a hit before the user has
    // opened the connection panel. Create/resume the graph without replacing
    // that first strike with a generated sound.
    this.ensure().then(()=>this.playSnare(velocity)).catch(error=>{this.lastSnareError=audioErrorMessage(error);});
    return true;
  },
  chokeOpenHat(when = null) {
    const time=when??this.raw?.currentTime??0;
    for(const source of this.openHatVoices){
      try{source.stop(Math.max(time,(this.raw?.currentTime??time))+.012);}
      catch{/* A voice that has already ended is harmless. */}
    }
  },
  playDrum(instrument='snare',articulation='unspecified',velocity=105,when=null) {
    if(instrument==='snare')return this.playSnare(velocity,when,articulation);
    const canonical=instrument==='pedal_hat'?'closed_hat':instrument;
    if(!Object.hasOwn(drumSampleFiles,canonical))return false;
    if(!this.raw||this.raw.state!=='running'||Number(velocity)<=0)return false;
    const time=when??this.raw.currentTime;
    if(canonical==='closed_hat'||articulation==='closed'||articulation==='pedal')this.chokeOpenHat(time);
    const buffer=this.drumBuffers[canonical];
    if(!buffer)return false;
    try{
      const source=this.raw.createBufferSource(),gain=this.raw.createGain(),scheduled=time>this.raw.currentTime+.001;
      source.buffer=buffer;
      source.playbackRate.value=.99+Math.random()*.02;
      gain.gain.value=velocityGain(velocity)*(canonical==='kick'?1.15:.82);
      source.connect(gain);gain.connect(this.snareGain);
      this.voices.add(source);
      if(scheduled)this.scheduledVoices.add(source);
      if(canonical==='open_hat')this.openHatVoices.add(source);
      source.onended=()=>{this.voices.delete(source);this.scheduledVoices.delete(source);this.openHatVoices.delete(source);source.disconnect();gain.disconnect();};
      source.start(time);
      this.drumStarts[canonical]=(this.drumStarts[canonical]??0)+1;
      return true;
    }catch(error){this.lastSnareError=audioErrorMessage(error);return false;}
  },
  monitorDrum(instrument='snare',articulation='unspecified',velocity=105){
    if(Number(velocity)<=0)return false;
    if(this.playDrum(instrument,articulation,velocity))return true;
    if(this.raw?.state==='running')return true;
    this.ensure().then(()=>this.playDrum(instrument,articulation,velocity)).catch(error=>{this.lastSnareError=audioErrorMessage(error);});
    return true;
  },
  click(time, accent = false, timbre = 'woodblock') {
    const profileStart=performanceProfile.enabled?performance.now():0;
    const samplePath=metronomeSampleFiles[timbre]?.[accent?'accent':'regular'];
    const sampleName=samplePath?metronomeSampleName(samplePath):`${timbre}-${accent?'accent':'regular'}`;
    const buffer=this.metronomeBuffers[sampleName];
    if(!buffer){this.lastMetronomeError=`Missing decoded sample: ${sampleName}`;return false;}
    try{
      const source=this.raw.createBufferSource();source.buffer=buffer;source.connect(this.clickGain);
      source.onended=()=>{this.voices.delete(source);this.scheduledVoices.delete(source);source.disconnect();};
      this.voices.add(source);
      if(time>this.raw.currentTime+.001)this.scheduledVoices.add(source);
      source.start(time);
      performanceProfile.record('metronome_audio_scheduling_ms',performance.now()-profileStart);
      return true;
    }catch(error){
      this.lastMetronomeError=audioErrorMessage(error);
      performanceProfile.record('metronome_audio_scheduling_ms',performance.now()-profileStart);
      return false;
    }
  },
  stop() {
    // Settle any dispatched scoring tick before invalidating the run, so
    // pause, stop and completion never drop scoring state or feedback.
    flushPendingTick();
    if(this.activeRun){this.lastBeat=Math.min(this.activeRun.totalBeats,transportBeat(this.activeRun));if(presentation.isActive())presentation.position(this.lastBeat,Tone.getTransport().bpm.value);}
    this.generation++; cancelAnimationFrame(this.raf); this.raf = 0;
    playheadDebug.pause();
    // Stop the old clock at the current audio time, not Tone.now() (which is
    // in the future). Otherwise an output-time read sees the previous run
    // through the look-ahead window during a restart or bar retry.
    const transport = Tone.getTransport(); transport.stop(this.raw?.currentTime??Tone.immediate()); transport.cancel(0); transport.seconds = 0;
    for(const source of this.scheduledVoices){
      try{source.stop();}
      catch(error){this.lastSnareError=audioErrorMessage(error);}
    }
    this.scheduledVoices.clear();
    this.activeRun=null;visualMetronome.stop();
  },
  setTempo(bpm) {
    const requested=Math.max(30,Math.min(240,Number(bpm)||60)),run=this.activeRun;
    if(!run)return false;
    run.tempoScale=requested/run.sourceBpm;
    const transport=Tone.getTransport(),elapsed=elapsedTransportBeats(run,performance.now());
    const rawBeat=run.offset+elapsed-run.countBeats,exerciseBeat=Math.max(run.offset,rawBeat);
    const value=tempoAtBeat(exerciseBeat,run.tempoMap,run.sourceBpm)*run.tempoScale;
    transport.bpm.setValueAtTime(value,Tone.now());
    if(rawBeat<0)run.runPerformance=performance.now()+(-rawBeat)*60000/value;
    playheadDebug.setTempo(requested);
    return true;
  },
  async start(options, onFrame, onDone) {
    // Invalidate the current run before audio decoding. A Stop/Restart action
    // while ensure() is pending increments generation again, preventing this
    // asynchronous start from reviving a session the user already stopped.
    this.stop(); const generation = this.generation;
    try{
      await this.ensure();
      if(generation!==this.generation)return {cancelled:true,state:this.state};
      if(options.metronome)await this.waitForMetronome(options.timbre||'woodblock');
    }catch(error){
      if(generation!==this.generation)return {cancelled:true,state:this.state};
      throw error;
    }
    if(generation!==this.generation)return {cancelled:true,state:this.state};
    const transport = Tone.getTransport(), tempoMap=(options.tempoMap||[]).slice().sort((a,b)=>a.beat-b.beat),sourceBpm=options.sourceBpm||options.bpm,tempoScale=options.bpm/sourceBpm,offset = options.offsetBeat || 0,initialBpm=tempoAtBeat(offset,tempoMap,sourceBpm)*tempoScale,countBeats = Math.max(0,Number.isFinite(Number(options.countInBeats))?Number(options.countInBeats):Number(options.countInBars||0)*options.meter);
    const remaining = Math.max(0, options.totalBeats - offset), scoringTailBeats=Math.max(0,Number(options.scoringTailBeats)||0);
    const countInSeconds=countBeats*60/initialBpm,subdivision = Math.max(1, options.subdivision || 1), interval = `${4*subdivision}n`;
    const referenceNotes=(options.referenceNotes||[]).filter(note=>Number.isFinite(Number(note.beat))&&Number(note.beat)>=offset-.000001&&Number(note.beat)<options.totalBeats-.000001&&Number(note.velocity)>0);
    this.activeRun={tempoMap,sourceBpm,tempoScale,offset,countBeats,totalBeats:options.totalBeats,runPerformance:0,referenceNoteCount:referenceNotes.length,metronome:options.metronome,timbre:options.timbre};
    transport.bpm.value = initialBpm; transport.timeSignature = options.meter;
    for(const change of tempoMap){if(change.beat<=offset)continue;const position=transportPositionAtBeat(countBeats+change.beat-offset,options.meter);transport.scheduleOnce(time=>{if(generation===this.generation)this.activeRun&&transport.bpm.setValueAtTime(change.bpm*this.activeRun.tempoScale,time);},position);}
    for(const note of referenceNotes){const position=transportPositionAtBeat(countBeats+Number(note.beat)-offset,options.meter);transport.scheduleOnce(time=>{if(generation===this.generation)this.playDrum(note.instrument??'snare',note.articulation??'unspecified',Math.max(1,Math.min(127,Math.round(Number(note.velocity)||1))),time);},position);}
    const scheduleClick=(time,countIn=false)=>{
      if(generation!==this.generation)return;
      const transportBeat=transport.getTicksAtTime(time)/transport.PPQ;
      const beatStep=Math.round((countIn?transportBeat-countBeats:offset+transportBeat-countBeats)*subdivision);
      const beat=beatStep/subdivision;
      // The transport remains alive for the scoring late-delivery drain, but
      // that silent tail is not another musical bar and must not click.
      if(!countIn&&(beat<offset-.000001||beat>=options.totalBeats-.000001))return;
      visualMetronome.schedule(this.perfAt(time),beat,options.meter,subdivision,60000/transport.bpm.getValueAtTime(time));
      const barSteps=options.meter*subdivision;
      if(options.metronome)this.click(time,((beatStep%barSteps)+barSteps)%barSteps===0,options.timbre);
    };
    // Count-in uses its own grid. Practice resumes on the original exercise
    // grid, so a pause between beats never redefines the downbeat or shifts
    // subsequent clicks away from the written notes.
    const firstCountStep=Math.ceil(-countBeats*subdivision-.000001);
    for(let step=firstCountStep;step<0;step++){
      const position=transportPositionAtBeat(countBeats+step/subdivision,options.meter);
      transport.scheduleOnce(time=>scheduleClick(time,true),position);
    }
    const practiceStep=Math.ceil(offset*subdivision);
    const firstPracticeBeat=countBeats+practiceStep/subdivision-offset;
    transport.scheduleRepeat(time=>scheduleClick(time),interval,transportPositionAtBeat(firstPracticeBeat,options.meter));
    // Establish the scoring origin from the actual scheduled start. Tone
    // starts at Tone.now(), not raw.currentTime; schedule setup may itself
    // take several frames on a dense exercise.
    // A warm retry/resume already has audio buffered to the output. Count
    // that delay towards look-ahead, retaining at least one scheduler interval.
    const clock=Tone.getContext();
    const transportStart=options.immediate
      ?Math.max(this.raw.currentTime+clock.updateInterval,this.outputTime(performance.now())+clock.lookAhead)
      :Tone.now();
    const runAudio=transportStart+countInSeconds,runPerformance=this.perfAt(runAudio);
    this.activeRun.transportStart=transportStart;
    this.activeRun.runPerformance=runPerformance;
    transport.start(transportStart);
    const initialBeat=offset-countBeats;
    this.lastBeat=initialBeat;
    // Position the notation mirror at the run's opening beat before the first
    // animation frame. The synchronous initial Rust tick used to do this via
    // update_score; periodic ticks now deliver feedback only because they run
    // post-commit and must not reposition the playhead, so the bridge owns
    // this one deliberate pre-position (same for count-in: hidden).
    updateScore(scoreBeatForDisplay(Math.min(initialBeat,options.totalBeats-.000001),offset,options));
    onFrame(initialBeat,performance.now(),runPerformance);
    if(presentation.isActive())presentation.position(initialBeat,transport.bpm.value);
    let lastStateFrame=-Infinity;
    let lastAnimationFrame=performance.now();
    const frame = () => {
      if (generation !== this.generation) return;
      // Keep the score at the exact paused position during the short audio
      // scheduling lead-in. Without the clamp, resuming visibly jumps the
      // playhead backwards before the Tone transport starts.
      const now = performance.now(), elapsedBeats=elapsedTransportBeats(this.activeRun,now);
      const frameInterval=now-lastAnimationFrame;
      performanceProfile.record('animation_frame_interval_ms',frameInterval);lastAnimationFrame=now;
      // The HUD measures the PRODUCTION transport frame's own cadence (not
      // the reference overlay's loop), gated so it costs nothing unless a
      // measurement mode is active. This keeps the readout representative of
      // normal playback and usable with the overlay disabled.
      if(playheadHud.active()){playheadHud.sample(frameInterval);playheadHud.render(now);}
      const beat = offset + elapsedBeats-countBeats;
      const musicalBeat=Math.min(beat,options.totalBeats-.000001);
      this.lastBeat=Math.min(beat,options.totalBeats);
      const scoreBeat=scoreBeatForDisplay(musicalBeat,offset,options);
      const leadInBeat=scoreBeat>=0?nextDownbeatScoreBeat(musicalBeat,options):null;
      const bars=options.displayBars;
      if(scoreBeat>=0&&Array.isArray(bars)&&bars.length){
        const displayBar=Math.floor(scoreBeat/options.meter),previous=this.activeRun?.lastDisplayBar;
        if(Number.isInteger(previous)&&displayBar!==previous&&displayBar!==previous+1)scrollScoreToBeat(scoreBeat);
        if(this.activeRun)this.activeRun.lastDisplayBar=displayBar;
      }
      updateScore(scoreBeat,undefined,true,leadInBeat);
      if(presentation.isActive())presentation.position(musicalBeat,transport.bpm.value);
      visualMetronome.render(musicalBeat,options.meter,subdivision,now);
      // UI counters and scoring state do not need 60 updates per second. Keep
      // the playhead on every display frame, independent of the WASM work.
      // The scoring/UI tick is also a diagnostic switch: with it off, the
      // playhead and transport keep moving while the scoring/state work stops
      // (results are not saved for such a run), isolating its contribution to
      // frame pacing. Run completion is checked below, independent of this.
      if(diagnostics.stateTick&&(now-lastStateFrame>=32||beat>=options.totalBeats)){
        lastStateFrame=now;
        const tickBeat=beat,tickNow=now,tickStart=this.activeRun?.runPerformance??runPerformance;
        // iPadOS evidence: running the WASM scoring/UI tick inside this
        // animation-frame callback was the largest remaining cause of
        // playhead stutter, because its cost extended the same frame's
        // callback batch ahead of that frame's rendering steps. Dispatch it
        // as a separate macrotask instead, so the playhead writes are
        // followed directly by the rendering steps rather than by scoring
        // work; the tick then runs in the interval before the next frame,
        // where it can still contend with (but no longer sits inside) the
        // playhead commit's critical path. This is not a second clock - the
        // transport frame alone decides when scoring work is due, and
        // flushPendingTick keeps pause/stop/completion ordered with any
        // dispatched tick. Neither timeout completion nor rAF completion is
        // itself a presentation fence: the compositor presents
        // asynchronously.
        deferTick(()=>{ if(generation!==this.generation)return; const started=performance.now(); onFrame(tickBeat,tickNow,tickStart); recordTickDuration(performance.now()-started); });
      }
      if (elapsedBeats >= countBeats+remaining+scoringTailBeats) {
        // The final scoring tick must land before the run is torn down.
        flushPendingTick();
        this.stop();
        // With the scoring/UI tick disabled there is no final feedback tick to
        // hide the playhead at the end of the run; do it here so a diagnostic
        // run ends visually identical to a normal one.
        if(!diagnostics.stateTick)updateScore(-1);
        onDone();
      } else this.raf = requestAnimationFrame(frame);
    };
    this.raf = requestAnimationFrame(frame);
    playheadDebug.start({options,offset,tempoMap,sourceBpm,tempoScale,runPerformance});
    return { runPerformance, runAudio, state: this.state };
  },
  async calibration() {
    await this.ensure();await this.waitForMetronome('woodblock');const beatSeconds = .75, start = this.raw.currentTime + .3, targets = [];
    for (let i = 0; i < 8; i++) { const time = start + i * beatSeconds; this.click(time, i === 0, 'woodblock'); targets.push(this.perfAt(time)); }
    return targets;
  }
};

// Fetch and decode the acoustic layers as soon as the app loads. Creating and
// filling AudioBuffers is safe before the browser grants playback; Tone.start()
// remains inside ensure() so autoplay rules are still respected.
try { audio.preload(); }
catch(error) { audio.state='unavailable';audio.lastSnareError=audioErrorMessage(error); }

const SCORE_TOP_MARGIN=30,SCORE_ROW_HEIGHT=132,SCORE_BOTTOM_MARGIN=10;
let scoreData = { notes:[], svg:null, playhead:null, playheadOverlay:null, markerLayer:null, paintedGroups:new Set(), totalBeats:16, meter:4, container:null, bars:[], viewportWidth:0, viewportHeight:0, rowHeight:SCORE_ROW_HEIGHT, maxScroll:0, cameraY:0, preserveCameraBar:null, preserveViewportOnRewind:false, lastPlayheadRow:-1, lastPlayheadVisible:false, lastBeat:-Infinity, lastJudgements:[] };
let pendingScoreFeedback=null,pendingScoreFeedbackFrame=0,pendingScoreFeedbackTask=0;
let selectedBars=new Set();
let scoreSelectionCallback=null;
function applySelectedBarRange(){
  if(!scoreData.svg)return;
  for(const label of scoreData.svg.querySelectorAll('.score-measure-number[data-score-bar]')){
    const bar=Number(label.dataset.scoreBar);
    label.classList.toggle('selected-bar-number',selectedBars.has(bar));
  }
  for(const area of scoreData.svg.querySelectorAll('.score-bar-highlight')){
    const bar=Number(area.dataset.scoreBar),selected=selectedBars.has(bar);
    area.classList.toggle('selected',selected);area.setAttribute('aria-pressed',String(selected));
  }
}
function setSelectedBars(bars){selectedBars=new Set(Array.isArray(bars)?bars.map(Number):[]);applySelectedBarRange();}
function bindScoreSelection(callback){scoreSelectionCallback=callback;}
let scoreResizeObserver=null;
let scoreScrollContainer=null;
const handText = hand => hand === 'R' ? 'R' : hand === 'L' ? 'L' : '·';
const durationTable=[[4,'w',false,false],[3,'h',true,false],[2,'h',false,false],[1.5,'q',true,false],[1,'q',false,false],[.75,'8',true,false],[2/3,'q',false,true],[.5,'8',false,false],[.375,'16',true,false],[1/3,'8',false,true],[.25,'16',false,false],[1/6,'16',false,true],[.125,'32',false,false]];
const durationSpec = beats => durationTable.find(([value])=>Math.abs(value-beats)<.001)??durationTable.at(-1);
const writtenDuration = beats => {const [,duration,,tuplet]=durationSpec(beats);return tuplet?`${duration}t`:duration;};
function noteWithDuration(keys,beats,rest=false) {
  const [,duration,dotted,tuplet]=durationSpec(beats),note=new StaveNote({keys,duration:`${duration}${rest?'r':''}`,clef:'percussion',stemDirection:1});
  if(dotted){Dot.buildAndAttach([note],{all:!rest});note.applyTickMultiplier(3,2);}
  if(tuplet)note.rudimentTuplet=true;
  return note;
}
function restValues(gap) { const values=[];let left=gap;for(const [value] of durationTable)while(left>=value-.001){values.push(value);left-=value;}if(left>.001)values.push(.25);return values; }
function tupletsFor(slots) {
  const groups=[];let run=[];
  const flush=()=>{for(let index=0;index+2<run.length;index+=3)groups.push(run.slice(index,index+3));run=[];};
  for(const slot of slots){if(slot.rudimentTuplet)run.push(slot);else flush();}
  flush();
  return groups.map(notes=>new Tuplet(notes,{numNotes:3,notesOccupied:2,bracketed:true}));
}
function pitchForHit(hit) {
  const note=Number(hit.note),byNote={
    35:'f/4',36:'f/4',37:'c/5/x',38:'c/5',39:'a/5/x',40:'c/5',
    41:'g/4',42:'g/5/x',43:'a/4',44:'g/5/x',45:'b/4',46:'g/5/x',
    47:'d/5',48:'e/5',49:'a/5/x',50:'f/5',51:'d/5/x',52:'a/5/x',
    53:'f/5/D2',54:'g/5/x',55:'a/5/x',56:'c/5',57:'a/5/x',
    58:'c/5/x',59:'d/5/x',60:'d/5',61:'b/4',62:'c/5/x',63:'c/5',
    64:'g/4',65:'d/5',66:'a/4',67:'f/5',68:'c/5',69:'g/5/x',
    70:'g/5/x',71:'a/5/x',72:'f/5/x',73:'c/5/x',74:'e/5/x',
    75:'d/5',76:'f/5',77:'b/4',78:'g/4/x',79:'c/5/x',
    80:'a/5/x',81:'a/5/x'
  };
  if(Number.isInteger(note)&&byNote[note])return byNote[note];
  return ({kick:'f/4',snare:'c/5',closed_hat:'g/5/x',open_hat:'g/5/x',pedal_hat:'g/5/x',ride:'d/5/x',crash:'a/5/x',tom:'c/5',other_percussion:'c/5/x'})[hit.instrument]??'c/5';
}
function instrumentForHit(hit) {
  // Rust classifies MIDI notes; engraving must not silently reinterpret them.
  if(!hit.instrument)throw new Error('Notation requires a classified instrument');
  return hit.instrument;
}
function articulationForHit(hit) {
  return hit.articulation??'unspecified';
}
function renderScore(exercise, minimumRowHeightOverride=0, layoutPass=0, focusBarOverride=null) {
  const profileStart=performanceProfile.enabled&&layoutPass===0?performance.now():0;
  const element = document.getElementById('notation'); if (!element) return;
  const isResize=scoreData.exercise===exercise;
  const active=document.activeElement;
  const focusBar=focusBarOverride??(isResize&&element.contains(active)&&active?.classList.contains('score-bar-highlight')?Number(active.dataset.scoreBar):null);
  element.innerHTML = '';
  const previousBeat=isResize&&Number.isFinite(scoreData.lastBeat)?scoreData.lastBeat:-99;
  const previousJudgements=isResize?scoreData.lastJudgements:[];
  if(!isResize){element.scrollTop=0;element.scrollLeft=0;}
  const hits=exercise.hits,metersPerBar=exercise.meter_numerator*4/Math.max(1,exercise.meter_denominator);
  const lastBeat=hits.reduce((last,hit)=>Math.max(last,hit.notation_beat??hit.beat),0);
  const barCount=Math.max(1,Math.floor(lastBeat/metersPerBar)+1);
  const onsetGroupsByBar=Array.from({length:barCount},()=>new Map()),onsetBuckets=new Map(),graceHits=[];
  for(const hit of hits){
    const position=hit.notation_beat??hit.beat;
    if(!Number.isFinite(position))continue;
    if(hit.grace){graceHits.push({hit,position});continue;}
    const bar=Math.floor((position+.001)/metersPerBar);
    if(bar<0||bar>=barCount)continue;
    const key=position.toFixed(4),groups=onsetGroupsByBar[bar];
    let group=groups.get(key);
    if(!group){
      group={position,hits:[],graceHits:[]};groups.set(key,group);
      const bucket=Math.round(position*1000),bucketGroups=onsetBuckets.get(bucket)??[];
      bucketGroups.push(group);onsetBuckets.set(bucket,bucketGroups);
    }
    group.hits.push(hit);
  }
  for(const {hit,position} of graceHits){
    const bucket=Math.round(position*1000),candidates=[];
    for(let offset=-1;offset<=1;offset++)candidates.push(...(onsetBuckets.get(bucket+offset)??[]));
    const group=candidates
      .filter(candidate=>Math.abs(candidate.position-position)<.001)
      .sort((left,right)=>Math.abs(left.position-position)-Math.abs(right.position-position))[0];
    group?.graceHits.push(hit);
  }
  const orderedOnsetsByBar=onsetGroupsByBar.map(groups=>[...groups.values()].sort((left,right)=>left.position-right.position));
  const singleLine=exercise.staff_mode==='single_line'&&hits.length>0&&hits.every(hit=>instrumentForHit(hit)==='snare');
  const width=Math.max(280,element.clientWidth),availableWidth=Math.max(240,width-28);
  const compactLandscape=window.innerHeight<=600&&window.innerWidth>window.innerHeight;
  const scoreTopMargin=compactLandscape?25:SCORE_TOP_MARGIN,scoreBottomMargin=compactLandscape?7:SCORE_BOTTOM_MARGIN;
  const renderer=new Renderer(element,Renderer.Backends.SVG);
  const notesMeta=[],firstNoteByRow=[],barLayouts=[],staves=[],barSlots=new Map(),barSlotBeats=new Map(),beams=[],tuplets=[],eventHitsByNote=new Map();
  const firstStave=new Stave(14,scoreTopMargin,availableWidth);
  firstStave.setConfigForLines(Array.from({length:5},(_,index)=>({visible:!singleLine||index===2})));
  firstStave.setBegBarType(BarlineType.NONE).addClef('percussion').addTimeSignature(`${exercise.meter_numerator}/${exercise.meter_denominator}`);
  const headerWidth=firstStave.getNoteStartX()-14;
  const musicWidth=Math.max(120,availableWidth-headerWidth);
  const barsPerLine=1;
  const barWidth=musicWidth/barsPerLine,lineCount=Math.ceil(barCount/barsPerLine);
  const viewportHeight=Math.max(0,element.clientHeight);
  const availableSystemHeight=Math.max(1,viewportHeight-scoreTopMargin-scoreBottomMargin);
  const minimumRowHeight=Math.max(compactLandscape?84:94,minimumRowHeightOverride);
  const targetSystems=Math.max(1,Math.min(lineCount,Math.floor(availableSystemHeight/minimumRowHeight)||1));
  const fittedRowHeight=availableSystemHeight/targetSystems;
  const scoreRowHeight=Math.max(minimumRowHeight,Math.min(compactLandscape?118:SCORE_ROW_HEIGHT,fittedRowHeight||SCORE_ROW_HEIGHT));
  const height=scoreTopMargin+lineCount*scoreRowHeight+scoreBottomMargin;
  renderer.resize(width,height);
  const context=renderer.getContext(),systemBarCounts=[],systemWidths=[];
  const beamGroups=exercise.meter_numerator>=6&&exercise.meter_numerator%3===0
    ?[new Fraction(3,Math.max(1,exercise.meter_denominator))]
    :[new Fraction(1,Math.max(1,exercise.meter_denominator))];
  for(let row=0;row<lineCount;row++){
    const rowBarCount=Math.min(barsPerLine,barCount-row*barsPerLine),staveWidth=headerWidth+rowBarCount*barWidth;
    const stave=row===0?firstStave:new Stave(14,scoreTopMargin+row*scoreRowHeight,staveWidth);
    if(row>0){
      stave.setConfigForLines(Array.from({length:5},(_,index)=>({visible:!singleLine||index===2})));
      stave.setBegBarType(BarlineType.NONE).addClef('percussion').addTimeSignature(`${exercise.meter_numerator}/${exercise.meter_denominator}`);
    }
    if(row===lineCount-1)stave.setEndBarType(BarlineType.END);
    stave.setWidth(staveWidth).setContext(context).draw();staves.push(stave);
    systemBarCounts.push(rowBarCount);systemWidths.push(staveWidth);
  }
  for(let bar=0;bar<barCount;bar++){
    const row=Math.floor(bar/barsPerLine),stave=staves[row];
    const firstLine=singleLine?2:0,lastLine=singleLine?2:4;
    const xStart=stave.getNoteStartX()+(bar%barsPerLine)*barWidth;
    barLayouts.push({index:bar,startBeat:bar*metersPerBar,endBeat:(bar+1)*metersPerBar,row,xStart,xEnd:xStart+barWidth,y:stave.getY(),y1:stave.getYForLine(firstLine)-12,y2:stave.getYForLine(lastLine)+12});
  }
  for(let row=0;row<lineCount;row++){
    const firstBar=row*barsPerLine,lastBar=Math.min(barCount,firstBar+barsPerLine),rowTickables=[];
    for(let bar=firstBar;bar<lastBar;bar++){
      const start=bar*metersPerBar,end=start+metersPerBar,slots=[];
      const ordered=orderedOnsetsByBar[bar];let cursor=start;
      for(let index=0;index<ordered.length;index++){
        const group=ordered[index],next=ordered[index+1]?.position??end;
        if(group.position>cursor+.001){
          for(const value of restValues(group.position-cursor)){
            const rest=noteWithDuration(['b/4'],value,true);barSlotBeats.set(rest,cursor);slots.push(rest);cursor+=value;
          }
        }
        const keys=[...new Set(group.hits.map(pitchForHit))];
        const requestedDuration=Math.min(...group.hits.map(hit=>hit.notation_duration_beats||.25));
        const value=Math.min(requestedDuration,next-group.position,end-group.position);
        const note=noteWithDuration(keys,value);
        eventHitsByNote.set(note,group.hits);
        const hands=[...new Set(group.hits.map(hit=>hit.hand).filter(hand=>hand==='R'||hand==='L'))];
        // Keep sticking below the staff so accents cannot displace individual hand labels.
        if(hands.length===1)note.addModifier(new Annotation(handText(hands[0])).setVerticalJustification(STICKING_PLACEMENT).setFont('Inter',11,'700'),0);
        // Tag the accent glyph so theme CSS can colour it without per-frame work;
        // VexFlow renders articulations as plain text inside the notehead group.
        if(group.hits.some(hit=>hit.accent)){note.rudimentAccent=new Articulation('a>').setPosition(3);note.addModifier(note.rudimentAccent,0);}
        barSlotBeats.set(note,group.position);slots.push(note);
        for(const hit of group.hits){const key=pitchForHit(hit),meta={note,beat:hit.beat,notationBeat:group.position,durationBeats:value,hit,key,headIndex:Math.max(0,keys.indexOf(key)),row,bar};notesMeta.push(meta);firstNoteByRow[row]??=meta;}
        const ornamentHits=group.graceHits;
        if(ornamentHits.length){
          const graceNotes=ornamentHits.map(hit=>{
            const key=pitchForHit(hit),graceNote=new GraceNote({keys:[key],duration:'16',clef:'percussion',stemDirection:1,slash:true});
            if(hit.hand==='R'||hit.hand==='L')graceNote.addModifier(new Annotation(handText(hit.hand)).setVerticalJustification(STICKING_PLACEMENT).setFont('Inter',8,'600'),0);
            eventHitsByNote.set(graceNote,[hit]);
            const meta={note:graceNote,beat:hit.beat,notationBeat:group.position,durationBeats:.125,hit,key,headIndex:0,row,bar};notesMeta.push(meta);firstNoteByRow[row]??=meta;
            return graceNote;
          });
          const graceGroup=new GraceNoteGroup(graceNotes,false);
          if(graceNotes.length>1)graceGroup.beamNotes();
          note.addModifier(graceGroup,0);
        }
        cursor=group.position+value;
      }
      if(cursor<end-.001)for(const value of restValues(end-cursor)){
        const rest=noteWithDuration(['b/4'],value,true);barSlotBeats.set(rest,cursor);slots.push(rest);cursor+=value;
      }
      barSlots.set(bar,slots);rowTickables.push(...slots);
      tuplets.push(...tupletsFor(slots));
      beams.push(...Beam.generateBeams(slots,{groups:beamGroups,stemDirection:1}));
    }
    const voice=new Voice({numBeats:(lastBar-firstBar)*metersPerBar,beatValue:4}).setStrict(false);
    voice.addTickables(rowTickables);
    new Formatter().joinVoices([voice]).format([voice],staves[row].getNoteEndX()-staves[row].getNoteStartX());
    const firstSlot=barSlots.get(firstBar)?.[0],firstContext=firstSlot?.getTickContext();
    const rowStartRelativeX=firstSlot&&firstContext?firstSlot.getX()+firstSlot.getGlyphWidth()/2:0;
    const rowStartX=staves[row].getNoteStartX()+rowStartRelativeX;
    // The row's final bar must end at its drawn barline. Extrapolating a full
    // barWidth from the first engraved beat overshoots the system: the bar's
    // linear beat map starts at the first notehead centre (inset from the
    // staff origin by the formatter's left padding), so its end would land in
    // the blank margin right of the final barline. Compress that bar's beat
    // span onto the actual staff width and keep the engraved noteheads on the
    // same map, so the playhead and the notes stay aligned and the playhead
    // never leaves the system.
    const barlineX=staves[row].getX()+staves[row].getWidth();
    // VexFlow draws a note at tickContext.x + stave note start + Stave.padding,
    // so the padded origin must be removed here for the notehead centres to
    // land on the linear beat map anchored at the staff origin. Any residual
    // glyph-metrics offset is absorbed by the post-draw correction below.
    const stavePadding=Metrics.get('Stave.padding',0);
    for(let bar=firstBar;bar<lastBar;bar++){
      const layout=barLayouts[bar],xStart=rowStartX+(bar-firstBar)*barWidth;
      const beatSpan=bar===lastBar-1?Math.max(1,barlineX-xStart):barWidth;
      layout.beatWidth=beatSpan;layout.xStart=xStart;layout.xEnd=xStart+beatSpan;
      const pixelsPerBeat=beatSpan/metersPerBar;
      for(const slot of barSlots.get(bar)??[]){
        const tickContext=slot.getTickContext(),xShift=slot.getX()-tickContext.getX();
        const targetCenter=rowStartRelativeX+(bar-firstBar)*barWidth+(barSlotBeats.get(slot)-layout.startBeat)*pixelsPerBeat;
        tickContext.setX(targetCenter-xShift-slot.getGlyphWidth()/2-stavePadding);
      }
    }
    voice.draw(context,staves[row]);
    for(const tickable of rowTickables)if(tickable.isRest?.())tickable.getSVGElement?.()?.setAttribute('data-score-rest','true');
  }
  for(const beam of beams)beam.setContext(context).draw();
  for(const tuplet of tuplets)tuplet.setContext(context).draw();
  const svg=element.querySelector('svg');
  for(let row=0;row<lineCount;row++){
    const firstNote=firstNoteByRow[row],head=firstNote?.note.getSVGElement?.()?.querySelector('.vf-notehead > text');
    const bounds=head?.getBBox();
    if(!firstNote||!bounds)continue;
    const layout=barLayouts[firstNote.bar],target=layout.xStart+(firstNote.notationBeat-layout.startBeat)*layout.beatWidth/metersPerBar;
    const correction=bounds.x+bounds.width/2-target;
    for(let bar=row*barsPerLine;bar<Math.min(barCount,(row+1)*barsPerLine);bar++){
      barLayouts[bar].xStart+=correction;barLayouts[bar].xEnd+=correction;
    }
  }
  for(let bar=0;bar<barCount-1;bar++){
    if(barLayouts[bar].row!==barLayouts[bar+1].row)continue;
    const stave=staves[barLayouts[bar].row],x=barLayouts[bar].xEnd-Math.min(12,barWidth*.06);
    const barline=document.createElementNS('http://www.w3.org/2000/svg','line');
    barline.setAttribute('class','score-barline');barline.setAttribute('x1',String(x));barline.setAttribute('x2',String(x));
    barline.setAttribute('y1',String(singleLine?stave.getYForLine(2)-9:stave.getYForLine(0)));
    barline.setAttribute('y2',String(singleLine?stave.getYForLine(2)+9:stave.getYForLine(4)));
    barline.setAttribute('stroke-width','1.2');svg?.appendChild(barline);
  }
  for(let bar=0;bar<barCount;bar++){
    const stave=staves[barLayouts[bar].row],label=document.createElementNS('http://www.w3.org/2000/svg','text');
    const x=bar===0||bar%barsPerLine===0?stave.getX()+6:barLayouts[bar].xStart-8;
    label.setAttribute('class','score-measure-number');label.setAttribute('x',String(x));
    label.dataset.scoreBar=String(bar);
    label.setAttribute('y',String(stave.getYForLine(0)-12));label.setAttribute('text-anchor',bar===0||bar%barsPerLine===0?'start':'end');
    label.setAttribute('font-family','Inter, sans-serif');label.setAttribute('font-size','10');label.setAttribute('font-weight','600');
    label.textContent=String(bar+1);svg?.appendChild(label);
  }
  if (svg) {
    svg.setAttribute('role','group');svg.setAttribute('aria-label',`${exercise.title} percussion notation with sticking`);
    const highlights=document.createElementNS('http://www.w3.org/2000/svg','g');
    for(const bar of barLayouts){
      const area=document.createElementNS('http://www.w3.org/2000/svg','rect');
      area.setAttribute('class','score-bar-highlight');area.dataset.scoreBar=String(bar.index);
      const left=bar.index%barsPerLine===0?14:Math.max(14,bar.xStart-12),right=Math.min(width-14,bar.xEnd);
      area.setAttribute('x',String(left));area.setAttribute('y',String(scoreTopMargin+bar.row*scoreRowHeight-12));
      area.setAttribute('width',String(Math.max(0,right-left)));area.setAttribute('height',String(scoreRowHeight-4));
      area.setAttribute('rx','6');area.setAttribute('role','button');area.setAttribute('tabindex',bar.index===(focusBar??0)?'0':'-1');area.setAttribute('aria-label',`Select bar ${bar.index+1}`);
      area.addEventListener('keydown',event=>{
        if(event.key==='Enter'||event.key===' '){event.preventDefault();event.stopPropagation();scoreSelectionCallback?.(bar.index,event.shiftKey);}
        else if(['ArrowLeft','ArrowUp','ArrowRight','ArrowDown'].includes(event.key)){
          event.preventDefault();event.stopPropagation();
          const direction=['ArrowLeft','ArrowUp'].includes(event.key)?-1:1;
          const next=highlights.children[Math.max(0,Math.min(barLayouts.length-1,bar.index+direction))];
          if(next){area.setAttribute('tabindex','-1');next.setAttribute('tabindex','0');next.focus();}
        }
      });
      highlights.appendChild(area);
    }
    svg.insertBefore(highlights,svg.firstChild);
    svg.addEventListener('click',event=>{
      const point=svg.createSVGPoint(),matrix=svg.getScreenCTM();if(!matrix)return;
      point.x=event.clientX;point.y=event.clientY;const local=point.matrixTransform(matrix.inverse());
      const row=Math.floor((local.y-(scoreTopMargin-12))/scoreRowHeight);
      const bar=barLayouts.find(item=>item.row===row&&local.x>=(item.index%barsPerLine===0?14:Math.max(14,item.xStart-12))&&local.x<=Math.min(width-14,item.xEnd));
      if(bar)scoreSelectionCallback?.(bar.index,event.shiftKey);
    });
  }
  if(svg){
    svg.dataset.barsPerLine=String(barsPerLine);svg.dataset.staffLines=singleLine?'1':'5';
    svg.dataset.systemBarCounts=JSON.stringify(systemBarCounts);svg.dataset.systemWidths=JSON.stringify(systemWidths);
    svg.dataset.staffBounds=JSON.stringify(staves.map(stave=>({top:stave.getYForLine(singleLine?2:0),bottom:stave.getYForLine(singleLine?2:4)})));
    svg.dataset.systemRowHeight=String(scoreRowHeight);svg.dataset.systemCount=String(lineCount);
  }
  for (const meta of notesMeta) {
    const group=meta.note.getSVGElement?.();
    if(group){
      const eventHits=eventHitsByNote.get(meta.note)??[meta.hit];
      group.dataset.beat=String(meta.beat);group.dataset.writtenDuration=writtenDuration(meta.durationBeats);group.dataset.systemRow=String(meta.row);group.dataset.scoreBar=String(meta.bar);
      group.dataset.dotted=String(durationSpec(meta.durationBeats)[2]);
      group.dataset.tuplet=String(durationSpec(meta.durationBeats)[3]);
      group.dataset.grace=String(Boolean(meta.hit.grace));
      group.dataset.accent=String(Boolean(meta.hit.accent));
      const accent=meta.note.rudimentAccent;
      if(accent)for(const text of group.querySelectorAll('.vf-notehead > text')){if(text.textContent===accent.text){text.classList.add('score-accent');break;}}
      group.dataset.midiNotes=eventHits.map(candidate=>candidate.note??38).join(',');
      group.dataset.instruments=[...new Set(eventHits.map(instrumentForHit))].join(',');
      group.dataset.articulations=[...new Set(eventHits.map(articulationForHit))].join(',');
      group.classList.add('score-note');
      for(const [index,hit] of eventHits.entries()){
        if(instrumentForHit(hit)!=='open_hat'||group.querySelector(`.open-hat-articulation[data-hit-index="${index}"]`))continue;
        const key=pitchForHit(hit),headIndex=[...new Set(eventHits.map(pitchForHit))].indexOf(key);
        const head=group.querySelectorAll('.vf-notehead > text')[Math.max(0,headIndex)];
        const bounds=head?.getBBox();if(!bounds)continue;
        const open=document.createElementNS('http://www.w3.org/2000/svg','circle');
        open.setAttribute('class','open-hat-articulation');open.dataset.hitIndex=String(index);
        open.setAttribute('cx',String(bounds.x+bounds.width/2));open.setAttribute('cy',String(bounds.y-5));
        open.setAttribute('r','3.4');open.setAttribute('fill','none');open.setAttribute('stroke','currentColor');open.setAttribute('stroke-width','1.3');
        group.appendChild(open);
      }
    }
  }
  // Cache feedback DOM refs and notehead geometry so painting never queries
  // the score tree or forces SVG layout during hits.
  for(const meta of notesMeta){
    const group=meta.note.getSVGElement?.();
    if(group){
      const noteheads=group.querySelectorAll('.vf-notehead');
      const notehead=noteheads[meta.headIndex??0]??noteheads[0];
      meta.group=group;meta.noteheads=noteheads;meta.notehead=notehead;
      meta.headText=notehead?.querySelector(':scope > text')??null;
    }
    const head=meta.headText;
    if(!head)continue;
    try{
      const bounds=head.getBBox();
      meta.headBounds={cx:bounds.x+bounds.width/2,cy:bounds.y+bounds.height/2,r:Math.max(6,bounds.width*.84)};
      const matrix=head.getCTM();
      meta.headCenterY=matrix?matrix.b*(bounds.x+bounds.width/2)+matrix.d*(bounds.y+bounds.height/2)+matrix.f:(bounds.y+bounds.height/2);
    }catch{}
  }
  if(svg&&lineCount>1){
    const textBaseline=text=>{
      const matrix=text.getCTM(),x=Number(text.getAttribute('x'))||0,y=Number(text.getAttribute('y'))||0;
      return matrix?matrix.b*x+matrix.d*y+matrix.f:y;
    };
    // Index the SVG once. Querying the complete score separately for every
    // system made this clearance check quadratic on long exercises (625 full
    // DOM scans for the 10,000-note benchmark).
    const stickingBaselines=Array.from({length:lineCount},()=>[]),accentBaselines=Array.from({length:lineCount},()=>[]);
    for(const text of svg.querySelectorAll('.score-note .vf-annotation > text')){
      const row=Number(text.closest('.score-note')?.dataset.systemRow);
      if(Number.isInteger(row)&&stickingBaselines[row])stickingBaselines[row].push(textBaseline(text));
    }
    for(const note of svg.querySelectorAll('.score-note[data-accent="true"]')){
      const row=Number(note.dataset.systemRow);if(!Number.isInteger(row)||!accentBaselines[row])continue;
      for(const text of [...note.querySelectorAll(':scope > .vf-notehead > text')].slice(1))accentBaselines[row].push(textBaseline(text));
    }
    const clearances=stickingBaselines.slice(0,-1).map((baselines,row)=>baselines.length&&accentBaselines[row+1].length?Math.min(...accentBaselines[row+1])-Math.max(...baselines):null);
    svg.dataset.systemMarkClearances=JSON.stringify(clearances);
    const shortfall=Math.max(0,...clearances.filter(Number.isFinite).map(clearance=>SYSTEM_MARK_BASELINE_CLEARANCE-clearance));
    if(shortfall>.5&&layoutPass<2){
      renderScore(exercise,Math.ceil(scoreRowHeight+shortfall),layoutPass+1,focusBar);
      if(layoutPass===0)performanceProfile.record('notation_render_ms',performance.now()-profileStart);
      return;
    }
  }
  const markers=document.createElementNS('http://www.w3.org/2000/svg','g');markers.id='unmatched-markers';markers.setAttribute('role','group');markers.setAttribute('aria-label','Unmatched physical strikes');svg?.appendChild(markers);
  const line = document.createElementNS('http://www.w3.org/2000/svg','line'); line.id='playhead'; line.setAttribute('x1','0');line.setAttribute('x2','0');line.setAttribute('class','playhead-line'); svg?.appendChild(line);
  // The visible playhead is a composited HTML overlay above the engraved
  // score: mutating the SVG line's geometry every frame invalidated and
  // repainted the whole notation SVG. The hidden line above keeps the
  // historical #playhead attributes/classes for tests and diagnostics.
  const overlay=document.createElement('div'); overlay.className='playhead-overlay'; overlay.setAttribute('aria-hidden','true'); element.appendChild(overlay);
  // Subtle lead-in marker: during the final beat of a system it previews where
  // the playhead will appear in the next system, so the system transition is
  // visible before the next downbeat without moving the active playhead early.
  const preview=document.createElement('div'); preview.className='playhead-preview'; preview.setAttribute('aria-hidden','true'); element.appendChild(preview);
  const noteByExpectedId=new Map(),noteByBeatInstrument=new Map(),notesByRow=Array.from({length:lineCount},()=>[]);
  for(const meta of notesMeta){
    if(meta.hit?.id)noteByExpectedId.set(meta.hit.id,meta);
    noteByBeatInstrument.set(`${Number(meta.beat).toFixed(6)}|${instrumentForHit(meta.hit)}`,meta);
    notesByRow[meta.row]?.push(meta);
  }
  scoreData={notes:notesMeta,notesByRow,noteByExpectedId,noteByBeatInstrument,headYCache:new WeakMap(),appliedExpectedJudgements:new Map(),markerLayer:markers,paintedGroups:new Set(),svg,playhead:line,playheadOverlay:overlay,playheadPreview:preview,totalBeats:barCount*metersPerBar,meter:metersPerBar,container:element,bars:barLayouts,viewportWidth:width,viewportHeight,rowHeight:scoreRowHeight,maxScroll:Math.max(0,height-viewportHeight),cameraY:element.scrollTop,preserveCameraBar:null,preserveViewportOnRewind:false,lastPlayheadRow:-1,lastPlayheadVisible:false,exercise,lastBeat:-Infinity,lastJudgements:[]};
  playheadDebug.attachContainer(element);
  if(scoreScrollContainer!==element){
    scoreScrollContainer?.removeEventListener('scroll',onScoreScroll);
    element.addEventListener('scroll',onScoreScroll,{passive:true});
    scoreScrollContainer=element;
  }
  applySelectedBarRange();
  applyJudgements(previousJudgements);updateScore(previousBeat,null);
  if(focusBar!==null)svg?.querySelector(`.score-bar-highlight[data-score-bar="${focusBar}"]`)?.focus({preventScroll:true});
  if(typeof ResizeObserver!=='undefined'){
    scoreResizeObserver??=new ResizeObserver(entries=>{
      const rect=entries[0]?.contentRect,observedWidth=rect?.width??scoreData.container?.clientWidth??0,observedHeight=rect?.height??scoreData.container?.clientHeight??0;
      if(scoreData.exercise&&(Math.abs(observedWidth-scoreData.viewportWidth)>1||Math.abs(observedHeight-scoreData.viewportHeight)>2))renderScore(scoreData.exercise);
    });
    scoreResizeObserver.observe(element);
  }
  if(layoutPass===0)performanceProfile.record('notation_render_ms',performance.now()-profileStart);
}
function positionAtBeat(beat) {
  const bars=scoreData.bars;if(!bars.length)return {x:0,y:0,row:0,bar:0};
  const value=Math.max(0,Math.min(scoreData.totalBeats,beat));
  const index=Math.min(bars.length-1,Math.floor(value/scoreData.meter));
  const bar=bars[index];
  const ratio=Math.max(0,Math.min(1,(value-bar.startBeat)/Math.max(.001,bar.endBeat-bar.startBeat)));
  return {x:bar.xStart+(bar.xEnd-bar.xStart)*ratio,y:bar.y,y1:bar.y1,y2:bar.y2,row:bar.row,bar:index};
}
// Temporary opt-in comparison clock for diagnosing notation playhead motion.
// It deliberately does not feed playback, scoring, feedback, or scrolling.
const playheadDebug={
  get enabled(){return diagnostics.referenceOverlay},container:null,overlay:null,raf:0,run:null,runId:0,startedAt:performance.now(),frames:[],lastFrame:null,lastRendered:null,lastPaintRow:null,lastDatasetRow:null,lastDatasetWrite:0,
  attachContainer(container){
    // Attach unconditionally: the overlay is invisible unless a run is
    // active, and the diagnostic can be enabled at any time from Settings.
    if(!container)return;
    this.container=container;
    if(!container.style.position)container.style.position='relative';
    if(!this.overlay||!this.overlay.isConnected){
      this.overlay=document.createElement('div');
      this.overlay.className='playhead-reference-overlay';
      this.overlay.setAttribute('aria-hidden','true');
    }
    if(this.overlay.parentNode!==container)container.appendChild(this.overlay);
    if(this.lastRendered)this.paint(this.lastRendered);
  },
  start({options,offset,tempoMap,sourceBpm,tempoScale,runPerformance}){
    this.pause();
    const countBeats=Math.max(0,Number(options.countInBeats)||0);
    // Always remember the run so Settings can enable the reference overlay
    // mid-practice; only sample frames while the diagnostic is on.
    this.run={options,visibleOffset:offset,initialBeat:offset-countBeats,baseBeat:offset,basePerformance:runPerformance,tempoMap,sourceBpm,tempoScale};
    this.runId++;this.lastFrame=null;
    // Fresh measurement windows per run so HUD readings describe this run.
    playheadHud.reset();
    this.lastPaintRow=null;this.lastDatasetRow=null;this.lastDatasetWrite=0;
    if(!this.enabled)return;
    this.raf=requestAnimationFrame(now=>this.frame(now));
  },
  pause(){
    cancelAnimationFrame(this.raf);this.raf=0;this.run=null;this.lastFrame=null;
  },
  hide(){
    this.lastRendered=null;
    this.overlay?.classList.remove('visible');
  },
  setTempo(bpm){
    if(!this.run)return;
    const now=performance.now(),beat=this.beatAt(now);
    this.run.baseBeat=beat;this.run.basePerformance=now;this.run.tempoScale=bpm/this.run.sourceBpm;
  },
  beatAt(now){
    const run=this.run;if(!run)return Number.NaN;
    let remaining=now-run.basePerformance,beat=run.baseBeat;
    let bpm=tempoAtBeat(beat,run.tempoMap,run.sourceBpm)*run.tempoScale;
    if(remaining<=0)return Math.max(run.initialBeat,beat+remaining*bpm/60000);
    for(const change of run.tempoMap){
      if(change.beat<=beat)continue;
      const duration=(change.beat-beat)*60000/bpm;
      if(remaining<=duration)return beat+remaining*bpm/60000;
      remaining-=duration;beat=change.beat;bpm=change.bpm*run.tempoScale;
    }
    return beat+remaining*bpm/60000;
  },
  displayBeat(beat){
    const run=this.run,options=run.options,musicalBeat=Math.min(beat,options.totalBeats-.000001);
    const rangeStart=Number(options.displayRangeStart)||0,rangeBeats=Math.max(.001,Number(options.displayRangeBeats)||options.exerciseBeats);
    const rangePosition=((musicalBeat-rangeStart)%rangeBeats+rangeBeats)%rangeBeats;
    const bars=options.displayBars,barIndex=Math.floor(rangePosition/options.meter);
    return musicalBeat<run.visibleOffset?-1:Array.isArray(bars)&&bars.length?Number(bars[Math.min(barIndex,bars.length-1)])*options.meter+rangePosition-barIndex*options.meter:rangeStart+rangePosition;
  },
  paint(rendered){
    const overlay=this.overlay;if(!overlay)return;
    const {beat,position,visible,now}=rendered;
    // Same compositor discipline as the production playhead: geometry only at
    // row boundaries (WebKit freezes or re-rasters composited layers whose
    // geometry mutates every frame), transform per frame, and the diagnostic
    // data attributes at a low cadence - the reference must not disturb the
    // playback it measures. The in-memory trace keeps full frame resolution;
    // only the DOM mirror is throttled.
    if(position.row!==this.lastPaintRow){
      overlay.style.top=`${position.y1}px`;
      overlay.style.height=`${Math.max(1,position.y2-position.y1)}px`;
      this.lastPaintRow=position.row;
    }
    overlay.style.transform=`translate3d(${position.x}px,0,0)`;
    overlay.classList.toggle('visible',visible);
    if(position.row!==this.lastDatasetRow||now-this.lastDatasetWrite>500){
      this.lastDatasetRow=position.row;this.lastDatasetWrite=now;
      overlay.dataset.x=String(position.x);overlay.dataset.beat=String(beat);overlay.dataset.row=String(position.row);overlay.dataset.bar=String(position.bar);
    }
  },
  render(now){
    if(!this.run)return;
    const beat=this.displayBeat(this.beatAt(now)),position=positionAtBeat(beat);
    const rendered={beat,position,visible:beat>=0&&beat<=scoreData.totalBeats,now};
    this.lastRendered=rendered;this.paint(rendered);
    const green=scoreData.playhead,greenX=green?.classList.contains('visible')?Number(green.dataset.x):Number.NaN;
    const previous=this.lastFrame,blueX=rendered.visible?position.x:Number.NaN;
    const sample={
      run:this.runId,timestamp_ms:now,elapsed_ms:now-this.startedAt,frame_interval_ms:previous?now-previous.timestamp_ms:Number.NaN,
      green_x:greenX,blue_x:blueX,green_increment:previous&&Number.isFinite(greenX)&&Number.isFinite(previous.green_x)?greenX-previous.green_x:Number.NaN,
      blue_increment:previous&&Number.isFinite(blueX)&&Number.isFinite(previous.blue_x)?blueX-previous.blue_x:Number.NaN,
      position_error:Number.isFinite(greenX)&&Number.isFinite(blueX)?greenX-blueX:Number.NaN,
      green_row:green?.classList.contains('visible')?Number(green.dataset.row):Number.NaN,blue_row:rendered.visible?position.row:Number.NaN
    };
    // Bounded trace: an unbounded per-frame array grows for the whole session
    // and its GC pauses perturb the very pacing being measured.
    this.frames.push(sample);this.lastFrame=sample;
    if(this.frames.length>4096)this.frames.splice(0,1024);
  },
  frame(now){
    if(!this.run)return;
    this.render(now);
    this.raf=requestAnimationFrame(next=>this.frame(next));
  },
  reset(){this.frames.length=0;this.startedAt=performance.now();this.lastFrame=null;},
  snapshot(){return {enabled:this.enabled,running:Boolean(this.run),frames:this.frames.map(frame=>({...frame}))};},
  csv(){
    const columns=['run','timestamp_ms','elapsed_ms','frame_interval_ms','green_x','blue_x','green_increment','blue_increment','position_error','green_row','blue_row'];
    const value=item=>Number.isFinite(item)?String(item):'';
    return `${columns.join(',')}\n${this.frames.map(frame=>columns.map(column=>value(frame[column])).join(',')).join('\n')}\n`;
  },
  export(){
    if(!this.enabled)return false;
    downloadText(this.csv(),'rudiment-playhead-debug.csv','text/csv');return true;
  }
};
if(diagnostics.referenceOverlay)console.info('[Rudiment playhead debug] Blue reference enabled. Export with rudiment.exportPlayheadDebugCsv().');
// Runtime application for the Settings drawer's diagnostic switches. Every
// switch is reversible and nothing is persisted; `resetDiagnostics` restores
// the URL-seeded defaults, which for a plain app load means normal operation.
function setDiagnostic(name,value){
  if(!Object.hasOwn(diagnosticsDefault,name))return false;
  diagnostics[name]=Boolean(value);
  if(name==='referenceOverlay'){
    if(diagnostics.referenceOverlay)document.documentElement.dataset.playheadDebug='1';
    else delete document.documentElement.dataset.playheadDebug;
    if(diagnostics.referenceOverlay){
      // Enabled mid-run: resume the reference sampling loop.
      if(playheadDebug.run&&!playheadDebug.raf){
        playheadDebug.lastFrame=null;
        playheadDebug.raf=requestAnimationFrame(now=>playheadDebug.frame(now));
      }
    }else{
      cancelAnimationFrame(playheadDebug.raf);playheadDebug.raf=0;
      playheadDebug.hide();
    }
  }
  // The three metronome switches are independent enables. Switching the
  // master off clears only its own labels/current-dot state; the needle and
  // pulse sub-toggles cancel their own compositor animations.
  if(name==='metronomeVisual'&&!diagnostics.metronomeVisual)visualMetronome.clearLabels();
  if(name==='metronomeNeedle'&&!diagnostics.metronomeNeedle){
    visualMetronome.needleAnimation?.cancel();visualMetronome.needleAnimation=null;
  }
  if(name==='metronomePulses'&&!diagnostics.metronomePulses){
    const root=document.getElementById('visual-metronome');
    root?.querySelectorAll('.visual-metronome-beats i').forEach(dot=>{
      dot.pulseAnimation?.cancel();dot.pulseAnimation=null;
      const glow=dot.firstElementChild;
      glow?.glowAnimation?.cancel();if(glow)glow.glowAnimation=null;
    });
    if(root){delete root.dataset.pulse;delete root.dataset.scheduledAt;}
    visualMetronome.pending.length=0;
  }
  if(name==='perfCounters')performanceProfile.enabled=diagnostics.perfCounters;
  // The HUD shows whenever any measurement mode is active, so production
  // pacing can be measured with the reference overlay's visual overhead off.
  if(name==='referenceOverlay'||name==='perfCounters')playheadHud.syncVisibility();
  return true;
}
function resetDiagnostics(){for(const name of Object.keys(diagnosticsDefault))setDiagnostic(name,diagnosticsDefault[name]);}
function onScoreScroll(event){
  if(scoreData.container===event.currentTarget)scoreData.cameraY=scoreData.container.scrollTop;
}
function cameraAtBeat(beat,position) {
  if(beat<0||position.row<=0)return 0;
  // barsPerLine is always 1, so the row index is the bar index.
  const firstBarInRow=scoreData.bars[position.row]??scoreData.bars[position.bar]??scoreData.bars[0];
  const transitionBeats=Math.max(.25,Math.min(1,scoreData.meter*.25));
  const linear=Math.max(0,Math.min(1,(beat-firstBarInRow.startBeat)/transitionBeats));
  // A musical-time smoothstep makes the page movement deterministic at every
  // refresh rate and settles the new system during its first beat.
  const eased=linear*linear*(3-2*linear);
  const from=Math.min((position.row-1)*scoreData.rowHeight,scoreData.maxScroll);
  const to=Math.min(position.row*scoreData.rowHeight,scoreData.maxScroll);
  return from+(to-from)*eased;
}
function scrollScoreToBeat(beat) {
  const {container,bars,rowHeight,maxScroll}=scoreData;
  if(!container||!bars.length)return;
  const position=positionAtBeat(beat);
  const target=Math.min(position.row*rowHeight,maxScroll);
  container.scrollTop=target;
  container.scrollLeft=0;
  scoreData.cameraY=target;
  // The run's first score update occurs at the exact bar boundary. Preserve
  // this deliberate starting viewport through that bar instead of letting the
  // normal inter-system camera transition move it back one row.
  scoreData.preserveCameraBar=position.bar;
  scoreData.preserveViewportOnRewind=false;
}
function expectedJudgementKey(result){return result.expected_id?`id:${result.expected_id}`:`beat:${Number(result.expected_beat).toFixed(6)}|${result.instrument??''}`;}
// Compare the fields that affect notation feedback without serialising the
// whole projection. Long sessions accumulate confirmed judgements, so avoiding
// JSON.stringify here keeps each new hit O(fields) instead of O(JSON bytes).
function sameJudgement(left,right){
  if(left===right)return true;
  if(!left||!right)return false;
  return left.expected_id===right.expected_id
    &&left.iteration===right.iteration
    &&left.expected_beat===right.expected_beat
    &&left.actual_beat===right.actual_beat
    &&left.instrument===right.instrument
    &&left.judgement===right.judgement
    &&left.error_ms===right.error_ms
    &&left.excellent===right.excellent
    &&left.note_score===right.note_score
    &&left.event_id===right.event_id
    &&left.confirmed===right.confirmed;
}
function noteForJudgement(result){
  return (result.expected_id&&scoreData.noteByExpectedId?.get(result.expected_id))
    ??scoreData.noteByBeatInstrument?.get(`${Number(result.expected_beat).toFixed(6)}|${result.instrument??'snare'}`)
    ??scoreData.notes.find(note=>Math.abs(note.beat-result.expected_beat)<.001&&(!result.instrument||instrumentForHit(note.hit)===result.instrument));
}
function clearExpectedJudgement(result,touchedGroups){
  const meta=noteForJudgement(result),group=meta?.group??meta?.note.getSVGElement?.();if(!group)return;
  const noteheads=meta?.noteheads??group.querySelectorAll('.vf-notehead'),notehead=meta?.notehead??noteheads[meta?.headIndex??0]??noteheads[0];
  if(noteheads.length<=1)delete group.dataset.judgement;
  if(notehead){delete notehead.dataset.judgement;notehead.removeAttribute('role');notehead.removeAttribute('aria-label');notehead.querySelector('.judgement-halo')?.remove();}
  touchedGroups.add(group);
}
function paintExpectedJudgement(result,touchedGroups){
  const meta=noteForJudgement(result),group=meta?.group??meta?.note.getSVGElement?.();if(!group)return;
  const judgement=result.judgement==='on_time'?(result.excellent?'perfect':'good'):(result.excellent?'perfect':result.judgement);
  const noteheads=meta?.noteheads??group.querySelectorAll('.vf-notehead'),notehead=meta?.notehead??noteheads[meta?.headIndex??0]??noteheads[0];
  if(noteheads.length<=1)group.dataset.judgement=judgement;
  if(notehead){const hasOffset=judgement!=='missed'&&Number.isFinite(result.error_ms);notehead.dataset.judgement=judgement;notehead.setAttribute('role','img');notehead.setAttribute('aria-label',`${judgement.replace('_',' ')}${hasOffset?` ${Number(result.error_ms)>=0?'+':''}${Number(result.error_ms).toFixed(0)} milliseconds`:''}`);}
  if(['perfect','good','early','late'].includes(judgement)){
    const bounds=meta?.headBounds;
    const head=meta?.headText??notehead?.querySelector(':scope > text');
    if(notehead&&(bounds||head)){
      const cx=bounds?bounds.cx:(()=>{const b=head.getBBox();return b.x+b.width/2;})();
      const cy=bounds?bounds.cy:(()=>{const b=head.getBBox();return b.y+b.height/2;})();
      const r=bounds?bounds.r:(()=>{const b=head.getBBox();return Math.max(6,b.width*.84);})();
      const halo=document.createElementNS('http://www.w3.org/2000/svg','circle');
      halo.setAttribute('class','judgement-halo');halo.setAttribute('cx',String(cx));halo.setAttribute('cy',String(cy));halo.setAttribute('r',String(r));
      notehead.insertBefore(halo,head);
    }
  }
  scoreData.paintedGroups?.add(group);
  touchedGroups.add(group);
}
function nearestNoteInRow(row,beat){
  const notes=scoreData.notesByRow?.[row]??[];if(!notes.length)return null;
  let low=0,high=notes.length;
  while(low<high){const middle=(low+high)>>1;if(notes[middle].beat<beat)low=middle+1;else high=middle;}
  const after=notes[Math.min(low,notes.length-1)],before=notes[Math.max(0,low-1)];
  return Math.abs(before.beat-beat)<=Math.abs(after.beat-beat)?before:after;
}
function noteHeadY(meta,fallback){
  if(Number.isFinite(meta?.headCenterY))return meta.headCenterY;
  const head=meta?.headText??meta?.note.getSVGElement?.()?.querySelectorAll('.vf-notehead > text')[meta?.headIndex??0];
  if(!head)return fallback;
  const cached=scoreData.headYCache?.get(head);if(Number.isFinite(cached))return cached;
  const bounds=head.getBBox(),matrix=head.getCTM(),value=matrix?matrix.b*(bounds.x+bounds.width/2)+matrix.d*(bounds.y+bounds.height/2)+matrix.f:fallback;
  scoreData.headYCache?.set(head,value);return value;
}
function applyJudgements(judgements) {
  if(!diagnostics.visualFeedback)return;
  if(judgements===scoreData.lastJudgements)return;
  const profileStart=performanceProfile.enabled?performance.now():0;
  scoreData.lastJudgements=judgements;
  const previous=scoreData.appliedExpectedJudgements??new Map(),next=new Map(),touchedGroups=new Set();
  for(const result of judgements)if(Number.isFinite(result.expected_beat))next.set(expectedJudgementKey(result),result);
  for(const [key,result] of previous)if(!next.has(key)||!sameJudgement(next.get(key),result))clearExpectedJudgement(result,touchedGroups);
  for(const [key,result] of next)if(!previous.has(key)||!sameJudgement(previous.get(key),result))paintExpectedJudgement(result,touchedGroups);
  scoreData.appliedExpectedJudgements=next;
  const markers=scoreData.markerLayer??scoreData.svg.querySelector('#unmatched-markers');
  const markerResults=[];
  for(const result of judgements)if(!Number.isFinite(result.expected_beat)&&Number.isFinite(result.actual_beat)&&(result.judgement==='extra'||result.judgement==='wrong_instrument'))markerResults.push(result);
  if(markerResults.length||markers?.firstChild){
    markers?.replaceChildren();
    for(const result of markerResults){
      const position=positionAtBeat(result.actual_beat),nearest=nearestNoteInRow(position.row,result.actual_beat);
      const y=noteHeadY(nearest,(position.y1+position.y2)/2);
      const circle=document.createElementNS('http://www.w3.org/2000/svg','circle');circle.setAttribute('class','unmatched-hit-marker');circle.setAttribute('cx',String(position.x));circle.setAttribute('cy',String(y));circle.setAttribute('r','4');circle.dataset.beat=String(result.actual_beat);circle.dataset.systemRow=String(position.row);circle.dataset.judgement=result.judgement;circle.setAttribute('aria-label',`Unmatched strike at beat ${Number(result.actual_beat).toFixed(2)}`);markers?.appendChild(circle);
    }
  }
  performanceProfile.record('feedback_paint_work_ms',performance.now()-profileStart);
}
function updateScore(beat, judgements, loopAware=false, leadInBeat=null) {
  const profileStart=performanceProfile.enabled?performance.now():0;
  const hasJudgements=Array.isArray(judgements);
  const {notes,svg,container}=scoreData; if(!svg||!notes.length)return;
  // Keep playhead/scroll work on the transport frame. Feedback painting is
  // coalesced to a frame-aligned fence and applied in a task after that
  // frame's rendering steps, so its SVG mutations and layout reads do not
  // extend the animation-frame callback batch that must be followed by
  // rendering steps (see queueScoreFeedback).
  if(hasJudgements)queueScoreFeedback(judgements);
  const previousBeat=scoreData.lastBeat;
  const position=positionAtBeat(beat),playhead=scoreData.playhead;
  const rewound=beat>=0&&previousBeat>=0&&beat<previousBeat-.001;
  // Reading layout after mutating the score forces a synchronous SVG layout
  // on every frame. Only a rewind needs to inspect the actual viewport.
  const targetAlreadyVisible=rewound&&position.y1>=container.scrollTop-1&&position.y2<=container.scrollTop+scoreData.viewportHeight+1;
  const previousBar=previousBeat>=0?Math.floor(Math.max(0,previousBeat-.000001)/scoreData.meter):-1;
  if(rewound&&(loopAware||scoreData.preserveViewportOnRewind)&&(previousBar===position.bar||targetAlreadyVisible))scoreData.preserveCameraBar=position.bar;
  if(beat>=0)scoreData.preserveViewportOnRewind=false;
  scoreData.lastBeat=beat;
  if(playhead){
    // The overlay is the visible playhead: only its transform changes per
    // frame, so the compositor moves it without repainting the score SVG.
    // The SVG line stays hidden (CSS visibility) and only mirrors the
    // attributes and classes tests and the diagnostic harness observe; a
    // WebKit bug in composited SVG 3D transforms froze the old CSS-transform
    // line at system boundaries, so geometry stays in SVG coordinates. The
    // mirror keeps full frame rate: the browser performance-correctness
    // suite samples data-beat per frame as the notation view's timeline
    // readout, and measured WebKit pacing shows no per-frame stall to trade
    // against that contract.
    playhead.setAttribute('x1',String(position.x));
    playhead.setAttribute('x2',String(position.x));
    playhead.dataset.x=String(position.x);
    if(performanceProfile.enabled)playhead.dataset.beat=String(beat);
    const overlay=scoreData.playheadOverlay;
    if(position.row!==scoreData.lastPlayheadRow){
      playhead.setAttribute('y1',String(position.y1));playhead.setAttribute('y2',String(position.y2));playhead.dataset.row=String(position.row);
      // WebKit freezes composited layers whose geometry mutates every frame;
      // write overlay geometry only at row boundaries and transform per frame.
      if(overlay){overlay.style.top=`${position.y1}px`;overlay.style.height=`${Math.max(1,position.y2-position.y1)}px`;}
      scoreData.lastPlayheadRow=position.row;
    }
    if(playhead.dataset.bar!==String(position.bar))playhead.dataset.bar=String(position.bar);
    if(overlay)overlay.style.transform=`translate3d(${position.x}px,0,0)`;
    const visible=beat>=0&&beat<=scoreData.totalBeats;
    if(visible!==scoreData.lastPlayheadVisible){
      playhead.classList.toggle('visible',visible);
      overlay?.classList.toggle('visible',visible);
      scoreData.lastPlayheadVisible=visible;
    }
    // Lead-in preview: during the final beat of the current system, show where
    // the playhead will appear at the next downbeat. The transport frame passes
    // the next downbeat's score position so a bar selection or loop preview the
    // bar actually played next; without one, the following engraved bar is the
    // only honest target. The marker's geometry is written only when its target
    // bar changes; per-frame work is a class toggle. It never moves the active
    // playhead and disappears at the downbeat, when the playhead itself arrives.
    const preview=scoreData.playheadPreview;
    if(preview){
      const barEndBeat=(position.bar+1)*scoreData.meter;
      const show=leadInBeat!==null&&beat>=0&&beat>=barEndBeat-1&&beat<barEndBeat;
      if(show){
        const target=positionAtBeat(leadInBeat),targetBar=target.bar;
        if(preview.dataset.targetBar!==String(targetBar)){
          preview.dataset.targetBar=String(targetBar);
          preview.style.top=`${target.y1}px`;
          preview.style.height=`${Math.max(1,target.y2-target.y1)}px`;
          preview.style.transform=`translate3d(${target.x}px,0,0)`;
        }
      }
      if(show!==preview.classList.contains('visible'))preview.classList.toggle('visible',show);
    }
  }
  if(beat>=0){
    const preservedBar=scoreData.preserveCameraBar;
    const stillPreserving=preservedBar!==null&&(position.bar===preservedBar||beat<=(preservedBar+1)*scoreData.meter+.001);
    // A scroll listener tracks a deliberate seek's viewport without reading
    // layout on every ordinary playback frame. The diagnostic switch keeps
    // the camera fixed so per-frame scroll work can be isolated.
    if(diagnostics.autoScroll&&!stillPreserving){
      scoreData.preserveCameraBar=null;
      const cameraY=cameraAtBeat(beat,position);
      if(Math.abs(cameraY-scoreData.cameraY)>.05){container.scrollTop=cameraY;scoreData.cameraY=cameraY;}
    }
  }
  performanceProfile.record('score_update_ms',performance.now()-profileStart);
}
function cancelQueuedScoreFeedback(){
  if(pendingScoreFeedbackFrame)cancelAnimationFrame(pendingScoreFeedbackFrame);
  pendingScoreFeedbackFrame=0;
  if(pendingScoreFeedbackTask){clearTimeout(pendingScoreFeedbackTask);pendingScoreFeedbackTask=0;}
  pendingScoreFeedback=null;
}
function flushScoreFeedback(){
  cancelQueuedScoreFeedback();
  const pending=pendingScoreFeedback;pendingScoreFeedback=null;
  if(pending&&scoreData.svg&&scoreData.notes.length)applyJudgements(pending);
}
function applyPendingScoreFeedback(){
  pendingScoreFeedbackTask=0;
  const pending=pendingScoreFeedback;pendingScoreFeedback=null;
  if(pending&&scoreData.svg&&scoreData.notes.length)applyJudgements(pending);
}
function queueScoreFeedback(judgements){
  if(!Array.isArray(judgements))return;
  if(!diagnostics.visualFeedback)return;
  // A scoring update is a replacement projection, not an append-only event:
  // a later hit may correct an earlier provisional match. Retain only the
  // newest projection when several MIDI messages arrive in one browser turn.
  pendingScoreFeedback=judgements;
  if(pendingScoreFeedbackFrame)return;
  pendingScoreFeedbackFrame=requestAnimationFrame(()=>{
    pendingScoreFeedbackFrame=0;
    // Two-stage fence: the rAF only coalesces to the frame boundary and
    // verifies a frame is imminent; the paint itself is re-scheduled as a
    // macrotask so it runs after THIS rendering opportunity's style, layout
    // and paint steps complete. A feedback rAF callback would otherwise run
    // inside the same callback batch as the transport frame, ahead of that
    // batch's rendering steps, and its SVG mutations plus getBBox/getCTM
    // layout reads would extend the same frame's critical path - delaying
    // the very paint the playhead commit was written for. Painting from the
    // task instead lands the work in the interval after that opportunity's
    // rendering steps and before the next frame. The mutation is presented
    // on the following frame at the earliest; neither the rAF nor the
    // timeout is a presentation fence, and the compositor may lag both.
    // Newest-projection coalescing is preserved: a projection that arrives
    // between the fence and the task replaces the pending one before the
    // task drains it, so no intermediate feedback can go stale on screen.
    if(!pendingScoreFeedback)return;
    pendingScoreFeedbackTask=setTimeout(applyPendingScoreFeedback,0);
  });
}
function clearScoreFeedback(preserveViewport=false) {
  cancelQueuedScoreFeedback();
  const empty=[];
  if(diagnostics.visualFeedback){
    const painted=scoreData.paintedGroups??new Set();
    for(const group of painted){
      delete group.dataset.judgement;group.classList.remove('excellent-flash');
      for(const head of group.querySelectorAll('.vf-notehead')){delete head.dataset.judgement;head.removeAttribute('role');head.removeAttribute('aria-label');head.querySelector('.judgement-halo')?.remove();}
    }
    painted.clear();
  }
  scoreData.lastJudgements=empty;
  scoreData.appliedExpectedJudgements=new Map();
  (scoreData.markerLayer??scoreData.svg?.querySelector('#unmatched-markers'))?.replaceChildren();
  if(preserveViewport){scoreData.cameraY=scoreData.container.scrollTop;scoreData.preserveViewportOnRewind=true;return;}
  scoreData.lastBeat=-Infinity;
  scoreData.preserveCameraBar=null;
  scoreData.preserveViewportOnRewind=false;
  scoreData.cameraY=0;scoreData.container.scrollTop=0;scoreData.container.scrollLeft=0;
  updateScore(-99,empty);
  playheadDebug.hide();
}
function clearScoreFeedbackPreserveViewport(){clearScoreFeedback(true);}

const MIDI_INPUT_ID_KEY='rudiment-midi-input',MIDI_INPUT_IDENTITY_KEY='rudiment-midi-input-identity';
let selectedInput = null, midiCallback = null, midiBound = false;
const portInfo = input => ({id:input.id,name:input.name||'Unnamed MIDI input',manufacturer:input.manufacturer||'Unknown manufacturer',state:input.state,connection:input.connection});
const connectedMidiInputs = () => WebMidi.inputs.filter(input=>input.state==='connected'&&input.connection==='open');
function rememberMidiInput(input){
  if(!input)return;
  localStorage.setItem(MIDI_INPUT_ID_KEY,input.id);
  localStorage.setItem(MIDI_INPUT_IDENTITY_KEY,JSON.stringify({name:input.name||'',manufacturer:input.manufacturer||''}));
}
function rememberedMidiIdentity(){
  try{return JSON.parse(localStorage.getItem(MIDI_INPUT_IDENTITY_KEY)||'null');}
  catch{return null;}
}
function sendMidiState(extra={}) { midiCallback?.({kind:'state',ports:WebMidi.enabled?WebMidi.inputs.map(portInfo):[],selectedId:selectedInput?.id||null,...extra}); }
function bindMidiInputs() {
  if(!WebMidi.enabled)return;
  for(const input of WebMidi.inputs){input.removeListener('midimessage');input.addListener('midimessage',event=>{
    const profileStart=performanceProfile.enabled?performance.now():0;
    // Never merge multiple kits implicitly. With no selected port, input is
    // deliberately silent until the user chooses one.
    if(!selectedInput||selectedInput.id!==input.id)return;
    const raw=Array.from(event.rawData||[]),status=raw[0]||0,data1=raw[1]||0,data2=raw[2]||0,command=status&0xf0,channel=(status&0x0f)+1;
    const type=command===0x90?(data2===0?'note_off':'note_on'):command===0x80?'note_off':command===0xb0?'cc':'other';
    midiCallback?.({kind:'message',type,note:data1,velocity:data2,channel,timestamp:event.timestamp,rawBytes:raw,inputId:input.id,inputName:input.name||'MIDI input'});
    performanceProfile.record('midi_callback_ms',performance.now()-profileStart);
  });}
}
function choosePreferredInput(){
  const inputs=connectedMidiInputs();
  const savedId=localStorage.getItem(MIDI_INPUT_ID_KEY),savedIdentity=rememberedMidiIdentity();
  const identityMatches=savedIdentity?inputs.filter(input=>(input.name||'')===savedIdentity.name&&(input.manufacturer||'')===savedIdentity.manufacturer):[];
  selectedInput=inputs.find(input=>input.id===savedId)
    ||(identityMatches.length===1?identityMatches[0]:null)
    ||(selectedInput&&inputs.find(input=>input.id===selectedInput.id))
    ||(inputs.length===1?inputs[0]:null)
    ||null;
  rememberMidiInput(selectedInput);
}
function bindMidiLifecycle(){if(midiBound)return;WebMidi.addListener('connected',event=>{if(event.port?.type!=='input')return;bindMidiInputs();choosePreferredInput();sendMidiState({permission:'granted',reconnected:true});});WebMidi.addListener('disconnected',event=>{if(event.port?.type!=='input')return;const selectedDisconnected=event.port.id===selectedInput?.id;if(selectedDisconnected)selectedInput=null;bindMidiInputs();choosePreferredInput();sendMidiState({disconnectedId:selectedDisconnected?event.port.id:null,permission:'granted'});});midiBound=true;}
async function connectMidi(callback) {
  midiCallback=callback;
  try {
    if(!WebMidi.enabled)await WebMidi.enable({sysex:false});
    bindMidiLifecycle();bindMidiInputs();choosePreferredInput();sendMidiState({permission:'granted'});
  } catch(error) { sendMidiState({error:error?.message||'MIDI permission was denied.',permission:'denied'}); }
}
async function autoConnectMidi(callback){midiCallback=callback;if(!navigator.requestMIDIAccess){sendMidiState({permission:'unsupported'});return;}await connectMidi(callback);}
function selectMidi(id){selectedInput=connectedMidiInputs().find(input=>input.id===id)||null;rememberMidiInput(selectedInput);sendMidiState({permission:'granted'});}

async function readFile(file,callback,maxFileBytes){if(!file)return;const generation=++midiLoadGeneration;if(file.size>maxFileBytes){callback(new Uint8Array(),file.name,`Files larger than ${maxFileBytes/1024/1024} MB are not supported.`);return;}try{const bytes=await file.arrayBuffer();if(generation===midiLoadGeneration)callback(new Uint8Array(bytes),file.name,'');}catch(error){if(generation===midiLoadGeneration)callback(new Uint8Array(),file.name,error?.message||'Could not read file.');}}
function chooseMidi(callback,maxFileBytes){midiFileCallback=callback;const input=document.createElement('input');input.type='file';input.accept='.mid,.midi,audio/midi';input.onchange=()=>readFile(input.files?.[0],callback,maxFileBytes);input.click();}
async function loadBundled(url,callback,saveToLibrary=false,displayName=''){const generation=++midiLoadGeneration;try{const response=await fetch(url);if(!response.ok)throw new Error(`HTTP ${response.status}`);const bytes=await response.arrayBuffer();if(generation===midiLoadGeneration)callback(new Uint8Array(bytes),displayName||url.split('/').pop(),'',saveToLibrary);}catch(error){if(generation===midiLoadGeneration)callback(new Uint8Array(),displayName||url,error?.message||'Could not load bundled exercise.');}}
async function loadBundledFromLibrary(url,displayName=''){if(!midiFileCallback)return;return loadBundled(url,midiFileCallback,false,displayName);}
let dropController;
let midiFileCallback;
function bindDrop(callback,maxFileBytes){
  midiFileCallback=callback;
  dropController?.abort();
  dropController = new AbortController();
  const element=document.getElementById('score-card');if(!element)return;
  const options={signal:dropController.signal};
  const over=event=>{event.preventDefault();element.classList.add('dragging');};
  element.addEventListener('dragover',over,options);
  element.addEventListener('dragleave',()=>element.classList.remove('dragging'),options);
  element.addEventListener('drop',event=>{event.preventDefault();element.classList.remove('dragging');readFile(event.dataTransfer?.files?.[0],callback,maxFileBytes);},options);
}
function trapDialogTab(event,selector){
  if(event.key!=='Tab')return;
  const root=document.querySelector(selector);if(!root)return;
  const focusable=[...root.querySelectorAll('button:not([disabled]),[href],input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])')].filter(element=>element.getClientRects().length>0);
  if(!focusable.length){event.preventDefault();return;}
  const first=focusable[0],last=focusable.at(-1),active=document.activeElement;
  if(event.shiftKey&&(active===first||!root.contains(active))){event.preventDefault();last.focus();}
  else if(!event.shiftKey&&(active===last||!root.contains(active))){event.preventDefault();first.focus();}
}
function downloadText(value,name,type){const anchor=document.createElement('a');anchor.href=URL.createObjectURL(new Blob([value],{type}));anchor.download=name;anchor.click();setTimeout(()=>URL.revokeObjectURL(anchor.href),500);}
function downloadJson(value,name){downloadText(value,name,'application/json');}

window.rudiment={
  bindPresentationSnapshot:provider=>presentation.bind(provider),unbindPresentation:()=>{setHighwayMode(false);presentation.unbind();},presentationActive:()=>presentation.isActive(),mountPresentationRenderer:(renderer,lanes)=>presentation.mount(renderer,lanes),presentationExerciseChanged:highwayExerciseChanged,presentationConfiguration:value=>presentation.configure(value),presentationJudgements:value=>presentation.judgements(value),savedHighwayMode,saveHighwayMode,setHighwayMode,savedNotationTheme,saveNotationTheme,savedOnboardingSkip,saveOnboardingSkip,
  ensureAudio:()=>audio.ensure(),audioState:()=>audio.state,audioDiagnostics:()=>({state:audio.state,contextState:audio.raw?.state??'uninitialised',activeRun:Boolean(audio.activeRun),currentBeat:audio.activeRun?Math.min(audio.activeRun.totalBeats,transportBeat(audio.activeRun)):audio.lastBeat,referenceNoteCount:audio.activeRun?.referenceNoteCount??0,runMetronome:audio.activeRun?.metronome??false,runTimbre:audio.activeRun?.timbre??null,activeVoices:audio.voices.size,scheduledVoices:audio.scheduledVoices.size,openHatVoices:audio.openHatVoices.size,snareGain:audio.snareGain?.gain.value??0,clickGain:audio.clickGain?.gain.value??0,sampleLayers:audio.buffers.filter(Boolean).length,drumSampleCount:Object.values(audio.drumBuffers).filter(Boolean).length,drumSampleErrors:{...audio.drumSampleErrors},drumStarts:{...audio.drumStarts},metronomeSampleCount:Object.values(audio.metronomeBuffers).filter(Boolean).length,metronomeSampleNames:Object.keys(audio.metronomeBuffers),metronomeSampleErrors:{...audio.metronomeSampleErrors},sampleErrors:[...audio.sampleErrors],snareStarts:audio.snareStarts,lastSnareError:audio.lastSnareError,lastMetronomeError:audio.lastMetronomeError,transportBpm:Tone.getTransport().bpm.value,transportTicks:Tone.getTransport().ticks,layerPeaks:[...audio.layerPeaks],layerGains:[...audio.layerGains],velocityGains:Array.from({length:127},(_,index)=>velocityGain(index+1)),velocityLayers:Array.from({length:127},(_,index)=>velocityLayers(index+1))}),identifyMetronomeBuffer:buffer=>Object.entries(audio.metronomeBuffers).find(([,sample])=>sample===buffer)?.[0]??null,playSnare:velocity=>audio.monitorSnare(velocity),playDrum:(instrument,articulation,velocity)=>audio.monitorDrum(instrument,articulation,velocity),previewClick:async(timbre,accent=true)=>{await audio.ensure();await audio.waitForMetronome(timbre);audio.click(audio.raw.currentTime+.03,accent,timbre);},
  setVolumes:(snare,click)=>{if(audio.snareGain)audio.snareGain.gain.value=snare;if(audio.clickGain)audio.clickGain.gain.value=click*1.5;},startAudio:(options,frame,done)=>audio.start(options,frame,done),setAudioTempo:bpm=>audio.setTempo(bpm),currentAudioBeat:()=>audio.activeRun?Math.min(audio.activeRun.totalBeats,transportBeat(audio.activeRun)):Number.NaN,audioOutputRunning:()=>!!audio.raw&&audio.raw.state==='running',stopAudio:()=>audio.stop(),startCalibration:()=>audio.calibration(),
  renderScore,setSelectedBars,bindScoreSelection,scrollScoreToBeat,updateScore,nextDownbeatScoreBeat,queueScoreFeedback,flushScoreFeedback,clearScoreFeedback,clearScoreFeedbackPreserveViewport,connectMidi,autoConnectMidi,selectMidi,chooseMidi,loadBundled,loadBundledFromLibrary,bindDrop,trapDialogTab,performanceDiagnostics:()=>performanceProfile.snapshot(),resetPerformanceDiagnostics:()=>performanceProfile.reset(),playheadDebugSnapshot:()=>playheadDebug.snapshot(),resetPlayheadDebug:()=>playheadDebug.reset(),playheadDebugCsv:()=>playheadDebug.csv(),exportPlayheadDebugCsv:()=>playheadDebug.export(),diagnosticsSnapshot:()=>({...diagnostics}),setDiagnostic:(name,value)=>setDiagnostic(name,value),resetDiagnostics:()=>resetDiagnostics(),bridgeBuild:()=>BRIDGE_BUILD,playheadHudSnapshot:()=>playheadHud.snapshot(),
  saveSession:record=>put('sessions',record),loadSessions:()=>all('sessions'),clearHistory:()=>practiceTime.clearHistory(),setPracticeActive:active=>active?practiceTime.start():practiceTime.stop(),loadPracticeTime:()=>practiceTime.read(),saveSettings:value=>put('settings',{key:'app',value}),loadSettings:async()=>(await (await dbPromise).get('settings','app'))?.value||{},saveProfile:profile=>put('profiles',profile),loadProfiles:()=>all('profiles'),resetLocalSetup:async()=>{await (await dbPromise).delete('settings','app');await clear('profiles');localStorage.removeItem(MIDI_INPUT_ID_KEY);localStorage.removeItem(MIDI_INPUT_IDENTITY_KEY);localStorage.removeItem('rudiment-onboarding-skipped');},
  saveImport:(hash,name,bytes,metadata)=>put('imports',{hash,name,bytes:Array.from(bytes),metadata,schemaVersion:1}),loadImports:()=>all('imports'),loadImport:hash=>(async()=>{const db=await dbPromise;return db.get('imports',hash);})(),loadSavedImport,deleteImport:hash=>(async()=>{const db=await dbPromise;return db.delete('imports',hash);})(),saveLastImport,exportHistory:async()=>downloadJson(JSON.stringify(await all('sessions'),null,2),'rudiment-history.json')
};
