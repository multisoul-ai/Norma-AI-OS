import assert from 'node:assert/strict';
import {execFileSync, spawn} from 'node:child_process';
import {createHash, randomUUID, X509Certificate} from 'node:crypto';
import {once} from 'node:events';
import {mkdtempSync, readFileSync, rmSync, writeFileSync} from 'node:fs';
import {request} from 'node:https';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {makeGateway} from '../server.mjs';
import {makeLocalTLSGateway} from '../local-tls.mjs';

/// 场景：真机TLS入口复用正式准入，不能暴露管理员或Mac注册端点。
/// 数据构造：1天有效P256证书、1个一次性邀请、回环随机端口；TLS最低1.3。
/// 执行过程：创建正式HTTP网关与TLS前端→检查管理路径拒绝→Swift验证错误pin/正确pin/重复邀请及真实WSS。
/// 预期结果：邀请只兑换一次，错误pin不发送请求；HTTPS/WSS可用，管理员路径仍只在回环后端。
const root = mkdtempSync(join(tmpdir(), 'norma-local-tls-'));
const keyPath = join(root, 'key.pem'), certPath = join(root, 'cert.pem');
execFileSync('openssl', ['req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:P-256', '-nodes',
  '-keyout', keyPath, '-out', certPath, '-subj', '/CN=Norma Local Acceptance', '-days', '1',
  '-addext', 'subjectAltName=IP:127.0.0.1', '-addext', 'extendedKeyUsage=serverAuth'], {stdio: 'ignore'});
const cert = readFileSync(certPath), pin = createHash('sha256').update(new X509Certificate(cert).raw).digest('hex');
const admin = 'local-tls-test-admin-at-least-24-characters';
const gateway = makeGateway({env: {COMPANION_ACCESS_TOKEN: admin,
  PUBLIC_ORIGIN: 'http://127.0.0.1:8787', NORMA_CLOUD_DEVICE_STATE: join(root, 'devices.json')}});
const tls = makeLocalTLSGateway({gateway, key: readFileSync(keyPath), cert});
let claims = 0, relayRequests = 0;
tls.on('request', req => { if (req.url === '/v1/devices/claim') claims++; });
tls.on('upgrade', req => { if (req.url === '/v1/relay/phone') relayRequests++; });
try {
  gateway.listen(0, '127.0.0.1'); tls.listen(0, '127.0.0.1');
  await Promise.all([once(gateway, 'listening'), once(tls, 'listening')]);
  const origin = `https://127.0.0.1:${tls.address().port}`;
  const status = path => new Promise((resolve, reject) => {
    const req = request(origin + path, {ca: cert, method: 'POST', headers: {Authorization: `Bearer ${admin}`}}, res => {
      res.resume(); res.on('end', () => resolve(res.statusCode));
    }); req.on('error', reject); req.end('{}');
  });
  assert.equal(await status('/v1/admin/devices/invite'), 404, 'TLS前端不得开放管理员邀请');
  assert.equal(await status('/v1/relay/mac'), 404, 'TLS前端不得开放Mac注册');
  const invitation = await fetch(`http://127.0.0.1:${gateway.address().port}/v1/admin/devices/invite`, {
    method: 'POST', headers: {'Content-Type': 'application/json', Authorization: `Bearer ${admin}`}, body: '{}'});
  assert.equal(invitation.status, 200, '管理员仍可从回环正式入口签发邀请');
  const fixture = join(root, 'fixture.json');
  const namespace = randomUUID();
  writeFileSync(fixture, JSON.stringify({origin, pin, namespace, code: (await invitation.json()).code}), {mode: 0o600});
  const repo = fileURLToPath(new URL('../../../', import.meta.url));
  const simulator = process.env.NORMA_IOS_SIMULATOR;
  const swift = spawn(simulator ? 'xcodebuild' : 'swift', simulator ? ['test',
    '-project', join(repo, 'ios/Norma.xcodeproj'), '-scheme', 'Norma',
    '-destination', `platform=iOS Simulator,id=${simulator}`,
    '-derivedDataPath', join(repo, 'target/norma-ios-local-tls-native'),
    '-clonedSourcePackagesDirPath', join(repo, 'target/ios-native-packages'),
    '-packageCachePath', join(repo, 'target/ios-native-cache'), '-disableAutomaticPackageResolution',
    '-parallel-testing-enabled', 'NO',
    '-only-testing:NormaDeviceTests/CloudInvitationLinkTests/testBundledLocalInvitationPersistsThroughModelReload',
    `NORMA_LOCAL_TLS_FIXTURE=${fixture}`, `NORMA_LOCAL_GATEWAY_ORIGIN=${origin}`,
    `NORMA_LOCAL_GATEWAY_PIN=${pin}`, `NORMA_LOCAL_GATEWAY_NAMESPACE=${namespace}`
  ] : ['test', '--package-path', join(repo, 'ios'),
    '--filter', 'LocalCloudConfigurationTests/testPinnedInvitationAndRelayUseRealTLS'],
    {env: {...process.env, NORMA_LOCAL_TLS_FIXTURE: fixture}, stdio: 'inherit'});
  const [code] = await once(swift, 'exit');
  assert.equal(code, 0, 'Swift真实TLS与证书拒绝检查必须通过');
  assert.equal(claims, simulator ? 1 : 2, '只能收到原生模型兑换，或Core正确兑换和重复兑换；错误pin不能发送邀请');
  assert.equal(relayRequests, 1, '真实手机客户端必须经TLS到达WSS中继');
  console.log('Local pinned HTTPS/WSS PASS');
} finally {
  tls.closeAllConnections(); gateway.closeAllConnections();
  await Promise.all([new Promise(resolve => tls.close(resolve)), new Promise(resolve => gateway.close(resolve))]);
  rmSync(root, {recursive: true, force: true});
}
