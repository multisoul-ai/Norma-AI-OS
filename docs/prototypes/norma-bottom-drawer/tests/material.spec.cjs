const {test,expect}=require('playwright/test');
const path=require('node:path');const {pathToFileURL}=require('node:url');
const url=pathToFileURL(path.resolve(__dirname,'../index.html')).href;
test.use({channel:'chrome',viewport:{width:1600,height:1050}});
const open=async page=>{await page.goto(url);await page.getByTestId('dock-open').click()};
// 场景：Material 搜索浮层只在确认时改变已选范围。
// 数据：Norma OS 与实验室两个空间。执行：键盘浏览、取消、搜索无结果、重新搜索并确认。
// 预期：浏览不选中，空态有反馈；Escape 不关闭工作抽屉，确认和取消都恢复触发器焦点。
test('material scope separates browsing from selection and restores focus',async({page})=>{
 await open(page);const trigger=page.locator('[data-action="workspace"]');await trigger.focus();await page.keyboard.press('ArrowDown');
 await expect(page.locator('.scope-search')).toBeFocused();await page.keyboard.press('ArrowDown');
 await expect(page.locator('[data-option="lab"]')).toHaveClass(/browsing/);await expect(page.locator('[data-option="product"]')).toHaveAttribute('aria-selected','true');
 await page.keyboard.press('Escape');await expect(page.locator('#work-drawer')).toBeVisible();await expect(trigger).toBeFocused();await expect(trigger).toContainText('Norma OS');
 await trigger.click();await page.locator('.scope-search').fill('missing workspace');await expect(page.getByTestId('scope-empty')).toBeVisible();
 await page.keyboard.press('Enter');await expect(trigger).toContainText('Norma OS');await expect(page.locator('#scope-popover')).toBeVisible();
 await page.locator('.scope-search').fill('实验');await page.keyboard.press('Enter');await expect(trigger).toContainText('实验室');await expect(trigger).toBeFocused();await expect(page.locator('[data-work]')).toHaveCount(1);
 await trigger.click();await page.keyboard.press('Tab');await expect(page.locator('#scope-popover')).not.toBeVisible();await expect(trigger).toBeFocused();
});
// 场景：任务搜索与状态筛选缩小列表，不打断右侧正在输入的会话。
// 数据：Grace 草稿和两条需要决定的工作。执行：筛选、搜索 Kevin、清空及空搜索。
// 预期：计数与匹配准确，对话保持 Grace，切回后草稿和输入位置仍在。
test('task filters preserve the current conversation and expose empty results',async({page})=>{
 await open(page);await page.getByTestId('thread-input').fill('采样时保留全部现场');
 await page.locator('[data-filter="needs"]').click();await expect(page.locator('[data-work]')).toHaveCount(2);await expect(page.locator('[data-filter="needs"]')).toHaveAttribute('aria-pressed','true');
 await page.locator('[data-filter="all"]').click();await page.getByTestId('work-search').fill('Kevin');await expect(page.locator('[data-work]')).toHaveCount(1);
 await expect(page.getByTestId('thread-title')).toContainText('卡顿');await expect(page.getByTestId('thread-input')).toHaveValue('采样时保留全部现场');await expect(page.getByTestId('filtered-conversation')).toBeVisible();
 await page.getByTestId('work-search').fill('no matching task');await expect(page.getByTestId('work-empty')).toBeVisible();await page.locator('[data-action="clear-filters"]').first().click();
 await expect(page.locator('[data-work]')).toHaveCount(7);await expect(page.getByTestId('thread-input')).toHaveValue('采样时保留全部现场');
});
// 场景：键盘聚焦列表项目不立即切换对话；通知只在用户选择“查看”后切换。
// 执行：下方向键浏览、Enter 确认、模拟新进展后点击 Snackbar 操作。
// 预期：当前行与焦点分离；通知不抢任务，明确点击后才打开 Kevin。
test('row keyboard browsing and snackbar actions are explicit',async({page})=>{
 await open(page);await page.locator('[data-work="login"]').focus();await page.keyboard.press('ArrowDown');
 await expect(page.locator('[data-work="navigation"]')).toBeFocused();await expect(page.getByTestId('thread-title')).toContainText('卡顿');
 await page.keyboard.press('Enter');await expect(page.getByTestId('thread-title')).toContainText('PR #182');await expect(page.locator('[data-work="navigation"]')).toBeFocused();
 await page.getByTestId('simulate-event').click();await expect(page.getByTestId('thread-title')).toContainText('PR #182');
 await page.locator('#notice [data-snackbar-action]').click();await expect(page.getByTestId('thread-title')).toContainText('底部全局工作抽屉');
});
// 场景：浅深主题下 Material 状态反馈清楚可辨，而且不改变控件尺寸。
// 执行：读取任务行静止、悬停、按下状态并比较；切换主题再重复。
// 预期：状态层不同、选中标记独立，按下不会移动命中区域。
test('state layers are distinct in both themes without layout shifts',async({page})=>{
 await open(page);await expect.poll(()=>page.locator('.sheet').evaluate(e=>new DOMMatrixReadOnly(getComputedStyle(e).transform).m42)).toBe(0);
 for(const theme of ['dark','light']){
  if(theme==='light')await page.locator('#drawer-dock [data-action="theme"]').click();
  const row=page.locator('[data-work="navigation"]');await row.scrollIntoViewIfNeeded();await page.mouse.move(5,5);const normal=await row.evaluate(e=>getComputedStyle(e).backgroundColor);const before=await row.boundingBox();
  await row.hover();const hover=await row.evaluate(e=>getComputedStyle(e).backgroundColor);expect(hover,`${theme} 悬停不能等于静止背景`).not.toBe(normal);
  await page.mouse.down();const pressed=await row.evaluate(e=>getComputedStyle(e).backgroundColor);expect(pressed,`${theme} 按下必须区别于悬停`).not.toBe(hover);
  expect(await row.boundingBox(),`${theme} 状态变化不得移动行`).toEqual(before);await page.mouse.move(5,5);await page.mouse.up();await expect(page.locator('#work-drawer'),'从行内按下后移出松手只能取消点击，不能关闭抽屉').toBeVisible();
  await expect(page.locator('[data-work="login"] .selection-check')).toBeVisible();
 }
});
// 场景：窄窗口中任务列表和会话分屏呈现，聚焦不能落在已隐藏列表。
// 执行：返回列表、点击项目、搜索菜单取消，收起重新打开。
// 预期：切换后的焦点位于可见会话标题，取消后仍处于同一对话。
test('compact navigation focuses visible content and preserves the thread',async({page})=>{
 await page.setViewportSize({width:390,height:900});await open(page);
 await page.locator('[data-action="list"]').click();await page.locator('[data-work="navigation"]').click();await expect(page.getByTestId('thread-title')).toBeFocused();
 await page.locator('[data-action="workspace"]').click();await page.keyboard.press('Escape');await expect(page.locator('[data-action="workspace"]')).toBeFocused();
 await page.keyboard.press('Escape');await page.keyboard.press('Meta+e');await expect(page.getByTestId('thread-title')).toBeFocused();await expect(page.getByTestId('thread-title')).toContainText('PR #182');
});
