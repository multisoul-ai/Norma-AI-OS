import type { State } from './model.ts';

export type PendingDecision = {
  id: string;
  kind: 'verification' | 'design' | 'preview' | 'audience';
  agentId: string;
  taskId?: string;
};
export type DecisionTask = {
  id: string;
  title: string;
  agentId: string;
  status: 'waiting' | 'working' | 'paused' | 'done';
  progress: number;
  nextStep: string;
};
export type DecisionItem = {
  id: string;
  task: string;
  summary: string;
  owner: string;
  title: string;
  reason: string;
  recommendation: string;
  impact: string;
  detail: string;
  primary: { label: string; action: string };
  secondary: { label: string; action: string };
};
const options = {
  verification: [
    ['focused', '运行聚焦测试', 'runtime 聚焦测试'],
    ['full', '扩大验证范围', '扩展回归测试'],
  ],
  design: [
    ['calm', '采用安静留白', '完善留白与内容层级'],
    ['dense', '采用紧凑布局', '完善紧凑工作台布局'],
  ],
  preview: [
    ['private', '仅自己预览', '准备个人预览'],
    ['team', '生成团队预览', '准备团队演示预览'],
  ],
  audience: [
    ['beginner', '面向首次使用者', '编写入门说明'],
    ['developer', '面向开发者', '编写开发接入说明'],
  ],
} as const;

export function initialDecisionTasks(): DecisionTask[] {
  return [
    { id: 'homepage', title: '空间首页', agentId: 'claude' },
    { id: 'preview', title: '演示站点', agentId: 'codex' },
    { id: 'docs', title: '上手文档', agentId: 'claude' },
    { id: 'compatibility', title: '兼容性回归', agentId: 'verifier' },
  ].map((task) => ({
    ...task,
    status: 'waiting',
    progress: 0,
    nextStep: '等待你的决定',
  }));
}
export function initialDecisions(): PendingDecision[] {
  return [
    { id: 'verification', kind: 'verification', agentId: 'verifier' },
    { id: 'homepage', kind: 'design', agentId: 'claude', taskId: 'homepage' },
    { id: 'preview', kind: 'preview', agentId: 'codex', taskId: 'preview' },
    { id: 'docs', kind: 'audience', agentId: 'claude', taskId: 'docs' },
    {
      id: 'compatibility',
      kind: 'verification',
      agentId: 'verifier',
      taskId: 'compatibility',
    },
  ];
}
export function decisionSnapshot(state: State) {
  const items: DecisionItem[] = state.decisions.map((decision) => {
    const task = state.decisionTasks.find(
      (task) => task.id === decision.taskId,
    );
    const owner =
      state.agents.find((agent) => agent.id === decision.agentId)?.name ||
      decision.agentId;
    const detail = {
      verification: {
        summary: '确认测试范围',
        title: '这次测试，跑到什么范围？',
        reason: '验证任务等待范围确认。其他任务按各自的进度执行。',
        recommendation: '先运行聚焦测试，验证当前改动；需要时再扩大回归范围。',
        view: 'data',
      },
      design: {
        summary: '选择视觉方向',
        title: '首页采用哪一种信息密度？',
        reason: '首页草案已准备好。Claude 等你确定方向，再细化布局。',
        recommendation: '采用安静留白，优先呈现当前任务，次要操作按需展开。',
        view: 'preview',
      },
      preview: {
        summary: '决定分享范围',
        title: '这个预览，准备给谁看？',
        reason: '站点内容已准备好。Codex 等待你选择这次演示的受众。',
        recommendation: '先生成个人预览，确认内容后再准备团队版本。',
        view: 'preview',
      },
      audience: {
        summary: '明确目标读者',
        title: '这份说明，主要写给谁？',
        reason: '同一份上手文档需要不同的深度。Claude 等待你确定读者。',
        recommendation: '先帮助首次使用者完成一个任务，再补充开发者接入文档。',
        view: 'research',
      },
    }[decision.kind];
    const [primary, secondary] = options[decision.kind];
    const action = (option: readonly [string, string, string]) => ({
      label: option[1],
      action: `command:决策:${decision.id}:${option[0]}`,
    });
    return {
      id: decision.id,
      task: task?.title || '终端焦点修复',
      owner,
      ...detail,
      detail: `view:${detail.view}`,
      impact: `选择后仅推进「${task?.title || '终端焦点修复'}」。所有执行均为模拟。`,
      primary: action(primary),
      secondary: action(secondary),
    };
  });
  if (
    !state.approved &&
    state.agents.every((agent) => agent.status === 'done')
  ) {
    items.push({
      id: 'review',
      task: '终端焦点修复',
      summary: '确认交付成果',
      owner: 'Codex + Claude',
      title: '这次改动，可以交付了吗？',
      reason: '实现、独立审查与模拟测试已完成。任务等待你确认成果。',
      recommendation: '查看 diff 与审查记录，再确认本次模拟交付。',
      impact: '仅确认当前任务的演示成果，其他待决策任务保持原样。',
      detail: 'view:code',
      primary: { label: '确认这次改动', action: 'command:确认改动' },
      secondary: {
        label: '退回补充检查',
        action: 'command:Codex 根据审查记录补充输入路由边界检查',
      },
    });
  }
  return { items, handled: state.decisionsHandled + Number(state.approved) };
}
export function decisionWindow(state: State, requestedPage: number) {
  const { items, handled } = decisionSnapshot(state);
  const pages = Math.max(1, Math.ceil(items.length / 4));
  const page = Math.min(pages - 1, Math.max(0, Math.floor(requestedPage)));
  return {
    items: items.slice(page * 4, page * 4 + 4),
    total: items.length,
    page,
    pages,
    handled,
  };
}
export function resolveDecision(state: State, raw: string): State {
  const [, id, choice] = raw.split(':');
  const decision = state.decisions.find((item) => item.id === id);
  if (!decision) return state;
  const selected = options[decision.kind].find(
    (option) => option[0] === choice,
  );
  if (!selected) return state;
  const decisions = state.decisions.filter((item) => item.id !== id);
  const waiting = decisions.some((item) =>
    decision.taskId
      ? item.taskId === decision.taskId
      : !item.taskId && item.agentId === decision.agentId,
  );
  const nextStatus = waiting ? ('waiting' as const) : ('working' as const);
  const taskTitle =
    state.decisionTasks.find((task) => task.id === decision.taskId)?.title ||
    '终端焦点修复';
  return {
    ...state,
    decisions,
    decisionsHandled: state.decisionsHandled + 1,
    revision: state.revision + 1,
    agents: decision.taskId
      ? state.agents
      : state.agents.map((agent) =>
          agent.id === decision.agentId
            ? {
                ...agent,
                status: nextStatus,
                task: `运行${selected[2]}（模拟）`,
              }
            : agent,
        ),
    decisionTasks: state.decisionTasks.map((task) =>
      task.id === decision.taskId
        ? { ...task, status: nextStatus, nextStep: selected[2] }
        : task,
    ),
    messages: [
      ...state.messages,
      { role: 'user', text: `${taskTitle}：${selected[1]}` },
      {
        role: 'norma',
        text: `「${taskTitle}」已确认：${selected[1]}。${waiting ? '该任务还有待处理事项。' : '对应任务继续推进。'}其他待办保留。`,
      },
    ].slice(-30) as State['messages'],
  };
}
