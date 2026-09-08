import test from 'node:test';
import assert from 'node:assert/strict';
import { createSession, resolveDecision } from './session.mjs';

// 场景：两个 Mac 任务同时等待决策；选择第二项，只有对应任务恢复运行。
// 构造默认会话，提交 build-preview 的 keep-private 选项，再重放同一请求。
// 预期：另一项仍等待、无真实 Agent 执行、副本重放不再次改变版本。
test('a decision resumes only its own mock task and duplicate requests are idempotent', () => {
  const initial = createSession();
  const command = {
    version: 1, type: 'decision.resolve', requestId: 'request-2',
    decisionId: 'preview-access', optionId: 'keep-private',
  };
  const result = resolveDecision(initial, command);
  assert.equal(result.accepted, true, '合法选项应得到确认');
  assert.equal(result.snapshot.agents.find(a => a.id === 'codex').status, 'working', '预览任务应继续');
  assert.equal(result.snapshot.agents.find(a => a.id === 'claude').status, 'waiting', '审查任务不能被放行');
  assert.deepEqual(result.snapshot.decisions.map(d => d.id), ['review-scope'], '只移除被处理的决策');
  assert.equal(result.snapshot.source, 'mock', '协议必须明确说明数据来自 mock');
  assert.equal(initial.decisions.length, 2, '原快照不能被就地修改');
  const replay = resolveDecision(result.snapshot, command);
  assert.equal(replay.accepted, false, '已处理请求不得重复执行');
  assert.equal(replay.snapshot.sequence, result.snapshot.sequence, '重放不得推进版本');
});
