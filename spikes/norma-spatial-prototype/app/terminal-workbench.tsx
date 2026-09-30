'use client';
import { useEffect, useRef, useState } from 'react';
import {
  ArrowUp,
  GitBranch,
  TerminalSquare,
  Square,
  Play,
  GitPullRequest,
  Columns2,
  FileDiff,
  FlaskConical,
  Trash2,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { visibleTranscript, type CliId } from '@/lib/cli';
import type { State } from '@/lib/model';
export function TerminalSession({
  state,
  id,
  run,
  compact = false,
}: {
  state: State;
  id: CliId;
  run: (id: CliId, text: string) => void;
  compact?: boolean;
}) {
  const [input, setInput] = useState('');
  const log = useRef<HTMLDivElement>(null);
  const follow = useRef(true);
  const agent = state.agents.find((a) => a.id === id)!;
  const lines = visibleTranscript(state, id);
  const title = id === 'codex' ? 'Codex' : 'Claude';
  useEffect(() => {
    if (log.current && follow.current)
      log.current.scrollTop = log.current.scrollHeight;
  }, [lines.length]);
  return (
    <section
      className={`cli-session cli-${id} ${compact ? 'cli-compact' : ''}`}
      aria-label={`${title} CLI 会话`}
    >
      <div className="cli-session-heading">
        <span>
          <span className="cli-status-light" />
          {title}
          <small>CLI</small>
        </span>
        <span className="cli-session-state">
          {agent.status === 'working'
            ? '执行中'
            : agent.status === 'paused'
              ? '已中断'
              : agent.status === 'done'
                ? '等待指令'
                : '就绪'}
          <b>{agent.progress}%</b>
        </span>
      </div>
      <div
        ref={log}
        className="cli-log"
        role="log"
        aria-label={`${title} 终端输出`}
        onScroll={(e) => {
          const el = e.currentTarget;
          follow.current =
            el.scrollHeight - el.scrollTop - el.clientHeight < 45;
        }}
      >
        {lines.map((line, i) => (
          <div
            key={`${state.sessions[id].run}-${i}`}
            className={`cli-line line-${line.kind}`}
          >
            {line.text || ' '}
          </div>
        ))}
        {agent.status === 'working' && (
          <div className="cli-working">
            <span>●</span> 正在处理… <small>Ctrl+C 中断</small>
          </div>
        )}
      </div>
      <form
        className="cli-input"
        onSubmit={(e) => {
          e.preventDefault();
          if (input.trim()) {
            run(id, input);
            setInput('');
            follow.current = true;
          }
        }}
      >
        <span>›</span>
        <input
          aria-label={`给 ${title} 的 CLI 指令`}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.ctrlKey && e.key.toLowerCase() === 'c') {
              e.preventDefault();
              run(id, '/stop');
            }
            if (e.key === 'Escape') e.stopPropagation();
          }}
          placeholder={
            agent.status === 'paused'
              ? '补充方向，或 /resume 继续'
              : `直接输入给 ${title} 的任务…`
          }
          maxLength={500}
        />
        <Button
          type="submit"
          size="icon"
          aria-label={`发送到 ${title}`}
          disabled={!input.trim()}
        >
          <ArrowUp size={15} />
        </Button>
      </form>
      <div className="cli-session-actions">
        <button
          onClick={() =>
            run(id, agent.status === 'paused' ? '/resume' : '/stop')
          }
          disabled={agent.status !== 'working' && agent.status !== 'paused'}
        >
          {agent.status === 'paused' ? (
            <Play size={12} />
          ) : (
            <Square size={12} />
          )}{' '}
          {agent.status === 'paused' ? '继续' : '中断'}
        </button>
        <button onClick={() => run(id, '/diff')}>
          <FileDiff size={12} />
          diff
        </button>
        <button onClick={() => run(id, '/test')}>
          <FlaskConical size={12} />
          测试
        </button>
        <button
          aria-label={`清空 ${title} 显示`}
          onClick={() => run(id, '/clear')}
        >
          <Trash2 size={12} />
        </button>
        <span>模拟会话</span>
      </div>
    </section>
  );
}
export function TerminalWorkbench({
  state,
  run,
  select,
}: {
  state: State;
  run: (id: CliId, text: string) => void;
  select: (id: CliId) => void;
}) {
  const [split, setSplit] = useState(false);
  return (
    <div className="terminal-workbench">
      <div className="cli-toolbar">
        <div className="cli-tabs" role="group" aria-label="CLI 会话">
          <button
            aria-pressed={state.activeCli === 'codex'}
            onClick={() => select('codex')}
          >
            <TerminalSquare size={14} />
            Codex
          </button>
          <button
            aria-pressed={state.activeCli === 'claude'}
            onClick={() => select('claude')}
          >
            <TerminalSquare size={14} />
            Claude
          </button>
        </div>
        <div>
          <button
            onClick={() => run('claude', '/review')}
            title="让 Claude 审查"
          >
            <GitPullRequest size={14} />
            审查
          </button>
          <button
            aria-label="并排显示 CLI"
            aria-pressed={split}
            onClick={() => setSplit(!split)}
          >
            <Columns2 size={15} />
          </button>
        </div>
      </div>
      <div className="cli-repo">
        <GitBranch size={12} />
        <span>Norma-AI-OS</span>
        <small>codex/focus-routing · 模拟</small>
      </div>
      <div className={split ? 'cli-split' : ''}>
        {split ? (
          <>
            {(['codex', 'claude'] as CliId[]).map((id) => (
              <TerminalSession
                key={id}
                state={state}
                id={id}
                run={run}
                compact
              />
            ))}
          </>
        ) : (
          <TerminalSession state={state} id={state.activeCli} run={run} />
        )}
      </div>
    </div>
  );
}
