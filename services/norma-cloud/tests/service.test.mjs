import test from 'node:test';
import assert from 'node:assert/strict';
import {once} from 'node:events';
import {generateKeyPairSync} from 'node:crypto';
import {chmodSync, mkdtempSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import WebSocket from 'ws';
import {makeGateway} from '../server.mjs';
import {createRelay} from '../push/relay.mjs';

/// 场景：单一监听端口同时提供云端与推送入口，凭据不能跨模块使用。
/// 数据构造：回环端口 0 由系统分配；relay key 为 32 bytes = 64 hex 字符。
///   固定 now = 1800000000 秒，事件 expires_at = now + 300，等于原推送有效期上限。
///   1 个 completed 事件重复提交 2 次，只允许 1 次 APNs 投递。
/// 执行过程：启动统一网关→查询两个健康入口→交叉使用凭据→正常领取语音票据与投递→尝试未授权 WSS。
/// 预期结果：健康入口均 200；错误凭据均 401 且未投递；重复事件只投递一次；WSS 鉴权与未知路径仍拒绝。
test('one port serves cloud and push without sharing authorization', async t => {
  const now = 1800000000, key = 'c'.repeat(64), admin = 'cloud-admin-at-least-24-characters';
  const sent = [];
  const pushRelay = createRelay({key, now: () => now * 1000, deliver: async event => {
    sent.push(event); return {status: 200, body: {status: 'accepted'}};
  }});
  const gateway = makeGateway({pushRelay, env: {COMPANION_ACCESS_TOKEN: admin, OPENAI_API_KEY: 'test-only'},
    fetchImpl: async () => ({ok: true, json: async () => ({value: 'temporary-token', expires_at: now + 60})})});
  t.after(async () => {gateway.closeAllConnections(); await new Promise(resolve => gateway.close(resolve));});
  gateway.listen(0, '127.0.0.1'); await once(gateway, 'listening');
  const base = `http://127.0.0.1:${gateway.address().port}`;
  assert.equal((await fetch(base + '/health')).status, 200, '云端健康入口必须由统一端口提供');
  assert.equal((await fetch(base + '/healthz')).status, 200, '推送健康入口必须由同一个端口提供');
  const post = (path, token, body) => fetch(base + path, {method: 'POST',
    headers: {Authorization: `Bearer ${token}`, 'Content-Type': 'application/json'}, body: JSON.stringify(body)});
  const event = {event_id: '00000000-0000-4000-8000-000000000001', token: 'a'.repeat(64),
    expires_at: now + 300, kind: 'completed', machine_id: `mac-${'b'.repeat(64)}`,
    workspace_id: 'workspace', route_id: 'route', node_id: '7', run_id: 'run', session_id: 'session', turn_id: 'turn'};
  assert.equal((await post('/v1/notifications', admin, event)).status, 401, '云端管理凭据不能发送推送');
  assert.equal(sent.length, 0, '拒绝的请求不能进入 APNs');
  assert.equal((await post('/v1/session', key, {provider: 'openai'})).status, 401, '推送凭据不能领取语音票据');
  assert.equal((await post('/v1/session', admin, {provider: 'openai'})).status, 200, '云端原有鉴权和会话创建必须保留');
  assert.equal((await post('/v1/notifications', key, event)).status, 200, '正确推送凭据必须允许投递');
  assert.equal((await post('/v1/notifications', key, event)).status, 200, '重复提交必须返回已有结果');
  assert.equal(sent.length, 1, '统一服务不能导致重复 APNs 投递');
  assert.equal(sent[0].node_id, '7', '投递事件必须保留具体卡片身份');
  assert.equal((await fetch(base + '/unknown')).status, 404, '未知入口不能被推送模块吞掉');
  const peer = new WebSocket(base.replace('http:', 'ws:') + '/v1/voice');
  peer.on('error', () => {});
  const status = await new Promise(resolve => peer.once('unexpected-response', (req, res) => {
    resolve(res.statusCode); req.destroy();
  }));
  assert.equal(status, 401, '统一端口必须继续执行 WebSocket 票据鉴权');
});

/// 场景：正式统一入口从同一组环境配置启动推送，部分配置或不安全的密钥文件必须拒绝启动。
/// 数据构造：临时 P-256 私钥，权限 0600；32 bytes = 64 hex 的合成 relay key；不向 Apple 投递。
///   错误权限 0644 = owner 读写 + group/other 可读，违反原服务 owner-only 约束。
/// 执行过程：只给 APNS_KEY_PATH→拒绝；补齐配置→同端口查询两个健康入口→扩大私钥读取权限→拒绝。
/// 预期结果：缺少配置和权限错误均抛出明确错误；完整配置下两模块都就绪，云端口令不能发送推送。
test('environment configuration starts push on the cloud port and fails closed', async t => {
  const directory = mkdtempSync(join(tmpdir(), 'norma-unified-push-'));
  t.after(() => rmSync(directory, {recursive: true, force: true}));
  const keyPath = join(directory, 'apns.p8'), relayPath = join(directory, 'relay.key');
  const {privateKey} = generateKeyPairSync('ec', {namedCurve: 'prime256v1'});
  writeFileSync(keyPath, privateKey.export({type: 'pkcs8', format: 'pem'}), {mode: 0o600});
  writeFileSync(relayPath, 'c'.repeat(64), {mode: 0o600});
  const env = {COMPANION_ACCESS_TOKEN: 'cloud-admin-at-least-24-characters', APNS_KEY_PATH: keyPath};
  assert.throws(() => makeGateway({env}), /APNs|Secret files/, '部分推送配置不能被静默当作已禁用');
  Object.assign(env, {APNS_TEAM_ID: 'TEAM123456', APNS_KEY_ID: 'KEY1234567',
    APNS_ENVIRONMENT: 'sandbox', NORMA_PUSH_RELAY_KEY_FILE: relayPath});
  const gateway = makeGateway({env});
  t.after(async () => {gateway.closeAllConnections(); await new Promise(resolve => gateway.close(resolve));});
  gateway.listen(0, '127.0.0.1'); await once(gateway, 'listening');
  const base = `http://127.0.0.1:${gateway.address().port}`;
  assert.equal((await fetch(base + '/health')).status, 200, '完整配置不能阻止原云端入口启动');
  assert.equal((await fetch(base + '/healthz')).status, 200, '推送配置必须由统一入口自动加载');
  const denied = await fetch(base + '/v1/notifications', {method: 'POST',
    headers: {Authorization: `Bearer ${env.COMPANION_ACCESS_TOKEN}`, 'Content-Type': 'application/json'}, body: '{}'});
  assert.equal(denied.status, 401, '正式环境配置也必须保留独立推送鉴权');
  chmodSync(keyPath, 0o644);
  assert.throws(() => makeGateway({env}), /owner-only/, '不能加载其他用户可读的 APNs 私钥');
});
