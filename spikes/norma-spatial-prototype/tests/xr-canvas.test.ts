import { test } from 'node:test';
import assert from 'node:assert/strict';
import { initialCanvas } from '../lib/free-canvas.ts';
import {
  spatialCanvasViewport,
  spatialCanvasPoint,
  spatialCanvasBounds,
} from '../lib/xr-canvas.ts';
// 场景：VR 射线与平面画笔落在相同内容。构造 Codex 节点位置，换算纹理坐标再反算，预期无偏移且在画布范围内。
test('VR ray pixels map back onto the shared canvas document', () => {
  const doc = initialCanvas(),
    v = spatialCanvasViewport(doc),
    p = { x: 100, y: 130 };
  const pixel = { x: v.x + p.x * v.zoom, y: v.y + p.y * v.zoom };
  const result = spatialCanvasPoint(doc, pixel);
  assert.ok(Math.abs(result.x - p.x) < 0.0001, 'VR 输入的横坐标保持一致');
  assert.ok(Math.abs(result.y - p.y) < 0.0001, 'VR 输入的纵坐标保持一致');
  assert.ok(
    pixel.y > spatialCanvasBounds.y &&
      pixel.y < spatialCanvasBounds.y + spatialCanvasBounds.h,
    '初始内容落在可操作画布范围',
  );
});
