import { mkdir, writeFile } from 'node:fs/promises';

const vlq = n => { const out=[n&127]; while((n>>=7)) out.unshift((n&127)|128); return out; };
const text = (type, s) => [0xff,type,s.length,...Buffer.from(s)];
function midi(name, hands, notes, ambiguous=false) {
  const track=[...vlq(0),...text(0x03,name),...vlq(0),0xff,0x51,0x03,0x0f,0x42,0x40,
    ...vlq(0),0xff,0x58,0x04,0x04,0x02,0x18,0x08];
  for(let i=0;i<notes.length;i++) {
    // The prior note-off advances 120 ticks; another 120 reaches the next 240-tick eighth note.
    const delta=i===0?0:120;
    track.push(...vlq(delta),...text(0x05,hands[i] ?? '?'),...vlq(0),0x99,notes[i],i%8===0?120:92);
    if (ambiguous && i===0) track.push(...vlq(0),0x99,38,90);
    track.push(...vlq(120),0x89,notes[i],0);
  }
  track.push(...vlq(0),0xff,0x2f,0x00);
  const head=Buffer.from([0x4d,0x54,0x68,0x64,0,0,0,6,0,0,0,1,0x01,0xe0]);
  const th=Buffer.from([0x4d,0x54,0x72,0x6b,(track.length>>>24)&255,(track.length>>>16)&255,(track.length>>>8)&255,track.length&255]);
  return Buffer.concat([head,th,Buffer.from(track)]);
}
function quarterReference() {
  const track=[...vlq(0),...text(0x03,'Quarter Note Reference'),...vlq(0),0xff,0x51,0x03,0x0b,0x71,0xb0,...vlq(0),0xff,0x58,0x04,0x04,0x02,0x18,0x08];
  for(let i=0;i<8;i++){track.push(...vlq(i===0?0:370),...text(0x05,i%2===0?'R':'L'),...vlq(0),0x99,38,100,...vlq(110),0x89,38,0);}
  track.push(...vlq(0),0xff,0x2f,0x00);const head=Buffer.from([0x4d,0x54,0x68,0x64,0,0,0,6,0,0,0,1,0x01,0xe0]);const th=Buffer.from([0x4d,0x54,0x72,0x6b,(track.length>>>24)&255,(track.length>>>16)&255,(track.length>>>8)&255,track.length&255]);return Buffer.concat([head,th,Buffer.from(track)]);
}
function performanceMidi({name,bars,bpm=120,subdivision=4,simultaneousEvery=0}) {
  const microseconds=Math.round(60_000_000/bpm),spacing=Math.round(480/subdivision),gate=Math.max(1,Math.floor(spacing/2));
  const track=[...vlq(0),...text(0x03,name),...vlq(0),0xff,0x51,0x03,(microseconds>>>16)&255,(microseconds>>>8)&255,microseconds&255,
    ...vlq(0),0xff,0x58,0x04,0x04,0x02,0x18,0x08];
  const onsets=bars*4*subdivision;
  let noteCount=0;
  for(let i=0;i<onsets;i++){
    const notes=simultaneousEvery&&i%simultaneousEvery===0?[38,36]:[38];
    const advance=i===0?0:spacing-gate;
    track.push(...vlq(advance),...text(0x05,i%2===0?'R':'L'));
    notes.forEach((note,index)=>track.push(...vlq(0),0x99,note,i%16===0?120:92+index*4));
    notes.forEach((note,index)=>track.push(...vlq(index===0?gate:0),0x89,note,0));
    noteCount+=notes.length;
  }
  track.push(...vlq(0),0xff,0x2f,0x00);
  const head=Buffer.from([0x4d,0x54,0x68,0x64,0,0,0,6,0,0,0,1,0x01,0xe0]);
  const th=Buffer.from([0x4d,0x54,0x72,0x6b,(track.length>>>24)&255,(track.length>>>16)&255,(track.length>>>8)&255,track.length&255]);
  return {bytes:Buffer.concat([head,th,Buffer.from(track)]),noteCount};
}
await mkdir('web/public/exercises',{recursive:true});
await writeFile('web/public/exercises/single-paradiddle.mid',midi('Single Paradiddle','RLRRLRLL'.repeat(4).split(''),Array(32).fill(38)));
await writeFile('web/public/exercises/single-stroke-roll.mid',midi('Single Stroke Roll','RLRLRLRL'.repeat(4).split(''),Array(32).fill(38)));
await writeFile('web/public/exercises/ambiguous-sticking.mid',midi('Ambiguous Sticking',['R','L'],[38,38],true));
await writeFile('web/public/exercises/quarter-note-reference.mid',quarterReference());

const performanceFixtures={
  small:{name:'Performance Small',bars:4,bpm:60,subdivision:4},
  medium:{name:'Performance Medium',bars:30,bpm:120,subdivision:4},
  large:{name:'Performance Large',bars:125,bpm:180,subdivision:4},
  extreme:{name:'Performance Extreme',bars:625,bpm:240,subdivision:4},
  dense:{name:'Performance Dense',bars:64,bpm:240,subdivision:8,simultaneousEvery:4},
};
await mkdir('web/public/exercises/performance',{recursive:true});
const manifest={schemaVersion:1,generatedAt:'deterministic',fixtures:{}};
for(const [id,definition] of Object.entries(performanceFixtures)){
  const {bytes,noteCount}=performanceMidi(definition);
  await writeFile(`web/public/exercises/performance/${id}.mid`,bytes);
  manifest.fixtures[id]={...definition,noteCount};
}
await writeFile('web/public/exercises/performance/manifest.json',`${JSON.stringify(manifest,null,2)}\n`);

