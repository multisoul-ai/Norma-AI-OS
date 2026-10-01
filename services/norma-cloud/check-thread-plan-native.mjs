import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {randomBytes, randomUUID} from 'node:crypto';
import {once} from 'node:events';
import {mkdtempSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {makeGateway} from './server.mjs';

/// 正式Swift客户端/存储经回环HTTP调用正式网关；只有供应商为明确测试数据。
/// 数据：1手机、2同名成员；3次模型调用=合法执行+讨论+截断，重复和撤权不调用上游。
/// 执行：隔离服务→Swift兑换/请求/落盘/重开/负例→核对次数→清理服务与凭据。
/// 预期：只有iOS Alice有待发任务，不伪造Mac回执，不访问生产。
const root = fileURLToPath(new URL('../../', import.meta.url));
const directory = mkdtempSync(join(tmpdir(), 'norma-plan-native-'));
const admin = randomBytes(32).toString('base64url');
let calls = 0;
const server = makeGateway({env: {COMPANION_ACCESS_TOKEN: admin,
  NORMA_CLOUD_DEVICE_STATE: join(directory, 'devices.json'), DOUBAO_SPEECH_API_KEY: 'test-provider-only'},
  fetchImpl: async (_url, options) => {
    calls++;
    const input = JSON.parse(JSON.parse(options.body).messages.at(-1).content);
    const candidate = {reply: input.message.includes('讨论') ? '先讨论首屏目标。' : '由 iOS Alice 检查首屏。',
      assignments: input.message.includes('讨论') ? [] : [{target_id: input.agents[0].id, task: '检查首屏导航'}]};
    return Response.json({choices: [{finish_reason: input.message.includes('截断') ? 'length' : 'stop',
      message: {content: JSON.stringify(candidate)}}]});
  }});
try {
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const path = join(directory, 'fixture.json');
  writeFileSync(path, JSON.stringify({port: server.address().port, admin, namespace: randomUUID()}), {mode: 0o600});
  if (process.argv.includes('--serve-ui')) {
    console.log(`Local UI test provider ready. NORMA_THREAD_PLAN_FIXTURE=${path}`);
    await once(process, 'SIGINT');
  } else {
  const child = spawn('swift', ['test', '--disable-sandbox', '--package-path', join(root, 'ios'),
    '--cache-path', join(root, 'target/swift-cache'), '--filter', 'ThreadPlanNetworkTests'], {
    cwd: root, env: {...process.env, NORMA_THREAD_PLAN_FIXTURE: path}, stdio: 'inherit'});
  // Cached compile 60s + local HTTP assertions/scheduling 60s = 120s.
  const timeout = setTimeout(() => child.kill('SIGTERM'), 120_000);
  try {
    const [code, signal] = await once(child, 'exit');
    assert.equal(signal, null, 'Swift HTTP验收必须正常结束');
    assert.equal(code, 0, 'Swift正式客户端编排链路必须通过');
  } finally { clearTimeout(timeout); }
  assert.equal(calls, 3, '只有执行、讨论和截断三轮调用供应商');
  console.log('PASS: Swift ↔ local gateway, exact target, atomic persistence, discussion, dedup and revoke (test provider).');
  }
} finally {
  server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
  rmSync(directory, {recursive: true, force: true});
}
