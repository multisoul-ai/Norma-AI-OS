const {test,expect}=require('playwright/test');
test.use({channel:'chrome',viewport:{width:1500,height:980}});
const base='http://127.0.0.1:8745/docs/prototypes/norma-inbox/';
async function pair(page){await page.goto(base+'?window=canvas&demo='+Date.now()+'-'+Math.random());const popup=page.waitForEvent('popup');await page.locator('[data-action="open-inbox"]').click();const tasks=await popup;await expect(tasks.locator('#inbox')).toBeVisible();return tasks;}
// 场景：画布与任务页应是两个独立浏览器窗口。执行：从画布打开任务页、选择任务、定位卡片。
// 预期：画布没有模态遮罩；选择任务不移动画布，明确定位才移动；任务窗保持打开。
test('tasks open in a second window and only explicit location moves the canvas',async({page})=>{
 const tasks=await pair(page);expect(tasks,'任务页必须是独立页面对象').not.toBe(page);
 await expect(page.locator('#inbox')).not.toBeVisible();const original=await page.locator('#canvas-world').getAttribute('style');
 await tasks.locator('[data-work="login"]').click();await expect(page.locator('#canvas-world')).toHaveAttribute('style',original);
 await tasks.locator('[data-action="locate"]').click();await expect(page.locator('[data-card="login"]')).toHaveClass(/focused/);await expect(tasks.locator('.thread')).toBeVisible();
 await expect(tasks.locator('#canvas')).not.toBeVisible();await expect(page.locator('#inbox')).not.toBeVisible();
});
// 场景：两个窗口继续同一张卡片。执行：任务窗写草稿、画布发送，再反向发送。
// 预期：草稿和消息双向同步，没有同名实验室 Grace 串线，收件箱选中项保持。
test('two windows share the original draft and messages without mixing same-name agents',async({page})=>{
 const tasks=await pair(page);await tasks.locator('[data-work="login"]').click();await tasks.locator('[data-action="locate"]').click();
 await tasks.getByTestId('thread-input').fill('来自独立 Norma 窗口');await expect(page.getByTestId('canvas-input')).toHaveValue('来自独立 Norma 窗口');
 await page.getByTestId('canvas-send').click();await expect(tasks.locator('.thread-history')).toContainText('来自独立 Norma 窗口');await expect(tasks.getByTestId('thread-input')).toHaveValue('');
 await tasks.getByTestId('thread-input').fill('继续在同一张卡片处理');await tasks.getByTestId('thread-send').click();await expect(page.locator('[data-card="login"] .canvas-terminal')).toContainText('继续在同一张卡片处理');
 await tasks.locator('[data-work="lab-login"]').click();await expect(tasks.locator('.thread-history')).not.toContainText('来自独立 Norma 窗口');await expect(page.locator('[data-card="login"]')).toHaveClass(/focused/);
});
// 场景：关闭任务窗口不能影响画布及会话。执行：记录草稿、关闭窗口，再从同一画布呼出。
// 预期：画布仍可操作，任务窗恢复同一任务和草稿；重复呼出复用已有任务窗口。
test('closing and reopening the task window preserves work and reuses the window',async({page,context})=>{
 const tasks=await pair(page);await tasks.locator('[data-work="navigation"]').click();await tasks.getByTestId('thread-input').fill('重新打开后继续验收');
 await expect.poll(()=>tasks.evaluate(()=>window.__pendingSyncCount),'关闭前共享状态应已保存').toBe(0);
 await tasks.locator('[data-action="hide-inbox"]').last().click();await expect.poll(()=>tasks.isClosed(),'只关闭独立任务窗口').toBe(true);await expect(page.locator('#canvas')).toBeVisible();
 const popup=page.waitForEvent('popup');await page.locator('[data-action="open-inbox"]').click();const reopened=await popup;await expect(reopened.getByTestId('thread-input')).toHaveValue('重新打开后继续验收');
 const count=context.pages().length;await page.locator('[data-action="open-inbox"]').click();await expect.poll(()=>context.pages().length,'重复呼出应复用窗口').toBe(count);
});
// 场景：同时在两窗追加不同任务的消息。执行：同一时刻向两个独立会话发送。
// 预期：共享存储合并增量，无整份快照覆盖导致消息丢失。
test('concurrent writes to independent sessions survive in both windows',async({page})=>{
 const tasks=await pair(page);await tasks.locator('[data-work="login"]').click();await tasks.locator('[data-action="locate"]').click();await tasks.locator('[data-work="lab-login"]').click();
 await Promise.all([page.getByTestId('canvas-input').fill('主空间并行消息'),tasks.getByTestId('thread-input').fill('实验室并行消息')]);
 await Promise.all([page.getByTestId('canvas-send').click(),tasks.getByTestId('thread-send').click()]);
 await expect(tasks.locator('.thread-history')).toContainText('实验室并行消息');await tasks.locator('[data-work="login"]').click();await expect(tasks.locator('.thread-history')).toContainText('主空间并行消息');await expect(tasks.locator('.thread-history')).not.toContainText('实验室并行消息');
});
