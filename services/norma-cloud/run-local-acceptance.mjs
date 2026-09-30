// Local-only TestFlight acceptance. The config directory is prepared by the release operator.
import {readFileSync} from 'node:fs';
import {join} from 'node:path';
import {networkInterfaces} from 'node:os';
import {createHash, X509Certificate} from 'node:crypto';
import {makeGateway} from './server.mjs';
import {makeLocalTLSGateway} from './local-tls.mjs';

const directory = process.argv[2];
if (!directory) throw new Error('Usage: node run-local-acceptance.mjs <private-config-directory>');
const config = JSON.parse(readFileSync(join(directory, 'acceptance.json')));
const {host, port, tlsPort, pin} = config;
if (!/^192\.168\.\d{1,3}\.\d{1,3}$/.test(host) ||
    !Object.values(networkInterfaces()).flat().some(address => address?.address === host) ||
    ![port, tlsPort].every(value => Number.isInteger(value) && value > 1024 && value <= 65535) || port === tlsPort) {
  throw new Error('Expected the configured private LAN interface and two distinct unprivileged ports');
}
const cert = readFileSync(join(directory, 'gateway-cert.pem'));
if (createHash('sha256').update(new X509Certificate(cert).raw).digest('hex') !== pin) {
  throw new Error('Certificate changed; cannot serve the pinned TestFlight build');
}
const identity = JSON.parse(readFileSync(join(directory, 'mac-public-identity.json')));
const gateway = makeGateway({env: {
  PUBLIC_ORIGIN: `http://127.0.0.1:${port}`,
  COMPANION_ACCESS_TOKEN: readFileSync(join(directory, 'admin-token'), 'utf8').trim(),
  NORMA_CLOUD_DEVICE_STATE: join(directory, 'cloud-devices.json'),
  NORMA_RELAY_TRUSTED_CERTIFICATES: JSON.stringify([Buffer.from(identity.certificate_der).toString('base64')]),
}});
const tls = makeLocalTLSGateway({gateway, key: readFileSync(join(directory, 'gateway-key.pem')), cert});
const sockets = new Set();
for (const server of [gateway, tls]) {
  server.on('connection', socket => { sockets.add(socket); socket.once('close', () => sockets.delete(socket)); });
  server.on('error', error => { console.error(error.message); stop(); process.exitCode = 1; });
}
function stop() {
  for (const socket of sockets) socket.destroy();
  gateway.close(); tls.close();
}
process.once('SIGTERM', stop); process.once('SIGINT', stop);
gateway.listen(port, '127.0.0.1');
tls.listen(tlsPort, host, () => console.log(`Local TestFlight gateway ready at https://${host}:${tlsPort}`));
