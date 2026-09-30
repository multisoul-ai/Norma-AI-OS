'use client';
import { useSearchParams } from 'next/navigation';
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import {
  ArrowUp,
  AudioLines,
  Compass,
  Globe2,
  Code2,
  ChartNoAxesCombined,
  Box,
  Scan,
  Sparkles,
  Users,
  Grip,
  Plus,
  Minus,
  CircleHelp,
  Maximize2,
  Pin,
  PinOff,
  Radio,
  Layers,
  Mountain,
  ChevronRight,
  Check,
  Search,
  Pause,
  Play,
  RotateCcw,
  MessageCircle,
  Volume2,
  VolumeX,
  X,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog';
import { Slider } from '@/components/ui/slider';
import {
  initialState,
  command,
  advance,
  defaultPanel,
  adjustPanel,
  viewLabels,
  type View,
  type Panel,
  type Agent,
} from '@/lib/model';
import { registerSpatialTools } from '@/lib/webmcp';
import { useVoice } from '@/hooks/use-voice';
import {
  Preview,
  Research,
  Code,
  Data,
  ModelPlaceholder,
} from './artifact-content';
import { SpatialScene } from './spatial-scene';
import { initialSpatialMode } from '@/lib/interaction-chrome';
import { initialVRState } from '@/lib/spatial-activity';
import { QuietWorkspace } from './quiet-workspace';
import { FreeCanvas } from './free-canvas';
import {
  PrototypeSwitcher,
  VariantA,
  VariantB,
  VariantC,
  type Variant,
} from './prototype-variants';
import type { CanvasDocument } from '@/lib/free-canvas';
import { TerminalWorkbench } from './terminal-workbench';
import { runCli, developmentGoal, type CliId } from '@/lib/cli';
const statusLabels = {
  waiting: '等你决策',
  ready: '就绪',
  working: '执行中',
  paused: '已暂停',
  done: '已完成',
};
const icons = {
  canvas: Layers,
  terminal: Code2,
  preview: Globe2,
  research: Search,
  code: Code2,
  data: ChartNoAxesCombined,
  model: Box,
};
type Mode = 'workspace' | 'focus' | 'orbit';
export default function Spatial() {
  const searchParams = useSearchParams();
  const [state, setState] = useState(() =>
    initialSpatialMode(
      searchParams.get('mode'),
      searchParams.get('compare') === '1',
    ) === 'orbit'
      ? initialVRState()
      : initialState(),
  );
  const stateRef = useRef(state);
  stateRef.current = state;
  const [input, setInput] = useState('');
  const [notice, setNotice] = useState('');
  const noticeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const notify = useCallback((text: string) => {
    setNotice(text);
    if (noticeTimer.current) clearTimeout(noticeTimer.current);
    noticeTimer.current = setTimeout(() => setNotice(''), 7500);
  }, []);
  const send = useCallback((text: string) => {
    const next = command(stateRef.current, text);
    stateRef.current = next;
    setState(next);
    setInput('');
    return next;
  }, []);
  const cliRun = useCallback((id: CliId, text: string) => {
    setState((previous) => {
      const next = runCli(previous, id, text);
      if (!['/diff', '/test'].includes(text)) next.view = previous.view;
      stateRef.current = next;
      return next;
    });
  }, []);
  const selectCli = useCallback((id: CliId) => {
    setState((s) => ({
      ...s,
      activeCli: id,
      view: 'terminal',
      revision: s.revision + 1,
    }));
  }, []);
  const voice = useVoice(send, notify);
  const voiceRef = useRef(voice);
  voiceRef.current = voice;
  const [variant, setVariant] = useState<Variant>('A');
  const changeCanvas = useCallback(
    (canvas: CanvasDocument) =>
      setState((previous) => ({
        ...previous,
        canvas,
        revision: previous.revision + 1,
      })),
    [],
  );
  const [mode, setMode] = useState<Mode>(() =>
    initialSpatialMode(
      searchParams.get('mode'),
      searchParams.get('compare') === '1',
    ),
  );
  const [panel, setPanel] = useState<Panel>({ ...defaultPanel });
  const [immersion, setImmersion] = useState(28);
  const [dialog, setDialog] = useState<'guide' | 'history' | 'agent' | null>(
    null,
  );
  const [selectedAgent, setSelectedAgent] = useState('codex');
  const [xrSupported, setXrSupported] = useState(false);
  const [xrSession, setXrSession] = useState<XRSession | null>(null);
  const [xrPending, setXrPending] = useState(false);
  const drag = useRef<{
    id: number;
    x: number;
    y: number;
    start: Panel;
  } | null>(null);
  const active =
    state.agents.some((a) => a.status === 'working') ||
    state.decisionTasks.some((task) => task.status === 'working');
  const allDone = state.agents.every((a) => a.status === 'done');
  const paused = state.agents.some((a) => a.status === 'paused');
  const changeView = useCallback((view: View) => {
    setState((s) => {
      const next = { ...s, view, revision: s.revision + 1 };
      stateRef.current = next;
      return next;
    });
  }, []);
  const changeMode = useCallback((next: Mode) => {
    setMode(next);
    const url = new URL(window.location.href);
    url.searchParams.set('mode', next);
    // Vinext intercepts native history and updates useSearchParams without remounting the CLI workspace.
    window.history.replaceState(null, '', url);
  }, []);
  useEffect(() => {
    const params = new URL(window.location.href).searchParams;
    const initialVariant = params.get('variant');
    if (
      initialVariant === 'A' ||
      initialVariant === 'B' ||
      initialVariant === 'C'
    )
      setVariant(initialVariant);
    const initialMode = params.get('mode');
    setMode(initialSpatialMode(initialMode, params.get('compare') === '1'));
    navigator.xr
      ?.isSessionSupported('immersive-vr')
      .then(setXrSupported)
      .catch(() => setXrSupported(false));
    return () => {
      if (noticeTimer.current) clearTimeout(noticeTimer.current);
    };
  }, []);
  useEffect(() => {
    const timer = setInterval(
      () =>
        setState((previous) => {
          const next = advance(previous);
          stateRef.current = next;
          return next;
        }),
      950,
    );
    return () => clearInterval(timer);
  }, []);
  useEffect(() => registerSpatialTools(() => stateRef.current, send), [send]);
  const lastReply = state.messages.at(-1)?.text;
  useEffect(() => {
    if (lastReply) voiceRef.current.speak(lastReply);
  }, [lastReply]);
  useEffect(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      voiceRef.current.stop();
      if (dialog) setDialog(null);
      else if (mode !== 'workspace') changeMode('workspace');
    };
    window.addEventListener('keydown', escape);
    return () => window.removeEventListener('keydown', escape);
  }, [dialog, mode, changeMode]);
  useEffect(() => {
    const key = searchParams.get('variant');
    if (key === 'A' || key === 'B' || key === 'C') setVariant(key);
  }, [searchParams]);
  const changeVariant = useCallback((next: Variant) => {
    setVariant(next);
    setMode('workspace');
    setPanel({ ...defaultPanel });
    const url = new URL(window.location.href);
    url.searchParams.set('variant', next);
    url.searchParams.set('mode', 'workspace');
    // Vinext intercepts native history and updates useSearchParams without remounting the CLI workspace.
    window.history.replaceState(null, '', url);
  }, []);
  useEffect(() => {
    if (
      process.env.NODE_ENV === 'production' ||
      searchParams.get('compare') !== '1'
    )
      return;
    const cycle = (e: KeyboardEvent) => {
      if (
        e.defaultPrevented ||
        e.ctrlKey ||
        e.metaKey ||
        e.altKey ||
        (e.target as HTMLElement).closest(
          'input,textarea,[contenteditable=true],[role=slider],[role=tablist],[role=dialog]',
        )
      )
        return;
      if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
        e.preventDefault();
        const choices: Variant[] = ['A', 'B', 'C'];
        changeVariant(
          choices[
            (choices.indexOf(variant) + (e.key === 'ArrowRight' ? 1 : 2)) % 3
          ],
        );
      }
    };
    window.addEventListener('keydown', cycle);
    return () => window.removeEventListener('keydown', cycle);
  }, [variant, changeVariant]);
  function startXR() {
    if (!navigator.xr || !xrSupported) {
      setDialog('guide');
      notify(
        '当前浏览器未检测到 WebXR 头显。这里仍可预演空间选择、抓取与标注。',
      );
      return;
    }
    setXrPending(true);
    navigator.xr
      .requestSession('immersive-vr', { optionalFeatures: ['local-floor'] })
      .then((session) => {
        setXrSession(session);
        setXrPending(false);
        session.addEventListener(
          'end',
          () => {
            setXrSession(null);
            voiceRef.current.stop();
            changeMode('orbit');
          },
          { once: true },
        );
      })
      .catch((error) => {
        setXrPending(false);
        notify(
          error?.name === 'NotAllowedError'
            ? '未进入 VR，你可以随时重试。'
            : '无法启动 VR 会话，请检查头显连接与浏览器支持。',
        );
      });
  }
  const updatePanel = (change: Partial<Panel>) =>
    setPanel((previous) => adjustPanel(previous, change));
  function dragStart(event: ReactPointerEvent<HTMLDivElement>) {
    if (panel.pinned || drag.current) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = {
      id: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      start: panel,
    };
  }
  function dragMove(event: ReactPointerEvent<HTMLDivElement>) {
    const g = drag.current;
    if (!g || g.id !== event.pointerId) return;
    setPanel(
      adjustPanel(g.start, {
        x: g.start.x + event.clientX - g.x,
        y: g.start.y + event.clientY - g.y,
      }),
    );
  }
  function dragEnd(event: ReactPointerEvent<HTMLDivElement>) {
    if (drag.current?.id !== event.pointerId) return;
    drag.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
  }
  function agentClick(agent: Agent) {
    setSelectedAgent(agent.id);
    setDialog('agent');
  }
  const selected =
    state.agents.find((a) => a.id === selectedAgent) ?? state.agents[0];
  const canvas = (
    <div className="canvas-surface">
      <FreeCanvas
        state={state}
        doc={state.canvas}
        change={changeCanvas}
        run={cliRun}
        send={send}
        layout={variant}
      />
      {variant !== 'A' &&
        ['data', 'research', 'preview', 'model'].includes(state.view) && (
          <div className="canvas-result-overlay">
            <header>
              <strong>{viewLabels[state.view]}</strong>
              <button
                aria-label="关闭内容预览"
                onClick={() => changeView('canvas')}
              >
                <X size={17} />
              </button>
            </header>
            <div>
              {state.view === 'data' ? (
                <Data state={state} />
              ) : state.view === 'research' ? (
                <Research state={state} />
              ) : state.view === 'preview' ? (
                <Preview send={send} state={state} />
              ) : (
                <ModelPlaceholder open={() => changeMode('orbit')} />
              )}
            </div>
          </div>
        )}
    </div>
  );
  if (searchParams.get('compare') !== '1')
    return (
      <QuietWorkspace
        state={state}
        send={send}
        run={cliRun}
        change={changeCanvas}
        view={changeView}
        voice={voice}
        space={() => changeMode('orbit')}
        enterVR={startXR}
        notice={notice}
        dismiss={() => setNotice('')}
        scene={
          mode === 'orbit' || xrSession ? (
            <SpatialScene
              enterVR={startXR}
              pending={xrPending}
              session={xrSession}
              bridge={{
                read: () => stateRef.current,
                send,
                view: changeView,
                canvas: changeCanvas,
                voice: voice.start,
                listening: () => voiceRef.current.listening,
                voiceState: () => voiceRef.current,
                toggleVoiceOutput: () =>
                  voiceRef.current.setReadAloud(!voiceRef.current.readAloud),
                notice: notify,
                leave: () => {
                  setXrSession(null);
                  changeMode(xrSession ? 'orbit' : 'workspace');
                },
              }}
            />
          ) : null
        }
      />
    );
  return (
    <main
      className={`spatial-app mode-${mode} variant-${variant}`}
      data-testid="norma-spatial"
    >
      <div className="environment" />
      <div
        className="environment-shade"
        style={{ backgroundColor: `rgba(5,15,28,${immersion / 150})` }}
      />
      <header className="topbar">
        <div className="brand">
          <div className="brand-mark">
            <Sparkles size={23} />
          </div>
          <span>
            norma<span className="brand-separator">/</span>
            <b>spatial</b>
          </span>
          <span className="prototype-label">交互原型</span>
        </div>
        <div className="space-name">
          <span className="status-dot" />
          我的空间<span className="subtle">·</span>宁静湖畔
        </div>
        <div className="top-actions">
          <Button
            variant="ghost"
            className="glass-button"
            onClick={() => setDialog('guide')}
          >
            <CircleHelp />
            操作指南
          </Button>
          <Button className="enter-xr" onClick={startXR} disabled={xrPending}>
            <Scan />
            {xrPending ? '正在连接…' : '进入 VR'}
          </Button>
        </div>
      </header>
      <section className="greeting">
        <div>
          <span className="eyebrow">NORMA-AI-OS / SPATIAL WORKSPACE</span>
          <h1>
            {mode === 'focus'
              ? '专注眼前这一件事。'
              : '把想法，直接画在工作现场。'}
          </h1>
        </div>
        <p>CLI、改动、图片与笔记，在同一张自由画布上。</p>
      </section>
      {variant === 'A' ? (
        <VariantA>
          <aside className="team-panel glass">
            <div className="panel-heading">
              <span>
                <Users size={17} />
                我的团队
              </span>
              <span className="tiny-pill">3 位</span>
            </div>
            <div className="team-intro">
              <div className="norma-avatar">
                <Sparkles size={22} />
              </div>
              <div>
                <strong>Norma</strong>
                <p>
                  你的协调者
                  <span className="status-dot" />
                </p>
              </div>
              <Radio className="team-radio" size={17} />
            </div>
            <div className="section-label">
              AGENTS <span>{active ? '协作中' : '随时待命'}</span>
            </div>
            {state.agents.map((a) => (
              <button
                key={a.id}
                className={`agent-row agent-${a.status}`}
                aria-label={`${a.name} · ${a.role} · ${statusLabels[a.status]}`}
                onClick={() => agentClick(a)}
              >
                <span
                  className="agent-avatar"
                  style={{ color: a.color, background: a.color + '1c' }}
                >
                  {a.id === 'codex' ? (
                    <Compass />
                  ) : a.id === 'claude' ? (
                    <Sparkles />
                  ) : (
                    <Code2 />
                  )}
                </span>
                <span className="agent-meta">
                  <strong>
                    {a.name}
                    <span className="agent-status">
                      {statusLabels[a.status]}
                    </span>
                  </strong>
                  <span>{a.role}</span>
                  <small>{a.task}</small>
                  {a.status !== 'ready' && (
                    <span
                      className="agent-progress"
                      role="progressbar"
                      aria-label={`${a.name} 进度`}
                      aria-valuenow={a.progress}
                      aria-valuemin={0}
                      aria-valuemax={100}
                    >
                      <i
                        style={{ width: `${a.progress}%`, background: a.color }}
                      />
                    </span>
                  )}
                </span>
              </button>
            ))}
            <div className="team-bottom">
              <span className="status-dot" />
              {active ? '团队正在协作' : '所有 Agent 已连接'}
              <span className="mock-tag">模拟</span>
            </div>
          </aside>
          <article
            className={`main-panel glass ${panel.pinned ? 'pinned' : ''}`}
            style={{
              transform: `translate(${panel.x}px,${panel.y}px) scale(${panel.scale * (2 / panel.depth) ** 0.35})`,
            }}
            data-testid="content-panel"
          >
            <div className="window-heading">
              <span className="window-dots" aria-hidden="true">
                <i />
                <i />
                <i />
              </span>
              <span className="window-title">
                <Layers size={15} />
                开发现场
              </span>
              <div className="window-actions">
                <button
                  aria-label={panel.pinned ? '解除固定' : '固定窗口'}
                  aria-pressed={panel.pinned}
                  onClick={() => {
                    updatePanel({ pinned: !panel.pinned });
                    notify(
                      panel.pinned ? '窗口已解锁，可以拖动。' : '窗口已固定。',
                    );
                  }}
                >
                  {panel.pinned ? <PinOff size={15} /> : <Pin size={15} />}
                </button>
                <button
                  aria-label={mode === 'focus' ? '退出专注' : '专注内容'}
                  aria-pressed={mode === 'focus'}
                  onClick={() =>
                    changeMode(mode === 'focus' ? 'workspace' : 'focus')
                  }
                >
                  <Maximize2 size={15} />
                </button>
              </div>
            </div>
            <Tabs
              value={state.view}
              onValueChange={(v) => changeView(v as View)}
            >
              <div className="content-nav">
                <TabsList className="content-tabs" aria-label="主面板内容">
                  {(
                    ['canvas', 'terminal', 'code', 'preview', 'model'] as View[]
                  ).map((key) => {
                    const label = viewLabels[key];
                    const Icon = icons[key];
                    return (
                      <TabsTrigger key={key} value={key}>
                        <Icon size={15} />
                        {label}
                      </TabsTrigger>
                    );
                  })}
                </TabsList>
                <span className="live-badge">
                  <span className="status-dot" />
                  {active ? '实时生成' : '实时预览'}
                </span>
              </div>
              <TabsContent value="canvas">{canvas}</TabsContent>
              <TabsContent value="terminal">
                <TerminalWorkbench
                  state={state}
                  run={cliRun}
                  select={selectCli}
                />
              </TabsContent>
              <TabsContent value="preview">
                <Preview send={send} state={state} />
              </TabsContent>
              <TabsContent value="research">
                <Research state={state} />
              </TabsContent>
              <TabsContent value="code">
                <Code state={state} />
              </TabsContent>
              <TabsContent value="data">
                <Data state={state} />
              </TabsContent>
              <TabsContent value="model">
                <ModelPlaceholder open={() => changeMode('orbit')} />
              </TabsContent>
            </Tabs>
            <div className="artifact-footer">
              <span>
                <Sparkles size={14} />
                {allDone
                  ? '团队成果已就绪 · 等你审阅'
                  : 'Codex + Claude · 会话输出与改动'}
              </span>
              <button
                className="return-canvas"
                onClick={() => changeView('canvas')}
              >
                返回画布 <Layers size={13} />
              </button>
            </div>
            <div
              className="window-grab"
              role="slider"
              tabIndex={0}
              aria-label="移动窗口"
              aria-valuemin={-360}
              aria-valuemax={360}
              aria-valuenow={panel.x}
              aria-valuetext={`水平 ${panel.x}，垂直 ${panel.y}`}
              aria-disabled={panel.pinned}
              onPointerDown={dragStart}
              onPointerMove={dragMove}
              onPointerUp={dragEnd}
              onPointerCancel={dragEnd}
              onKeyDown={(e) => {
                const moves: Record<string, Partial<Panel>> = {
                  ArrowLeft: { x: panel.x - 15 },
                  ArrowRight: { x: panel.x + 15 },
                  ArrowUp: { y: panel.y - 15 },
                  ArrowDown: { y: panel.y + 15 },
                };
                if (moves[e.key]) {
                  e.preventDefault();
                  updatePanel(moves[e.key]);
                }
              }}
            >
              <span />
            </div>
          </article>
          <aside className="activity-panel glass">
            <div className="panel-heading">
              <span>
                <Layers size={17} />
                当前任务
              </span>
              <span className="tiny-pill">01</span>
            </div>
            <div className="mission-kicker">
              {state.revision ? '正在让想法发生' : '一起做点什么'}
            </div>
            <h2>{state.mission}</h2>
            <p className="mission-description">
              {allDone
                ? '成果已经准备好。查看主面板，由你确认下一步。'
                : active
                  ? '查看命令与改动，随时进入 CLI 接管。'
                  : '说出目标，Norma 会把它分配给合适的伙伴。'}
            </p>
            <div className="mission-steps">
              <div>
                <span className="step-node">
                  <Check size={13} />
                </span>
                <span>
                  <strong>理解你的想法</strong>
                  <small>目标与上下文</small>
                </span>
              </div>
              <div>
                <span className={`step-node ${active ? 'running' : 'hollow'}`}>
                  {allDone ? <Check size={13} /> : 2}
                </span>
                <span>
                  <strong>团队协同执行</strong>
                  <small>
                    {active
                      ? `总体进度 ${Math.round(state.agents.reduce((sum, a) => sum + a.progress, 0) / 3)}%`
                      : '实现 → 审查 → 验证'}
                  </small>
                </span>
              </div>
              <div>
                <span className={`step-node ${state.approved ? '' : 'hollow'}`}>
                  {state.approved ? <Check size={13} /> : 3}
                </span>
                <span>
                  <strong>
                    {state.approved ? '方案已确认' : '由你确认成果'}
                  </strong>
                  <small>
                    {allDone ? '所有演示成果已整理' : '每一步都可以接管'}
                  </small>
                </span>
              </div>
            </div>
            {active || paused ? (
              <Button
                variant="ghost"
                className="mission-control"
                onClick={() => send(active ? '全部暂停' : '继续执行')}
              >
                {active ? <Pause size={15} /> : <Play size={15} />}{' '}
                {active ? '暂停团队' : '继续执行'}
              </Button>
            ) : allDone ? (
              <Button
                className="approve-button"
                disabled={state.approved}
                onClick={() => send('批准方案')}
              >
                <Check size={15} />
                {state.approved ? '方案已确认' : '确认成果'}
              </Button>
            ) : (
              <button
                className="suggested-mission"
                onClick={() => send(developmentGoal)}
              >
                <span>
                  <Compass size={18} />
                  <strong>启动 Codex + Claude 协作</strong>
                </span>
                <small>
                  试试让整个团队动起来 <ChevronRight size={14} />
                </small>
              </button>
            )}
            <div className="activity-note">
              <span className="status-dot" />
              你的想法，始终由你掌控。
            </div>
          </aside>
        </VariantA>
      ) : variant === 'B' ? (
        <VariantB canvas={canvas} state={state} send={send} />
      ) : (
        <VariantC canvas={canvas} state={state} send={send} />
      )}
      <section className="conversation">
        <div className="norma-caption">
          <span className="norma-spark">
            <Sparkles size={17} />
          </span>
          <p aria-live="polite">{voice.transcript || lastReply}</p>
          <button
            className="caption-action"
            onClick={() => setDialog('history')}
            aria-label="查看对话记录"
          >
            <MessageCircle size={16} />
          </button>
        </div>
        <form
          className={`composer glass ${voice.listening ? 'is-listening' : ''}`}
          onSubmit={(e) => {
            e.preventDefault();
            if (input.trim()) {
              voice.stop();
              send(input);
            }
          }}
        >
          <button
            className="mic-button"
            type="button"
            aria-label={voice.listening ? '结束语音对话' : '与 Norma 对话'}
            aria-pressed={voice.listening}
            onClick={voice.start}
          >
            {voice.listening ? (
              <span className="audio-wave">
                <i />
                <i />
                <i />
                <i />
              </span>
            ) : (
              <AudioLines size={23} />
            )}
          </button>
          <input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onFocus={() => {
              if (voice.speaking) voice.stop();
            }}
            placeholder={
              voice.listening ? '正在听你说话…' : '告诉 Norma，你想完成什么…'
            }
            aria-label="给 Norma 的指令"
            maxLength={500}
          />
          <span className="composer-hint">
            {voice.listening ? '语音已开启' : '随时打断 · 随时改变方向'}
          </span>
          <button
            type="button"
            className="voice-output"
            aria-label={voice.readAloud ? '关闭语音回复' : '开启语音回复'}
            aria-pressed={voice.readAloud}
            onClick={() => {
              if (voice.readAloud) voice.stop();
              voice.setReadAloud(!voice.readAloud);
            }}
          >
            {voice.readAloud ? <Volume2 size={16} /> : <VolumeX size={16} />}
          </button>
          <Button
            type="submit"
            size="icon"
            className="send-button"
            disabled={!input.trim()}
            aria-label="发送指令"
          >
            <ArrowUp />
          </Button>
        </form>
        <div className="prompt-chips">
          <button onClick={() => send(developmentGoal)}>
            <Sparkles size={13} />
            启动 Codex + Claude 协作
          </button>
          <button onClick={() => send('让 Claude 审查 Codex 的改动')}>
            让 Claude 审查改动
          </button>
          <button onClick={() => send(active ? '全部暂停' : '汇报进度')}>
            {active ? '暂停一下，我有新想法' : '团队进展怎么样？'}
          </button>
        </div>
      </section>
      <footer className="bottom-bar">
        <div className="environment-control">
          <Mountain size={15} />
          <span>沉浸度</span>
          <Slider
            value={[immersion]}
            onValueChange={(value) =>
              setImmersion(Array.isArray(value) ? value[0] : value)
            }
            min={0}
            max={80}
            aria-label="环境沉浸度"
          />
          <span>{immersion}%</span>
        </div>
        <div className="mode-switch glass" role="group" aria-label="空间模式">
          <button
            className={mode === 'workspace' ? 'selected' : ''}
            aria-pressed={mode === 'workspace'}
            onClick={() => changeMode('workspace')}
          >
            <Layers size={16} />
            工作台
          </button>
          <button
            className={mode === 'focus' ? 'selected' : ''}
            aria-pressed={mode === 'focus'}
            onClick={() => changeMode('focus')}
          >
            <Maximize2 size={16} />
            专注
          </button>
          <button
            className={mode === 'orbit' ? 'selected' : ''}
            aria-pressed={mode === 'orbit'}
            onClick={() => changeMode('orbit')}
          >
            <Scan size={16} />
            空间漫游
          </button>
        </div>
        <div className="panel-controls">
          <button
            onClick={() => updatePanel({ scale: panel.scale - 0.1 })}
            aria-label="缩小窗口"
            disabled={panel.scale <= 0.7}
          >
            <Minus size={15} />
          </button>
          <span>{Math.round(panel.scale * 100)}%</span>
          <button
            onClick={() => updatePanel({ scale: panel.scale + 0.1 })}
            aria-label="放大窗口"
            disabled={panel.scale >= 1.35}
          >
            <Plus size={15} />
          </button>
          <button
            onClick={() => updatePanel({ depth: panel.depth === 2 ? 2.6 : 2 })}
            aria-label="调整窗口距离"
          >
            {panel.depth.toFixed(1)}m
          </button>
          <button
            onClick={() => {
              setPanel({ ...defaultPanel });
              notify('窗口已回到初始位置。');
            }}
            aria-label="重置窗口布局"
          >
            <RotateCcw size={15} />
          </button>
        </div>
      </footer>
      <PrototypeSwitcher variant={variant} change={changeVariant} />
      {notice && (
        <div className="notice glass" role="status">
          <span>{notice}</span>
          <button onClick={() => setNotice('')} aria-label="关闭提示">
            <X size={15} />
          </button>
        </div>
      )}
      {(mode === 'orbit' || xrSession) && (
        <SpatialScene
          enterVR={startXR}
          pending={xrPending}
          session={xrSession}
          bridge={{
            read: () => stateRef.current,
            send,
            view: changeView,
            canvas: changeCanvas,
            voice: voice.start,
            listening: () => voiceRef.current.listening,
            voiceState: () => voiceRef.current,
            toggleVoiceOutput: () =>
              voiceRef.current.setReadAloud(!voiceRef.current.readAloud),
            notice: notify,
            leave: () => {
              setXrSession(null);
              changeMode(xrSession ? 'orbit' : 'workspace');
            },
          }}
        />
      )}
      <Dialog
        open={dialog !== null}
        onOpenChange={(open) => {
          if (!open) setDialog(null);
        }}
      >
        <DialogContent className="norma-dialog" showCloseButton={false}>
          <button
            className="dialog-close"
            onClick={() => setDialog(null)}
            aria-label="关闭面板"
          >
            <X size={19} />
          </button>
          <DialogTitle>
            {dialog === 'guide'
              ? '你的空间，跟随你的方式。'
              : dialog === 'history'
                ? '与 Norma 的对话'
                : `${selected.name} · ${selected.role}`}
          </DialogTitle>
          <DialogDescription>
            {dialog === 'guide'
              ? '数据与执行均为模拟；窗口、三维空间和对话输入可以实际操作。'
              : dialog === 'history'
                ? '这次对话只保留在当前页面内。'
                : `当前状态：${statusLabels[selected.status]} · ${selected.progress}%`}
          </DialogDescription>
          {dialog === 'guide' && (
            <>
              <div className="guide-grid">
                <section>
                  <Scan />
                  <h3>在头显里</h3>
                  <p>
                    看向目标并捏合选择。选择画笔后捏合移动进行标注；选择工具可移动内容。标题栏移动窗口，按钮调整大小、远近与居中。
                  </p>
                </section>
                <section>
                  <Grip />
                  <h3>在电脑上</h3>
                  <p>
                    拖动面板下方横条布置空间。在「空间漫游」中，点击选择，拖动标题移动，拖动空白环顾。
                  </p>
                </section>
                <section>
                  <AudioLines />
                  <h3>直接说出来</h3>
                  <p>
                    点击麦克风开始连续对话。支持时使用浏览器语音服务。点名、暂停、继续或改变目标都可以。
                  </p>
                </section>
                <section>
                  <Layers />
                  <h3>让内容来到眼前</h3>
                  <p>
                    自由添加
                    CLI、便签、图片与预览。拖动标题摆放，右下角缩放；画笔圈出问题后，点「标注交给
                    Norma」。
                  </p>
                </section>
              </div>
              <div className="device-status">
                <span className="status-dot" />
                {xrSupported
                  ? '检测到 VR 支持，可以进入头显空间。'
                  : '当前浏览器未连接 VR 头显。空间漫游可直接体验。'}
              </div>
              <p className="guide-footnote">
                头显需支持 WebXR 的浏览器和可信 HTTPS
                地址。此版本未经过真实头显佩戴验收，不读取持续眼动数据。
              </p>
              <Button
                className="guide-start"
                onClick={() => {
                  setDialog(null);
                  changeMode('orbit');
                }}
              >
                体验空间漫游 <ChevronRight size={16} />
              </Button>
            </>
          )}
          {dialog === 'history' && (
            <div className="message-history">
              {state.messages.map((m, i) => (
                <div key={i} className={`message-${m.role}`}>
                  <strong>{m.role === 'norma' ? 'Norma' : '你'}</strong>
                  <p>{m.text}</p>
                </div>
              ))}
            </div>
          )}
          {dialog === 'agent' && (
            <div className="agent-detail">
              <span className="detail-avatar" style={{ color: selected.color }}>
                {selected.name[0]}
              </span>
              <p>{selected.task}</p>
              <div className="detail-progress">
                <span
                  style={{
                    width: `${selected.progress}%`,
                    background: selected.color,
                  }}
                />
              </div>
              <div className="agent-detail-actions">
                <Button
                  disabled={
                    selected.status === 'ready' || selected.status === 'done'
                  }
                  onClick={() =>
                    send(
                      `${selected.status === 'paused' ? '继续' : '暂停'} ${selected.name}`,
                    )
                  }
                >
                  {selected.status === 'paused' ? <Play /> : <Pause />}
                  {selected.status === 'paused' ? '继续执行' : '暂停任务'}
                </Button>
                <Button
                  variant="ghost"
                  onClick={() => {
                    setDialog(null);
                    selected.id === 'verifier'
                      ? changeView('data')
                      : selectCli(selected.id as CliId);
                  }}
                >
                  查看成果 <ChevronRight />
                </Button>
              </div>
              <p className="guide-footnote">
                也可以说：“{selected.name}
                ，帮我重新设计首页。”其他伙伴会保留自己的任务。
              </p>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </main>
  );
}
