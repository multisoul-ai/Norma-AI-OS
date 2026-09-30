export type CanvasKind =
  | 'codex'
  | 'claude'
  | 'diff'
  | 'note'
  | 'image'
  | 'preview';
export type CanvasNode = {
  id: string;
  kind: CanvasKind;
  title: string;
  x: number;
  y: number;
  w: number;
  h: number;
  text?: string;
};
export type Point = { x: number; y: number; pressure: number };
export type Stroke = {
  id: string;
  color: string;
  width: number;
  points: Point[];
};
export type Viewport = { x: number; y: number; zoom: number };
export type CanvasTool = 'select' | 'pen' | 'eraser' | 'pan';
export type CanvasDocument = {
  nodes: CanvasNode[];
  strokes: Stroke[];
  undone: Stroke[];
};
export function initialCanvas(): CanvasDocument {
  return {
    nodes: [
      {
        id: 'codex-1',
        kind: 'codex',
        title: 'Codex · 实现',
        x: 45,
        y: 25,
        w: 500,
        h: 350,
      },
      {
        id: 'claude-1',
        kind: 'claude',
        title: 'Claude · 审查',
        x: 580,
        y: 45,
        w: 460,
        h: 350,
      },
      {
        id: 'note-1',
        kind: 'note',
        title: '我的想法',
        x: 60,
        y: 410,
        w: 355,
        h: 210,
        text: '先让 Codex 改输入路由，\n再请 Claude 检查边界。\n\n可以用画笔圈出想调整的地方 →',
      },
      {
        id: 'diff-1',
        kind: 'diff',
        title: 'route.rs · 演示 diff',
        x: 460,
        y: 435,
        w: 500,
        h: 265,
      },
    ],
    strokes: [],
    undone: [],
  };
}
export function toWorld(point: { x: number; y: number }, viewport: Viewport) {
  return {
    x: (point.x - viewport.x) / viewport.zoom,
    y: (point.y - viewport.y) / viewport.zoom,
  };
}
export function zoomAt(
  view: Viewport,
  scale: number,
  anchor: { x: number; y: number },
): Viewport {
  const world = toWorld(anchor, view);
  const zoom = Math.max(0.35, Math.min(1.6, scale));
  return { zoom, x: anchor.x - world.x * zoom, y: anchor.y - world.y * zoom };
}
export function appendStroke(
  doc: CanvasDocument,
  stroke: Stroke,
): CanvasDocument {
  return stroke.points.length < 2
    ? doc
    : { ...doc, strokes: [...doc.strokes, stroke], undone: [] };
}
export function undoStroke(doc: CanvasDocument): CanvasDocument {
  const last = doc.strokes.at(-1);
  return !last
    ? doc
    : {
        ...doc,
        strokes: doc.strokes.slice(0, -1),
        undone: [...doc.undone, last],
      };
}
export function redoStroke(doc: CanvasDocument): CanvasDocument {
  const last = doc.undone.at(-1);
  return !last
    ? doc
    : {
        ...doc,
        strokes: [...doc.strokes, last],
        undone: doc.undone.slice(0, -1),
      };
}
export function strokeHits(
  stroke: Stroke,
  p: { x: number; y: number },
  radius = 18,
): boolean {
  return stroke.points.some((a, i) => {
    const b = stroke.points[i + 1] ?? a;
    const dx = b.x - a.x,
      dy = b.y - a.y;
    const t = Math.max(
      0,
      Math.min(
        1,
        ((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy || 1),
      ),
    );
    return (
      Math.hypot(p.x - a.x - t * dx, p.y - a.y - t * dy) <=
      radius + stroke.width
    );
  });
}
export function annotatedNodes(doc: CanvasDocument): CanvasNode[] {
  return doc.nodes.filter((node) =>
    doc.strokes.some((stroke) => {
      const xs = stroke.points.map((p) => p.x),
        ys = stroke.points.map((p) => p.y);
      return (
        Math.max(...xs) >= node.x &&
        Math.min(...xs) <= node.x + node.w &&
        Math.max(...ys) >= node.y &&
        Math.min(...ys) <= node.y + node.h
      );
    }),
  );
}
export function annotationCommand(doc: CanvasDocument) {
  const nodes = annotatedNodes(doc);
  return `请按照自由画布上的 ${doc.strokes.length} 处画笔标注调整${nodes.length ? '，关联内容：' + nodes.map((n) => n.title).join('、') : '当前任务'}。先解释你的修改计划。`;
}
