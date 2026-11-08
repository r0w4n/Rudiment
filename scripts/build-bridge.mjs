import { build } from 'esbuild';
import { mkdir } from 'node:fs/promises';
await mkdir('web/public', { recursive: true });
await build({ entryPoints: ['web/js/bridge.js'], bundle: true, minify: false,
  format: 'iife', globalName: 'RudimentBridge', outfile: 'web/public/bridge.js', target: ['chrome110'] });

