// Explicit L3 only: production relay and cloud invitation, with no model or task fixtures.
import assert from 'node:assert/strict';
import {randomBytes} from 'node:crypto';
import {readFileSync, writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {once} from 'node:events';
import {makeGateway} from './server.mjs';

const input = process.argv[2];
const fixture = JSON.parse(readFileSync(input));
assert.equal(process.env.NORMA_THREAD_CODEX_REAL_L3, '1', 'Must explicitly enable real Codex L3');
const origin = `http://127.0.0.1:${fixture.port}`;
const admin = randomBytes(32).toString('hex');
const server = makeGateway({env: {PUBLIC_ORIGIN: origin, COMPANION_ACCESS_TOKEN: admin,
  NORMA_CLOUD_DEVICE_STATE: join(fixture.root, 'cloud-devices.json'),
  NORMA_RELAY_TRUSTED_CERTIFICATES: JSON.stringify([Buffer.from(fixture.certificate).toString('base64')])}});
const sockets = new Set();
server.on('connection', socket => { sockets.add(socket); socket.on('close', () => sockets.delete(socket)); });
server.listen(fixture.port, '127.0.0.1');
await once(server, 'listening');
const inviteResponse = await fetch(origin + '/v1/admin/devices/invite', {method: 'POST',
  headers: {Authorization: `Bearer ${admin}`, 'Content-Type': 'application/json'}, body: '{}'});
assert.equal(inviteResponse.status, 200, 'Real local gateway must issue invitation');
const invite = await inviteResponse.json();
const claimResponse = await fetch(origin + '/v1/devices/claim', {method: 'POST',
  headers: {'Content-Type': 'application/json'}, body: JSON.stringify({code: invite.code, name: 'Real Codex L3 phone'})});
assert.equal(claimResponse.status, 200, 'Real invitation must be claimed');
const claim = await claimResponse.json();
assert.ok(claim.token, 'Cloud credential is required');
fixture.cloud = {deviceID: claim.device_id, token: claim.token};
writeFileSync(join(fixture.root, 'phone-fixture.json'), JSON.stringify(fixture), {mode: 0o600});
console.log('Real Mac gateway ready');
function stop() { for (const socket of sockets) socket.destroy(); server.close(() => process.exit(0)); }
process.once('SIGTERM', stop);
process.once('SIGINT', stop);
// 10min = 3min model budget + native compile/UI inspection and cleanup allowance; local only.
setTimeout(stop, 600_000).unref();
