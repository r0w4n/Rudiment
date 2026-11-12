import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

const argument=(name,fallback)=>{const index=process.argv.indexOf(name);return index>=0?process.argv[index+1]:fallback;};
const currentPath=argument('--current','performance-results/fast.json'),baselinePath=argument('--baseline',null),output=argument('--output','performance-results/report.md');
const current=JSON.parse(await readFile(currentPath,'utf8')),baseline=baselinePath?JSON.parse(await readFile(baselinePath,'utf8')):null;
const rows=[];
for(const [name,metric] of Object.entries(current.metrics)){
  if(typeof metric.p95!=='number')continue;
  const before=baseline?.metrics?.[name]?.p95,change=typeof before==='number'&&before!==0?((metric.p95-before)/before)*100:null;
  rows.push(`| ${name} | ${before?.toFixed(2)??'—'} | ${metric.p50?.toFixed(2)??'—'} | ${metric.p95.toFixed(2)} | ${metric.p99?.toFixed(2)??'—'} | ${metric.max?.toFixed(2)??'—'} | ${change===null?'—':`${change>=0?'+':''}${change.toFixed(1)}%`} | ${metric.samples} |`);
}
const scenarios=current.scenarios.filter(item=>item.fixture).map(item=>`- ${item.fixture.id}: ${item.fixture.bars} bars, ${item.fixture.noteCount} notes, ${item.fixture.bpm} BPM${item.fixture.simultaneousEvery?' with simultaneous hits':''}`).join('\n');
const markdown=`# Rudiment performance report\n\nGenerated ${current.finishedAt} from commit \`${current.environment.commit}\`${current.environment.dirty?' (dirty worktree)':''}.\n\n## Environment and methodology\n\n- Tier: ${current.tier}; repeats: ${current.methodology.repeats}\n- Platform: ${current.environment.platform} ${current.environment.release} (${current.environment.arch}); ${current.environment.cpus} logical CPUs\n- Runtime: ${current.environment.node}\n- Direct observations: ${current.methodology.observed}\n- Estimates and limitations: ${current.methodology.estimated}\n\n## Scenarios\n\n${scenarios||'- No fixture loads recorded.'}\n\n## Measurements\n\nTimes are milliseconds. “Before” is the baseline p95; current percentile columns are from this run.\n\n| Metric | Before p95 | p50 | p95 | p99 | Max | p95 change | Samples |\n|---|---:|---:|---:|---:|---:|---:|---:|\n${rows.join('\n')}\n\n## Reproduction\n\n\`npm run perf:fast\` runs the PR-sized profile. \`npm run perf:full\` runs the complete workload matrix. \`npm run perf:endurance\` defaults to 15 minutes. Use \`npm run perf:trace\` for Playwright traces. Physical USB-MIDI and audible output latency require the hardware checklist in \`PERFORMANCE.md\`; simulated Web MIDI does not validate either.\n`;
const browserLabel=typeof current.environment.browser==='object'?`${current.environment.browser.channel} ${current.environment.browser.version}`:current.environment.browser;
const enrichedMarkdown=markdown
  .replace(`- Runtime: ${current.environment.node}`,`- Runtime: ${current.environment.node}; browser: ${browserLabel}`)
  .replace('Times are milliseconds. “Before”','Times are milliseconds except `heap_growth_bytes`, which is bytes. “Before”');
await mkdir(path.dirname(output),{recursive:true});await writeFile(output,enrichedMarkdown);console.log(`Markdown report: ${path.resolve(output)}`);

