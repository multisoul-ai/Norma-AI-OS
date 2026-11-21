const {test,expect}=require('playwright/test');const path=require('node:path');const {pathToFileURL}=require('node:url');
const url=pathToFileURL(path.resolve(__dirname,'../index.html')).href;
test.use({channel:'chrome',viewport:{width:1600,height:1050}});
async function open(page,n=1){await page.goto(url+'?v='+n);await expect(page.locator('[data-variant-root]')).toBeVisible();}
// 场景：五种形态共用基准场景但布局与入口不同。执行：逐个切换并检查交互入口。
// 预期：五个不同布局存在、URL 与选中项一致、脚本无异常，数字键不打断输入。
test('five distinct forms render and picker preserves typing',async({page})=>{
 const errors=[];page.on('pageerror',e=>errors.push(e.message));await open(page);
 for(let n=1;n<=5;n++){await page.locator('.proto-picker-item').nth(n-1).click();await expect(page.locator('[data-variant-root]')).toHaveAttribute('data-variant-root',String(n));await expect(page).toHaveURL(new RegExp('v='+n));}
 await page.getByTestId('room-input').fill('先处理');await page.keyboard.press('1');await expect(page.getByTestId('room-input')).toHaveValue('先处理1');await expect(page.locator('[data-variant-root]')).toHaveAttribute('data-variant-root','5');
 expect(errors,'五种原型均不得有脚本异常').toEqual([]);
});
// 场景：指挥台明确选择两个 Agent 后派活。执行：确认接收者并查看逐目标回执。
// 预期：只有已选卡片收到消息，另一空间同名 Grace 不被波及。
test('control deck dispatches only to selected stable card identities',async({page})=>{
 await open(page);await page.locator('[data-select="login"]').check();await page.locator('[data-select="drawer"]').check();
 await page.getByTestId('global-input').fill('先整理各自的验收清单');await page.getByTestId('prepare-command').click();
 await expect(page.locator('#dispatch-dialog [data-target]')).toHaveCount(2);await page.getByTestId('confirm-dispatch').click();
 await expect(page.locator('#dispatch-dialog [data-receipt]')).toHaveCount(2);await page.locator('#dispatch-dialog [data-action="close-dialog"]').last().click();
 await page.locator('[data-chat="login"]').first().click();await expect(page.locator('#agent-dialog')).toContainText('先整理各自的验收清单');await page.keyboard.press('Escape');
 await page.locator('[data-chat="lab-login"]').first().click();await expect(page.locator('#agent-dialog')).not.toContainText('先整理各自的验收清单');
});
// 场景：任务地图揭示依赖，并把来源上下文交给另一个 Agent 协助。
// 执行：选中发布说明，查看阻塞链，选择助手并预览指令。
// 预期：依赖边突出、来源和接收者分别展示，不偷偷重建原会话。
test('map explains dependencies and hands context to an explicit helper',async({page})=>{
 await open(page,2);await page.locator('[data-map-node="site-notes"]').click();await page.getByTestId('show-dependencies').click();
 await expect(page.locator('[data-map-node="navigation"]')).toHaveClass(/dependency/);await page.getByTestId('request-help').click();
 await expect(page.locator('#dispatch-dialog')).toContainText('Echo');
 await page.locator('#helper-target').selectOption('navigation');await page.getByTestId('dispatch-input').fill('请同步打包验收结果，供发布说明使用');await page.getByTestId('confirm-dispatch').click();await expect(page.locator('[data-receipt="navigation"]')).toBeVisible();
 await page.keyboard.press('Escape');await page.locator('[data-map-node="navigation"]').click();await page.locator('[data-chat="navigation"]').click();
 await expect(page.locator('#agent-dialog .message.user').last()).toContainText('Echo');await expect(page.locator('#agent-dialog .message.user').last()).toContainText('Norma Website');
 await expect(page.locator('#agent-dialog .message.user').last()).toContainText('请同步打包验收结果，供发布说明使用');
});
// 场景：决策台逐项批阅。执行：稍后处理一条、撤销，然后接受验收建议。
// 预期：稍后可恢复，验收仅进入待合并，未直接标成完成。
test('decision desk defers reversibly and keeps review separate from merge',async({page})=>{
 await open(page,3);await page.getByTestId('defer-decision').click();await page.locator('[data-action="undo-defer"]').click();await expect(page.getByTestId('decision-title')).toContainText('卡顿');
 await page.locator('[data-decision="navigation"]').click();await page.getByTestId('accept-decision').click();await expect(page.getByTestId('decision-result')).toContainText('待合并');
});
// 场景：调度表批量控制只暂停运行中的 Agent，计划队列不伪造实际完成。
// 执行：选择一个运行中与一个等待中的 Agent 暂停，调整下一条计划。
// 预期：逐目标反馈不同，计划移到稍后仍保留当前运行阶段。
test('dispatch table separates runtime control from planned queue',async({page})=>{
 await open(page,4);await page.locator('[data-select="drawer"]').check();await page.locator('[data-select="meteor"]').check();await page.getByTestId('pause-selected').click();
 await expect(page.locator('[data-table-row="drawer"]')).toContainText('已暂停');await expect(page.locator('[data-table-row="meteor"]')).toContainText('等待中');
 await page.locator('[data-queue="site"]').click();await page.locator('[data-schedule="later"]').click();await expect(page.locator('[data-table-row="site"]')).toContainText('稍后');await expect(page.locator('[data-table-row="site"]')).toContainText('处理中');
});
// 场景：协作房间的并行目标各自拥有草稿与接收者。
// 执行：两个房间输入不同草稿，再指定一位 Agent 继续讨论。
// 预期：草稿不串房间；发送前显示实际目标，不广播给未参与者。
test('rooms keep parallel goals and drafts separate',async({page})=>{
 await open(page,5);await page.getByTestId('room-input').fill('先协调桌面验收');await page.locator('[data-room="site"]').click();await page.getByTestId('room-input').fill('网站下载入口保持不变');
 await page.locator('[data-room="release"]').click();await expect(page.getByTestId('room-input')).toHaveValue('先协调桌面验收');
 await page.locator('[data-room-target="navigation"]').first().click();await page.getByTestId('room-send').click();await expect(page.locator('#dispatch-dialog [data-target]')).toHaveCount(1);await expect(page.locator('#dispatch-dialog')).toContainText('Jasper');
});
// 场景：每个形态都可进入原卡片会话并定位画布。执行：写草稿、定位、从现场返回。
// 预期：消息上下文与浏览位置保持，每个形态恢复自己的状态。
test('agent conversation and canvas preserve the same draft',async({page})=>{
 await open(page);await page.locator('[data-chat="login"]').first().click();await page.getByTestId('agent-input').fill('保留这条现场草稿');await page.getByTestId('agent-locate').click();
 await expect(page.locator('[data-canvas-card="login"]')).toHaveClass(/focused/);await page.getByTestId('canvas-return').click();await expect(page.getByTestId('agent-input')).toHaveValue('保留这条现场草稿');
});
// 场景：比较选择与可访问性在小窗口及英文下仍有效。
// 执行：五形态逐个缩窄，英文与减少动态效果，标记偏好。
// 预期：无页面横向溢出，视频暂停，选择通过刷新保留。
test('comparison, preference and compact layouts remain usable',async({page})=>{
 await page.emulateMedia({reducedMotion:'reduce'});await open(page);expect(await page.locator('video').evaluate(v=>v.paused),'减少动态效果应暂停视频').toBe(true);
 await page.locator('[data-action="language"]').click();
 for(let n=1;n<=5;n++){await page.locator('.proto-picker-item').nth(n-1).click();await page.setViewportSize({width:900,height:900});await expect(page.locator('[data-variant-root]')).toBeVisible();expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'原型页面不得横向溢出').toBe(true);}
 await page.locator('[data-action="choose"]').click();await page.reload();await expect(page.locator('[data-action="choose"]')).toHaveAttribute('aria-pressed','true');
});
// 场景：地图切换到另一个工作空间后，原选中节点已不在范围内。
// 执行：切到实验室并邀请助手。预期：交接来源为当前显示的实验室 Grace，不沿用 Echo。
test('map handoff follows the visible selection after a scope change',async({page})=>{
 await open(page,2);await page.locator('#scope-space').selectOption('lab');await page.getByTestId('request-help').click();
 await expect(page.locator('.handoff-source')).toContainText('Grace');await expect(page.locator('.handoff-source')).not.toContainText('Echo');
});
// 场景：重置仅影响当前方案，且清除用户编辑的队列与筛选。
// 执行：在指挥台存草稿，在调度表改计划、筛选后重置。预期：调度表恢复默认，指挥台草稿保留。
test('reset restores the whole form without clearing another form',async({page})=>{
 await open(page);await page.getByTestId('global-input').fill('保留指挥台方向');await page.locator('.proto-picker-item').nth(3).click();
 await page.locator('[data-queue="site"]').click();await page.locator('#queue-text').fill('临时调整');await page.locator('[data-schedule="later"]').click();
 await page.locator('[data-table-search]').fill('Luna');await page.locator('[data-action="reset"]').click();
 await expect(page.locator('[data-table-search]')).toHaveValue('');await expect(page.locator('[data-table-row]')).toHaveCount(8);
 await expect(page.locator('[data-table-row="site"]')).toContainText('检查移动端下载引导');await expect(page.locator('[data-table-row="site"]')).not.toContainText('临时调整');
 await page.locator('.proto-picker-item').nth(0).click();await expect(page.getByTestId('global-input')).toHaveValue('保留指挥台方向');
});
// 场景：背景视频控制须反馈实际播放状态。执行：点击暂停再播放。
// 预期：按钮与视频状态一致，减少动态效果的系统偏好仍可自动暂停。
test('background control exposes its actual playback state',async({page})=>{
 await open(page);await page.locator('video').evaluate(v=>v.play());
 await expect(page.locator('[data-action="video"]')).toHaveAttribute('aria-pressed','true');await page.locator('[data-action="video"]').click();
 await expect(page.locator('[data-action="video"]')).toHaveAttribute('aria-pressed','false');expect(await page.locator('video').evaluate(v=>v.paused),'暂停按钮应真的暂停背景').toBe(true);
});
// 场景：延期多个决定后仍须能找回任意一项。执行：连续延期两条，再打开第一条。
// 预期：第一条回到待决策状态，第二条仍在稍后列表，其他工作状态不变。
test('every deferred decision remains reachable',async({page})=>{
 await open(page,3);await page.getByTestId('defer-decision').click();await page.getByTestId('defer-decision').click();
 await page.locator('[data-restore-decision="login"]').click();await expect(page.getByTestId('decision-title')).toContainText('卡顿');
 await expect(page.locator('[data-restore-decision="navigation"]')).toBeVisible();await expect(page.locator('[data-restore-decision="login"]')).toHaveCount(0);
});
// 场景：工作空间筛选同时约束全局与推荐侧栏。执行：切到实验室。
// 预期：仅显示当前空间的工作，不继续推荐主空间 Jasper / Echo 的交接。
test('control recommendations respect the workspace scope',async({page})=>{
 await open(page);await page.locator('#scope-space').selectOption('lab');
 await expect(page.locator('[data-chat="lab-login"]').first()).toBeVisible();await expect(page.locator('[data-handoff="navigation"]')).toHaveCount(0);
 await expect(page.locator('.control-aside')).not.toContainText('Henry');
});
