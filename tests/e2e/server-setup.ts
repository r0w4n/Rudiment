import { spawn, type ChildProcess } from 'node:child_process';
import { once } from 'node:events';

async function stopServer(server: ChildProcess) {
  if (!server.pid || server.exitCode !== null || server.signalCode !== null) return;
  const closed = once(server, 'close');
  if (!server.kill()) throw new Error('Failed to stop the browser test server');
  await closed;
}

export default async function startServer() {
  const server = spawn('python3', ['-u', '-m', 'http.server', '0', '--bind', '127.0.0.1', '--directory', 'dist'], {
    stdio: ['ignore', 'pipe', 'inherit'],
  });

  try {
    const port = await new Promise<string>((resolve, reject) => {
      let output = '';
      const finish = (error?: Error, port?: string) => {
        clearTimeout(timeout);
        server.stdout.off('data', onData);
        server.off('error', onError);
        server.off('exit', onExit);
        if (error) reject(error);
        else if (port) resolve(port);
      };
      const onData = (chunk: Buffer) => {
        output += chunk.toString();
        const match = output.match(/Serving HTTP on .* port (\d+)/);
        if (match) finish(undefined, match[1]);
      };
      const onError = (error: Error) => finish(error);
      const onExit = (code: number | null, signal: NodeJS.Signals | null) =>
        finish(new Error(`Browser test server exited before starting (code ${code}, signal ${signal})`));
      const timeout = setTimeout(() => finish(new Error('Timed out starting the browser test server')), 15_000);
      server.stdout.on('data', onData);
      server.once('error', onError);
      server.once('exit', onExit);
    });

    const url = `http://127.0.0.1:${port}`;
    const response = await fetch(url, { signal: AbortSignal.timeout(15_000) });
    if (!response.ok) throw new Error(`Browser test server returned HTTP ${response.status} for ${url}`);
    process.env.RUDIMENT_E2E_BASE_URL = url;
  } catch (error) {
    await stopServer(server);
    throw error;
  }

  return async () => {
    delete process.env.RUDIMENT_E2E_BASE_URL;
    await stopServer(server);
  };
}
