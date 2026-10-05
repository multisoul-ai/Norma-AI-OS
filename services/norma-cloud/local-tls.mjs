import {createServer} from 'node:https';

// Local acceptance front door. Administrative routes and Mac registration stay on loopback.
export function makeLocalTLSGateway({gateway, key, cert}) {
  const allowed = new Set(['GET /health', 'POST /v1/devices/claim', 'POST /v1/devices/register',
    'POST /v1/text/turn']);
  const server = createServer({key, cert, minVersion: 'TLSv1.3'}, (req, res) => {
    if (allowed.has(`${req.method} ${req.url}`)) return gateway.emit('request', req, res);
    res.writeHead(404, {'Content-Type': 'application/json', 'Cache-Control': 'no-store'});
    res.end('{"error":"not_found"}');
  });
  server.requestTimeout = 10000;
  server.headersTimeout = 10000;
  server.on('upgrade', (req, socket, head) => {
    if (req.method === 'GET' && req.url === '/v1/relay/phone') {
      gateway.emit('upgrade', req, socket, head);
    } else socket.end('HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n');
  });
  return server;
}
