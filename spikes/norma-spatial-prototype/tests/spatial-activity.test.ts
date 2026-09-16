import { test } from 'node:test';
import assert from 'node:assert/strict';
import { initialState, command, advance } from '../lib/model.ts';
import { developmentGoal } from '../lib/cli.ts';
import { activitySnapshot } from '../lib/spatial-activity.ts';
// 场景：任务执行与内容展示各自独立。数据：三个正在运行的 Agent；推进十个 tick 后切换报告、暂停 Codex，再推进，预期进度仍可见且准确区分运行和暂停。
test('spatial activity remains available across content changes', () => {
  let state = command(initialState(), developmentGoal);
  for (let i = 0; i < 10; i++) state = advance(state);
  state = command(state, '展示审查报告');
  let snapshot = activitySnapshot(state);
  assert.equal(snapshot.running, 3, '切换成果不隐藏正在执行的三位 Agent');
  assert.equal(
    snapshot.progress,
    43,
    '10 次推进产生 50/40/40，均值四舍五入为 43',
  );
  state = command(state, '暂停 Codex');
  const before = state.agents[0].progress;
  state = advance(state);
  snapshot = activitySnapshot(state);
  assert.equal(snapshot.running, 2, '暂停后仍明确显示两位 Agent 执行中');
  assert.equal(snapshot.rows[0].status, '已暂停', 'Codex 行提供状态反馈');
  assert.equal(
    snapshot.rows[0].progress,
    before,
    '暂停的 Agent 进度不会继续变化',
  );
  assert.equal(snapshot.rows[1].progress, 44, 'Claude 的进度继续推进');
  assert.ok(snapshot.rows[1].detail.length > 0, '运行行提供当前工具或步骤');
  assert.equal(
    snapshot.rows[0].action,
    'command:打开 Codex CLI',
    '点击执行状态可以进入对应 CLI',
  );
  assert.equal(snapshot.rows[2].action, 'view:data', '验证行打开测试结果');
});

// 场景：仅向一个 Agent 发出新任务。执行：推进 Codex 到完成；预期空闲 Agent 不稀释该任务进度。
test('idle agents do not dilute a single-agent task progress', () => {
  let state = command(initialState(), 'Codex 检查输入路由');
  for (let i = 0; i < 20; i++) state = advance(state);
  assert.equal(
    activitySnapshot(state).progress,
    100,
    '仅 Codex 参与时，其完成应显示任务 100%',
  );
  assert.equal(
    activitySnapshot(state).running,
    0,
    '完成后不再有运行中的 Agent',
  );
  assert.equal(
    state.agents[1].status,
    'ready',
    'Claude 保持空闲，不视为未完成的参与者',
  );
});
