import { test } from 'node:test';
import assert from 'node:assert/strict';
import { canvasControls, spatialControls } from '../lib/interaction-chrome.ts';
// 场景：进入画布时不被低频操作淹没。数据：选择/画笔两种模式；执行读取可见操作，预期默认仅三个核心动作。
test('canvas chrome only expands for annotation work', () => {
  assert.deepEqual(
    canvasControls('select'),
    ['select', 'pen', 'add'],
    '默认只提供选择、画笔与添加',
  );
  assert.ok(!canvasControls('select').includes('eraser'), '未绘图时不显示橡皮');
  assert.ok(canvasControls('pen').includes('eraser'), '进入画笔模式后提供橡皮');
});
// 场景：头显默认工作视野。数据：关闭菜单；执行读取空间操作，预期四项以内，布局控制仅在更多内。
test('spatial controls disclose window settings on demand', () => {
  const controls = spatialControls(false, 'select');
  assert.equal(controls.length, 4, '默认空间工具条为四项');
  assert.ok(!controls.includes('nearer'), '阅读距离不常驻');
  assert.ok(
    spatialControls(true, 'select').includes('nearer'),
    '打开更多后可以调整阅读距离',
  );
});

// 场景：原型是 VR 产品。数据：默认入口与显式桌面参数；预期默认进入空间，桌面仅按明确参数展示。
test('prototype opens into VR preview by default', async () => {
  const { initialSpatialMode } = await import('../lib/interaction-chrome.ts');
  assert.equal(
    initialSpatialMode(null, false),
    'orbit',
    '默认进入 VR 空间预览',
  );
  assert.equal(
    initialSpatialMode('workspace', false),
    'workspace',
    '显式选择桌面时允许二维编辑',
  );
  assert.equal(
    initialSpatialMode(null, true),
    'workspace',
    '旧比较页保持原入口',
  );
});
