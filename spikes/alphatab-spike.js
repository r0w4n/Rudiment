import * as alphaTab from '@coderline/alphatab';

// The exact normalized shape produced by rudiment-core for the bundled MIDI fixture.
const hands='RLRRLRLL'.repeat(4).split('');
const hits=hands.map((hand,i)=>({beat:i*.5,hand,accent:i%8===0||i%8===4,note:38}));
const beat=h=>`SnareHit2@3${h.accent?'{hac}':''} {txt "${h.hand}"}`;
const bars=[]; for(let b=0;b<4;b++) bars.push(`:8 ${hits.slice(b*8,b*8+8).map(beat).join(' ')} |`);
const tex=`\\title "Single Paradiddle" \\tempo 60 \\track "Snare" { instrument percussion } \\staff { score } \\clef neutral \\articulation defaults ${bars.join('\n')}`;
const status=document.getElementById('status');
try {
  const api=new alphaTab.AlphaTabApi(document.getElementById('score'),{
    core:{fontDirectory:'/font/',includeNoteBounds:true,useWorkers:false},
    display:{layoutMode:alphaTab.LayoutMode.Horizontal,scale:.9,stretchForce:.75},
    player:{playerMode:alphaTab.PlayerMode.EnabledExternalMedia,scrollMode:alphaTab.ScrollMode.Continuous}
  });
  api.renderFinished.on(()=>{
    const score=api.score; const rendered=!!document.querySelector('#score canvas, #score svg');
    const beats=score?.tracks?.[0]?.staves?.[0]?.bars?.flatMap(b=>b.voices[0].beats)||[];
    status.textContent=`rendered=${rendered}; bars=${score?.masterBars?.length||0}; beats=${beats.length}; externalOutput=${!!api.player?.output?.updatePosition}`;
    window.__alphaTabSpike={api,rendered,bars:score?.masterBars?.length||0,beats:beats.length,
      externalUpdate:typeof api.player?.output?.updatePosition==='function', beatStylePresent:!!beats[0]?.style,
      requiresRelayoutForFeedback:true};
  });
  api.error.on(e=>{status.textContent=`error: ${e.message||e}`;window.__alphaTabSpike={error:String(e.message||e)};});
  api.tex(tex);
} catch(e) { status.textContent=`error: ${e.message}`; window.__alphaTabSpike={error:e.message}; }
