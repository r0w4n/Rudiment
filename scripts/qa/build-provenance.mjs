import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

const marker = path.resolve('dist/.rudiment-build.json');
// Git exports repository-local variables while running hooks. They must not
// override the checkout (or a separate clean worktree) inspected here.
const gitEnv = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('GIT_')));

async function sourceFingerprint() {
  const hash = createHash('sha256');
  const git = (...args) => execFileSync('git', args, { encoding: 'buffer', env: gitEnv });
  hash.update(git('rev-parse', 'HEAD'));
  hash.update(git('diff', '--binary', 'HEAD', '--'));
  const untracked = git('ls-files', '--others', '--exclude-standard', '-z')
    .toString()
    .split('\0')
    .filter(Boolean)
    .sort();
  for (const file of untracked) {
    hash.update(file);
    hash.update(await readFile(file));
  }
  return hash.digest('hex');
}

async function builtOutputs() {
  const files = [];
  async function visit(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const file = path.join(directory, entry.name);
      if (file === marker) continue;
      if (entry.isDirectory()) await visit(file);
      else if (entry.isFile()) files.push(file);
      else throw new Error(`Unexpected build output type: ${file}`);
    }
  }
  await visit(path.resolve('dist'));
  const outputs = {};
  for (const file of files.sort()) {
    outputs[path.relative(process.cwd(), file)] = createHash('sha256')
      .update(await readFile(file))
      .digest('hex');
  }
  if (
    !outputs['dist/index.html'] ||
    !outputs['dist/public/bridge.js'] ||
    !files.some((file) => file.endsWith('.wasm'))
  ) {
    throw new Error('required production assets are missing');
  }
  return outputs;
}

export async function verifyBuild() {
  let recorded;
  try {
    recorded = JSON.parse(await readFile(marker, 'utf8'));
    const current = { source: await sourceFingerprint(), outputs: await builtOutputs() };
    if (JSON.stringify(recorded) !== JSON.stringify(current)) throw new Error('build inputs or outputs changed');
  } catch (error) {
    throw new Error(`Playwright requires a fresh production build: ${error.message}. Run npm run build.`, {
      cause: error,
    });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === import.meta.filename) {
  const mode = process.argv[2];
  if (mode === 'record') {
    await writeFile(
      marker,
      `${JSON.stringify({ source: await sourceFingerprint(), outputs: await builtOutputs() })}\n`,
    );
  } else if (mode === 'verify') {
    await verifyBuild();
  } else {
    throw new Error('Expected record or verify');
  }
}
