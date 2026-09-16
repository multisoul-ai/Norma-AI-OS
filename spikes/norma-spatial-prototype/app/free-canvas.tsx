'use client';
import { useEffect, useRef, useState, type PointerEvent as PE } from 'react';
import {
  MousePointer2,
  PenLine,
  Eraser,
  Hand,
  Undo2,
  Redo2,
  Plus,
  Minus,
  Focus,
  Send,
  TerminalSquare,
  StickyNote,
  ImageIcon,
  Globe2,
  FileDiff,
  X,
  Move,
  Grip,
  CornerDownLeft,
  Ellipsis,
  Check,
} from 'lucide-react';
import { canvasControls } from '@/lib/interaction-chrome';
import type { State } from '@/lib/model';
import type { CliId } from '@/lib/cli';
import { TerminalSession } from './terminal-workbench';
import {
  toWorld,
  zoomAt,
  appendStroke,
  undoStroke,
  redoStroke,
  strokeHits,
  annotationCommand,
  type CanvasDocument,
  type CanvasNode,
  type CanvasKind,
  type CanvasTool,
  type Viewport,
  type Stroke,
} from '@/lib/free-canvas';
export const diffLines = [
  '@@ runtime::route · 焦点路由演示 @@',
  '  match mode {',
  '    InputMode::NodeFocused(fid) => match event {',
  '+     InputEvent::Key => Route::SendInputToNode(*fid),',
  '+     InputEvent::Escape => Route::ExitNodeFocus,',
  '      InputEvent::CloseNode { node } => Route::RemoveNode(*node),',
  '    }',
  '  }',
  '',
  '+ test focused_node_routes_key_to_active_session',
  '+ test escape_exits_focus_without_closing_session',
];
export function CanvasDiff() {
  return (
    <div className="canvas-diff">
      <div className="diff-file">
        <FileDiff size={13} />
        crates/runtime/src/route.rs <span>演示 diff</span>
      </div>
      <pre>
        {diffLines.map((line, i) => (
          <div
            key={i}
            className={
              line.startsWith('+')
                ? 'added'
                : line.startsWith('@@')
                  ? 'hunk'
                  : ''
            }
          >
            {line || ' '}
          </div>
        ))}
      </pre>
    </div>
  );
}
type Gesture = {
  kind: 'pan' | 'move' | 'resize' | 'draw' | 'erase';
  id: number;
  startX: number;
  startY: number;
  view: Viewport;
  node?: CanvasNode;
  stroke?: Stroke;
};
export function FreeCanvas({
  state,
  doc,
  change,
  run,
  send,
  layout = 'A',
  minimal = false,
}: {
  state: State;
  doc: CanvasDocument;
  change: (doc: CanvasDocument) => void;
  run: (id: CliId, text: string) => void;
  send: (text: string) => void;
  layout?: string;
  minimal?: boolean;
}) {
  const stage = useRef<HTMLDivElement>(null);
  const ink = useRef<HTMLCanvasElement>(null);
  const docRef = useRef(doc);
  docRef.current = doc;
  const [tool, setTool] = useState<CanvasTool>('select');
  const [color, setColor] = useState('#b5e7dc');
  const [view, setView] = useState<Viewport>({ x: 12, y: 10, zoom: 0.76 });
  const viewRef = useRef(view);
  viewRef.current = view;
  const [selected, setSelected] = useState<string | null>(
    minimal ? null : 'codex-1',
  );
  const gesture = useRef<Gesture | null>(null);
  const [draft, setDraft] = useState<Stroke | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const [nodeMenu, setNodeMenu] = useState<string | null>(null);
  const [viewMenu, setViewMenu] = useState(false);
  const [penSettings, setPenSettings] = useState(false);
  const controls = canvasControls(tool);
  const [spaceHeld, setSpaceHeld] = useState(false);
  const setDocument = (next: CanvasDocument) => {
    docRef.current = next;
    change(next);
  };
  function fit() {
    if (!stage.current || !docRef.current.nodes.length) return;
    const nodes = docRef.current.nodes;
    const minX = Math.min(...nodes.map((n) => n.x)),
      minY = Math.min(...nodes.map((n) => n.y));
    const maxX = Math.max(...nodes.map((n) => n.x + n.w)),
      maxY = Math.max(...nodes.map((n) => n.y + n.h));
    const zoom = Math.max(
      0.35,
      Math.min(
        1,
        (stage.current.clientWidth - 75) / (maxX - minX),
        (stage.current.clientHeight - 65) / (maxY - minY),
      ),
    );
    setView({ zoom, x: 35 - minX * zoom, y: 28 - minY * zoom });
  }
  useEffect(() => {
    if (!stage.current) return;
    const width = stage.current.clientWidth;
    if (minimal) {
      const zoom = Math.min(
        1,
        (width - 120) / 1100,
        (stage.current.clientHeight - 80) / 735,
      );
      setView({ zoom, x: (width - 1080 * zoom) / 2, y: 38 });
      return;
    }
    setView({
      x: 10,
      y: 8,
      zoom: Math.max(
        0.45,
        Math.min(layout === 'B' ? 0.95 : 0.8, (width - 36) / 1080),
      ),
    });
  }, [layout]);
  useEffect(() => {
    const canvas = ink.current,
      container = stage.current;
    if (!canvas || !container) return;
    const paint = () => {
      const ratio = Math.min(window.devicePixelRatio, 2);
      canvas.width = container.clientWidth * ratio;
      canvas.height = container.clientHeight * ratio;
      const c = canvas.getContext('2d')!;
      c.scale(ratio, ratio);
      c.translate(view.x, view.y);
      c.scale(view.zoom, view.zoom);
      for (const stroke of [...doc.strokes, ...(draft ? [draft] : [])]) {
        c.strokeStyle = stroke.color;
        c.lineCap = 'round';
        c.lineJoin = 'round';
        for (let i = 1; i < stroke.points.length; i++) {
          const a = stroke.points[i - 1],
            b = stroke.points[i];
          c.lineWidth = stroke.width * (0.65 + b.pressure * 0.7);
          c.beginPath();
          c.moveTo(a.x, a.y);
          c.lineTo(b.x, b.y);
          c.stroke();
        }
      }
    };
    paint();
    const observer = new ResizeObserver(paint);
    observer.observe(container);
    return () => observer.disconnect();
  }, [doc.strokes, draft, view]);
  useEffect(() => {
    const el = stage.current;
    if (!el) return;
    const wheel = (e: WheelEvent) => {
      if (
        !e.ctrlKey &&
        !e.metaKey &&
        (e.target as HTMLElement).closest('.cli-log,textarea,.canvas-diff')
      )
        return;
      e.preventDefault();
      const b = el.getBoundingClientRect();
      setView((v) =>
        e.ctrlKey || e.metaKey
          ? zoomAt(v, v.zoom * Math.exp(-e.deltaY * 0.002), {
              x: e.clientX - b.left,
              y: e.clientY - b.top,
            })
          : { ...v, x: v.x - e.deltaX, y: v.y - e.deltaY },
      );
    };
    el.addEventListener('wheel', wheel, { passive: false });
    return () => el.removeEventListener('wheel', wheel);
  }, []);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.closest('input,textarea,[contenteditable=true],[role=dialog]'))
        return;
      if (e.code === 'Space') {
        e.preventDefault();
        setSpaceHeld(e.type === 'keydown');
      }
      if (e.type === 'keyup') return;
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'z') {
        e.preventDefault();
        setDocument(
          e.shiftKey ? redoStroke(docRef.current) : undoStroke(docRef.current),
        );
      }
      if (e.key === 'Escape') {
        setTool('select');
        setSelected(null);
      }
    };
    window.addEventListener('keydown', key);
    window.addEventListener('keyup', key);
    return () => {
      window.removeEventListener('keydown', key);
      window.removeEventListener('keyup', key);
    };
  }, []);
  useEffect(() => {
    if (layout === 'A' || state.view === 'canvas' || !stage.current) return;
    const kind =
      state.view === 'terminal'
        ? state.activeCli
        : state.view === 'code'
          ? 'diff'
          : state.view === 'preview'
            ? 'preview'
            : null;
    const node = kind && docRef.current.nodes.find((n) => n.kind === kind);
    if (node) {
      const z = Math.min(1, (stage.current.clientHeight - 45) / node.h);
      setSelected(node.id);
      setView({
        zoom: z,
        x: (stage.current.clientWidth - node.w * z) / 2 - node.x * z,
        y: 20 - node.y * z,
      });
    }
  }, [state.view, state.activeCli, layout]);
  function coordinate(e: {
    clientX: number;
    clientY: number;
    pressure?: number;
  }) {
    const b = stage.current!.getBoundingClientRect();
    return {
      ...toWorld(
        { x: e.clientX - b.left, y: e.clientY - b.top },
        viewRef.current,
      ),
      pressure: e.pressure || 0.5,
    };
  }
  function start(e: PE<HTMLElement>, kind: Gesture['kind'], node?: CanvasNode) {
    if (gesture.current) return;
    e.preventDefault();
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    const g: Gesture = {
      kind,
      id: e.pointerId,
      startX: e.clientX,
      startY: e.clientY,
      view: viewRef.current,
      node,
    };
    if (kind === 'draw') {
      g.stroke = {
        id: crypto.randomUUID(),
        color,
        width: 3.5,
        points: [coordinate(e)],
      };
      setDraft(g.stroke);
    }
    gesture.current = g;
    if (node) setSelected(node.id);
  }
  function move(e: PE<HTMLElement>) {
    const g = gesture.current;
    if (!g || g.id !== e.pointerId) return;
    const dx = e.clientX - g.startX,
      dy = e.clientY - g.startY;
    if (g.kind === 'pan')
      setView({ ...g.view, x: g.view.x + dx, y: g.view.y + dy });
    if ((g.kind === 'move' || g.kind === 'resize') && g.node) {
      const n = g.node;
      setDocument({
        ...docRef.current,
        nodes: docRef.current.nodes.map((item) =>
          item.id !== n.id
            ? item
            : g.kind === 'move'
              ? {
                  ...item,
                  x: n.x + dx / g.view.zoom,
                  y: n.y + dy / g.view.zoom,
                }
              : {
                  ...item,
                  w: Math.max(280, Math.min(1000, n.w + dx / g.view.zoom)),
                  h: Math.max(190, Math.min(800, n.h + dy / g.view.zoom)),
                },
        ),
      });
    }
    if (g.kind === 'draw' && g.stroke) {
      const samples = e.nativeEvent.getCoalescedEvents?.() ?? [e.nativeEvent];
      g.stroke = {
        ...g.stroke,
        points: [
          ...g.stroke.points,
          ...(samples.length ? samples : [e.nativeEvent]).map(coordinate),
        ],
      };
      setDraft(g.stroke);
    }
    if (g.kind === 'erase') {
      const p = coordinate(e);
      setDocument({
        ...docRef.current,
        strokes: docRef.current.strokes.filter(
          (s) => !strokeHits(s, p, 15 / view.zoom),
        ),
        undone: [],
      });
    }
  }
  function cancel(e: PE<HTMLElement>) {
    gesture.current = null;
    setDraft(null);
    if (e.currentTarget.hasPointerCapture(e.pointerId))
      e.currentTarget.releasePointerCapture(e.pointerId);
  }
  function end(e: PE<HTMLElement>) {
    const g = gesture.current;
    if (!g || g.id !== e.pointerId) return;
    if (g.kind === 'draw' && g.stroke)
      setDocument(appendStroke(docRef.current, g.stroke));
    gesture.current = null;
    setDraft(null);
    if (e.currentTarget.hasPointerCapture(e.pointerId))
      e.currentTarget.releasePointerCapture(e.pointerId);
  }
  function add(kind: CanvasKind) {
    const center = toWorld(
      { x: (stage.current?.clientWidth ?? 800) / 2, y: 150 },
      viewRef.current,
    );
    const title = {
      codex: 'Codex · 实现',
      claude: 'Claude · 审查',
      diff: 'route.rs · 演示 diff',
      note: '新便签',
      image: '空间参考图',
      preview: '应用预览',
    }[kind];
    const node: CanvasNode = {
      id: crypto.randomUUID(),
      kind,
      title,
      x: center.x - 210,
      y: center.y,
      w: kind === 'note' ? 340 : 460,
      h: kind === 'note' ? 220 : 360,
      text: kind === 'note' ? '在这里写下你的想法…' : undefined,
    };
    setDocument({ ...docRef.current, nodes: [...docRef.current.nodes, node] });
    setSelected(node.id);
    setShowAdd(false);
    setTool('select');
  }
  return (
    <div
      className={`free-canvas ${minimal ? 'quiet-canvas' : ''}`}
      data-testid="free-canvas"
    >
      {!minimal && (
        <>
          <div className="canvas-topline">
            <span>
              <span className="status-dot" />
              自由画布{' '}
              <small>
                {doc.nodes.length} 个内容 · {doc.strokes.length} 处标注
              </small>
            </span>
            <button
              className="canvas-add-button"
              aria-expanded={showAdd}
              onClick={() => setShowAdd(!showAdd)}
            >
              <Plus size={14} />
              添加内容
            </button>
          </div>
        </>
      )}
      {showAdd && (
        <div className="canvas-add-menu" role="group" aria-label="添加画布内容">
          {(
            [
              ['codex', 'Codex CLI', TerminalSquare],
              ['claude', 'Claude CLI', TerminalSquare],
              ['note', '便签', StickyNote],
              ['diff', '代码改动', FileDiff],
              ['image', '图片', ImageIcon],
              ['preview', '网页预览', Globe2],
            ] as const
          ).map(([kind, label, Icon]) => (
            <button key={kind} onClick={() => add(kind)}>
              <Icon size={16} />
              {label}
            </button>
          ))}
        </div>
      )}
      <div
        ref={stage}
        className={`canvas-stage tool-${spaceHeld ? 'pan' : tool}`}
        style={{
          backgroundSize: `${24 * view.zoom}px ${24 * view.zoom}px`,
          backgroundPosition: `${view.x}px ${view.y}px`,
        }}
        onPointerDown={(e) => {
          if (e.target === e.currentTarget) {
            setSelected(null);
            setNodeMenu(null);
            setShowAdd(false);
            setViewMenu(false);
          }
          if (e.target === e.currentTarget || spaceHeld || tool === 'pan')
            start(e, 'pan');
        }}
        onPointerMove={move}
        onPointerUp={end}
        onPointerCancel={cancel}
      >
        <div
          className="canvas-world"
          style={{
            transform: `translate(${view.x}px,${view.y}px) scale(${view.zoom})`,
          }}
        >
          {doc.nodes.map((node) => (
            <article
              key={node.id}
              className={`canvas-node node-${node.kind} ${selected === node.id ? 'node-selected' : ''}`}
              style={{
                left: node.x,
                top: node.y,
                width: node.w,
                height: node.h,
              }}
              tabIndex={minimal ? 0 : undefined}
              onFocus={() => {
                if (minimal) setSelected(node.id);
              }}
              onPointerDown={(e) => {
                if (tool === 'select' && !spaceHeld) {
                  e.stopPropagation();
                  setSelected(node.id);
                }
              }}
            >
              <header
                className="node-heading"
                onDoubleClick={() => {
                  if (minimal) {
                    if (node.kind === 'codex' || node.kind === 'claude')
                      send(`打开 ${node.kind} CLI`);
                    else if (node.kind === 'diff') send('展示代码改动');
                  }
                }}
                onPointerDown={(e) =>
                  start(e, spaceHeld || tool === 'pan' ? 'pan' : 'move', node)
                }
                onPointerMove={move}
                onPointerUp={end}
                onPointerCancel={cancel}
              >
                <span className={`node-glyph glyph-${node.kind}`}>
                  {node.kind === 'codex' || node.kind === 'claude' ? (
                    <TerminalSquare size={14} />
                  ) : node.kind === 'note' ? (
                    <StickyNote size={14} />
                  ) : node.kind === 'diff' ? (
                    <FileDiff size={14} />
                  ) : (
                    <Globe2 size={14} />
                  )}
                </span>
                <span>{node.title}</span>
                {!minimal && <Grip size={12} />}
                {minimal && selected === node.id && (
                  <button
                    aria-label={`${node.title} 操作`}
                    aria-expanded={nodeMenu === node.id}
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={() =>
                      setNodeMenu(nodeMenu === node.id ? null : node.id)
                    }
                  >
                    <Ellipsis size={18} />
                  </button>
                )}
                {!minimal && (
                  <>
                    <button
                      aria-label={`关闭 ${node.title}`}
                      onPointerDown={(e) => e.stopPropagation()}
                      onClick={() =>
                        setDocument({
                          ...docRef.current,
                          nodes: docRef.current.nodes.filter(
                            (n) => n.id !== node.id,
                          ),
                        })
                      }
                    >
                      <X size={12} />
                    </button>
                  </>
                )}
              </header>
              {minimal && nodeMenu === node.id && (
                <div
                  className="quiet-node-menu"
                  role="group"
                  aria-label={`${node.title} 内容操作`}
                >
                  {(node.kind === 'codex' || node.kind === 'claude') && (
                    <>
                      <button
                        onClick={() => {
                          send(`打开 ${node.kind} CLI`);
                          setNodeMenu(null);
                        }}
                      >
                        专注阅读
                      </button>
                      <button
                        onClick={() => {
                          run(node.kind as CliId, '/diff');
                          setNodeMenu(null);
                        }}
                      >
                        查看改动
                      </button>
                      <button
                        onClick={() => {
                          run(node.kind as CliId, '/stop');
                          setNodeMenu(null);
                        }}
                      >
                        中断会话
                      </button>
                      <button
                        onClick={() => {
                          run(node.kind as CliId, '/resume');
                          setNodeMenu(null);
                        }}
                      >
                        继续会话
                      </button>
                    </>
                  )}
                  <button
                    onClick={() => {
                      setDocument({
                        ...docRef.current,
                        nodes: docRef.current.nodes.filter(
                          (n) => n.id !== node.id,
                        ),
                      });
                      setNodeMenu(null);
                    }}
                  >
                    移出画布
                  </button>
                </div>
              )}
              <div className="node-content">
                {node.kind === 'codex' || node.kind === 'claude' ? (
                  <TerminalSession
                    state={state}
                    id={node.kind}
                    run={run}
                    compact
                  />
                ) : node.kind === 'diff' ? (
                  <CanvasDiff />
                ) : node.kind === 'note' ? (
                  <textarea
                    aria-label={node.title}
                    value={node.text ?? ''}
                    onChange={(e) =>
                      setDocument({
                        ...docRef.current,
                        nodes: docRef.current.nodes.map((n) =>
                          n.id === node.id ? { ...n, text: e.target.value } : n,
                        ),
                      })
                    }
                  />
                ) : node.kind === 'image' ? (
                  <img src="/environment.png" alt="宁静湖畔环境参考" />
                ) : (
                  <div className="canvas-preview">
                    <span>NORMA OS / LIVE NODE</span>
                    <h3>你的下一个想法</h3>
                    <p>
                      网页、视频、设计稿与运行结果，都可以成为画布上的一个节点。
                    </p>
                    <button onClick={() => send('Codex 帮我实现这个预览节点')}>
                      交给 Codex <CornerDownLeft size={13} />
                    </button>
                  </div>
                )}
              </div>
              <div
                className="node-resize"
                role="button"
                tabIndex={minimal && selected !== node.id ? -1 : 0}
                aria-hidden={minimal && selected !== node.id ? true : undefined}
                aria-label={`调整 ${node.title} 大小`}
                onKeyDown={(e) => {
                  if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
                    e.preventDefault();
                    const width = Math.max(
                      280,
                      Math.min(
                        1000,
                        node.w + (e.key === 'ArrowRight' ? 25 : -25),
                      ),
                    );
                    setDocument({
                      ...docRef.current,
                      nodes: docRef.current.nodes.map((n) =>
                        n.id === node.id ? { ...n, w: width } : n,
                      ),
                    });
                  }
                }}
                onPointerDown={(e) => start(e, 'resize', node)}
                onPointerMove={move}
                onPointerUp={end}
                onPointerCancel={cancel}
              />
            </article>
          ))}
        </div>
        <canvas
          ref={ink}
          className="canvas-ink"
          aria-label="画笔交互区域"
          style={{
            pointerEvents:
              tool === 'pen' || tool === 'eraser' ? 'auto' : 'none',
          }}
          onPointerDown={(e) =>
            start(e, spaceHeld ? 'pan' : tool === 'pen' ? 'draw' : 'erase')
          }
          onPointerMove={move}
          onPointerUp={end}
          onPointerCancel={cancel}
        />
        {!doc.nodes.length && (
          <div className="canvas-empty">
            画布是空的。添加一个 CLI、便签或图片开始。
          </div>
        )}
        <div className="canvas-map" aria-hidden="true">
          <div className="map-viewport" />
          {doc.nodes.map((n) => (
            <i
              key={n.id}
              style={{
                left: n.x / 20,
                top: n.y / 20,
                width: n.w / 20,
                height: n.h / 20,
              }}
            />
          ))}
        </div>
      </div>
      {minimal ? (
        <div
          className="quiet-canvas-ornament"
          role="group"
          aria-label="画布工具"
        >
          <button
            aria-label="选择"
            title="选择 · 拖动空白移动画布"
            aria-pressed={tool === 'select'}
            onClick={() => {
              setTool('select');
              setPenSettings(false);
            }}
          >
            <MousePointer2 size={19} />
          </button>
          <button
            aria-label="画笔"
            title="画笔"
            aria-pressed={tool === 'pen'}
            onClick={() => {
              setTool('pen');
              setShowAdd(false);
              setViewMenu(false);
            }}
          >
            <PenLine size={20} />
          </button>
          {controls.includes('add') && (
            <button
              aria-label="添加内容"
              title="添加内容"
              aria-expanded={showAdd}
              onClick={() => {
                setShowAdd(!showAdd);
                setViewMenu(false);
              }}
            >
              <Plus size={21} />
            </button>
          )}
          {controls.includes('eraser') && (
            <>
              <button
                aria-label="橡皮"
                aria-pressed={tool === 'eraser'}
                onClick={() => setTool('eraser')}
              >
                <Eraser size={19} />
              </button>
              <button
                aria-label="撤销笔迹"
                disabled={!doc.strokes.length}
                onClick={() => setDocument(undoStroke(doc))}
              >
                <Undo2 size={19} />
              </button>
              {doc.undone.length > 0 && (
                <button
                  aria-label="重做笔迹"
                  onClick={() => setDocument(redoStroke(doc))}
                >
                  <Redo2 size={19} />
                </button>
              )}
              <button
                aria-label="画笔颜色"
                onClick={() => setPenSettings(!penSettings)}
              >
                <i className="quiet-color" style={{ background: color }} />
              </button>
              {doc.strokes.length > 0 && (
                <button
                  className="quiet-annotation-send"
                  aria-label="标注交给 Norma"
                  onClick={() => {
                    send(annotationCommand(doc));
                    setTool('select');
                  }}
                >
                  <Send size={16} />
                  <span>交给 Norma</span>
                </button>
              )}
            </>
          )}
          <span className="quiet-tool-divider" />
          <button
            aria-label="画布视图选项"
            title="视图选项"
            aria-expanded={viewMenu}
            onClick={() => {
              setViewMenu(!viewMenu);
              setShowAdd(false);
            }}
          >
            <Ellipsis size={20} />
          </button>
          {viewMenu && (
            <div className="quiet-view-menu">
              <button
                onClick={() => {
                  fit();
                  setViewMenu(false);
                }}
              >
                适应全部内容
              </button>
              <button
                onClick={() =>
                  setView((v) => zoomAt(v, v.zoom + 0.1, { x: 300, y: 200 }))
                }
              >
                放大
              </button>
              <button
                onClick={() =>
                  setView((v) => zoomAt(v, v.zoom - 0.1, { x: 300, y: 200 }))
                }
              >
                缩小
              </button>
              <span>
                {Math.round(view.zoom * 100)}% · {doc.strokes.length} 处标注
              </span>
            </div>
          )}
          {penSettings && (
            <div className="quiet-pen-menu">
              {['#b5e7dc', '#f1c987', '#f2f5ff'].map((c) => (
                <button
                  key={c}
                  aria-label={`颜色 ${c}`}
                  onClick={() => {
                    setColor(c);
                    setPenSettings(false);
                  }}
                >
                  <i style={{ background: c }} />
                </button>
              ))}
            </div>
          )}
        </div>
      ) : (
        <div className="canvas-bottomline">
          <div className="canvas-tools" role="group" aria-label="画布工具">
            {(
              [
                ['select', '选择', MousePointer2],
                ['pan', '移动画布', Hand],
                ['pen', '画笔', PenLine],
                ['eraser', '橡皮', Eraser],
              ] as const
            ).map(([value, label, Icon]) => (
              <button
                key={value}
                aria-label={label}
                title={label}
                aria-pressed={tool === value}
                onClick={() => setTool(value)}
              >
                <Icon size={17} />
              </button>
            ))}
            <span className="tool-divider" />
            {['#b5e7dc', '#f1c987', '#f2f5ff'].map((value) => (
              <button
                className="pen-color"
                key={value}
                aria-label={
                  value === '#b5e7dc'
                    ? '绿色画笔'
                    : value === '#f1c987'
                      ? '橙色画笔'
                      : '白色画笔'
                }
                aria-pressed={color === value}
                onClick={() => {
                  setColor(value);
                  setTool('pen');
                }}
              >
                <i style={{ background: value }} />
              </button>
            ))}
            <span className="tool-divider" />
            <button
              aria-label="撤销笔迹"
              disabled={!doc.strokes.length}
              onClick={() => setDocument(undoStroke(doc))}
            >
              <Undo2 size={16} />
            </button>
            <button
              aria-label="重做笔迹"
              disabled={!doc.undone.length}
              onClick={() => setDocument(redoStroke(doc))}
            >
              <Redo2 size={16} />
            </button>
          </div>
          <div className="canvas-zoom">
            <button
              aria-label="缩小画布"
              onClick={() =>
                setView((v) => zoomAt(v, v.zoom - 0.1, { x: 300, y: 200 }))
              }
            >
              <Minus size={14} />
            </button>
            <span>{Math.round(view.zoom * 100)}%</span>
            <button
              aria-label="放大画布"
              onClick={() =>
                setView((v) => zoomAt(v, v.zoom + 0.1, { x: 300, y: 200 }))
              }
            >
              <Plus size={14} />
            </button>
            <button aria-label="适应全部内容" onClick={fit}>
              <Focus size={15} />
            </button>
          </div>
          <button
            className="send-annotation"
            disabled={!doc.strokes.length}
            onClick={() => send(annotationCommand(doc))}
          >
            <Send size={14} />
            <span>标注交给 Norma</span>
          </button>
        </div>
      )}
      {minimal && (
        <span className="quiet-canvas-state" aria-live="polite">
          {doc.nodes.length} 个内容 · {doc.strokes.length} 处标注
        </span>
      )}
    </div>
  );
}
