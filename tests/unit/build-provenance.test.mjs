import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const script = fileURLToPath(new URL('../../scripts/qa/build-provenance.mjs', import.meta.url));
const gitEnv = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('GIT_')));

test('a source edit or changed compiled asset invalidates the Playwright build', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'rudiment-build-'));
  const run = (command, args) => execFileSync(command, args, { cwd: root, stdio: 'pipe', env: gitEnv });
  try {
    run('git', ['init', '-q']);
    run('git', ['config', 'user.name', 'Test']);
    run('git', ['config', 'user.email', 'test@example.com']);
    await writeFile(path.join(root, 'source.txt'), 'current source\n');
    await writeFile(path.join(root, '.gitignore'), 'dist/\n');
    run('git', ['add', 'source.txt', '.gitignore']);
    run('git', ['commit', '-qm', 'base']);
    await mkdir(path.join(root, 'dist/public'), { recursive: true });
    await writeFile(path.join(root, 'dist/index.html'), '<html></html>');
    await writeFile(path.join(root, 'dist/public/bridge.js'), 'bridge');
    await writeFile(path.join(root, 'dist/app.wasm'), 'compiled');

    run('node', [script, 'record']);
    run('node', [script, 'verify']);

    await writeFile(path.join(root, 'source.txt'), 'changed source\n');
    assert.throws(() => run('node', [script, 'verify']), /Playwright requires a fresh production build/);
    await writeFile(path.join(root, 'source.txt'), 'current source\n');

    await writeFile(path.join(root, 'dist/app.wasm'), 'stale binary');
    assert.throws(() => run('node', [script, 'verify']), /Playwright requires a fresh production build/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
