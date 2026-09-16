'use client';
// 三种自由画布原型在同一路由上通过 ?variant=A|B|C 切换，用于比较空间与工作流的优先级。
import {
  ArrowLeft,
  ArrowRight,
  TerminalSquare,
  GitPullRequest,
  Check,
  Play,
  PenLine,
  Layers,
  FileDiff,
} from 'lucide-react';
import type { ReactNode } from 'react';
import type { State } from '@/lib/model';
import { developmentGoal } from '@/lib/cli';
export type Variant = 'A' | 'B' | 'C';
export const variantLabels = {
  A: '空间工作台',
  B: '双终端驾驶舱',
  C: '任务指挥台',
};
export function PrototypeSwitcher({
  variant,
  change,
}: {
  variant: Variant;
  change: (v: Variant) => void;
}) {
  if (process.env.NODE_ENV === 'production') return null;
  const variants: Variant[] = ['A', 'B', 'C'];
  const index = variants.indexOf(variant);
  return (
    <nav className="prototype-switcher" aria-label="原型方案切换">
      <span>设计探索</span>
      <button
        aria-label="上一个原型"
        onClick={() => change(variants[(index + 2) % 3])}
      >
        <ArrowLeft size={17} />
      </button>
      <div>
        <b>{variant}</b>
        <span>{variantLabels[variant]}</span>
        <small>{index + 1} / 3</small>
      </div>
      <button
        aria-label="下一个原型"
        onClick={() => change(variants[(index + 1) % 3])}
      >
        <ArrowRight size={17} />
      </button>
    </nav>
  );
}
export function VariantA({ children }: { children: ReactNode }) {
  return <section className="workspace">{children}</section>;
}
export function VariantB({
  canvas,
  state,
  send,
}: {
  canvas: ReactNode;
  state: State;
  send: (text: string) => void;
}) {
  return (
    <section className="variant-b">
      <div className="duet-context">
        <div>
          <TerminalSquare size={20} />
          <h2>两位伙伴，同一个开发现场。</h2>
        </div>
        <span>
          Codex 实现 <i /> Claude 审查
        </span>
        <button onClick={() => send(developmentGoal)}>
          <Play size={14} />
          启动协作
        </button>
      </div>
      <div className="duet-canvas glass">{canvas}</div>
      <div className="duet-bottom">
        <span>
          <PenLine size={15} />
          直接圈出问题，让团队接着做。
        </span>
        <span>
          {state.agents.filter((a) => a.status === 'working').length} 个执行中 ·{' '}
          {state.canvas.strokes.length} 处标注
        </span>
        <button onClick={() => send('让 Claude 审查 Codex 的改动')}>
          <GitPullRequest size={14} />
          把改动交给 Claude
        </button>
      </div>
    </section>
  );
}
export function VariantC({
  canvas,
  state,
  send,
}: {
  canvas: ReactNode;
  state: State;
  send: (text: string) => void;
}) {
  return (
    <section className="variant-c">
      <aside className="task-rail">
        <div className="rail-kicker">NORMA / MISSION 01</div>
        <h2>从目标，到可审阅的改动。</h2>
        <p>{state.mission}</p>
        <button className="rail-start" onClick={() => send(developmentGoal)}>
          <Play size={15} />
          执行这个任务
        </button>
        <div className="rail-timeline">
          {[
            ['理解目标', '仓库上下文 · AGENTS.md'],
            ['Codex 实现', '工具输出 · 文件改动'],
            ['Claude 审查', '边界 · 回归 · 建议'],
            ['你来确认', '演示 diff · 验证结果'],
          ].map(([title, detail], i) => (
            <button
              key={title}
              onClick={() =>
                send(
                  i === 1
                    ? '打开 Codex CLI'
                    : i === 2
                      ? '打开 Claude CLI'
                      : '汇报进度',
                )
              }
            >
              <span>{i === 0 ? <Check size={12} /> : i + 1}</span>
              <div>
                <strong>{title}</strong>
                <small>{detail}</small>
              </div>
            </button>
          ))}
        </div>
        <div className="rail-note">
          <FileDiff size={17} />
          <span>
            把窗口、笔迹和想法
            <br />
            留在同一张画布上。
          </span>
        </div>
      </aside>
      <div className="mission-canvas glass">
        <div className="mission-canvas-heading">
          <Layers size={16} />
          <strong>这次任务的工作画布</strong>
          <span>可自由摆放</span>
        </div>
        {canvas}
      </div>
    </section>
  );
}
