/**
 * 场景：1280×720 的 VR 浏览器预演，验证 Agent 等待范围决策。
 * 数据：内存演示任务。坐标来自 Computer Use 观察到的三块独立玻璃面板。
 * 执行：查看决策上下文、选择范围、暂停、切换 CLI 与成果，再继续并验收。
 * 预期：只有显式选择才能解锁任务；内容切换保留进度；确认后待办卡片收起。
 */
function checks(page) {
  const evidence = [];
  const check = async (pattern, message) => {
    await page.getByRole('img', {name: pattern}).waitFor({state:'visible', timeoutMs:35000});
    const value = await page.getByRole('img', {name:/^Norma 三维空间/}).getAttribute('aria-label');
    if (!pattern.test(value)) throw new Error(`${message}：${value}`);
    evidence.push(message);
    return value;
  };
  return {evidence, check};
}
export async function runNormaVRDecisionEntryE2E(page, pointer) {
  const {evidence, check} = checks(page);
  const click = (x, y) => pointer.click([x, y]);
  const content = async (x, y) => { await click(738, 535); await click(710, 356); await click(x, y); };
  await check(/主面板：Codex CLI.*验证 等你决策.*待决策 1 项/, '初始右侧存在阻塞验证任务的决策');
  await click(1127, 398);
  await check(/主面板：实时图表.*验证 等你决策.*待决策 1 项/, '查看决策上下文不会擅自推进任务');
  await click(1127, 433);
  await check(/验证 执行中.*待决策 0 项/, '选择范围后验证继续，决策卡片收起');
  await click(151, 480);
  const stopped = await check(/0 位正在执行.*验证 已暂停/, '独立任务面板可以暂停已解锁的执行');
  const progress = stopped.match(/任务进度 (\d+)%/)[1];
  await click(139, 354);
  await check(/主面板：Claude CLI.*验证 已暂停/, '点击 Agent 在玻璃主面板打开 Claude CLI');
  await content(861, 369);
  await check(new RegExp(`主面板：审查报告.*任务进度 ${progress}%`), '报告呈现时保留任务进度');
  await content(710, 415);
  await check(new RegExp(`主面板：实时图表.*任务进度 ${progress}%`), '图表与独立进度使用相同数据');
  await content(861, 415);
  await check(/主面板：网页与图片.*验证 已暂停/, '玻璃面板可以呈现网页与图片');
  await content(710, 459);
  await check(/主面板：三维对象.*验证 已暂停/, '三维对象与任务状态同时可见');
  return evidence;
}
export async function runNormaVRDecisionCompletionE2E(page, pointer) {
  const {evidence, check} = checks(page);
  const click = (x,y) => pointer.click([x,y]);
  await click(151, 480);
  await check(/任务进度 100%.*待决策 1 项.*这次改动/, '执行完成后自动出现成果验收决策');
  await click(1127, 398);
  await check(/主面板：代码改动.*待决策 1 项/, '查看 diff 保留待确认状态');
  await click(1127, 433);
  await check(/主面板：代码改动.*待决策 0 项/, '明确确认后待决策清空');
  return evidence;
}

export async function runNormaVRWorkflowE2E(page, pointer) {
  return [
    ...await runNormaVRDecisionEntryE2E(page, pointer),
    ...await runNormaVRDecisionCompletionE2E(page, pointer),
  ];
}
