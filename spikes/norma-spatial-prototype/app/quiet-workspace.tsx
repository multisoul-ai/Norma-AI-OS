'use client';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import {
  Sparkles,
  Ellipsis,
  AudioLines,
  ArrowUp,
  X,
  Scan,
  Play,
  Pause,
  Volume2,
  Check,
  MessageCircle,
} from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog';
import type { State, View } from '@/lib/model';
import { developmentGoal, type CliId } from '@/lib/cli';
import type { CanvasDocument } from '@/lib/free-canvas';
import type { useVoice } from '@/hooks/use-voice';
import { FreeCanvas } from './free-canvas';
import {
  Code,
  Data,
  Research,
  Preview,
  ModelPlaceholder,
} from './artifact-content';
import { TerminalWorkbench } from './terminal-workbench';
import './quiet.css';
export function QuietWorkspace({
  state,
  send,
  run,
  change,
  view,
  voice,
  space,
  enterVR,
  scene,
  notice,
  dismiss,
}: {
  state: State;
  send: (text: string) => void;
  run: (id: CliId, text: string) => void;
  change: (doc: CanvasDocument) => void;
  view: (view: View) => void;
  voice: ReturnType<typeof useVoice>;
  space: () => void;
  enterVR: () => void;
  scene: ReactNode;
  notice: string;
  dismiss: () => void;
}) {
  const [input, setInput] = useState('');
  const [popover, setPopover] = useState<'team' | 'more' | null>(null);
  const [sheet, setSheet] = useState<'guide' | 'history' | null>(null);
  const shell = useRef<HTMLDivElement>(null);
  const running = state.agents.filter((a) => a.status === 'working').length;
  useEffect(() => {
    const outside = (e: PointerEvent) => {
      if (
        !(e.target as HTMLElement).closest('.quiet-popover,.quiet-top-actions')
      )
        setPopover(null);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setPopover(null);
        setSheet(null);
      }
    };
    document.addEventListener('pointerdown', outside);
    window.addEventListener('keydown', key);
    return () => {
      document.removeEventListener('pointerdown', outside);
      window.removeEventListener('keydown', key);
    };
  }, []);
  const reply =
    voice.transcript ||
    (state.revision
      ? state.messages.at(-1)?.text
      : '从一个想法开始。剩下的，一起完成。');
  return (
    <div ref={shell} className="quiet-workspace" data-testid="quiet-workspace">
      {scene || (
        <>
          <div className="quiet-environment" />
          <header className="quiet-header">
            <div className="quiet-brand">
              <Sparkles size={21} />
              <span>norma</span>
              <small>Spatial</small>
            </div>
            <div className="quiet-project">
              <span className="status-dot" />
              Norma-AI-OS
            </div>
            <div className="quiet-top-actions">
              <button
                aria-label="查看团队"
                aria-expanded={popover === 'team'}
                onClick={() => setPopover(popover === 'team' ? null : 'team')}
              >
                <span className="quiet-team-dots">
                  <i />
                  <i />
                  <i />
                </span>
                <span>{running ? `${running} 位协作中` : '团队就绪'}</span>
              </button>
              <button
                aria-label="空间选项"
                aria-expanded={popover === 'more'}
                onClick={() => setPopover(popover === 'more' ? null : 'more')}
              >
                <Ellipsis size={21} />
              </button>
            </div>
          </header>
          {popover && (
            <div
              className={`quiet-popover popover-${popover}`}
              role="region"
              aria-label={popover === 'team' ? '团队面板' : '空间选项菜单'}
            >
              {popover === 'team' ? (
                <>
                  <div className="quiet-popover-title">
                    你的团队<span>模拟会话</span>
                  </div>
                  {state.agents.map((a) => (
                    <div className="quiet-agent" key={a.id}>
                      <i style={{ background: a.color }} />
                      <div>
                        <strong>{a.name}</strong>
                        <small>
                          {a.status === 'working'
                            ? `执行中 · ${a.progress}%`
                            : a.status === 'paused'
                              ? '已暂停'
                              : a.status === 'done'
                                ? '已完成'
                                : '准备就绪'}
                        </small>
                      </div>
                      <button
                        aria-label={`${a.status === 'paused' ? '继续' : '暂停'} ${a.name}`}
                        disabled={a.status === 'ready' || a.status === 'done'}
                        onClick={() =>
                          send(
                            `${a.status === 'paused' ? '继续' : '暂停'} ${a.name}`,
                          )
                        }
                      >
                        {a.status === 'paused' ? (
                          <Play size={15} />
                        ) : (
                          <Pause size={15} />
                        )}
                      </button>
                    </div>
                  ))}
                  <button
                    className="quiet-start"
                    onClick={() => {
                      send(developmentGoal);
                      setPopover(null);
                    }}
                  >
                    <Play size={14} />
                    开始协作
                  </button>
                </>
              ) : (
                <>
                  <button
                    onClick={() => {
                      setPopover(null);
                      space();
                    }}
                  >
                    <Scan size={17} />
                    空间漫游
                  </button>
                  <button
                    onClick={() => {
                      setPopover(null);
                      enterVR();
                    }}
                  >
                    <Scan size={17} />
                    进入 VR
                  </button>
                  <button
                    aria-pressed={voice.readAloud}
                    onClick={() => voice.setReadAloud(!voice.readAloud)}
                  >
                    <Volume2 size={17} />
                    语音回复{voice.readAloud && <Check size={15} />}
                  </button>
                  <button
                    onClick={() => {
                      setPopover(null);
                      setSheet('history');
                    }}
                  >
                    <MessageCircle size={17} />
                    对话记录
                  </button>
                  <button
                    onClick={() => {
                      setPopover(null);
                      setSheet('guide');
                    }}
                  >
                    <Sparkles size={17} />
                    操作指南
                  </button>
                </>
              )}
            </div>
          )}
          <div
            style={{
              visibility: state.view === 'canvas' ? 'visible' : 'hidden',
            }}
            inert={state.view !== 'canvas'}
            aria-hidden={state.view !== 'canvas'}
          >
            <FreeCanvas
              state={state}
              doc={state.canvas}
              change={change}
              run={run}
              send={send}
              layout="quiet"
              minimal
            />
          </div>
          {state.view !== 'canvas' && (
            <div
              className="quiet-content-focus"
              role="region"
              aria-label="内容阅读"
            >
              <header>
                <span>
                  {state.view === 'terminal'
                    ? 'CLI 会话'
                    : state.view === 'code'
                      ? '代码改动'
                      : '内容'}
                </span>
                <button
                  aria-label="返回自由画布"
                  onClick={() => view('canvas')}
                >
                  <X size={19} />
                </button>
              </header>
              {state.view === 'terminal' ? (
                <TerminalWorkbench
                  state={state}
                  run={run}
                  select={(id) => send(`打开 ${id} CLI`)}
                />
              ) : state.view === 'code' ? (
                <Code state={state} />
              ) : state.view === 'data' ? (
                <Data state={state} />
              ) : state.view === 'research' ? (
                <Research state={state} />
              ) : state.view === 'preview' ? (
                <Preview state={state} send={send} />
              ) : (
                <ModelPlaceholder open={space} />
              )}
            </div>
          )}
          <section className="quiet-conversation">
            <p aria-live="polite">{reply}</p>
            <form
              className={voice.listening ? 'listening' : ''}
              onSubmit={(e) => {
                e.preventDefault();
                if (input.trim()) {
                  voice.stop();
                  send(input);
                  setInput('');
                }
              }}
            >
              <button
                type="button"
                className="quiet-voice"
                aria-label={voice.listening ? '结束语音对话' : '与 Norma 对话'}
                aria-pressed={voice.listening}
                onClick={voice.start}
              >
                <AudioLines size={23} />
              </button>
              <input
                aria-label="给 Norma 的指令"
                placeholder={
                  voice.listening ? '正在听…' : '告诉 Norma，你想做什么'
                }
                value={input}
                onChange={(e) => setInput(e.target.value)}
                maxLength={500}
              />
              {input.trim() && (
                <button
                  type="submit"
                  className="quiet-send"
                  aria-label="发送指令"
                >
                  <ArrowUp size={18} />
                </button>
              )}
            </form>
          </section>
          <span className="quiet-prototype-note">
            交互原型 · CLI 执行为模拟
          </span>
        </>
      )}
      {notice && (
        <div className="quiet-notice" role="status">
          {notice}
          <button aria-label="关闭提示" onClick={dismiss}>
            <X size={16} />
          </button>
        </div>
      )}
      <Dialog
        open={sheet !== null}
        onOpenChange={(open) => {
          if (!open) setSheet(null);
        }}
      >
        <DialogContent
          className="norma-dialog quiet-dialog"
          showCloseButton={false}
        >
          <button
            className="dialog-close"
            aria-label="关闭面板"
            onClick={() => setSheet(null)}
          >
            <X />
          </button>
          <DialogTitle>
            {sheet === 'history' ? '与 Norma 的对话' : '让内容留在眼前。'}
          </DialogTitle>
          <DialogDescription>
            {sheet === 'history'
              ? '对话只保留在当前页面。'
              : '操作跟随你的意图出现。'}
          </DialogDescription>
          {sheet === 'history' ? (
            <div className="message-history">
              {state.messages.map((m, i) => (
                <div key={i}>
                  <strong>{m.role === 'user' ? '你' : 'Norma'}</strong>
                  <p>{m.text}</p>
                </div>
              ))}
            </div>
          ) : (
            <div className="quiet-guide">
              <p>拖动标题摆放内容。滚轮移动画布，Cmd / Ctrl + 滚轮缩放。</p>
              <p>
                选中内容后，右上角「···」提供会话操作。双击标题可以专注阅读。
              </p>
              <p>
                选择画笔开始标注，完成后交给 Norma。Escape 返回选择；Cmd / Ctrl
                + Z 撤销。
              </p>
              <p>
                在空间漫游中同样可以画、移动内容和捏合选择。头显需支持
                WebXR，当前尚未经过实机佩戴验收。
              </p>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
