/**
 * 场景：五个独立任务在 VR 中同时等待决策，每页四项。
 * 数据：初始内存任务；坐标来自真实 1280×720 浏览器观察。
 * 执行：任意选项、切换详情、翻页、处理末页和中间项、查看上下文，再逐项解决。
 * 预期：详情不取代列表，决定只移除对应项，末页清空后回到有效页，主任务验证不会被其他决定解锁。
 */
export async function runNormaVRQueueE2E(page, pointer) {
  const evidence = [];
  const check = async (pattern, message) => {
    await page.getByRole('img', {name:pattern}).waitFor({state:'visible',timeoutMs:10000});
    const value = await page.getByRole('img', {name:/^Norma 三维空间/}).getAttribute('aria-label');
    if (!pattern.test(value)) throw new Error(`${message}：${value}`);
    evidence.push(message);
  };
  const click=(x,y)=>pointer.click([x,y]);
  await check(/待决策 5 项.*第 1\/2 页：终端焦点修复、空间首页、演示站点、上手文档/, '首屏提供五项总数与四个不同任务');
  await click(1120,314);
  await check(/待决策 5 项.*正在查看决策：空间首页/, '可直接打开第二项，队列仍然保留');
  await click(1120,370);
  await check(/待决策 5 项.*正在查看决策：演示站点/, '展开详情时仍可切换其他任务');
  await click(815,71);
  await check(/待决策 5 项.*决策详情已关闭/, '关闭详情不会丢弃待办');
  await click(1198,500);
  await check(/待决策 5 项.*第 2\/2 页：兼容性回归/, '下一页能访问第五个任务');
  await click(1120,260);
  await check(/正在查看决策：兼容性回归/, '末页任务有自己的详情');
  await click(640,513);
  await check(/验证 等你决策.*待决策 4 项.*第 1\/1 页.*决策详情已关闭/, '只解锁兼容性任务，清空末页后返回有效列表');
  await click(1120,314); await click(640,575);
  await check(/待决策 3 项.*第 1\/1 页：终端焦点修复、演示站点、上手文档/, '处理首页替代方案只移除首页任务');
  await click(1120,314); await click(640,513);
  await check(/待决策 2 项.*第 1\/1 页：终端焦点修复、上手文档/, '处理演示分享保留其他待办');
  await click(1120,314); await click(640,446);
  await check(/主面板：审查报告.*待决策 2 项.*决策详情已关闭/, '查看文档上下文不代表批准');
  await click(1120,314); await click(640,513);
  await check(/待决策 1 项.*第 1\/1 页：终端焦点修复/, '文档决定不改变主任务验证状态');
  await click(1120,260);
  await check(/验证 等你决策.*正在查看决策：终端焦点修复/, '最后一项仍可正常打开与处理');
  await click(640,513);
  await check(/验证 执行中.*待决策 0 项.*决策详情已关闭/, '逐项处理后队列清空并解锁主任务');
  return evidence;
}
