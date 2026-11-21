const {test,expect}=require('playwright/test');
const path=require('node:path');const {pathToFileURL}=require('node:url');
const url=pathToFileURL(path.resolve(__dirname,'../index.html')).href;
test.use({channel:'chrome',viewport:{width:1600,height:1050}});
const open=async page=>{await page.goto(url);await page.getByTestId('dock-open').click()};
// 场景：画布通过底部菜单唤起工作抽屉；数据为两个项目的现有 Agent。
// 执行：点击入口、编辑、Escape 收起、快捷键再打开。预期：会话草稿保留，焦点回入口。
test('bottom drawer opens, closes and restores its session',async({page})=>{
 const errors=[];page.on('pageerror',e=>errors.push(e.message));await page.goto(url);
 await expect(page.locator('#work-drawer')).not.toBeVisible();await expect.poll(()=>page.locator('video').evaluate(v=>v.videoWidth)).toBe(1920);
 await page.getByTestId('dock-open').click();await expect(page.locator('#work-drawer')).toBeVisible();
 await page.getByTestId('thread-input').fill('保留原有数据');await page.keyboard.press('Escape');
 await expect(page.locator('#work-drawer')).not.toBeVisible();await expect(page.getByTestId('dock-open')).toBeFocused();
 await page.keyboard.press('Meta+e');await expect(page.getByTestId('thread-input')).toHaveValue('保留原有数据');
 expect(errors,'原型不得产生脚本异常').toEqual([]);
});
// 场景：多个任务与跨工作空间同名 Agent；数据中两个空间均有 Norma OS / Simon。
// 执行：分别输入、切换项目与空间。预期：列表范围正确，同名卡片也不串草稿。
test('workspace, project and card identities isolate conversations',async({page})=>{
 await open(page);await page.getByTestId('thread-input').fill('产品空间草稿');
 await page.locator('[data-work="site"]').click();await page.getByTestId('thread-input').fill('官网草稿');
 await page.locator('[data-work="login"]').click();await expect(page.getByTestId('thread-input')).toHaveValue('产品空间草稿');
 await page.locator('[data-action="workspace"]').click();await page.locator('[data-option="lab"]').click();
 await expect(page.locator('[data-work]')).toHaveCount(1);await page.getByTestId('thread-input').fill('实验空间草稿');
 await page.locator('[data-action="workspace"]').click();await page.locator('[data-option="product"]').click();
 await expect(page.getByTestId('thread-input')).toHaveValue('产品空间草稿');
 await page.locator('[data-action="project"]').click();await page.locator('[data-option="site"]').click();
 await expect(page.locator('[data-work]')).toHaveCount(2);await expect(page.getByTestId('thread-input')).toHaveValue('官网草稿');
});
// 场景：从抽屉定位现场，在原卡片中继续沟通后返回。
// 执行：输入草稿、定位、从画布发送、返回。预期：同一会话和草稿，没有重复消息，原画布视角恢复。
test('canvas and drawer share one session and restore the previous view',async({page})=>{
 await open(page);const before=await page.locator('#world').getAttribute('style');
 await page.getByTestId('thread-input').fill('从画布继续这条消息');await page.getByTestId('locate').click();
 await expect(page.locator('#work-drawer')).not.toBeVisible();await expect(page.getByTestId('canvas-input')).toHaveValue('从画布继续这条消息');
 const box=await page.locator('.canvas-card.focused').boundingBox();expect(Math.abs(box.x+box.width/2-800),'目标卡片应水平居中').toBeLessThan(3);
 await page.getByTestId('canvas-send').click();await expect(page.getByTestId('canvas-input')).toHaveValue('');
 await page.getByTestId('return').click();await expect(page.getByTestId('thread-input')).toHaveValue('');
 await expect(page.locator('#thread')).toContainText('从画布继续这条消息');await expect(page.locator('#world')).toHaveAttribute('style',before);
 await page.locator('[data-work="site"]').click();await expect(page.locator('#thread')).not.toContainText('从画布继续这条消息');
});
// 场景：采用建议只解除阻塞，验收通过只是待合并；新事件不抢正在编辑的会话。
// 数据：登录任务需要选择策略，定位任务需要验收。执行：采用建议、验收、模拟通知。
test('decisions advance the correct stage and notifications do not steal focus',async({page})=>{
 await open(page);await page.getByTestId('accept').click();await expect(page.getByTestId('thread-status')).toHaveText('处理中');
 await page.locator('[data-work="navigation"]').click();await page.getByTestId('accept').click();await expect(page.getByTestId('thread-status')).toHaveText('待合并');
 await page.getByTestId('thread-input').fill('正在写的消息');await page.getByTestId('simulate-event').click();
 await expect(page.getByTestId('thread-input')).toHaveValue('正在写的消息');await expect(page.getByTestId('thread-title')).toContainText('PR #182');
});
// 场景：创建工作必须明确目标区域，尚未派工时不能假装已有 Agent 卡片。
// 执行：填写新任务并选择目标项目，确认创建。预期：等待 Norma 拆解、定位禁用、没有声称已执行。
test('new work has an explicit target and waits for planning',async({page})=>{
 await open(page);await page.getByTestId('new-input').fill('整理桌面发布验收清单');await page.getByTestId('new-send').click();
 await expect(page.locator('#target-dialog')).toBeVisible();await page.locator('#target-project').selectOption('norma');await page.getByTestId('confirm-create').click();
 await expect(page.getByTestId('thread-title')).toHaveText('整理桌面发布验收清单');await expect(page.getByTestId('thread-status')).toHaveText('等待拆解');await expect(page.getByTestId('locate')).toBeDisabled();
});
// 场景：窄窗口、中英文、减少动态效果；执行：切换语言、尺寸与视频。
// 预期：输入与返回可达、无横向溢出，reduce 下视频暂停且抽屉不移动。
test('responsive layouts and accessibility preferences remain usable',async({page})=>{
 await page.emulateMedia({reducedMotion:'reduce'});await open(page);
 expect(await page.locator('video').evaluate(v=>v.paused),'减少动态效果时应暂停视频').toBe(true);
 await page.locator('[data-action="language"]').click();await expect(page.getByTestId('thread-title')).toContainText('canvas drag');
 for(const width of [1024,768,390]){
  await page.setViewportSize({width,height:900});await expect(page.getByTestId('thread-input')).toBeVisible();
  const b=await page.locator('.sheet').boundingBox();expect(b.x,'抽屉左侧不得越界').toBeGreaterThanOrEqual(0);expect(b.x+b.width,'抽屉右侧不得越界').toBeLessThanOrEqual(width+1);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'禁止页面横向溢出').toBe(true);
 }
 await page.keyboard.press('Escape');await expect(page.getByTestId('dock-open')).toBeVisible();
});
// 场景：任务列表滚动后定位卡片，返回时保留列表位置；弹层有原生焦点约束。
// 执行：滚动列表、写草稿、定位返回、Tab 导航。预期：滚动和焦点均在正确上下文内。
test('scroll, focus and quick reversals preserve the work context',async({page})=>{
 await open(page);await page.locator('#work-list').evaluate(el=>el.scrollTop=250);
 const scroll=await page.locator('#work-list').evaluate(el=>el.scrollTop);
 await page.getByTestId('locate').click();await page.getByTestId('return').click();
 expect(await page.locator('#work-list').evaluate(el=>el.scrollTop),'返回后保留任务列表位置').toBe(scroll);
 await page.keyboard.press('Tab');expect(await page.evaluate(()=>document.querySelector('#work-drawer').contains(document.activeElement)),'焦点不得跳到背景画布').toBe(true);
 await page.locator('#drawer-dock [data-action="close"]').click();await page.keyboard.press('Meta+e');await expect(page.locator('#work-drawer')).toBeVisible();
 await page.waitForTimeout(300);await expect(page.locator('#work-drawer')).toBeVisible();
 await page.getByTestId('simulate-event').click();await expect(page.locator('#notice')).toBeVisible();
});
// 场景：真实文本输入不得作为 HTML 执行；每个归档卡片可继续独立讨论。
// 执行：打开已完成任务，发送包含标签的文本。预期：原样显示，卡片状态保持完成。
test('completed work accepts a safe continuation without changing its stage',async({page})=>{
 await open(page);await page.locator('[data-work="cleanup"]').click();
 await page.getByTestId('thread-input').fill('<img src=x onerror="window.injected=true">');await page.getByTestId('thread-send').click();
 await expect(page.getByTestId('thread-status')).toHaveText('已完成');await expect(page.locator('#thread .user-message').last()).toHaveText('<img src=x onerror="window.injected=true">');
 expect(await page.evaluate(()=>window.injected),'文本不能成为脚本').toBeUndefined();
});
