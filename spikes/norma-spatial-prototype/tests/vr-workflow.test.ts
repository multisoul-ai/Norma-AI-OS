import { test } from 'node:test';
import assert from 'node:assert/strict';
import { initialState, command } from '../lib/model.ts';
import {
  spaceMenu,
  contentMenu,
  conversationSuggestions,
} from '../lib/spatial-workflow.ts';
// 场景：用户留在 VR 中选择内容。数据：默认空间菜单；执行查询内容入口，预期可以直接查看 CLI、报告、预览和三维对象。
test('VR exposes content and conversation only through contextual menus', () => {
  assert.ok(
    spaceMenu.some((item) => item.action === 'menu:content'),
    '空间菜单提供内容入口',
  );
  assert.ok(
    spaceMenu.some((item) => item.action === 'dialogue'),
    '空间菜单提供对话演示入口',
  );
  for (const kind of ['code', 'research', 'data', 'preview', 'model'])
    assert.ok(
      contentMenu.some((item) => item.action === `view:${kind}`),
      `可在 VR 中切换 ${kind}`,
    );
  assert.ok(
    !spaceMenu.some((item) => item.action === 'nearer'),
    '低频距离控制只放在布局子菜单',
  );
});
// 场景：没有麦克风也能预演连续指挥。数据：开始协作、暂停 Codex、继续 Codex；执行同一命令 reducer，预期 Claude 持续工作，Codex 可被中断并保留目标。
test('mock dialogue demonstrates delegation and interruption without leaving VR', () => {
  let state = initialState();
  state = command(state, conversationSuggestions(state)[0].text);
  assert.equal(
    state.agents.filter((a) => a.status === 'working').length,
    3,
    '第一句应启动三个模拟 Agent',
  );
  state = command(state, conversationSuggestions(state)[0].text);
  assert.equal(state.agents[0].status, 'paused', '第二句应中断 Codex');
  assert.equal(state.agents[1].status, 'working', '中断不影响 Claude');
  const goal = state.sessions.codex.prompt;
  state = command(state, conversationSuggestions(state)[0].text);
  assert.equal(state.agents[0].status, 'working', '第三句恢复 Codex');
  assert.equal(state.sessions.codex.prompt, goal, '恢复保留原目标');
});
