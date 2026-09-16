import type { State } from './model';
import { activitySnapshot } from './spatial-activity';
export type DrawingTools = {
  rect: (
    x: number,
    y: number,
    w: number,
    h: number,
    fill: string,
    r?: number,
  ) => void;
  text: (
    s: string,
    x: number,
    y: number,
    size?: number,
    color?: string,
    weight?: number,
  ) => void;
  button: (
    s: string,
    x: number,
    y: number,
    w: number,
    action: string,
    selected?: boolean,
    h?: number,
  ) => void;
};
// Render content directly into the XR texture: DOM previews are not visible in an immersive session.
export function drawSpatialArtifact(
  c: CanvasRenderingContext2D,
  state: State,
  p: DrawingTools,
  environment?: HTMLImageElement,
) {
  const { rect, text, button } = p;
  if (state.view === 'research') {
    text('CLAUDE / REVIEW', 80, 150, 21, '#dfc3b0');
    text('让输入，始终到达正确的会话。', 80, 215, 37, '#eff5f7', 500);
    const rows = [
      ['01', '焦点归属', '聚焦终端后，按键只发送给当前节点。'],
      ['02', '随时接管', 'Escape 退出焦点，会话与已有输出继续保留。'],
      ['03', '验证边界', '覆盖关闭非焦点节点的负断言与暂停恢复。'],
    ];
    rows.forEach(([n, title, body], i) => {
      const y = 299 + i * 108;
      text(n, 82, y, 24, '#aabfcc');
      text(title, 141, y, 27);
      text(body, 141, y + 38, 23, '#bed0dc');
    });
    text('演示报告 · 当前任务上下文已保留', 80, 645, 20, '#aacabd');
    button('查看改动', 825, 614, 276, 'view:code', false, 58);
  } else if (state.view === 'data') {
    text('团队执行概览', 80, 158, 33, '#edf5f7', 500);
    text('MOCK DATA', 900, 157, 20, '#a6bfcc');
    const total = activitySnapshot(state).progress;
    text(`${total}%`, 80, 272, 72, '#d5ebe5', 500);
    text('总体进度', 365, 269, 24, '#b6cbd6');
    state.agents.forEach((a, i) => {
      const y = 365 + i * 96;
      text(a.name, 82, y, 27);
      rect(310, y - 20, 620, 17, '#8caaba25', 9);
      if (a.progress)
        rect(310, y - 20, (620 * a.progress) / 100, 17, a.color, 9);
      text(`${a.progress}%`, 979, y, 25);
      text(
        a.status === 'paused'
          ? '已暂停'
          : a.status === 'waiting'
            ? '等你决策'
            : a.status === 'working'
              ? '执行中'
              : a.status === 'done'
                ? '已完成'
                : '准备就绪',
        82,
        y + 30,
        18,
        '#aabfcd',
      );
    });
    text('点名暂停某个 Agent，其余工作继续。', 80, 655, 23, '#a6c3cf');
  } else if (state.view === 'preview') {
    text('norma.local / preview', 80, 147, 23, '#bed1db');
    text('网页与图片 · 演示', 899, 148, 20, '#9bb6c6');
    if (environment) {
      c.save();
      c.beginPath();
      c.roundRect(680, 190, 422, 360, 22);
      c.clip();
      c.drawImage(environment, 680, 190, 422, 360);
      c.restore();
    }
    text('把一个想法，', 82, 284, 45, '#edf5f5', 500);
    text('变成可运行的内容。', 82, 345, 45, '#edf5f5', 500);
    text('终端、网页、设计稿和执行结果，', 84, 409, 25, '#c4d5df');
    text('都可以留在同一个工作空间。', 84, 451, 25, '#c4d5df');
    rect(81, 500, 500, 75, '#bedfd019', 18);
    text('Terminal Node', 103, 531, 25, '#c1dfd2');
    text('Live · Escape 返回画布', 103, 559, 18, '#aec8c9');
    button(
      '交给 Codex 继续实现',
      80,
      615,
      390,
      'command:Codex 实现预览节点',
      false,
      58,
    );
    text('静态预览 · 未连接外部网站', 738, 614, 20, '#b8cbd7');
  } else if (state.view === 'model') {
    text('三维对象', 80, 157, 34);
    text('捏住模型，直接旋转。', 80, 202, 25, '#b9cdd8');
    text('SPATIAL OBJECT / 01', 80, 655, 19, '#a7c0ce');
  }
}
