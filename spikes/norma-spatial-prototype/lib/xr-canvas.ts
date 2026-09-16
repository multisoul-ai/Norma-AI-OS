import { type CanvasDocument, type Point, toWorld } from './free-canvas.ts';
export const spatialCanvasBounds = { x: 25, y: 82, w: 1150, h: 642 };
export function spatialCanvasViewport(doc: CanvasDocument) {
  const nodes = doc.nodes;
  const minX = Math.min(0, ...nodes.map((n) => n.x)),
    minY = Math.min(0, ...nodes.map((n) => n.y));
  const right = Math.max(1080, ...nodes.map((n) => n.x + n.w)),
    bottom = Math.max(700, ...nodes.map((n) => n.y + n.h));
  const zoom = Math.min(0.86, 1100 / (right - minX), 592 / (bottom - minY));
  return {
    zoom,
    x: (1200 - (right - minX) * zoom) / 2 - minX * zoom,
    y: 98 - minY * zoom,
  };
}
export function spatialCanvasPoint(
  doc: CanvasDocument,
  pixel: { x: number; y: number },
): Point {
  return { ...toWorld(pixel, spatialCanvasViewport(doc)), pressure: 0.5 };
}
