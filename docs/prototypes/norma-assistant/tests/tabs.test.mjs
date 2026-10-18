import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const source = html.match(/<script id="assistant-tabs">([\s\S]*?)<\/script>/)?.[1];
const sandbox = {};
if (source) runInNewContext(source, sandbox);
const drawerSource = html.match(/<script id="assistant-drawers">([\s\S]*?)<\/script>/)?.[1];
if (drawerSource) runInNewContext(drawerSource, sandbox);
const plain = value => JSON.parse(JSON.stringify(value));

/// 场景：画布中的网页和文件依次进入右侧阅读区。
/// 数据构造：从空标签开始，重复打开网页，再打开设计文件。
/// 执行过程：调用 open 三次，并检查标签与当前选择。
/// 预期结果：网页仅出现一次；文件与网页并存；当前文件可单独阅读。
test('右侧可同时保留网页与文件标签，重复打开定位原标签', () => {
  assert.ok(source, '页面应提供独立的标签状态逻辑');
  const first = sandbox.AssistantTabs.open([], {kind:'web'});
  const repeated = sandbox.AssistantTabs.open(first.tabs, {kind:'web'});
  const file = sandbox.AssistantTabs.open(repeated.tabs, {kind:'files',file:'design'});
  assert.deepEqual(plain(file.tabs.map(tab => tab.id)), ['web','files:design'], '重复网页不应复制标签，文件应共存');
  assert.equal(file.activeId, 'files:design', '新打开的文件应成为当前标签');
});

/// 场景：关闭正在阅读的文件后，网页仍可继续阅读。
/// 数据构造：已打开网页和文件，当前选择文件。
/// 执行过程：关闭文件标签。
/// 预期结果：网页仍在，且成为当前标签；文件标签消失。
test('关闭当前标签会选择相邻标签', () => {
  const tabs = [{id:'web',kind:'web'},{id:'files:design',kind:'files',file:'design'}];
  const result = sandbox.AssistantTabs.close(tabs, 'files:design', 'files:design');
  assert.deepEqual(plain(result.tabs.map(tab => tab.id)), ['web'], '只关闭指定文件标签');
  assert.equal(result.activeId, 'web', '剩余网页应自动选中');
});

/// 场景：同类文件中的不同文件需要独立阅读位置。
/// 数据构造：设计文件与需求文件分别打开。
/// 执行过程：生成两个资源标识。
/// 预期结果：标识不同，切换时可按标识恢复各自的滚动位置。
test('不同文件有不同标签标识', () => {
  assert.equal(sandbox.AssistantTabs.id({kind:'files',file:'design'}), 'files:design', '设计文件标识正确');
  assert.equal(sandbox.AssistantTabs.id({kind:'files',file:'spec'}), 'files:spec', '需求文件标识正确');
  assert.notEqual(sandbox.AssistantTabs.id({kind:'files',file:'design'}), sandbox.AssistantTabs.id({kind:'files',file:'spec'}), '两个文件不能复用同一个标签');
});

/// 场景：左右抽屉可分别隐藏，中间画布仍然存在。
/// 数据构造：两个抽屉均打开，依次收起左侧与右侧。
/// 执行过程：调用 set 更新对应侧的打开状态。
/// 预期结果：一次只改变目标抽屉；两侧都收起后画布可占满空间。
test('左右抽屉可独立收起', () => {
  assert.ok(drawerSource, '页面应有独立的抽屉状态逻辑');
  const left = sandbox.AssistantDrawers.set({chat:true,reader:true},'chat',false);
  assert.deepEqual(plain(left), {chat:false,reader:true}, '收起对话不能连带关闭浏览器');
  const both = sandbox.AssistantDrawers.set(left,'reader',false);
  assert.deepEqual(plain(both), {chat:false,reader:false}, '两侧都应可收起');
});

/// 场景：窄窗口一次只展示一个抽屉，避免遮住整个画布。
/// 数据构造：左侧对话打开，准备打开右侧浏览器。
/// 执行过程：以 compact 模式打开右侧。
/// 预期结果：右侧打开、左侧收起；再次打开左侧则反向切换。
test('窄窗口打开一侧会收起另一侧', () => {
  const right = sandbox.AssistantDrawers.set({chat:true,reader:false},'reader',true,true);
  assert.deepEqual(plain(right), {chat:false,reader:true}, '窄窗口的右侧抽屉应独占可见空间');
  const left = sandbox.AssistantDrawers.set(right,'chat',true,true);
  assert.deepEqual(plain(left), {chat:true,reader:false}, '回到左侧时应收起右侧');
});

/// 场景：宽屏时同时展开两侧，随后窗口缩为手机宽度。
/// 数据构造：起点为双开状态，分别模拟正在阅读内容和右侧空态。
/// 执行过程：执行断点适配，再核对保留的抽屉。
/// 预期结果：阅读时保留右侧，否则优先保留左侧聊天；两者不应重叠并同时声明可操作。
test('切入窄窗口时只保留当前有用的一侧抽屉', () => {
  assert.deepEqual(plain(sandbox.AssistantDrawers.fitViewport({chat:true,reader:true}, true, true)), {chat:false,reader:true}, '阅读内容时应保留右侧');
  assert.deepEqual(plain(sandbox.AssistantDrawers.fitViewport({chat:true,reader:true}, true, false)), {chat:true,reader:false}, '右侧空态时应保留对话');
  assert.deepEqual(plain(sandbox.AssistantDrawers.fitViewport({chat:true,reader:true}, false, true)), {chat:true,reader:true}, '宽屏不应改变双开状态');
});
