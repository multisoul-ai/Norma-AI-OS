import assert from 'node:assert/strict';
import {once} from 'node:events';
import {mkdtempSync, rmSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {makeGateway} from '../server.mjs';

/// 场景：手机群聊迁到Mac后，云服务不再提供独立线程规划。
/// 数据构造：1次匿名旧规划请求、1次健康查询；上游计数初值0，无真实密钥。
/// 执行过程：启动正式回环Gateway → 请求旧路径 → 请求健康状态 → 检查上游计数。
/// 预期结果：旧路径404；健康查询200；不调用模型、不消耗群聊额度。
const directory = mkdtempSync(join(tmpdir(), 'norma-retired-thread-'));
let calls = 0;
const server = makeGateway({env: {COMPANION_ACCESS_TOKEN: 'local-retirement-fixture-secret-32', DOUBAO_PROTOCOL: 'plan', NORMA_CLOUD_DEVICE_STATE: join(directory, 'devices.json')},
  fetchImpl: async () => { calls++; throw new Error('retired route called a model'); }});
try {
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const base = `http://127.0.0.1:${server.address().port}`;
  const response = await fetch(base + '/v1/threads/plan', {method: 'POST',
    headers: {'Content-Type': 'application/json'}, body: '{}'});
  assert.equal(response.status, 404, 'retired planning route must not remain available');
  assert.equal((await response.json()).error, 'not_found', 'retired route must not request cloud authorization');
  assert.equal((await fetch(base + '/health')).status, 200, 'relay gateway health must remain available');
  assert.equal(calls, 0, 'retired group chat planning must never invoke a provider');
  console.log('PASS: legacy thread planning retired; gateway remains healthy');
} finally { await new Promise(resolve => server.close(resolve)); rmSync(directory, {recursive: true}); }
