import * as Tone from 'tone';
import { WebMidi } from 'webmidi';
import { openDB } from 'idb';
import { Renderer, Stave, StaveNote, Voice, Formatter, Beam, Annotation, Articulation } from 'vexflow';

const DB_NAME = 'rudiment';
const DB_VERSION = 2;
const bridgeUrl = document.currentScript?.src || document.querySelector('script[src*="bridge.js"]')?.src || document.baseURI;
const publicAsset = path => new URL(path, bridgeUrl).toString();
const dbPromise = openDB(DB_NAME, DB_VERSION, {
  upgrade(db) {
    if (!db.objectStoreNames.contains('sessions')) db.createObjectStore('sessions', { keyPath: 'id' });
    if (!db.objectStoreNames.contains('settings')) db.createObjectStore('settings', { keyPath: 'key' });
    if (!db.objectStoreNames.contains('imports')) db.createObjectStore('imports', { keyPath: 'hash' });
    if (!db.objectStoreNames.contains('profiles')) db.createObjectStore('profiles', { keyPath: 'id' });
  }
});
const put = async (store, value) => (await dbPromise).put(store, value);
const all = async store => (await dbPromise).getAll(store);
const clear = async store => (await dbPromise).clear(store);
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

// Tone.Transport is the single musical transport; source nodes use callback audio time.
const audio = {
  raw: null, snareGain: null, clickGain: null, master: null, buffers: [], layerGains: [], layerPeaks: [], voices:new Set(), raf: 0, generation: 0, state: 'locked',
  async ensure() {
    this.state = 'loading';
    await Tone.start();
    this.raw = Tone.getContext().rawContext;
    if (!this.master) {
      this.master = this.raw.createGain(); this.master.gain.value = .84;
      this.snareGain = this.raw.createGain(); this.snareGain.gain.value = .72;
      this.clickGain = this.raw.createGain(); this.clickGain.gain.value = .48;
      const limiter = this.raw.createDynamicsCompressor(); limiter.threshold.value = -3; limiter.ratio.value = 12;
      this.snareGain.connect(this.master); this.clickGain.connect(this.master); this.master.connect(limiter); limiter.connect(this.raw.destination);
      const urls = ['audio/snare-soft.m4a','audio/snare-medium.m4a','audio/snare-hard.m4a','audio/snare-accent.m4a'].map(publicAsset);
      this.buffers = await Promise.all(urls.map(async (url,index) => {
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
          this.layerPeaks[index]=peak;
          this.layerGains[index]=peak>0?Math.min(24,Math.max(.5,.32/peak)):1;
          return buffer;
        }
        catch { return null; }
      }));
    }
    this.state = this.buffers.some(Boolean) ? 'ready' : 'fallback';
    return { state: this.state, sampleLayers: this.buffers.filter(Boolean).length };
  },
  perfAt(audioTime) {
    const direct = performance.now() + (audioTime - this.raw.currentTime) * 1000;
    if (this.raw.getOutputTimestamp) {
      const time = this.raw.getOutputTimestamp();
      const output = time.performanceTime + (audioTime - time.contextTime) * 1000;
      // Some engines report a zero/stale output timestamp until the first render quantum.
      if (Number.isFinite(output) && Math.abs(output - direct) < 120) return output;
    }
    return direct;
  },
  playSnare(velocity = 105, when = null) {
    if (!this.raw || this.raw.state !== 'running') { this.state='locked'; return false; }
    if(Number(velocity)<=0)return false;
    const time = when ?? this.raw.currentTime, response=velocityGain(velocity);
    const layers=velocityLayers(velocity).filter(layer=>this.buffers[layer.index]);
    if (layers.length) {
      for(const layer of layers){
        const source = this.raw.createBufferSource(), gain = this.raw.createGain(); source.buffer = this.buffers[layer.index];
        source.playbackRate.value = .99 + Math.random() * .02;
        gain.gain.value = (this.layerGains[layer.index]||1) * response * layer.weight;
        source.connect(gain); gain.connect(this.snareGain);this.voices.add(source);
        source.onended=()=>{this.voices.delete(source);source.disconnect();gain.disconnect();}; source.start(time);
      }
    } else this.syntheticSnare(time, velocity);
    return true;
  },
  syntheticSnare(time, velocity) {
    const length = Math.floor(this.raw.sampleRate * .2), buffer = this.raw.createBuffer(1, length, this.raw.sampleRate), data = buffer.getChannelData(0);
    for (let i = 0; i < length; i++) data[i] = (Math.random() * 2 - 1) * Math.exp(-i / (this.raw.sampleRate * .05));
    const source = this.raw.createBufferSource(), filter = this.raw.createBiquadFilter(), gain = this.raw.createGain();
    source.buffer = buffer; filter.type = 'highpass'; filter.frequency.value = 850; gain.gain.value = .6 * velocityGain(velocity);
    source.connect(filter); filter.connect(gain); gain.connect(this.snareGain);this.voices.add(source);source.onended=()=>{this.voices.delete(source);source.disconnect();filter.disconnect();gain.disconnect();}; source.start(time);
  },
  click(time, accent = false, timbre = 'woodblock') {
    const osc = this.raw.createOscillator(), gain = this.raw.createGain(), filter = this.raw.createBiquadFilter();
    const choices = { woodblock:[accent?1450:1080,'sine',.055], crisp:[accent?2400:1850,'square',.022], cowbell:[accent?760:560,'triangle',.09] };
    const [frequency, wave, duration] = choices[timbre] || choices.woodblock;
    osc.type = wave; osc.frequency.setValueAtTime(frequency,time); osc.frequency.exponentialRampToValueAtTime(frequency*.72,time+duration);
    filter.type = 'bandpass'; filter.frequency.value = frequency; filter.Q.value = 2.5;
    gain.gain.setValueAtTime(accent?.34:.22,time); gain.gain.exponentialRampToValueAtTime(.0001,time+duration);
    osc.connect(filter); filter.connect(gain); gain.connect(this.clickGain); osc.start(time); osc.stop(time+duration+.01);
  },
  stop() {
    this.generation++; cancelAnimationFrame(this.raf); this.raf = 0;
    const transport = Tone.getTransport(); transport.stop(); transport.cancel(0); transport.seconds = 0;
  },
  async start(options, onFrame, onDone) {
    await this.ensure(); this.stop(); const generation = this.generation;
    const transport = Tone.getTransport(), beatSeconds = 60 / options.bpm, countBeats = options.countInBars * options.meter;
    const offset = options.offsetBeat || 0, remaining = Math.max(0, options.totalBeats - offset), startDelay = .16;
    const runAudio = this.raw.currentTime + startDelay + countBeats * beatSeconds, finishAudio = runAudio + remaining * beatSeconds + .14;
    const runPerformance = this.perfAt(runAudio), subdivision = Math.max(1, options.subdivision || 1), interval = beatSeconds / subdivision;
    let step = 0; transport.bpm.value = options.bpm; transport.timeSignature = options.meter;
    transport.scheduleRepeat(time => {
      const localBeat = step / subdivision, globalBeat = localBeat < countBeats ? localBeat : offset + localBeat - countBeats;
      if (options.metronome) this.click(time, Number.isInteger(globalBeat) && Math.round(globalBeat) % options.meter === 0, options.timbre);
      step++;
    }, interval, 0, (countBeats + remaining) * beatSeconds);
    transport.start(`+${startDelay}`);
    const frame = () => {
      if (generation !== this.generation) return;
      // Keep the score at the exact paused position during the short audio
      // scheduling lead-in. Without the clamp, resuming visibly jumps the
      // playhead backwards before the Tone transport starts.
      const now = performance.now(), lead = now - runPerformance;
      const beat = offset + (countBeats > 0 ? lead : Math.max(0, lead)) / (beatSeconds * 1000); onFrame(beat, now, runPerformance);
      if (this.raw.currentTime >= finishAudio) { this.stop(); onDone(); } else this.raf = requestAnimationFrame(frame);
    };
    this.raf = requestAnimationFrame(frame);
    return { runPerformance, runAudio, state: this.state };
  },
  async calibration() {
    await this.ensure(); const beatSeconds = .75, start = this.raw.currentTime + .3, targets = [];
    for (let i = 0; i < 8; i++) { const time = start + i * beatSeconds; this.click(time, i === 0, 'woodblock'); targets.push(this.perfAt(time)); }
    return targets;
  }
};

let scoreData = { notes:[], svg:null, totalBeats:16, container:null, follow:'continuous', anchors:[], viewportWidth:0, current:null, lastBeat:-Infinity, lastPage:-1, judgementSignature:'' };
const handText = hand => hand === 'R' ? 'R' : hand === 'L' ? 'L' : '·';
const writtenDuration = beats => beats >= 3.999 ? 'w' : beats >= 1.999 ? 'h' : beats >= .999 ? 'q' : beats >= .499 ? '8' : '16';
const writtenValue = beats => beats >= 3.999 ? 4 : beats >= 1.999 ? 2 : beats >= .999 ? 1 : beats >= .499 ? .5 : .25;
function restValues(gap) { const values=[];let left=gap;for(const value of [4,2,1,.5,.25])while(left>=value-.001){values.push(value);left-=value;}return values; }
function renderScore(exercise, follow = 'continuous') {
  const element = document.getElementById('notation'); if (!element) return; element.innerHTML = '';
  const meter = exercise.meter_numerator, lastBeat = Math.max(...exercise.hits.map(hit=>hit.beat),0);
  const bars = Math.max(1, Math.ceil((lastBeat + .25) / meter));
  const shortest=Math.min(...exercise.hits.map(hit=>hit.notation_duration_beats||.25));
  const barWidth = shortest < .4 ? 620 : shortest < .75 ? 470 : 390, width = Math.max(element.clientWidth - 24, bars * barWidth + 28), height = 250;
  const renderer = new Renderer(element, Renderer.Backends.SVG); renderer.resize(width,height); const context = renderer.getContext(), notesMeta = [], barAnchors=[];
  for (let bar = 0; bar < bars; bar++) {
    const x = 14 + bar * barWidth, stave = new Stave(x,78,barWidth);
    stave.setConfigForLines([{visible:false},{visible:false},{visible:true},{visible:false},{visible:false}]);
    if (bar === 0) stave.addClef('percussion').addTimeSignature(`${meter}/${exercise.meter_denominator}`); stave.setContext(context).draw();
    const slots = [], slotMeta = [], start=bar*meter, end=start+meter;
    const groups=new Map();for(const hit of exercise.hits.filter(hit=>(hit.notation_beat??hit.beat)>=start-.001&&(hit.notation_beat??hit.beat)<end-.001)){const position=hit.notation_beat??hit.beat;const key=position.toFixed(4);if(!groups.has(key))groups.set(key,{position,hits:[]});groups.get(key).hits.push(hit);}
    const ordered=[...groups.values()].sort((a,b)=>a.position-b.position);let cursor=start;
    for(const group of ordered){
      if(group.position>cursor+.001){for(const value of restValues(group.position-cursor)){slots.push(new StaveNote({keys:['b/4'],duration:`${writtenDuration(value)}r`,clef:'percussion'}));cursor+=value;}}
      const hit=group.hits[0], value=writtenValue(Math.min(hit.notation_duration_beats||.25,end-group.position));
      const note=new StaveNote({keys:['b/4'],duration:writtenDuration(value),clef:'percussion'});
      note.addModifier(new Annotation(handText(hit.hand)).setVerticalJustification(1).setFont('Inter',15,'700'),0);if(hit.accent)note.addModifier(new Articulation('a>').setPosition(3),0);
      slots.push(note);for(const item of group.hits)slotMeta.push({note,beat:item.beat,notationBeat:group.position,durationBeats:value,hit:item});cursor=group.position+value;
    }
    if(cursor<end-.001)for(const value of restValues(end-cursor))slots.push(new StaveNote({keys:['b/4'],duration:`${writtenDuration(value)}r`,clef:'percussion'}));
    const voice = new Voice({numBeats:meter,beatValue:4}).setStrict(false); voice.addTickables(slots);
    new Formatter().joinVoices([voice]).format([voice],barWidth-(bar===0?92:34)); voice.draw(context,stave);
    barAnchors.push({beat:start,x:stave.getNoteStartX?.()??x+(bar===0?82:20)},{beat:end,x:stave.getNoteEndX?.()??x+barWidth-18});
    Beam.generateBeams(slots).forEach(beam=>beam.setContext(context).draw()); notesMeta.push(...slotMeta.filter(meta=>meta.hit));
  }
  const svg = element.querySelector('svg'); if (svg) { svg.setAttribute('role','img'); svg.setAttribute('aria-label',`${exercise.title} percussion notation with sticking`); }
  for (const meta of notesMeta) { const group = meta.note.getSVGElement?.(); if (group) { group.dataset.beat=String(meta.beat);group.dataset.writtenDuration=writtenDuration(meta.durationBeats); group.classList.add('score-note');const box=group.getBBox();meta.x=box.x+box.width/2; } }
  const anchorMap=new Map();for(const anchor of barAnchors)anchorMap.set(anchor.beat.toFixed(4),anchor);for(const meta of notesMeta)anchorMap.set(meta.beat.toFixed(4),{beat:meta.beat,x:meta.x});
  const anchors=[...anchorMap.values()].sort((a,b)=>a.beat-b.beat);
  const markers=document.createElementNS('http://www.w3.org/2000/svg','g');markers.id='unmatched-markers';markers.setAttribute('aria-label','Unmatched physical strikes');svg?.appendChild(markers);
  const line = document.createElementNS('http://www.w3.org/2000/svg','line'); line.id='playhead'; line.setAttribute('x1','0');line.setAttribute('x2','0');line.setAttribute('y1','49'); line.setAttribute('y2','205'); line.setAttribute('class','playhead-line'); svg?.appendChild(line);
  scoreData={notes:notesMeta,svg,totalBeats:bars*meter,container:element,follow,anchors,viewportWidth:element.clientWidth,current:null,lastBeat:-Infinity,lastPage:-1,judgementSignature:''}; updateScore(-99,[]);
}
function xAtBeat(beat) {
  const anchors=scoreData.anchors;if(!anchors.length)return 0;
  const value=Math.max(0,Math.min(scoreData.totalBeats,beat));
  let before=anchors[0],after=anchors.at(-1);
  for(const anchor of anchors){if(anchor.beat<=value)before=anchor;if(anchor.beat>=value){after=anchor;break;}}
  const span=Math.max(.001,after.beat-before.beat),ratio=Math.max(0,Math.min(1,(value-before.beat)/span));
  return before.x+(after.x-before.x)*ratio;
}
function applyJudgements(judgements) {
  const signature=JSON.stringify(judgements);if(signature===scoreData.judgementSignature)return;scoreData.judgementSignature=signature;
  for(const meta of scoreData.notes){const group=meta.note.getSVGElement?.();if(!group)continue;delete group.dataset.judgement;group.classList.remove('excellent-flash');}
  for(const result of judgements){
    if(Number.isFinite(result.expected_beat)){
      const meta=scoreData.notes.find(note=>Math.abs(note.beat-result.expected_beat)<.001),group=meta?.note.getSVGElement?.();if(!group)continue;
      group.dataset.judgement=result.excellent?'excellent':result.judgement;
      if(result.excellent){group.classList.add('excellent-flash');group.getAnimations?.().forEach(animation=>animation.cancel());group.animate?.([{filter:'drop-shadow(0 0 13px #38e0ad)',opacity:.45},{filter:'drop-shadow(0 0 3px #38ae8e)',opacity:1}],{duration:340,easing:'ease-out'});}
    }
  }
  const markers=scoreData.svg.querySelector('#unmatched-markers');markers?.replaceChildren();
  for(const result of judgements.filter(item=>!Number.isFinite(item.expected_beat)&&Number.isFinite(item.actual_beat)&&(item.judgement==='extra'||item.judgement==='wrong_instrument'))){
    const circle=document.createElementNS('http://www.w3.org/2000/svg','circle');circle.setAttribute('class','unmatched-hit-marker');circle.setAttribute('cx',xAtBeat(result.actual_beat));circle.setAttribute('cy','195');circle.setAttribute('r','4');circle.dataset.beat=String(result.actual_beat);circle.dataset.judgement=result.judgement;circle.setAttribute('aria-label',`Unmatched strike at beat ${Number(result.actual_beat).toFixed(2)}`);markers?.appendChild(circle);
  }
}
function updateScore(beat, judgements) {
  const {notes,svg,container}=scoreData; if(!svg||!notes.length)return;
  if(Array.isArray(judgements))applyJudgements(judgements);
  let nearest=notes[0]; for(const meta of notes) if(Math.abs(meta.beat-beat)<Math.abs(nearest.beat-beat)) nearest=meta;
  if(scoreData.current!==nearest){scoreData.current?.note.getSVGElement?.()?.classList.remove('is-current');if(beat>=-.1)nearest.note.getSVGElement?.()?.classList.add('is-current');scoreData.current=nearest;}
  if(beat<scoreData.lastBeat-.05||beat<0){for(const meta of notes)meta.note.getSVGElement?.()?.classList.toggle('is-past',meta.beat<beat-.05);}else{for(const meta of notes)if(meta.beat>=scoreData.lastBeat-.05&&meta.beat<beat-.05)meta.note.getSVGElement?.()?.classList.add('is-past');}
  scoreData.lastBeat=beat;
  const x=xAtBeat(beat);
  if(Number.isFinite(x)){const playhead=svg.querySelector('#playhead');if(playhead){playhead.style.transform=`translate3d(${x}px,0,0)`;playhead.dataset.x=String(x);playhead.classList.toggle('visible',beat>=0&&beat<=scoreData.totalBeats);}
    if(scoreData.follow==='continuous'&&beat>=0){const desired=Math.max(0,x-scoreData.viewportWidth*.36);if(Math.abs(container.scrollLeft-desired)>.25)container.scrollLeft=desired;}
    else if(scoreData.follow==='page'&&beat>=0){const page=Math.floor(x/Math.max(1,scoreData.viewportWidth));if(page!==scoreData.lastPage){scoreData.lastPage=page;container.scrollLeft=page*scoreData.viewportWidth;}}
    else if(beat<0&&container.scrollLeft!==0)container.scrollLeft=0;
  }
}

let selectedInput = null, midiCallback = null, midiBound = false;
const portInfo = input => ({id:input.id,name:input.name||'Unnamed MIDI input',manufacturer:input.manufacturer||'Unknown manufacturer',state:input.state,connection:input.connection});
function sendMidiState(extra={}) { midiCallback?.({kind:'state',ports:WebMidi.enabled?WebMidi.inputs.map(portInfo):[],selectedId:selectedInput?.id||null,...extra}); }
function bindMidiInputs() {
  if(!WebMidi.enabled)return;
  for(const input of WebMidi.inputs){input.removeListener('midimessage');input.addListener('midimessage',event=>{
    if(selectedInput&&selectedInput.id!==input.id)return;
    const raw=Array.from(event.rawData||[]),status=raw[0]||0,data1=raw[1]||0,data2=raw[2]||0,command=status&0xf0,channel=(status&0x0f)+1;
    const type=command===0x90?(data2===0?'note_off':'note_on'):command===0x80?'note_off':command===0xb0?'cc':'other';
    midiCallback?.({kind:'message',type,note:data1,velocity:data2,channel,timestamp:event.timestamp,rawBytes:raw,inputId:input.id,inputName:input.name||'MIDI input'});
  });}
}
function choosePreferredInput(){const saved=localStorage.getItem('rudiment-midi-input');selectedInput=WebMidi.inputs.find(input=>input.id===saved)||((WebMidi.inputs.length===1)?WebMidi.inputs[0]:selectedInput&&WebMidi.inputs.find(input=>input.id===selectedInput.id))||null;if(selectedInput)localStorage.setItem('rudiment-midi-input',selectedInput.id);}
function bindMidiLifecycle(){if(midiBound)return;WebMidi.addListener('connected',()=>{bindMidiInputs();choosePreferredInput();sendMidiState({permission:'granted',reconnected:true});});WebMidi.addListener('disconnected',event=>{if(event.port?.id===selectedInput?.id)selectedInput=null;bindMidiInputs();sendMidiState({disconnectedId:event.port?.id,permission:'granted'});});midiBound=true;}
async function connectMidi(callback) {
  midiCallback=callback;
  try {
    if(!WebMidi.enabled)await WebMidi.enable({sysex:false});
    bindMidiLifecycle();bindMidiInputs();choosePreferredInput();sendMidiState({permission:'granted'});
  } catch(error) { sendMidiState({error:error?.message||'MIDI permission was denied.',permission:'denied'}); }
}
async function autoConnectMidi(callback){midiCallback=callback;if(!navigator.requestMIDIAccess){sendMidiState({error:'Web MIDI is not available in this browser.',permission:'unsupported'});return;}try{if(!navigator.permissions?.query){sendMidiState({permission:'prompt'});return;}const status=await navigator.permissions.query({name:'midi',sysex:false});if(status.state==='granted')await connectMidi(callback);else sendMidiState({permission:status.state});status.onchange=()=>{if(status.state==='granted')connectMidi(callback);else sendMidiState({permission:status.state});};}catch{sendMidiState({permission:'prompt'});}}
function selectMidi(id){selectedInput=WebMidi.inputs.find(input=>input.id===id)||null;if(selectedInput)localStorage.setItem('rudiment-midi-input',id);sendMidiState({permission:'granted'});}

async function readFile(file,callback){if(!file)return;if(file.size>8*1024*1024){callback(new Uint8Array(),file.name,'Files larger than 8 MB are not supported.');return;}try{callback(new Uint8Array(await file.arrayBuffer()),file.name,'');}catch(error){callback(new Uint8Array(),file.name,error?.message||'Could not read file.');}}
function chooseMidi(callback){const input=document.createElement('input');input.type='file';input.accept='.mid,.midi,audio/midi';input.onchange=()=>readFile(input.files?.[0],callback);input.click();}
async function loadBundled(url,callback){try{const response=await fetch(url);if(!response.ok)throw new Error(`HTTP ${response.status}`);callback(new Uint8Array(await response.arrayBuffer()),url.split('/').pop(),'');}catch(error){callback(new Uint8Array(),url,error?.message||'Could not load bundled exercise.');}}
let dropController;
function bindDrop(callback){
  dropController?.abort();
  dropController = new AbortController();
  const element=document.getElementById('score-card');if(!element)return;
  const options={signal:dropController.signal};
  const over=event=>{event.preventDefault();element.classList.add('dragging');};
  element.addEventListener('dragover',over,options);
  element.addEventListener('dragleave',()=>element.classList.remove('dragging'),options);
  element.addEventListener('drop',event=>{event.preventDefault();element.classList.remove('dragging');readFile(event.dataTransfer?.files?.[0],callback);},options);
}
function downloadJson(value,name){const anchor=document.createElement('a');anchor.href=URL.createObjectURL(new Blob([value],{type:'application/json'}));anchor.download=name;anchor.click();setTimeout(()=>URL.revokeObjectURL(anchor.href),500);}

window.rudiment={
  ensureAudio:()=>audio.ensure(),audioState:()=>audio.state,audioDiagnostics:()=>({state:audio.state,activeVoices:audio.voices.size,layerPeaks:[...audio.layerPeaks],layerGains:[...audio.layerGains],velocityGains:Array.from({length:127},(_,index)=>velocityGain(index+1)),velocityLayers:Array.from({length:127},(_,index)=>velocityLayers(index+1))}),previewSnare:async()=>{await audio.ensure();audio.playSnare(112);},playSnare:velocity=>audio.playSnare(velocity),previewClick:async timbre=>{await audio.ensure();audio.click(audio.raw.currentTime+.03,true,timbre);},
  setVolumes:(snare,click)=>{if(audio.snareGain)audio.snareGain.gain.value=snare;if(audio.clickGain)audio.clickGain.gain.value=click;},startAudio:(options,frame,done)=>audio.start(options,frame,done),stopAudio:()=>audio.stop(),startCalibration:()=>audio.calibration(),
  renderScore,updateScore,connectMidi,autoConnectMidi,selectMidi,chooseMidi,loadBundled,bindDrop,
  saveSession:record=>put('sessions',record),loadSessions:()=>all('sessions'),clearHistory:()=>clear('sessions'),saveSettings:value=>put('settings',{key:'app',value}),loadSettings:async()=>(await (await dbPromise).get('settings','app'))?.value||{},saveProfile:profile=>put('profiles',profile),loadProfiles:()=>all('profiles'),resetLocalSetup:async()=>{await clear('settings');await clear('profiles');localStorage.removeItem('rudiment-midi-input');},
  saveImport:(hash,name,bytes,metadata)=>put('imports',{hash,name,bytes:Array.from(bytes),metadata,schemaVersion:1}),loadImports:()=>all('imports'),exportHistory:async()=>downloadJson(JSON.stringify(await all('sessions'),null,2),'rudiment-history.json')
};
