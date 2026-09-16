import { test } from 'node:test';
import assert from 'node:assert/strict';
import { command, advance } from '../lib/model.ts';
import { initialVRState } from '../lib/spatial-activity.ts';
import { decisionSnapshot } from '../lib/decisions.ts';

// 场景：验证任务等待范围决策。数据：VR 演示任务；推进与“全部继续”均不能越过决策，选择后只解锁验证 Agent。
test('pending decision blocks only its agent until an explicit choice', () => {
  const before = initialVRState();
  assert.equal(before.agents[2].status, 'waiting', '验证 Agent 应明确等待用户');
  const ticking = advance(command(before, '全部继续'));
  assert.equal(
    ticking.agents[2].progress,
    before.agents[2].progress,
    '普通继续不能绕过决策',
  );
  assert.ok(
    ticking.agents[0].progress > before.agents[0].progress,
    'Codex 不受其他任务的决策阻塞',
  );
  const expanded = command(ticking, '决策:verification:full');
  assert.match(
    expanded.agents[2].task,
    /扩展回归测试/,
    '替代选择必须改变实际验证任务',
  );
  const next = command(ticking, '决策:verification:focused');
  assert.equal(next.agents[2].status, 'working', '选择验证范围后继续执行');
  assert.deepEqual(
    next.agents.slice(0, 2),
    ticking.agents.slice(0, 2),
    '不得重启其他 Agent',
  );
  assert.equal(
    decisionSnapshot(next).items.length,
    4,
    '已处理事项应从待决策卡片移除',
  );
  assert.equal(
    command(next, '决策:verification:focused'),
    next,
    '重复点击旧决策应无副作用',
  );
});

// 场景：执行结束后审阅成果。数据：解锁验证并推进至完成；查看详情不批准，确认后清空队列。
test('completed task presents review decision and waits for explicit approval', () => {
  let state = command(initialVRState(), '决策:verification:focused');
  for (let i = 0; i < 35; i++) state = advance(state);
  assert.equal(
    decisionSnapshot(state).items.find((item) => item.id === 'review')?.id,
    'review',
    '完成后应出现成果决策',
  );
  const returned = command(state, 'Codex 根据审查记录补充输入路由边界检查');
  assert.equal(returned.agents[0].status, 'working', '退回检查应启动补充任务');
  assert.equal(returned.agents[1].status, 'done', '退回 Codex 不重启 Claude');
  assert.equal(
    decisionSnapshot(returned).items.some((item) => item.id === 'review'),
    false,
    '补充执行期间不残留过期验收',
  );
  state = command(state, '展示代码改动');
  assert.equal(state.approved, false, '打开上下文不能等同确认');
  state = command(state, '确认改动');
  assert.equal(state.approved, true, '用户确认才完成交付');
  assert.equal(
    decisionSnapshot(state).items.length,
    4,
    '确认当前任务后，其他四项仍需处理',
  );
});
