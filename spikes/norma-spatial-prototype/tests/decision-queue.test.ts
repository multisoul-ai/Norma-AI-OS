import { test } from 'node:test';
import assert from 'node:assert/strict';
import { initialVRState } from '../lib/spatial-activity.ts';
import { command, advance } from '../lib/model.ts';
import { decisionSnapshot, decisionWindow } from '../lib/decisions.ts';

// 场景：多个任务同时等待，先处理队列中间的一项。构造五项待办，选择首页方向；其他任务与当前 CLI 必须保持原样。
test('resolving a non-first decision only resumes its own task', () => {
  const state = initialVRState();
  assert.equal(
    decisionSnapshot(state).items.length,
    5,
    '默认应展示五个独立任务待办',
  );
  const next = command(state, '决策:homepage:calm');
  const paused = command(next, '全部暂停');
  assert.equal(
    paused.decisionTasks.find((task) => task.id === 'homepage')?.status,
    'paused',
    '全部暂停必须涵盖独立任务',
  );
  assert.equal(
    advance(paused).decisionTasks.find((task) => task.id === 'homepage')
      ?.progress,
    0,
    '暂停的独立任务不推进',
  );
  assert.deepEqual(
    next.agents,
    state.agents,
    '首页决策不能重启当前 CLI 或绕过验证范围',
  );
  assert.deepEqual(
    next.decisions.map((d) => d.id),
    state.decisions.filter((d) => d.id !== 'homepage').map((d) => d.id),
    '只移除被处理的待办',
  );
  assert.equal(
    next.decisionTasks.find((t) => t.id === 'homepage')?.status,
    'working',
    '首页任务应恢复执行',
  );
  assert.equal(
    next.decisionTasks.find((t) => t.id === 'preview')?.status,
    'waiting',
    '预览任务继续等待自己的决定',
  );
  assert.equal(
    command(next, '决策:homepage:calm'),
    next,
    '重复提交不得影响其他待办',
  );
  assert.ok(
    advance(next).decisionTasks.find((t) => t.id === 'homepage')!.progress > 0,
    '恢复的任务有可推进的执行状态',
  );
});

// 场景：待办超过一页，最后一页项目被处理。使用五项数据，处理第 5 项后页码应钳回有效页，剩余四项可达且不重复。
test('decision pages retain every pending task and clamp after removal', () => {
  const state = initialVRState();
  const first = decisionWindow(state, 0);
  const second = decisionWindow(state, 1);
  assert.equal(first.items.length, 4, '每页四项，保证 VR 阅读密度');
  assert.equal(second.items.length, 1, '第二页呈现剩余任务');
  assert.equal(
    new Set([...first.items, ...second.items].map((i) => i.id)).size,
    5,
    '所有待办必须可达且无重复',
  );
  const next = command(state, '决策:compatibility:focused');
  assert.equal(decisionWindow(next, 1).page, 0, '删空末页后不能留下空白页');
  assert.equal(decisionWindow(next, 1).items.length, 4, '其余待办仍可处理');
});
