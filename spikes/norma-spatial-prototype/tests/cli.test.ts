import { test } from 'node:test';
import assert from 'node:assert/strict';
import { initialState, command, advance } from '../lib/model.ts';
import { runCli, visibleTranscript } from '../lib/cli.ts';
// 场景：打开原型直接看到开发现场。初始状态必须选择终端，并提供两个相互独立的 CLI 上下文。
test('default workspace is a Codex and Claude CLI working surface', () => {
  const state = initialState();
  assert.equal(state.view, 'canvas', '主面板默认是承载 CLI 的自由画布');
  assert.equal(
    state.sessions.codex.cwd,
    '~/Documents/code/Norma-AI-OS',
    'Codex 显示仓库上下文',
  );
  assert.equal(state.sessions.claude.id, 'claude', 'Claude 有独立会话');
  assert.match(
    visibleTranscript(state, 'codex')
      .map((l) => l.text)
      .join('\n'),
    /Codex/,
    '默认能看到 Codex CLI 内容',
  );
});
// 场景：指令发给 Codex。构造初始状态，执行 CLI 输入，预期仅启动该会话并记录指令。
test('CLI input is scoped and stopping preserves transcript', () => {
  let state = runCli(initialState(), 'codex', '检查终端焦点切换');
  assert.equal(state.agents[0].status, 'working', 'Codex 开始工作');
  assert.equal(state.agents[1].status, 'ready', 'Claude 未被误启动');
  state = advance(state);
  state = runCli(state, 'codex', '/stop');
  assert.equal(state.agents[0].status, 'paused', 'Ctrl+C 对应 stop 能打断执行');
  assert.ok(
    visibleTranscript(state, 'codex').some((l) =>
      l.text.includes('检查终端焦点切换'),
    ),
    '打断保留输入与上下文',
  );
});
// 场景：Codex 完成后交给 Claude 审查。构造两 CLI 独立输入，预期报告包含 diff 路径，原会话历史保留。
test('Claude review reuses the task context without wiping Codex', () => {
  let state = runCli(initialState(), 'codex', '检查 runtime 输入路由');
  for (let i = 0; i < 25; i++) state = advance(state);
  const before = state.sessions.codex;
  state = runCli(state, 'claude', '/review');
  assert.deepEqual(state.sessions.codex, before, '审查不会清空 Codex 历史');
  assert.equal(state.agents[1].status, 'working', 'Claude 启动审查');
  assert.match(
    state.sessions.claude.prompt,
    /审查/,
    'Claude 收到明确审查上下文',
  );
});
// 场景：语音指令打开某 CLI。执行打开与内容切换，预期只导航，不能误创建新任务。
test('natural language can bring Codex or Claude to the main panel', () => {
  const state = command(initialState(), '把 Claude CLI 放到主面板');
  assert.equal(state.view, 'terminal', '应打开终端');
  assert.equal(state.activeCli, 'claude', '选中指定的 Claude CLI');
  assert.equal(
    state.agents.some((a) => a.status === 'working'),
    false,
    '导航不会启动任务',
  );
});
