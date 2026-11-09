import { build, context } from 'esbuild';
import { mkdir } from 'node:fs/promises';
await mkdir('web/public', { recursive: true });
const options = { entryPoints: ['web/js/bridge.js'], bundle: true, minify: false,
  format: 'iife', globalName: 'RudimentBridge', outfile: 'web/public/bridge.js', target: ['chrome110'] };

if (process.argv.includes('--watch')) {
  const buildContext = await context(options);
  await buildContext.watch();
  console.log('Watching web/js for bridge changes...');
  await new Promise(resolve => {
    const keepAlive = setInterval(() => {}, 2_147_000_000);
    const stop = () => {
      clearInterval(keepAlive);
      resolve();
    };
    process.once('SIGINT', stop);
    process.once('SIGTERM', stop);
  });
  await buildContext.dispose();
} else {
  await build(options);
}

