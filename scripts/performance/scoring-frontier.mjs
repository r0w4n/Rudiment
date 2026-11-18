// Isolated browser/WASM study: this does not change or benchmark the app's
// production MIDI callback. Build the opt-in WASM example before running it.
import { chromium } from 'playwright';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import os from 'node:os';
import path from 'node:path';

const wasm = await readFile('target/wasm32-unknown-unknown/release/examples/scoring_frontier.wasm');
const output = path.resolve(process.argv[2] ?? 'performance-results/scoring-frontier-browser.json');
const browser = await chromium.launch({ channel: 'chrome', headless: true, args: ['--enable-precise-memory-info'] });
const page = await browser.newPage();
await page.goto('data:text/html,<title>isolated scoring study</title>');
const scenarios = [
  { count: 500, spacing: 1000 }, { count: 500, spacing: 250 },
  { count: 500, spacing: 125 }, { count: 500, spacing: 62.5 },
  { count: 1000, spacing: 31.25 }, { count: 2000, spacing: 62.5 },
  { count: 10000, spacing: 62.5 }, { count: 2000, spacing: 62.5, gap: true },
  { count: 10000, spacing: 62.5, gap: true }, { count: 500, spacing: 0 },
  { count: 500, spacing: 62.5, gap: true, frames: true },
];
try {
  const runs = await page.evaluate(async ({ encoded, scenarios }) => {
    const bytes = Uint8Array.from(atob(encoded), char => char.charCodeAt(0));
    const module = await WebAssembly.compile(bytes);
    const runs = [];
    const longTasks = [];
    const observer = new PerformanceObserver(list => longTasks.push(...list.getEntries().map(entry => entry.duration)));
    observer.observe({ type: 'longtask', buffered: true });
    const nextFrame = () => new Promise(resolve => requestAnimationFrame(resolve));
    for (const scenario of scenarios) {
      for (let repeat = 0; repeat < 3; repeat++) {
        const instance = await WebAssembly.instantiate(module);
        const api = instance.exports;
        const started = performance.now();
        api.frontier_setup(scenario.count, scenario.spacing);
        const setupMs = performance.now() - started;
        const samples = [], frameSamples = [], batches = [];
        let failedAt = null, status = 0, firstConfirmed = null, firstMiss = null, missDelayMs = null;
        let batchStart = performance.now(), frameTime = 0;
        for (let index = 0; index < scenario.count; index++) {
          if (scenario.frames) {
            while (frameTime < index * scenario.spacing + 5) {
              const start = performance.now();
              status = api.frontier_advance(frameTime);
              frameSamples.push(performance.now() - start);
              if (status !== 0) { failedAt = index; break; }
              if (missDelayMs === null && api.frontier_misses() > 0) missDelayMs = frameTime - Math.floor(scenario.count / 4) * scenario.spacing;
              frameTime += 1000 / 60;
            }
            if (status !== 0) break;
          }
          if (scenario.gap && index === Math.floor(scenario.count / 4)) continue;
          const start = performance.now();
          status = api.frontier_hit(index, scenario.spacing);
          samples.push(performance.now() - start);
          if (status !== 0) { failedAt = index; break; }
          if (firstConfirmed === null && api.frontier_confirmed() > 0) firstConfirmed = index;
          if (firstMiss === null && api.frontier_misses() > 0) firstMiss = index;
          if (scenario.frames && missDelayMs === null && api.frontier_misses() > 0) missDelayMs = index * scenario.spacing + 5 - Math.floor(scenario.count / 4) * scenario.spacing;
          if (index % 256 === 255) {
            batches.push(performance.now() - batchStart);
            await nextFrame();
            batchStart = performance.now();
          }
        }
        batches.push(performance.now() - batchStart);
        const confirmedBeforeFinish = api.frontier_confirmed();
        const missesBeforeFinish = api.frontier_misses();
        const pending = api.frontier_pending(), nodes = api.frontier_nodes();
        if (scenario.frames && status === 0) {
          const end = (scenario.count - 1) * scenario.spacing + 120;
          while (frameTime <= end) {
            const start = performance.now();
            status = api.frontier_advance(frameTime);
            frameSamples.push(performance.now() - start);
            if (status !== 0) break;
            if (missDelayMs === null && api.frontier_misses() > 0) missDelayMs = frameTime - Math.floor(scenario.count / 4) * scenario.spacing;
            frameTime += 1000 / 60;
          }
        }
        const beforeFinish = performance.now();
        const finishStatus = status === 0 ? api.frontier_finish() : null;
        const finishMs = performance.now() - beforeFinish;
        runs.push({ ...scenario, repeat: repeat + 1, setupMs, samples, frameSamples, batches, failedAt, status,
          firstConfirmed, firstMiss, confirmedBeforeFinish, missesBeforeFinish, pending, nodes,
          missDelayMs, finishStatus, finishMs, wasmMemoryBytes: api.memory.buffer.byteLength });
        await nextFrame();
      }
    }
    await nextFrame();
    observer.disconnect();
    return { runs, longTasks, precision: 'performance.now() is quantized by Chrome; zero values are below its resolution.' };
  }, { encoded: wasm.toString('base64'), scenarios });
  const result = {
    schemaVersion: 1,
    methodology: 'Isolated experimental WASM on the Chrome main thread; 256-hit synthetic batches yield to requestAnimationFrame. Not integrated with MIDI or audio.',
    environment: { platform: process.platform, arch: process.arch, release: os.release(), node: process.version,
      chrome: browser.version(), commit: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim() },
    ...runs,
  };
  await mkdir(path.dirname(output), { recursive: true });
  await writeFile(output, `${JSON.stringify(result, null, 2)}\n`);
  const p = (values, fraction) => {
    const ordered = [...values].sort((a, b) => a - b);
    return ordered[Math.ceil((ordered.length - 1) * fraction)] ?? null;
  };
  for (const scenario of scenarios) {
    const subset = result.runs.filter(run => run.count === scenario.count && run.spacing === scenario.spacing && Boolean(run.gap) === Boolean(scenario.gap) && Boolean(run.frames) === Boolean(scenario.frames));
    const samples = subset.flatMap(run => run.samples), frames = subset.flatMap(run => run.frameSamples), batches = subset.flatMap(run => run.batches);
    console.log(JSON.stringify({ ...scenario, runs: subset.length, samples: samples.length,
      p50Ms: p(samples, .50), p95Ms: p(samples, .95), p99Ms: p(samples, .99), maxMs: p(samples, 1),
      maxBatchMs: p(batches, 1), maxFinishMs: Math.max(...subset.map(run => run.finishMs)),
      failedAt: subset.map(run => run.failedAt), confirmedBeforeFinish: subset.map(run => run.confirmedBeforeFinish),
      missesBeforeFinish: subset.map(run => run.missesBeforeFinish), maxPending: Math.max(...subset.map(run => run.pending)),
      frameP95Ms: p(frames, .95), missDelayMs: subset.map(run => run.missDelayMs),
      maxWasmMemoryBytes: Math.max(...subset.map(run => run.wasmMemoryBytes)) }));
  }
  console.log(`Long Tasks: ${JSON.stringify(result.longTasks)}; report: ${output}`);
} finally {
  await browser.close();
}
