import { spawnSync } from 'node:child_process';

const staged = spawnSync('git', ['diff', '--cached', '--name-only', '--diff-filter=ACMR', '-z'], {
  encoding: 'utf8',
});
if (staged.status !== 0) {
  process.stderr.write(staged.stderr);
  process.exit(staged.status ?? 1);
}
const files = staged.stdout.split('\0').filter(Boolean);
if (!files.length) {
  console.log('No staged files require QA checks.');
  process.exit(0);
}

const selected = new Set();
if (files.some((file) => /(^|\/)(Cargo\.(toml|lock)|[^/]+\.rs)$/.test(file))) {
  selected.add('rust-fmt');
  selected.add('rust-check');
}
if (files.some((file) => /\.(?:js|mjs|ts)$/.test(file) || /(^|\/)package(?:-lock)?\.json$/.test(file))) {
  selected.add('eslint');
  selected.add('format');
}
if (files.some((file) => /\.(?:md|json|ya?ml)$/.test(file))) selected.add('format');
if (!selected.size) {
  console.log(`No configured fast checks apply to ${files.length} staged file(s).`);
  process.exit(0);
}

const execution = spawnSync(process.execPath, ['scripts/qa/run.mjs', 'staged', `--checks=${[...selected].join(',')}`], {
  stdio: 'inherit',
  env: process.env,
});
process.exit(execution.status ?? 1);
