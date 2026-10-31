// Run: node docs/prototypes/tests/norma-desktop-companion.cjs
const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs');
const {chromium} = require('playwright');

/// [桌宠工作窗口]：三个方向都能对话、查看任务并处理幕僚建议。
/// 数据构造：初始 3 个运行任务、2 条建议；批准 1 条后为 4 个任务、1 条建议。
/// 执行过程：逐个方向打开任务、查看细节、发送消息、批准并忽略剩余建议。
/// 预期结果：回执和计数同步；重复点击不会重复派发；全部处理后显示空态。
(async () => {
  const browser = await chromium.launch({headless:true, channel:process.env.BROWSER_CHANNEL || 'chrome'});
  const page = await browser.newPage({viewport:{width:1440,height:960}});
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const target = 'file://' + path.resolve(__dirname, '../norma-desktop-companion/index.html');
  const evidence = path.resolve(__dirname, '../norma-desktop-companion/evidence');
  fs.mkdirSync(evidence, {recursive:true});
  const click = name => page.getByRole('button', {name, exact:true}).click();
  try {
    for (let variant=1; variant<=3; variant++) {
      await page.goto(`${target}?v=${variant}`);
      await page.locator('.companion-window').waitFor();
      assert.equal(await page.locator('.proto-picker-item[data-active]').count(), 1, '只能有一个选中的原型方向');
      assert.equal(await page.locator('.pet-image').first().evaluate(el=>el.complete && el.naturalWidth>0), true, '原创桌宠图片应真实加载');
      await page.screenshot({path:path.join(evidence, `direction-${variant}-chat.png`)});
      if (variant===1) {
        await page.getByRole('button',{name:'运行中 3',exact:true}).focus();
        await page.keyboard.press('Enter');
        assert.equal(await page.evaluate(()=>document.activeElement.getAttribute('aria-label')),'运行中 3','键盘切页后焦点应保留在已选按钮');
        assert.equal(await page.locator('.companion-window').evaluate(el=>el.getAnimations().length),0,'高频键盘操作不能重播窗口入场动画');
      } else await click('运行中 3');
      await page.locator('[data-task-id="t1"]').click();
      assert.match(await page.locator('.task-detail').innerText(), /cargo test/, '任务详情应显示具体执行记录');
      await click('返回任务');
      await click('对话');
      await page.getByRole('textbox', {name:'给 Norma 发消息'}).fill('先帮我总结目前的任务');
      await click('发送消息');
      await page.locator('.message.assistant').last().filter({hasText:'3 个任务'}).waitFor();
      assert.match(await page.locator('.messages').innerText(), /先帮我总结目前的任务/, '发送后必须保留用户原文');
      await click('待审批 2');
      await page.screenshot({path:path.join(evidence, `direction-${variant}-approvals.png`)});
      await click('查看完整指令 p1');
      assert.match(await page.locator('[data-proposal-id="p1"]').innerText(), /cargo test -p runtime/, '批准前应能查看完整派发指令');
      const approve = page.getByRole('button', {name:'批准并派发 p1', exact:true});
      await approve.dblclick({delay:30});
      await page.getByRole('button', {name:'待审批 1', exact:true}).waitFor();
      assert.equal(await page.locator('[data-proposal-id="p1"]').count(), 0, '成功派发才移除建议');
      await click('运行中 4');
      assert.equal(await page.locator('[data-task-id]').count(), 4, '快速重复点击必须只创建一个任务');
      await click('待审批 1');
      await click('忽略建议 p2');
      assert.match(await page.locator('.window-content').innerText(), /都处理好了/, '最后一条建议处理完应显示明确空态');
      await click('收起窗口');
      assert.equal(await page.locator('.companion-window').count(), 0, '收起后应保留桌宠且隐藏窗口');
      await click('打开 Norma 窗口');
      assert.equal(await page.getByRole('button', {name:'运行中 4',exact:true}).count(), 1, '重开窗口不得重置任务');
    }

    /// [派发失败]：失败保留建议与原目标，重试成功后才新增任务。
    /// 数据构造：失败场景 2 条建议、3 个任务；第一次失败不扣减，第二次成功扣减为 1。
    /// 执行过程：切换失败场景、批准、核验错误、重试、切换 Master。
    /// 预期结果：错误可见、无虚假成功，Master 切换不改变幕僚建议的派发目标。
    await page.goto(target);
    await click('派发失败');
    await click('待审批 2');
    await click('批准并派发 p1');
    await page.locator('.dispatch-error').waitFor();
    assert.equal(await page.locator('[data-proposal-id="p1"]').count(), 1, '派发失败不能吞掉建议');
    await click('重试派发 p1');
    await page.getByRole('button', {name:'待审批 1',exact:true}).waitFor();
    await click('切换 Master Agent');
    await click('选择 Simon');
    assert.match(await page.locator('.master-identity').innerText(), /Simon/, '窗口应显示所选 Master Agent');
    await click('运行中 4');
    assert.match(await page.locator('[data-task-id="task-p1"]').innerText(), /Chloe/, '更换 Master 不能偷换建议的派发目标');

    /// [双语与减少动态效果]：切语言不重置数据，减少动态效果禁用位移动画。
    /// 数据构造：批准后的 4 个任务；英文界面仍应展示 Running 4。
    /// 执行过程：切英文、检查标题和任务、切回中文，再模拟减少动态效果。
    /// 预期结果：界面翻译且用户内容不翻译；无脚本异常。
    await click('切换语言');
    assert.equal(await page.getByRole('button', {name:'Running 4',exact:true}).count(), 1, '已有任务计数必须随语言切换');
    assert.equal(await page.getByRole('button', {name:'Approvals 1',exact:true}).count(), 1, '审批入口必须随语言切换');
    await click('Switch language');
    await page.emulateMedia({reducedMotion:'reduce'});
    assert.equal(await page.locator('.companion-window').evaluate(el=>getComputedStyle(el).animationName), 'none', '减少动态效果时不得播放窗口位移动画');

    /// [顶边安全区和桌宠浮层]：小桌面收起按钮可点击；桌宠拖到顶边后菜单向下展开。
    /// 数据构造：1280×800 桌面，顶部工具条占用 83px，工作窗口预留至少 96px。
    /// 执行过程：逐方向核验按钮中心命中；拖桌宠到左上角；检查气泡与菜单边界。
    /// 预期结果：按钮不被覆盖，气泡与菜单不越出可视区域；拖拽不误触开窗。
    await page.setViewportSize({width:1280,height:800});
    for (let variant=1; variant<=3; variant++) {
      await page.goto(`${target}?v=${variant}`);
      const close=page.getByRole('button',{name:'收起窗口',exact:true});
      await close.waitFor();
      const hit=await close.evaluate(el=>{const r=el.getBoundingClientRect();return el.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2));});
      assert.equal(hit,true,`方向 ${variant} 的收起按钮必须可真实点击`);
      assert.ok((await page.locator('.companion-window').boundingBox()).y>=96,`方向 ${variant} 必须留出顶部工具条安全区`);
      await close.click();
    }
    await page.goto(target);
    await click('收起窗口');
    const pet=await page.locator('.pet-button').boundingBox();
    await page.mouse.move(pet.x+pet.width/2,pet.y+pet.height/2);
    await page.mouse.down();
    await page.mouse.move(15,42,{steps:12});
    await page.mouse.up();
    assert.equal(await page.locator('.companion-window').count(),0,'拖动不能被当成单击打开窗口');
    const speech=await page.locator('.pet-speech').boundingBox();
    assert.ok(speech.y>=36 && speech.x>=0 && speech.y+speech.height<=800,'顶边气泡必须翻到可视范围内');
    await page.locator('.pet-button').click({button:'right'});
    const menu=await page.locator('.pet-menu').boundingBox();
    assert.ok(menu.y>=36 && menu.y+menu.height<=800,'桌宠菜单不能越出屏幕顶边');

    /// [窄窗口与不同失败目标]：减少动态效果不破坏定位，错误文案必须说明实际目标。
    /// 数据构造：390×844 窗口、第二条建议目标 Morgan，2 条建议均保留直到成功。
    /// 执行过程：桌面触发 Morgan 失败，再切窄窗口的第二方向并截图。
    /// 预期结果：错误不能错误归因 Chloe；窗口完整可见，页面无脚本异常。
    await page.goto(target);
    await click('派发失败');
    await click('待审批 2');
    await click('批准并派发 p2');
    await page.locator('.dispatch-error').waitFor();
    assert.match(await page.locator('.dispatch-error').innerText(),/Morgan/,'失败原因必须使用当前建议的真实目标');
    await click('切换深浅主题');
    await page.locator('.dispatch-error').scrollIntoViewIfNeeded();
    await page.screenshot({path:path.join(evidence,'dark-dispatch-failure.png')});
    await page.goto(target);
    await click('收起窗口');
    await page.screenshot({path:path.join(evidence,'pet-only.png')});
    await page.setViewportSize({width:390,height:844});
    await page.goto(`${target}?v=2`);
    const bounds=await page.locator('.companion-window').boundingBox();
    assert.ok(bounds.x>=0 && bounds.x+bounds.width<=390,'减少动态效果下，窄窗口仍须完整可见');
    const pickerHeight=await page.locator('.proto-picker').evaluate(el=>el.getBoundingClientRect().height);
    assert.equal(pickerHeight,36,'窄窗口选择器应保持单行，不能把中文挤成竖排');
    await page.screenshot({path:path.join(evidence,'narrow-capsule.png')});
    assert.deepEqual(errors, [], '三个原型与关键交互不得产生浏览器脚本异常');
    console.log('PASS: 3 directions; chat, tasks, approvals, duplicate guard, retry, Master, bilingual, keyboard focus, reduced motion, drag bounds, narrow windows.');
  } finally {
    await browser.close();
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
