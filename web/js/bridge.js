import * as Tone from 'tone';
import { WebMidi } from 'webmidi';
import { openDB } from 'idb';
import { Renderer, Stave, StaveNote, Voice, Formatter, Beam, Annotation, Articulation, BarlineType, Dot, Fraction, Tuplet } from 'vexflow';

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
async function loadSavedImport(hash){const item=await (await dbPromise).get('imports',hash);if(!item)throw new Error('This MIDI file is no longer in the local library.');if(!midiFileCallback)throw new Error('MIDI import is not ready yet.');midiFileCallback(new Uint8Array(item.bytes),item.name,'');}
async function saveLastImport(hash){const db=await dbPromise,current=await db.get('settings','app');await db.put('settings',{key:'app',value:{...(current?.value||{}),lastImportHash:hash||null}});}
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
function tempoAtBeat(beat,map,initialBpm){let bpm=initialBpm;for(const change of map){if(change.beat>beat)break;bpm=change.bpm;}return bpm;}
function transportPositionAtBeat(beat,meter){const bars=Math.floor(beat/meter),within=beat-bars*meter;return `${bars}:${within}:0`;}

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
    const transport = Tone.getTransport(), tempoMap=(options.tempoMap||[]).slice().sort((a,b)=>a.beat-b.beat),sourceBpm=options.sourceBpm||options.bpm,tempoScale=options.bpm/sourceBpm,offset = options.offsetBeat || 0,initialBpm=tempoAtBeat(offset,tempoMap,sourceBpm)*tempoScale,countBeats = options.countInBars * options.meter;
    const remaining = Math.max(0, options.totalBeats - offset), startDelay = .16;
    const countInSeconds=countBeats*60/initialBpm,runAudio = this.raw.currentTime + startDelay + countInSeconds;
    const runPerformance = this.perfAt(runAudio), subdivision = Math.max(1, options.subdivision || 1), interval = `${4*subdivision}n`;
    let step = 0; transport.bpm.value = initialBpm; transport.timeSignature = options.meter;
    for(const change of tempoMap){if(change.beat<=offset)continue;const position=transportPositionAtBeat(countBeats+change.beat-offset,options.meter);transport.scheduleOnce(time=>transport.bpm.setValueAtTime(change.bpm*tempoScale,time),position);}
    transport.scheduleRepeat(time => {
      const localBeat = step / subdivision, globalBeat = localBeat < countBeats ? localBeat : offset + localBeat - countBeats;
      if (options.metronome) this.click(time, Number.isInteger(globalBeat) && Math.round(globalBeat) % options.meter === 0, options.timbre);
      step++;
    }, interval, 0);
    transport.start(`+${startDelay}`);
    const frame = () => {
      if (generation !== this.generation) return;
      // Keep the score at the exact paused position during the short audio
      // scheduling lead-in. Without the clamp, resuming visibly jumps the
      // playhead backwards before the Tone transport starts.
      const now = performance.now(), elapsedBeats=transport.ticks/transport.PPQ;
      const beat = offset + elapsedBeats-countBeats; onFrame(beat, now, runPerformance);
      if (elapsedBeats >= countBeats+remaining) { this.stop(); onDone(); } else this.raf = requestAnimationFrame(frame);
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

const SCORE_TOP_MARGIN=42,SCORE_ROW_HEIGHT=196,SCORE_BOTTOM_MARGIN=24;
let scoreData = { notes:[], svg:null, totalBeats:16, meter:4, container:null, bars:[], viewportWidth:0, rowHeight:SCORE_ROW_HEIGHT, lastBeat:-Infinity, lastJudgements:[], judgementSignature:'' };
let scoreResizeObserver=null;
const handText = hand => hand === 'R' ? 'R' : hand === 'L' ? 'L' : '·';
const durationTable=[[4,'w',false,false],[3,'h',true,false],[2,'h',false,false],[1.5,'q',true,false],[1,'q',false,false],[.75,'8',true,false],[2/3,'q',false,true],[.5,'8',false,false],[.375,'16',true,false],[1/3,'8',false,true],[.25,'16',false,false],[1/6,'16',false,true],[.125,'32',false,false]];
const durationSpec = beats => durationTable.find(([value])=>Math.abs(value-beats)<.001)??durationTable.at(-1);
const writtenDuration = beats => {const [,duration,,tuplet]=durationSpec(beats);return tuplet?`${duration}t`:duration;};
function noteWithDuration(keys,beats,rest=false) {
  const [value,duration,dotted,tuplet]=durationSpec(beats),note=new StaveNote({keys,duration:`${duration}${rest?'r':''}`,clef:'percussion',stemDirection:1});
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
    35:'f/4',36:'f/4',37:'b/4/x',38:'b/4',39:'a/5/x',40:'b/4',
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
  return ({kick:'f/4',snare:'b/4',closed_hat:'g/5/x',open_hat:'g/5/x',pedal_hat:'g/5/x',ride:'d/5/x',crash:'a/5/x',tom:'c/5',other_percussion:'c/5/x'})[hit.instrument]??'b/4';
}
function instrumentForHit(hit) {
  if(hit.instrument)return hit.instrument;
  const note=Number(hit.note);
  if([35,36].includes(note))return 'kick';
  if([37,38,40].includes(note))return 'snare';
  if([42,44,46].includes(note))return note===46?'open_hat':note===44?'pedal_hat':'closed_hat';
  if([41,43,45,47,48,50].includes(note))return 'tom';
  if([51,53,59].includes(note))return 'ride';
  if([49,52,55,57].includes(note))return 'crash';
  if(note>=39&&note<=81)return 'other_percussion';
  return 'snare';
}
function renderScore(exercise) {
  const element = document.getElementById('notation'); if (!element) return; element.innerHTML = '';
  const isResize=scoreData.exercise===exercise;
  const previousBeat=isResize&&Number.isFinite(scoreData.lastBeat)?scoreData.lastBeat:-99;
  const previousJudgements=isResize?scoreData.lastJudgements:[];
  if(!isResize){element.scrollTop=0;element.scrollLeft=0;}
  const hits=exercise.hits,metersPerBar=exercise.meter_numerator*4/Math.max(1,exercise.meter_denominator);
  const lastBeat=hits.reduce((last,hit)=>Math.max(last,hit.notation_beat??hit.beat),0);
  const barCount=Math.max(1,Math.floor(lastBeat/metersPerBar)+1);
  const shortest=hits.length?Math.min(...hits.map(hit=>hit.notation_duration_beats||.25)):1;
  const minimumBarWidth=shortest<.4?270:shortest<.75?210:180;
  const singleLine=exercise.staff_mode==='single_line'&&hits.length>0&&hits.every(hit=>instrumentForHit(hit)==='snare');
  const width=Math.max(280,element.clientWidth),availableWidth=Math.max(240,width-28);
  const renderer=new Renderer(element,Renderer.Backends.SVG);
  const notesMeta=[],barLayouts=[],staves=[],barSlots=new Map(),beams=[],tuplets=[],eventHitsByNote=new Map();
  const firstStave=new Stave(14,SCORE_TOP_MARGIN,availableWidth);
  firstStave.setConfigForLines(Array.from({length:5},(_,index)=>({visible:!singleLine||index===2})));
  firstStave.setBegBarType(BarlineType.NONE).addClef('percussion').addTimeSignature(`${exercise.meter_numerator}/${exercise.meter_denominator}`);
  const headerWidth=firstStave.getNoteStartX()-14;
  const musicWidth=Math.max(120,availableWidth-headerWidth);
  const barsPerLine=Math.max(1,Math.min(3,barCount,Math.floor(musicWidth/minimumBarWidth)));
  const barWidth=musicWidth/barsPerLine,lineCount=Math.ceil(barCount/barsPerLine);
  const height=SCORE_TOP_MARGIN+lineCount*SCORE_ROW_HEIGHT+SCORE_BOTTOM_MARGIN;
  renderer.resize(width,height);
  const context=renderer.getContext(),systemBarCounts=[],systemWidths=[];
  const beamGroups=exercise.meter_numerator>=6&&exercise.meter_numerator%3===0
    ?[new Fraction(3,Math.max(1,exercise.meter_denominator))]
    :[new Fraction(1,Math.max(1,exercise.meter_denominator))];
  for(let row=0;row<lineCount;row++){
    const rowBarCount=Math.min(barsPerLine,barCount-row*barsPerLine),staveWidth=headerWidth+rowBarCount*barWidth;
    const stave=row===0?firstStave:new Stave(14,SCORE_TOP_MARGIN+row*SCORE_ROW_HEIGHT,staveWidth);
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
    barLayouts.push({index:bar,startBeat:bar*metersPerBar,endBeat:(bar+1)*metersPerBar,row,xStart:stave.getNoteStartX(),xEnd:stave.getNoteEndX(),y:stave.getY(),y1:stave.getYForLine(firstLine)-12,y2:stave.getYForLine(lastLine)+12,anchors:[]});
  }
  for(let row=0;row<lineCount;row++){
    const firstBar=row*barsPerLine,lastBar=Math.min(barCount,firstBar+barsPerLine),rowTickables=[];
    for(let bar=firstBar;bar<lastBar;bar++){
      const start=bar*metersPerBar,end=start+metersPerBar,slots=[];
      const groups=new Map();
      for(const hit of hits){
        const position=hit.notation_beat??hit.beat;
        if(position<start-.001||position>=end-.001)continue;
        const key=position.toFixed(4);
        if(!groups.has(key))groups.set(key,{position,hits:[]});
        groups.get(key).hits.push(hit);
      }
      const ordered=[...groups.values()].sort((a,b)=>a.position-b.position);let cursor=start;
      for(let index=0;index<ordered.length;index++){
        const group=ordered[index],next=ordered[index+1]?.position??end;
        if(group.position>cursor+.001){
          for(const value of restValues(group.position-cursor)){slots.push(noteWithDuration(['b/4'],value,true));cursor+=value;}
        }
        const keys=[...new Set(group.hits.map(pitchForHit))];
        const requestedDuration=Math.min(...group.hits.map(hit=>hit.notation_duration_beats||.25));
        const value=Math.min(requestedDuration,next-group.position,end-group.position);
        const note=noteWithDuration(keys,value);
        eventHitsByNote.set(note,group.hits);
        const hands=[...new Set(group.hits.map(hit=>hit.hand).filter(hand=>hand==='R'||hand==='L'))];
        if(hands.length===1)note.addModifier(new Annotation(handText(hands[0])).setVerticalJustification(1).setFont('Inter',11,'700'),0);
        if(group.hits.some(hit=>hit.accent))note.addModifier(new Articulation('a>').setPosition(3),0);
        slots.push(note);
        for(const hit of group.hits)notesMeta.push({note,beat:hit.beat,notationBeat:group.position,durationBeats:value,hit,row,bar});
        cursor=group.position+value;
      }
      if(cursor<end-.001)for(const value of restValues(end-cursor))slots.push(noteWithDuration(['b/4'],value,true));
      barSlots.set(bar,slots);rowTickables.push(...slots);
      tuplets.push(...tupletsFor(slots));
      beams.push(...Beam.generateBeams(slots,{groups:beamGroups,stemDirection:1}));
    }
    const voice=new Voice({numBeats:(lastBar-firstBar)*metersPerBar,beatValue:4}).setStrict(false);
    voice.addTickables(rowTickables);
    new Formatter().joinVoices([voice]).format([voice],staves[row].getNoteEndX()-staves[row].getNoteStartX());
    voice.draw(context,staves[row]);
    for(const tickable of rowTickables)if(tickable.isRest?.())tickable.getSVGElement?.()?.setAttribute('data-score-rest','true');
  }
  for(const beam of beams)beam.setContext(context).draw();
  for(const tuplet of tuplets)tuplet.setContext(context).draw();
  const svg=element.querySelector('svg');
  for(const [bar,slots] of barSlots)barLayouts[bar].xStart=slots[0]?.getAbsoluteX()??barLayouts[bar].xStart;
  for(let bar=0;bar<barCount-1;bar++){
    if(barLayouts[bar].row!==barLayouts[bar+1].row)continue;
    const previous=barSlots.get(bar)?.at(-1),next=barSlots.get(bar+1)?.[0];
    if(!previous||!next)continue;
    const stave=staves[barLayouts[bar].row],x=(previous.getAbsoluteX()+next.getAbsoluteX())/2;
    const barline=document.createElementNS('http://www.w3.org/2000/svg','line');
    barline.setAttribute('class','score-barline');barline.setAttribute('x1',String(x));barline.setAttribute('x2',String(x));
    barline.setAttribute('y1',String(singleLine?stave.getYForLine(2)-9:stave.getYForLine(0)));
    barline.setAttribute('y2',String(singleLine?stave.getYForLine(2)+9:stave.getYForLine(4)));
    barline.setAttribute('stroke','#343a36');barline.setAttribute('stroke-width','1.2');svg?.appendChild(barline);
  }
  for(let bar=0;bar<barCount;bar++){
    const stave=staves[barLayouts[bar].row],label=document.createElementNS('http://www.w3.org/2000/svg','text');
    const x=bar===0||bar%barsPerLine===0?stave.getX()+6:barLayouts[bar].xStart-8;
    label.setAttribute('class','score-measure-number');label.setAttribute('x',String(x));
    label.setAttribute('y',String(stave.getYForLine(0)-12));label.setAttribute('text-anchor',bar===0||bar%barsPerLine===0?'start':'end');
    label.setAttribute('font-family','Inter, sans-serif');label.setAttribute('font-size','10');label.setAttribute('font-weight','600');
    label.setAttribute('fill','#666d68');label.textContent=String(bar+1);svg?.appendChild(label);
  }
  if (svg) { svg.setAttribute('role','img'); svg.setAttribute('aria-label',`${exercise.title} percussion notation with sticking`); }
  if(svg){
    svg.dataset.barsPerLine=String(barsPerLine);svg.dataset.staffLines=singleLine?'1':'5';
    svg.dataset.systemBarCounts=JSON.stringify(systemBarCounts);svg.dataset.systemWidths=JSON.stringify(systemWidths);
    svg.dataset.staffBounds=JSON.stringify(staves.map(stave=>({top:stave.getYForLine(singleLine?2:0),bottom:stave.getYForLine(singleLine?2:4)})));
  }
  for (const meta of notesMeta) {
    const group=meta.note.getSVGElement?.();
    if(group){
      const eventHits=eventHitsByNote.get(meta.note)??[meta.hit];
      group.dataset.beat=String(meta.beat);group.dataset.writtenDuration=writtenDuration(meta.durationBeats);group.dataset.systemRow=String(meta.row);group.dataset.scoreBar=String(meta.bar);
      group.dataset.dotted=String(durationSpec(meta.durationBeats)[2]);
      group.dataset.tuplet=String(durationSpec(meta.durationBeats)[3]);
      group.dataset.midiNotes=eventHits.map(candidate=>candidate.note??38).join(',');
      group.dataset.instruments=[...new Set(eventHits.map(instrumentForHit))].join(',');
      group.classList.add('score-note');
      const head=group.querySelector('.vf-notehead > text'),bounds=head?.getBBox();
      if(bounds)barLayouts[meta.bar].anchors.push({beat:meta.beat,x:bounds.x+bounds.width/2});
    }
  }
  for(let index=0;index<barLayouts.length-1;index++){
    const bar=barLayouts[index],nextBar=barLayouts[index+1];
    if(bar.row!==nextBar.row)continue;
    const nextStart=nextBar.anchors.find(anchor=>Math.abs(anchor.beat-nextBar.startBeat)<.001);
    if(nextStart)bar.xEnd=nextStart.x;
  }
  for(const bar of barLayouts){
    const anchors=new Map([[bar.startBeat.toFixed(4),{beat:bar.startBeat,x:bar.xStart}],[bar.endBeat.toFixed(4),{beat:bar.endBeat,x:bar.xEnd}]]);
    for(const anchor of bar.anchors)anchors.set(anchor.beat.toFixed(4),anchor);
    bar.anchors=[...anchors.values()].sort((a,b)=>a.beat-b.beat);
  }
  const markers=document.createElementNS('http://www.w3.org/2000/svg','g');markers.id='unmatched-markers';markers.setAttribute('aria-label','Unmatched physical strikes');svg?.appendChild(markers);
  const line = document.createElementNS('http://www.w3.org/2000/svg','line'); line.id='playhead'; line.setAttribute('x1','0');line.setAttribute('x2','0');line.setAttribute('class','playhead-line'); svg?.appendChild(line);
  scoreData={notes:notesMeta,svg,totalBeats:barCount*metersPerBar,meter:metersPerBar,container:element,bars:barLayouts,viewportWidth:width,rowHeight:SCORE_ROW_HEIGHT,exercise,lastBeat:-Infinity,lastJudgements:[],judgementSignature:''};
  applyJudgements(previousJudgements);updateScore(previousBeat,null);
  if(typeof ResizeObserver!=='undefined'){
    scoreResizeObserver??=new ResizeObserver(entries=>{
      const observedWidth=entries[0]?.contentRect.width??scoreData.container?.clientWidth??0;
      if(scoreData.exercise&&Math.abs(observedWidth-scoreData.viewportWidth)>1)renderScore(scoreData.exercise);
    });
    scoreResizeObserver.observe(element);
  }
}
function positionAtBeat(beat) {
  const bars=scoreData.bars;if(!bars.length)return {x:0,y:0,row:0,bar:0};
  const value=Math.max(0,Math.min(scoreData.totalBeats,beat));
  const index=Math.min(bars.length-1,Math.floor(value/scoreData.meter));
  const bar=bars[index],anchors=bar.anchors;
  let before=anchors[0],after=anchors.at(-1);
  for(const anchor of anchors){if(anchor.beat<=value)before=anchor;if(anchor.beat>=value){after=anchor;break;}}
  const ratio=Math.max(0,Math.min(1,(value-before.beat)/Math.max(.001,after.beat-before.beat)));
  return {x:before.x+(after.x-before.x)*ratio,y:bar.y,y1:bar.y1,y2:bar.y2,row:bar.row,bar:index};
}
function applyJudgements(judgements) {
  const signature=JSON.stringify(judgements);if(signature===scoreData.judgementSignature)return;scoreData.judgementSignature=signature;
  scoreData.lastJudgements=judgements;
  for(const meta of scoreData.notes){const group=meta.note.getSVGElement?.();if(!group)continue;delete group.dataset.judgement;group.classList.remove('excellent-flash');group.querySelector('.judgement-halo,.missed-note-marker')?.remove();}
  for(const result of judgements){
    if(Number.isFinite(result.expected_beat)){
      const meta=scoreData.notes.find(note=>Math.abs(note.beat-result.expected_beat)<.001),group=meta?.note.getSVGElement?.();if(!group)continue;
      const judgement=result.excellent?'excellent':result.judgement;
      group.dataset.judgement=judgement;
      if(judgement==='missed'){
        const notehead=group.querySelector('.vf-notehead'),head=notehead?.querySelector(':scope > text');
        if(notehead&&head){
          const bounds=head.getBBox(),marker=document.createElementNS('http://www.w3.org/2000/svg','circle');
          marker.setAttribute('class','missed-note-marker');marker.setAttribute('cx',String(bounds.x+bounds.width/2));marker.setAttribute('cy',String(bounds.y+bounds.height/2));marker.setAttribute('r',String(Math.max(3,bounds.width*.36)));
          notehead.insertBefore(marker,head);
        }
      }
      if(['on_time','excellent','early','late'].includes(judgement)){
        const notehead=group.querySelector('.vf-notehead'),head=notehead?.querySelector(':scope > text');
        if(notehead&&head){
          const bounds=head.getBBox(),halo=document.createElementNS('http://www.w3.org/2000/svg','circle');
          halo.setAttribute('class','judgement-halo');halo.setAttribute('cx',String(bounds.x+bounds.width/2));halo.setAttribute('cy',String(bounds.y+bounds.height/2));halo.setAttribute('r',String(Math.max(6,bounds.width*.72)));
          notehead.insertBefore(halo,head);
          if(judgement==='excellent'){
            group.classList.add('excellent-flash');
          }
        }
      }
    }
  }
  const markers=scoreData.svg.querySelector('#unmatched-markers');markers?.replaceChildren();
  for(const result of judgements.filter(item=>!Number.isFinite(item.expected_beat)&&Number.isFinite(item.actual_beat)&&(item.judgement==='extra'||item.judgement==='wrong_instrument'))){
    const position=positionAtBeat(result.actual_beat),circle=document.createElementNS('http://www.w3.org/2000/svg','circle');circle.setAttribute('class','unmatched-hit-marker');circle.setAttribute('cx',position.x);circle.setAttribute('cy',position.y+20);circle.setAttribute('r','4');circle.dataset.beat=String(result.actual_beat);circle.dataset.systemRow=String(position.row);circle.dataset.judgement=result.judgement;circle.setAttribute('aria-label',`Unmatched strike at beat ${Number(result.actual_beat).toFixed(2)}`);markers?.appendChild(circle);
  }
}
function updateScore(beat, judgements) {
  const {notes,svg,container}=scoreData; if(!svg||!notes.length)return;
  if(Array.isArray(judgements))applyJudgements(judgements);
  scoreData.lastBeat=beat;
  const position=positionAtBeat(beat),playhead=svg.querySelector('#playhead');
  if(playhead){playhead.style.transform=`translate3d(${position.x}px,0,0)`;playhead.setAttribute('y1',String(position.y1));playhead.setAttribute('y2',String(position.y2));playhead.dataset.x=String(position.x);playhead.dataset.row=String(position.row);playhead.dataset.bar=String(position.bar);playhead.classList.toggle('visible',beat>=0&&beat<=scoreData.totalBeats);}
  if(beat>=0){
    const target=Math.min(position.row*scoreData.rowHeight,Math.max(0,container.scrollHeight-container.clientHeight));
    const distance=target-container.scrollTop;
    if(container.scrollLeft!==0)container.scrollLeft=0;
    container.scrollTop=Math.abs(distance)<.5?target:container.scrollTop+distance*.14;
  }
}
function clearScoreFeedback() {
  const empty=[];
  for(const meta of scoreData.notes){
    const group=meta.note.getSVGElement?.();if(!group)continue;
    delete group.dataset.judgement;group.classList.remove('excellent-flash');
    group.querySelector('.missed-note-marker')?.remove();
    group.getAnimations?.().forEach(animation=>animation.cancel());
  }
  scoreData.judgementSignature=JSON.stringify(empty);scoreData.lastJudgements=empty;
  scoreData.lastBeat=-Infinity;
  scoreData.svg?.querySelector('#unmatched-markers')?.replaceChildren();
  scoreData.container.scrollTop=0;scoreData.container.scrollLeft=0;
  updateScore(-99,empty);
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
function chooseMidi(callback){midiFileCallback=callback;const input=document.createElement('input');input.type='file';input.accept='.mid,.midi,audio/midi';input.onchange=()=>readFile(input.files?.[0],callback);input.click();}
async function loadBundled(url,callback){try{const response=await fetch(url);if(!response.ok)throw new Error(`HTTP ${response.status}`);callback(new Uint8Array(await response.arrayBuffer()),url.split('/').pop(),'');}catch(error){callback(new Uint8Array(),url,error?.message||'Could not load bundled exercise.');}}
let dropController;
let midiFileCallback;
function bindDrop(callback){
  midiFileCallback=callback;
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
  ensureAudio:()=>audio.ensure(),audioState:()=>audio.state,audioDiagnostics:()=>({state:audio.state,activeVoices:audio.voices.size,layerPeaks:[...audio.layerPeaks],layerGains:[...audio.layerGains],velocityGains:Array.from({length:127},(_,index)=>velocityGain(index+1)),velocityLayers:Array.from({length:127},(_,index)=>velocityLayers(index+1))}),playSnare:velocity=>audio.playSnare(velocity),previewClick:async timbre=>{await audio.ensure();audio.click(audio.raw.currentTime+.03,true,timbre);},
  setVolumes:(snare,click)=>{if(audio.snareGain)audio.snareGain.gain.value=snare;if(audio.clickGain)audio.clickGain.gain.value=click;},startAudio:(options,frame,done)=>audio.start(options,frame,done),stopAudio:()=>audio.stop(),startCalibration:()=>audio.calibration(),
  renderScore,updateScore,clearScoreFeedback,connectMidi,autoConnectMidi,selectMidi,chooseMidi,loadBundled,bindDrop,
  saveSession:record=>put('sessions',record),loadSessions:()=>all('sessions'),clearHistory:()=>clear('sessions'),saveSettings:value=>put('settings',{key:'app',value}),loadSettings:async()=>(await (await dbPromise).get('settings','app'))?.value||{},saveProfile:profile=>put('profiles',profile),loadProfiles:()=>all('profiles'),resetLocalSetup:async()=>{await clear('settings');await clear('profiles');localStorage.removeItem('rudiment-midi-input');},
  saveImport:(hash,name,bytes,metadata)=>put('imports',{hash,name,bytes:Array.from(bytes),metadata,schemaVersion:1}),loadImports:()=>all('imports'),loadImport:hash=>(async()=>{const db=await dbPromise;return db.get('imports',hash);})(),loadSavedImport,deleteImport:hash=>(async()=>{const db=await dbPromise;return db.delete('imports',hash);})(),saveLastImport,exportHistory:async()=>downloadJson(JSON.stringify(await all('sessions'),null,2),'rudiment-history.json')
};
