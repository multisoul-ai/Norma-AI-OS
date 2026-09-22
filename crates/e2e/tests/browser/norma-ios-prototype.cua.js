// CUA browser E2E for the archived v28 workbench HTML; no native iOS / GPUI claim.
// Current interactive prototype is docs/prototypes/norma-realtime.html.
// Start: python3 -m http.server 8766 --bind 127.0.0.1 --directory docs
// Import this module in the CUA JavaScript session, then run each exported
// test with the existing prototypeTab binding at an iPhone viewport (390 x 844).
// Uses only the documented CUA Tab API; no eval or generated code is needed.
const mobileButton = (tab, name) => tab.playwright.getByRole('button', {name, exact:true});
const mobileAssert = (condition, message) => { if (!condition) throw new Error(message); };

export {testNormaMobileEmerald,testNormaMobileCardManagement,testNormaMobilePairing,testNormaMobileChat,testNormaMobileDetailLayout,testNormaMobileProjectCards,testNormaMobileTasks,testNormaMobileConnection,testNormaMobileNavigation,testNormaMobileDecisions,testNormaMobileNextSteps};

/// 场景：全局黑色样式与成果验收提供完整可操作闭环。
/// 数据构造：初始 workspace 为唯一已完成成果；voice 仍运行；一条间距反馈重新进入待接收。
/// 执行过程：指挥台 → 验收入口 → 对比前后 → 验收 → 继续调整 → 对话 → 运行中成果。
/// 预期结果：导航严格居中；成果比较有实际变化；验收记录只生效一次；反馈回到原任务；运行中不能验收。
async function testNormaMobileEmerald(tab) {
  await tab.goto('http://127.0.0.1:8766/prototypes/archive/2026-09-06-norma-ios/norma-ios.html?check=emerald-'+Date.now()+'#overview');
  const layout=await tab.playwright.evaluate(()=>{
    const nav=document.querySelector('#navigation').getBoundingClientRect();
    const command=document.querySelector('#navigation [data-page="overview"]').getBoundingClientRect();
    return {center:Math.abs(command.x+command.width/2-nav.x-nav.width/2),bg:getComputedStyle(document.querySelector('.phone')).backgroundColor,overflow:document.documentElement.scrollWidth>innerWidth};
  });
  mobileAssert(layout.bg==='rgb(0, 0, 0)','深色页面必须显示纯黑背景，不能残留墨绿色底色');
  mobileAssert(layout.center<=1&&!layout.overflow,'底部指挥台应在 1px 舍入容差内居中且页面无横向溢出');
  await mobileButton(tab,'查看待验收').click();
  await tab.playwright.locator('[data-review-task="workspace"]').click();
  mobileAssert(await tab.playwright.getByRole('tab',{name:'修改后',exact:true}).getAttribute('aria-selected')==='true','默认应展示修改后成果');
  await tab.playwright.getByRole('tab',{name:'修改前',exact:true}).click();
  mobileAssert(await tab.playwright.locator('.preview-grid.is-before').isVisible(),'修改前必须展示旧版紧凑布局');
  await tab.playwright.getByRole('tab',{name:'修改后',exact:true}).click();
  mobileAssert(!(await tab.playwright.locator('.preview-grid.is-before').isVisible()),'修改后不能残留旧版布局');
  await mobileButton(tab,'验收通过').click();
  mobileAssert(await tab.playwright.getByText('已验收 · 决定已同步',{exact:true}).isVisible(),'成功验收必须有回执');
  mobileAssert(!(await mobileButton(tab,'已验收').isEnabled()),'已验收结果不能重复提交');
  await mobileButton(tab,'批注项目卡片').click();
  await tab.playwright.getByLabel('调整意见',{exact:true}).fill('卡片间距再松一点。');
  await mobileButton(tab,'发送调整意见').click();
  mobileAssert(await tab.playwright.getByText('等待 Agent 完成 · 暂不可验收',{exact:true}).isVisible(),'反馈送达后不能保留旧验收状态');
  await tab.playwright.getByRole('tab',{name:'对话',exact:true}).click();
  mobileAssert(await tab.playwright.getByText('验收反馈：卡片间距再松一点。',{exact:true}).isVisible(),'反馈必须出现在原任务对话');
  const chat=await tab.playwright.evaluate(()=>{const history=document.querySelector('.chat-history').getBoundingClientRect(),dock=document.querySelector('.chat-dock').getBoundingClientRect();return {overlap:history.bottom>dock.top+1,input:!!document.querySelector('#message-text'),overflow:document.documentElement.scrollWidth>innerWidth};});
  mobileAssert(chat.input&&!chat.overlap&&!chat.overflow,'聊天输入必须存在且不能挤压、覆盖消息区域');
  await mobileButton(tab,'返回指挥台').click();
  await mobileButton(tab,'项目').click();
  await mobileButton(tab,'查看卡片：完善语音输入体验').click();
  await tab.playwright.getByRole('tab',{name:'成果',exact:true}).click();
  mobileAssert(!(await mobileButton(tab,'验收通过').isEnabled()),'运行中成果不能验收');
  return '黑色外观、导航、对比、验收、反馈、聊天与未完成禁用：通过';
}

/// 场景：项目可以创建、删除卡片；只有显式安排才产生任务。
/// 数据构造：Norma OS 初始 4 张卡片；新增「界面助手」后 5 张；安排任务后仍为 5 张。
/// 执行过程：创建 Claude 空闲卡片 → 检查任务列表 → 安排前返回 → 删除取消 → 显式发送 → 删除卡片 → 查看保留任务。
/// 预期结果：创建时没有任务和运行动画；取消删除保留卡片；安排复用原卡片；删除后任务仍可访问。
async function testNormaMobileCardManagement(tab) {
  await tab.goto('http://127.0.0.1:8766/prototypes/archive/2026-09-06-norma-ios/norma-ios.html?check=card-management-'+Date.now()+'#project/norma-os');
  await mobileButton(tab,'创建卡片').click();
  mobileAssert(!(await tab.playwright.getByLabel('你希望完成什么？').isVisible()), '创建卡片时不能要求填写任务');
  await tab.playwright.getByLabel('Agent',{exact:true}).selectOption('Claude');
  await tab.playwright.getByLabel('卡片名称（选填）',{exact:true}).fill('界面助手');
  await tab.playwright.locator('#new-card-form').getByRole('button',{name:'创建卡片',exact:true}).click();
  const created=await tab.playwright.evaluate(()=>({count:document.querySelectorAll('.ai-card').length,status:document.querySelector('.ai-card').textContent,animated:document.querySelector('.ai-card').dataset.running,provider:document.querySelector('.ai-card>.provider-icon').dataset.provider}));
  mobileAssert(created.count===5&&created.status.includes('空闲'), '新卡片应出现且为空闲：4 + 1 = 5');
  mobileAssert(created.animated==='false'&&created.provider==='Claude', '空闲卡片显示选中的 Agent，但不能假装运行');
  await mobileButton(tab,'任务').click();
  mobileAssert(!(await mobileButton(tab,'查看任务：界面助手').isVisible()), '创建卡片不能自动生成同名任务');
  await mobileButton(tab,'项目').click();
  await mobileButton(tab,'查看卡片：界面助手').click();
  await mobileButton(tab,'暂时不用').click();
  await mobileButton(tab,'卡片选项：界面助手').click();
  await mobileButton(tab,'删除卡片').click();
  await mobileButton(tab,'取消').click();
  mobileAssert(await mobileButton(tab,'查看卡片：界面助手').isVisible(), '取消删除必须保留卡片');
  await mobileButton(tab,'查看卡片：界面助手').click();
  await mobileButton(tab,'安排任务').click();
  await tab.playwright.getByLabel('你希望完成什么？',{exact:true}).fill('检查手机端卡片操作');
  await mobileButton(tab,'发送到 Mac').click();
  mobileAssert(await tab.playwright.getByRole('heading',{name:'检查手机端卡片操作',exact:true}).isVisible(), '显式发送后才进入任务详情');
  await mobileButton(tab,'返回项目卡片').click();
  mobileAssert(await tab.playwright.evaluate(()=>document.querySelectorAll('.ai-card').length)===5, '安排任务不能复制卡片');
  await mobileButton(tab,'卡片选项：界面助手').click();
  await mobileButton(tab,'删除卡片').click();
  await mobileButton(tab,'确认删除卡片').click();
  mobileAssert(!(await mobileButton(tab,'查看卡片：界面助手').isVisible()), '确认删除后卡片必须消失');
  await mobileButton(tab,'任务').click();
  mobileAssert(await mobileButton(tab,'查看任务：检查手机端卡片操作').isVisible(), '删除卡片后任务记录必须仍可查看');
  return '独立创建卡片、延后安排、取消删除、关联任务复用及删除保留任务：通过';
}

/// 场景：机器注册提供扫码引导、设备核对与 Mac 确认，底部指挥台保持居中。
/// 数据构造：未配对演示；二维码代表 Alan 的 MacBook Pro；默认授权为只读。
/// 执行过程：引导 → 扫码 → 核对 → 重新扫描 → 手动错误码 → 正确码 → Mac 确认 → 指挥台 → 设备 → 取消连接。
/// 预期结果：识别不能提前配对；错误码留在原页；确认后可查看设备；返回保留现有配对；布局无横向溢出。
async function testNormaMobilePairing(tab) {
  await tab.goto('http://127.0.0.1:8766/prototypes/archive/2026-09-06-norma-ios/norma-ios.html?scenario=pairing&check=scan-'+Date.now()+'#connect');
  mobileAssert(await mobileButton(tab,'扫描二维码').isVisible(), '未配对应显示独立扫码引导');
  mobileAssert(!(await mobileButton(tab,'指挥台').isVisible()), '连接引导期间应收起主导航');
  await mobileButton(tab,'连接帮助').click();
  mobileAssert(await tab.playwright.getByText('找不到二维码？').isVisible(), '帮助应说明在哪里获取二维码');
  await mobileButton(tab,'关闭弹层').click();
  await mobileButton(tab,'扫描二维码').click();
  mobileAssert(await tab.playwright.getByRole('heading',{name:'对准 Mac 上的二维码',exact:true}).isVisible(), '应进入扫码取景页面');
  await mobileButton(tab,'模拟扫描成功').click();
  mobileAssert(await tab.playwright.getByRole('heading',{name:'在 Mac 上确认连接',exact:true}).isVisible(), '扫码后应先核对设备');
  mobileAssert(!(await mobileButton(tab,'前往指挥台').isVisible()), '扫码不能直接显示连接成功');
  await mobileButton(tab,'不是这台 Mac？重新扫描').click();
  await mobileButton(tab,'无法扫描？输入配对码').click();
  await tab.playwright.getByLabel('配对码',{exact:true}).fill('000000');
  await mobileButton(tab,'核对设备').click();
  mobileAssert(await tab.playwright.getByText('配对码不匹配，请输入演示码 482619',{exact:true}).isVisible(), '错误码应显示明确错误');
  mobileAssert(!(await mobileButton(tab,'模拟 Mac 确认并连接').isVisible()), '错误码不能进入授权确认');
  await tab.playwright.getByLabel('配对码',{exact:true}).fill('482619');
  await mobileButton(tab,'核对设备').click();
  await mobileButton(tab,'模拟 Mac 确认并连接').click();
  mobileAssert(await tab.playwright.getByRole('heading',{name:'你的 Mac，已在手边。',exact:true}).isVisible(), 'Mac 确认后应出现成功页面');
  await mobileButton(tab,'前往指挥台').click();
  const layout=await tab.playwright.evaluate(()=>{
    const title=document.querySelector('#navigation [data-page="overview"]').getBoundingClientRect();
    const screen=document.querySelector('.screen').getBoundingClientRect();
    return {offset:Math.abs(title.x+title.width/2-screen.x-screen.width/2),overflow:document.documentElement.scrollWidth>innerWidth,readOnly:document.querySelector('#connection-banner').textContent.includes('只读访问')};
  });
  // One CSS pixel allows rounding in fractional layout of the three equal navigation destinations.
  mobileAssert(layout.offset<=1, '指挥台导航应对准页面中轴，不能被其他入口挤偏');
  mobileAssert(!layout.overflow, '手机布局不能横向溢出');
  mobileAssert(layout.readOnly, '应保留 Mac 确认的默认只读权限');
  await mobileButton(tab,'设备').click();
  await mobileButton(tab,'添加另一台电脑').click();
  await mobileButton(tab,'返回设备').click();
  mobileAssert(await mobileButton(tab,'撤销当前电脑的配对').isVisible(), '取消新增连接不能撤销现有机器');
  return '扫码引导、重扫、手动码错误与恢复、Mac 确认、设备返回及中央导航：通过';
}

/// 场景：任务默认使用聊天，底部输入固定，草稿与发送回执按任务保存。
/// 数据构造：voice 是运行任务；一条带换行与尖括号的消息；320×740 为最小验收视口。
/// 执行过程：进入对话→空输入→输入草稿→切终端再返回→发送→换行/键盘发送→只读草稿。
/// 预期结果：空消息不可发送；草稿不丢失；发送后消息与模拟回复出现，输入清空且仍可继续输入。
async function testNormaMobileChat(tab) {
  await tab.goto('http://127.0.0.1:8766/prototypes/archive/2026-09-06-norma-ios/norma-ios.html?check=chat&run='+Date.now()+'#task/voice');
  mobileAssert(await tab.playwright.getByRole('tab',{name:'对话',exact:true}).isVisible(), '任务详情必须提供默认对话页');
  mobileAssert(!(await mobileButton(tab, '发送消息').isEnabled()), '空消息不能发送');
  const message='先检查 <重连> 问题\n暂时不要调整界面';
  await tab.playwright.getByLabel('消息内容',{exact:true}).fill(message);
  await tab.playwright.getByRole('tab',{name:'终端',exact:true}).click();
  await tab.playwright.getByRole('tab',{name:'对话',exact:true}).click();
  mobileAssert(await tab.playwright.evaluate(()=>document.querySelector('#message-text').value)===message, '切换视图不能丢失未发送消息');
  await mobileButton(tab, '发送消息').click();
  const chat=await tab.playwright.evaluate(()=>({
    last:document.querySelector('.chat-turn:last-child .chat-user').textContent,
    reply:document.querySelector('.chat-turn:last-child .chat-assistant').textContent,
    value:document.querySelector('#message-text').value,
    focused:document.activeElement.id,
    bottom:document.querySelector('#message-form').getBoundingClientRect().bottom,
    phoneBottom:document.querySelector('.phone').getBoundingClientRect().bottom,
    outerOverflow:document.querySelector('#main').scrollHeight-document.querySelector('#main').clientHeight,
    messageSize:getComputedStyle(document.querySelector('.chat-user')).fontSize
  }));
  mobileAssert(chat.last===message, '消息应按原文显示，不能把尖括号作为 HTML 执行');
  mobileAssert(chat.reply.includes('已加入当前任务上下文'), '应显示明确的模拟接收回复');
  mobileAssert(chat.value===''&&chat.focused==='message-text', '发送后应清空并保留输入焦点');
  mobileAssert(chat.bottom<=chat.phoneBottom&&chat.outerOverflow<=1, '输入固定可见，外层不能随聊天内容变长');
  mobileAssert(chat.messageSize==='13px', '消息字号应按用户要求缩小到13px');
  await tab.playwright.getByLabel('消息内容',{exact:true}).fill('先给我测试结论');
  await tab.playwright.getByLabel('消息内容',{exact:true}).press('Shift+Enter');
  mobileAssert(await tab.playwright.evaluate(()=>document.querySelector('#message-text').value)==='先给我测试结论\n', 'Shift+Enter 必须换行，不应发送');
  await tab.playwright.getByLabel('消息内容',{exact:true}).press('Enter');
  mobileAssert(await tab.playwright.evaluate(()=>document.querySelectorAll('.chat-turn').length)===3, 'Enter 应恰好新增第三轮对话');
  await tab.goto('http://127.0.0.1:8766/prototypes/archive/2026-09-06-norma-ios/norma-ios.html?scenario=readonly&check=chat#task/voice');
  await tab.playwright.getByLabel('消息内容',{exact:true}).fill('恢复控制后再发送');
  mobileAssert(!(await mobileButton(tab, '发送消息').isEnabled()), '只读状态可以编辑草稿，但不能发送');
  mobileAssert(await tab.playwright.evaluate(()=>document.querySelectorAll('.chat-turn').length)===1, '编辑草稿不能伪造已送达对话');
  return '聊天发送、多行草稿、回执、输入焦点与固定布局：通过';
}

/// 场景：详情收起全局导航，终端占据主要空间，输入与快捷键不挤压日志。
/// 数据构造：390×844 或 320×740 视口；日志至少占详情可用高度的 55%，即 740×0.55≈407px。
/// 执行过程：项目卡片→终端→量取内容与工具栏→发送模拟输入→中断/继续→任务信息→返回项目。
/// 预期结果：详情无全局头尾导航；日志占主要空间；输入可见；返回后导航恢复。
async function testNormaMobileDetailLayout(tab) {
  await tab.goto('http://127.0.0.1:8766/prototypes/archive/2026-09-06-norma-ios/norma-ios.html?check=detail-layout#project/norma-os');
  await mobileButton(tab, '查看卡片：完善语音输入体验').click();
  await tab.playwright.getByRole('tab',{name:'终端',exact:true}).click();
  const layout = await tab.playwright.evaluate(() => ({
    main:document.querySelector('#main').clientHeight,
    output:document.querySelector('.terminal').getBoundingClientRect().height,
    appbar:getComputedStyle(document.querySelector('.appbar')).display,
    nav:getComputedStyle(document.querySelector('#navigation')).display,
    inputBottom:document.querySelector('#terminal-form').getBoundingClientRect().bottom,
    phoneBottom:document.querySelector('.phone').getBoundingClientRect().bottom,
    overflow:document.querySelector('#main').scrollHeight-document.querySelector('#main').clientHeight
  }));
  mobileAssert(layout.output>=layout.main*.55, '终端内容必须获得至少55%的详情高度');
  mobileAssert(layout.appbar==='none' && layout.nav==='none', '详情不能保留抢占空间的全局头尾导航');
  mobileAssert(layout.inputBottom<=layout.phoneBottom && layout.overflow<=1, '输入必须可见，详情外层不能产生第二条滚动区');
  await tab.playwright.getByLabel('终端演示输入').fill('echo layout-check');
  await mobileButton(tab, '发送终端输入').click();
  mobileAssert((await tab.playwright.locator('.terminal pre').textContent()).includes('echo layout-check'), '重新布局后输入回执必须仍然显示');
  await mobileButton(tab, 'Ctrl C').click();
  await mobileButton(tab, '确认中断').click();
  mobileAssert(await mobileButton(tab, '继续任务').isVisible(), '中断后底部应提供明确的继续入口');
  mobileAssert(!(await mobileButton(tab, '发送终端输入').isVisible()), '中断时不能继续显示可发送的终端输入');
  await mobileButton(tab, '继续任务').click();
  mobileAssert(await mobileButton(tab, '发送终端输入').isEnabled(), '恢复执行后应恢复输入能力');
  await mobileButton(tab, '任务信息').click();
  mobileAssert(await tab.playwright.getByText('codex/voice-recovery',{exact:true}).isVisible(), '收起的分支信息必须仍可查看');
  await mobileButton(tab, '关闭弹层').click();
  await tab.playwright.locator('.back').click();
  mobileAssert(await mobileButton(tab, '项目').isVisible(), '返回项目后必须恢复底部导航');
  mobileAssert(!(await mobileButton(tab, '发送终端输入').isVisible()), '退出详情不能残留终端控制栏');
  return '沉浸式详情、终端空间、输入回执、元信息和返回导航：通过';
}

/// 场景：极简项目卡片呈现正确品牌，运行图标及边框连续移动，并随中断/继续/离线停止或恢复。
/// 数据构造：Norma OS 四张卡片 = 运行 2 + 完成 1 + 空闲 1；Studio Site 有待确认 3 张。
/// 执行过程：加载 → 两帧读取动画位置 → 中断/返回 → 继续/返回 → 切项目 → 离线。
/// 预期结果：只显示所属项目；亮线位置连续变化；非运行状态及离线无动态 SVG。
async function testNormaMobileProjectCards(tab) {
  await tab.goto('http://127.0.0.1:8766/prototypes/archive/2026-09-06-norma-ios/norma-ios.html?check=project-cards#project/norma-os');
  await tab.getAXState();
  let snapshot = await tab.playwright.evaluate(() => ({
    ids: [...document.querySelectorAll('.ai-card')].map(c => c.dataset.card),
    running: [...document.querySelectorAll('.ai-card[data-running=true]')].map(c => c.dataset.card),
    offset: getComputedStyle(document.querySelector('.snake-head')).strokeDashoffset,
    codex: document.querySelector('[data-card="voice"] use').getAttribute('href'),
    claude: document.querySelector('[data-card="parser"] use').getAttribute('href'),
    codexTransform: getComputedStyle(document.querySelector('[data-card="voice"] .provider-icon')).transform,
    claudeTransform: getComputedStyle(document.querySelector('[data-card="parser"] .provider-icon')).transform,
    completedAnimation: getComputedStyle(document.querySelector('[data-card="workspace"] .provider-icon')).animationName,
    voiceText: document.querySelector('[data-card="voice"]').textContent
  }));
  mobileAssert(snapshot.ids.length === 4 && snapshot.ids.includes('planner'), '全部卡片必须包含空闲卡片，共四张');
  mobileAssert(!snapshot.ids.includes('login'), '不能混入 Studio Site 的卡片');
  mobileAssert(snapshot.running.join(',') === 'voice,parser', '只能给两张运行卡片加动画');
  mobileAssert(snapshot.codex === '#brand-codex' && snapshot.claude === '#brand-claude', 'Codex 和 Claude 必须显示各自品牌图标');
  mobileAssert(!snapshot.voiceText.includes('NORMA AI') && !snapshot.voiceText.includes('验证弱网恢复') && !snapshot.voiceText.includes('3/5'), '卡片不能残留编号、阶段说明或步骤数');
  mobileAssert(snapshot.completedAnimation === 'none', '已完成卡片的品牌图标必须静止');
  await tab.getScreenshot();
  const nextOffset = Number.parseFloat(await tab.playwright.evaluate(() => getComputedStyle(document.querySelector('.snake-head')).strokeDashoffset));
  mobileAssert(Number.isFinite(nextOffset) && nextOffset !== Number.parseFloat(snapshot.offset), '两帧之间边框必须移动，不能停在同一个位置');
  mobileAssert(nextOffset > -99 && nextOffset < 1, '必须有中间插值，不能只在首尾端点跳变');
  const iconTransforms = await tab.playwright.evaluate(() => [
    getComputedStyle(document.querySelector('[data-card="voice"] .provider-icon')).transform,
    getComputedStyle(document.querySelector('[data-card="parser"] .provider-icon')).transform
  ]);
  mobileAssert(iconTransforms[0] !== snapshot.codexTransform && iconTransforms[1] !== snapshot.claudeTransform, '运行中的两种品牌图标都必须在帧间移动');
  await mobileButton(tab, '查看卡片：完善语音输入体验').click();
  await mobileButton(tab, '任务信息').click();
  await mobileButton(tab, '中断任务').click();
  await mobileButton(tab, '确认中断').click();
  await mobileButton(tab, '返回项目卡片').click();
  mobileAssert(await tab.playwright.evaluate(() => document.querySelector('[data-card="voice"]').dataset.running === 'false' && !document.querySelector('[data-card="voice"] .snake-border')), '中断后不能残留亮线动画');
  mobileAssert(await tab.playwright.evaluate(() => getComputedStyle(document.querySelector('[data-card="voice"] .provider-icon')).animationName === 'none'), '中断任务后图标动画也必须停止');
  await mobileButton(tab, '查看卡片：完善语音输入体验').click();
  await mobileButton(tab, '继续任务').click();
  await mobileButton(tab, '返回项目卡片').click();
  mobileAssert(await tab.playwright.evaluate(() => !!document.querySelector('[data-card="voice"] .snake-border')), '继续任务后必须恢复边框动画');
  await mobileButton(tab, '切换项目').click();
  await tab.playwright.getByRole('button', {name:'Studio Site 3 张 Norma AI 卡片', exact:true}).click();
  await tab.getAXState();
  mobileAssert(await tab.playwright.evaluate(() => document.querySelectorAll('.ai-card').length === 3 && document.querySelector('.ai-card').dataset.card === 'login'), '切换项目后必须只有该项目的卡片');
  mobileAssert(await tab.playwright.evaluate(() => !document.querySelector('.snake-border')), '等待确认卡片不能模拟运行');
  await mobileButton(tab, '设备').click();
  await mobileButton(tab, '模拟断线').click();
  await mobileButton(tab, '项目').click();
  await mobileButton(tab, '切换项目').click();
  await tab.playwright.getByRole('button', {name:'Norma OS 4 张 Norma AI 卡片', exact:true}).click();
  mobileAssert(await tab.playwright.evaluate(() => !document.querySelector('.snake-border')), '离线快照不能保留运行动画');
  await mobileButton(tab, '模拟重连').click();
  mobileAssert(await tab.playwright.evaluate(() => document.querySelectorAll('.snake-border').length === 2), '重连后两个运行卡片应恢复动画');
  return '项目卡片、连续边框、中断/继续、项目隔离与离线：通过';
}

/// 场景：用户搜索、新建任务、发送方向、中断与继续、处理测试授权。
/// 数据构造：初始任务 6 个；新增 1 个 → 共 7 个；唯一测试标题避免匹配歧义。
/// 执行过程：搜索不存在名称 → 空结果；清除 → 创建 → 指令回执 → 中断 → 继续 → 审批。
/// 预期结果：新任务和消息存在；未创建任务不出现；拒绝/允许入口不在处理后残留。
async function testNormaMobileTasks(tab) {
  await tab.goto('http://127.0.0.1:8766/prototypes/archive/2026-09-06-norma-ios/norma-ios.html#tasks');
  await tab.playwright.getByRole('searchbox', {name:'搜索任务'}).fill('不存在的测试任务');
  await tab.getAXState();
  mobileAssert(await tab.playwright.getByRole('heading', {name:'没有找到任务'}).isVisible(), '搜索无结果应显示空状态');
  mobileAssert(!(await mobileButton(tab, '查看任务：完善语音输入体验').isVisible()), '筛选无结果时不能残留旧任务');
  await mobileButton(tab, '清除筛选').click();
  await mobileButton(tab, '新建任务').click();
  await tab.playwright.getByLabel('你希望完成什么？').fill('验证 iPhone 的断线恢复');
  await mobileButton(tab, '发送到 Mac').click();
  await tab.getAXState();
  mobileAssert(await tab.playwright.getByRole('heading', {name:'验证 iPhone 的断线恢复'}).isVisible(), '新建任务应打开自己的详情');
  mobileAssert(await tab.playwright.getByText('等待 Agent 接收', {exact:true}).isVisible(), '送达不等于执行，应显示等待接收');
  await tab.playwright.getByLabel('消息内容').fill('保留草稿，恢复后让我重新确认。');
  await mobileButton(tab, '发送消息').click();
  await tab.getAXState();
  mobileAssert(await tab.playwright.getByText('保留草稿，恢复后让我重新确认。', {exact:true}).isVisible(), '发送的方向应保留在任务内');
  await mobileButton(tab, '任务信息').click();
  await mobileButton(tab, '中断任务').click();
  await mobileButton(tab, '确认中断').click();
  await tab.getAXState();
  mobileAssert(await mobileButton(tab, '继续任务').isVisible(), '中断后必须可以继续');
  mobileAssert(!(await mobileButton(tab, '中断任务').isVisible()), '中断状态不能再次中断');
  await mobileButton(tab, '继续任务').click();
  await tab.playwright.locator('.back').click();
  await mobileButton(tab, '查看任务：优化登录与设备连接').click();
  await tab.playwright.getByRole('tab', {name:'变更',exact:true}).click();
  await mobileButton(tab, '允许这一次').click();
  await tab.getAXState();
  mobileAssert(!(await mobileButton(tab, '允许这一次').isVisible()), '审批处理后不应保留可重复批准按钮');
  mobileAssert(!(await mobileButton(tab, '拒绝').isVisible()), '审批处理后不应保留旧拒绝按钮');
  return '任务搜索、新建、指令、中断、继续与审批：通过';
}

/// 场景：手机离线、只读和配对撤销不会产生越权操作。
/// 数据构造：演示配对码 482619；错误码 000000；一条离线草稿。
/// 执行过程：断线→保存草稿→重连→显式发送→只读→恢复授权→撤销→错误码→正确配对。
/// 预期结果：离线发送禁用，草稿重连后存在，只读不能发送；正确配对才恢复访问。
async function testNormaMobileConnection(tab) {
  await tab.goto('http://127.0.0.1:8766/prototypes/archive/2026-09-06-norma-ios/norma-ios.html?check=connection&run='+Date.now()+'#devices');
  await mobileButton(tab, '模拟断线').click();
  await mobileButton(tab, '任务').click();
  await mobileButton(tab, '新建任务').click();
  await tab.getAXState();
  mobileAssert(!(await mobileButton(tab, '发送到 Mac').isEnabled()), '离线发送按钮必须禁用');
  await tab.playwright.getByLabel('你希望完成什么？').fill('离线草稿验证');
  await mobileButton(tab, '保留草稿').click();
  await mobileButton(tab, '模拟重连').click();
  await mobileButton(tab, '继续编辑').click();
  await tab.getAXState();
  mobileAssert(await tab.playwright.getByLabel('你希望完成什么？').textContent() === '离线草稿验证', '重连后不能丢失草稿');
  await mobileButton(tab, '发送到 Mac').click();
  await tab.playwright.locator('.back').click();
  await mobileButton(tab, '设备').click();
  await mobileButton(tab, '切换只读').click();
  await mobileButton(tab, '任务').click();
  await mobileButton(tab, '新建任务').click();
  await tab.getAXState();
  mobileAssert(!(await mobileButton(tab, '发送到 Mac').isEnabled()), '只读用户不得发送新任务');
  await mobileButton(tab, '关闭弹层').click();
  await mobileButton(tab, '设备').click();
  await mobileButton(tab, '模拟 Mac 授予控制').click();
  await mobileButton(tab, '撤销当前电脑的配对').click();
  await mobileButton(tab, '撤销配对').click();
  await tab.getAXState();
  mobileAssert(await mobileButton(tab, '扫描二维码').isVisible(), '撤销后必须返回扫码引导页');
  mobileAssert(!(await mobileButton(tab, '新建任务').isVisible()), '未配对不能访问新建任务入口');
  await mobileButton(tab, '输入配对码').click();
  await tab.playwright.getByLabel('配对码', {exact:true}).fill('000000');
  await mobileButton(tab, '核对设备').click();
  await tab.getAXState();
  mobileAssert(await tab.playwright.getByText('配对码不匹配，请输入演示码 482619', {exact:true}).isVisible(), '错误码必须显示错误而非连接成功');
  await tab.playwright.getByLabel('配对码', {exact:true}).fill('482619');
  await mobileButton(tab, '核对设备').click();
  await mobileButton(tab, '模拟 Mac 确认并连接').click();
  await mobileButton(tab, '前往指挥台').click();
  await tab.getAXState();
  mobileAssert(await tab.playwright.getByRole('heading', {name:'指挥台',exact:true}).isVisible(), '正确配对应恢复决策入口');
  return '离线草稿、重连、只读、撤销与配对：通过';
}

/// 场景：指挥台位于底部正中，设备移到顶部；旧发布链接安全返回指挥台。
/// 数据构造：底部项目、指挥台、任务共 3 项；顶部设备 1 项；中心误差容许 1 CSS px 的布局舍入。
/// 执行过程：打开旧链接 → 量取指挥台与导航中轴 → 通知 → 项目 → 任务 → 指挥台 → 顶部设备。
/// 预期结果：指挥台居中且保留品牌图标；设备不占底部位置；四个页面均可访问，无发布入口。
async function testNormaMobileNavigation(tab) {
  await tab.goto('http://127.0.0.1:8766/prototypes/archive/2026-09-06-norma-ios/norma-ios.html?check=navigation-'+Date.now()+'#releases');
  await tab.getAXState();
  const navigation = await tab.playwright.evaluate(() => ({
    labels: [...document.querySelectorAll('#navigation button')].map(b => b.textContent),
    offset: Math.abs(document.querySelector('#navigation [data-page="overview"]').getBoundingClientRect().x+document.querySelector('#navigation [data-page="overview"]').getBoundingClientRect().width/2-document.querySelector('#navigation').getBoundingClientRect().x-document.querySelector('#navigation').getBoundingClientRect().width/2),
    hasBrand: !!document.querySelector('#navigation [data-page="overview"] .norma-mark'),
    deviceInHeader: !!document.querySelector('.appbar [data-page="devices"]'),
    page: document.querySelector('[data-screen]').dataset.screen,
    hash: location.hash
  }));
  mobileAssert(navigation.labels.join(',') === '项目,指挥台,任务', '底部应是三项，指挥台排在中间，设备不应仍占底部位置');
  mobileAssert(navigation.offset<=1, '指挥台入口必须位于底部导航几何中心');
  mobileAssert(navigation.hasBrand, '指挥台应使用 Norma 标志，不能继续使用方向箭头');
  mobileAssert(navigation.deviceInHeader, '设备入口必须保留在顶部');
  mobileAssert(navigation.page === 'overview' && navigation.hash === '#overview', '旧发布链接必须回到指挥台，不能白屏');
  await mobileButton(tab, '通知中心').click();
  mobileAssert(!(await tab.playwright.getByText(/发布/).isVisible()), '通知不能残留发布内容');
  await mobileButton(tab, '关闭弹层').click();
  await mobileButton(tab, '项目').click();
  mobileAssert(await mobileButton(tab, '查看卡片：完善语音输入体验').isVisible(), '项目卡片入口应继续可用');
  await mobileButton(tab, '任务').click();
  mobileAssert(await mobileButton(tab, '新建任务').isVisible(), '底部任务入口应正常进入任务页');
  await mobileButton(tab, '指挥台').click();
  mobileAssert(await tab.playwright.getByRole('heading',{name:'指挥台',exact:true}).isVisible(), '中央入口应打开指挥台');
  await mobileButton(tab, '设备').click();
  mobileAssert(!(await mobileButton(tab, '发布失败').isVisible()), '设备页不能残留发布场景按钮');
  return '底部三项导航、指挥台居中、旧链接回退与顶部设备入口：通过';
}

/// 场景：短滑不决策，跳过不丢请求，左右滑分别允许/拒绝，最终出现空状态。
/// 数据构造：三项待确认；卡宽 W，短滑 0.1W，确认滑动 0.42W > 门槛 max(88, 0.28W)。
/// 执行过程：短滑→跳过→右滑锁文件→左滑连接测试→按钮允许快照→处理记录。
/// 预期结果：队列 3→3→2→1→0；记录正好三条；空队列没有允许按钮。
async function testNormaMobileDecisions(tab) {
  await tab.goto('http://127.0.0.1:8766/prototypes/archive/2026-09-06-norma-ios/norma-ios.html?check=decisions#overview');
  await tab.playwright.getByRole('tab', {name:'待你决策 · 3',exact:true}).click();
  const dragCard = async ratio => {
    const point = await tab.playwright.evaluate(() => {
      const card=document.querySelector('#decision-card').getBoundingClientRect();
      const reason=document.querySelector('.decision-reason').getBoundingClientRect();
      return {x:card.x+card.width/2,y:reason.y+10,width:card.width};
    });
    await tab.drag([point.x,point.y],[point.x+point.width*ratio,point.y]);
    await tab.getAXState();
  };
  await dragCard(0.1);
  mobileAssert(await tab.playwright.evaluate(() => document.querySelector('#decision-card').dataset.decision === 'login'), '短滑不能处理第一项');
  await mobileButton(tab, '暂时跳过').click();
  mobileAssert(await tab.playwright.evaluate(() => document.querySelector('.decision-subtitle').textContent.includes('3 个请求')), '跳过不能减少待处理数量');
  await dragCard(0.42);
  mobileAssert(await tab.playwright.evaluate(() => document.querySelector('#decision-card').dataset.decision === 'login'), '允许锁文件后必须回到仍待处理的连接测试');
  await dragCard(-0.42);
  mobileAssert(await tab.playwright.evaluate(() => document.querySelector('#decision-card').dataset.decision === 'snapshots'), '拒绝后必须只剩界面快照');
  await mobileButton(tab, '允许一次').click();
  await tab.getAXState();
  mobileAssert(await tab.playwright.getByRole('heading',{name:'全部处理好了',exact:true}).isVisible(), '最后一项处理完必须显示空状态');
  mobileAssert(!(await mobileButton(tab, '允许一次').isVisible()), '空队列不能保留确认按钮');
  await mobileButton(tab, '处理记录').click();
  mobileAssert(await tab.playwright.evaluate(() => document.querySelectorAll('#sheet [data-task]').length === 3), '每个决定必须有一条回执，不能重复');
  await mobileButton(tab, '关闭弹层').click();
  return '决策卡短滑、跳过、左右决策、空状态与处理记录：通过';
}


/// 场景：用户只做方向与确认决策，Agent 待办跟随任务状态。
/// 数据构造：五项待办；默认 login 第一，方向改为 voice 后仍是五项。
/// 执行过程：调整优先项和目标→提交→查看待办交付→任务收到指令→确认 login→待办同步。
/// 预期结果：表单确实提交；方向/排序一致；负责人是 Agent，用户无完成勾选；确认只更新对应工作。
async function testNormaMobileNextSteps(tab) {
  await tab.goto('http://127.0.0.1:8766/prototypes/archive/2026-09-06-norma-ios/norma-ios.html?check=nextsteps-e2e#overview');
  await mobileButton(tab, '调整方向').click();
  await tab.playwright.locator('#direction-form select[name="priorityId"]').selectOption('voice');
  await tab.playwright.locator('#direction-form textarea[name="text"]').fill('先保证语音稳定，视觉调整稍后再做。');
  await mobileButton(tab, '交给 Norma').click();
  await tab.getAXState();
  const plan = await tab.playwright.evaluate(() => ({
    direction:document.querySelector('#plan-direction').textContent,
    first:document.querySelector('.todo-row').dataset.todo,
    count:document.querySelectorAll('.todo-row').length,
    checkboxes:document.querySelectorAll('.todo-list input[type="checkbox"]').length,
    dialog:document.querySelector('#sheet').open
  }));
  mobileAssert(!plan.dialog, '发送后表单必须关闭，不能因字段名遮蔽 form.id 而静默不提交');
  mobileAssert(plan.direction==='先保证语音稳定，视觉调整稍后再做。', '当前方向必须更新为提交内容');
  mobileAssert(plan.first==='voice' && plan.count===5, '调整优先级必须重排且保留五项待办');
  mobileAssert(plan.checkboxes===0, '用户不能通过勾选伪造 Agent 已完成');
  await mobileButton(tab, '查看待办：验证弱网下的语音连续性').click();
  mobileAssert(await tab.playwright.getByText('Codex 负责执行',{exact:true}).isVisible(), '待办详情必须标明执行 Agent');
  await mobileButton(tab, '查看任务').click();
  mobileAssert(await tab.playwright.getByText('方向调整：先保证语音稳定，视觉调整稍后再做。',{exact:true}).isVisible(), 'Agent 任务上下文必须收到方向');
  await tab.playwright.locator('.back').click();
  await tab.playwright.locator('[data-decision-open="login"]').click();
  await mobileButton(tab, '允许一次').click();
  await tab.playwright.getByRole('tab',{name:'行动计划',exact:true}).click();
  mobileAssert((await tab.playwright.locator('[data-todo="login"]').textContent()).includes('进行中'), '允许后连接测试必须由 Agent 继续执行');
  mobileAssert(!(await tab.playwright.locator('[data-todo="login"]').textContent()).includes('等你确认'), '已批准事项不能继续阻塞用户');
  return '下一步梳理、方向提交、优先级、Agent 回执与审批联动：通过';
}

/// 场景：切换电脑保留各自的草稿与任务，浏览器历史不会串到其他机器。
/// 数据构造：MacBook 和 Mac mini 各有 voice ID；MacBook 留一条草稿，mini 发送一条不同消息。
/// 执行过程：A 草稿 → B → 后退/前进 → B 发消息 → A 草稿。
/// 预期结果：消息不串机；不同项目可辨认；A 的草稿和任务保持原样。
async function testNormaMobileMachines(tab) {
  await tab.goto('http://127.0.0.1:8766/prototypes/archive/2026-09-06-norma-ios/norma-ios.html?check=machines-'+Date.now()+'#project/norma-os');
  await mobileButton(tab,'查看卡片：完善语音输入体验').click();
  await tab.playwright.getByLabel('消息内容',{exact:true}).fill('MacBook 的未发送草稿');
  await mobileButton(tab,'返回项目卡片').click();
  await mobileButton(tab,'切换电脑').click();
  await mobileButton(tab,'切换到工作室 Mac mini').click();
  mobileAssert(await tab.playwright.getByRole('heading',{name:'Website',exact:true}).isVisible(),'mini 应显示自己的 Website 项目');
  mobileAssert(!(await mobileButton(tab,'查看卡片：完善语音输入体验').isVisible()),'mini 不能显示 MacBook 的语音卡片');
  await tab.back();
  mobileAssert(await tab.playwright.getByRole('heading',{name:'Norma OS',exact:true}).isVisible(),'后退必须同时恢复原电脑和项目');
  await tab.forward();
  mobileAssert(await tab.playwright.getByRole('heading',{name:'Website',exact:true}).isVisible(),'前进必须恢复 mini 的项目');
  await mobileButton(tab,'查看卡片：优化官网首屏').click();
  mobileAssert(await tab.playwright.locator('#message-text').evaluate(e=>e.value)==='','mini 同 ID 任务不能继承 MacBook 草稿');
  await tab.playwright.getByLabel('消息内容',{exact:true}).fill('这条指令只发给 Mac mini');
  await mobileButton(tab,'发送消息').click();
  mobileAssert(await tab.playwright.getByText('这条指令只发给 Mac mini',{exact:true}).isVisible(),'mini 消息必须有对话记录');
  await mobileButton(tab,'返回项目卡片').click();
  await mobileButton(tab,'切换电脑').click();
  await mobileButton(tab,'切换到Alan 的 MacBook Pro').click();
  await mobileButton(tab,'查看卡片：完善语音输入体验').click();
  mobileAssert(await tab.playwright.locator('#message-text').evaluate(e=>e.value)==='MacBook 的未发送草稿','切回后必须恢复 MacBook 草稿');
  mobileAssert(!(await tab.playwright.getByText('这条指令只发给 Mac mini',{exact:true}).isVisible()),'mini 的消息不能出现在 MacBook 对话');
  return '电脑切换、浏览器历史、同 ID 消息和草稿隔离：通过';
}

/// 场景：单台机器离线、只读或撤销配对不影响其他电脑。
/// 数据构造：mini 可控且在线；MacBook 的两个运行任务作为未受影响的对照。
/// 执行过程：mini 只读 → 离线 → 禁止创建 → 撤销 → 切换 MacBook。
/// 预期结果：mini 无法写入；撤销后仍能切换；MacBook 可创建且两个 Agent 继续运行。
async function testNormaMobileMachineAccess(tab) {
  await tab.goto('http://127.0.0.1:8766/prototypes/archive/2026-09-06-norma-ios/norma-ios.html?machine=mac-mini&check=machine-access-'+Date.now()+'#devices');
  await mobileButton(tab,'切换只读').click();
  await mobileButton(tab,'模拟断线').click();
  await mobileButton(tab,'项目').click();
  mobileAssert(!(await mobileButton(tab,'创建卡片').isEnabled()),'离线只读的 mini 不能创建卡片');
  await mobileButton(tab,'设备').click();
  await mobileButton(tab,'撤销当前电脑的配对').click();
  await mobileButton(tab,'撤销配对').click();
  await mobileButton(tab,'切换电脑').click();
  await mobileButton(tab,'切换到Alan 的 MacBook Pro').click();
  await mobileButton(tab,'项目').click();
  mobileAssert(await mobileButton(tab,'创建卡片').isEnabled(),'撤销 mini 后 MacBook 仍应可操作');
  mobileAssert(await tab.playwright.locator('.ai-card[data-running="true"]').count()===2,'切换不应中断 MacBook 的两个 Agent');
  return '单台电脑的只读、离线、撤销与其他电脑继续运行：通过';
}

/// 场景：添加另一台 Mac 需要确认才入列，不覆盖已配对电脑。
/// 数据构造：初始 2 台；取消一次仍为 2 台；Mac 确认后 2 + 1 = 3 台，新机无卡片。
/// 执行过程：添加 → 取消 → 再添加 → 扫码 → 命名 → Mac 只读确认 → 查看设备 → 切回旧电脑。
/// 预期结果：取消不能注册；新机默认只读；原机器和运行卡片仍存在。
async function testNormaMobileAddMachine(tab) {
  await tab.goto('http://127.0.0.1:8766/prototypes/archive/2026-09-06-norma-ios/norma-ios.html?check=add-machine-'+Date.now()+'#project/norma-os');
  await mobileButton(tab,'切换电脑').click();
  await mobileButton(tab,'添加电脑').click();
  await mobileButton(tab,'返回设备').click();
  mobileAssert(await tab.playwright.locator('#main .machine-row').count()===2,'取消添加不能注册第三台电脑');
  await mobileButton(tab,'连接 Mac').click();
  await mobileButton(tab,'扫描二维码').click();
  await mobileButton(tab,'模拟扫描成功').click();
  await tab.playwright.getByLabel('电脑名称',{exact:true}).fill('家里的 Mac Studio');
  await mobileButton(tab,'模拟 Mac 确认并连接').click();
  mobileAssert(await tab.playwright.getByRole('heading',{name:'你的 Mac，已在手边。',exact:true}).isVisible(),'新电脑确认后必须展示成功页');
  await mobileButton(tab,'查看设备').click();
  mobileAssert(await tab.playwright.locator('#main .machine-row').count()===3,'确认后应有三台独立电脑');
  mobileAssert(await mobileButton(tab,'切换到家里的 Mac Studio').getAttribute('aria-pressed')==='true','新增机器应成为当前电脑');
  await mobileButton(tab,'项目').click();
  mobileAssert(await tab.playwright.locator('.ai-card').count()===0,'新机器不应复制已有电脑的卡片');
  mobileAssert(!(await mobileButton(tab,'创建卡片').isEnabled()),'新机器的只读授权不能变成可控制');
  await mobileButton(tab,'切换电脑').click();
  await mobileButton(tab,'切换到Alan 的 MacBook Pro').click();
  mobileAssert(await tab.playwright.locator('.ai-card').count()===4,'新增机器后原 MacBook 的四张卡片必须保留');
  mobileAssert(await mobileButton(tab,'创建卡片').isEnabled(),'新增机器不能改变原 MacBook 的授权');
  return '添加、取消、Mac 确认、独立空状态和原电脑保留：通过';
}

export {testNormaMobileMachines,testNormaMobileMachineAccess,testNormaMobileAddMachine};
