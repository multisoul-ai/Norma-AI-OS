import {createServer} from 'node:http';
import {pathToFileURL} from 'node:url';
import {makeCloudGateway} from './voice/gateway.mjs';
import {fetchPlan} from './voice/plan.mjs';
import {createPushRelay} from './push/relay.mjs';
import {createAuth} from './auth/routes.mjs';

export {behaviorTool, doubaoSession} from './voice/gateway.mjs';

export function makeGateway({pushRelay, auth, ...options} = {}) {
  pushRelay ??= createPushRelay(options.env || process.env);
  let gateway;
  try {
    gateway = makeCloudGateway(options);
    if (auth === undefined) auth = createAuth(options.env || process.env);
  } catch (error) {pushRelay?.emit('close'); gateway?.emit('close'); throw error;}
  if (!pushRelay && !auth) return gateway;
  const server = createServer({maxHeaderSize: 8192}, (req, res) => {
    const path = (req.url || '').split('?')[0];
    if (auth && path.startsWith('/v1/auth/')) return auth.handle(req, res);
    const target = pushRelay && (path === '/healthz' || path === '/v1/notifications') ? pushRelay : gateway;
    target.emit('request', req, res);
  });
  server.headersTimeout = gateway.headersTimeout;
  server.requestTimeout = gateway.requestTimeout;
  if (pushRelay) server.maxConnections = pushRelay.maxConnections;
  server.on('upgrade', (req, socket, head) => gateway.emit('upgrade', req, socket, head));
  server.on('close', () => {gateway.emit('close'); pushRelay?.emit('close');});
  return server;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const server = makeGateway({fetchImpl:fetchPlan});
  const connections = new Set();
  server.on('connection', socket => {connections.add(socket); socket.once('close', () => connections.delete(socket));});
  const close = () => {server.close(); for (const socket of connections) socket.destroy();};
  process.once('SIGINT', close); process.once('SIGTERM', close);
  server.on('error', () => {close(); console.error('Norma cloud could not listen'); process.exitCode = 1;});
  server.listen(Number(process.env.PORT || 8787), process.env.HOST || '127.0.0.1', () => console.log('Norma cloud gateway ready'));
}
