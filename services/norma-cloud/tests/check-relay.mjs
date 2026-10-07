import assert from 'node:assert/strict';
import {createHash, randomUUID, sign, X509Certificate} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {once} from 'node:events';
import {mkdtempSync, readFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import WebSocket from 'ws';
import {makeGateway} from '../server.mjs';

function peer(url, token, headers = {}) {
  const socket = new WebSocket(url, {headers: {...(token ? {Authorization: `Bearer ${token}`} : {}), ...headers},
    handshakeTimeout: 2000}); // Loopback handshake budget: 1s scheduling + 1s failure margin.
  const queue = [], waiting = [];
  socket.on('message', data => {
    const value = JSON.parse(data);
    if (waiting.length) waiting.shift()(value); else queue.push(value);
  });
  const closed = new Promise(resolve => socket.once('close', (code, reason) => resolve({code, reason: reason.toString()})));
  socket.on('error', () => {}); // Expected handshake rejections are observed by once(open), below.
  return {socket, closed, opened: once(socket, 'open'), send: value => socket.send(JSON.stringify(value)),
    next: async () => {
      if (queue.length) return queue.shift();
      let timer, receiver;
      try {
        return await new Promise((resolve, reject) => {
          receiver = resolve; waiting.push(receiver);
          timer = setTimeout(() => reject(new Error('relay response timed out')), 2000);
        });
      } finally {
        clearTimeout(timer);
        const index = waiting.indexOf(receiver); if (index >= 0) waiting.splice(index, 1);
      }
    }};
}

/// 场景：受邀手机经真实本机 WebSocket 中继连接受信 Mac，转发成功不会产生送达回执。
/// 数据构造：1 张测试 P-256 证书、2 个受邀手机、1 条在线通道；载荷为 4 字节非文本。
///   每次等待上限 2s = 本机调度余量 1s + 故障反馈余量 1s，避免网络失败无限挂住测试。
/// 执行过程：先查离线 → Mac 签一次性挑战上线 → 手机开通道 → 双向转发 → 另一手机越权 → Mac 断线。
/// 预期结果：只向所属连接转发原字节；不能串流、顶掉在线 Mac；断线后恢复离线且无 accepted/delivered。
///   重启后验证 8 通道上限（第 9 条 = 8 + 1）、256 KiB 载荷边界和 120 次/分钟限制。
async function checkRelayRouting() {
  const directory = mkdtempSync(join(tmpdir(), 'norma-relay-'));
  const keyPath = join(directory, 'key.pem'), certPath = join(directory, 'cert.pem');
  execFileSync('openssl', ['req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:P-256',
    '-nodes', '-keyout', keyPath, '-out', certPath, '-subj', '/CN=norma-relay-test', '-days', '1'], {stdio: 'ignore'});
  const cert = new X509Certificate(readFileSync(certPath)), key = readFileSync(keyPath);
  const machineID = 'mac-' + createHash('sha256').update(cert.raw).digest('hex');
  const admin = 'relay-test-admin-secret-at-least-24-characters';
  const env = {COMPANION_ACCESS_TOKEN: admin, PUBLIC_ORIGIN: 'http://127.0.0.1:8787',
    NORMA_CLOUD_DEVICE_STATE: join(directory, 'devices.json'),
    NORMA_RELAY_TRUSTED_CERTIFICATES: JSON.stringify([cert.raw.toString('base64')])};
  let time = Date.now();
  let server = makeGateway({env, now: () => time});
  const peers = [];
  const listen = async () => { server.listen(0, '127.0.0.1'); await once(server, 'listening');
    return `http://127.0.0.1:${server.address().port}`; };
  let base = await listen();
  const post = (path, body, token) => fetch(base + path, {method: 'POST',
    headers: {'Content-Type': 'application/json', Authorization: `Bearer ${token}`}, body: JSON.stringify(body)});
  const invite = async name => {
    const {code} = await (await post('/v1/admin/devices/invite', {}, admin)).json();
    const response = await post('/v1/devices/claim', {code, name}, '');
    assert.equal(response.status, 200, '中继手机应复用真实邀请准入，不能伪造认证结果');
    return response.json();
  };
  const connect = async (role, token, headers) => {
    const connection = peer(base.replace('http:', 'ws:') + '/v1/relay/' + role, token, headers);
    peers.push(connection); await connection.opened; return connection;
  };
  const register = async connection => {
    const challenge = await connection.next();
    assert.equal(challenge.type, 'challenge', 'Mac 首先应收到一次性签名挑战');
    assert.equal(challenge.audience, env.PUBLIC_ORIGIN, '签名必须绑定当前中继部署');
    assert.match(challenge.challenge, /^[0-9a-f]{64}$/, '挑战应为 32 字节随机数');
    const signed = Buffer.from(`Norma relay registration v1\n${challenge.audience}\n${challenge.challenge}`);
    const registration = {type: 'register', machine_id: machineID, signature: sign('sha256', signed, key).toString('hex')};
    connection.send(registration);
    return {reply: await connection.next(), registration};
  };
  const open = (phone, machine = machineID) => {
    const requestID = randomUUID(); phone.send({type: 'open', request_id: requestID, machine_id: machine}); return requestID;
  };
  try {
    const firstDevice = await invite('iPhone A'), secondDevice = await invite('iPhone B');
    await assert.rejects(connect('phone', 'invalid-token'), /401/, '匿名或无效凭据不能占用中继');
    await assert.rejects(connect('phone', firstDevice.token, {Origin: ''}), /404/,
      '即使 Origin 为空也属于浏览器来源，原生专用中继必须拒绝');
    const phone = await connect('phone', firstDevice.token);
    const offlineID = open(phone);
    assert.deepEqual(await phone.next(), {type: 'offline', request_id: offlineID, machine_id: machineID},
      'Mac 离线时中继不能建立持久队列或谎报已送达');
    const expired = await connect('mac'), lateChallenge = await expired.next();
    time += 10_000; // Exactly the 10s signature deadline, without sleeping or delaying other tests.
    expired.send({type: 'register', machine_id: machineID, signature: sign('sha256',
      Buffer.from(`Norma relay registration v1\n${lateChallenge.audience}\n${lateChallenge.challenge}`), key).toString('hex')});
    const expiry = await Promise.race([expired.closed, expired.next()]);
    assert.equal(expiry.code, 1008, '到期签名即使先于定时器回调到达，也不能登记在线 Mac');
    const mac = await connect('mac');
    const {reply, registration} = await register(mac);
    assert.deepEqual(reply, {type: 'registered', machine_id: machineID}, '正确证书签名必须注册真实在线路由');
    const replay = await connect('mac');
    await replay.next(); replay.send(registration);
    assert.equal((await replay.closed).code, 1008, '旧挑战签名不能重放到另一连接');
    const duplicate = await connect('mac');
    const duplicateResult = await register(duplicate);
    assert.equal(duplicateResult.reply.error, 'already_online', '同身份第二连接不能顶掉原 Mac');

    const requestID = open(phone);
    const opened = await phone.next(), inbound = await mac.next();
    assert.equal(opened.type, 'open', '已上线 Mac 应能建立转发通道');
    assert.equal(opened.request_id, requestID, '开通道响应必须对应手机本次请求');
    assert.equal(inbound.stream_id, opened.stream_id, '两端必须共享同一随机通道身份');
    assert.equal(inbound.client_id, firstDevice.device_id, 'Mac 仅得到已认证的云设备身份，项目权限仍须自行验证');
    const frame = {type: 'data', stream_id: opened.stream_id, data: Buffer.from([0, 255, 128, 1]).toString('base64')};
    phone.send(frame);
    assert.deepEqual(await mac.next(), frame, '中继必须保持不透明载荷，不解释、修改或生成回执');
    mac.send(frame);
    assert.deepEqual(await phone.next(), frame, 'Mac 到手机的载荷必须返回原通道');
    const stranger = await connect('phone', secondDevice.token);
    stranger.send(frame);
    assert.equal((await stranger.closed).code, 1008, '另一受邀手机不能借已知 stream_id 向该 Mac 注入');
    assert.equal(phone.socket.readyState, WebSocket.OPEN, '越权连接关闭不能波及原手机');

    const revoked = await post('/v1/admin/devices/revoke', {device_id: firstDevice.device_id}, admin);
    assert.equal(revoked.status, 200, '撤销通过既有真实准入接口完成');
    mac.send(frame);
    assert.equal((await phone.closed).code, 1008, '设备撤销后，已经在线的手机也不能继续接收数据');
    assert.equal((await mac.next()).type, 'close', '撤销应释放通道并通知 Mac');

    const survivor = await connect('phone', secondDevice.token);
    open(survivor); const secondStream = await survivor.next(); await mac.next();
    mac.socket.close(); await mac.closed;
    assert.deepEqual(await survivor.next(), {type: 'close', stream_id: secondStream.stream_id, reason: 'peer_disconnected'},
      'Mac 断线必须通知手机，不能把连接存活误报为执行存活');
    const laterID = open(survivor);
    assert.equal((await survivor.next()).request_id, laterID, '断线后再次打开必须返回本次离线结果');
    assert.equal(JSON.stringify(readFileSync(env.NORMA_CLOUD_DEVICE_STATE, 'utf8')).includes(frame.data), false,
      '设备准入数据库不能持久保存中继正文');

    for (const connection of peers) connection.socket.terminate();
    server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
    server = makeGateway({env, now: () => time}); base = await listen();
    const restarted = await connect('phone', secondDevice.token);
    const restartID = open(restarted);
    assert.deepEqual(await restarted.next(), {type: 'offline', request_id: restartID, machine_id: machineID},
      '中继重启只保留设备准入，不恢复幽灵在线 Mac 或重放旧消息');
    await assert.rejects(connect('phone', firstDevice.token), /403/, '撤销状态必须跨服务重启保留');
    const newMac = await connect('mac'); await register(newMac);
    const streamIDs = [];
    for (let index = 0; index < 8; index++) {
      open(restarted); streamIDs.push((await restarted.next()).stream_id); await newMac.next();
    }
    const overLimit = open(restarted);
    assert.deepEqual(await restarted.next(), {type: 'error', request_id: overLimit, error: 'busy'},
      '第九条通道必须明确繁忙，不能无限占用内存');
    restarted.send({type: 'data', stream_id: streamIDs[0], data: Buffer.alloc(256 * 1024, 7).toString('base64')});
    assert.equal(Buffer.from((await newMac.next()).data, 'base64').length, 256 * 1024,
      '恰好上限的密文必须完整转发，不能提前按 base64 长度误判');
    restarted.send({type: 'close', stream_id: streamIDs[0]});
    assert.equal((await restarted.next()).stream_id, streamIDs[0], '主动关闭应明确回应该通道');
    assert.equal((await newMac.next()).stream_id, streamIDs[0], '对端也应收到精确关闭通知');
    open(restarted); const replacement = await restarted.next(); await newMac.next();
    assert.equal(replacement.type, 'open', '关闭旧通道必须释放容量');
    restarted.send({type: 'data', stream_id: replacement.stream_id, data: Buffer.alloc(256 * 1024 + 1).toString('base64')});
    assert.equal((await restarted.closed).code, 1008, '256 KiB + 1 字节必须拒绝，不能截断转发');
    for (let index = 0; index < 8; index++) await newMac.next(); // Each live stream is closed once.
    const malformed = await connect('phone', secondDevice.token);
    open(malformed); const malformedStream = await malformed.next(); await newMac.next();
    malformed.send({type: 'data', stream_id: malformedStream.stream_id, data: 'A==='});
    assert.equal((await malformed.closed).code, 1008, '非规范 base64 不能进入不透明通道');
    await newMac.next();
    const limited = await connect('phone', secondDevice.token);
    const absent = 'mac-' + '0'.repeat(64);
    for (let index = 0; index < 120; index++) { open(limited, absent); await limited.next(); }
    open(limited, absent);
    assert.equal((await limited.closed).reason, 'rate_limit', '同一分钟第 121 条请求必须触发有界限流');
  } finally {
    for (const connection of peers) connection.socket.terminate();
    server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
    rmSync(directory, {recursive: true, force: true});
  }
}

await checkRelayRouting();
console.log('PASS: authenticated relay, opaque routing, isolation, revocation, disconnect and restart');
