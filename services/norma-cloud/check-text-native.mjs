import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {randomBytes} from 'node:crypto';
import {once} from 'node:events';
import {mkdtempSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {makeGateway} from './server.mjs';

/// 场景：Swift 原生客户端通过真实本机 HTTP 使用正式网关的邀请、文字流、去重与撤销。
/// 数据：1 台隔离设备、2 次假供应商调用（成功和中断各一次），所有令牌随机生成，状态位于临时目录。
/// 执行：启动回环网关 → Swift 兑换邀请/收流/重启本机历史/重复请求/中断/撤销 → 核对调用次数 → 清理。
/// 预期：成功正文完整；中断不伪造回复；重复与撤销不调用上游；不访问正式服务或真实模型。
const root = fileURLToPath(new URL('../../', import.meta.url));
const directory = mkdtempSync(join(tmpdir(), 'norma-text-native-'));
const admin = randomBytes(32).toString('base64url');
let calls = 0;
const server = makeGateway({env: {
  COMPANION_ACCESS_TOKEN: admin, NORMA_CLOUD_DEVICE_STATE: join(directory, 'devices.json'),
  DOUBAO_SPEECH_API_KEY: 'isolated-local-test-provider',
}, fetchImpl: async (_url, options) => {
  calls++;
  const interrupted = JSON.parse(options.body).messages.at(-1).content === '模拟中断';
  const content = interrupted
    ? 'data: {"choices":[{"delta":{"content":"尚未完成"}}]}\n\n'
    : 'data: {"choices":[{"delta":{"content":"先把想法"}}]}\n\n'
      + 'data: {"choices":[{"delta":{"content":"留下。"},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n';
  return new Response(content, {headers: {'Content-Type': 'text/event-stream'}});
}});
try {
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const path = join(directory, 'fixture.json');
  writeFileSync(path, JSON.stringify({origin: `http://127.0.0.1:${server.address().port}/`, admin}), {mode: 0o600});
  console.log('Local text gateway ready; running Swift HTTP integration (deterministic test provider).');
  const child = spawn('swift', ['test', '--disable-sandbox', '--package-path', join(root, 'ios'),
    '--cache-path', join(root, 'target/swift-cache'), '--filter', 'CloudTextNetworkTests'], {
    cwd: root, env: {...process.env, NORMA_CLOUD_TEXT_FIXTURE: path}, stdio: 'inherit',
  });
  // 120s = cached Swift compilation allowance 60s + six short HTTP round trips and CI scheduling 60s.
  const timeout = setTimeout(() => child.kill('SIGTERM'), 120_000);
  try {
    const [code, signal] = await once(child, 'exit');
    assert.equal(signal, null, 'Swift integration must finish normally, not time out');
    assert.equal(code, 0, 'Swift client-to-gateway integration failed');
  } finally { clearTimeout(timeout); }
  assert.equal(calls, 2, 'Only the successful and interrupted turns may invoke the provider');
  console.log('PASS: Swift client ↔ local gateway; success, interrupted stream, persistence, dedup and revoke.');
} finally {
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
  rmSync(directory, {recursive: true, force: true});
}
