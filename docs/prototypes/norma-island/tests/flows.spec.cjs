const {test,expect}=require('playwright/test');
const path=require('node:path');const {pathToFileURL}=require('node:url');
const url=pathToFileURL(path.resolve(__dirname,'../index.html')).href;
test.use({channel:'chrome',viewport:{width:1440,height:1000}});
async function overview(page){if(!await page.getByTestId('island-panel').isVisible())await page.getByTestId('island-trigger').click();if(await page.locator('[data-action="overview"]').count())await page.locator('[data-action="overview"]').first().click()}
async function openWork(page,id){await overview(page);await page.locator('[data-tab="all"]').click();await page.locator(`[data-open-work="${id}"]`).first().click()}
// 场景：从收起的岛开始，6 张卡片在默认视频上；展开后提供可执行的待办。
// 数据：三个人工决定，一个后台验证、一个依赖项和一个已交付工作。
// 预期：视频真实解码，岛能收起展开；摄像头占位与可操作内容不重叠。
test('island starts quiet and opens the decision queue',async({page})=>{
 const errors=[];page.on('pageerror',e=>errors.push(e.message));await page.goto(url);
 await expect(page.locator('[data-node]')).toHaveCount(6);await expect(page.getByTestId('island-panel')).not.toBeVisible();
 await expect.poll(()=>page.locator('video').evaluate(v=>v.videoWidth)).toBe(1920);
 await page.getByTestId('island-trigger').click();await expect(page.getByTestId('pending-count')).toHaveText('3');
 await expect(page.locator('[data-open-work="login"]')).toBeVisible();
 await page.keyboard.press('Escape');await expect(page.getByTestId('island-panel')).not.toBeVisible();expect(errors,'原型不能产生脚本异常').toEqual([]);
});
// 场景：多个任务对话隔离；新事件不切走正在编辑的任务。
// 数据：登录和预览分别输入草稿，模拟 Nova 发来新的问题。
// 预期：输入与消息不串任务，新事件只更新队列数量。
test('drafts stay with work while new events arrive',async({page})=>{
 await page.goto(url);await openWork(page,'login');await page.getByTestId('composer').fill('保留登录数据');
 await openWork(page,'preview');await page.getByTestId('composer').fill('只更新测试预览');
 await openWork(page,'login');await expect(page.getByTestId('composer')).toHaveValue('保留登录数据');
 await page.locator('[data-action="event"]').click();await expect(page.getByTestId('active-work')).toContainText('修复登录');await expect(page.getByTestId('composer')).toHaveValue('保留登录数据');await expect(page.getByTestId('pending-count')).toHaveText('4');
 await page.getByTestId('send').click();await page.locator('[data-work-tab="chat"]').click();await expect(page.getByTestId('island-panel')).toContainText('保留登录数据');
 await openWork(page,'preview');await expect(page.getByTestId('composer')).toHaveValue('只更新测试预览');await expect(page.getByTestId('island-panel')).not.toContainText('保留登录数据');
});
// 场景：修改答复与验收分别推进真实的工作阶段，避免把答复误当完成。
// 数据：编辑登录建议、采用卡片定位验收。
// 预期：待办依次减少；登录继续执行，定位进入待合并。
test('edited answers continue work and reviews await merge',async({page})=>{
 await page.goto(url);await openWork(page,'login');await page.locator('[data-action="edit"]').click();
 await page.getByTestId('decision-edit').fill('保留数据，重新登录一次');await page.getByTestId('submit-edit').click();
 await expect(page.getByTestId('pending-count')).toHaveText('2');await expect(page.getByTestId('work-stage')).toContainText('继续执行');
 await page.locator('[data-work-tab="chat"]').click();await expect(page.getByTestId('island-panel')).toContainText('保留数据，重新登录一次');
 await openWork(page,'navigation');await page.getByTestId('apply-decision').click();await expect(page.getByTestId('work-stage')).toContainText('待合并');await expect(page.getByTestId('pending-count')).toHaveText('1');
});
// 场景：在其他 App 场景中定位卡片，再返回原来的决定。
// 数据：从备忘录场景写一份登录草稿，跳到 Simon 卡片。
// 预期：目标卡片居中，岛收起，返回时恢复场景、任务和草稿。
test('jump to canvas and return to the original context',async({page})=>{
 await page.goto(url);await page.locator('[data-scene="desktop"]').click();await openWork(page,'login');await page.getByTestId('composer').fill('返回后继续写');
 await page.locator('[data-action="locate"]').click();await expect(page.getByTestId('island-panel')).not.toBeVisible();
 const centered=await page.locator('[data-node="simon"]').evaluate(el=>{const r=el.getBoundingClientRect(),v=document.querySelector('.canvas-viewport').getBoundingClientRect();return Math.abs(r.x+r.width/2-v.x-v.width/2)<3&&Math.abs(r.y+r.height/2-v.y-v.height/2)<3});expect(centered,'定位误差必须小于 3px').toBe(true);
 await page.getByTestId('island-trigger').click();await expect(page.locator('body')).toHaveAttribute('data-scene','desktop');await expect(page.getByTestId('composer')).toHaveValue('返回后继续写');
});
// 场景：各尺寸和无刘海屏幕保持入口可达，减少动态效果与背景控制生效。
// 数据：1440、1024、768、390px，切英文和无刘海；首帧开启 reduce。
// 预期：岛不越界、不横向溢出，视频真实暂停，按钮可操作。
test('notch, flat display, locale and reduced motion',async({page})=>{
 await page.emulateMedia({reducedMotion:'reduce'});await page.goto(url);expect(await page.locator('video').evaluate(v=>v.paused),'reduce 应暂停背景').toBe(true);
 for(const width of [1440,1024,768,390]){
  await page.setViewportSize({width,height:900});await openWork(page,'login');
  for(const hardware of ['notch','flat']){
   await page.locator('[data-testid="hardware"]').selectOption(hardware);const box=await page.getByTestId('island-panel').boundingBox();expect(box.x,'岛左边不能越界').toBeGreaterThanOrEqual(0);expect(box.x+box.width,'岛右边不能越界').toBeLessThanOrEqual(width+1);expect(box.y+box.height,'岛底部不能越界').toBeLessThanOrEqual(901);
   expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'页面不能横向溢出').toBe(true);
  }
 }
 await page.keyboard.press('Escape');await page.locator('[data-action="language"]').click();await openWork(page,'login');await expect(page.getByTestId('active-work')).toContainText('blank screen');await page.getByTestId('composer').fill('1');await page.keyboard.press('3');await expect(page.getByTestId('composer')).toHaveValue('13');
});
// 场景：人工接管暂停采用，交付物可实际下载，简报使用当前状态。
// 数据：接管 Simon，查看 Iris 的示例文档，随后打开简报。
// 预期：接管时禁用执行动作；下载内容有明确示例身份；简报仍有待办。
test('takeover, deliverables and briefing work',async({page})=>{
 await page.goto(url);await page.locator('.demo-toolbar [data-scene="canvas"]').click();await page.locator('[data-control="simon"]').click();await openWork(page,'login');await expect(page.getByTestId('apply-decision')).toBeDisabled();
 await overview(page);await page.locator('[data-tab="deliveries"]').click();await page.locator('[data-action="artifact"]').click();await expect(page.locator('dialog')).toContainText('示例');
 const downloadPromise=page.waitForEvent('download');await page.locator('[data-action="download"]').click();const download=await downloadPromise;expect(download.suggestedFilename(),'交付物文件名明确').toBe('norma-visual-spec-example.md');await page.keyboard.press('Escape');
 await page.locator('[data-action="brief"]').click();await expect(page.getByTestId('island-panel')).toContainText('刚才的进展');await expect(page.getByTestId('pending-count')).toHaveText('3');
});

// 场景：参考图的任务工作台可创建任务，任务页与岛共用同一工作上下文。
// 数据：从大输入框创建一项示例工作，再在任务页与岛间切换草稿。
// 预期：新工作进入待拆解阶段；两处讨论保留同一份草稿与消息。
test('task workspace creates local work and shares discussion with island',async({page})=>{
 await page.goto(url);await expect(page.locator('body')).toHaveAttribute('data-scene','tasks');
 await page.getByTestId('new-task').fill('整理今天的发布验收清单');await page.getByTestId('create-task').click();
 await expect(page.getByTestId('main-title')).toContainText('整理今天的发布验收清单');
 await page.getByTestId('main-composer').fill('只整理桌面端');await page.locator('[data-action="main-to-island"]').click();
 await expect(page.getByTestId('composer')).toHaveValue('只整理桌面端');await page.getByTestId('send').click();
 await page.keyboard.press('Escape');await expect(page.getByTestId('main-composer')).toHaveValue('');await expect(page.locator('.main-thread')).toContainText('只整理桌面端');
 await page.getByTestId('main-composer').fill('第二条从任务页发送');await page.getByTestId('main-send').click();await page.locator('[data-action="main-to-island"]').click();await expect(page.getByTestId('composer'),'已发送的草稿不能从收起的岛中复活').toHaveValue('');
});
