import { build } from 'esbuild';
import { cp, mkdir } from 'node:fs/promises';
await mkdir('spikes/alphatab-dist/font', { recursive: true });
await build({ entryPoints: ['spikes/alphatab-spike.js'], bundle: true, format: 'esm',
  outfile: 'spikes/alphatab-dist/spike.js', target: ['chrome110'] });
await cp('spikes/alphatab-spike.html', 'spikes/alphatab-dist/index.html');
await cp('node_modules/@coderline/alphatab/dist/font', 'spikes/alphatab-dist/font', { recursive: true });

