import type { State } from './model';
import type { DrawingTools } from './xr-content';
import type { Region } from './xr-panels';
import { decisionSnapshot, decisionWindow } from './decisions';

export function drawDecisionList(
  c: CanvasRenderingContext2D,
  state: State,
  p: DrawingTools,
  region: (region: Region) => void,
  page: number,
  selected?: string,
  hover?: string,
  pressed?: string,
) {
  const { rect, text, button } = p;
  const queue = decisionWindow(state, page);
  text('待你决策', 34, 60, 32, '#f2f5f7', 500);
  if (!queue.total) {
    text('现在无需处理', 34, 123, 27, '#d4e8df', 500);
    text(
      `已处理 ${queue.handled} 项 · 需要时会在这里提醒`,
      34,
      170,
      21,
      '#c7d7e1',
    );
    return;
  }
  rect(472, 26, 72, 44, '#f4d2a62b', 22);
  text(String(queue.total).padStart(2, '0'), 492, 57, 25, '#ffe0b5', 500);
  text(`${queue.total} 项待办 · 选择一项处理`, 34, 111, 22, '#c7d7e1');
  queue.items.forEach((item, i) => {
    const y = 157 + i * 130;
    const action = `decision:open:${item.id}`;
    if (selected === item.id || hover === action || pressed === action) {
      rect(
        18,
        y - 10,
        544,
        121,
        pressed === action
          ? '#ffffff35'
          : selected === item.id
            ? '#dfecf329'
            : '#ffffff16',
        18,
      );
    }
    text(item.task, 34, y + 22, 29, '#eff4f6', 500);
    text(item.summary, 34, y + 61, 24, '#e3edf2');
    text(
      `${item.owner} · ${item.id === 'review' ? '等待验收' : '等待决定'}`,
      34,
      y + 96,
      20,
      '#e6c9a5',
    );
    text('›', 523, y + 56, 32, '#d8e4ed');
    if (i < queue.items.length - 1) rect(34, y + 120, 510, 1, '#d9e6ee24', 0);
    region({ x: 18, y: y - 10, w: 544, h: 121, action });
  });
  const resumed = state.decisionTasks.findLast(
    (task) => task.status !== 'waiting',
  );
  if (resumed)
    text(
      `${resumed.title} · ${resumed.status === 'done' ? '已完成' : resumed.status === 'paused' ? '已暂停' : `执行中 ${resumed.progress}%`}`,
      34,
      705,
      20,
      '#bddbce',
    );
  if (queue.pages > 1) {
    if (queue.page > 0)
      button('‹', 25, 743, 88, 'decision:page:previous', false, 59);
    text(`${queue.page + 1} / ${queue.pages}`, 260, 782, 24, '#c7d7e1');
    if (queue.page < queue.pages - 1)
      button('下一页 ›', 372, 743, 180, 'decision:page:next', false, 59);
  } else
    text(
      queue.handled
        ? `已处理 ${queue.handled} 项`
        : '分别处理，其他任务继续保留',
      34,
      781,
      21,
      '#c7d7e1',
    );
}

export function drawDecisionDetail(
  c: CanvasRenderingContext2D,
  state: State,
  p: DrawingTools,
  selected?: string,
) {
  const { text, button } = p;
  const item = decisionSnapshot(state).items.find(
    (item) => item.id === selected,
  );
  if (!item) return;
  text('任务决策', 34, 60, 30, '#f2f5f7', 500);
  button('×', 486, 22, 56, 'decision:close', false, 56);
  text(`${item.task} · ${item.owner}`, 34, 124, 23, '#f0d1ab', 500);
  const wrapped = (
    value: string,
    y: number,
    size: number,
    color: string,
    weight = 400,
  ) => {
    c.font = `${weight} ${size}px -apple-system,BlinkMacSystemFont,"PingFang SC",sans-serif`;
    const lines: string[] = [];
    let line = '';
    for (const char of value) {
      if (c.measureText(line + char).width > 502) {
        lines.push(line);
        line = '';
      }
      line += char;
    }
    if (line) lines.push(line);
    lines.forEach((value, i) =>
      text(value, 34, y + i * (size + 14), size, color, weight),
    );
  };
  wrapped(item.title, 187, 34, '#f4f7f9', 500);
  wrapped(item.reason, 280, 25, '#d1dee7');
  text('NORMA 建议', 34, 393, 19, '#c4d8e2', 500);
  wrapped(item.recommendation, 435, 24, '#e1eaf0');
  button('查看相关内容 ↗', 25, 503, 525, item.detail, false, 55);
  button(item.primary.label, 25, 582, 530, item.primary.action, true, 68);
  button(item.secondary.label, 25, 665, 530, item.secondary.action, false, 58);
  wrapped(item.impact, 778, 20, '#bfcfda');
}
