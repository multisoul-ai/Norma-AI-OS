// CUA E2E for the isolated HTML prototype. Run with an authorized preview Tab.
// Call testNormaRealtimePrototype(tab) from the CUA session after opening the page.
// This exercises mock UI only; it does not certify native iOS audio or Mac execution.
const check = (condition, message) => { if (!condition) throw new Error(message); };
const button = (tab, name) => tab.playwright.getByRole('button', {name, exact:true});

/// 场景：较矮桌面窗口也必须完整显示手机底部导航。
/// 数据构造：桌面视口高 720px；外层上下留白 88 + 38 = 126px，手机应不高于 594px。
/// 执行过程：保持桌面宽度 → 读取手机与导航边界 → 与视口底边比较。
/// 预期结果：手机、底部导航与陪伴页主文案均可见；不产生整页纵向溢出。
export async function testNormaRealtimeDesktopLayout(tab) {
  const layout = await tab.playwright.evaluate(() => ({
    phoneBottom: document.querySelector('.phone').getBoundingClientRect().bottom,
    navBottom: document.querySelector('.bottom-nav').getBoundingClientRect().bottom,
    height: innerHeight,
    documentHeight: document.documentElement.scrollHeight,
    quoteBottom: document.querySelector('.quote')?.getBoundingClientRect().bottom,
    mainBottom: document.querySelector('.main').getBoundingClientRect().bottom,
  }));
  check(layout.phoneBottom <= layout.height, '手机外框不能被桌面窗口底部截断');
  check(layout.navBottom <= layout.height, '底部导航必须完整可见');
  check(layout.documentHeight <= layout.height, '正常桌面高度不应因手机最小高度产生整页滚动');
  if (layout.quoteBottom !== undefined) check(layout.quoteBottom <= layout.mainBottom, '角色不能挤掉陪伴页主文案');
  return '较矮桌面窗口的手机外框与底部导航：通过';
}

/// 场景：三个方向均可从讨论走到派工、成果验收与退回修改。
/// 数据构造：三种布局共用同一组 3 条任务；显式派工新增 1 条手机端任务。
/// 执行过程：逐个选方向 → 开聊 → 讨论 → 派工 → 模拟完成 → 看前后版本 → 提交修改。
/// 预期结果：每个方向均可操作；讨论只有草稿；派工有回执；修改返回原任务。
export async function testNormaRealtimePrototype(tab) {
  for (const direction of ['陪伴', '对话', '调度']) {
    await button(tab, direction).click();
    await button(tab, '开始通话').click();
    if (direction === '调度') {
      await tab.playwright.locator('.phone [data-action="idea"]').click();
    } else {
      await tab.playwright.locator('.phone').getByRole('button', {name:'聊聊指挥台的想法', exact:true}).click();
    }
    check(await tab.playwright.getByText('讨论已整理 · Norma OS', {exact:true}).isVisible(), '明确派工前必须显示讨论草稿');
    check(!(await button(tab, '查看任务：让指挥台支持语音派工，做好后让我看页面。').isVisible()), '讨论不能提前创建任务卡片');
    await button(tab, '交给 Agent 执行').click();
    check(!(await button(tab, '交给 Agent 执行').isVisible()), '成功接收后草稿派工按钮应消失');
    check(await tab.playwright.locator('.phone-notice').innerText() === 'Codex 已接收 · Norma OS', '必须展示实际模拟接收回执');
    await tab.playwright.locator('.bottom-nav').getByRole('button', {name:'项目', exact:true}).click();
    await button(tab, '查看任务：让指挥台支持语音派工，做好后让我看页面。').click();
    await button(tab, '模拟此任务完成').click();
    await button(tab, '查看成果并验收').click();
    await tab.playwright.getByRole('tab', {name:'修改前', exact:true}).click();
    check(await tab.playwright.locator('.preview.before').isVisible(), '修改前必须呈现不同布局');
    await tab.playwright.getByRole('tab', {name:'修改后', exact:true}).click();
    check(!(await tab.playwright.locator('.preview.before').isVisible()), '修改后不能残留旧版布局');
    await tab.playwright.getByRole('textbox', {name:'需要调整的地方'}).fill('卡片间距再大一点。');
    await button(tab, '把修改意见交回 Agent').click();
    await button(tab, '查看任务：让指挥台支持语音派工，做好后让我看页面。').click();
    check(await tab.playwright.getByText('你的意见：卡片间距再大一点。', {exact:true}).isVisible(), '意见应保留在原任务');
    check(!(await button(tab, '查看成果并验收').isVisible()), '重新修改期间不能沿用旧版验收入口');
    await button(tab, '关闭弹层').click();
  }
  const errors = await tab.dev.logs({levels:['error'], limit:20});
  check(errors.length === 0, '三种布局与完整交互不得产生控制台错误');
  return '三方向讨论、派工、前后预览与原任务修改：通过';
}

/// 场景：手机窄屏可查看全部控件，Mac 离线与锁屏不改变任务执行事实。
/// 数据构造：390×844 为常见 iPhone CSS 视口；原型初始 3 条任务；44px 为主要触控目标下限。
/// 执行过程：开聊 → 打开演示控制 → Mac 离线 → 锁屏 → 解锁 → 挂断。
/// 预期结果：无横向溢出；触控目标不小于 44px；离线提示明确；锁屏可返回；挂断仍能浏览任务。
export async function testNormaRealtimeMobile(tab) {
  await button(tab, '陪伴').click();
  await button(tab, '开始通话').click();
  const targets = await tab.playwright.evaluate(() => Array.from(document.querySelectorAll('.phone .quick-talk button,.phone .composer button')).map(b => ({width:b.getBoundingClientRect().width,height:b.getBoundingClientRect().height})));
  check(targets.length === 3, '两项快捷讨论和发送按钮必须都存在');
  check(targets.every(b => b.width >= 44 && b.height >= 44), '手机讨论与发送按钮必须满足 44px 触控目标');
  const pickerLines = await tab.playwright.evaluate(() => Array.from(document.querySelectorAll('.proto-picker-item:not(.proto-picker-replay)')).map(b => { const range=document.createRange();range.selectNodeContents(b);return range.getClientRects().length; }));
  check(pickerLines.length === 3 && pickerLines.every(n => n === 1), '三个方向名称在手机屏幕上都必须保持单行');
  await tab.playwright.getByText('原型演示控制 · 不采集麦克风', {exact:true}).click();
  await tab.playwright.locator('.phone').getByRole('button', {name:'模拟 Mac 离线', exact:true}).click();
  check(await tab.playwright.getByText('Mac 暂时离线。可以继续聊，想法先保留；下方为最后同步状态。', {exact:true}).isVisible(), '离线必须明确标记最后快照');
  await tab.playwright.locator('.appbar').getByRole('button', {name:'体验锁屏', exact:true}).click();
  check(await tab.playwright.getByText('耳机已连接 · 角色渲染已暂停', {exact:true}).isVisible(), '锁屏示意必须暂停角色');
  check(!(await tab.playwright.getByRole('img', {name:'Norma 虚拟人形象示意'}).isVisible()), '锁屏不能继续渲染角色');
  await button(tab, '返回指挥台').click();
  await button(tab, '结束通话').click();
  check(await button(tab, '开始通话').isVisible(), '挂断后应能重新开聊');
  await tab.playwright.locator('.bottom-nav').getByRole('button', {name:'项目', exact:true}).click();
  check(await button(tab, '查看任务：改善耳机切换体验').isVisible(), '挂断不能删除既有任务');
  const overflow = await tab.playwright.evaluate(() => document.documentElement.scrollWidth > innerWidth);
  check(!overflow, '手机视口不能横向溢出');
  return '手机布局、离线标记、锁屏返回、挂断保留任务：通过';
}
