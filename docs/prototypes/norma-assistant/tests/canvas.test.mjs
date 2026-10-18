import { readFileSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const root = new URL('../../../../', import.meta.url);
const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const canvas = html.match(/<main class="stage" id="stage"[^>]*>([\s\S]*?)<\/main>/)?.[1];

/// 场景：中间画布遵守真实 App 的 Live Node 类型范围。
/// 数据构造：读取 HTML 中画布节点，不读取左右抽屉的内容。
/// 执行过程：提取所有 data-node-kind，并检查画布区域的内容标签。
/// 预期结果：只有智能体和终端；至少各一张；网页、文件、进度、图片、视频都不作为中间节点。
test('中间画布只展示智能体与终端卡片', () => {
  assert.ok(canvas, '页面应有中间画布');
  const kinds = [...canvas.matchAll(/data-node-kind="([^"]+)"/g)].map(match => match[1]);
  assert.ok(kinds.includes('agent'), '画布应展示智能体卡片');
  assert.ok(kinds.includes('terminal'), '画布应展示终端卡片');
  assert.ok(kinds.every(kind => kind === 'agent' || kind === 'terminal'), '画布不能展示其他内容卡片');
  assert.doesNotMatch(canvas, /class="deck"/, '旧的进度、网页、文件和媒体卡片容器应离开画布');
});

/// 场景：原型复用生产 App 的默认地球背景。
/// 数据构造：读取生产资源与原型本地资源。
/// 执行过程：比较两份文件的 SHA-256，并检查页面确实引用原型资源。
/// 预期结果：原型使用完全相同的默认背景图片，而非近似渐变或重新绘制版本。
test('画布使用项目默认背景图的精确副本', () => {
  const source = new URL('crates/app/assets/home/earth-horizon.png', root);
  const copy = new URL('../assets/earth-horizon.png', import.meta.url);
  assert.ok(existsSync(copy), '原型应包含可由本地服务读取的默认背景图');
  const digest = path => createHash('sha256').update(readFileSync(path)).digest('hex');
  assert.equal(digest(copy), digest(source), '原型背景应与 App 默认资源逐字节一致');
  assert.match(html, /url\(["']?assets\/earth-horizon\.png/, '画布样式应使用该背景图');
});

/// 场景：两侧抽屉打开时仍能看到跨越整个工作台的默认背景。
/// 数据构造：读取最终材质样式和画布下方的页面导航结构。
/// 执行过程：检查背景归属、透明抽屉、减少透明度回退及底部导航的示意项。
/// 预期结果：背景在 app 上连续铺满；左右抽屉透出背景，系统要求时恢复实色；导航只呈现当前页与其他页面入口。
test('全窗口背景透过左右抽屉，画布下方有页面导航布局', () => {
  const material = html.split('/* 全窗口背景与抽屉材质。 */')[1];
  assert.ok(material, '页面应有专门的全窗口背景与抽屉材质样式');
  assert.match(material, /\.app\{[^}]*earth-horizon\.png[^}]*\}/, '默认背景应由整个工作台绘制');
  assert.match(material, /\.workspace-panel\{[^}]*background:transparent/, '画布外壳不能盖住背景');
  assert.match(material, /\.stage\{[^}]*background:transparent/, '画布滚动层不能盖住背景');
  assert.match(material, /\.conversation-panel\{[^}]*background:[^;}]*#[0-9a-f]{8}/i, '左侧抽屉应使用带透明度的材质');
  assert.match(material, /\.reader-panel\{[^}]*background:[^;}]*#[0-9a-f]{8}/i, '右侧抽屉应使用带透明度的材质');
  assert.match(material, /prefers-reduced-transparency:reduce/, '系统减少透明度时应有不透明回退');
  const workspace = html.match(/<section class="workspace-panel"[\s\S]*?<\/section>\s*<aside class="reader-panel"/)?.[0];
  assert.ok(workspace, '页面应包含画布工作区');
  assert.match(workspace, /<nav class="page-dock"[^>]*aria-label="页面切换"/, '画布下方应有页面切换导航');
  assert.match(workspace, /aria-current="page"/, '导航应标识当前项目页面');
  assert.match(workspace, /项目总览/, '导航应提供项目总览入口');
});

/// 场景：用户在左侧即可扫视中间画布中的 Agent 工作状态与后续待办。
/// 数据构造：只读取左侧对话抽屉和中间画布的节点标识。
/// 执行过程：检查悬浮进度框覆盖两名 Agent、执行任务、待审核项及文件入口示意。
/// 预期结果：进度信息只在左侧展示；中间画布仍只有智能体和终端节点。
test('左侧悬浮进度框汇总画布 Agent 与待处理内容', () => {
  const sidebar = html.match(/<aside class="conversation-panel"[\s\S]*?<\/aside>/)?.[0];
  assert.ok(sidebar, '页面应保留左侧对话抽屉');
  const progress = sidebar.match(/<section class="progress-float"[\s\S]*?<\/section>/)?.[0];
  assert.ok(progress, '左侧应提供悬浮进度框');
  assert.ok(sidebar.indexOf(progress) < sidebar.indexOf('class="conversation-heading"'), '进度框应吸附在左侧顶部，先于对话内容');
  assert.ok(sidebar.indexOf(progress) < sidebar.indexOf('class="conversation-scroll"'), '进度框不应随聊天记录滚走');
  assert.match(progress, /Bella/, '进度框应展示 Bella 的执行情况');
  assert.match(progress, /Bruno/, '进度框应展示 Bruno 的执行情况');
  assert.match(progress, /进行中[\s\S]*?任务 · 调整原型布局/, '进度框应写明当前任务及其状态');
  assert.match(progress, /待审核/, '进度框应展示待审核项');
  assert.match(progress, /待查看文件/, '进度框应展示需要查看的文件');
  assert.doesNotMatch(canvas, /progress-float/, '进度框不应占据中间画布节点');
});

/// 场景：从悬浮进度框快速定位 Agent、审核项、结果和文件。
/// 数据构造：读取进度框按钮、画布目标标识及右侧文件预览数据。
/// 执行过程：检查每个跳转入口都有真实目标，并有事件委托处理点击。
/// 预期结果：Agent 与审核入口定位画布卡片，结果与文件入口打开右侧预览。
test('进度框快捷按钮有对应画布目标或文件预览', () => {
  const sidebar = html.match(/<aside class="conversation-panel"[\s\S]*?<\/aside>/)?.[0];
  const progress = sidebar?.match(/<section class="progress-float"[\s\S]*?<\/section>/)?.[0];
  assert.ok(progress, '左侧应有进度框');
  assert.match(progress, /id="progress-float"/, '事件委托应能找到进度框');
  assert.match(progress, /<button[^>]*data-progress-node="agent-bella"/, 'Bella 状态应可点击定位');
  assert.match(progress, /<button[^>]*data-progress-node="agent-bruno"/, 'Bruno 状态应可点击定位');
  assert.match(progress, /<button[^>]*data-progress-node="approval-bella"/, '待审核项应可定位到具体卡片位置');
  assert.match(progress, /<button[^>]*data-progress-file="result"/, '结果应有打开预览的按钮');
  assert.match(progress, /<button[^>]*data-progress-file="prototype"/, 'HTML 文件应有打开预览的按钮');
  assert.match(progress, /<button[^>]*data-progress-file="wallpaper"/, '背景图文件应有打开预览的按钮');
  assert.match(canvas, /id="agent-bella"/, 'Bella 画布卡片应有跳转目标');
  assert.match(canvas, /id="agent-bruno"/, 'Bruno 画布卡片应有跳转目标');
  assert.match(canvas, /id="approval-bella"/, '审核项应有对应的画布内容');
  assert.match(html, /data-progress-file[^\n]*openPresentation|progress-float[^\n]*addEventListener/, '快捷按钮应有实际事件处理');
});

/// 场景：用户从吸顶卡片判断当前项目、Agent 任务，以及谁提交了待审方案与结果。
/// 数据构造：读取左侧进度框及其 Agent、审核、结果行。
/// 执行过程：检查项目名称与每条操作项的归属标识，并检查抽屉图标使用侧边栏语义。
/// 预期结果：无需猜测任务归属；审核和方案可直接定位，顶部按钮只呈现对应抽屉的开关。
test('吸顶概览说明项目和任务归属，顶部抽屉入口语义清晰', () => {
  const progress = html.match(/<section class="progress-float"[\s\S]*?<\/section>/)?.[0];
  assert.ok(progress, '应有吸顶进度框');
  assert.match(progress, /当前项目[\s\S]*?Norma-AI-OS/, '概览应标明当前项目');
  assert.match(progress, /Bella[\s\S]*?任务 · 调整原型布局/, 'Bella 应有明确任务');
  assert.match(progress, /Bruno[\s\S]*?任务 · 检查抽屉/, 'Bruno 应有明确任务');
  assert.match(progress, /待审核[\s\S]*?Bella · 抽屉视觉方案[\s\S]*?data-progress-node="approval-bella"/, '审核项应写明提交 Agent 并可定位');
  assert.match(progress, /成果待查看[\s\S]*?Bella · 布局验收结果[\s\S]*?data-progress-file="result"/, '成果应写明提交 Agent 并可打开');
  assert.match(html, /id="chat-collapse"[^>]*>\s*<span data-icon="panel-left-close"/, '左侧收起按钮应使用面板语义图标');
  assert.match(html, /id="chat-open"[^>]*>\s*<span data-icon="panel-left-open"/, '左侧展开按钮应使用面板语义图标');
  assert.match(html, /id="reader-open"[^>]*>\s*<span data-icon="panel-right-open"/, '右侧展开按钮应使用面板语义图标');
  assert.match(html, /id="reader-collapse"[^>]*>\s*<span data-icon="panel-right-close"/, '右侧收起按钮应使用面板语义图标');
});

/// 场景：用户希望概览悬浮吸顶，空出对话空间，并可随时折叠或展开。
/// 数据构造：读取概览结构、最终样式、键盘状态与截图指出的冗余文案。
/// 执行过程：确认初始紧凑态、展开控制与收起时的键盘隔离，并检查概览不参加侧栏纵向排版。
/// 预期结果：待办角标可见，任务按钮按需展开；输入区和画布没有冗余提示文字。
test('概览作为可折叠的吸顶悬浮层，不挤压对话并清理冗余文案', () => {
  const progress = html.match(/<section class="progress-float"[\s\S]*?<\/section>/)?.[0];
  const body = html.split('<body>')[1] || '';
  assert.ok(progress, '应有概览悬浮层');
  assert.match(progress, /data-collapsed="true"/, '概览初始应收为紧凑入口');
  assert.match(progress, /id="progress-toggle"[^>]*aria-controls="progress-body"[^>]*aria-expanded="false"/, '折叠按钮应暴露展开状态及受控内容');
  assert.match(progress, /id="progress-body"[^>]*aria-hidden="true"[^>]*inert/, '初始收起时任务按钮不应进入键盘焦点');
  assert.match(progress, /class="progress-badge"[^>]*>3<\/span>/, '紧凑入口应提示三个待处理项');
  assert.match(html, /\.progress-float\{position:absolute;top:78px;/, '概览应吸附在左栏顶部且不占据纵向排版空间');
  assert.match(html, /function setProgressCollapsed\(/, '概览应有展开和收起状态同步逻辑');
  assert.match(html, /\$\('progress-toggle'\)\.addEventListener\('click'/, '点击悬浮入口应切换展开状态');
  assert.doesNotMatch(body, /中间查看智能体与终端 · 右侧浏览内容/, '输入框下方的功能提示应移除');
  assert.doesNotMatch(body, /视觉示例 · 未连接真实会话/, '画布顶栏的示例说明应移除');
  assert.match(body, /<footer class="bottom-bar" hidden>/, '底部状态栏应从可见布局中移除');
});
