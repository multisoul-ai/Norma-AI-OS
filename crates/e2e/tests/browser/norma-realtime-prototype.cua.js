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
/// 预期结果：默认展示 7 张卡片；项目过滤只缩小视图；卡片详情使用手机层级导航而非弹窗，并保留方案、定向对话与原列表位置。
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
  await button(tab, '查看 Agent Card 派工计划').click();
  check(await tab.playwright.getByText('Daisy · Codex', {exact:true}).isVisible(), '派工计划必须显示将新建的 Daisy 卡片');
  check(await tab.playwright.getByText('Ethan · Claude', {exact:true}).isVisible(), '派工计划必须显示将新建的 Ethan 卡片');
  check(await tab.playwright.getByText('Felix · Codex', {exact:true}).isVisible(), '派工计划必须显示将新建的 Felix 卡片');
  check(await tab.playwright.getByText('新建卡片', {exact:true}).count() === 3, '三个并行 Agent 必须明确标记新建卡片');
  await button(tab, '确认并开始执行').click();
  check(await tab.playwright.locator('.phone-notice').innerText() === 'Daisy、Ethan、Felix 已创建并接收 · 3 张 Agent Card', '明确执行后必须展示具名卡片接收回执');
  check(await tab.playwright.locator('.call-dock').count() === 0, 'Work 页面不能保留 Norma 通话 Dock');
  check(await tab.playwright.locator('.task-row').count() === 7, '明确执行后应显示 4 + 3 = 7 张 Agent Card');
  check(await button(tab, '全部 7').getAttribute('aria-pressed') === 'true', 'Work 默认必须选中“全部”过滤条件');
  check(await tab.playwright.getByText('实现 iOS 多任务入口与状态反馈', {exact:true}).isVisible(), 'Daisy 卡片必须显示 Norma 概括的当前任务');
  check(await tab.playwright.getByText('补齐跨仓库任务聚合接口', {exact:true}).isVisible(), 'Ethan 卡片必须显示 Norma 概括的当前任务');
  check(await tab.playwright.getByText('验证发布前的跨仓检查', {exact:true}).isVisible(), 'Felix 卡片必须显示 Norma 概括的当前任务');
  await button(tab, 'Norma API 2').click();
  check(await tab.playwright.locator('.task-row').count() === 2, 'Norma API 过滤后必须只显示 Bella 与 Ethan 两张卡片');
  check(await tab.playwright.getByText('Bella · Claude', {exact:true}).isVisible(), 'Norma API 过滤结果必须包含 Bella');
  check(await tab.playwright.getByText('Ethan · Claude', {exact:true}).isVisible(), 'Norma API 过滤结果必须包含 Ethan');
  check(await tab.playwright.getByText('Alan · Codex', {exact:true}).count() === 0, 'Norma API 过滤结果不能混入 Norma OS 的 Alan');
  await button(tab, '全部 7').click();
  check(await tab.playwright.locator('.task-row').count() === 7, '切回全部后必须恢复 7 张 Agent Card');
  await button(tab, '查看 Agent Card：Alan · 修复 iOS 登录恢复与 Keychain 回归').click();
  check(await tab.playwright.locator('.card-main[aria-label="Agent Card 工作空间"]').isVisible(), '点击卡片必须进入全屏 Agent Card 工作空间');
  check(await tab.playwright.locator('dialog[open]').count() === 0, 'Agent Card 详情不能继续使用桌面式模态弹窗');
  check(await tab.playwright.getByRole('button', {name:/模拟/}).count() === 0, 'Agent Card 内不能暴露原型模拟控制');
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
  const presenceHeight = await tab.playwright.locator('.norma-stage').evaluate(el => el.getBoundingClientRect().height);
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
