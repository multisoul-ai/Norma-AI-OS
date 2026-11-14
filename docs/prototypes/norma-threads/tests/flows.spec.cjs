const {test, expect}=require('playwright/test');
const {pathToFileURL}=require('node:url');
const path=require('node:path');
const url=pathToFileURL(path.resolve(__dirname,'../index.html')).href;
test.use({channel:'chrome',viewport:{width:1440,height:1000}});

// 场景：并行工作不能共享草稿和对话。分别给登录、预览写草稿，再给其中一个发消息。
// 数据：两个独立 work id，分别使用可区分的中文草稿。
// 预期：切换后恢复各自草稿，消息只出现在所属线程，右侧进度同时切换。
test('parallel threads isolate drafts, messages and context',async({page})=>{
 await page.goto(url);
 await expect(page.getByTestId('thread-title')).toContainText('修复登录');
 await page.getByTestId('composer').fill('登录任务独立草稿');
 await page.locator('[data-thread="preview"]').first().click();
 await expect(page.getByTestId('composer')).toHaveValue('');
 await page.getByTestId('composer').fill('预览任务独立草稿');
 await page.locator('[data-thread="login"]').first().click();
 await expect(page.getByTestId('composer')).toHaveValue('登录任务独立草稿');
 await page.getByTestId('send').click();
 await expect(page.getByTestId('thread-messages')).toContainText('登录任务独立草稿');
 await page.locator('[data-thread="preview"]').first().click();
 await expect(page.getByTestId('composer')).toHaveValue('预览任务独立草稿');
 await expect(page.getByTestId('thread-messages')).not.toContainText('登录任务独立草稿');
 await expect(page.getByTestId('progress-panel')).toContainText('更新团队预览');
});

// 场景：全局进度与待办都必须回到明确工作，采用方案后统一更新状态。
// 数据：6 项工作，初始 3 项需人工；处理登录决策。
// 预期：待办降为 2，登录仍是继续执行，验收任务仍保持待验收。
test('overview and attention route to work and update consistently',async({page})=>{
 await page.goto(url);
 await page.getByTestId('nav-overview').click();
 await expect(page.locator('[data-overview-work]')).toHaveCount(6);
 await page.locator('[data-overview-work="regression"] [data-open-thread]').click();
 await expect(page.getByTestId('thread-title')).toContainText('回归');
 await page.getByTestId('nav-attention').click();
 await page.locator('[data-attention-work="login"] [data-open-thread]').click();
 await page.getByTestId('apply-decision').click();
 await expect(page.getByTestId('pending-count')).toHaveText('2');
 await expect(page.getByTestId('progress-panel')).toContainText('继续执行');
 await page.getByTestId('nav-overview').click();
 await expect(page.locator('[data-overview-work="login"]')).toContainText('继续执行');
 await expect(page.locator('[data-overview-work="navigation"]')).toContainText('等待你验收');
});

// 场景：从任务进入原画布，卡片居中并可接管；返回后原任务、草稿和阅读上下文保留。
// 数据：登录任务与 Simon 卡片存在稳定关联。
// 预期：画布上没有频道侧栏，接管会禁用自动采用建议，交还后恢复。
test('canvas preserves thread and supports explicit takeover',async({page})=>{
 await page.goto(url);
 await page.getByTestId('composer').fill('暂不发布');
 await page.getByTestId('thread-canvas').click();
 await expect(page.locator('.threads-sidebar')).toHaveCount(0);
 await expect(page.locator('[data-node="simon"]')).toHaveAttribute('data-focused','true');
 expect(await page.locator('[data-node="simon"]').evaluate(el=>{const a=el.getBoundingClientRect(),v=el.closest('.canvas-viewport').getBoundingClientRect();return Math.abs(a.x+a.width/2-v.x-v.width/2)<4&&Math.abs(a.y+a.height/2-v.y-v.height/2)<4;}),'目标卡片应居中').toBe(true);
 await page.getByTestId('takeover').click();
 await page.getByTestId('nav-norma').click();
 await expect(page.getByTestId('thread-title')).toContainText('修复登录');
 await expect(page.getByTestId('composer')).toHaveValue('暂不发布');
 await expect(page.getByTestId('apply-decision')).toBeDisabled();
 await page.getByTestId('thread-canvas').click();
 await page.getByTestId('takeover').click();
 await page.getByTestId('nav-norma').click();
 await expect(page.getByTestId('apply-decision')).toBeEnabled();
});

// 场景：频道讨论、项目导航与新工作有各自入口，搜索只定位到匹配线程。
// 数据：创建一项官网工作，再在该任务和频道之间切换。
// 预期：新工作进入对应频道，草稿保持独立，搜索结果打开正确工作。
test('channels and new work stay organized by project',async({page})=>{
 await page.goto(url);
 await page.locator('[data-channel="desktop"]').first().click();
 await expect(page.getByTestId('channel-title')).toContainText('桌面工作台');
 await page.getByTestId('composer').fill('频道里的讨论草稿');
 await page.getByTestId('new-work').click();
 await page.getByTestId('new-work-title').fill('检查官网移动端排版');
 await page.getByTestId('new-work-project').selectOption('website');
 await page.getByTestId('create-work').click();
 await expect(page.getByTestId('thread-title')).toContainText('检查官网移动端排版');
 await expect(page.getByTestId('composer')).toHaveValue('');
 await page.locator('[data-channel="desktop"]').first().click();
 await expect(page.getByTestId('composer')).toHaveValue('频道里的讨论草稿');
 await page.getByTestId('search-work').click();
 await page.getByTestId('search-input').fill('官网移动端');
 await page.locator('[data-search-work] [data-open-thread]').click();
 await expect(page.getByTestId('thread-title')).toContainText('检查官网移动端排版');
});

// 场景：线程阅读位置和验收状态不能因切换丢失；视频与双语入口真实可用。
// 数据：向长线程追加消息，切换后检查同一 scrollTop，再通过定位任务验收。
// 预期：阅读位置误差不超过 2px，验收只进入待合并；无页面脚本错误。
test('scroll, review, video and locale remain reliable',async({page})=>{
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(url);
 await expect.poll(()=>page.locator('video').evaluate(v=>v.videoWidth)).toBe(1920);
 await page.getByTestId('video-toggle').click();
 expect(await page.locator('video').evaluate(v=>v.paused),'背景按钮应真实暂停视频').toBe(true);
 await page.getByTestId('composer').fill('补充登录任务的验收要求\n请保持旧账号的本地数据。\n本次先不发布。\n验收需要覆盖重新登录和数据保留。');await page.getByTestId('send').click();
 const scroll=await page.getByTestId('thread-messages').evaluate(e=>{e.scrollTop=80;return e.scrollTop});
 await page.locator('[data-thread="preview"]').first().click();await page.locator('[data-thread="login"]').first().click();
 expect(await page.getByTestId('thread-messages').evaluate(e=>Math.abs(e.scrollTop-80)),'线程应恢复之前的阅读位置').toBeLessThanOrEqual(2);
 expect(scroll,'构造的长线程应确实可滚动').toBe(80);
 await page.locator('[data-thread="navigation"]').first().click();await page.getByTestId('apply-decision').click();
 await expect(page.getByTestId('progress-panel')).toContainText('待合并');
 await page.getByTestId('language').click();
 await expect(page.getByTestId('nav-overview')).toContainText('Overview');
 expect(errors,'原型不应产生脚本错误').toEqual([]);
});

// 场景：切换项目后统计必须与当前项目一致；清空待办不能把验收状态伪装为完成。
// 数据：官网项目初始仅 1 项已交付；处理三个待办，再在窄窗口展开、收起任务进度。
// 预期：统计为 0/0/0/1，人工待办归零，窄窗口仍能访问并关闭进度。
test('project totals, empty attention and compact progress stay consistent',async({page})=>{
 await page.goto(url);
 await page.getByTestId('nav-overview').click();
 await page.locator('[data-overview-project]').selectOption('website');
 await expect(page.locator('[data-overview-work]')).toHaveCount(1);
 expect(await page.locator('.overview-metric b').allTextContents(),'项目统计必须与筛选后的工作一致').toEqual(['00','00','00','01']);
 await page.getByTestId('nav-attention').click();
 for(const id of ['login','preview','navigation'])await page.locator(`[data-attention-work="${id}"] [data-apply]`).click();
 await expect(page.getByTestId('pending-count')).toHaveText('0');
 await expect(page.locator('.attention-list')).toContainText('暂时没有必须由你处理');
 await page.setViewportSize({width:900,height:900});
 await page.locator('[data-thread="navigation"]').first().click();
 await expect(page.getByTestId('progress-panel')).toBeHidden();
 await page.locator('.progress-toggle').click();
 await expect(page.getByTestId('progress-panel')).toBeVisible();
 await expect(page.getByTestId('progress-panel')).toContainText('待合并');
 await page.locator('.progress-close').click();
 await expect(page.getByTestId('progress-panel')).toBeHidden();
});
