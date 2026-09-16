import {
  resolveDecision,
  type PendingDecision,
  type DecisionTask,
} from './decisions.ts';
import { initialCanvas, type CanvasDocument } from './free-canvas.ts';
import {
  initialSessions,
  startCliTask,
  runCli,
  developmentGoal,
  type CliId,
  type CliSession,
} from './cli.ts';
export type View =
  | 'canvas'
  | 'terminal'
  | 'preview'
  | 'research'
  | 'code'
  | 'data'
  | 'model';
export type Agent = {
  id: string;
  name: string;
  role: string;
  color: string;
  progress: number;
  status: 'ready' | 'working' | 'paused' | 'waiting' | 'done';
  task: string;
};
export type Message = { role: 'user' | 'norma'; text: string };
export type State = {
  mission: string;
  agents: Agent[];
  messages: Message[];
  view: View;
  approved: boolean;
  decisions: PendingDecision[];
  decisionTasks: DecisionTask[];
  decisionsHandled: number;
  revision: number;
  sessions: Record<CliId, CliSession>;
  activeCli: CliId;
  canvas: CanvasDocument;
};
export type Panel = {
  x: number;
  y: number;
  scale: number;
  depth: number;
  pinned: boolean;
};
export const viewLabels: Record<View, string> = {
  canvas: '画布',
  terminal: '终端',
  preview: '预览',
  research: '审查',
  code: '改动',
  data: '测试',
  model: '3D 空间',
};
export const defaultPanel: Panel = {
  x: 0,
  y: 0,
  scale: 1,
  depth: 2,
  pinned: false,
};
export function initialState(): State {
  return {
    mission: developmentGoal,
    canvas: initialCanvas(),
    view: 'canvas',
    approved: false,
    decisions: [],
    decisionTasks: [],
    decisionsHandled: 0,
    revision: 0,
    activeCli: 'codex',
    sessions: initialSessions(),
    agents: [
      {
        id: 'codex',
        name: 'Codex',
        role: '实现 · CLI',
        color: '#a9dcca',
        progress: 0,
        status: 'ready',
        task: '读取仓库、修改代码、运行验证',
      },
      {
        id: 'claude',
        name: 'Claude',
        role: '审查 · CLI',
        color: '#e7b99b',
        progress: 0,
        status: 'ready',
        task: '独立审查 diff，检查边界与回归',
      },
      {
        id: 'verifier',
        name: '验证',
        role: '测试 · Runner',
        color: '#a9c8ef',
        progress: 0,
        status: 'ready',
        task: '聚焦验证与执行结果',
      },
    ],
    messages: [
      {
        role: 'norma',
        text: 'Codex 和 Claude 的会话已准备好。告诉我先改什么，或者直接在终端里输入。',
      },
    ],
  };
}
export function command(state: State, raw: string): State {
  const text = raw.trim().slice(0, 500);
  if (!text) return state;
  if (text.startsWith('决策:')) return resolveDecision(state, text);
  let next: State = {
    ...state,
    agents: state.agents.map((a) => ({ ...a })),
    messages: [...state.messages, { role: 'user', text }],
    revision: state.revision + 1,
  };
  let reply = '';
  const named = next.agents.find((a) =>
    text.toLowerCase().includes(a.name.toLowerCase()),
  );
  const matches = (a: Agent) => !named || a.id === named.id;
  if (/暂停|停一下|停止|等一下|pause|stop/i.test(text)) {
    next.agents = next.agents.map((a) =>
      matches(a) && a.status === 'working' ? { ...a, status: 'paused' } : a,
    );
    next.decisionTasks = next.decisionTasks.map((task) =>
      (!named || task.agentId === named.id) && task.status === 'working'
        ? { ...task, status: 'paused' }
        : task,
    );
    reply = `${named ? named.name + ' ' : '团队'}已暂停，会话输出与改动保留。可以补充指令后继续。`;
  } else if (/继续|恢复|resume/i.test(text)) {
    next.agents = next.agents.map((a) =>
      matches(a) && a.status === 'paused' ? { ...a, status: 'working' } : a,
    );
    next.decisionTasks = next.decisionTasks.map((task) =>
      (!named || task.agentId === named.id) && task.status === 'paused'
        ? { ...task, status: 'working' }
        : task,
    );
    reply = `${named ? named.name + ' ' : '团队'}继续执行，接着刚才的 CLI 上下文。`;
  } else if (/批准|确认方案|确认改动|approve/i.test(text)) {
    if (next.agents.every((a) => a.status === 'done')) {
      next.approved = true;
      reply =
        '演示改动已确认。保留两份 CLI 会话和审查记录；没有真实提交、合并或发布。';
    } else reply = '还有会话未完成。等实现与审查结束后，你可以确认演示改动。';
  } else if (/画布/.test(text) && /返回|回到|打开|显示|切换/.test(text)) {
    next.view = 'canvas';
    reply = '已回到自由画布。内容、标注与 CLI 会话都保留。';
  } else if (
    /(codex|claude|cli|终端)/i.test(text) &&
    /打开|显示|放到|切换|看看/i.test(text)
  ) {
    next.view = 'terminal';
    next.activeCli = /claude/i.test(text) ? 'claude' : 'codex';
    reply = `${next.activeCli === 'codex' ? 'Codex' : 'Claude'} CLI 已放到主面板。你可以直接输入或中断当前会话。`;
  } else if (/让.*claude.*审查|交给.*claude|审查.*codex/i.test(text)) {
    next = runCli(next, 'claude', '/review');
    reply =
      '已把 Codex 的任务上下文交给 Claude，审查输出会出现在独立 CLI 会话中。';
  } else if (
    /代码|改动|diff|研究|报告|审查|数据|测试|图表|网页|预览|主面板|3d|三维|模型/i.test(
      text,
    ) &&
    /看|展示|打开|放|切换|show|open/i.test(text)
  ) {
    next.view = /代码|改动|diff/i.test(text)
      ? 'code'
      : /研究|报告|审查/.test(text)
        ? 'research'
        : /数据|图表|测试/.test(text)
          ? 'data'
          : /3d|三维|模型/i.test(text)
            ? 'model'
            : 'preview';
    reply = `已将${viewLabels[next.view]}放到主面板。CLI 会话会保留，你随时可以返回。`;
  } else if (/进度|怎么样|汇报|status/i.test(text)) {
    reply =
      next.agents
        .map(
          (a) =>
            `${a.name} ${a.status === 'done' ? '已完成' : a.status === 'paused' ? '已暂停' : `${a.progress}%`}`,
        )
        .join('，') + '。终端中可以查看逐步输出。';
  } else {
    next.mission = text;
    next.approved = false;
    // A multi-agent instruction names both CLIs; a single name scopes the work.
    const both = /codex/i.test(text) && /claude/i.test(text);
    const target = named && !both ? named.id : undefined;
    if (!target)
      next.decisions = next.decisions.filter((decision) => decision.taskId);
    if (!target || target === 'codex') next = startCliTask(next, 'codex', text);
    if (!target || target === 'claude')
      next = startCliTask(
        next,
        'claude',
        `审查实现方案与输入路由边界：${text}`,
      );
    if (!target || target === 'verifier')
      next.agents = next.agents.map((a) =>
        a.id === 'verifier'
          ? {
              ...a,
              status: 'working',
              progress: 0,
              task: '准备 runtime 聚焦测试（模拟）',
            }
          : a,
      );
    next.activeCli = target === 'claude' ? 'claude' : 'codex';
    next.view = 'canvas';
    reply = target
      ? `已发送给 ${named?.name}。命令与工具输出会逐步显示在 CLI 中。`
      : 'Codex 开始实现，Claude 独立审查，验证会话跟进测试。你可以直接进入任一 CLI 接管。';
  }
  next.messages = [
    ...next.messages,
    { role: 'norma' as const, text: reply },
  ].slice(-30);
  return next;
}
export function advance(state: State): State {
  if (
    !state.agents.some((a) => a.status === 'working') &&
    !state.decisionTasks.some((task) => task.status === 'working')
  )
    return state;
  const decisionTasks = state.decisionTasks.map((task) => {
    if (task.status !== 'working') return task;
    const progress = Math.min(100, task.progress + 5);
    return {
      ...task,
      progress,
      status: progress === 100 ? ('done' as const) : task.status,
    };
  });
  const agents = state.agents.map((a, i) => {
    if (a.status !== 'working') return a;
    const progress = Math.min(100, a.progress + [5, 4, 4][i]);
    return {
      ...a,
      progress,
      status: progress === 100 ? ('done' as const) : a.status,
    };
  });
  const completed =
    agents.every((a) => a.status === 'done') &&
    !state.agents.every((a) => a.status === 'done');
  return {
    ...state,
    agents,
    decisionTasks,
    revision: state.revision + 1,
    messages: completed
      ? [
          ...state.messages,
          {
            role: 'norma',
            text: 'Codex 的实现、Claude 的审查和验证输出都已准备好。请查看演示 diff，再决定是否确认。',
          },
        ]
      : state.messages,
  };
}
export function adjustPanel(panel: Panel, change: Partial<Panel>): Panel {
  const clamp = (n: number, min: number, max: number) =>
    Math.min(max, Math.max(min, n));
  return {
    ...panel,
    ...change,
    x: panel.pinned ? panel.x : clamp(change.x ?? panel.x, -360, 360),
    y: panel.pinned ? panel.y : clamp(change.y ?? panel.y, -160, 160),
    scale: clamp(change.scale ?? panel.scale, 0.7, 1.35),
    depth: clamp(change.depth ?? panel.depth, 1.2, 3.5),
  };
}
