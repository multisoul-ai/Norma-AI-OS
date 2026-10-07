import assert from 'node:assert/strict';
import {createHash, randomUUID, sign, X509Certificate} from 'node:crypto';
import {mkdtempSync, readFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {execFileSync} from 'node:child_process';
import {once} from 'node:events';
import {makeGateway} from '../server.mjs';

/// 扫码后的服务注册必须接受可信 Mac 的证明，且重试不能生成新身份或恢复撤销权限。
/// 数据构造：32 个零字节编码为 43 个 A；SHA256 绑定此 token；证明寿命 2×60=120 秒。
/// 执行过程：无证明拒绝→注册→重复→换 token 拒绝→重启恢复→过期拒绝→撤销后重放拒绝。
/// 预期结果：同一配对仅一个身份；不保存明文 token；无邀请且无模型 Key 也能注册。
const directory = mkdtempSync(join(tmpdir(), 'norma-paired-registration-'));
let current = Date.now(), server;
try {
  let fixture;
  if (process.argv[2]) fixture = JSON.parse(readFileSync(process.argv[2]));
  else {
    const key = join(directory, 'key.pem'), cert = join(directory, 'cert.pem');
    execFileSync('openssl', ['req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:prime256v1',
      '-nodes', '-keyout', key, '-out', cert, '-days', '1', '-subj', '/CN=Registration Test'], {stdio: 'ignore'});
    const certificate = new X509Certificate(readFileSync(cert)).raw;
    const iat = Math.floor(current / 1000);
    const payload = Buffer.from(JSON.stringify({v: 1, iss: 'mac-' + createHash('sha256').update(certificate).digest('hex'),
      sub: 'control-phone', aud: 'https://soulvoiceai.com/norma/voice', iat, exp: iat + 120,
      client: '13f257c9-c14a-4d76-974c-ebdc1f06a5dc', token_hash: createHash('sha256').update('A'.repeat(43)).digest('hex')}));
    fixture = {certificate: [...certificate], proof: 'ncr1.' + payload.toString('hex') + '.' +
      sign('sha256', Buffer.concat([Buffer.from('Norma cloud registration v1\n'), payload]), readFileSync(key)).toString('hex')};
  }
  const env = {COMPANION_ACCESS_TOKEN: 'test-admin-credential-32-characters',
    NORMA_CLOUD_DEVICE_STATE: join(directory, 'devices.json'),
    NORMA_RELAY_TRUSTED_CERTIFICATES: JSON.stringify([Buffer.from(fixture.certificate).toString('base64')])};
  const start = async () => { server = makeGateway({env, now: () => current}); server.listen(0, '127.0.0.1'); await once(server, 'listening'); };
  const stop = async () => { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); };
  await start();
  const post = async (path, body, token) => {
    const response = await fetch(`http://127.0.0.1:${server.address().port}${path}`, {method: 'POST',
      headers: {'Content-Type': 'application/json', ...(token ? {Authorization: `Bearer ${token}`} : {})}, body: JSON.stringify(body)});
    return {status: response.status, body: await response.json()};
  };
  const body = {proof: fixture.proof, device_id: '13f257c9-c14a-4d76-974c-ebdc1f06a5dc', token: 'A'.repeat(43), name: 'iPhone'};
  const route = '/v1/devices/register';
  const absent = await post(route, {...body, proof: ''});
  assert.equal(absent.status, 401, '缺少可信 Mac 证明时必须拒绝注册');
  const parts = body.proof.split('.'), altered = JSON.parse(Buffer.from(parts[1], 'hex'));
  altered.sub = 'different-phone';
  const forged = [parts[0], Buffer.from(JSON.stringify(altered)).toString('hex'), parts[2]].join('.');
  assert.equal((await post(route, {...body, proof: forged})).status, 401, '修改已签名设备必须验签失败');
  const trusted = env.NORMA_RELAY_TRUSTED_CERTIFICATES;
  env.NORMA_RELAY_TRUSTED_CERTIFICATES = '[]';
  assert.equal((await post(route, body)).status, 401, '未被服务信任的 Mac 不能授予手机访问权');
  env.NORMA_RELAY_TRUSTED_CERTIFICATES = trusted;
  const accepted = await post(route, body);
  assert.equal(accepted.status, 200, 'Mac 已批准的手机必须直接完成注册，无需第二张邀请码');
  assert.equal(accepted.body.device_id, body.device_id, '注册结果必须匹配手机保存的身份');
  assert.equal(Object.hasOwn(accepted.body, 'token'), false, '服务器无需回传手机已持有的密钥');
  assert.equal((await post(route, body)).status, 200, '丢失回包后的同身份重试必须幂等');
  assert.equal((await post(route, {...body, token: 'B'.repeat(43)})).status, 401, '证明不能被用于不同 token');
  assert.equal((await post(route, {...body, device_id: randomUUID()})).status, 401, '证明不能被用于不同手机身份');
  const persisted = readFileSync(env.NORMA_CLOUD_DEVICE_STATE, 'utf8');
  assert.equal(persisted.includes(body.token), false, '磁盘只能保存 token 哈希');
  assert.equal(Object.keys(JSON.parse(persisted).devices).length, 1, '重复注册不能产生多个设备');
  await stop(); await start();
  assert.equal((await post(route, body)).status, 200, '服务重启后仍应恢复原注册身份');
  current += 121_000; // 120 秒证明寿命 + 1 秒，必须过期；长期设备凭据不随之过期。
  assert.equal((await post(route, body)).status, 401, '过期注册证明不得注册新身份');
  const access = await post('/v1/text/turn', {turn_id: randomUUID(), messages: [{role: 'user', content: 'test'}]}, body.token);
  assert.equal(access.body.error, 'provider_unavailable', '注册证明过期后，设备仍应通过鉴权；此测试未配置模型');
  current -= 121_000;
  await post('/v1/admin/devices/revoke', {device_id: body.device_id}, env.COMPANION_ACCESS_TOKEN);
  const revoked = await post(route, body);
  assert.equal(revoked.status, 403, '已撤销云身份不得借旧配对注册重新启用');
  assert.equal(revoked.body.error, 'revoked', '应明确报告撤销，而不是声称已注册');
  console.log('PASS: paired registration, retry, persistence, token binding, expiry, revocation');
} finally {
  if (server?.listening) { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
  rmSync(directory, {recursive: true, force: true});
}
