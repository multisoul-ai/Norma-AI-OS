const { test, expect } = require('playwright/test');
const { pathToFileURL } = require('node:url');
const path = require('node:path');
const pageUrl = pathToFileURL(path.resolve(__dirname, '../index.html')).href;
test.use({channel:'chrome',viewport:{width:1440,height:1000}});

// 场景：三种风格共用正式 App 默认视频，并保留大对话、待办、现场与返回。
// 数据：6 件示例工作，3 件需要人；定位待办所属 Simon 卡片后返回。
// 预期：视频确实解码，卡片在视口中心，输入草稿和选中事项不丢失。
for(const v of [1,2,3])test(`style ${v}: video, conversation, decision and canvas`,async({page})=>{
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(`${pageUrl}?v=${v}`);
 await expect(page.getByTestId('conversation')).toBeVisible();
 await expect(page.getByTestId('pending-count')).toHaveText('3');
 await expect.poll(()=>page.locator('video').evaluate(v=>v.videoWidth)).toBe(1920);
 await page.getByTestId('composer').fill('先不发布，保留这个要求');
 await page.locator('[data-focus="login"]').first().click();
 await page.locator('[data-locate="login"]').first().click();
 await expect(page.locator('[data-node="simon"]')).toHaveAttribute('data-focused','true');
 expect(await page.locator('[data-node="simon"]').evaluate(el=>{const a=el.getBoundingClientRect(),v=el.closest('.canvas-viewport').getBoundingClientRect();return Math.abs(a.x+a.width/2-v.x-v.width/2)<4&&Math.abs(a.y+a.height/2-v.y-v.height/2)<4;}),'定位卡片应完整居中').toBe(true);
 await page.getByTestId('nav-norma').click();
 await expect(page.getByTestId('composer')).toHaveValue('先不发布，保留这个要求');
 await expect(page.locator('[data-active-decision="login"]')).toBeVisible();
 expect(errors,'三种风格不应有脚本错误').toEqual([]);
});

// 场景：采用方案只解除原问题，不把工作误标为完成。切换风格后读同一份状态。
// 数据：对登录问题采用保留数据方案；预期需人工数从 3 降到 2，原工作继续执行。
test('answer closes attention but does not complete work',async({page})=>{
 await page.goto(pageUrl);
 await page.locator('[data-focus="login"]').first().click();
 await page.getByTestId('apply-decision').first().click();
 await expect(page.getByTestId('pending-count')).toHaveText('2');
 await page.getByTestId('all-work').click();
 await expect(page.locator('[data-work="login"]')).toContainText('继续执行');
 await expect(page.locator('[data-work="login"]')).not.toContainText('已完成');
 await page.getByTestId('close-dialog').click();
 await page.locator('.proto-picker-item').nth(2).click();
 await expect(page.getByTestId('pending-count')).toHaveText('2');
});

// 场景：到画布接管会暂停自动派发，释放后才可采用推荐；验收并不自动合并。
// 数据：接管 Simon 再答登录问题；随后处理 Luna 的验收单。
// 预期：接管时操作被阻止，验收后工作明确保留待合并阶段。
test('takeover and review use honest work states',async({page})=>{
 await page.goto(`${pageUrl}?v=2`);
 await page.locator('[data-focus="login"]').first().click();
 await page.locator('[data-locate="login"]').first().click();
 await page.getByTestId('takeover').click();
 await page.getByTestId('nav-norma').click();
 await expect(page.getByTestId('apply-decision').first()).toBeDisabled();
 await page.locator('[data-locate="login"]').first().click();
 await page.getByTestId('takeover').click();
 await page.getByTestId('nav-norma').click();
 await expect(page.getByTestId('apply-decision').first()).toBeEnabled();
 await page.locator('[data-focus="navigation"]').first().click();
 await page.getByTestId('apply-decision').first().click();
 await page.getByTestId('all-work').click();
 await expect(page.locator('[data-work="navigation"]')).toContainText('待合并');
});

// 场景：对话、修改方案、已处理历史、语言与键盘比较器均可用。
// 数据：修改一次建议，再发送全局问题；各风格切换并使用英文。
// 预期：编辑原文有回执、历史可查、输入数字不切风格、视频能暂停。
test('edit, history, chat, locale and background controls',async({page})=>{
 await page.goto(pageUrl);
 await page.locator('[data-focus="login"]').first().click();
 await page.getByTestId('edit-decision').first().click();
 await page.getByTestId('decision-edit').fill('保留所有本地数据，只刷新登录状态');
 await page.getByTestId('submit-edit').click();
 await expect(page.getByTestId('pending-count')).toHaveText('2');
 await page.getByTestId('handled').click();
 await expect(page.getByRole('dialog')).toContainText('保留所有本地数据');
 await page.keyboard.press('Escape');
 await page.getByTestId('composer').fill('现在有什么需要我处理？');
 await page.getByTestId('send').click();
 await expect(page.locator('[data-chat-message]').last()).toContainText('2');
 await page.getByTestId('video-toggle').click();
 expect(await page.locator('video').evaluate(v=>v.paused),'视频暂停按钮应真实暂停背景').toBe(true);
 await page.getByTestId('language').click();
 await expect(page.getByTestId('nav-norma')).toContainText('Norma');
 for(let i=0;i<3;i++){
  await page.locator('.proto-picker-item').nth(i).click();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'英文页面不应横向溢出').toBe(true);
 }
 await page.getByTestId('composer').fill('1');await page.keyboard.press('2');
 await expect(page.locator('.proto-picker-item[data-active]')).toContainText('连续行动流');
});
