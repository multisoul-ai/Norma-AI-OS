// Scenario: choose a palette in the HTML prototype, compare existing colors, and try
// both product scenes. This checks the prototype only, not the native GPUI App.
// Data: five fixed candidate palettes, two original-color references, local wallpaper.
// Steps: select every candidate, save both scenes, exercise input/focus/selection,
// reload a favorite, verify URL and narrow-width behavior. Expected: no errors,
// clipped application columns, lost selection, or interactions that leave the page.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs/promises');
const { pathToFileURL } = require('node:url');

(async () => {
  const output = path.resolve(__dirname, '../../../artifacts/theme-candidates');
  await fs.mkdir(output, { recursive: true });
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1560, height: 1260 }, deviceScaleFactor: 1 });
    const errors = [];
    page.on('pageerror', e => errors.push(e.message));
    page.on('console', m => { if (m.type() === 'error') errors.push(m.text()); });
    const url = pathToFileURL(path.join(__dirname, 'index.html')).href;
    const ids = ['moonstone', 'smoked-plum', 'moss-ink', 'deep-petrol', 'burnished-copper'];
    await page.goto(url);
    for (let i = 0; i < ids.length; i++) {
      await page.locator('.proto-picker-item').nth(i).click();
      await page.waitForSelector('[data-palette="' + ids[i] + '"]');
      assert.equal(await page.locator('.proto-picker-item[data-active]').count(), 1, 'Exactly one candidate must be selected');
      assert.equal(new URL(page.url()).searchParams.get('v'), String(i + 1), 'Selected palette must survive in the URL');
      await page.locator('[data-scene="workspace"]').click();
      await page.waitForSelector('.workspace');
      await page.waitForFunction(() => document.querySelector('.wallpaper')?.naturalWidth > 1000);
      assert.equal(await page.locator('.wallpaper').isVisible(), true, 'The real default wallpaper must render behind the translucent surfaces');
      assert.equal(await page.locator('.drawer').count(), 2, 'Both drawers must remain present');
      assert.equal(await page.locator('.terminal').count(), 2, 'Both terminal cards must remain present');
      const tint = await page.locator('.app').evaluate(e => getComputedStyle(e).getPropertyValue('--tint-from'));
      assert.match(tint, /,0\.6\)/, 'Every candidate must use the agreed 60% drawer tint');
      await page.screenshot({ path: path.join(output, ids[i] + '-workspace.png'), fullPage: true });
      await page.locator('[data-scene="home"]').click();
      await page.waitForSelector('.home-scene');
      await page.screenshot({ path: path.join(output, ids[i] + '-home.png'), fullPage: true });
      await page.getByRole('button', { name: '打开工作空间 →' }).click();
      await page.waitForSelector('.workspace');
    }
    await page.locator('#reference').selectOption('violet');
    await page.locator('#compare').click();
    await page.waitForSelector('[data-palette="old-violet"]');
    await page.locator('#reference').selectOption('green');
    await page.waitForSelector('[data-palette="old-green"]');
    await page.locator('#compare').click();
    await page.waitForSelector('[data-palette="burnished-copper"]');
    await page.locator('[data-idea-tab="已指派"]').click();
    assert.equal(await page.locator('[data-idea-tab="已指派"]').getAttribute('aria-selected'), 'true', 'Idea tab must show selected state');
    assert.match(await page.locator('.idea-copy').textContent(), /Chloe/, 'Tab selection must change its content');
    assert.equal(await page.getByRole('button', { name: '发送想法' }).isDisabled(), true, 'Empty idea input must disable sending');
    await page.getByRole('textbox', { name: '写下一个想法' }).fill('收藏这套配色');
    await page.getByRole('button', { name: '发送想法' }).click();
    assert.match(await page.locator('.idea-copy').textContent(), /收藏这套配色/, 'Submitted prototype idea must be visible');
    assert.equal(await page.getByRole('button', { name: '发送想法' }).isDisabled(), true, 'Sending must reset the input and disabled state');
    const address = page.getByRole('textbox', { name: '搜索或输入网址' });
    await address.fill('12345');
    await address.press('ArrowLeft');
    assert.match(await page.locator('.app').getAttribute('data-palette'), /burnished-copper/, 'Typing palette shortcuts in input must not switch the palette');
    await page.locator('.tool[data-tool="文件"]').click();
    assert.match(await page.locator('.right-panel').textContent(), /navigation\.tsx/, 'File tool must show the prototype file contents');
    await page.locator('[data-back]').click();
    assert.equal(await page.locator('.tool').count(), 6, 'Back must restore all tools');
    await page.locator('#favorite').click();
    await page.reload();
    await page.waitForSelector('[data-palette="burnished-copper"]');
    assert.match(await page.locator('#shortlist').textContent(), /赤铜/, 'Favorites must survive reload');
    await page.locator('h1').click();
    await page.keyboard.press('1');
    await page.waitForSelector('[data-palette="moonstone"]');
    await page.locator('.address').focus();
    await page.locator('[data-idea-tab="已指派"]').hover();
    await page.screenshot({ path: path.join(output, 'moonstone-focus-hover.png'), fullPage: true });
    const hoverTarget = await page.locator('[data-idea-tab="已指派"]').boundingBox();
    await page.mouse.move(hoverTarget.x + hoverTarget.width / 2, hoverTarget.y + hoverTarget.height / 2);
    await page.mouse.down();
    await page.screenshot({ path: path.join(output, 'moonstone-pressed.png'), fullPage: true });
    await page.mouse.up();
    await page.locator('[data-scene="home"]').click();
    await page.locator('#workspace-search').fill('找不到的工作空间');
    assert.equal(await page.locator('.workspace-home-card').isVisible(), false, 'Nonmatching workspace search must hide the card');
    assert.equal(await page.locator('.home-empty').isVisible(), true, 'Empty search result must explain the state');
    await page.locator('#workspace-search').fill('');
    await page.getByRole('button', { name: '＋ 新建' }).click();
    assert.equal(await page.locator('.workspace-home-card').count(), 2, 'New workspace button must create a prototype workspace');
    await page.goto(url + '?v=999');
    await page.waitForSelector('[data-palette="moonstone"]');
    await page.setViewportSize({ width: 390, height: 844 });
    const labelsFit = await page.locator('.proto-picker-item').evaluateAll(items => items.every(item => {
      const range = document.createRange();
      range.selectNodeContents(item);
      return range.getBoundingClientRect().height < 20;
    }));
    assert.equal(labelsFit, true, 'Chinese theme names must remain readable on one line in a narrow drawer');
    await page.screenshot({ path: path.join(output, 'narrow.png'), fullPage: true });
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth);
    assert.equal(overflow, false, 'The prototype page itself must fit a narrow browser drawer');
    await page.locator('#zoom').click();
    await page.waitForSelector('.viewport.fullsize');
    await page.waitForFunction(() => document.querySelector('.app')?.style.getPropertyValue('--scale') === '1');
    assert.equal(await page.locator('.app').evaluate(e => getComputedStyle(e).getPropertyValue('--scale')), '1', '1:1 mode must preserve the app layout instead of hiding drawers');
    assert.deepEqual(errors, [], 'Prototype must not emit browser or JavaScript errors');
    await fs.writeFile(path.join(output, 'check-result.json'), JSON.stringify({ result: 'pass', browser: 'Chromium', candidates: 5, scenes: 2, errors, nativeAppValidation: false }, null, 2));
    console.log('PASS: 5 palettes × 2 scenes, references, tabs, focus, input, tools, favorites, URL, narrow view; 0 browser errors.');
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
