'use client';
import {
  ArrowUpRight,
  Check,
  FileText,
  Globe2,
  ChevronRight,
  Code2,
  Compass,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import type { State } from '@/lib/model';
import { activitySnapshot } from '@/lib/spatial-activity';
import { CanvasDiff } from './free-canvas';
export function Preview({
  send,
  state,
}: {
  send: (text: string) => void;
  state: State;
}) {
  return (
    <>
      <div className="browser-bar">
        <span>‹ &nbsp; ›</span>
        <span>
          <Globe2 size={12} />
          norma.local / preview
        </span>
        <span className="mock-browser-label">运行结果演示</span>
      </div>
      <div className="canvas-preview standalone-preview">
        <span>NORMA OS / LIVE NODE</span>
        <h3>想法在这里，开始运行。</h3>
        <p>同一张画布，可以放下终端、网页、设计稿和你画出的下一步。</p>
        <div className="preview-node-demo">
          <Code2 size={20} />
          <div>
            <strong>Terminal Node</strong>
            <small>焦点已交给终端 · Escape 返回画布</small>
          </div>
          <span>Live</span>
        </div>
        <button onClick={() => send('打开 Codex CLI')}>
          回到 Codex 继续实现 <ChevronRight size={14} />
        </button>
      </div>
    </>
  );
}
export function Research({ state }: { state: State }) {
  return (
    <div className="research-content">
      <div className="document-tag">
        <FileText size={14} /> CLAUDE · REVIEW <span>模拟审查</span>
      </div>
      <h2>先把输入交给正确的会话。</h2>
      <p className="research-brief">{state.mission}</p>
      <div className="research-points">
        {[
          [
            '01',
            '焦点归属',
            '聚焦终端后，按键只发送到当前节点，其他会话继续执行。',
          ],
          [
            '02',
            '可随时接管',
            'Escape 退出终端焦点，不关闭会话，也不清空已经产生的输出。',
          ],
          [
            '03',
            '最小改动与验证',
            '先审阅 runtime::route 的演示 diff，再验证暂停、恢复与关闭的行为。',
          ],
        ].map(([n, t, d]) => (
          <section key={n}>
            <span>{n}</span>
            <div>
              <h3>{t}</h3>
              <p>{d}</p>
            </div>
          </section>
        ))}
      </div>
      <footer>
        <Check size={14} />
        审查上下文已整理 <span>演示报告</span>
      </footer>
    </div>
  );
}
export function Code({ state }: { state: State }) {
  return (
    <div className="standalone-diff">
      <CanvasDiff />
      <footer>
        <Check size={13} />
        {state.agents[0].status === 'done'
          ? 'Codex 演示改动已就绪'
          : '可审阅的示例改动'}
        <span>未写入真实仓库</span>
      </footer>
    </div>
  );
}
export function Data({ state }: { state: State }) {
  return (
    <div className="data-content">
      <header>
        <span>团队执行概览</span>
        <span className="document-tag">MOCK DATA</span>
      </header>
      <div className="metric-row">
        <div>
          <strong>
            {activitySnapshot(state).progress}
            <small>%</small>
          </strong>
          <span>总体进度</span>
        </div>
        <div>
          <strong>
            {state.agents.filter((a) => a.status === 'done').length}
            <small>/ 3</small>
          </strong>
          <span>已完成产物</span>
        </div>
        <div>
          <strong>3</strong>
          <span>协作伙伴</span>
        </div>
      </div>
      <div className="agent-chart">
        {state.agents.map((a) => (
          <div key={a.id}>
            <span>
              {a.name}
              <small>{a.role}</small>
            </span>
            <div>
              <i style={{ width: `${a.progress}%`, background: a.color }} />
            </div>
            <strong>{a.progress}%</strong>
          </div>
        ))}
      </div>
      <p className="chart-note">
        任务进度随模拟执行实时更新。你可以点名暂停，保留其他人的工作。
      </p>
    </div>
  );
}
export function ModelPlaceholder({ open }: { open: () => void }) {
  return (
    <div className="model-content">
      <div className="model-orbit" aria-hidden="true">
        <Compass size={76} strokeWidth={0.6} />
      </div>
      <span className="document-tag">SPATIAL OBJECT · 01</span>
      <h2>把想法，放进空间。</h2>
      <p>打开可旋转的三维构型，用鼠标或捏合操作它。</p>
      <Button onClick={open} className="model-open">
        进入 3D 空间 <ArrowUpRight size={16} />
      </Button>
    </div>
  );
}
