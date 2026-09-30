// Run: node scripts/check-norma-realtime-prototype.cjs
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const html = fs.readFileSync('docs/prototypes/norma-realtime.html', 'utf8');
const source = html.match(/<script id="realtime-model">([\s\S]*?)<\/script>/);
assert.ok(source, '原型必须提供可独立检查的状态逻辑');
for (const [, script] of html.matchAll(/<script[^>]*>([\s\S]*?)<\/script>/g)) new vm.Script(script);
const context = vm.createContext({});
vm.runInContext(source[1] + '\nthis.api={initialRealtime, act, orderedWorkTasks, iphone17PreviewFrame, showsConversationDock};', context);
const {initialRealtime, act, orderedWorkTasks, iphone17PreviewFrame, showsConversationDock} = context.api;
const css = html.match(/<style>([\s\S]*?)<\/style>/)?.[1] ?? '';

/// [方案审核门禁]：三张新卡片先提交方案；只允许看过当前版本后批准，修改意见仍回原卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   初始卡片 = 4 张；新建待审核卡片 = 3 张；总数 = 4 + 3 = 7 张。
///   Daisy 初稿 v1；一次修改后为 v2，旧版已查看状态必须失效。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 确认带审核开关的派工 → 三张卡片停在待审核，不能直接完成。
///   2. 只查看 Daisy → 全部批准被拒；要求修改 → v2 再次待审核。
///   3. 查看 v2 与其他两份方案 → 离线放行被拒；重连后单独批准 Ethan，再批量批准剩余两份。
///
/// 预期结果：
///   - 正断言：审核前有三份方案和相同任务身份，审核后这三张卡片运行。
///   - 负断言：未查看、旧版已查看、离线状态不能放行；其他已有卡片不受影响。
{
  const s = initialRealtime();
  assert.equal(act(s, {type:'dispatch', operation:'review-1', thoughtId:'thought-1', reviewPlans:true}).ok, true, '审核派工应创建待审核的 Agent Card');
  assert.equal(s.tasks.length, 7, '审核派工后应保留原 4 张并新建 3 张卡片');
  const daisy = s.tasks.find(t => t.cardName === 'Daisy');
  const ethan = s.tasks.find(t => t.cardName === 'Ethan');
  const felix = s.tasks.find(t => t.cardName === 'Felix');
  assert.equal(daisy.status, 'plan-review', 'Daisy 不能在用户审核前执行');
  assert.equal(ethan.status, 'plan-review', 'Ethan 不能在用户审核前执行');
  assert.equal(felix.status, 'plan-review', 'Felix 不能在用户审核前执行');
  assert.equal(act(s, {type:'complete', id:daisy.id}).ok, false, '未批准的方案不能越过门禁直接完成');
  assert.equal(act(s, {type:'approve-plan', id:daisy.id}).ok, false, '未查看 Daisy 方案时不能批准');
  assert.equal(act(s, {type:'inspect-plan', id:daisy.id}).ok, true, '用户应能查看 Daisy 的当前方案');
  assert.equal(act(s, {type:'approve-all-plans', operation:'review-1'}).ok, false, '只看一份不能批准三份方案');
  assert.equal(act(s, {type:'revise-plan', id:daisy.id, text:'先完成登录恢复测试'}).ok, true, '修改意见应发回 Daisy 原卡片');
  assert.equal(daisy.planReview.version, 2, 'Daisy 的修订方案应升级到 v2');
  assert.equal(daisy.planReview.seen, false, 'Daisy v1 的查看记录不能批准 v2');
  assert.equal(act(s, {type:'approve-plan', id:daisy.id}).ok, false, 'v2 未再次查看时不能批准');
  assert.equal(act(s, {type:'inspect-plan', id:daisy.id}).ok, true, '用户应能重新查看 Daisy v2');
  assert.equal(act(s, {type:'inspect-plan', id:ethan.id}).ok, true, '用户应能查看 Ethan 方案');
  assert.equal(act(s, {type:'inspect-plan', id:felix.id}).ok, true, '用户应能查看 Felix 方案');
  act(s, {type:'connection'});
  assert.equal(act(s, {type:'approve-all-plans', operation:'review-1'}).ok, false, 'Mac 离线时即使全部已读也不能放行');
  assert.equal(daisy.status, 'plan-review', '离线拒绝后 Daisy 仍应等待审核');
  assert.equal(ethan.status, 'plan-review', '离线拒绝后 Ethan 仍应等待审核');
  assert.equal(felix.status, 'plan-review', '离线拒绝后 Felix 仍应等待审核');
  act(s, {type:'connection'});
  assert.equal(act(s, {type:'approve-plan', id:ethan.id}).ok, true, '重连后应能单独批准 Ethan');
  assert.equal(ethan.status, 'running', '单独批准只应让 Ethan 运行');
  assert.equal(daisy.status, 'plan-review', 'Ethan 获批不能误放行 Daisy');
  assert.equal(act(s, {type:'approve-all-plans', operation:'review-1'}).ok, true, '剩余两份当前方案看过后可以一次放行');
  assert.equal(daisy.status, 'running', '批准后 Daisy 应开始运行');
  assert.equal(ethan.status, 'running', '批准后 Ethan 应开始运行');
  assert.equal(felix.status, 'running', '批准后 Felix 应开始运行');
  assert.equal(s.tasks.find(t => t.id === 'api').status, 'decision', '方案审核不能改变已有 Bella 的决定状态');
  assert.equal(act(s, {type:'approve-plan', id:daisy.id}).ok, false, '已批准方案不能重复放行');
}

/// [iOS 页面层级]：执行空间使用手机底部 Sheet，通话控制只属于 Norma 页面。
///
/// 数据构造（含关键数值的推导过程）：
///   主页面 = Norma、Work、Inbox 共 3 个
///   通话控制可见页面 = 1 个（Norma）
///   项目 Sheet 状态 = closed → projects → closed
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 初始化 → 没有 Sheet
///   2. 打开执行空间 → phoneSheet 进入 projects
///   3. 选择 Norma API → 切到 Work 并关闭 Sheet
///
/// 预期结果：
///   - 正断言：Norma 显示通话控制，执行空间可打开为 projects Sheet
///   - 正断言：选择项目后过滤值为 Norma API，Sheet 已关闭
///   - 负断言：Work、Inbox 和 Agent Card 不得显示持久通话 Dock
{
  const s = initialRealtime();
  assert.equal(s.phoneSheet, null, '初始化时不能残留手机 Sheet');
  assert.equal(showsConversationDock('norma'), true, 'Norma 页面必须保留通话控制');
  assert.equal(showsConversationDock('work'), false, 'Work 页面不能显示占据内容的通话 Dock');
  assert.equal(showsConversationDock('inbox'), false, 'Inbox 页面不能显示占据内容的通话 Dock');
  assert.equal(showsConversationDock('card'), false, 'Agent Card 页面必须使用自己的定向输入栏');
  assert.equal(act(s, {type:'open-conversation'}).ok, true, 'Norma 的文字对话与想法必须能进入手机抽屉');
  assert.equal(s.phoneSheet, 'conversation', 'Norma 首页的次要内容必须收进 conversation Sheet');
  assert.equal(act(s, {type:'close-sheet'}).ok, true, '用户必须能关闭对话抽屉回到虚拟人');
  assert.equal(s.phoneSheet, null, '关闭对话抽屉后不能残留遮罩或 Sheet 状态');
  assert.equal(act(s, {type:'open-projects'}).ok, true, '点击执行空间应打开手机项目 Sheet');
  assert.equal(s.phoneSheet, 'projects', '执行空间必须进入 phone-local projects Sheet 状态');
  assert.equal(act(s, {type:'browse', project:'Norma API'}).ok, true, '应能从 Sheet 选择 Norma API');
  assert.equal(s.viewProject, 'Norma API', '项目选择必须更新 Work 过滤条件');
  assert.equal(s.phoneSheet, null, '选择项目后手机 Sheet 必须关闭');
  assert.match(css, /\.phone button,\.review button\{-webkit-appearance:none;appearance:none;/, '所有手机层级按钮都必须重置浏览器原生外观，包括 screen 外的 Sheet');
  assert.doesNotMatch(css, /\.screen button,\.review button\{/, '按钮基础样式不能只覆盖 screen，否则手机 Sheet 会回退为灰色浏览器按钮');
}

/// [扫码注册电脑]：扫描有效的 Norma 配对码后，必须经用户确认才能新增电脑。
///
/// 数据构造（含关键数值的推导过程）：
///   初始电脑 = Alan Studio + Build Mac = 2 台
///   无效二维码 = https://example.com
///   有效二维码 = norma://pair/studio-mini
///   确认后电脑 = 2 + 1 = 3 台
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开扫码器 → 先扫描无效码，保持扫码状态且不新增
///   2. 扫描有效 Norma 配对码 → 只生成待确认电脑，不立即注册
///   3. 用户确认 → 新增 Studio Mini，并关闭注册流程
///
/// 预期结果：
///   - 正断言：有效码解析出 Studio Mini，确认后电脑总数为 3
///   - 负断言：无效码和仅扫描未确认都不得新增电脑
///   - 负断言：重复确认不能生成第二台 Studio Mini
{
  const s = initialRealtime();
  assert.equal(s.machines.length, 2, '初始必须只有两台已注册电脑');
  assert.equal(act(s, {type:'open-scanner'}).ok, true, '应能从执行空间进入扫码注册');
  assert.equal(s.phoneSheet, 'scanner', '扫码注册必须使用手机全屏扫描界面');
  assert.equal(act(s, {type:'scan-code', code:'https://example.com'}).ok, false, '非 Norma 配对码必须被拒绝');
  assert.equal(s.machines.length, 2, '无效二维码不得新增电脑');
  assert.equal(s.phoneSheet, 'scanner', '无效二维码后应留在扫码界面继续扫描');
  assert.equal(act(s, {type:'scan-code', code:'norma://pair/studio-mini'}).ok, true, '有效 Norma 配对码应进入确认页');
  assert.equal(s.pendingMachine.name, 'Studio Mini', '配对确认页必须显示二维码对应的 Studio Mini');
  assert.equal(s.machines.length, 2, '扫描成功但用户未确认前不能注册电脑');
  assert.equal(act(s, {type:'confirm-pair'}).ok, true, '用户确认后应完成电脑注册');
  assert.equal(s.machines.length, 3, '确认后电脑总数必须从 2 增至 3');
  assert.equal(s.machines.filter(machine => machine.name === 'Studio Mini').length, 1, 'Studio Mini 必须且只能注册一次');
  assert.equal(s.phoneSheet, null, '注册完成后必须退出扫码流程');
  assert.equal(act(s, {type:'confirm-pair'}).ok, false, '没有待确认电脑时不能重复注册');
  assert.equal(s.machines.length, 3, '重复确认不得生成第二台 Studio Mini');
}

/// [首次使用空状态]：没有电脑和项目时不能显示演示数据，配对后仍须等待电脑添加项目。
///
/// 数据构造（含关键数值的推导过程）：
///   演示初始 = 2 台电脑 + 3 个项目 + 4 张卡片。
///   清空后 = 0 台电脑 + 0 个项目 + 0 张卡片。
///   配对 1 台后 = 0 + 1 = 1 台电脑；项目仍为 0 个。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 切到首次使用状态 → 清空电脑、项目和卡片，进入 Work。
///   2. 记录想法 → 想法可保存，但不能向不存在的仓库派工。
///   3. 扫描并确认 Studio Mini → 只注册电脑，不凭空生成项目或卡片。
///
/// 预期结果：
///   - 正断言：空状态与配对后状态分别可区分，想法不会丢失。
///   - 负断言：不能筛选不存在的项目，配对不能自动创建仓库或 Agent Card。
{
  const s = initialRealtime();
  assert.equal(s.projects.length, 3, '常规演示应有 3 个可过滤项目');
  assert.equal(act(s, {type:'show-empty'}).ok, true, '必须能进入首次使用空状态');
  assert.equal(s.page, 'work', '空状态应直接展示 Work 引导页');
  assert.equal(s.machines.length, 0, '空状态不能显示已注册电脑');
  assert.equal(s.projects.length, 0, '空状态不能显示演示项目');
  assert.equal(s.tasks.length, 0, '空状态不能显示演示 Agent Card');
  assert.equal(act(s, {type:'filter', project:'Norma OS'}).ok, false, '不存在的项目不能继续被筛选');
  assert.equal(act(s, {type:'discuss', text:'以后让 Agent 检查发布流程'}).ok, true, '未接电脑时仍应允许保存想法');
  assert.equal(s.thoughts[0].text, '以后让 Agent 检查发布流程', '空状态下保存的想法不能丢失');
  assert.equal(act(s, {type:'dispatch', operation:'empty-1', thoughtId:s.focusThought}).ok, false, '没有电脑和项目时不能派工');
  assert.equal(act(s, {type:'open-scanner'}).ok, true, '空状态应能进入已有扫码流程');
  assert.equal(act(s, {type:'scan-code', code:'norma://pair/studio-mini'}).ok, true, '有效配对码应进入确认');
  assert.equal(act(s, {type:'confirm-pair'}).ok, true, '确认后应注册 Studio Mini');
  assert.equal(s.machines.length, 1, '配对后应有 1 台电脑');
  assert.equal(s.projects.length, 0, '注册电脑不得假装已经添加项目');
  assert.equal(s.tasks.length, 0, '注册电脑不得假装已有 Agent Card');
}

/// Work 默认展示全部卡片，并允许用项目过滤：过滤只改变视图，不改变任务归属。
///
/// 数据构造（含关键数值的推导过程）：
///   Norma OS   = 2 张卡片（Alan、Chloe）
///   Norma API  = 1 张卡片（Bella）
///   Release Ops = 1 张卡片（Bruno）
///   Total      = 2 + 1 + 1 = 4 张卡片
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 初始化 Work → 默认过滤值为 all，4 张卡片都属于当前视图
///   2. 选择 Norma API → 视图只命中 Bella，底层任务仍保持 4 张
///   3. 切回全部 → 过滤值恢复 all
///
/// 预期结果：
///   - 正断言：默认值与切回后的值都为 all
///   - 正断言：Norma API 过滤结果只包含 Bella
///   - 负断言：过滤不得删除 Norma OS 或 Release Ops 的任务
{
  const s = initialRealtime();
  assert.equal(s.viewProject, 'all', 'Work 初始必须选择“全部项目”');
  assert.equal(s.tasks.length, 4, 'Work 默认必须保留全部 4 张 Agent Card');
  assert.equal(act(s, {type:'filter', project:'Norma API'}).ok, true, '选择已知项目应成功更新过滤条件');
  assert.equal(s.viewProject, 'Norma API', 'Work 应记录当前选择的 Norma API 过滤条件');
  const apiCards = s.tasks.filter(t => t.project === s.viewProject);
  assert.equal(apiCards.length, 1, 'Norma API 过滤后应只显示 1 张卡片');
  assert.equal(apiCards[0].cardName, 'Bella', 'Norma API 过滤结果必须包含 Bella');
  assert.equal(s.tasks.length, 4, '项目过滤不能删除其他项目的 Agent Card');
  assert.ok(s.tasks.some(t => t.cardName === 'Alan'), '过滤后底层任务仍应保留 Norma OS 的 Alan');
  assert.ok(s.tasks.some(t => t.cardName === 'Bruno'), '过滤后底层任务仍应保留 Release Ops 的 Bruno');
  assert.equal(act(s, {type:'filter', project:'all'}).ok, true, '选择“全部项目”应恢复总览');
  assert.equal(s.viewProject, 'all', '切回全部后过滤条件必须恢复为 all');
}

/// [Work 处理优先级]：阻塞用户的卡片先于后台执行，项目过滤与状态变化后立即重排。
///
/// 数据构造（含关键数值的推导过程）：
///   初始卡片 = 决定 1 + 验收 1 + 运行中 2 = 4 张。
///   审核派工 = 新增待审核 3 张；总数 = 4 + 3 = 7 张。
///   优先级 = 决定 → 方案待审核 → 成果待验收 → 运行中 → 已验收；同级保持原顺序。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 初始展示 → Bella、Chloe 位于 Alan、Bruno 前。
///   2. 过滤 Norma OS → Chloe 位于 Alan 前，底层卡片顺序不变。
///   3. 创建待审核方案 → Daisy、Ethan、Felix 插到决定与验收之间。
///   4. Bella 决定完成、Chloe 验收完成 → 每次都按新状态重新排序。
///
/// 预期结果：
///   - 正断言：高优先级先显示，同级保持稳定，过滤和状态变化即时生效。
///   - 负断言：排序不能修改底层任务顺序，已验收卡片不能留在最前。
{
  const s = initialRealtime();
  assert.equal(orderedWorkTasks(s).map(t => t.cardName).join(','), 'Bella,Chloe,Alan,Bruno', '初始 Work 应先显示待决定与待验收卡片');
  assert.equal(s.tasks.map(t => t.cardName).join(','), 'Alan,Bella,Bruno,Chloe', '展示排序不能改写底层任务顺序');
  assert.equal(act(s, {type:'filter', project:'Norma OS'}).ok, true, 'Norma OS 项目过滤必须生效');
  assert.equal(orderedWorkTasks(s).map(t => t.cardName).join(','), 'Chloe,Alan', '过滤后的 Norma OS 应先显示待验收卡片');
  assert.equal(act(s, {type:'filter', project:'all'}).ok, true, '切回全部项目必须生效');
  assert.equal(act(s, {type:'dispatch', operation:'priority-1', thoughtId:'thought-1', reviewPlans:true}).ok, true, '审核派工必须创建三张待审核卡片');
  assert.equal(orderedWorkTasks(s).map(t => t.cardName).join(','), 'Bella,Daisy,Ethan,Felix,Chloe,Alan,Bruno', '待审核方案应先于验收和运行中，同级保持派工顺序');
  assert.equal(act(s, {type:'resolve', id:'api'}).ok, true, '处理 Bella 的决定应更新原卡片状态');
  assert.equal(orderedWorkTasks(s)[0].cardName, 'Daisy', 'Bella 恢复运行后不应继续占据最前位置');
  assert.equal(act(s, {type:'inspect', id:'web'}).ok, true, 'Chloe 成果应允许先查看');
  assert.equal(act(s, {type:'accept', id:'web'}).ok, true, 'Chloe 成果应允许验收');
  assert.equal(orderedWorkTasks(s).at(-1).cardName, 'Chloe', '已验收的 Chloe 必须移到列表最后');
  assert.equal(s.tasks.length, 7, '过滤和排序不能删除或复制卡片');
}

/// [Agent Card 手机工作空间]：卡片必须携带可查看的执行方案，并允许用户把补充指令发回原卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   初始卡片 = 4 张
///   Alan 执行方案 = 3 个步骤
///   补充指令 = 1 条「先补登录恢复测试，再处理视觉细节」
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 Alan → 读取目标、3 步方案与当前步骤
///   2. 向 Alan 发送补充指令 → 写回 Alan 的最近指令
///   3. 核对任务集合 → 仍为原 4 张卡片
///
/// 预期结果：
///   - 正断言：Alan 有目标、3 步方案和且仅一个 current 步骤
///   - 正断言：补充指令记录在 Alan，而不是新建卡片
///   - 负断言：Bella 不得收到发给 Alan 的指令
{
  const s = initialRealtime();
  const alan = s.tasks.find(t => t.id === 'voice');
  const bella = s.tasks.find(t => t.id === 'api');
  assert.ok(alan.goal.length > 0, 'Alan 卡片必须提供可查看的任务目标');
  assert.equal(alan.plan.length, 3, 'Alan 卡片必须提供 3 步执行方案');
  assert.equal(alan.plan.filter(step => step.state === 'current').length, 1, 'Alan 方案必须有且仅有一个当前步骤');
  assert.equal(act(s, {type:'message', id:'voice', text:'先补登录恢复测试，再处理视觉细节'}).ok, true, '应能向 Alan 原卡片发送补充指令');
  assert.equal(alan.lastInstruction, '先补登录恢复测试，再处理视觉细节', 'Alan 必须记录用户刚发送的补充指令');
  assert.equal(bella.lastInstruction, undefined, 'Bella 不得收到发给 Alan 的补充指令');
  assert.equal(s.tasks.length, 4, '定向对话不得复制或新建 Agent Card');
}

/// 场景：桌面预览框必须是 iPhone 17 / 17 Pro 逻辑点阵，矮窗口只能等比缩放。
/// 数据构造：2622×1206 @3x → 402×874；比例 402/874。
///   桌面上下留白 88+38=126；满高窗口 1000 的可用高 1000-126=874，缩放=1。
///   720 窗口可用高 720-126=594；缩放=594/874；宽=402×594/874。
/// 执行过程：读取 CSS 常量与预览框公式，分别计算满高与 720 窗口。
/// 预期结果：两档窗口宽高比均为 402/874；720 窗口高度为 594，不得固定 390 宽或单独压到 820。
{
  assert.match(css, /--iphone-w:\s*402\b/, '桌面框宽度必须是 iPhone 17 Pro 逻辑宽度 402');
  assert.match(css, /--iphone-h:\s*874\b/, '桌面框高度必须是 iPhone 17 Pro 逻辑高度 874');
  assert.doesNotMatch(css, /\.phone\{width:390px/, '不能继续使用 iPhone 14/15 的 390 宽');
  assert.doesNotMatch(css, /height:min\(820px/, '不能再单独把高度压到 820，那会破坏 402/874 比例');
  assert.equal(typeof iphone17PreviewFrame, 'function', '必须提供可独立检查的预览框公式');
  const full = iphone17PreviewFrame(1000);
  assert.equal(full.width, 402, '可用高度恰好 874 时应保持 402 宽');
  assert.equal(full.height, 874, '可用高度恰好 874 时应保持 874 高');
  assert.equal(full.scale, 1, '满高窗口不得缩小 iPhone 17 画布');
  const short = iphone17PreviewFrame(720);
  assert.equal(short.height, 594, '720-126=594，矮窗口高度必须钳在可用高度');
  assert.equal(short.width, 402 * 594 / 874, '矮窗口宽度必须按 594/874 同比缩小');
  assert.ok(Math.abs(short.width / short.height - 402 / 874) < 1e-9, '缩放后仍须保持 iPhone 17 的 402/874 比例');
  assert.notEqual(short.width, 390, '矮窗口不得继续使用固定 390 宽');
}

/// 场景：想法默认只记录，只有用户明确确认才升级为跨仓 Mission。
/// 数据构造：初始想法 1 条、Agent Card 4 张；已有卡片均忙，明确执行新建 Daisy/Ethan/Felix 3 张，最终为 4 + 3 = 7 张。
/// 执行过程：记录想法 → 先收起 → 浏览 Norma API → 明确执行 → 重复回执 → 挂断。
/// 预期结果：记录和收起都不派工；明确执行后三仓分流；重复回执不新增；挂断不取消任务。
{
  const s = initialRealtime();
  assert.equal(s.tasks.find(t => t.id === 'voice').cardName, 'Alan', '已有 Codex 卡片必须显示稳定名称 Alan');
  assert.equal(s.tasks.find(t => t.id === 'api').cardName, 'Bella', '已有 Claude 卡片必须显示稳定名称 Bella');
  assert.equal(s.tasks.find(t => t.id === 'voice').title, '修复 iOS 登录恢复与 Keychain 回归', '卡片动态标题必须概括当前正在做的事');
  assert.notEqual(s.tasks.find(t => t.id === 'voice').cardName, s.tasks.find(t => t.id === 'voice').agent, '卡片名称不能退化为 Agent Provider 名');
  assert.equal(s.thoughts.length, 1, '初始应有一条已保存想法作为示例');
  act(s, {type:'discuss', text:'也许可以让 Norma 在通勤时提醒我没想完的事'});
  const thoughtId = s.focusThought;
  assert.equal(s.thoughts.length, 2, '新想法必须被记录');
  assert.equal(s.thoughts[0].status, 'saved', '新想法默认必须是未执行状态');
  assert.equal(s.tasks.length, 4, '记录想法不能创建任务');
  act(s, {type:'shelve'});
  assert.equal(s.focusThought, null, '先收起应关闭当前行动提示');
  assert.equal(s.thoughts.find(t => t.id === thoughtId).status, 'saved', '收起不能丢失或执行想法');
  act(s, {type:'browse', project:'Norma API'});
  assert.equal(s.mission, '秋季发布', '浏览仓库不能切换当前 Mission');
  assert.equal(act(s, {type:'dispatch', operation:'op-1', thoughtId}).ok, true, '明确执行应返回已接收');
  assert.equal(s.tasks.length, 7, '新建 Daisy/Ethan/Felix 后应为 4 + 3 = 7 张卡片');
  assert.equal(new Set(s.tasks.map(t => t.cardName)).size, 7, '每张 Agent Card 必须拥有唯一稳定名称');
  assert.equal(s.thoughts.find(t => t.id === thoughtId).status, 'mission', '被确认的想法应留下已升级标记');
  assert.equal(s.tasks[0].project, 'Norma OS', 'iOS 任务必须进入 Norma OS');
  assert.equal(s.tasks[1].project, 'Norma API', '聚合接口任务必须进入 Norma API');
  assert.equal(s.tasks[2].project, 'Release Ops', '发布检查任务必须进入 Release Ops');
  assert.equal(s.tasks[0].cardName, 'Daisy', 'iOS 任务必须派给新建 Daisy 卡片');
  assert.equal(s.tasks[0].cardRoute, 'create', 'Daisy 不存在时必须明确新建卡片');
  assert.equal(s.tasks[1].cardName, 'Ethan', 'API 任务必须派给新建 Ethan 卡片');
  assert.equal(s.tasks[1].cardRoute, 'create', 'Ethan 不存在时必须明确新建卡片');
  assert.equal(s.tasks[2].cardName, 'Felix', '发布任务必须派给新建 Felix 卡片');
  assert.equal(s.tasks[2].cardRoute, 'create', 'Felix 不存在时必须明确新建卡片');
  assert.equal(s.tasks[0].machine, 'Alan Studio', 'iOS 任务必须绑定 Alan Studio');
  assert.equal(s.tasks[1].machine, 'Build Mac', 'API 任务必须绑定 Build Mac');
  assert.notEqual(s.tasks[0].project, s.tasks[1].project, '跨仓拆解不能把所有任务误派到同一仓库');
  act(s, {type:'dispatch', operation:'op-1', thoughtId});
  assert.equal(s.tasks.length, 7, '同一回执重试不得重复创建卡片');
  act(s, {type:'hangup'});
  assert.equal(s.tasks[0].status, 'running', '挂断不能取消已派任务');
  assert.equal(s.call, false, '挂断后通话必须结束');
}

/// 场景：Mac 离线仍能记录想法，重连不会自动执行。
/// 数据构造：初始 Agent Card 4 张；离线记录一条想法，显式执行后新建 3 张，共 4 + 3 = 7 张。
/// 执行过程：Mac 离线 → 记录想法与尝试执行 → 重连 → 显式执行。
/// 预期结果：离线执行被拒绝；想法不丢失；重连不自动新增任务。
{
  const s = initialRealtime();
  act(s, {type:'connection'});
  act(s, {type:'discuss', text:'保留离线讨论'});
  const thoughtId = s.focusThought;
  assert.equal(s.call, false, '文字记录想法不应偷偷开启语音会话');
  assert.equal(act(s, {type:'dispatch', operation:'offline', thoughtId}).ok, false, '离线不能声称派工成功');
  assert.equal(s.thoughts.find(t => t.id === thoughtId).text, '保留离线讨论', '离线想法必须保留');
  assert.equal(s.thoughts.find(t => t.id === thoughtId).status, 'saved', '离线想法不能被误标为 Mission');
  act(s, {type:'connection'});
  assert.equal(s.tasks.length, 4, '重连不能自动提交草稿');
  act(s, {type:'dispatch', operation:'online', thoughtId});
  assert.equal(s.tasks.length, 7, '明确继续后应新建三张具名 Agent Card');
  assert.equal(s.thoughts.find(t => t.id === thoughtId).status, 'mission', '产生接收回执后才能升级想法');
}

/// 场景：界面成果必须经用户查看，退回修改沿用原任务。
/// 数据构造：web 待验收，voice/release 运行中，api 待决定，共 4 条任务；一条具体修改意见。
/// 执行过程：越序验收 → 查看 web → 验收 → 重复验收；重新构造状态后退回 web。
/// 预期结果：越序和重复请求失败；查看后才能验收；反馈只更新原任务。
{
  const s = initialRealtime();
  assert.equal(act(s, {type:'accept', id:'voice'}).ok, false, '运行中不能验收');
  assert.equal(act(s, {type:'accept', id:'web'}).ok, false, '未打开成果不能算用户已查看');
  act(s, {type:'inspect', id:'web'});
  assert.equal(act(s, {type:'accept', id:'web'}).ok, true, '查看成果后应能验收');
  assert.equal(s.tasks.find(t=>t.id==='web').status, 'done', 'web 应留下用户验收状态');
  assert.equal(act(s, {type:'accept', id:'web'}).ok, false, '同一成果不能重复验收');
  const retry = initialRealtime();
  act(retry, {type:'inspect', id:'web'});
  act(retry, {type:'feedback', id:'web', text:'增大卡片间距'});
  assert.equal(retry.tasks.find(t=>t.id==='web').status, 'running', '反馈必须让原任务继续修改');
  assert.equal(retry.tasks.find(t=>t.id==='web').seen, false, '新版本不能沿用旧查看状态');
  assert.equal(retry.tasks.find(t=>t.id==='web').feedback, '增大卡片间距', '调整意见必须保留');
  assert.equal(retry.tasks.length, 4, '退回修改不得复制任务');
}

/// 场景：注意力收件箱中的决定必须回到原 Agent，且不能误改其他任务。
/// 数据构造：api 为唯一待决定任务；其余 3 条分别为运行中或待验收。
/// 执行过程：对运行中 voice 越权决定 → 对 api 采用建议。
/// 预期结果：越权请求失败；api 恢复运行；voice 与 web 状态保持不变。
{
  const s = initialRealtime();
  assert.equal(act(s, {type:'resolve', id:'voice'}).ok, false, '运行中的任务不能接受决定操作');
  assert.equal(act(s, {type:'resolve', id:'api'}).ok, true, '待决定任务应能采用 Agent 建议继续');
  assert.equal(s.tasks.find(t=>t.id==='api').status, 'running', '决定必须让原 api 任务继续运行');
  assert.equal(s.tasks.find(t=>t.id==='voice').status, 'running', '决定不能改变无关的 voice 任务');
  assert.equal(s.tasks.find(t=>t.id==='web').status, 'review', '决定不能跳过待验收成果');
}
console.log('PASS · 想法记录、明确升级、跨仓拆解、离线保留、决策与验收；全部脚本语法通过');
