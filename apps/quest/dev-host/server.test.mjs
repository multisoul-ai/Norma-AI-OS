import test from 'node:test';
import assert from 'node:assert/strict';
import { once } from 'node:events';
import WebSocket from 'ws';
import { startHost } from './server.mjs';

function inbox(socket) {
  const messages = [];
  const waiters = [];
  socket.on('message', data => {
    const value = JSON.parse(data.toString());
    const resolve = waiters.shift();
    if (resolve) resolve(value); else messages.push(value);
  });
  return () => messages.length ? Promise.resolve(messages.shift()) : new Promise(resolve => waiters.push(resolve));
}

// 场景：两个 Quest 客户端通过真实 loopback WebSocket 连接同一 Mac mock 会话。
// 构造临时端口，完成 hello，先提交无效选项，再提交合法决策并重连。
// 预期：无效选项不改变会话；合法选择广播给两端；重连获得当前快照。
test('the wire protocol validates decisions, broadcasts snapshots and survives reconnect', { timeout: 5000 }, async t => {
  const host = await startHost({ port: 0 });
  t.after(() => host.close());
  const clients = [new WebSocket(host.url), new WebSocket(host.url)];
  const receive = clients.map(inbox);
  t.after(() => clients.forEach(client => client.terminate()));
  await Promise.all(clients.map(client => once(client, 'open')));
  clients.forEach(client => client.send(JSON.stringify({ version: 1, type: 'hello' })));
  assert.equal((await receive[0]()).decisions.length, 2, '初次连接应收到两个待办');
  assert.equal((await receive[1]()).sequence, 1, '第二端应看到同一会话版本');
  clients[0].send(JSON.stringify({ version: 1, type: 'decision.resolve', requestId: 'bad', decisionId: 'preview-access', optionId: 'invalid' }));
  assert.equal((await receive[0]()).accepted, false, '非法选项必须拒绝');
  clients[0].send(JSON.stringify({ version: 1, type: 'decision.resolve', requestId: 'good', decisionId: 'preview-access', optionId: 'keep-private' }));
  assert.equal((await receive[0]()).accepted, true, '合法请求应被确认');
  const updated = await receive[0]();
  assert.deepEqual(updated.decisions.map(d => d.id), ['review-scope'], '未处理待办应保留');
  assert.deepEqual(await receive[1](), updated, '两端必须接收同一权威快照');
  const reconnect = new WebSocket(host.url);
  t.after(() => reconnect.terminate());
  const receiveReconnect = inbox(reconnect);
  await once(reconnect, 'open');
  reconnect.send(JSON.stringify({ version: 1, type: 'hello' }));
  assert.equal((await receiveReconnect()).sequence, 2, '重连不得恢复过期初始快照');
});
