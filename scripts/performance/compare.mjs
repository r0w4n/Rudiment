import { readFile } from 'node:fs/promises';

const [baselinePath,currentPath]=process.argv.slice(2).filter(value=>!value.startsWith('--'));
if(!baselinePath||!currentPath)throw new Error('Usage: node scripts/performance/compare.mjs <baseline.json> <current.json> [--enforce]');
const baseline=JSON.parse(await readFile(baselinePath,'utf8')),current=JSON.parse(await readFile(currentPath,'utf8'));
const budgets=JSON.parse(await readFile('tests/performance/budgets.json','utf8'));
const failures=[];
if(baseline.schemaVersion!==current.schemaVersion)failures.push(`report schema mismatch (${baseline.schemaVersion} versus ${current.schemaVersion})`);
if(baseline.tier!==current.tier)failures.push(`tier mismatch (${baseline.tier} versus ${current.tier})`);
const scenarioSignature=report=>JSON.stringify((report.scenarios??[]).map(scenario=>({
  fixture:scenario.fixture?.id??null,
  expectedOnsets:scenario.fixture?.expectedOnsets??null,
  repeat:scenario.repeat??null,
  interaction:scenario.fixtureId??null,
})));
const matchingWorkloads=scenarioSignature(baseline)===scenarioSignature(current);
if(!matchingWorkloads)failures.push('candidate and baseline scenario workloads differ');
const browserMajor=value=>{
  const version=typeof value==='object'?value?.version:null;
  const match=String(version??'').match(/^(\d+)/);
  return match?.[1]??null;
};
const comparableEnvironment=baseline.environment.platform===current.environment.platform&&baseline.environment.arch===current.environment.arch&&browserMajor(baseline.environment.browser)!==null&&browserMajor(baseline.environment.browser)===browserMajor(current.environment.browser)&&matchingWorkloads;
if(!comparableEnvironment)console.log(`Relative regression checks are diagnostic only: baseline is ${baseline.environment.platform}/${baseline.environment.arch}/Chrome ${browserMajor(baseline.environment.browser)??'unknown'}, current is ${current.environment.platform}/${current.environment.arch}/Chrome ${browserMajor(current.environment.browser)??'unknown'}, and workload match is ${matchingWorkloads}. Absolute budgets and required-result validation remain enforced.`);
for(const [name,budget] of Object.entries(budgets.metrics)){
  const before=baseline.metrics[name],after=current.metrics[name];
  if(!after||typeof after.p95!=='number'||!Number.isFinite(after.p95)){
    failures.push(`${name} is a required metric but has no finite p95 result`);
    continue;
  }
  if(after.samples<budget.minimumSamples)failures.push(`${name} has ${after.samples} samples; at least ${budget.minimumSamples} are required`);
  const change=before?.p95?((after.p95-before.p95)/before.p95)*100:null;
  console.log(`${name}: baseline p95=${before?.p95?.toFixed(2)??'n/a'} ms, current p95=${after.p95.toFixed(2)} ms, change=${change===null?'n/a':`${change.toFixed(1)}%`}`);
  if(budget.absoluteP95Ms&&after.p95>budget.absoluteP95Ms)failures.push(`${name} p95 ${after.p95.toFixed(2)} ms exceeds ${budget.absoluteP95Ms} ms`);
  if(comparableEnvironment&&before&&before.samples>=budget.minimumSamples&&after.samples>=budget.minimumSamples&&change>budget.regressionPercent&&after.p95-before.p95>budget.minimumAbsoluteRegressionMs)failures.push(`${name} regressed ${change.toFixed(1)}% (${before.p95.toFixed(2)} → ${after.p95.toFixed(2)} ms)`);
}
if(failures.length){console.error(`\nPerformance validation failures:\n- ${failures.join('\n- ')}`);if(process.argv.includes('--enforce'))process.exitCode=1;}

