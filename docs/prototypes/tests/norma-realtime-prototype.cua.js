// Call these functions with a Playwright Page or a CUA tab.playwright binding.
// Open docs/prototypes/norma-realtime.html?variant=A before the flow check.
const check = (condition, message) => { if (!condition) throw new Error(message); };
const button = (page, name) => page.getByRole('button', {name, exact:true});

/// [频道协作闭环]：保存想法后，用户明确开始，Norma 派工并把成果带回同一频道。
/// 数据构造：初始 3 个频道 + 新建 1 个频道 = 4 个；新频道开始前 0 张卡，开始后 2 张卡。
/// 执行过程：1. 保存频道；2. 开始并 @Codex；3. 查看上下文和语音转写；4. 模拟完成并验收；5. 归档与重开。
/// 预期结果：Agent 回执、证据、转写与状态均可见；仅保存时不执行，续聊不复制卡片，未验收时不归档。
export async function testNormaChannelFlow(page) {
  const title = '让手机交接更清楚';
  await button(page, '保存新想法').click();
  await page.getByRole('textbox', {name:'你的想法'}).fill(title);
  await button(page, '保存为频道').click();
  check((await page.locator('.channel-intro .subtitle').innerText()).includes('尚未执行'), '保存后的频道必须明确标为尚未执行');
  check(await page.locator('.task-card').count() === 0, '仅保存想法不能派工');
  await button(page, '让 Norma 开始').click();
  check(await page.locator('.task-card').count() === 2, '明确开始后必须有两张具名 Agent 卡片');
  check((await page.locator('.stream').innerText()).includes('Codex 已接收') || (await page.locator('.stream').innerText()).includes('Codex\n已接收'), 'Codex 必须在频道留下接收回执');
  await page.getByRole('textbox', {name:'频道消息'}).fill('@Codex 请继续检查 API 状态');
  await button(page, '发送频道消息').click();
  check((await page.locator('.stream').innerText()).includes('收到。我会沿当前频道目标'), '@Codex 指令必须有明确回应');
  check(await page.locator('.task-card').count() === 2, '活跃 Agent 续聊不能复制任务卡片');
  await page.getByRole('button', {name:/工作简报/}).click();
  const context = await page.locator('.context-page').innerText();
  check(context.includes(title) && context.includes('iOS App') && context.includes('Norma API'), '频道应共享目标和两个关联项目');
  check(!context.includes('部署前检查'), '其他频道的消息不能混入当前上下文');
  await button(page, '返回频道').click();
  await button(page, '在此频道与 Norma 通话').click();
  await button(page, '演示说一句：@Codex 继续检查 API').click();
  await button(page, '挂断').click();
  check((await page.locator('.stream').innerText()).includes('实时对话转写'), '挂断后语音转写必须回到频道');
  check(await page.locator('.task-card').count() === 2, '挂断不能取消或复制 Agent 任务');
  await button(page, '频道操作').click();
  await button(page, '打开演示操作').click();
  await button(page, '模拟当前频道完成任务').click();
  check(await button(page, `待验收：${title}`).count() === 1, '成果只应产生一个待验收事项');
  await button(page, `待验收：${title}`).click();
  check((await page.locator('.detail-panel').innerText()).includes('验证'), '成果页必须展示证据');
  await button(page, '验收通过').click();
  await button(page, '验收已完成 · 归档频道').click();
  check((await page.locator('.channel-intro .subtitle').innerText()).includes('已归档'), '验收后频道应可归档');
  await button(page, '重开频道').click();
  check((await page.locator('.channel-intro .subtitle').innerText()).includes('待继续'), '重开后频道应恢复为待继续');
  check((await page.locator('.stream').innerText()).includes('实时对话转写'), '重开不得丢失原频道转写');
}

/// [工作空间项目配置]：频道即工作空间，用户可在该频道设置里添加已注册电脑上的项目。
/// 数据构造：初始有 2 台电脑与 3 个现有项目；模拟注册 Studio Mini 后为 3 台电脑，但新电脑仍有 0 个项目。
/// 执行过程：1. 查看全局设备并模拟扫码注册；2. 进入未执行频道的工作空间设置；3. 选择 Release Ops；4. 检查简报和频道回执。
/// 预期结果：项目只属于当前频道；可见项目来源；新电脑不虚构项目，用户可从同一设置页移除项目。
export async function testWorkspaceProjectSettings(page) {
  await button(page, '已连接电脑').click();
  check((await page.getByRole('dialog', {name:'已连接电脑'}).innerText()).includes('Alan Studio'), '设备列表应展示已注册电脑');
  await button(page, '扫码添加电脑').click();
  check((await page.getByRole('dialog', {name:'扫码添加电脑'}).innerText()).includes('模拟扫码'), '原型应明确说明扫码为模拟');
  await button(page, '确认注册 Studio Mini').click();
  check((await page.getByRole('dialog', {name:'已连接电脑'}).innerText()).includes('Studio Mini'), '注册后设备列表应出现 Studio Mini');
  await button(page, '完成').click();
  await button(page, '打开频道：新人首次连接体验').click();
  await button(page, '工作空间设置').click();
  await button(page, '添加已有项目').click();
  const projects = await page.getByRole('dialog', {name:'工作空间项目'}).innerText();
  check(projects.includes('Release Ops') && projects.includes('Build Mac'), '项目选择页应显示现有项目与所属电脑');
  check(projects.includes('Studio Mini') && projects.includes('尚无项目'), '新注册电脑不能自动产生项目');
  await page.getByRole('button', {name:/^关联 Release Ops/}).click();
  check((await page.getByRole('dialog', {name:'工作空间项目'}).innerText()).includes('已关联'), '选择后应立即显示关联状态');
  await button(page, '完成').click();
  await button(page, '工作简报').click();
  check((await page.locator('.brief-page').innerText()).includes('Release Ops'), '工作简报应显示当前频道关联的项目');
  await button(page, '返回频道').click();
  check((await page.locator('.stream').innerText()).includes('关联项目：Release Ops'), '项目变化应留在频道记录');
  await button(page, '工作空间设置').click();
  await button(page, '管理已关联项目').click();
  await page.getByRole('button', {name:/^移除 Release Ops/}).click();
  check((await page.getByRole('dialog', {name:'工作空间项目'}).innerText()).includes('尚未关联项目'), '移除后该频道项目应为空');
}

/// [三种首屏与手机触控]：频道、虚拟人和待办方向可切换，状态保留且主控件可触摸。
/// 数据构造：A/B/C 共 3 种首屏；iOS 主触控目标下限为 44pt。
/// 执行过程：1. 从 A 切 B、切 C；2. 读取首屏主区域与关键按钮的未缩放尺寸。
/// 预期结果：B 展示 Norma，C 展示待办；A/B/C 方向不同，关键触控目标不低于 44pt。
export async function testNormaChannelVariants(page) {
  check(await page.locator('main[aria-label="频道列表"]').count() === 1, 'A 首屏应是频道列表');
  await button(page, '下一种视觉方案').click();
  check(await page.locator('main[aria-label="Norma 实时陪伴"]').count() === 1, 'B 首屏应以 Norma 虚拟人为主');
  await button(page, '下一种视觉方案').click();
  check(await page.locator('main[aria-label="收件箱"]').count() === 1, 'C 首屏应聚焦待办');
  check(await page.locator('main[aria-label="频道列表"]').count() === 0, 'C 首屏不能退化为频道列表');
  const small = await page.evaluate(() => Array.from(document.querySelectorAll('.tabbar button,.icon-button,.segmented button,.mention-row button,.composer button,.primary,.secondary')).filter(el => el.offsetWidth < 44 || el.offsetHeight < 44).map(el => `${el.textContent.trim()}: ${el.offsetWidth}×${el.offsetHeight}`));
  check(small.length === 0, `主要触控目标必须至少 44pt：${small.join(', ')}`);
}
