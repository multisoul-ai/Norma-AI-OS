// 场景：验证三种原型的导航去重、草稿保留与关键交互。
// 数据：4 个初始任务、3 个项目；2 项需要用户处理、1 项执行中。
// 过程：逐个打开布局，检查单一全局导航；再操作筛选、详情、草稿、派发演示与窄窗。
// 预期：导航不重复，草稿不丢失，未采用不运行；采用后计划完成，窗口不横向溢出。
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const out = process.env.PROTOTYPE_EVIDENCE || path.resolve('artifacts/staff-m1/material');
const base = process.env.PROTOTYPE_URL || 'http://127.0.0.1:8850/';
fs.mkdirSync(out, { recursive: true });
(async () => {
  const browser = await chromium.launch({ headless: true, channel: 'chrome' });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  page.on('console', m => { if (m.type() === 'error') errors.push(m.text()); });
  try {
    for (const v of [1, 2, 3]) {
      await page.goto(`${base}?v=${v}`);
      await page.getByRole('heading', { name: '开发 HTML 贪吃蛇游戏', exact: true }).waitFor();
      assert.equal(await page.getByRole('combobox', { name: '切换工作空间' }).count(), 1, '工作空间选择器必须只有一处');
      assert.equal(await page.getByRole('button', { name: '画布', exact: true }).count(), 1, '画布入口必须只有一处');
      assert.equal(await page.getByRole('button', { name: '幕僚', exact: true }).count(), 1, '幕僚入口必须只有一处');
      await page.screenshot({ path: path.join(out, `variant-${v}.png`) });
      await page.getByRole('button', { name: '查看执行详情', exact: true }).click();
      await page.getByRole('complementary', { name: '执行详情' }).waitFor();
      await page.getByRole('button', { name: '关闭详情', exact: true }).click();
      assert.equal(await page.getByRole('complementary', { name: '执行详情' }).count(), 0, '关闭后不应残留详情面板');
      await page.getByRole('button', { name: '画布', exact: true }).click();
      await page.getByRole('region', { name: '画布预览' }).waitFor();
      await page.getByRole('button', { name: '幕僚', exact: true }).click();
      await page.getByRole('heading', { name: '开发 HTML 贪吃蛇游戏', exact: true }).waitFor();
    }
    await page.goto(`${base}?v=1`);
    await page.getByRole('button', { name: '需要你 2', exact: true }).click();
    assert.equal(await page.locator('.task-row').count(), 2, '需要你应包括启动问题和待验收两项');
    assert.equal(await page.locator('.task-row[data-task="site"]').count(), 0, '执行中任务不应出现在需要你筛选');
    await page.getByRole('button', { name: '全部 4', exact: true }).click();
    await page.getByRole('textbox', { name: '给 Norma 发消息' }).fill('第一行草稿\n第二行草稿');
    await page.locator('.task-row[data-task="site"]').click();
    assert.equal(await page.getByRole('textbox', { name: '给 Norma 发消息' }).inputValue(), '', '其他任务不应带入原任务草稿');
    await page.locator('.task-row[data-task="snake"]').click();
    assert.equal(await page.getByRole('textbox', { name: '给 Norma 发消息' }).inputValue(), '第一行草稿\n第二行草稿', '返回任务必须保留两行草稿');
    await page.getByRole('combobox', { name: '筛选项目' }).selectOption('官网');
    assert.equal(await page.locator('.task-row').count(), 1, '官网筛选只显示官网任务');
    await page.getByRole('combobox', { name: '筛选项目' }).selectOption('所有项目');
    await page.getByRole('button', { name: '搜索任务', exact: true }).click();
    await page.getByRole('textbox', { name: '搜索任务或成员' }).fill('no-match');
    assert.equal(await page.locator('.task-row').count(), 0, '不匹配搜索必须隐藏任务行');
    await page.getByRole('button', { name: '清除搜索', exact: true }).click();
    await page.getByRole('button', { name: '重新提议', exact: true }).click();
    const adopt = page.getByRole('button', { name: '采用并派发', exact: true });
    await adopt.waitFor();
    assert.equal(await page.locator('.waiting').count(), 0, '采用前不应开始执行演示');
    await adopt.click();
    await page.getByText('正在等待 Worker 就绪…', { exact: true }).waitFor();
    await page.getByText('3/3 完成', { exact: true }).waitFor();
    assert.equal(await page.getByRole('combobox', { name: '任务阶段' }).inputValue(), '待验收', '执行演示完成后应推进待验收');
    await page.screenshot({ path: path.join(out, 'progress.png') });
    await page.getByRole('button', { name: '验收通过', exact: true }).click();
    await page.getByRole('textbox', { name: '给 Norma 发消息' }).fill('补充一项验收要求');
    await page.getByRole('button', { name: '发送消息', exact: true }).click();
    await page.getByText('正在整理你的要求…', { exact: true }).waitFor();
    assert.equal(await page.getByRole('combobox', { name: '任务阶段' }).inputValue(), '进行中', '已完成任务有新消息时应重开');
    await page.getByRole('button', { name: '查看执行提议', exact: true }).waitFor();
    await page.getByRole('combobox', { name: '切换工作空间' }).selectOption('个人工作空间');
    assert.equal(await page.locator('.task-row').count(), 0, '切换空工作空间不得泄漏原任务');
    await page.getByRole('button', { name: '新任务', exact: true }).first().click();
    await page.getByLabel('你想完成什么？').fill('个人任务');
    await page.getByRole('button', { name: '创建任务', exact: true }).click();
    await page.getByRole('heading', { name: '个人任务', exact: true }).waitFor();
    await page.getByRole('combobox', { name: '切换工作空间' }).selectOption('Workspace 1');
    assert.equal(await page.getByRole('heading', { name: '个人任务', exact: true }).count(), 0, '其他工作空间的新任务不得泄漏回来');
    await page.getByRole('button', { name: '切换浅色 / 深色', exact: true }).click();
    await page.screenshot({ path: path.join(out, 'light.png') });
    for (const v of [1, 2, 3]) {
      await page.setViewportSize({ width: 600, height: 800 });
      await page.goto(`${base}?v=${v}`);
      await page.getByRole('heading', { name: '开发 HTML 贪吃蛇游戏', exact: true }).waitFor();
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `布局 ${v} 不应横向溢出`);
      const input = page.getByRole('textbox', { name: '给 Norma 发消息' });
      await input.fill('窄窗草稿');
      const box = await page.getByRole('button', { name: '发送消息', exact: true }).boundingBox();
      assert.ok(box && box.x >= 0 && box.x + box.width <= 600, `布局 ${v} 的发送按钮应位于窗口内`);
      await page.screenshot({ path: path.join(out, `narrow-${v}.png`) });
    }
    assert.deepEqual(errors, [], '浏览器不应产生 JavaScript 或控制台错误');
    fs.writeFileSync(path.join(out, 'report.json'), JSON.stringify({ result: 'pass', variants: 3, consoleErrors: errors, checks: ['navigation uniqueness', 'details', 'canvas return', 'filters', 'project filter', 'search', 'draft isolation', 'approval gate', 'progress', 'reopen', 'workspace isolation', 'new task', 'light theme', '600px layout'] }, null, 2));
    console.log('PASS: 3 variants, 14 behavior groups, no console errors');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
