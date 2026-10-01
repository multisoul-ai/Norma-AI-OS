const assert = require('node:assert/strict');
const { chromium } = require(process.argv[2] || 'playwright');
const path = require('node:path');

/**
 * 场景：真实页面的指令补全、直接创建、主题切换和窄屏保持可操作。
 *
 * 数据构造：
 *   初始卡片 1 张；执行一次创建后应为 1 + 1 = 2 张。
 *   桌面宽度 1440px 对应完整工作台；320px 对应最窄验收宽度。
 * 执行过程：
 *   1. 输入 /ne，通过两次 Tab 选择 /new 与 Codex。
 *   2. 检查只有一个直接创建项、无 footer、无位置候选，回车一次创建。
 *   3. 输入主题命令；模拟中文组词，验证组词中的 Enter 不执行。
 *   4. 结束组词后执行主题切换；Esc 收起时保留输入。
 *   5. 切到 320px 与暗色检查页面边界，保存桌面与窄屏证据。
 * 预期结果：
 *   - 只创建一张新卡片，原卡片保留；候选无布局步骤与 footer。
 *   - 组词中的 Enter 不执行；结束后主题正确切换。
 *   - Esc 不清空输入；无横向溢出，无 JavaScript 错误。
 */
(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto('http://127.0.0.1:8766/browser-preview.html');
    const field = page.getByRole('combobox');
    const stage = page.locator('#norma-command-prototype');
    assert.equal(await stage.getAttribute('data-theme'), 'graphite', '初始界面应采用截图中的石墨深色主题');
    assert.equal(await page.locator('.n-dock .n-tool').count(), 4, 'Dock 应只保留工作空间、布局、Voice 与设置');
    assert.equal(await page.getByRole('button', { name: 'Voice', exact: true }).isVisible(), true, 'Voice 必须保持可见');
    assert.equal(await page.getByRole('button', { name: '设置', exact: true }).isVisible(), true, '设置必须保持可见');
    assert.equal(await page.getByRole('button', { name: '新建 Agent', exact: true }).count(), 0, '创建按钮应被指令入口替代');
    await field.fill('/ne');
    await field.press('Tab');
    assert.equal(await field.inputValue(), '/new ', '第一次 Tab 应补全 /new');
    await field.press('Tab');
    assert.equal(await field.inputValue(), '/new codex', '第二次 Tab 应补全 Codex 命令（直接创建，无后续参数）');
    const options = page.getByRole('option');
    assert.equal(await options.count(), 1, '/new codex 应只有一个直接创建项');
    assert.match(await options.first().innerText(), /新建 Codex/, '候选应表达直接新建 Codex');
    assert.equal(await page.locator('.n-panel-footer, .n-preview, .n-ghost').count(), 0, '不应存在说明行、快捷键 footer 与位置预览');
    assert.equal(await page.getByText(/right|below|在选中卡片/).count(), 0, '不应出现布局位置候选');
    await stage.screenshot({ path: path.join(__dirname, 'placement.png') });
    await field.press('Enter');
    assert.equal(await page.locator('article.n-agent').count(), 1, '一次 Enter 应只创建一张卡片');
    assert.equal(await page.locator('.n-reference').count(), 1, '原卡片必须保留');
    await field.fill('/theme midnight-violet');
    await field.dispatchEvent('compositionstart');
    await field.press('Enter');
    assert.notEqual(await stage.getAttribute('data-theme'), 'midnight-violet', '输入法组词中的 Enter 不应执行主题切换');
    await field.dispatchEvent('compositionend', { data: '' });
    await field.press('Enter');
    assert.equal(await stage.getAttribute('data-theme'), 'midnight-violet', '主题命令应切换到 Midnight Violet');
    await field.fill('/theme');
    await field.press('Escape');
    assert.equal(await page.locator('.n-panel').isVisible(), false, 'Esc 应收起候选');
    assert.equal(await field.inputValue(), '/theme', 'Esc 不应清除草稿');
    await field.press('ArrowDown');
    assert.equal(await page.locator('.n-panel').isVisible(), true, '方向键应重新打开候选');
    await page.setViewportSize({ width: 320, height: 900 });
    const bounds = await page.evaluate(() => ({ width: innerWidth, content: document.documentElement.scrollWidth }));
    assert.ok(bounds.content <= bounds.width, '320px 窄屏不应横向溢出：' + JSON.stringify(bounds));
    await stage.screenshot({ path: path.join(__dirname, 'narrow.png') });
    await page.setViewportSize({ width: 1440, height: 960 });
    await field.fill('创建一个 Codex');
    assert.equal(await page.getByRole('option').count(), 1, '完整中文示例应收敛为一个明确动作');
    await stage.screenshot({ path: path.join(__dirname, 'natural-language.png') });
    await page.reload();
    await stage.screenshot({ path: path.join(__dirname, 'suggestions.png') });
    assert.equal(errors.length, 0, '浏览器不应有 JavaScript 错误：' + errors.join('; '));
    console.log('浏览器检查通过：Tab、方向键、Enter、Esc、中文组词、直接创建、主题与 320px 布局。');
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
