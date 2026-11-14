import { spawnSync } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

const checks = {
  eslint: { label: 'JavaScript/TypeScript lint', command: 'npm', args: ['run', 'lint'] },
  format: { label: 'Repository formatting', command: 'npm', args: ['run', 'format:check'] },
  'rust-fmt': { label: 'Rust formatting', command: 'cargo', args: ['fmt', '--all', '--', '--check'] },
  'rust-check': {
    label: 'Rust static check',
    command: 'cargo',
    args: ['check', '--workspace', '--all-targets', '--locked'],
  },
  'rust-clippy': {
    label: 'Rust Clippy',
    command: 'cargo',
    args: ['clippy', '--workspace', '--all-targets', '--all-features', '--locked', '--', '-D', 'warnings'],
  },
  unit: { label: 'Rust unit and integration tests', command: 'cargo', args: ['test', '--workspace', '--locked'] },
  build: { label: 'Production WASM/browser build', command: 'npm', args: ['run', 'build'] },
  'npm-audit': {
    label: 'npm dependency audit',
    command: 'npm',
    args: ['audit', '--audit-level=high'],
  },
  'cargo-audit': {
    label: 'RustSec dependency audit',
    command: 'cargo',
    args: ['audit'],
    missingHint: 'Install it with `cargo install cargo-audit --locked`.',
  },
  browser: {
    label: 'Browser correctness tests',
    command: 'npx',
    args: [
      'playwright',
      'test',
      'tests/e2e/server.spec.ts',
      'tests/e2e/rudiment.spec.ts',
      'tests/e2e/performance-correctness.spec.ts',
    ],
    dependsOn: ['build'],
  },
  a11y: {
    label: 'WCAG accessibility automation',
    command: 'npx',
    args: ['playwright', 'test', 'tests/e2e/accessibility.spec.ts'],
    dependsOn: ['build'],
  },
  performance: {
    label: 'Fast performance absolute gates and diagnostics',
    command: 'npm',
    args: ['run', 'perf:fast:measure'],
    dependsOn: ['build'],
  },
};

const profiles = {
  quick: ['eslint', 'format', 'rust-fmt', 'rust-check', 'rust-clippy'],
  unit: ['unit'],
  security: ['npm-audit', 'cargo-audit'],
  browser: ['build', 'browser'],
  a11y: ['build', 'a11y'],
  performance: ['build', 'performance'],
  'pre-push': [
    'eslint',
    'format',
    'rust-fmt',
    'rust-check',
    'rust-clippy',
    'unit',
    'build',
    'npm-audit',
    'cargo-audit',
    'browser',
    'a11y',
  ],
  full: [
    'eslint',
    'format',
    'rust-fmt',
    'rust-check',
    'rust-clippy',
    'unit',
    'build',
    'npm-audit',
    'cargo-audit',
    'browser',
    'a11y',
    'performance',
  ],
};

const profile = process.argv[2] ?? 'quick';
const checksArgument = process.argv.find((value) => value.startsWith('--checks='));
const requested = checksArgument
  ? checksArgument.slice('--checks='.length).split(',').filter(Boolean)
  : profiles[profile];
if (!requested) {
  console.error(`Unknown QA profile: ${profile}. Available profiles: ${Object.keys(profiles).join(', ')}`);
  process.exit(2);
}
for (const name of requested) {
  if (!checks[name]) {
    console.error(`Unknown QA check: ${name}`);
    process.exit(2);
  }
}

const skipBuild = process.argv.includes('--skip-build');
const selected = requested.filter((name) => !(skipBuild && name === 'build'));
if (profile === 'pre-push' && process.env.RUDIMENT_PRE_PUSH_PERF === '1') selected.push('performance');

const resultDirectory = path.resolve('quality-results');
const logDirectory = path.join(resultDirectory, 'logs');
await mkdir(logDirectory, { recursive: true });
const startedAt = new Date();
const results = [];

const tail = (value, lines = 30) => value.trim().split('\n').slice(-lines).join('\n');

for (const name of selected) {
  const check = checks[name];
  const dependencyFailure = check.dependsOn?.find(
    (dependency) =>
      selected.includes(dependency) && results.find((result) => result.name === dependency)?.status !== 'passed',
  );
  if (dependencyFailure) {
    results.push({
      name,
      label: check.label,
      status: 'skipped',
      mandatory: true,
      durationMs: 0,
      reason: `Required check ${dependencyFailure} did not pass.`,
    });
    console.log(`SKIP ${check.label} (dependency ${dependencyFailure} failed)`);
    continue;
  }

  const checkStarted = Date.now();
  console.log(`RUN  ${check.label}`);
  const execution = spawnSync(check.command, check.args, {
    cwd: process.cwd(),
    encoding: 'utf8',
    env: { ...process.env, CI: process.env.CI ?? '1', FORCE_COLOR: '0' },
    maxBuffer: 50 * 1024 * 1024,
  });
  const output = `${execution.stdout ?? ''}${execution.stderr ?? ''}`;
  const logPath = path.join(logDirectory, `${name}.log`);
  await writeFile(logPath, output);
  const passed = execution.status === 0 && !execution.error;
  const reason = execution.error
    ? `${execution.error.message}${check.missingHint ? ` ${check.missingHint}` : ''}`
    : passed
      ? undefined
      : `Exited with status ${execution.status ?? 'unknown'}.`;
  results.push({
    name,
    label: check.label,
    status: passed ? 'passed' : 'failed',
    mandatory: true,
    durationMs: Date.now() - checkStarted,
    command: [check.command, ...check.args].join(' '),
    log: path.relative(process.cwd(), logPath),
    ...(reason ? { reason } : {}),
  });
  console.log(`${passed ? 'PASS' : 'FAIL'} ${check.label} (${((Date.now() - checkStarted) / 1000).toFixed(1)}s)`);
  if (!passed && output.trim()) console.error(tail(output));
  if (!passed && reason) console.error(reason);
}

const failed = results.filter((result) => result.status === 'failed');
const skippedMandatory = results.filter((result) => result.mandatory && result.status === 'skipped');
const finishedAt = new Date();
const report = {
  schemaVersion: 1,
  profile,
  startedAt: startedAt.toISOString(),
  finishedAt: finishedAt.toISOString(),
  durationMs: finishedAt - startedAt,
  environment: { platform: process.platform, arch: process.arch, node: process.version, release: os.release() },
  gate: failed.length === 0 && skippedMandatory.length === 0 ? 'passed' : 'failed',
  results,
};

const icon = (status) => ({ passed: 'PASS', failed: 'FAIL', skipped: 'SKIP' })[status] ?? status.toUpperCase();
const markdown = [
  `# Rudiment QA report — ${profile}`,
  '',
  `Overall gate: **${report.gate.toUpperCase()}**`,
  '',
  `Duration: ${(report.durationMs / 1000).toFixed(1)}s`,
  '',
  '| Check | Result | Duration | Diagnostic |',
  '|---|---:|---:|---|',
  ...results.map(
    (result) =>
      `| ${result.label} | ${icon(result.status)} | ${(result.durationMs / 1000).toFixed(1)}s | ${result.reason ?? result.log ?? ''} |`,
  ),
  '',
  `Generated ${finishedAt.toISOString()}.`,
  '',
].join('\n');

await writeFile(path.join(resultDirectory, 'latest.json'), `${JSON.stringify(report, null, 2)}\n`);
await writeFile(path.join(resultDirectory, 'latest.md'), markdown);
if (process.env.GITHUB_STEP_SUMMARY) await writeFile(process.env.GITHUB_STEP_SUMMARY, markdown, { flag: 'a' });

console.log(
  `\nQA ${report.gate.toUpperCase()}: ${results.filter((result) => result.status === 'passed').length} passed, ${failed.length} failed, ${results.filter((result) => result.status === 'skipped').length} skipped.`,
);
console.log(`Reports: quality-results/latest.{json,md}`);
if (report.gate !== 'passed') process.exitCode = 1;
