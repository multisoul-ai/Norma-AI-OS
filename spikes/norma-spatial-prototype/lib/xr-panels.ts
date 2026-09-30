import { type State } from './model';
import { visibleTranscript, developmentGoal } from './cli';
import { spatialCanvasViewport, spatialCanvasBounds } from './xr-canvas';
import { spatialControls } from './interaction-chrome';
import type { Stroke, CanvasTool } from './free-canvas';
import {
  spaceMenu,
  contentMenu,
  conversationSuggestions,
  type SpatialMenu,
  type SpatialVoice,
} from './spatial-workflow';
import { drawSpatialArtifact } from './xr-content';
import { drawDecisionList, drawDecisionDetail } from './xr-decisions';
import { decisionSnapshot } from './decisions';
import { drawActivity } from './xr-activity';
import { drawGlassSurface } from './xr-glass';
export type Region = {
  x: number;
  y: number;
  w: number;
  h: number;
  action: string;
};
export type PanelDrawing = {
  canvas: HTMLCanvasElement;
  glass: HTMLCanvasElement;
  regions: Region[];
  kind:
    | 'main'
    | 'team'
    | 'control'
    | 'activity'
    | 'decisions'
    | 'decision-detail';
};
export function hitRegion(regions: Region[], x: number, y: number) {
  return regions.findLast(
    (r) => x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h,
  );
}
export function createDrawing(kind: PanelDrawing['kind']): PanelDrawing {
  const canvas = document.createElement('canvas');
  canvas.width = kind === 'main' ? 1200 : 580;
  canvas.height = kind === 'main' ? 940 : kind === 'activity' ? 760 : 840;
  const glass = document.createElement('canvas');
  glass.width = canvas.width;
  glass.height = canvas.height;
  return { canvas, glass, regions: [], kind };
}
type Options = {
  hover?: string;
  pinned: boolean;
  scale: number;
  depth: number;
  notice: string;
  environment?: HTMLImageElement;
  listening: boolean;
  focus: boolean;
  canvasTool: CanvasTool;
  draft?: Stroke;
  menu: SpatialMenu;
  voiceState: SpatialVoice;
  pressed?: string;
  selectedNode?: string;
  selectedDecision?: string;
  decisionPage: number;
};
export function drawPanel(panel: PanelDrawing, state: State, o: Options) {
  const { canvas, kind } = panel,
    c = canvas.getContext('2d')!,
    w = canvas.width;
  panel.regions = [];
  c.clearRect(0, 0, w, canvas.height);
  const mask = panel.glass.getContext('2d')!;
  mask.clearRect(0, 0, w, canvas.height);
  const glass = (x: number, y: number, width: number, height: number, r = 30) =>
    drawGlassSurface(c, mask, x, y, width, height, r);
  const rect = (
    x: number,
    y: number,
    width: number,
    height: number,
    fill: string,
    r = 20,
  ) => {
    c.fillStyle = fill;
    c.beginPath();
    c.roundRect(x, y, width, height, r);
    c.fill();
  };
  const text = (
    s: string,
    x: number,
    y: number,
    size = 25,
    color = '#edf3f6',
    weight = 400,
  ) => {
    c.font = `${weight} ${size}px -apple-system,BlinkMacSystemFont,"PingFang SC",sans-serif`;
    c.fillStyle = color;
    c.fillText(s, x, y);
  };
  const button = (
    label: string,
    x: number,
    y: number,
    width: number,
    action: string,
    selected = false,
    height = 60,
  ) => {
    rect(
      x,
      y,
      width,
      height,
      o.pressed === action
        ? '#e0ebf37c'
        : o.hover === action
          ? '#e0ebf344'
          : selected
            ? '#e3edf0'
            : '#ffffff08',
      height / 2,
    );
    text(
      label,
      x + width / 2,
      y + height / 2 + 8,
      23,
      selected ? '#344e5f' : '#edf3f6',
    );
    c.textAlign = 'left';
    panel.regions.push({ x, y, w: width, h: height, action });
  };
  const centeredButton = (
    label: string,
    x: number,
    y: number,
    width: number,
    action: string,
    selected = false,
    height = 60,
  ) => {
    c.textAlign = 'center';
    button(label, x, y, width, action, selected, height);
  };
  if (kind === 'decisions') {
    glass(1, 1, w - 2, decisionSnapshot(state).items.length ? 838 : 218, 30);
    panel.regions.push({ x: 0, y: 0, w, h: 85, action: 'drag' });
    drawDecisionList(
      c,
      state,
      { rect, text, button: centeredButton },
      (region) => panel.regions.push(region),
      o.decisionPage,
      o.selectedDecision,
      o.hover,
      o.pressed,
    );
    return;
  }
  if (kind === 'decision-detail') {
    glass(1, 1, w - 2, 838, 35);
    panel.regions.push({ x: 0, y: 0, w: 470, h: 85, action: 'drag' });
    drawDecisionDetail(
      c,
      state,
      { rect, text, button: centeredButton },
      o.selectedDecision,
    );
    return;
  }
  if (kind === 'activity') {
    glass(1, 1, 578, 758, 30);
    drawActivity(
      c,
      state,
      { rect, text, button: centeredButton },
      (r) => panel.regions.push(r),
      o.hover,
      o.pressed,
    );
    return;
  }
  if (kind !== 'main') {
    glass(1, 1, w - 2, 838, 35);
    text(kind === 'team' ? '你的团队' : 'Norma', 35, 59, 30, '#f2f7f9', 500);
    centeredButton(
      '×',
      w - 90,
      22,
      55,
      kind === 'team' ? 'team' : 'dialogue',
      false,
      55,
    );
    if (kind === 'team') {
      state.agents.forEach((a, i) => {
        const y = 155 + i * 175;
        text(a.name, 40, y, 34, a.color, 500);
        text(
          a.status === 'working'
            ? `协作中 · ${a.progress}%`
            : a.status === 'paused'
              ? '已暂停'
              : a.status === 'waiting'
                ? '等你决策'
                : a.status === 'done'
                  ? '已完成'
                  : '准备就绪',
          40,
          y + 46,
          24,
          '#d4e2ec',
        );
        if (a.status === 'working' || a.status === 'paused')
          centeredButton(
            a.status === 'paused' ? '继续' : '暂停',
            380,
            y - 12,
            150,
            `command:${a.status === 'paused' ? '继续' : '暂停'} ${a.name}`,
          );
      });
      centeredButton('开始协作', 40, 724, w - 80, `command:${developmentGoal}`);
    } else {
      const voice = o.voiceState;
      const wrapped = (
        value: string,
        y: number,
        color = '#d3e1e8',
        limit = 3,
      ) => {
        const lines = value.match(/.{1,20}/gu) || [];
        lines
          .slice(0, limit)
          .forEach((line, i) =>
            text(
              line + (i === limit - 1 && lines.length > limit ? '…' : ''),
              35,
              y + i * 34,
              24,
              color,
            ),
          );
      };
      text(
        voice.speaking
          ? '正在回复'
          : voice.listening
            ? '正在听你说话'
            : '随时说出你的想法',
        35,
        112,
        23,
        '#b7d5d2',
      );
      const user = state.messages.findLast((m) => m.role === 'user');
      wrapped(
        voice.transcript || user?.text || '“Codex 实现，Claude 独立审查。”',
        166,
        '#eff6f6',
        2,
      );
      wrapped(
        voice.error || state.messages.at(-1)?.text || '',
        269,
        '#c2d5df',
        4,
      );
      centeredButton(
        voice.speaking
          ? '打断回复，继续说'
          : voice.listening
            ? '结束语音对话'
            : '开始语音对话',
        35,
        412,
        w - 70,
        'voice:toggle',
        voice.listening,
        65,
      );
      centeredButton(
        voice.readAloud ? '语音回复 · 开' : '语音回复 · 关',
        35,
        493,
        w - 70,
        'voice:output',
        voice.readAloud,
        50,
      );
      text('演示指令 · 点击模拟说出一句', 35, 581, 19, '#a8c2d0');
      conversationSuggestions(state).forEach((suggestion, i) =>
        centeredButton(
          suggestion.label,
          35,
          603 + i * 64,
          w - 70,
          `say:${suggestion.text}`,
          false,
          56,
        ),
      );
    }
    return;
  }
  // Content lives on a world-space glass canvas, with one detached ornament.
  text('Norma-AI-OS', 52, 51, 27, '#edf2f5', 500);
  text(
    state.view === 'canvas' ? '自由画布' : '当前内容',
    w - 155,
    51,
    21,
    '#d9e3e9',
  );
  panel.regions.push({ x: 0, y: 0, w, h: 78, action: 'drag' });
  if (state.view === 'canvas') {
    const b = spatialCanvasBounds,
      v = spatialCanvasViewport(state.canvas);
    panel.regions.push({
      ...b,
      action: o.canvasTool === 'select' ? '' : `canvas:${o.canvasTool}`,
    });
    glass(b.x, b.y, b.w, b.h, 32);
    c.save();
    c.beginPath();
    c.roundRect(b.x, b.y, b.w, b.h, 32);
    c.clip();
    c.translate(v.x, v.y);
    c.scale(v.zoom, v.zoom);
    for (const n of state.canvas.nodes) {
      c.shadowColor = '#101f3048';
      c.shadowBlur = 30;
      c.shadowOffsetY = 12;
      rect(
        n.x,
        n.y,
        n.w,
        n.h,
        n.kind === 'note' ? '#e8e7d9dc' : '#15233042',
        22,
      );
      c.shadowBlur = 0;
      c.shadowOffsetY = 0;
      c.strokeStyle = o.selectedNode === n.id ? '#e8f2f9a0' : '#dfedf33f';
      c.lineWidth = 1.5;
      c.beginPath();
      c.roundRect(n.x, n.y, n.w, n.h, 22);
      c.stroke();
      text(
        n.title,
        n.x + 22,
        n.y + 36,
        23,
        n.kind === 'note' ? '#5b6658' : '#e4edf2',
        500,
      );
      c.save();
      c.beginPath();
      c.rect(n.x + 12, n.y + 54, n.w - 24, n.h - 68);
      c.clip();
      if (n.kind === 'codex' || n.kind === 'claude')
        visibleTranscript(state, n.kind)
          .slice(-9)
          .forEach((line, i) =>
            text(
              line.text,
              n.x + 22,
              n.y + 81 + i * 28,
              20,
              line.kind === 'success'
                ? '#baddc3'
                : line.kind === 'warning'
                  ? '#e2cba8'
                  : line.kind === 'muted'
                    ? '#b5c7d2'
                    : '#dce7ed',
            ),
          );
      else if (n.kind === 'note')
        (n.text ?? '')
          .split('\n')
          .forEach((line, i) =>
            text(line, n.x + 22, n.y + 83 + i * 34, 23, '#596659'),
          );
      else if (n.kind === 'image' && o.environment)
        c.drawImage(o.environment, n.x, n.y + 55, n.w, n.h - 55);
      else if (n.kind === 'diff')
        [
          'crates/runtime/src/route.rs',
          '',
          '+ Key → SendInputToNode',
          '+ Escape → ExitNodeFocus',
          '',
          '演示 diff · 等待审阅',
        ].forEach((line, i) =>
          text(
            line,
            n.x + 22,
            n.y + 79 + i * 29,
            21,
            line.startsWith('+') ? '#b6d8bd' : '#bfd0db',
          ),
        );
      else {
        text('NORMA / LIVE NODE', n.x + 22, n.y + 100, 24);
        text('运行结果与新的想法', n.x + 22, n.y + 147, 22, '#c4d4de');
      }
      c.restore();
      if (o.canvasTool === 'select') {
        panel.regions.push({
          x: v.x + n.x * v.zoom,
          y: v.y + n.y * v.zoom,
          w: n.w * v.zoom,
          h: n.h * v.zoom,
          action: `node:${n.id}`,
        });
        if (
          o.selectedNode === n.id &&
          (n.kind === 'codex' || n.kind === 'claude')
        ) {
          rect(n.x + n.w - 125, n.y + 8, 114, 36, '#ecf3f529', 18);
          text('打开会话', n.x + n.w - 112, n.y + 33, 20);
          panel.regions.push({
            x: v.x + (n.x + n.w - 125) * v.zoom,
            y: v.y + (n.y + 8) * v.zoom,
            w: 114 * v.zoom,
            h: 36 * v.zoom,
            action: `command:打开 ${n.kind} CLI`,
          });
        }
      }
    }
    for (const s of [...state.canvas.strokes, ...(o.draft ? [o.draft] : [])]) {
      c.strokeStyle = s.color;
      c.lineWidth = s.width;
      c.lineCap = 'round';
      c.lineJoin = 'round';
      c.beginPath();
      s.points.forEach((p, i) => (i ? c.lineTo(p.x, p.y) : c.moveTo(p.x, p.y)));
      c.stroke();
    }
    c.restore();
  } else {
    glass(45, 100, 1110, 607, 28);
    if (state.view === 'terminal') {
      text(
        state.activeCli === 'codex' ? 'Codex' : 'Claude',
        75,
        153,
        30,
        '#e8f1f5',
        500,
      );
      text('CLI · 模拟会话', 230, 153, 22, '#beced9');
      const active = state.agents.find((a) => a.id === state.activeCli)!;
      centeredButton(
        state.activeCli === 'codex' ? 'Claude CLI' : 'Codex CLI',
        625,
        111,
        205,
        `command:打开 ${state.activeCli === 'codex' ? 'Claude' : 'Codex'} CLI`,
        false,
        55,
      );
      centeredButton(
        active.status === 'working'
          ? '暂停'
          : active.status === 'paused'
            ? '继续'
            : '查看改动',
        850,
        111,
        250,
        active.status === 'working'
          ? `command:暂停 ${active.name}`
          : active.status === 'paused'
            ? `command:继续 ${active.name}`
            : 'view:code',
        false,
        55,
      );
      c.save();
      c.beginPath();
      c.rect(75, 188, 1050, 467);
      c.clip();
      visibleTranscript(state, state.activeCli)
        .slice(-13)
        .forEach((l, i) =>
          text(
            l.text,
            76,
            218 + i * 34,
            24,
            l.kind === 'success' ? '#c5ebcf' : '#e2edf3',
          ),
        );
      c.restore();
    } else if (state.view === 'code') {
      text('route.rs · 演示改动', 78, 158, 30);
      [
        '@@ 输入焦点路由 @@',
        '',
        '+ InputEvent::Key => SendInputToNode',
        '+ InputEvent::Escape => ExitNodeFocus',
        '',
        '保留会话，退出输入焦点。',
      ].forEach((l, i) =>
        text(
          l,
          78,
          236 + i * 50,
          27,
          l.startsWith('+') ? '#bbdfc4' : '#d0e0e8',
        ),
      );
      centeredButton(
        '让 Claude 审查',
        80,
        617,
        320,
        'command:让 Claude 审查 Codex 的改动',
        false,
        57,
      );
      if (state.agents.every((a) => a.status === 'done'))
        centeredButton(
          state.approved ? '已确认' : '确认演示改动',
          750,
          617,
          350,
          'command:确认改动',
          state.approved,
          57,
        );
    } else
      drawSpatialArtifact(
        c,
        state,
        { rect, text, button: centeredButton },
        o.environment,
      );
  }

  // Four controls in the resting state. Layout actions only exist while the menu is open.
  const controls = spatialControls(false, o.canvasTool),
    labels: Record<string, string> = {
      select: state.view === 'canvas' ? '选择' : '画布',
      pen: '画笔',
      voice: o.listening ? '聆听中' : '对话',
      more: '···',
      eraser: '橡皮',
      undo: '撤销',
      send: '交给 Norma',
    };
  glass(357, 744, 486, 75, 38);
  controls.forEach((control, i) =>
    centeredButton(
      labels[control],
      365 + i * 118,
      751,
      112,
      control === 'select'
        ? state.view !== 'canvas'
          ? 'view:canvas'
          : 'tool:select'
        : control === 'pen' || control === 'eraser'
          ? `tool:${control}`
          : control === 'undo' || control === 'send'
            ? `canvas:${control}`
            : control,
      (control === 'select' && o.canvasTool === 'select') ||
        (control === 'pen' && o.canvasTool === 'pen'),
      60,
    ),
  );
  // Spoken feedback stays in the rendered XR scene, so it remains visible inside a headset.
  c.textAlign = 'center';
  text(o.listening ? 'Norma · 正在听' : 'Norma', 600, 864, 20, '#cee4e6', 500);
  const caption = o.listening
    ? o.voiceState.transcript || '说出你的目标，或随时打断当前任务。'
    : o.notice ||
      (state.messages.length > 1
        ? state.messages.at(-1)?.text
        : '看向内容，捏合选择；捏住并移动，重新摆放。') ||
      '';
  const chunks = caption.match(/.{1,36}/gu) || [];
  chunks
    .slice(0, 2)
    .forEach((line, i) =>
      text(
        line + (i === 1 && chunks.length > 2 ? '…' : ''),
        600,
        894 + i * 28,
        21,
        '#dce8ec',
      ),
    );
  c.textAlign = 'left';
  if (o.menu) {
    const menu = o.menu;
    const items =
      menu === 'space'
        ? spaceMenu
        : menu === 'content'
          ? contentMenu
          : spatialControls(true, o.canvasTool)
              .filter((a) => !['team', 'exit'].includes(a))
              .map((action) => ({
                action,
                label: (
                  {
                    pin: o.pinned ? '解除固定' : '固定画布',
                    smaller: '缩小',
                    larger: '放大',
                    nearer: '拉近',
                    farther: '推远',
                    recenter: '重新居中',
                  } as Record<string, string>
                )[action],
              }));
    const y = menu === 'content' ? 297 : 353;
    c.save();
    c.beginPath();
    c.roundRect(573, y, 582, 730 - y, 27);
    c.clip();
    c.clearRect(573, y, 582, 730 - y);
    c.restore();
    glass(573, y, 582, 730 - y, 27);
    rect(574, y + 1, 580, 728 - y, '#1d2c3b66', 26);
    text(
      menu === 'space'
        ? '空间'
        : menu === 'content'
          ? '呈现内容'
          : '摆放与大小',
      605,
      y + 43,
      26,
      '#f0f5f8',
      500,
    );
    centeredButton('×', 1072, y + 12, 59, 'menu:close', false, 50);
    items.forEach((item, i) =>
      centeredButton(
        item.label,
        593 + (i % 2) * 274,
        y + 75 + Math.floor(i / 2) * 81,
        262,
        item.action,
        false,
        68,
      ),
    );
  }
}
