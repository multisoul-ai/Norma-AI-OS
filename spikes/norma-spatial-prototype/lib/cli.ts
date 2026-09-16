import type { State } from './model.ts';
export type CliId = 'codex' | 'claude';
export type CliLine = {
  kind:
    | 'brand'
    | 'muted'
    | 'prompt'
    | 'tool'
    | 'success'
    | 'warning'
    | 'text'
    | 'diff';
  text: string;
};
export type CliSession = {
  id: CliId;
  cwd: string;
  branch: string;
  prompt: string;
  history: CliLine[];
  offset: number;
  run: number;
};
export const developmentGoal =
  'Codex 检查终端焦点路由，Claude 审查改动并补齐测试';
export const repoContext = {
  name: 'Norma-AI-OS',
  branch: 'codex/focus-routing',
  language: 'Rust · GPUI',
  files: [
    'crates/runtime/src/route.rs',
    'crates/runtime/tests/unit/agent_manual_interrupt_tests.rs',
    'crates/app/src/cards/term/view.rs',
  ],
};
export function initialSessions(): Record<CliId, CliSession> {
  const create = (id: CliId): CliSession => ({
    id,
    cwd: '~/Documents/code/Norma-AI-OS',
    branch: repoContext.branch,
    prompt: '',
    run: 0,
    offset: 0,
    history: [
      {
        kind: 'brand',
        text:
          id === 'codex'
            ? '╭─ OpenAI Codex ──────────────────────────╮'
            : '╭─ Claude Code ───────────────────────────╮',
      },
      { kind: 'muted', text: '│  Norma-AI-OS · Rust workspace            │' },
      { kind: 'muted', text: '╰─────────────────────────────────────────╯' },
      { kind: 'muted', text: `workspace  ~/Documents/code/Norma-AI-OS` },
      { kind: 'muted', text: `branch     ${repoContext.branch}  [模拟分支]` },
      { kind: 'muted', text: 'mode       演示会话 · 不执行真实命令' },
      { kind: 'text', text: '' },
      {
        kind: 'text',
        text:
          id === 'codex'
            ? '我可以读取代码、实施改动并运行验证。'
            : '我会独立检查 Codex 的改动，关注边界与回归。',
      },
      { kind: 'text', text: '直接输入任务开始，随时可以中断与继续。' },
    ],
  });
  return { codex: create('codex'), claude: create('claude') };
}
export function transcriptAt(state: State, id: CliId): CliLine[] {
  const session = state.sessions[id];
  const agent = state.agents.find((a) => a.id === id)!;
  const lines = [...session.history];
  if (session.prompt) {
    lines.push({ kind: 'prompt', text: `› ${session.prompt}` });
    const steps: [number, CliLine['kind'], string][] =
      id === 'codex'
        ? [
            [0, 'text', '先读取仓库规范和输入路由，再决定最小修改。'],
            [5, 'tool', '• Read AGENTS.md, crates/runtime/src/route.rs'],
            [12, 'muted', '  InputMode::NodeFocused → Route::SendInputToNode'],
            [
              20,
              'tool',
              '• Search "Escape|NodeFocused|CloseNode" in crates/runtime',
            ],
            [29, 'text', '  已定位：焦点、选择与输入投递应分别验证。'],
            [38, 'tool', '• Write focused regression cases [模拟]'],
            [47, 'diff', '  + focused_node_routes_key_to_active_session()'],
            [53, 'diff', '  + escape_exits_focus_without_closing_session()'],
            [61, 'tool', '• Run cargo test -p runtime [模拟命令]'],
            [69, 'muted', '  compiling runtime …'],
            [
              78,
              'success',
              '  PASS · focused route cases  2 passed [模拟结果]',
            ],
            [86, 'tool', '• Inspect git diff --stat [模拟命令]'],
            [93, 'text', '  2 个文件待审阅 · +24 / −3 [演示改动]'],
            [100, 'success', '✓ 实现阶段完成。可以交给 Claude 独立审查。'],
          ]
        : [
            [0, 'text', '我会先看目标和 diff，再检查是否引入行为回归。'],
            [7, 'tool', '● Read AGENTS.md + Codex task context'],
            [16, 'tool', '● Inspect crates/runtime/src/route.rs'],
            [27, 'text', '  检查焦点退出、键盘转发和关闭节点三条路径。'],
            [40, 'tool', '● Review regression tests [模拟]'],
            [53, 'warning', '  Review note: 增加关闭非焦点节点的负断言。'],
            [63, 'tool', '● Add negative assertion to route tests [模拟]'],
            [76, 'tool', '● Run cargo test -p runtime [模拟命令]'],
            [89, 'success', '  PASS · 审查建议已覆盖 [模拟结果]'],
            [100, 'success', '✓ 审查完成。无阻塞项，等待你确认演示 diff。'],
          ];
    for (const [at, kind, text] of steps)
      if (agent.progress >= at) lines.push({ kind, text });
  }
  if (agent.status === 'paused')
    lines.push({
      kind: 'warning',
      text: '■ 已中断。上下文保留，输入 /resume 继续。',
    });
  return lines;
}
export function visibleTranscript(state: State, id: CliId) {
  return transcriptAt(state, id).slice(state.sessions[id].offset);
}
export function startCliTask(state: State, id: CliId, prompt: string): State {
  const old = state.sessions[id];
  const history = old.prompt ? transcriptAt(state, id).slice(-90) : old.history;
  return {
    ...state,
    activeCli: id,
    view: 'terminal',
    approved: false,
    revision: state.revision + 1,
    agents: state.agents.map((a) =>
      a.id === id ? { ...a, status: 'working', progress: 0, task: prompt } : a,
    ),
    sessions: {
      ...state.sessions,
      [id]: { ...old, prompt, history, offset: 0, run: old.run + 1 },
    },
  };
}
export function runCli(state: State, id: CliId, raw: string): State {
  const text = raw.trim().slice(0, 500);
  if (!text) return state;
  const base = { ...state, activeCli: id, revision: state.revision + 1 };
  if (text === '/stop' || text === '\u0003')
    return {
      ...base,
      agents: state.agents.map((a) =>
        a.id === id && a.status === 'working' ? { ...a, status: 'paused' } : a,
      ),
    };
  if (text === '/resume')
    return {
      ...base,
      agents: state.agents.map((a) =>
        a.id === id && a.status === 'paused' ? { ...a, status: 'working' } : a,
      ),
    };
  if (text === '/clear')
    return {
      ...base,
      sessions: {
        ...state.sessions,
        [id]: { ...state.sessions[id], offset: transcriptAt(state, id).length },
      },
    };
  if (text === '/diff') return { ...base, view: 'code' };
  if (text === '/review')
    return startCliTask(
      base,
      'claude',
      `审查 Codex 的当前改动：${state.mission}`,
    );
  if (text === '/test')
    return {
      ...base,
      view: 'data',
      agents: state.agents.map((a) =>
        a.id === 'verifier'
          ? {
              ...a,
              status: 'working',
              progress: 0,
              task: '运行 runtime 聚焦验证（模拟）',
            }
          : a,
      ),
    };
  return startCliTask({ ...base, mission: text }, id, text);
}
