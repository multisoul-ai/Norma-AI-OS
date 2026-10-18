import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const source = html.match(/<script id="assistant-resize">([\s\S]*?)<\/script>/)?.[1];
const sandbox = {};
if (source) runInNewContext(source, sandbox);

/// 场景：用户左右拖拽抽屉边缘以扩展宽度。
/// 数据构造：1400px 视口，另一侧抽屉 300px；分别尝试过小、合理与过大的宽度。
/// 执行过程：调用 clamp。
/// 预期结果：宽度被限制在上下限内，并始终为画布保留至少 380px。
test('抽屉宽度受上下限约束并为画布留出空间', () => {
  assert.ok(source, '页面应提供独立的抽屉宽度逻辑');
  assert.equal(sandbox.AssistantResize.clamp('reader', 100, 1400, 300), 340);
  assert.equal(sandbox.AssistantResize.clamp('reader', 600, 1400, 300), 600);
  assert.equal(sandbox.AssistantResize.clamp('reader', 2000, 1400, 300), 720, '1400-300-380');
  assert.equal(sandbox.AssistantResize.clamp('chat', 900, 2400, 0), 560);
});

/// 场景：页面提供左右抽屉的可访问拖拽把手。
/// 预期结果：两个把手均为 separator 且可聚焦。
test('左右抽屉均有可聚焦的 separator 把手', () => {
  for (const id of ['chat-resize', 'reader-resize']) {
    assert.match(html, new RegExp(`id="${id}"[^>]*role="separator"[^>]*tabindex="0"`));
  }
});
