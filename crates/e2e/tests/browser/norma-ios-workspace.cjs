// Run: node crates/e2e/tests/browser/norma-ios-workspace.cjs
const assert = require('node:assert/strict');
const path = require('node:path');
const {chromium} = require('../../../../spikes/norma-spatial-prototype/node_modules/playwright');

/// [九屏主流程]：参考图 W01–W09 都能从可见控件抵达，且点击后显示对应设计图。
/// 数据构造（含关键数值的推导过程）：9 张参考图 = W01 列表 + W02 设备 + W03 项目 + W04 对话 + W05 简报 + W06 任务 + W07 通话 + W08 收件箱 + W09 成果。
/// 执行过程：1. 从列表进入设备和对话；2. 从对话进入项目、简报、任务和通话；3. 从收件箱进入成果。
/// 预期结果：每次转场的图像文件名与参考图一致；返回后仍处于原 Workspace；页面无脚本异常。
(async () => {
  const browser = await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL||undefined});
  const page = await browser.newPage({viewport:{width:1280,height:900}});
  const errors=[];
  page.on('pageerror',error=>errors.push(error.message));
  const pagePath=path.resolve(__dirname,'../../../../docs/prototypes/archive/2026-09-28-norma-ios-workspace/norma-ios-workspace.html');
  try {
    await page.goto('file://'+pagePath);
    const image=page.locator('#reference-screen');
    const expectScreen=async (file,label)=>{
      assert.ok((await image.getAttribute('src')).endsWith(file),`${label} 应显示 ${file}`);
      const dimensions=await image.evaluate(async element=>{await element.decode();return [element.naturalWidth,element.naturalHeight];});
      assert.deepEqual(dimensions,[1024,1536],`${label} 的参考图必须真实加载且保持原始比例`);
    };
    const click=async label=>page.getByRole('button',{name:label,exact:true}).click();
    await expectScreen('W01-workspace-list.png','首页');
    /// [参考图热点反馈]：鼠标经过图片热点不覆盖原型画面，键盘导航仍显示焦点。
    /// 数据构造（含关键数值的推导过程）：首页 1 个“已连接电脑”热点；透明背景和无阴影各检查 1 项，键盘焦点检查 1 项。
    /// 执行过程：1. 鼠标移入热点并读取样式；2. 按 Tab 聚焦第一个热点；3. 继续点击进入设备页。
    /// 预期结果：鼠标悬停时没有蓝色覆盖或描边；键盘焦点仍可见；热点仍可点击。
    const deviceSpot=page.getByRole('button',{name:'已连接电脑',exact:true});
    await deviceSpot.hover();
    const hovered=await deviceSpot.evaluate(element=>({background:getComputedStyle(element).backgroundColor,shadow:getComputedStyle(element).boxShadow}));
    assert.equal(hovered.background,'rgba(0, 0, 0, 0)','鼠标悬停不应在参考图上叠加选中底色');
    assert.equal(hovered.shadow,'none','鼠标悬停不应在参考图上绘制选中描边');
    await page.keyboard.press('Tab');
    assert.equal(await deviceSpot.evaluate(element=>getComputedStyle(element).outlineStyle),'solid','键盘导航仍应显示可见焦点');
    await click('已连接电脑');
    await expectScreen('W02-connected-computers.png','设备页');
    await click('返回工作空间列表');
    await click('打开改进 iOS 首屏交接');
    await expectScreen('W04-workspace-chat.png','对话页');
    await click('工作空间设置');
    await click('添加已有项目');
    await expectScreen('W03-project-picker.png','项目选择层');
    await click('完成项目选择');
    assert.match(await page.getByRole('dialog',{name:'工作空间设置'}).innerText(),/关联项目.*iOS App · Norma API/s,'项目选择完成后应回到当前工作空间设置');
    await click('完成');
    await click('打开工作简报');
    await expectScreen('W05-work-brief.png','工作简报');
    await click('返回工作空间对话');
    await click('查看 Codex 任务');
    await expectScreen('W06-agent-task.png','Agent 任务');
    await click('返回工作空间对话');
    /// [频道可见回执]：发送给 Codex 的消息应进入同一频道，并显示具名回应。
    /// 数据构造（含关键数值的推导过程）：发送 1 条 @Codex 消息，模型追加用户消息 1 条及 Codex 回应 1 条。
    /// 执行过程：打开输入 Sheet → 发送消息 → 回到原频道查看最新对话。
    /// 预期结果：用户原文和 Codex 回应同时可见，且不改变 Workspace。
    await click('发送消息或 @Agent');
    await page.getByRole('textbox',{name:'消息内容'}).fill('@Codex 继续核对 API 状态');
    await click('发送消息');
    assert.match(await page.locator('.chat-updated').innerText(),/@Codex 继续核对 API 状态/,'发送的指令必须显示在频道里');
    assert.match(await page.locator('.chat-updated').innerText(),/Codex.*收到/s,'被提及的 Agent 必须在频道里具名回应');
    await click('与 Norma 通话');
    await expectScreen('W07-norma-call.png','实时通话');
    await click('说一句话给 Norma');
    assert.match(await page.getByRole('status').innerText(),/语音指令已转写并送达原任务/,'通话指令应有送达回执');
    await click('输入文字');
    await page.getByRole('textbox',{name:'消息内容'}).fill('@Norma 请继续跟进');
    await click('发送消息');
    assert.match(await page.locator('.voice-caption').innerText(),/收到.*继续协调/,'通话中的文字消息应得到可见回应');
    await click('结束对话');
    await expectScreen('W04-workspace-chat.png','挂断后');
    assert.match(await page.locator('.chat-updated').innerText(),/@Codex 再核对一下 API 状态/,'语音转写应留在原工作空间');
    assert.match(await page.locator('.chat-updated').innerText(),/Codex.*原任务继续/s,'挂断后应保留 Agent 在原任务的回应');
    await click('返回工作空间列表');
    assert.equal(await page.getByRole('status').innerText(),'','换页后不应残留上一步的通话提示');
    await click('前往收件箱');
    await expectScreen('W08-inbox.png','收件箱');
    await click('查看成果与证据');
    await expectScreen('W09-acceptance.png','成果验收');
    assert.equal(await page.getByRole('status').innerText(),'','成果页不应被前一页提示遮挡');
    assert.deepEqual(errors,[],'九屏流程不应产生页面脚本异常');

    /// [创建与纠偏]：保存想法不派工；项目、简报和语音操作都有可见反馈。
    /// 数据构造（含关键数值的推导过程）：新建 1 个 Workspace，初始任务数 0；原示例 Workspace 有 2 个关联项目和 1 条硬约束。
    /// 执行过程：1. 返回列表并保存想法；2. 检查尚未执行；3. 编辑工作简报；4. 在通话中切换静音。
    /// 预期结果：新 Workspace 不出现 Agent 任务；更新和静音有明确状态，原频道的项目不丢失。
    await click('返回收件箱');
    /// [收件箱搜索]：搜索只过滤待办，清除后恢复全部事项。
    /// 数据构造（含关键数值的推导过程）：当前有 2 件待办；关键词“首屏交接”只命中 1 件成果。
    /// 执行过程：打开收件箱搜索 → 输入关键词 → 查看结果 → 清除搜索。
    /// 预期结果：结果中有成果、没有部署决定；清除后两件待办都恢复。
    await click('搜索收件箱');
    await page.getByRole('textbox',{name:'事项或工作空间'}).fill('首屏交接');
    await click('搜索待办');
    assert.equal(await page.locator('.inbox-updated .inbox-card[data-action="acceptance"]').count(),1,'关键词应命中首屏交接成果');
    assert.equal(await page.locator('.inbox-updated .inbox-card[data-action="decision"]').count(),0,'搜索成果时不应显示部署决定');
    await click('清除搜索');
    assert.equal(await page.locator('.inbox-updated .inbox-card').count(),2,'清除搜索后应恢复两件待办');
    await click('前往工作空间列表');
    await click('保存新想法');
    /// [iOS Sheet 手势]：从顶部拖拽条下滑应关闭 Sheet，并可再次打开。
    /// 数据构造（含关键数值的推导过程）：关闭阈值为 70 CSS px；拖动 110 CSS px，超过阈值 40 px。
    /// 执行过程：按住创建 Sheet 拖拽条 → 下移 110 px → 松手 → 重新点击创建入口。
    /// 预期结果：Sheet 关闭且列表仍可操作；重新打开后草稿输入可用。
    const handle=await page.locator('.sheet-handle').boundingBox();
    assert.ok(handle,'创建 Sheet 应有可拖拽的顶部把手');
    await page.mouse.move(handle.x+handle.width/2,handle.y+handle.height/2);
    await page.mouse.down();
    await page.mouse.move(handle.x+handle.width/2,handle.y+handle.height/2+110,{steps:4});
    await page.mouse.up();
    assert.equal(await page.getByRole('dialog',{name:'保存新想法'}).count(),0,'下滑超过阈值后创建 Sheet 应关闭');
    await click('保存新想法');
    await page.getByRole('textbox',{name:'新的想法'}).fill('让首次连接更安心');
    await click('保存为工作空间');
    assert.match(await page.getByRole('status').innerText(),/尚未执行/,'保存想法后应明确显示尚未执行');
    assert.equal(await page.getByRole('button',{name:'查看 Codex 任务',exact:true}).count(),0,'新想法不能立即拥有 Codex 任务');
    await click('工作空间设置');
    await click('添加已有项目');
    await page.locator('.project-dynamic button[data-project="iOS App"]').click();
    await click('完成项目选择');
    assert.match(await page.getByRole('dialog',{name:'工作空间设置'}).innerText(),/关联项目.*iOS App/s,'新工作空间关联的项目应在设置里可见');
    await click('完成');
    await click('发消息或 @Agent');
    await page.getByRole('textbox',{name:'消息内容'}).fill('@Norma 先讨论首次连接');
    await click('发送消息');
    assert.match(await page.locator('.new-thread').innerText(),/@Norma 先讨论首次连接/,'新工作空间应显示刚发送的消息');
    await click('返回工作空间列表');
    await click('构思中');
    assert.equal(await page.locator('.list-results .result-card[data-id="ios-handoff"]').count(),0,'构思中筛选不应显示执行中的 Workspace');
    assert.equal(await page.locator('.list-results .result-card[data-id="first-connect"]').count(),1,'构思中筛选应保留尚未执行的 Workspace');
    await click('全部');
    await click('打开工作空间：改进 iOS 首屏交接');
    await click('打开工作简报');
    await click('纠正目标与限制');
    await page.getByRole('textbox',{name:'目标'}).fill('先让交接状态清楚');
    await page.getByRole('textbox',{name:'硬约束，每行一条'}).fill('禁止发布生产环境\n不得删除用户数据');
    await click('保存工作简报');
    assert.match(await page.getByRole('status').innerText(),/工作简报已更新/,'工作简报更新后应有回执');
    await click('返回工作空间对话');
    await click('与 Norma 通话');
    await click('静音');
    assert.match(await page.getByRole('status').innerText(),/已静音/,'静音后应立即显示状态');
    await click('说一句话给 Norma');
    assert.match(await page.getByRole('status').innerText(),/麦克风已静音/,'静音时不能发送语音指令');
    /// [通话切换工作空间]：切换后通话继续绑定新工作空间，而非退回聊天。
    /// 数据构造（含关键数值的推导过程）：已有 1 通绑定首屏交接的通话；选择 1 个构思中的已有工作空间。
    /// 执行过程：点顶部工作空间 → 选择“新人首次连接体验” → 查看通话页标题。
    /// 预期结果：仍在通话页且顶部显示新工作空间，静音状态重置。
    await click('切换当前工作空间');
    await click('新人首次连接体验');
    await expectScreen('W07-norma-call.png','切换工作空间后的通话');
    assert.match(await page.locator('.call-workspace').innerText(),/新人首次连接体验/,'切换通话后应显示新的工作空间');
    assert.equal(await page.getByRole('button',{name:'静音',exact:true}).count(),1,'新通话应恢复非静音状态');
    assert.deepEqual(errors,[],'创建和纠偏流程不应产生页面脚本异常');

    /// [手机视口与验收]：在真实 iPhone 宽度下，设备配对和成果验收仍可直接触达。
    /// 数据构造（含关键数值的推导过程）：视口 390×844 CSS px；配对后电脑由 2 台增至 3 台；待验收 1 件成果，验收后为 0 件。
    /// 执行过程：1. 在手机视口完成扫码模拟；2. 进入 Workspace 和收件箱；3. 打开成果并验收；4. 返回收件箱检查。
    /// 预期结果：所有点击命中可见控件；新增电脑不虚构项目；验收后待验收卡片不再出现。
    const mobile=await browser.newPage({viewport:{width:390,height:844},deviceScaleFactor:1,hasTouch:true});
    mobile.on('pageerror',error=>errors.push(error.message));
    await mobile.goto('file://'+pagePath);
    const filterTarget=await mobile.getByRole('button',{name:'显示已归档',exact:true}).boundingBox();
    assert.ok(filterTarget.width>=44&&filterTarget.height>=44,'手机端状态筛选触控目标应至少为 44×44 CSS px');
    await mobile.getByRole('button',{name:'已连接电脑',exact:true}).click();
    await mobile.getByRole('button',{name:'扫码添加电脑',exact:true}).click();
    await mobile.getByRole('button',{name:'确认注册 Studio Mini',exact:true}).click();
    assert.match(await mobile.getByRole('status').innerText(),/Studio Mini 已注册/,'手机端配对必须有明确回执');
    assert.match(await mobile.locator('.device-new').innerText(),/0 个项目/,'新电脑不能虚构项目');
    await mobile.getByRole('button',{name:'返回工作空间列表',exact:true}).click();
    await mobile.getByRole('button',{name:'打开改进 iOS 首屏交接',exact:true}).click();
    await mobile.getByRole('button',{name:'返回工作空间列表',exact:true}).click();
    await mobile.getByRole('button',{name:'前往收件箱',exact:true}).click();
    await mobile.getByRole('button',{name:'查看成果与证据',exact:true}).click();
    await mobile.getByRole('button',{name:'验收通过',exact:true}).click();
    assert.match(await mobile.getByRole('status').innerText(),/已验收/,'验收通过应有可见反馈');
    /// [归档与重开]：已验收的工作空间可由用户归档并原位重开。
    /// 数据构造（含关键数值的推导过程）：已验收的工作空间 1 个；归档后为 1 个归档项；重开后归档项为 0 个。
    /// 执行过程：打开设置 → 归档 → 从归档筛选重新进入 → 重开。
    /// 预期结果：归档状态和重开状态均有明确文字；原频道消息仍存在。
    await mobile.getByRole('button',{name:'工作空间设置',exact:true}).click();
    await mobile.getByRole('button',{name:'归档工作空间',exact:true}).click();
    assert.match(await mobile.locator('.chat-status').innerText(),/已归档/,'验收后归档应更新工作空间状态');
    await mobile.getByRole('button',{name:'返回工作空间列表',exact:true}).click();
    await mobile.getByRole('button',{name:'已归档',exact:true}).click();
    assert.equal(await mobile.locator('.list-results .result-card[data-id="ios-handoff"]').count(),1,'归档筛选应能找回原工作空间');
    await mobile.getByRole('button',{name:'打开工作空间：改进 iOS 首屏交接',exact:true}).click();
    await mobile.getByRole('button',{name:'工作空间设置',exact:true}).click();
    await mobile.getByRole('button',{name:'重开工作空间',exact:true}).click();
    assert.match(await mobile.locator('.chat-status').innerText(),/待继续/,'重开后应进入待继续状态');
    assert.match(await mobile.locator('.chat-updated').innerText(),/此前的决定、消息和成果仍在这里/,'重开后应保留原频道记录');
    await mobile.getByRole('button',{name:'返回工作空间列表',exact:true}).click();
    await mobile.getByRole('button',{name:'前往收件箱',exact:true}).click();
    assert.equal(await mobile.locator('.inbox-updated .inbox-card[data-action="acceptance"]').count(),0,'验收后收件箱不应保留同一成果待办');
    assert.deepEqual(errors,[],'手机视口流程不应产生页面脚本异常');
    await mobile.close();
    /// [短屏 iPhone 可用性]：375×667 视口下，首页主要操作和底部导航应完整可见且能够点击。
    /// 数据构造（含关键数值的推导过程）：参考图 1024×1536，视口 375×667；原先按 151.48vw 放大时图片高约 852 px，底部导航起点约 662 px，已接近视口底部。
    /// 执行过程：1. 打开首页并检查筛选、创建和收件箱控件；2. 点击收件箱；3. 确认对应参考图已显示。
    /// 预期结果：三个控件均在视口内、触控盒至少 44×44 CSS px；点击收件箱后进入 W08，页面没有脚本错误。
    const compact=await browser.newPage({viewport:{width:375,height:667},deviceScaleFactor:1,hasTouch:true});
    compact.on('pageerror',error=>errors.push(error.message));
    await compact.goto('file://'+pagePath);
    for(const name of ['显示已归档','保存新想法','前往收件箱']){
      const box=await compact.getByRole('button',{name,exact:true}).boundingBox();
      assert.ok(box,`${name} 应有可触控区域`);
      assert.ok(box.width>=44&&box.height>=44,`${name} 的触控区域应至少为 44×44 CSS px`);
      assert.ok(box.x>=0&&box.y>=0&&box.x+box.width<=375&&box.y+box.height<=667,`${name} 应完整留在短屏视口内`);
    }
    await compact.getByRole('button',{name:'前往收件箱',exact:true}).click();
    assert.ok((await compact.locator('#reference-screen').getAttribute('src')).endsWith('W08-inbox.png'),'短屏点击底部导航应进入收件箱');
    assert.deepEqual(errors,[],'短屏 iPhone 流程不应产生页面脚本异常');
    await compact.close();
    /// [减少动态效果]：用户要求减少动态效果时，页面淡入和 Sheet 位移动画都应停用。
    /// 数据构造（含关键数值的推导过程）：系统偏好 reduce；页面动画 1 个、Sheet 动画 1 个，预期启用数为 0。
    /// 执行过程：以 reduce 偏好打开原型 → 进入 Workspace → 打开设置 Sheet。
    /// 预期结果：图片与 Sheet 的 animation-name 均为 none，入口仍可操作。
    const reduced=await browser.newPage({viewport:{width:390,height:844},reducedMotion:'reduce'});
    reduced.on('pageerror',error=>errors.push(error.message));
    await reduced.goto('file://'+pagePath);
    await reduced.getByRole('button',{name:'打开改进 iOS 首屏交接',exact:true}).click();
    assert.equal(await reduced.locator('#reference-screen').evaluate(element=>getComputedStyle(element).animationName),'none','减少动态效果时页面不应播放淡入动画');
    await reduced.getByRole('button',{name:'工作空间设置',exact:true}).click();
    assert.equal(await reduced.locator('.sheet').evaluate(element=>getComputedStyle(element).animationName),'none','减少动态效果时 Sheet 不应播放位移动画');
    assert.deepEqual(errors,[],'减少动态效果流程不应产生页面脚本异常');
    await reduced.close();
    console.log('PASS · W01–W09 九屏、创建、项目、消息、通话、验收及归档重开');
  } finally {
    await browser.close();
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
