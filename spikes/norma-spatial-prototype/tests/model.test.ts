import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  initialState,
  command,
  advance,
  adjustPanel,
  type State,
} from '../lib/model.ts';

// 场景：自然语言任务被拆成协作计划。构造初始状态，提交目标，预期三位 Agent 开始工作且保留用户原话。
test('a new mission starts three mock agents and preserves intent', () => {
  const state = command(initialState(), '帮我设计一个月球旅行网站');
  assert.equal(
    state.agents.filter((a) => a.status === 'working').length,
    3,
    '三位 Agent 应并行启动',
  );
  assert.equal(state.mission, '帮我设计一个月球旅行网站', '必须保留用户任务');
  assert.equal(state.messages.at(-2)?.role, 'user', '对话中应记录用户输入');
  assert.equal(state.approved, false, '任务不应自动被批准');
});
// 场景：执行中暂停再继续。推进十个 tick 后暂停，预期进度冻结，恢复后继续，旧结果不被删除。
test('pause freezes progress and resume continues', () => {
  let state = command(initialState(), '开始执行');
  state = advance(state);
  state = command(state, '全部暂停');
  const progress = state.agents.map((a) => a.progress);
  assert.deepEqual(
    advance(state).agents.map((a) => a.progress),
    progress,
    '暂停不能继续推进',
  );
  assert.equal(
    state.agents.some((a) => a.status === 'working'),
    false,
    '暂停后不得有执行中的 Agent',
  );
  state = advance(command(state, '继续执行'));
  assert.ok(
    state.agents.some((a, i) => a.progress > progress[i]),
    '恢复应从已有进度继续',
  );
});
// 场景：点名控制且上下文不丢失。启动任务后暂停 Codex，预期只改变目标 Agent。
test('named agent commands are scoped', () => {
  const state = command(command(initialState(), '开始执行'), '暂停 Codex');
  assert.equal(state.agents[0].status, 'paused', 'Codex 应暂停');
  assert.equal(state.agents[1].status, 'working', '其他 Agent 应继续');
  assert.equal(
    command(state, '把代码放到主面板').view,
    'code',
    '内容指令应切换主面板',
  );
});
// 场景：空间窗口安全边界。输入超量缩放和距离，预期钳制到可读区间，固定后位置不能改变。
test('spatial transforms are bounded and pin respects position', () => {
  let panel = adjustPanel(
    { x: 0, y: 0, scale: 1, depth: 2, pinned: false },
    { scale: 10, depth: -3 },
  );
  assert.equal(panel.scale, 1.35, '缩放最大 1.35 避免占满视野');
  assert.equal(panel.depth, 1.2, '距离至少 1.2m 保持舒适');
  panel = adjustPanel({ ...panel, pinned: true }, { x: 99, y: 99 });
  assert.equal(panel.x, 0, '固定窗口不得水平漂移');
  assert.equal(panel.y, 0, '固定窗口不得垂直漂移');
});
// 场景：完整任务审批。持续推进至完成，预期等待用户审阅；批准后才完成交付标记。
test('completion produces reviewable artifacts and needs explicit approval', () => {
  let state: State = command(initialState(), '开始执行');
  for (let i = 0; i < 35; i++) state = advance(state);
  assert.ok(
    state.agents.every((a) => a.status === 'done'),
    '所有产物应就绪',
  );
  assert.equal(state.approved, false, '完成不等于用户批准');
  assert.equal(command(state, '批准方案').approved, true, '明确批准才交付');
});
// 场景：语音在 CLI 与画布间导航。打开 Claude 后说返回画布，预期只切视图，不改变任务或启动 Agent。
test('returning to canvas preserves task and CLI selection', () => {
  const before = command(initialState(), '打开 Claude CLI');
  const next = command(before, '返回自由画布');
  assert.equal(next.view, 'canvas', '导航应返回自由画布');
  assert.equal(next.mission, before.mission, '导航不能成为新任务');
  assert.equal(next.activeCli, 'claude', '保留刚才的 CLI 选择');
});
