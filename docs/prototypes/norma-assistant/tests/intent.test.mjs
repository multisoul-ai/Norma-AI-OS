import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const source = html.match(/<script id="assistant-core">([\s\S]*?)<\/script>/)?.[1];
assert.ok(source, 'HTML 必须包含可独立验证的对话意图模块');
const sandbox = {};
runInNewContext(source, sandbox);
const { plan, applyCards, applyPresentation } = sandbox.AssistantCore;
const plain = (value) => JSON.parse(JSON.stringify(value));

/// 场景：一句话同时调出进度和视频，只出现请求的两类卡片。
/// 数据构造：输入“看看项目进度，再播放视频”，包含两个独立意图。
/// 执行过程：1. 解析输入。2. 对空卡片集合应用动作。
/// 预期结果：progress、video 存在；web、files、image 不存在。
test('复合指令打开准确的卡片', () => {
  const cards = applyCards([], plan('看看项目进度，再播放视频'));
  assert.deepEqual(plain(cards), ['progress', 'video'], '必须打开进度与视频，不能插入无关卡片');
  assert.ok(!cards.includes('web'), '没有网页请求时不应出现网页卡片');
});

/// 场景：关闭网页只移除网页，已展示的进度继续保留。
/// 数据构造：初始集合为 progress、web；输入“关闭网页”。
/// 执行过程：1. 识别关闭动作。2. 从集合中移除 web。
/// 预期结果：progress 存在；web 不存在。
test('定向关闭不影响其他卡片', () => {
  const cards = applyCards(['progress', 'web'], plan('关闭网页'));
  assert.ok(cards.includes('progress'), '定向关闭网页不能关闭进度卡片');
  assert.ok(!cards.includes('web'), '网页卡片应被指令关闭');
});

/// 场景：重复调出文件保持一张卡片，避免对话使页面不断变密。
/// 数据构造：初始有 files；连续输入两次“打开设计文件”。
/// 执行过程：1. 应用首次指令。2. 再次应用相同指令。
/// 预期结果：files 存在且仅一份；progress 不存在。
test('重复请求不复制卡片', () => {
  const action = plan('打开设计文件');
  const cards = applyCards(applyCards(['files'], action), action);
  assert.deepEqual(plain(cards), ['files'], '重复调出应保持唯一文件卡片');
  assert.ok(!cards.includes('progress'), '打开文件不能额外调出项目进度');
});

/// 场景：回到纯净模式收起全部信息卡片。
/// 数据构造：初始有 progress、web、files、image、video 共五类。
/// 执行过程：1. 解析“收起所有卡片”。2. 应用清空动作。
/// 预期结果：集合为空；任何既有卡片均不残留。
test('收起全部卡片', () => {
  const cards = applyCards(['progress', 'web', 'files', 'image', 'video'], plan('收起所有卡片'));
  assert.deepEqual(plain(cards), [], '收起全部应恢复空白舞台');
  assert.ok(!cards.includes('video'), '清空后视频不能残留或继续显示');
});

/// 场景：不支持的自由问题获得说明，不能编造搜索或展示结果。
/// 数据构造：现有 image；输入“明天天气如何”不属于演示指令。
/// 执行过程：1. 返回 help 动作。2. 保持已打开卡片。
/// 预期结果：image 保留；web 不新增；回复说明原型范围。
test('未知请求不会伪造执行结果', () => {
  const action = plan('明天天气如何');
  const cards = applyCards(['image'], action);
  assert.equal(action.type, 'help', '未支持的问题应进入帮助状态');
  assert.ok(action.reply.includes('原型'), '回复必须说明当前为原型演示');
  assert.ok(cards.includes('image'), '未知请求不应破坏已有图片卡片');
  assert.ok(!cards.includes('web'), '未知请求不能伪装为已搜索网页');
});

/// 场景：图片与视频意图各自独立，视频请求不会带出图片。
/// 数据构造：分别输入“看看图片”和“看看视频”，初始均为空。
/// 执行过程：1. 分别解析。2. 对独立空集合应用动作。
/// 预期结果：图片指令仅 image；视频指令仅 video。
test('图片和视频保持独立意图', () => {
  assert.deepEqual(plain(applyCards([], plan('看看图片'))), ['image'], '图片请求应打开图片卡片');
  assert.deepEqual(plain(applyCards([], plan('看看视频'))), ['video'], '视频请求应打开视频卡片');
});

/// 场景：单独打开网页直接进入右侧内容浏览器，保留中间的网页卡片。
/// 数据构造：空卡片集合、空展示状态；输入“打开 Apple Intelligence 网页”。
/// 执行过程：1. 解析打开意图。2. 同时更新卡片集合和展示目标。
/// 预期结果：web 阅读目标存在；progress 不新增；网页卡片持续可见。
test('打开网页直接进入右侧浏览器', () => {
  const action = plan('打开 Apple Intelligence 网页');
  assert.equal(action.type, 'present', '打开网页应进入内容展示，不能只显示摘要卡片');
  assert.equal(applyPresentation(null, action).kind, 'web', '右侧阅读目标必须为网页');
  const cards = applyCards([], action);
  assert.ok(cards.includes('web'), '展示网页时必须保留可返回的网页卡片');
  assert.ok(!cards.includes('progress'), '打开网页不能额外增加项目卡片');
});

/// 场景：在网页阅读中调出图片，新图片成为当前标签。
/// 数据构造：当前展示 web；输入“全屏展示图片”。
/// 执行过程：1. 识别展开指令。2. 更新当前阅读目标。
/// 预期结果：image 成为当前目标；web 标签仍可切回。
test('当前阅读目标可以切换到图片', () => {
  const next = applyPresentation({ kind: 'web' }, plan('全屏展示图片'));
  assert.equal(next.kind, 'image', '右侧应展示请求的图片');
  assert.notEqual(next.kind, 'web', '原网页不能继续占据当前内容区');
});

/// 场景：收起右侧浏览器，同时保留原有信息卡片。
/// 数据构造：当前展示 web；已有 progress、web；输入“退出全屏”。
/// 执行过程：1. 解析返回指令。2. 清除展示目标。3. 保持卡片集合。
/// 预期结果：当前展示目标清空；两张原有卡片均存在。
test('收起浏览器不删除原有卡片', () => {
  const action = plan('退出全屏');
  assert.equal(applyPresentation({ kind: 'web' }, action), null, '收起后内容不再占据右侧');
  const cards = applyCards(['progress', 'web'], action);
  assert.ok(cards.includes('progress'), '返回对话必须保留项目进度');
  assert.ok(cards.includes('web'), '返回对话必须保留已打开的网页卡片');
});

/// 场景：关闭画布卡片不会关闭已打开的阅读标签。
/// 数据构造：当前展示 image；分别输入“关闭网页”和“关闭图片”。
/// 执行过程：1. 定向移除网页。2. 定向移除图片。
/// 预期结果：两次关闭卡片后 image 阅读目标都保留。
test('关闭画布卡片不关闭右侧阅读标签', () => {
  const current = { kind: 'image' };
  assert.equal(applyPresentation(current, plan('关闭网页')).kind, 'image', '关闭网页卡片不能打断图片阅读');
  assert.equal(applyPresentation(current, plan('关闭图片')).kind, 'image', '关闭图片卡片后阅读标签仍应保留');
});

/// 场景：在阅读中询问暂不支持的问题，原内容继续显示。
/// 数据构造：当前展示 web；输入“明天天气如何”。
/// 执行过程：1. 解析为帮助回复。2. 保持展示状态。
/// 预期结果：web 保留；不会切换到未请求的内容或对话舞台。
test('帮助回复不会遮挡正在阅读的内容', () => {
  const next = applyPresentation({ kind: 'web' }, plan('明天天气如何'));
  assert.equal(next.kind, 'web', '普通回复时右侧阅读目标应保留');
  assert.notEqual(next, null, '帮助回复不能强制关闭阅读内容');
});
