import { initialState, command, advance, type State } from './model.ts';
import {
  decisionSnapshot,
  initialDecisions,
  initialDecisionTasks,
} from './decisions.ts';
import { visibleTranscript, developmentGoal } from './cli.ts';
export function initialVRState(): State {
  let state = command(initialState(), developmentGoal);
  for (let i = 0; i < 3; i++) state = advance(state);
  return {
    ...state,
    view: 'terminal',
    decisions: initialDecisions(),
    decisionTasks: initialDecisionTasks(),
    agents: state.agents.map((agent) =>
      agent.id === 'verifier'
        ? { ...agent, status: 'waiting', task: '等待选择验证范围' }
        : agent,
    ),
  };
}
function describeStep(text: string) {
  const steps: [RegExp, string][] = [
    [/Read /, '读取仓库与路由定义'],
    [/Search /, '定位焦点切换与按键转发'],
    [/Write /, '补充回归测试'],
    [/Run cargo/, '运行 runtime 测试'],
    [/Inspect /, '检查改动与执行结果'],
    [/Review /, '审查边界与回归'],
    [/Add /, '补充测试负断言'],
    [/PASS/, '聚焦验证通过'],
    [/完成/, '本阶段已完成，等待审阅'],
  ];
  return steps.find(([pattern]) => pattern.test(text))?.[1] || text;
}
export function activitySnapshot(state: State) {
  const participants = state.agents.filter((agent) => agent.status !== 'ready');
  const running = state.agents.filter((a) => a.status === 'working').length;
  const paused = state.agents.filter((a) => a.status === 'paused').length;
  const waiting = state.agents.filter((a) => a.status === 'waiting').length;
  return {
    mission: state.mission,
    running,
    paused,
    waiting,
    progress: Math.round(
      participants.reduce((sum, a) => sum + a.progress, 0) /
        Math.max(1, participants.length),
    ),
    rows: state.agents.map((a) => {
      const log =
        a.id === 'codex' || a.id === 'claude'
          ? visibleTranscript(state, a.id)
          : [];
      const detail =
        log.findLast(
          (line) =>
            ['tool', 'success', 'text'].includes(line.kind) && line.text.trim(),
        )?.text || (a.progress < 50 ? '准备聚焦测试' : '检查测试输出与回归');
      return {
        id: a.id,
        name: a.name,
        status:
          a.status === 'working'
            ? '执行中'
            : a.status === 'paused'
              ? '已暂停'
              : a.status === 'waiting'
                ? '等你决策'
                : a.status === 'done'
                  ? '已完成'
                  : '准备就绪',
        detail:
          a.status === 'done'
            ? state.approved
              ? '成果已确认'
              : '本阶段已完成，等待审阅'
            : a.status === 'ready' ||
                a.status === 'waiting' ||
                a.id === 'verifier'
              ? a.task
              : describeStep(detail),
        progress: a.progress,
        color: a.color,
        action:
          a.id === 'verifier' ? 'view:data' : `command:打开 ${a.name} CLI`,
      };
    }),
  };
}

export function describeSpatialScene(state: State) {
  const snapshot = activitySnapshot(state);
  const label =
    state.view === 'terminal'
      ? `${state.activeCli === 'codex' ? 'Codex' : 'Claude'} CLI`
      : (
          {
            canvas: '自由画布',
            code: '代码改动',
            research: '审查报告',
            data: '实时图表',
            preview: '网页与图片',
            model: '三维对象',
          } as Record<string, string>
        )[state.view];
  return (
    `Norma 三维空间。主面板：${label}。任务进度 ${snapshot.progress}%。${snapshot.running} 位正在执行。` +
    snapshot.rows
      .map((row) => `${row.name} ${row.status} ${row.progress}%`)
      .join('；') +
    `。待决策 ${decisionSnapshot(state).items.length} 项。` +
    decisionSnapshot(state)
      .items.map((item) => item.title)
      .join('；')
  );
}
