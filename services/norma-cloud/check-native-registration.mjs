import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
import {once} from 'node:events';
import {createInterface} from 'node:readline';
import {spawn} from 'node:child_process';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {makeGateway} from './server.mjs';

// Rust owns the real Mac pairing/approval; this runner owns an isolated production gateway.
const path = process.argv[2], fixture = JSON.parse(readFileSync(path));
const env = {COMPANION_ACCESS_TOKEN: randomUUID(), NORMA_CLOUD_DEVICE_STATE: path + '.devices',
  NORMA_RELAY_TRUSTED_CERTIFICATES: JSON.stringify([Buffer.from(fixture.certificate).toString('base64')])};
let server = makeGateway({env});
server.listen(0, '127.0.0.1'); await once(server, 'listening');
const port = server.address().port;
await new Promise(resolve => server.close(resolve));
env.PUBLIC_ORIGIN = `http://127.0.0.1:${port}`;
server = makeGateway({env}); server.listen(port, '127.0.0.1'); await once(server, 'listening');
const sockets = new Set();
server.on('connection', socket => { sockets.add(socket); socket.once('close', () => sockets.delete(socket)); });
writeFileSync(path, JSON.stringify({...fixture, port, namespace: randomUUID()}), {mode: 0o600});
process.stdout.write(JSON.stringify({port}) + '\n');
const input = createInterface({input: process.stdin})[Symbol.asyncIterator]();
await input.next();
const repo = fileURLToPath(new URL('../../', import.meta.url));
const output = join(repo, 'target/norma-ios-evidence/single-scan-20260930');
if (process.env.NORMA_REGISTRATION_UI === '1') {
  writeFileSync(join(output, 'ui-fixture.json'), readFileSync(path), {mode: 0o600});
  // Computer Use consumes the same real pairing fixture, without pre-registering this phone.
  const done = join(output, 'ui-done');
  const {existsSync} = await import('node:fs');
  const deadline = Date.now() + 600_000;
  while (!existsSync(done) && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 1000));
  for (const socket of sockets) socket.destroy();
  server.closeAllConnections(); server.close(); process.stdin.destroy();
  process.exit(existsSync(done) ? 0 : 1);
}
const child = spawn('xcodebuild', ['test', '-project', join(repo, 'ios/Norma.xcodeproj'), '-scheme', 'Norma',
  '-destination', `platform=iOS Simulator,id=${process.env.NORMA_IOS_SIMULATOR}`,
  '-derivedDataPath', join(repo, 'target/norma-ios-single-scan-native'),
  '-clonedSourcePackagesDirPath', join(repo, 'target/ios-native-packages'),
  '-packageCachePath', join(repo, 'target/ios-native-cache'), '-disableAutomaticPackageResolution',
  '-parallel-testing-enabled', 'NO', '-only-testing:NormaDeviceTests/PairedCloudRegistrationTests',
  `NORMA_REGISTRATION_FIXTURE=${path}`], {stdio: ['ignore', 'pipe', 'pipe']});
let log = '';
child.stdout.on('data', bytes => { log += bytes; }); child.stderr.on('data', bytes => { log += bytes; });
// 180s = 120s pairing lifetime + 60s XCTest teardown; a stale offer fails inside the test.
const deadline = setTimeout(() => child.kill('SIGTERM'), 180_000);
try {
  const [status] = await once(child, 'exit');
  writeFileSync(join(output, 'native-test.log'), log);
  assert.equal(status, 0, '原生扫码→Mac批准→自动注册→真实中继→Keychain恢复必须通过；见native-test.log');
  const state = JSON.parse(readFileSync(env.NORMA_CLOUD_DEVICE_STATE));
  assert.equal(Object.keys(state.devices).length, 1, '原生流程不得创建重复云身份');
  assert.equal(Object.keys(state.invites).length, 0, '原生注册不得要求或生成第二张邀请二维码');
  console.log('PASS: native one-scan registration and authenticated relay');
} finally {
  clearTimeout(deadline);
  for (const socket of sockets) socket.destroy();
  server.closeAllConnections(); server.close(); process.stdin.destroy();
}
