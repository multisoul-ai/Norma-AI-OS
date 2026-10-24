// Run: BROWSER_CHANNEL=chrome node docs/prototypes/tests/norma-ios-ideas-workspaces.cjs
const assert = require('node:assert/strict');
const path = require('node:path');
const {chromium} = require('playwright');

/// [想法与工作空间分离]：文字和语音想法只进入想法库，工作空间数量与配置保持独立。
/// 数据构造（含关键数值的推导过程）：初始 2 条想法、2 个工作空间；新增文字 1 条与语音转写 1 条后，应有 4 条想法、仍只有 2 个工作空间。
/// 执行过程：1. 打开新版 P01；2. 保存文字想法；3. 经语音页编辑转写并保存；4. 从抽屉进入工作空间列表。
/// 预期结果：两条新想法均可见；工作空间没有被自动创建；P01、P02、P03 使用对应新参考图。
(async () => {
  const browser = await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL||undefined});
  const page = await browser.newPage({viewport:{width:390,height:844},hasTouch:true});
  const errors=[];
  page.on('pageerror',error=>errors.push(error.message));
  const pagePath=path.resolve(__dirname,'../norma-ios-workspace.html');
  try {
    await page.goto('file://'+pagePath);
    const screen=()=>page.locator('#reference-screen').getAttribute('src');
    assert.ok((await screen()).endsWith('P01-idea-library.png'),'初始页面应使用新版想法库图片');
    await page.getByRole('button',{name:'记录想法',exact:true}).click();
    await page.getByRole('textbox',{name:'想法内容'}).fill('让跨端状态更容易理解');
    await page.getByRole('button',{name:'保存到想法库',exact:true}).click();
    assert.match(await page.getByRole('status').innerText(),/已记录/,'文字保存应明确反馈已记录');
    assert.match(await page.getByLabel('想法列表').innerText(),/让跨端状态更容易理解/,'新文字想法应出现在想法库');
    await page.getByRole('button',{name:'记录想法',exact:true}).click();
    await page.getByRole('button',{name:'语音输入',exact:true}).click();
    assert.ok((await screen()).endsWith('P02-voice-idea.png'),'语音入口应使用新版语音记录图片');
    await page.getByRole('textbox',{name:'转写文字'}).fill('让首次连接提示更清楚');
    await page.getByRole('button',{name:'保存语音想法',exact:true}).click();
    assert.ok((await screen()).endsWith('P01-idea-library.png'),'保存语音想法后应返回想法库');
    assert.match(await page.getByLabel('想法列表').innerText(),/让首次连接提示更清楚/,'编辑后的语音转写应被保存');
    await page.getByRole('button',{name:'打开更多模块',exact:true}).click();
    assert.ok((await screen()).endsWith('P03-workspaces-drawer.png'),'更多模块应使用新版工作空间抽屉图片');
    assert.equal(await page.getByRole('button',{name:'打开移动端体验设置',exact:true}).count(),1,'原有移动端体验工作空间应仍可进入');
    assert.equal(await page.getByRole('button',{name:'打开发布质量设置',exact:true}).count(),1,'原有发布质量工作空间应仍可进入');
    assert.equal(await page.getByLabel('工作空间列表').locator('[data-workspace-id]').count(),2,'记录两条想法后工作空间数仍应为 2');

    /// [各工作空间独立配置]：在移动端体验添加项目和 Agent，不改变发布质量的配置。
    /// 数据构造（含关键数值的推导过程）：移动端初始 2 项目、3 位 Agent；发布质量初始 1 项目、2 位 Agent；新增后移动端分别为 3 和 4。
    /// 执行过程：打开移动端设置 → 添加 Web Portal 与 Research → 返回列表 → 打开发布质量设置核对。
    /// 预期结果：移动端新增两项可见；发布质量不出现 Web Portal 或 Research；设置页使用 P04 参考图。
    await page.getByRole('button',{name:'打开移动端体验设置',exact:true}).click();
    assert.ok((await screen()).endsWith('P04-workspace-settings.png'),'工作空间设置应使用新版参考图');
    await page.getByRole('button',{name:'添加已有项目',exact:true}).click();
    await page.getByRole('button',{name:'关联 Web Portal',exact:true}).click();
    assert.match(await page.getByLabel('工作空间配置').innerText(),/Web Portal/,'移动端体验应显示新增项目');
    await page.getByRole('button',{name:'添加 Agent',exact:true}).click();
    await page.getByRole('button',{name:'加入 Research',exact:true}).click();
    assert.match(await page.getByLabel('工作空间配置').innerText(),/Research/,'移动端体验应显示新增 Agent');
    await page.getByRole('button',{name:'返回工作空间列表',exact:true}).click();
    await page.getByRole('button',{name:'打开发布质量设置',exact:true}).click();
    const release=await page.getByLabel('工作空间配置').innerText();
    assert.match(release,/Release Ops/,'发布质量应保留自己的项目');
    assert.doesNotMatch(release,/Web Portal|Research/,'移动端体验的配置不应串到发布质量');
    await page.getByRole('button',{name:'返回工作空间列表',exact:true}).click();

    /// [工作线程与来源想法]：具体任务留在工作线程，来源想法只作为可追溯引用。
    /// 数据构造（含关键数值的推导过程）：移动端 1 条首屏交接线程引用 1 条语音想法；发送 1 条 @Codex 消息后追加 1 条用户消息和 1 条 Agent 回执。
    /// 执行过程：从工作空间卡进入线程 → 查看来源想法 → 发出 @Codex 指令 → 核对线程回执。
    /// 预期结果：线程显示 P05 图；来源想法仍可查看；Codex 具名回应；想法库不新增工作空间。
    await page.getByRole('button',{name:'打开改进首屏交接',exact:true}).click();
    assert.ok((await screen()).endsWith('P05-work-thread.png'),'线程应使用新版工作线程图片');
    await page.getByRole('button',{name:'查看来源想法',exact:true}).click();
    assert.match(await page.getByRole('dialog',{name:'来源想法'}).innerText(),/从 Mac 切到 iPhone 时，首屏信息太多/,'线程应能回看原始想法');
    await page.getByRole('button',{name:'完成',exact:true}).click();
    await page.getByRole('button',{name:'发送消息或 @Agent',exact:true}).click();
    await page.getByRole('textbox',{name:'消息内容'}).fill('@Codex 请继续核对状态');
    await page.getByRole('button',{name:'发送消息',exact:true}).click();
    assert.match(await page.getByLabel('最新对话').innerText(),/@Codex 请继续核对状态/,'用户新指令应进入当前线程');
    assert.match(await page.getByLabel('最新对话').innerText(),/Codex.*收到/s,'Codex 应在当前线程具名回应');

    /// [从想法建立独立工作线程]：新工作空间先配置资源，之后再引用一条已有想法。
    /// 数据构造：原有 2 个工作空间 + 新建 1 个 = 3 个；新建空间初始 0 项目、仅 Norma，随后关联 iOS App 和 1 条文字想法。
    /// 执行过程：返回列表 → 新建网站体验 → 关联项目 → 从想法库选择文字想法并放入网站体验。
    /// 预期结果：新空间只显示自己的项目；工作线程标记待推进并可回看来源；原想法仍在想法库。
    await page.getByRole('button',{name:'返回工作空间列表',exact:true}).click();
    await page.getByRole('button',{name:'新建工作空间',exact:true}).click();
    await page.getByRole('textbox',{name:'工作空间名称'}).fill('网站体验');
    await page.getByRole('button',{name:'创建工作空间',exact:true}).click();
    const fresh=await page.getByLabel('工作空间配置').innerText();
    assert.match(fresh,/网站体验|Norma/,'新工作空间应有独立名称和默认负责人');
    assert.doesNotMatch(fresh,/iOS App|Norma API|Web Portal/,'新工作空间不应继承旧空间项目');
    await page.getByRole('button',{name:'添加已有项目',exact:true}).click();
    await page.getByRole('button',{name:'关联 iOS App',exact:true}).click();
    assert.match(await page.getByLabel('工作空间配置').innerText(),/iOS App/,'新空间应可关联已有项目');
    await page.getByRole('button',{name:'返回工作空间列表',exact:true}).click();
    assert.equal(await page.getByLabel('工作空间列表').locator('[data-workspace-id]').count(),3,'手动新建后应恰有 3 个工作空间');
    await page.getByRole('button',{name:'打开想法库',exact:true}).click();
    await page.getByLabel('想法列表').getByRole('button').filter({hasText:'让跨端状态更容易理解'}).click();
    await page.getByRole('button',{name:'在工作空间使用',exact:true}).click();
    await page.getByRole('button',{name:'网站体验',exact:true}).click();
    assert.match(await page.locator('.dynamic-thread').innerText(),/待推进|让跨端状态更容易理解/,'新线程应引用想法且尚未执行');
    await page.getByRole('button',{name:'返回工作空间列表',exact:true}).click();
    await page.getByRole('button',{name:'打开想法库',exact:true}).click();
    assert.match(await page.getByLabel('想法列表').innerText(),/让跨端状态更容易理解/,'建立线程后原想法仍应保留');
    assert.deepEqual(errors,[],'新版原型流程不应产生脚本错误');
    console.log('PASS · 想法独立、语音转写、多个工作空间配置与工作线程');
  } finally {
    await browser.close();
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
