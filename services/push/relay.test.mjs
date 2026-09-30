import test from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, verify } from 'node:crypto';
import * as http2 from 'node:http2';
import { createRelay, createAPNs } from './relay.mjs';

const key = 'a'.repeat(64); // 32 random bytes represented by 64 hex characters in deployment.
const seconds = 1_800_000_000;
const event = () => ({
  event_id: '00000000-0000-4000-8000-000000000001', token: 'b'.repeat(64),
  expires_at: seconds + 300, kind: 'approval', machine_id: `mac-${'c'.repeat(64)}`,
  workspace_id: 'workspace-one', route_id: 'route-one', node_id: '7',
  run_id: 'run-one', session_id: 'session-one', turn_id: 'turn-one', request_id: 'approval-one',
});
async function listen(server) {
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  return `http://127.0.0.1:${server.address().port}`;
}

/// Relay入口只接受已认证的最小事件，同一事件重试不重复发通知。
/// 数据构造：有效期300s=5分钟；4097字节比4096上限多1；事件ID相同但run不同模拟错误重用。
/// 执行过程：未授权/超限/夹带正文/过期拒绝 → 并发发送相同事件 → 改参数冲突 → 后续事件正常接收。
/// 预期结果：只有两份合法事件进入发送器，重试返回原accepted，绝不回报已送达或执行审批。
test('authenticated bounded events are idempotent without accepting task contents', async t => {
  const sent = [];
  const relay = createRelay({ key, now: () => seconds * 1000, deliver: async value => {
    sent.push(value);
    // One event-loop turn lets simultaneous duplicate requests share the in-flight result.
    await new Promise(resolve => setImmediate(resolve));
    if (value.kind === 'failed') throw new Error('test disconnect');
    return { status: 200, body: { status: 'accepted' } };
  } });
  const origin = await listen(relay);
  t.after(() => { relay.closeAllConnections(); relay.close(); });
  const post = async (body, secret = key) => {
    const response = await fetch(origin + '/v1/notifications', { method: 'POST',
      headers: { authorization: `Bearer ${secret}`, 'content-type': 'application/json' },
      body: typeof body === 'string' ? body : JSON.stringify(body) });
    return [response.status, await response.json()];
  };
  assert.equal((await post(event(), 'wrong'))[0], 401, 'wrong relay credential must be rejected');
  assert.equal((await post('x'.repeat(4097)))[0], 413, 'body must remain bounded');
  assert.equal((await post({ ...event(), command: 'rm -rf project' }))[0], 400, 'relay cannot accept execution commands');
  assert.equal((await post({ ...event(), summary: 'private source code' }))[0], 400, 'private content must not enter push payload');
  assert.equal((await post({ ...event(), expires_at: seconds }))[0], 400, 'expired event cannot produce a delayed prompt');
  assert.equal(sent.length, 0, 'invalid requests must not reach APNs');
  const [first, repeated] = await Promise.all([post(event()), post(event())]);
  assert.deepEqual(first, [200, { status: 'accepted' }], 'only APNs acceptance is acknowledged');
  assert.deepEqual(repeated, first, 'concurrent duplicate returns the same outcome');
  assert.equal(sent.length, 1, 'same event cannot be delivered twice');
  assert.equal((await post({ ...event(), run_id: 'other-run' }))[0], 409, 'event identity cannot be reused for another task');
  assert.equal(sent.length, 1, 'conflict must not send another notification');
  const completed = { ...event(), event_id: '00000000-0000-4000-8000-000000000002', kind: 'completed' };
  delete completed.request_id;
  assert.equal((await post(completed))[0], 200, 'distinct completion may be sent');
  assert.equal(sent.length, 2, 'only the approval and completion must be sent');
  assert.equal(sent[0].request_id, 'approval-one', 'approval must keep its exact target');
  assert.equal(Object.hasOwn(sent[1], 'request_id'), false, 'completion must not carry a stale approval identity');
  const failed = { ...completed, event_id: '00000000-0000-4000-8000-000000000003', kind: 'failed' };
  assert.deepEqual(await post(failed), [502, { error: 'delivery_unknown' }], 'network failure must not imply safe retry');
  assert.deepEqual(await post(failed), [502, { error: 'delivery_unknown' }], 'unknown result must stay unknown on retry');
  assert.equal(sent.length, 3, 'retry after uncertain delivery must not send a duplicate');
  const long = { ...event(), event_id: '00000000-0000-4000-8000-000000000004', request_id: 'a'.repeat(256) };
  assert.equal((await post(long))[0], 200, 'Provider approval IDs may use the full 256-character protocol limit');
  assert.equal(sent[3].request_id, long.request_id, 'approval identity must never be shortened or hashed');
  assert.equal((await post({ ...long, request_id: 'a'.repeat(257) }))[0], 400, '256+1 characters must be rejected');
  assert.equal(sent.length, 4, 'overlong ID must not create an extra send');
});

/// HTTP/2发送使用有效ES256 JWT和固定topic，失效设备与网络未知结果不能被伪装为送达。
/// 数据构造：临时P-256签名密钥；2个事件响应200/410；第3次连接在响应前断开。
/// 执行过程：本地HTTP/2接收请求并验证JWT签名 → 检查最小payload → 失效token → 断开且不重试。
/// 预期结果：只有3次请求，JWT在50分钟内复用；token只在APNs路径中，payload不含token或执行指令。
test('native HTTP/2 signs APNs requests and preserves unknown or unregistered outcomes', async t => {
  const { privateKey, publicKey } = generateKeyPairSync('ec', { namedCurve: 'prime256v1' });
  const received = [];
  let active = 0, peak = 0;
  const server = http2.createServer();
  server.on('stream', (stream, headers) => {
    active++; peak = Math.max(peak, active);
    let body = '';
    stream.setEncoding('utf8');
    stream.on('data', chunk => { body += chunk; });
    stream.on('end', () => setImmediate(() => {
      received.push({ headers, body: JSON.parse(body) });
      active--;
      if (received.length === 3) { stream.session.destroy(); return; }
      stream.respond({ ':status': received.length === 1 ? 200 : 410 });
      stream.end(received.length === 1 ? '' : JSON.stringify({ reason: 'Unregistered', timestamp: seconds * 1000 }));
    }));
  });
  const origin = await listen(server);
  let connections = 0;
  const apns = createAPNs({ privateKey, teamID: 'TEAM123456', keyID: 'KEY1234567', environment: 'sandbox', topic: 'com.norma-ai.os.ios' }, {
    now: () => seconds * 1000,
    connect: url => { assert.equal(url, 'https://api.sandbox.push.apple.com', 'environment must select the official APNs host'); connections++; return http2.connect(origin); },
  });
  t.after(() => { apns.close(); server.close(); });
  const [accepted, revoked, disconnected] = await Promise.all([apns.send(event()), apns.send(event()), apns.send(event())]);
  assert.deepEqual(accepted, { status: 200, body: { status: 'accepted' } }, '200 means accepted, not delivered');
  assert.deepEqual(revoked, { status: 410, body: { error: 'token_unregistered', invalidated_at: seconds * 1000 } }, 'revoked token must be surfaced to the Mac');
  assert.deepEqual(disconnected, { status: 502, body: { error: 'delivery_unknown' } }, 'disconnect must not claim acceptance');
  assert.equal(peak, 1, 'new APNs connections allow only one stream before token authentication');
  assert.equal(connections, 1, 'reuse the HTTP/2 connection instead of reconnecting for every push');
  assert.equal(received.length, 3, 'unknown outcome must never retry automatically');
  const first = received[0];
  assert.equal(first.headers['apns-topic'], 'com.norma-ai.os.ios', 'client input cannot replace the topic');
  assert.equal(first.headers['apns-push-type'], 'alert', 'decision reminders are visible notifications');
  assert.equal(first.headers['apns-expiration'], String(seconds + 300), 'late alerts must expire after five minutes');
  const jwt = first.headers.authorization.slice('bearer '.length).split('.');
  assert.deepEqual(JSON.parse(Buffer.from(jwt[0], 'base64url')), { alg: 'ES256', kid: 'KEY1234567' }, 'JWT header must match the signing key');
  assert.deepEqual(JSON.parse(Buffer.from(jwt[1], 'base64url')), { iss: 'TEAM123456', iat: seconds }, 'JWT claims must bind the configured team and current time');
  assert.ok(verify('sha256', Buffer.from(`${jwt[0]}.${jwt[1]}`), { key: publicKey, dsaEncoding: 'ieee-p1363' }, Buffer.from(jwt[2], 'base64url')), 'APNs signature must verify with the generated public key');
  assert.equal(received[1].headers.authorization, first.headers.authorization, 'JWT must not refresh for each notification');
  assert.equal(first.body.norma.machine_id, event().machine_id, 'notification target must keep its computer identity');
  assert.equal(first.body.norma.request_id, 'approval-one', 'notification tap must locate the original approval');
  assert.equal(first.body.aps.alert.body, '有一项操作需要你确认', 'older Macs without preview must remain compatible');
  assert.equal(Object.hasOwn(first.body.norma, 'token'), false, 'device token cannot leak in payload');
  assert.equal(Object.hasOwn(first.body.norma, 'command'), false, 'notification must never authorize task execution');
  apns.close();
  assert.deepEqual(await apns.send(event()), { status: 503, body: { error: 'relay_stopped' } }, 'shutdown must reject later queued work');
  assert.equal(connections, 1, 'shutdown must not recreate an APNs connection');
});

/// 完成通知展示项目、准确卡片和最终输出摘要；展示文本不能混入跳转身份。
/// 数据构造：本地HTTP/2接收器，UTF-8上限项目160B、卡片240B、摘要640B=160个四字节标量。
/// 执行过程：真实HTTP入口校验preview → APNs编码 → 超长/非字符串/额外字段拒绝。
/// 预期结果：标题/副标题/正文具体可读，norma只保留原目标；非法预览不产生额外投递。
test('completion preview reaches the alert without changing the routing hint', async t => {
  const received = [];
  const server = http2.createServer();
  server.on('stream', (stream) => {
    let body = '';
    stream.setEncoding('utf8');
    stream.on('data', chunk => { body += chunk; });
    stream.on('end', () => { received.push(JSON.parse(body)); stream.respond({ ':status': 200 }); stream.end(); });
  });
  const origin = await listen(server);
  const { privateKey } = generateKeyPairSync('ec', { namedCurve: 'prime256v1' });
  const apns = createAPNs({ privateKey, teamID: 'TEAM123456', keyID: 'KEY1234567', environment: 'sandbox', topic: 'com.norma-ai.os.ios' }, {
    now: () => seconds * 1000, connect: () => http2.connect(origin),
  });
  const relay = createRelay({ key, now: () => seconds * 1000, deliver: apns.send });
  const relayOrigin = await listen(relay);
  t.after(() => { relay.closeAllConnections(); relay.close(); apns.close(); server.close(); });
  const value = { ...event(), kind: 'completed', preview: { project: 'Norma OS', card: '修复通知 · #7', summary: '已修复跳转，回归测试通过。' } };
  delete value.request_id;
  const post = async value => (await fetch(relayOrigin + '/v1/notifications', { method: 'POST', headers: {
    authorization: `Bearer ${key}`, 'content-type': 'application/json',
  }, body: JSON.stringify(value) })).status;
  assert.equal(await post(value), 200, 'bounded final-output preview should be accepted');
  assert.deepEqual(received[0].aps.alert, { title: '修复通知 · #7 · 已完成', subtitle: 'Norma OS', body: '已修复跳转，回归测试通过。' }, 'alert must name the card and project and show its outcome');
  assert.equal(Object.hasOwn(received[0].norma, 'preview'), false, 'display text must not invalidate the strict iOS routing decoder');
  assert.equal(received[0].norma.turn_id, 'turn-one', 'completed turn identity must remain intact');
  assert.equal(await post({ ...value, preview: { ...value.preview, summary: '😀'.repeat(161) } }), 400, '161 × 4 = 644 bytes exceeds the 640-byte summary ceiling');
  assert.equal(await post({ ...value, preview: { ...value.preview, card: 7 } }), 400, 'card preview must be a string');
  assert.equal(await post({ ...value, preview: { ...value.preview, command: 'execute' } }), 400, 'preview cannot smuggle a control field');
  assert.equal(received.length, 1, 'invalid previews must never reach APNs');
  // Max token/IDs + quote-escaped preview = 3846 request bytes, still below 4096.
  const largest = { ...event(), event_id: '00000000-0000-4000-8000-000000000004', token: 'b'.repeat(512),
    workspace_id: 'a'.repeat(128), route_id: 'a'.repeat(128), node_id: '18446744073709551615',
    run_id: 'a'.repeat(128), session_id: 'a'.repeat(128), turn_id: 'a'.repeat(128), request_id: 'a'.repeat(256),
    preview: { project: '"'.repeat(160), card: '"'.repeat(240), summary: '"'.repeat(640) } };
  assert.equal(await post(largest), 200, 'full protocol identity and escaped previews must fit the request budget');
  assert.ok(Buffer.byteLength(JSON.stringify(received[1])) <= 4096, 'the final APNs payload must also fit its 4KiB ceiling');
  assert.equal(received[1].aps.alert.body.length, 640, 'exact preview boundary must preserve the final character');
});
