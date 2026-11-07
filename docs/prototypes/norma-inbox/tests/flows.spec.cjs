const {test,expect}=require('playwright/test');
const {pathToFileURL}=require('node:url');const path=require('node:path');
const url=pathToFileURL(path.resolve(__dirname,'../index.html')).href;
test.use({channel:'chrome',viewport:{width:1600,height:1050}});
async function open(page,query=''){await page.goto(url+query);await expect(page.locator('#inbox')).toBeVisible();}
// 场景：全局入口在首页、原会话与画布之间切换。执行：快捷键收起和呼出，再分级 Esc。
// 预期：回到同一条会话；关闭会话恢复居中首页；画布仍可从 Dock 打开任务页。
test('shortcut preserves the selected session and Escape closes one level',async({page})=>{
 await open(page);await expect(page.locator('.hero')).toContainText('把工作交出去');
 await page.locator('[data-work="login"]').click();await expect(page.locator('.thread')).toContainText('Grace');
 await page.keyboard.press('Meta+Shift+Space');await expect(page.locator('#inbox')).not.toBeVisible();
 await page.keyboard.press('Meta+Shift+Space');await expect(page.locator('.thread')).toContainText('Grace');
 await page.keyboard.press('Escape');await expect(page.locator('.thread')).toHaveCount(0);await expect(page.locator('.hero')).toBeVisible();
 await page.keyboard.press('Escape');await page.locator('[data-action="open-inbox"]').click();await expect(page.locator('.hero')).toBeVisible();
});
// 场景：同名 Agent 位于不同工作空间。执行：分别写草稿并发送其中一条。
// 预期：工作空间/项目/卡片/session 键隔离上下文，不因名字相同而串线。
test('parallel tasks and same-name agents keep separate drafts and messages',async({page})=>{
 await open(page);await page.locator('[data-work="login"]').click();await page.getByTestId('thread-input').fill('主空间的采样草稿');
 await page.locator('[data-work="lab-login"]').click();await expect(page.getByTestId('thread-input')).toHaveValue('');await page.getByTestId('thread-input').fill('实验室的渲染草稿');
 await page.locator('[data-work="login"]').click();await expect(page.getByTestId('thread-input')).toHaveValue('主空间的采样草稿');await page.getByTestId('thread-send').click();
 await expect(page.locator('.thread-history')).toContainText('主空间的采样草稿');await page.locator('[data-work="lab-login"]').click();
 await expect(page.getByTestId('thread-input')).toHaveValue('实验室的渲染草稿');await expect(page.locator('.thread-history')).not.toContainText('主空间的采样草稿');
});
// 场景：范围选择器的键盘浏览不是立即选中。执行：浏览后取消，再确认实验室。
// 预期：取消不更改范围，确认后列表计数和任务同时变化，发送项目不被悄悄改写。
test('scope menu separates browsing from selection and respects workspace identity',async({page})=>{
 await open(page);await page.locator('[data-menu="scope"]').click();await page.getByTestId('menu-search').fill('实验室');await page.keyboard.press('ArrowDown');await page.keyboard.press('Escape');
 await expect(page.locator('[data-work]')).toHaveCount(7);await expect(page.locator('[data-menu="scope"]')).toBeFocused();
 await page.locator('[data-menu="scope"]').click();await page.getByTestId('menu-search').fill('实验室');await page.locator('[data-choice="lab/all"]').click();
 await expect(page.locator('[data-work]')).toHaveCount(1);await expect(page.locator('[data-work="lab-login"]')).toBeVisible();
 await expect(page.locator('[data-menu="destination"]')).toContainText('Norma OS');
});
// 场景：原卡片和全局页共享会话。执行：写草稿、定位、在画布发送、返回。
// 预期：定位正确卡片，草稿与消息双向同步，返回保留所选任务。
test('locating the original card shares drafts and replies in both directions',async({page})=>{
 await open(page);await page.locator('[data-work="login"]').click();await page.getByTestId('thread-input').fill('从画布继续这件事');await page.locator('[data-action="locate"]').click();
 await expect(page.locator('[data-card="login"]')).toHaveClass(/focused/);await expect(page.getByTestId('canvas-input')).toHaveValue('从画布继续这件事');await page.getByTestId('canvas-send').click();
 await page.locator('[data-action="return-inbox"]').click();await expect(page.locator('.thread-history')).toContainText('从画布继续这件事');await expect(page.getByTestId('thread-input')).toHaveValue('');
});
// 场景：Norma 新建工作与给已有 Agent 下指令有不同去向。执行：先建工作，再选实验室 Grace。
// 预期：新工作等待拆解且不能定位；直发进入指定原会话，不创建同名卡片。
test('new work has explicit project scope and direct instructions use the original card',async({page})=>{
 await open(page);await page.getByTestId('new-input').fill('整理下一版的性能验收计划');await page.getByTestId('new-send').click();
 await expect(page.locator('.thread')).toContainText('等待拆解');await expect(page.locator('[data-action="locate"]')).toBeDisabled();
 await expect(page.locator('.thread')).toContainText('Norma-AI-OS');await page.locator('[data-action="close-thread"]').click();
 await page.locator('[data-menu="recipient"]').click();await page.getByTestId('menu-search').fill('实验室');await page.locator('[data-choice="lab-login"]').click();
 await page.getByTestId('new-input').fill('只继续实验室的对照实验');await page.getByTestId('new-send').click();
 await expect(page.locator('.thread')).toContainText('实验室');await expect(page.locator('.thread-history')).toContainText('只继续实验室的对照实验');
 await page.locator('[data-work="login"]').click();await expect(page.locator('.thread-history')).not.toContainText('只继续实验室的对照实验');
});
// 场景：异步进展不打断正在沟通的任务。执行：模拟 Kevin 完成，再处理 Jasper 验收。
// 预期：当前草稿不丢、任务不被切走；验收进入待合并而不是已完成。
test('updates remain quiet and review does not imply merging',async({page})=>{
 await open(page);await page.locator('[data-work="login"]').click();await page.getByTestId('thread-input').fill('继续保留现场');await page.locator('[data-action="simulate"]').click();
 await expect(page.locator('.thread')).toContainText('Grace');await expect(page.getByTestId('thread-input')).toHaveValue('继续保留现场');await expect(page.locator('[data-work="drawer"]')).toContainText('已回答');
 await page.locator('[data-work="navigation"]').click();await page.locator('[data-action="accept"]').click();await expect(page.locator('.thread .thread-status')).toContainText('待合并');
 await expect(page.locator('.thread .thread-status')).not.toContainText('已完成');
});
// 场景：已回答工作可以归档并找回。执行：查看 Henry、取消归档、再次归档。
// 预期：归档只改变收件箱组织，不删除会话或改变执行结果。
test('archive is reversible and keeps the original conversation',async({page})=>{
 await open(page);await page.locator('[data-action="toggle-archive"]').click();await page.locator('[data-work="cleanup"]').click();
 await page.locator('[data-action="archive"]').click();await expect(page.locator('[data-group="needs"] [data-work="cleanup"]')).toBeVisible();
 await page.locator('[data-action="archive"]').click();await expect(page.locator('.thread-history')).toContainText('17.4 GiB');await expect(page.locator('[data-group="archive"] [data-work="cleanup"]')).toBeVisible();
});
// 场景：窄窗口与英文保持同一任务上下文。执行：减少动态效果、双语切换、缩窄到 900 和 600。
// 预期：视频暂停、无页面横向溢出、窄屏对话有显式返回入口，脚本无错误。
test('compact layouts, language and reduced motion remain usable',async({page})=>{
 const errors=[];page.on('pageerror',e=>errors.push(e.message));await page.emulateMedia({reducedMotion:'reduce'});await open(page);
 expect(await page.locator('video').evaluate(v=>v.paused),'减少动态效果应暂停视频').toBe(true);await page.locator('[data-action="language"]').click();
 for(const width of [900,600]){await page.setViewportSize({width,height:900});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'小窗口不得使整个页面横向溢出').toBe(true);}
 await page.locator('[data-work="login"]').click();await expect(page.getByTestId('thread-input')).toBeVisible();await page.locator('[data-action="close-thread"]').click();await expect(page.locator('.hero')).toBeVisible();
 expect(errors,'核心双语流程应无脚本错误').toEqual([]);
});
