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

const overview = html.match(/<section class="run-overview"[\s\S]*?<!-- \/run-overview -->/)?.[0];
const sidebar = html.match(/<aside class="conversation-panel"[\s\S]*?<\/aside>/)?.[0] || '';
const header = html.match(/<header class="workspace-header"[\s\S]*?<\/header>\s*<main/)?.[0] || '';
const status = header.match(/<button class="run-status"[\s\S]*?<\/button>/)?.[0] || '';
const overviewCss = html.split('/* 运行概览：左侧抽屉视图、顶栏状态与悬浮面板。 */')[1]?.split('/* /运行概览 */')[0] || '';

/// 场景：运行概览作为左侧抽屉的一个视图，与对话并列；画布右上角只保留可点击的状态显示。
/// 数据构造：读取左侧抽屉、画布顶栏、右侧抽屉与画布节点区。
/// 执行过程：检查概览面板位于左侧抽屉、在输入区之前；顶栏只有状态按钮且排在内容浏览器开关左侧。
/// 预期结果：概览不在顶栏、右栏或画布节点中；顶栏状态按钮控制左侧概览面板。
test('运行概览位于左侧抽屉，右上角保留状态显示', () => {
  assert.ok(overview, '页面应有运行概览面板');
  assert.ok(sidebar.includes(overview), '运行概览应位于左侧抽屉');
  assert.ok(sidebar.indexOf(overview) < sidebar.indexOf('class="composer-area"'), '概览在输入区之前，输入区两种视图共用');
  assert.ok(status, '画布顶栏应保留状态按钮');
  assert.ok(header.indexOf(status) < header.indexOf('id="reader-open"'), '状态按钮应排在内容浏览器开关左侧');
  assert.doesNotMatch(header, /class="run-overview"/, '顶栏不再承载概览面板');
  assert.match(status, /id="progress-toggle"[^>]*aria-controls="progress-body"[^>]*aria-expanded="false"/, '状态按钮应控制左侧概览并暴露状态');
  const reader = html.match(/<aside class="reader-panel"[\s\S]*?<\/aside>/)?.[0] || '';
  assert.doesNotMatch(reader, /run-overview/, '右栏不承载概览');
  assert.doesNotMatch(canvas, /run-overview|run-status/, '概览与状态都不是画布节点');
});

/// 场景：用户在左侧抽屉中切换“对话 / 运行概览”，或从右上角状态直接切到概览。
/// 数据构造：读取抽屉顶部切换控件、视图隐藏样式与切换脚本。
/// 执行过程：检查两个 tab 的选中语义与待处理角标，切换函数会在抽屉收起时先展开，再次点击状态回到对话；方向键可在 tab 间移动；视图进入有方向感动效并有减少动态效果回退。
/// 预期结果：默认对话；概览 tab 显示 3；切换不丢失输入区；状态按钮的 aria-expanded 与概览可见性一致。
test('左侧抽屉固定显示运行概览，对话入口隐藏，右上角状态一键展开', () => {
  const tabs = sidebar.match(/<nav class="drawer-switch"[\s\S]*?<\/nav>/)?.[0] || '';
  assert.match(tabs, /<nav class="drawer-switch" id="drawer-switch" hidden/, '对话 / 概览切换控件隐藏（与产品一致）');
  assert.match(overviewCss, /\.drawer-switch\[hidden\]\{display:none\}/, 'hidden 必须真正生效，不被 display:grid 覆盖');
  assert.match(html, /data-left-view="overview"/, '默认就是运行概览');
  assert.match(html, /const LEFT_CHAT_HIDDEN=true;[\s\S]*?function setLeftView\([^)]*\)\s*\{\s*if\(LEFT_CHAT_HIDDEN\)view='overview'/, '对话视图不可达：任何切换都落在概览');
  const statusClick = html.match(/\$\('progress-toggle'\)\.addEventListener\('click'[\s\S]*?\n\}\);/)?.[0] || '';
  assert.match(statusClick, /setLeftView\('overview'/, '状态按钮展开左侧概览');
  assert.match(html, /function setLeftView\([\s\S]*?AssistantDrawers\.set\([^)]*'chat',true/, '抽屉收起时先展开抽屉');
  assert.match(overviewCss, /@keyframes left-view-in/, '保留视图进入动效定义');
});

/// 场景：用户以工作空间为单位管理智能体：先看哪些需要自己处理，再看哪些在运行、哪些待开始，不论智能体属于哪个项目。
/// 数据构造：读取概览中的三个分组标题和行内容。
/// 执行过程：比较分组顺序，并检查每行写明事项、提交 Agent 和所属项目。
/// 预期结果：待处理 3 项排在最前；Bella/Bruno 任务与状态明确；其他项目只显示上次状态，不自动加载。
test('概览按待处理、运行中、待开始跨项目分组且归属明确', () => {
  const pending = overview.indexOf('需要你处理');
  const agents = overview.indexOf('data-group="running"');
  const others = overview.indexOf('data-group="idle"');
  assert.ok(pending > 0 && pending < agents && agents < others, '分组顺序应为 待处理 → 运行中 → 待开始');
  assert.match(overview, /待审核 · 抽屉视觉方案[\s\S]*?Bella · Norma-AI-OS/, '审核项应写明提交 Agent 与项目');
  assert.match(overview, /成果待查看 · 布局验收结果[\s\S]*?Bella/, '成果应写明提交 Agent');
  assert.match(overview, /待查看文件 · index\.html[\s\S]*?Bella/, '文件应写明提交 Agent');
  assert.match(overview, /Bella[\s\S]*?运行中[\s\S]*?调整原型布局与透明材质/, 'Bella 应有状态与任务');
  assert.match(overview, /Bruno[\s\S]*?待开始[\s\S]*?检查抽屉响应式布局/, 'Bruno 应有状态与任务');
  assert.match(overview, /data-group="idle"[\s\S]*?Bruno[\s\S]*?class="run-row is-project"[\s\S]*?设计草稿[\s\S]*?未加载[\s\S]*?上次状态/, '未加载项目并入待开始组尾部，排在智能体之后，标明上次状态与未加载');
  assert.match(overview, /id="run-body-attention">\s*<button class="run-row is-failed"/, '出错的智能体置顶在需要你处理');
  assert.match(html, /run-row:not\(\.is-project\)/, '项目行不计入分组与统计数字');
  assert.match(overview, /id="run-summary"[\s\S]*?data-jump="attention"[\s\S]*?data-jump="running"[\s\S]*?data-jump="idle"/, '顶部状态概览应给出待处理、运行中、待开始三类数量并可跳转');
  assert.match(html, /function refreshRunCounts\(/, '状态变化后应刷新分组计数');
  assert.doesNotMatch(overview, /\d+%/, '没有可靠步骤时不显示完成百分比');
});

/// 场景：从概览直达画布卡片、审核位置或右侧文件预览。
/// 数据构造：读取概览行的目标标识、画布目标 id 与事件处理代码。
/// 执行过程：检查每一行都是单一操作按钮，且目标真实存在。
/// 预期结果：Agent 与审核定位画布；成果与文件打开右侧标签；其他项目行有切换示意处理。
test('概览每行只有一个直达动作且目标真实存在', () => {
  for (const id of ['agent-bella', 'agent-bruno', 'approval-bella']) {
    assert.match(overview, new RegExp(`<button[^>]*class="run-row[^"]*"[^>]*data-progress-node="${id}"`), `${id} 行应可定位`);
    assert.match(canvas, new RegExp(`id="${id}"`), `${id} 应有画布目标`);
  }
  for (const file of ['result', 'prototype']) {
    assert.match(overview, new RegExp(`<button[^>]*class="run-row[^"]*"[^>]*data-progress-file="${file}"`), `${file} 行应可打开预览`);
  }
  assert.match(overview, /<button[^>]*class="run-row[^"]*"[^>]*data-progress-project="design"/, '其他项目行应可切换');
  const rows = [...overview.matchAll(/<button[^>]*class="run-row[^"]*"[^>]*>([\s\S]*?)<\/button>/g)];
  assert.equal(rows.length, 7, '4 项待处理（含出错）+ 2 个智能体 + 1 个未加载项目行，共 7 行');
  assert.ok(rows.every(([, inner]) => !inner.includes('<button')), '行内不应嵌套第二个按钮');
  assert.match(html, /\$\('progress-body'\)\.addEventListener\('click'/, '概览应有事件委托');
  assert.match(html, /data-progress-file[^\n]*openPresentation/, '文件行应打开右侧预览');
  assert.match(html, /ArrowDown[\s\S]*?run-row/, '方向键应能在概览行之间移动');
  assert.match(html, /is-peeked/, '悬停或聚焦行时应预亮对应画布卡片');
});

/// 场景：用户不展开概览，也要一眼看出每个智能体在做什么、做到哪一步、是否在等自己。
/// 数据构造：读取右上角状态按钮、概览中的智能体行与画布卡片标题栏。
/// 执行过程：检查状态按钮逐个智能体显示状态与步骤（Bella 运行中 4/6 且有待审标记；Bruno 待开始），概览行有 6 段步骤条（4 段完成、第 5 段为当前），画布卡片标题同步状态。
/// 预期结果：三处状态与步骤一致；数字为已完成数 n/m，概览行写“第 n+1/m 项”指向进行中项，不显示无依据的百分比；读屏文字包含相同信息。
test('一眼可读：右上角状态、概览行与画布卡片同步显示智能体状态和步骤进度', () => {
  assert.match(status, /aria-label="[^"]*工作空间共 3 个智能体，1 个运行中，1 个待开始/, '状态按钮读屏文字应汇总跨项目智能体状态');
  assert.match(status, /class="run-agent-chip is-running"[\s\S]*?data-top="running">1<[\s\S]*?运行中/, '右上角应汇总运行中数量');
  assert.match(status, /class="run-agent-chip is-idle"[\s\S]*?data-top="idle">1<[\s\S]*?待开始/, '右上角应汇总待开始数量');
  assert.doesNotMatch(status, /Bella|Bruno/, '右上角不再逐个列出智能体');
  assert.match(status, /class="progress-badge"[^>]*>4</, '状态按钮应显示待处理数');
  const bella = overview.match(/<button class="run-row is-agent"[^>]*data-progress-node="agent-bella"[\s\S]*?<\/button>/)?.[0] || '';
  const bruno = overview.match(/<button class="run-row is-agent is-idle"[^>]*data-progress-node="agent-bruno"[\s\S]*?<\/button>/)?.[0] || '';
  const segments = [...bella.matchAll(/<i class="([^"]*)"><\/i>/g)].map(match => match[1]);
  // 6 段 = 待办总数；4 段 done = 已完成 4 项；第 5 段 current = 正在进行的“原型交互验收”，所以文案写“第 5/6 项”。
  assert.deepEqual(segments, ['done', 'done', 'done', 'done', 'current', ''], 'Bella 步骤条应为 4 完成 + 1 进行中 + 1 未开始');
  assert.match(bella, /第 5\/6 项 · 原型交互验收/, 'Bella 行应写明当前步骤');
  assert.match(bella, /1 待审核/, 'Bella 行应提示在等用户');
  assert.doesNotMatch(bruno, /run-steps/, '未开始的智能体没有可靠步骤，不画进度条');
  assert.match(bruno, /尚未开始/, 'Bruno 行应说明尚未开始');
  assert.match(canvas, /<strong>Bella<\/strong>[\s\S]*?class="live-task-status is-running"[^>]*>运行中 4\/6</, 'Bella 卡片标题应同步状态与步骤');
  assert.match(canvas, /<strong>Bruno<\/strong>[\s\S]*?class="live-task-status is-idle"[^>]*>待开始</, 'Bruno 卡片标题应同步状态');
});

/// 场景：用户想一边和 Norma 对话一边盯着概览，可把概览从抽屉弹出为画布上的悬浮面板。
/// 数据构造：读取概览头部的悬浮/移动/收回按钮、画布悬浮层、样式与事件代码。
/// 执行过程：检查悬浮按钮的切换语义、移动与收回按钮仅在悬浮时出现、拖动限制在画布区域内且可复位、偏好安全持久化。
/// 预期结果：悬浮后抽屉回到对话、面板常驻画布上方且可移动；收回后回到抽屉概览；存储不可用时页面仍正常。
test('概览可从抽屉弹出为悬浮固定面板，可拖动并记住位置', () => {
  assert.match(overview, /id="progress-pin"[^>]*aria-pressed="false"[^>]*aria-label="悬浮固定运行概览"/, '头部应有悬浮固定按钮');
  assert.match(overview, /id="progress-move"[^>]*aria-label="移动运行概览[^"]*"[^>]*hidden/, '移动手柄只在悬浮时出现');
  assert.match(header + html.match(/<section class="workspace-panel"[\s\S]*?<\/section>\s*<aside class="reader-panel"/)?.[0], /id="run-float-layer"/, '画布区应有悬浮层');
  assert.match(overviewCss, /\.run-overview\[data-mode=floating\]\{[^}]*position:absolute[^}]*translate\(var\(--run-x/, '悬浮面板按拖动偏移定位');
  assert.match(overviewCss, /\.is-dragging[^{]*\{[^}]*transition:none/, '拖动时应 1:1 跟手');
  assert.match(html, /function setProgressFloating\(/, '应有悬浮状态切换逻辑');
  assert.match(html, /function setProgressFloating\([\s\S]*?run-float-layer[\s\S]*?setLeftView\('overview'/, '弹出后面板移到画布悬浮层');
  assert.match(html, /function clampProgressOffset\(/, '拖动偏移应限制在画布区域内');
  assert.match(html, /setPointerCapture/, '拖动应捕获指针');
  assert.match(html, /\$\('progress-move'\)\.addEventListener\('keydown'[\s\S]*?Arrow/, '移动手柄应支持方向键');
  assert.match(html, /addEventListener\('dblclick'[\s\S]*?setProgressOffset\(0,0\)/, '双击标题栏应复位');
  assert.match(html, /try\{preferenceStorage\?\.setItem\('norma\.prototype\.runOverview'/, '偏好应在 try 中写入存储');
  assert.doesNotMatch(overviewCss, /font-size:(?:[0-9]|10)px|font:[^;}]*\b(?:[0-9]|10)px/, '概览字号不应低于 11px');
});

/// 场景：顶部抽屉开关使用侧边栏语义图标，冗余文案已清理。
/// 数据构造：读取四个抽屉开关按钮与页面正文。
/// 执行过程：检查各自的面板图标及已移除的说明。
/// 预期结果：顶部按钮只呈现对应抽屉的开关语义。
test('顶部抽屉入口语义清晰', () => {
  const body = html.split('<body>')[1] || '';
  assert.match(html, /id="chat-collapse"[^>]*>\s*<span data-icon="panel-left-close"/, '左侧收起按钮应使用面板语义图标');
  assert.match(html, /id="chat-open"[^>]*>\s*<span data-icon="panel-left-open"/, '左侧展开按钮应使用面板语义图标');
  assert.match(html, /id="reader-open"[^>]*>\s*<span data-icon="panel-right-open"/, '右侧展开按钮应使用面板语义图标');
  assert.match(html, /id="reader-collapse"[^>]*>\s*<span data-icon="panel-right-close"/, '右侧收起按钮应使用面板语义图标');
  assert.doesNotMatch(body, /视觉示例 · 未连接真实会话/, '画布顶栏的示例说明应移除');
  assert.match(body, /<footer class="bottom-bar" hidden>/, '底部状态栏应从可见布局中移除');
});

/// 场景：左栏的「对话」标签与底部对话输入框暂时隐藏，与 App 一致。
/// 数据构造：读取页面源码。
/// 执行过程：检查隐藏开关、强制运行概览与隐藏样式。
/// 预期结果：开关为 true；setLeftView 把任何视图都落到运行概览；标签条与对话输入区样式隐藏；DOM 结构仍保留以便恢复。
test('左栏隐藏对话标签与输入框，只呈现运行概览', () => {
  assert.match(html, /const LEFT_CHAT_HIDDEN=true;/, '隐藏开关');
  assert.match(html, /if\(LEFT_CHAT_HIDDEN\)view='overview';/, 'setLeftView 强制落到运行概览');
  assert.match(html, /data-chat-hidden=true\] \.drawer-switch[^{]*\.composer-area\{display:none\}/, '标签条与对话输入区隐藏');
  assert.match(html, /<nav class="drawer-switch"/, 'DOM 保留，开关改回 false 即可恢复');
});
