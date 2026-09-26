// CUA E2E for the isolated HTML prototype. Run with an authorized preview Tab.
// Call testNormaRealtimePrototype(tab) from the CUA session after opening the page.
// This exercises mock UI only; it does not certify native iOS audio or Mac execution.
const check = (condition, message) => { if (!condition) throw new Error(message); };
const button = (tab, name) => tab.playwright.getByRole('button', {name, exact:true});

/// 场景：较矮桌面窗口也必须完整显示手机底部导航，并保持 iPhone 17 比例。
/// 数据构造：桌面视口高 720px；外层上下留白 88 + 38 = 126px，手机应不高于 594px。
///   iPhone 17 Pro 逻辑点阵 402×874；720 窗口缩放 = 594/874，宽高比仍为 402/874。
/// 执行过程：保持桌面宽度 → 读取手机尺寸与导航边界 → 与视口底边、402/874 比较。
/// 预期结果：手机、底部导航与陪伴页主文案均可见；不产生整页纵向溢出；比例不被单独压扁。
export async function testNormaRealtimeDesktopLayout(tab) {
  const layout = await tab.playwright.evaluate(() => {
    const phone = document.querySelector('.phone').getBoundingClientRect();
    return {
      phoneBottom: phone.bottom,
      phoneWidth: phone.width,
      phoneHeight: phone.height,
      navBottom: document.querySelector('.bottom-nav').getBoundingClientRect().bottom,
      height: innerHeight,
      documentHeight: document.documentElement.scrollHeight,
      quoteBottom: document.querySelector('.quote')?.getBoundingClientRect().bottom,
      mainBottom: document.querySelector('.main').getBoundingClientRect().bottom,
    };
  });
  check(layout.phoneBottom <= layout.height, '手机外框不能被桌面窗口底部截断');
  check(layout.navBottom <= layout.height, '底部导航必须完整可见');
  check(layout.documentHeight <= layout.height, '正常桌面高度不应因手机最小高度产生整页滚动');
  check(Math.abs(layout.phoneWidth / layout.phoneHeight - 402 / 874) < 0.01, '桌面预览必须保持 iPhone 17 Pro 402/874 比例');
  if (layout.quoteBottom !== undefined) check(layout.quoteBottom <= layout.mainBottom, '角色不能挤掉陪伴页主文案');
  return '较矮桌面窗口的手机外框、iPhone 17 比例与底部导航：通过';
}

/// 场景：Norma 先记录想法，只有用户明确确认才升级为跨仓 Mission。
/// 数据构造：初始 1 条已保存想法和 4 张忙碌的 Agent Card；明确执行后新建 Daisy/Ethan/Felix，共 4 + 3 = 7 张。
/// 执行过程：输入想法 → 明确升级 → 查看全部卡片 → 按项目过滤 → 进入 Alan 全屏工作空间 → 查看方案 → 定向补充 → 返回 Work。
/// 预期结果：默认展示 7 张卡片，决定与验收优先于运行中；项目过滤保持优先级；卡片详情使用手机层级导航而非弹窗，并保留方案、定向对话与原列表位置。
export async function testNormaRealtimePrototype(tab) {
  const idea = '想让 Norma 记住我在通勤时冒出来的点子。';
  const initialTasks = await tab.playwright.locator('.task-row').count();
  check(await tab.playwright.locator('.norma-stage').isVisible(), 'Norma 首页必须以虚拟人为唯一主内容');
  check(await tab.playwright.locator('.norma-main .composer').count() === 0, '文字输入不能继续堆在虚拟人主画面');
  await button(tab, '打开对话与想法').click();
  check(await tab.playwright.locator('.conversation-sheet').isVisible(), '对话、想法与 Mission 入口必须收进手机底部抽屉');
  await tab.playwright.getByRole('textbox', {name:'告诉 Norma 你的想法'}).fill(idea);
  await button(tab, '告诉 Norma').click();
  check(await tab.playwright.getByText('想法已记录 · 尚未执行', {exact:true}).isVisible(), '输入后必须明确告知用户未执行');
  check(await tab.playwright.getByText(idea, {exact:true}).count() >= 1, '刚记下的想法必须可见');
  check(initialTasks === 0, 'Norma 首页不应混入任务列表');
  await button(tab, '先放着').click();
  check(await button(tab, `查看想法：${idea}`).isVisible(), '收起想法后必须仍能在记忆列表中找到');
  await button(tab, `查看想法：${idea}`).click();
  await tab.playwright.locator('.phone').getByRole('button', {name:'查看 Agent Card 派工计划', exact:true}).click();
  check(await tab.playwright.getByText('Daisy · Codex', {exact:true}).isVisible(), '派工计划必须显示将新建的 Daisy 卡片');
  check(await tab.playwright.getByText('Ethan · Claude', {exact:true}).isVisible(), '派工计划必须显示将新建的 Ethan 卡片');
  check(await tab.playwright.getByText('Felix · Codex', {exact:true}).isVisible(), '派工计划必须显示将新建的 Felix 卡片');
  check(await tab.playwright.getByText('新建卡片', {exact:true}).count() === 3, '三个并行 Agent 必须明确标记新建卡片');
  await tab.playwright.getByRole('checkbox', {name:'执行前先让我审核方案'}).uncheck();
  await button(tab, '确认并开始执行').click();
  check(await tab.playwright.locator('.phone-notice').innerText() === 'Daisy、Ethan、Felix 已创建并接收 · 3 张 Agent Card', '明确执行后必须展示具名卡片接收回执');
  check(await tab.playwright.locator('.call-dock').count() === 0, 'Work 页面不能保留 Norma 通话 Dock');
  check(await tab.playwright.locator('.task-row').count() === 7, '明确执行后应显示 4 + 3 = 7 张 Agent Card');
  const workOrder = await tab.playwright.evaluate(() => Array.from(document.querySelectorAll('.phone .task-row .card-name')).map(el => el.textContent.split(' · ')[0]).join(','));
  check(workOrder === 'Bella,Chloe,Daisy,Ethan,Felix,Alan,Bruno', 'Work 应将待决定、待验收放在运行中之前，并保持同级稳定顺序');
  check(await tab.playwright.getByText('7 张 · 按处理优先级', {exact:true}).isVisible(), 'Work 应向用户解释卡片排序依据');
  check(await button(tab, '全部 7').getAttribute('aria-pressed') === 'true', 'Work 默认必须选中“全部”过滤条件');
  check(await tab.playwright.getByText('实现 iOS 多任务入口与状态反馈', {exact:true}).isVisible(), 'Daisy 卡片必须显示 Norma 概括的当前任务');
  check(await tab.playwright.getByText('补齐跨仓库任务聚合接口', {exact:true}).isVisible(), 'Ethan 卡片必须显示 Norma 概括的当前任务');
  check(await tab.playwright.getByText('验证发布前的跨仓检查', {exact:true}).isVisible(), 'Felix 卡片必须显示 Norma 概括的当前任务');
  await button(tab, 'Norma API 2').click();
  check(await tab.playwright.locator('.task-row').count() === 2, 'Norma API 过滤后必须只显示 Bella 与 Ethan 两张卡片');
  const apiOrder = await tab.playwright.evaluate(() => Array.from(document.querySelectorAll('.phone .task-row .card-name')).map(el => el.textContent.split(' · ')[0]).join(','));
  check(apiOrder === 'Bella,Ethan', '项目过滤后仍应让待决定的 Bella 排在运行中的 Ethan 前');
  check(await tab.playwright.getByText('Bella · Claude', {exact:true}).isVisible(), 'Norma API 过滤结果必须包含 Bella');
  check(await tab.playwright.getByText('Ethan · Claude', {exact:true}).isVisible(), 'Norma API 过滤结果必须包含 Ethan');
  check(await tab.playwright.getByText('Alan · Codex', {exact:true}).count() === 0, 'Norma API 过滤结果不能混入 Norma OS 的 Alan');
  await button(tab, '全部 7').click();
  check(await tab.playwright.locator('.task-row').count() === 7, '切回全部后必须恢复 7 张 Agent Card');
  await button(tab, '查看 Agent Card：Alan · 修复 iOS 登录恢复与 Keychain 回归').click();
  check(await tab.playwright.locator('.card-main[aria-label="Agent Card 工作空间"]').isVisible(), '点击卡片必须进入全屏 Agent Card 工作空间');
  check(await tab.playwright.locator('dialog[open]').count() === 0, 'Agent Card 详情不能继续使用桌面式模态弹窗');
  check(await tab.playwright.locator('.phone').getByRole('button', {name:/模拟/}).count() === 0, 'Agent Card 内不能暴露原型模拟控制');
  check(await tab.playwright.locator('.bottom-nav').count() === 0, '进入 Agent Card 后应隐藏全局底部导航，保留明确返回路径');
  check(await tab.playwright.getByText('修正 Keychain 恢复并补回归测试', {exact:true}).isVisible(), '概览必须直接显示当前执行步骤');
  await tab.playwright.getByRole('tab', {name:'方案', exact:true}).click();
  check(await tab.playwright.getByText('恢复 iOS 登录态，并用回归测试守住 Keychain 恢复路径。', {exact:true}).isVisible(), '方案页必须显示任务目标');
  check(await tab.playwright.locator('.plan-step').count() === 3, 'Alan 的完整方案必须显示 3 个步骤');
  const instruction = '先补登录恢复测试，再处理视觉细节';
  await tab.playwright.getByRole('textbox', {name:'告诉 Alan', exact:true}).fill(instruction);
  await button(tab, '发送给 Alan').click();
  check(await tab.playwright.getByText('已交给 Alan · 原卡片继续', {exact:true}).isVisible(), '定向补充必须确认已回到 Alan 原卡片');
  await tab.playwright.getByRole('tab', {name:'动态', exact:true}).click();
  check(await tab.playwright.getByText(instruction, {exact:true}).isVisible(), '关键动态必须保留刚发给 Alan 的补充指令');
  await button(tab, '返回 Work').click();
  check(await tab.playwright.locator('.task-row').count() === 7, '返回 Work 后不得丢失或复制 Agent Card');
  const errors = await tab.dev.logs({levels:['error'], limit:20});
  check(errors.length === 0, '想法记录与跨仓升级流程不得产生控制台错误');
  return '想法记录、收起、明确升级与三仓派工：通过';
}

/// [跨仓方案审核]：用户在手机上逐份看方案、退回修改、单独放行，再批准剩余方案。
///
/// 数据构造（含关键数值的推导过程）：
///   已有 Agent Card = 4 张；新建 Daisy/Ethan/Felix = 3 张；总数 = 4 + 3 = 7 张。
///   Inbox 初始待办 = 1 组方案 + Bella 决定 + Chloe 成果 = 3 件。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 已存想法派给三张新卡片，并打开执行前方案审核。
///   2. Daisy v1 退回修改 → 查看 v2；Ethan 单独批准；Felix 标记已读。
///   3. 一次批准剩余两份方案 → 返回 Work 查看执行状态。
///
/// 预期结果：
///   - 正断言：三份方案集中审核，v2 与单独批准、剩余批量批准都可见。
///   - 负断言：未查看时不能批量批准，批准前不能显示运行中；不新增重复卡片。
export async function testNormaPlanReview(tab) {
  await button(tab, '打开对话与想法').click();
  await button(tab, '查看想法：也许可以让 Norma 在通勤时，提醒我昨天没想完的事。').click();
  await tab.playwright.locator('.phone').getByRole('button', {name:'查看 Agent Card 派工计划', exact:true}).click();
  check(await tab.playwright.getByRole('checkbox', {name:'执行前先让我审核方案'}).isVisible(), '派工前必须能选择审核方案');
  await button(tab, '创建卡片并收取方案').click();
  check(await tab.playwright.getByRole('heading', {name:'3 件待办'}).isVisible(), 'Inbox 应把三张待审核卡片合并为 1 件事');
  await button(tab, 'Work').click();
  const reviewOrder = await tab.playwright.evaluate(() => Array.from(document.querySelectorAll('.phone .task-row .card-name')).map(el => el.textContent.split(' · ')[0]).join(','));
  check(reviewOrder === 'Bella,Daisy,Ethan,Felix,Chloe,Alan,Bruno', '待审核方案应显示在成果验收与运行中卡片之前');
  await button(tab, 'Inbox').click();
  check(await button(tab, '3 3 个 Agent 的方案待审核 查看每份方案，再决定哪些可以开始').isVisible(), 'Inbox 应出现 3 份方案的聚合入口');
  await button(tab, '3 3 个 Agent 的方案待审核 查看每份方案，再决定哪些可以开始').click();
  check(!await button(tab, '全部批准并执行').isEnabled(), '未查看三份方案时不能一键放行');
  await button(tab, '查看 Daisy 的方案 v1').click();
  check(await tab.playwright.getByText('方案 v1 · 待你审核。批准前仅允许分析，不开始修改。', {exact:true}).isVisible(), 'Daisy 首版必须明确停在审核门禁');
  await tab.playwright.getByRole('textbox', {name:'要求 Agent 修改方案'}).fill('先补登录恢复测试，再调整界面');
  await button(tab, '提交修改意见').click();
  check(await tab.playwright.getByText('v2 调整：先补登录恢复测试，再调整界面', {exact:true}).isVisible(), '修改应回到 Daisy 原卡片并形成 v2');
  await button(tab, '已阅读，返回方案汇总').click();
  check(await button(tab, '查看 Daisy 的方案 v2').isVisible(), '聚合审核应显示 Daisy 当前版本 v2');
  check(!await button(tab, '全部批准并执行').isEnabled(), '只查看 Daisy 后仍不能批量批准');
  await button(tab, '查看 Ethan 的方案 v1').click();
  await button(tab, '批准这份方案并执行').click();
  check(await tab.playwright.getByRole('heading', {name:'2 份方案待你审核'}).isVisible(), 'Ethan 单独批准后应只剩 Daisy 与 Felix');
  check(await button(tab, '查看 Ethan 的方案 v1').count() === 0, '已批准的 Ethan 不应继续出现在待审核列表');
  await button(tab, '查看 Felix 的方案 v1').click();
  await button(tab, '已阅读，返回方案汇总').click();
  check(await button(tab, '全部批准并执行').isEnabled(), '剩余 Daisy 与 Felix 都看过当前版本后应允许批准');
  await button(tab, '全部批准并执行').click();
  check(await button(tab, '3 3 个 Agent 的方案待审核 查看每份方案，再决定哪些可以开始').count() === 0, '批准后 Inbox 不应残留方案审核入口');
  await button(tab, 'Work').click();
  check(await tab.playwright.locator('.task-row').count() === 7, '审核流程必须保留 4 + 3 张 Agent Card，不复制任务');
  check(await button(tab, '查看 Agent Card：Daisy · 实现 iOS 多任务入口与状态反馈').innerText().then(text=>text.includes('进行中')), 'Daisy 的 v2 批准后应运行');
  check(await button(tab, '查看 Agent Card：Ethan · 补齐跨仓库任务聚合接口').innerText().then(text=>text.includes('进行中')), 'Ethan 单独批准后应运行');
  check(await button(tab, '查看 Agent Card：Felix · 验证发布前的跨仓检查').innerText().then(text=>text.includes('进行中')), 'Felix 批量批准后应运行');
  const errors = await tab.dev.logs({levels:['error'], limit:20});
  check(errors.length === 0, '方案审核流程不能产生控制台错误');
  return '三份方案聚合审核、单独批准、退回 v2、批量批准：通过';
}

/// [iOS 手机交互与扫码注册]：窄屏以大尺寸 Norma 为主，并用系统式 Sheet 完成电脑注册。
///
/// 数据构造（含关键数值的推导过程）：
///   视口 = 390 × 844 px
///   最小触控目标 = 44 × 44 px
///   初始电脑 = Alan Studio + Build Mac = 2 台
///   注册后电脑 = 2 + Studio Mini = 3 台
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 检查 Norma 主画面 → 只有大尺寸虚拟人与精简入口
///   2. 打开执行空间 → 使用 phone-local Sheet，选择扫码注册
///   3. 点击演示配对码 → 显示 Studio Mini 权限确认页
///   4. 允许注册 → 回到 App，再次打开执行空间核对电脑列表
///
/// 预期结果：
///   - 正断言：虚拟人占据主要内容，所有关键触控目标不小于 44px
///   - 正断言：确认后 Studio Mini 出现在已注册电脑，顶部显示 3 台电脑
///   - 负断言：扫码流程不得打开网页 dialog，也不得横向溢出
export async function testNormaRealtimeMobile(tab) {
  const presenceHeight = await tab.playwright.evaluate(() => document.querySelector('.norma-stage').getBoundingClientRect().height);
  check(presenceHeight >= 360, 'Norma 人物在手机首页必须占据主要内容区域');
  await button(tab, '打开执行空间').click();
  check(await tab.playwright.locator('.phone-sheet[aria-labelledby="projects-title"]').isVisible(), '执行空间必须使用手机底部 Sheet');
  check(await tab.playwright.locator('dialog[open]').count() === 0, '执行空间不能打开网页 dialog');
  const rowStyle = await tab.playwright.locator('.phone-sheet [data-project="all"]').evaluate(el => ({appearance:getComputedStyle(el).appearance,background:getComputedStyle(el).backgroundColor}));
  check(rowStyle.appearance === 'none', 'Sheet 按钮必须移除浏览器原生按钮外观');
  check(rowStyle.background === 'rgba(0, 0, 0, 0)', '未按下的项目行必须保持透明，不能出现整块灰色按钮底');
  const targets = await tab.playwright.evaluate(() => Array.from(document.querySelectorAll('.phone-sheet button')).map(b => ({width:b.getBoundingClientRect().width,height:b.getBoundingClientRect().height})));
  check(targets.length >= 6, '执行空间必须包含项目选择、关闭与扫码入口');
  check(targets.every(b => b.width >= 44 && b.height >= 44), '执行空间内所有按钮必须满足 44px 触控目标');
  await button(tab, '扫描注册电脑').click();
  check(await tab.playwright.getByText('扫描电脑上的 Norma 配对码', {exact:true}).isVisible(), '扫码器必须说明二维码来源');
  await button(tab, '扫描 Studio Mini 配对码').click();
  check(await tab.playwright.getByRole('heading', {name:'Studio Mini', exact:true}).isVisible(), '扫描成功后必须先显示待注册电脑身份');
  check(await tab.playwright.getByText('Norma 不会在未确认时向这台电脑派发任务。', {exact:true}).isVisible(), '确认页必须解释授权边界');
  await button(tab, '允许并注册').click();
  check(await tab.playwright.getByText('Studio Mini 已注册', {exact:true}).isVisible(), '确认后必须给出注册成功反馈');
  check(await button(tab, '打开执行空间').innerText().then(text => text.includes('3 台电脑')), '注册后顶部执行空间必须显示 3 台电脑');
  await button(tab, '打开执行空间').click();
  check(await tab.playwright.getByText('Studio Mini', {exact:true}).isVisible(), '已注册电脑列表必须包含 Studio Mini');
  const overflow = await tab.playwright.evaluate(() => document.documentElement.scrollWidth > innerWidth);
  check(!overflow, '手机视口不能横向溢出');
  return '手机大角色、系统式执行空间与扫码注册：通过';
}

/// [首次使用执行空间]：没有电脑和项目时展示明确的下一步，配对后不伪造项目。
///
/// 数据构造（含关键数值的推导过程）：
///   常规演示 = 2 台电脑 + 3 个项目 + 4 张卡片。
///   首次使用 = 0 台电脑 + 0 个项目 + 0 张卡片。
///   扫码注册后 = 0 + 1 = 1 台电脑；项目仍为 0 个。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 切到首次使用 → Work 显示扫码引导，隐藏演示项目和任务。
///   2. 扫描配对码并确认 → 回到 Work，提示在电脑端添加项目。
///   3. 打开执行空间 → 只显示已注册电脑，不显示不存在的项目。
///
/// 预期结果：
///   - 正断言：两个空状态分别有清晰标题和下一步，扫码流程可完成。
///   - 负断言：空状态没有卡片、过滤项或虚构的项目；配对不等于已添加项目。
export async function testNormaEmptyWorkspace(tab) {
  await button(tab, '首次使用（空状态）').click();
  check(await tab.playwright.getByRole('heading', {name:'先连接一台电脑'}).isVisible(), '无电脑时 Work 应展示独立的首次使用引导');
  check(await button(tab, '扫描注册电脑').isVisible(), '首次使用页必须直接进入扫码注册');
  check(await tab.playwright.locator('.task-row').count() === 0, '没有项目时不能出现演示 Agent Card');
  check(await tab.playwright.locator('.project-filters').count() === 0, '没有项目时不能出现虚构的项目过滤');
  await button(tab, '扫描注册电脑').click();
  check(await tab.playwright.getByRole('heading', {name:'扫描配对码'}).isVisible(), '空状态扫码应沿用手机全屏扫描层级');
  await button(tab, '扫描 Studio Mini 配对码').click();
  await button(tab, '允许并注册').click();
  check(await tab.playwright.getByRole('heading', {name:'电脑已连接，等待项目'}).isVisible(), '配对后应进入无项目引导，而不是填充示例仓库');
  check(await tab.playwright.getByText('在 Studio Mini 的 Norma 中添加项目。项目和 Agent Card 会同步到这里。', {exact:true}).isVisible(), '无项目页应说明项目从哪台电脑添加');
  check(await tab.playwright.locator('.task-row').count() === 0, '配对后仍不能虚构 Agent Card');
  await button(tab, '先和 Norma 记下想法').click();
  check(await button(tab, '打开对话与想法').isVisible(), '没有项目时也应能进入 Norma，不得困在设置页');
  await button(tab, 'Work').click();
  await button(tab, '打开执行空间').click();
  check(await tab.playwright.getByText('Studio Mini', {exact:true}).isVisible(), '执行空间应显示刚注册的电脑');
  check(await tab.playwright.locator('.phone-sheet [data-project="Norma OS"]').count() === 0, '执行空间不能列出尚未同步的 Norma OS');
  return '首次使用扫码、配对后无项目引导与真实空列表：通过';
}

/// [微信式模拟语音输入]：Norma 对话输入可切换为按住说话，上滑取消，松开发送。
///
/// 数据构造（含关键数值的推导过程）：
///   初始已存想法 = 1 条；取消后仍为 1 条；成功发送后 = 1 + 1 = 2 条。
///   取消位移 = 向上 110px，大于交互阈值 70px。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开对话抽屉并切换到语音 → 显示按住说话与模拟说明。
///   2. 按住后上滑 110px 再松开 → 取消，不记录想法。
///   3. 再次按住并松开 → 记录一条标明模拟来源的想法。
///   4. 切回键盘 → 原文字输入重新可用。
///
/// 预期结果：
///   - 正断言：切换、上滑取消、松开发送、返回文字输入均有明确反馈。
///   - 负断言：取消不能新增想法；语音模式不能误显示文字输入或声称采集麦克风。
export async function testNormaVoiceInput(tab) {
  await button(tab, '打开对话与想法').click();
  await tab.playwright.getByRole('textbox', {name:'告诉 Norma 你的想法'}).fill('还没发送的文字草稿');
  await button(tab, '切换到语音输入').click();
  check(await button(tab, '按住说话').isVisible(), '语音模式必须提供按住说话的大触控区');
  check(await tab.playwright.getByText('交互演示 · 不采集麦克风', {exact:true}).isVisible(), '语音演示必须在手机内说明没有采集麦克风');
  check(await tab.playwright.locator('#idea-input').count() === 0, '语音模式不能同时显示文字输入框');
  const position = await tab.playwright.evaluate(() => {
    const box = document.querySelector('.hold-voice').getBoundingClientRect();
    return [box.left + box.width / 2, box.top + box.height / 2];
  });
  await tab.drag(position, [position[0], position[1] - 110]);
  check(await tab.playwright.locator('.voice-result').innerText() === '已取消语音输入', '上滑松开必须在对话抽屉内提示已取消');
  check(await tab.playwright.locator('.thought-list .thought-row').count() === 1, '取消不能新增想法');
  const sendPosition = await tab.playwright.evaluate(() => {
    const box = document.querySelector('.hold-voice').getBoundingClientRect();
    return [box.left + box.width / 2, box.top + box.height / 2];
  });
  await tab.drag(sendPosition, [sendPosition[0] + 2, sendPosition[1]]);
  check(await tab.playwright.locator('.voice-result').innerText() === '想法已记录 · 尚未执行', '松开发送必须在对话抽屉内展示想法记录回执');
  check(await tab.playwright.getByText('【模拟语音】我想让 Norma 帮我记下这个想法。', {exact:true}).count() >= 1, '发送的演示语音必须标明模拟来源');
  await button(tab, '切换到文字输入').click();
  check(await tab.playwright.getByRole('textbox', {name:'告诉 Norma 你的想法'}).isVisible(), '切回键盘后文字输入必须可用');
  check(await tab.playwright.getByRole('textbox', {name:'告诉 Norma 你的想法'}).getAttribute('value') === '还没发送的文字草稿', '语音与文字切换不能丢失未发送草稿');
  check(await button(tab, '按住说话').count() === 0, '文字模式不能残留按住说话按钮');
  return '微信式语音切换、上滑取消、松开发送与文字回退：通过';
}

/// [Agent Card 定向模拟语音输入]：Chloe 卡片底栏沿用按住说话，输入只回 Chloe 原卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   初始 Agent Card = 4 张，其中 Chloe 待验收、Alan 运行中。
///   取消位移 = 向上 110px，大于 70px 取消阈值；发送后卡片数仍为 4。
///   Chloe 的文字草稿 = 1 条；Alan 的文字草稿 = 0 条。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从 Work 进入 Chloe → 空输入时隐藏发送动作；输入文字后出现发送动作，再切换到语音。
///   2. 上滑松开 → 取消，不向 Chloe 发指令。
///   3. 再次按住并松开 → 将标明模拟来源的指令发给 Chloe，动态可见。
///   4. 切回文字、访问 Alan、再回 Chloe → 草稿只留在 Chloe。
///
/// 预期结果：
///   - 正断言：输入与触控目标至少 44px，文字输入是独立圆角区域；卡片底栏有语音入口、取消反馈、定向发送回执与保留的 Chloe 草稿。
///   - 负断言：取消无指令；Alan 不收到 Chloe 的语音或文字草稿；不新建 Agent Card。
export async function testAgentCardVoiceInput(tab) {
  await button(tab, 'Work').click();
  await button(tab, '查看 Agent Card：Chloe · 等待你验收新版任务总览').click();
  check(await tab.playwright.locator('#agent-form button[type="submit"]').isVisible() === false, '空输入不能持续显示高对比度发送按钮');
  await tab.playwright.getByRole('textbox', {name:'告诉 Chloe'}).fill('Chloe 的未发送草稿');
  check(await button(tab, '发送给 Chloe').isVisible(), '输入文字后应出现明确的发送动作');
  const composer = await tab.playwright.evaluate(() => {
    const input = document.querySelector('#agent-input');
    const mic = document.querySelector('#agent-form .mode-toggle');
    const send = document.querySelector('#agent-form button[type="submit"]');
    return {inputHeight:input.getBoundingClientRect().height,micWidth:mic.getBoundingClientRect().width,sendWidth:send.getBoundingClientRect().width,inputRadius:parseFloat(getComputedStyle(input).borderTopLeftRadius),inputOutline:getComputedStyle(input).outlineStyle};
  });
  check(composer.inputHeight >= 44 && composer.micWidth >= 44 && composer.sendWidth >= 44, '输入框和两侧按钮必须保留至少 44px 的触控尺寸');
  check(composer.inputRadius >= 18 && composer.inputOutline === 'none', '输入焦点应融入圆角输入区，不能出现醒目的方形内框');
  await button(tab, '切换 Chloe 到语音输入').click();
  check(await button(tab, '按住说话，发给 Chloe').isVisible(), 'Chloe 底栏必须提供定向按住说话控件');
  check(await tab.playwright.getByText('交互演示 · 不采集麦克风 · 发给 Chloe', {exact:true}).isVisible(), '卡片内必须说明模拟性质和接收 Agent');
  check(await tab.playwright.locator('#agent-input').count() === 0, '语音模式不能同时显示 Chloe 文字输入框');
  const cancelPosition = await tab.playwright.evaluate(() => {
    const box = document.querySelector('.agent-dock .hold-voice').getBoundingClientRect();
    return [box.left + box.width / 2, box.top + box.height / 2];
  });
  await tab.drag(cancelPosition, [cancelPosition[0], cancelPosition[1] - 110]);
  check(await tab.playwright.locator('.agent-dock .voice-result').innerText() === '已取消语音输入', '上滑取消必须在 Chloe 底栏反馈');
  await tab.playwright.getByRole('tab', {name:'动态', exact:true}).click();
  check(await tab.playwright.getByText('你补充了指令', {exact:true}).count() === 0, '取消不能写入 Chloe 的活动记录');
  const sendPosition = await tab.playwright.evaluate(() => {
    const box = document.querySelector('.agent-dock .hold-voice').getBoundingClientRect();
    return [box.left + box.width / 2, box.top + box.height / 2];
  });
  await tab.drag(sendPosition, [sendPosition[0] + 12, sendPosition[1]]);
  check(await tab.playwright.locator('.agent-dock .voice-result').innerText() === '已交给 Chloe · 原卡片继续', '松开后必须在 Chloe 底栏显示定向回执');
  check(await tab.playwright.getByText('【模拟语音】请继续完善这张卡片的任务。', {exact:true}).isVisible(), 'Chloe 动态必须显示标明模拟来源的语音指令');
  await button(tab, '切换 Chloe 到文字输入').click();
  check(await tab.playwright.getByRole('textbox', {name:'告诉 Chloe'}).getAttribute('value') === 'Chloe 的未发送草稿', '语音切换不能丢失 Chloe 草稿');
  await button(tab, '返回 Work').click();
  check(await tab.playwright.locator('.task-row').count() === 4, '语音指令不能复制或新建 Agent Card');
  await button(tab, '查看 Agent Card：Alan · 修复 iOS 登录恢复与 Keychain 回归').click();
  check(await tab.playwright.getByRole('textbox', {name:'告诉 Alan'}).getAttribute('value') === '', 'Alan 输入栏不能出现 Chloe 的未发送草稿');
  await tab.playwright.getByRole('tab', {name:'动态', exact:true}).click();
  check(await tab.playwright.getByText('【模拟语音】请继续完善这张卡片的任务。', {exact:true}).count() === 0, 'Alan 活动不能出现发给 Chloe 的语音指令');
  await button(tab, '返回 Work').click();
  await button(tab, '查看 Agent Card：Chloe · 等待你验收新版任务总览').click();
  check(await tab.playwright.getByRole('textbox', {name:'告诉 Chloe'}).getAttribute('value') === 'Chloe 的未发送草稿', '返回 Chloe 后应找回她的草稿');
  return 'Agent Card 语音切换、取消、定向发送与卡片独立草稿：通过';
}
