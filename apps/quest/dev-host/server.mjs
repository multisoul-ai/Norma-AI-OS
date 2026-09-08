import { createServer } from 'node:http';
import { once } from 'node:events';
import { WebSocketServer, WebSocket } from 'ws';
import { createSession, resolveDecision } from './session.mjs';

// Only a loopback development fixture. The production Rust adapter owns real sessions.
export async function startHost({ port = 8787 } = {}) {
  let snapshot = createSession();
  const ready = new Set();
  const requests = new Set();
  const server = createServer((request, response) => {
    response.writeHead(request.url === '/health' ? 200 : 404, { 'Content-Type': 'application/json' });
    response.end(JSON.stringify(request.url === '/health' ? { status: 'ok', source: 'mock', protocolVersion: 1 } : { error: 'not_found' }));
  });
  const wss = new WebSocketServer({ noServer: true, maxPayload: 65536 });
  server.on('upgrade', (request, socket, head) => {
    // Android/CLI clients have no browser origin. Prevent arbitrary web pages from controlling this fixture.
    if (request.url !== '/session' || request.headers.origin) { socket.destroy(); return; }
    wss.handleUpgrade(request, socket, head, client => wss.emit('connection', client));
  });
  const send = (client, value) => {
    if (client.readyState === WebSocket.OPEN) client.send(JSON.stringify(value));
  };
  wss.on('connection', client => {
    client.on('error', () => {});
    client.on('close', () => ready.delete(client));
    client.on('message', bytes => {
      let command;
      try { command = JSON.parse(bytes.toString()); }
      catch { send(client, { version: 1, type: 'error', message: 'invalid_json' }); return; }
      if (command?.version !== 1) {
        send(client, { version: 1, type: 'error', message: 'unsupported_version' }); return;
      }
      if (command.type === 'hello') { ready.add(client); send(client, snapshot); return; }
      if (!ready.has(client)) { send(client, { version: 1, type: 'error', message: 'hello_required' }); return; }
      if (command.type !== 'decision.resolve') {
        send(client, { version: 1, type: 'error', message: 'unsupported_command' }); return;
      }
      const result = requests.has(command.requestId)
        ? { accepted: false, snapshot, reason: 'duplicate_request' }
        : resolveDecision(snapshot, command);
      send(client, { version: 1, type: 'command.result', requestId: command.requestId ?? '', accepted: result.accepted, message: result.reason ?? 'accepted' });
      if (result.accepted) {
        requests.add(command.requestId);
        snapshot = result.snapshot;
        for (const peer of ready) send(peer, snapshot);
      }
    });
  });
  server.listen(port, '127.0.0.1');
  await once(server, 'listening');
  return {
    url: `ws://127.0.0.1:${server.address().port}/session`,
    close: async () => {
      for (const client of wss.clients) client.terminate();
      await new Promise(resolve => wss.close(resolve));
      await new Promise(resolve => server.close(resolve));
    },
  };
}
