import { spawn } from 'node:child_process';

const run = (command, args, options = {}) => spawn(command, args, {
  stdio: 'inherit',
  ...options,
});

const fixtures = run(process.execPath, ['scripts/generate-fixtures.mjs']);
const fixtureResult = await new Promise(resolve => fixtures.once('exit', code => resolve(code ?? 1)));
if (fixtureResult !== 0) process.exit(fixtureResult);

const bridge = run(process.execPath, ['scripts/build-bridge.mjs', '--watch']);
const port = process.env.RUDIMENT_PORT || '5173';
const trunk = run('trunk', [
  'serve', 'index.html', '--dist', 'dist',
  '--watch', 'src', '--watch', 'index.html', '--watch', 'styles.css', '--watch', 'public',
  '--watch', 'Cargo.toml', '--watch', '../Cargo.toml', '--watch', '../Cargo.lock',
  '--watch', '../crates/core', '--enable-cooldown', '--port', port,
  '--skip-version-check', '--locked=true',
], { cwd: 'web' });

const children = [bridge, trunk];
let stopping = false;
const stop = signal => {
  if (stopping) return;
  stopping = true;
  for (const child of children) {
    if (!child.killed) child.kill(signal);
  }
};

process.on('SIGINT', () => stop('SIGINT'));
process.on('SIGTERM', () => stop('SIGTERM'));
for (const child of children) {
  child.once('exit', code => {
    if (!stopping) {
      process.exitCode = code ?? 1;
      stop('SIGTERM');
    }
  });
}
