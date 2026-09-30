import type { State } from './model';
import type { DrawingTools } from './xr-content';
import { activitySnapshot } from './spatial-activity';
export function drawActivity(
  c: CanvasRenderingContext2D,
  state: State,
  p: DrawingTools,
  region: (r: {
    x: number;
    y: number;
    w: number;
    h: number;
    action: string;
  }) => void,
  hover?: string,
  pressed?: string,
) {
  const { rect, text, button } = p;
  const activity = activitySnapshot(state);
  text('任务与 Agent', 32, 57, 29, '#e5eff3', 500);
  text('模拟执行', 426, 56, 18, '#a6bdcb');
  region({ x: 0, y: 0, w: 580, h: 85, action: 'drag' });
  c.font = '22px -apple-system,BlinkMacSystemFont,"PingFang SC",sans-serif';
  const lines: string[] = [];
  let line = '';
  for (const char of activity.mission) {
    if (c.measureText(line + char).width > 516) {
      lines.push(line);
      line = '';
    }
    line += char;
  }
  if (line) lines.push(line);
  lines
    .slice(0, 2)
    .forEach((value, i) =>
      text(
        value + (i === 1 && lines.length > 2 ? '…' : ''),
        32,
        105 + i * 29,
        22,
        '#d1dee7',
      ),
    );
  text(`${activity.progress}%`, 32, 184, 50, '#d5e8e1', 500);
  text(
    activity.running
      ? `${activity.running} 位正在执行`
      : activity.paused
        ? `${activity.paused} 位已暂停`
        : activity.waiting
          ? `${activity.waiting} 项等你决策`
          : activity.progress === 100
            ? '工作已完成'
            : '团队已就绪',
    205,
    179,
    23,
    '#bcd1dc',
  );
  rect(32, 207, 516, 7, '#b2c9d725', 4);
  if (activity.progress)
    rect(32, 207, (516 * activity.progress) / 100, 7, '#b8d8ca', 4);
  activity.rows.forEach((a, i) => {
    const y = 245 + i * 137;
    if (hover === a.action || pressed === a.action)
      rect(
        16,
        y - 11,
        548,
        127,
        pressed === a.action ? '#ddebf33d' : '#ddebf31c',
        17,
      );
    rect(32, y + 4, 8, 8, a.color, 4);
    text(a.name, 59, y + 22, 33, '#ecf3f5', 500);
    text(`${a.progress}%`, 439, y + 22, 30, a.color, 500);
    text(a.status, 59, y + 56, 21, a.color);
    text(
      a.detail.replace(/^[•●✓ ]+/, '').slice(0, 34),
      32,
      y + 97,
      21,
      '#c5d7e2',
    );
    region({ x: 16, y: y - 11, w: 548, h: 127, action: a.action });
  });
  if (!activity.running && !activity.paused && activity.waiting) {
    text('前往右侧，决定下一步', 78, 715, 23, '#edd1ac', 500);
    return;
  }
  button(
    activity.running
      ? '暂停全部'
      : activity.paused
        ? '继续任务'
        : activity.progress === 100
          ? '审阅成果'
          : '开始协作',
    32,
    682,
    516,
    activity.running
      ? 'command:全部暂停'
      : activity.paused
        ? 'command:全部继续'
        : activity.progress === 100
          ? 'view:code'
          : 'say:Codex 检查终端焦点路由，Claude 审查改动并补齐测试',
    false,
    52,
  );
}
