import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  initialCanvas,
  appendStroke,
  undoStroke,
  redoStroke,
  zoomAt,
  toWorld,
  strokeHits,
  annotatedNodes,
  annotationCommand,
} from '../lib/free-canvas.ts';
// 场景：笔迹跟随画布。构造缩放视口，将屏幕点换算为世界坐标并围绕同一锚点缩放，预期内容不跳。
test('zoom keeps the point under the pointer stable', () => {
  const view = { x: 30, y: 20, zoom: 0.8 },
    anchor = { x: 400, y: 250 };
  const before = toWorld(anchor, view);
  assert.deepEqual(
    toWorld(anchor, zoomAt(view, 1.3, anchor)),
    before,
    '缩放前后锚点对应同一世界位置',
  );
});
// 场景：画笔修改可恢复。向 Codex 节点画线，撤销再重做，预期关联节点与笔迹都恢复。
test('annotation is undoable, redoable, and grounded in node content', () => {
  const stroke = {
    id: 's1',
    color: '#fff',
    width: 3,
    points: [
      { x: 70, y: 100, pressure: 0.5 },
      { x: 160, y: 150, pressure: 0.5 },
    ],
  };
  const doc = appendStroke(initialCanvas(), stroke);
  assert.equal(annotatedNodes(doc)[0]?.id, 'codex-1', '笔迹应关联 Codex 内容');
  assert.equal(undoStroke(doc).strokes.length, 0, '撤销移除笔迹');
  assert.equal(redoStroke(undoStroke(doc)).strokes.length, 1, '重做恢复笔迹');
  assert.match(
    annotationCommand(doc),
    /Codex/,
    '交给 Norma 的指令包含标注上下文',
  );
  assert.equal(
    strokeHits(stroke, { x: 110, y: 122 }),
    true,
    '橡皮能命中线段中间',
  );
  assert.equal(
    strokeHits(stroke, { x: 800, y: 900 }),
    false,
    '橡皮不能擦除远处线段',
  );
});
