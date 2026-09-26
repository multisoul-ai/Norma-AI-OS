// Run: node scripts/check-norma-realtime-prototype.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {initialState, act, inboxItems} = require('../docs/prototypes/norma-channel-model.js');
const html = fs.readFileSync('docs/prototypes/norma-realtime.html', 'utf8');

/// [浏览器脚本加载]：模型脚本和页面脚本在同一全局环境执行时，首页应完成渲染。
/// 数据构造：1 份模型脚本 + 1 份页面内联脚本；根节点初始内容为空。
/// 执行过程：1. 加载模型脚本；2. 加载页面脚本；3. 检查根节点内容。
/// 预期结果：频道首页可见；脚本加载不发生重复声明异常或黑屏。
{
  const root = {innerHTML:''};
  const context = vm.createContext({
    window:{addEventListener(){}},
    document:{getElementById:id=>id==='root'?root:null,addEventListener(){},documentElement:{style:{setProperty(){}}}},
    location:{search:'?variant=A',href:'http://127.0.0.1:8766/prototypes/norma-realtime.html?variant=A'},
    URLSearchParams, URL, innerHeight:900, matchMedia:()=>({matches:false}),
  });
  const inline = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
  assert.ok(inline, '页面必须包含可执行的原型脚本');
  assert.doesNotThrow(() => {
    vm.runInContext(fs.readFileSync('docs/prototypes/norma-channel-model.js','utf8'), context);
    vm.runInContext(inline, context);
  }, '浏览器加载模型和页面脚本时不能发生重复声明或运行异常');
  assert.match(root.innerHTML, /频道/, '模型和页面脚本加载后应渲染频道首页');
}

/// [保存想法]：保存即建频道，但必须等明确执行指令才让 Norma 派工。
///
/// 数据构造（含关键数值的推导过程）：初始 3 个频道；保存 1 个后应有 3 + 1 = 4 个，新频道初始任务 0 个。
/// 执行过程：1. 保存一句想法 → 建频道；2. 明确开始 → 关联 2 个项目、派给 2 位 Agent。
/// 预期结果：执行前没有任务，执行后 Codex 和 Design 均有接收回执。
{
  const state = initialState();
  const saved = act(state, {type:'save', text:'让手机交接更清楚'});
  assert.equal(saved.ok, true, '想法应成功保存');
  assert.equal(state.channels.length, 4, '只应新增一个频道');
  const channel = state.channels.find(item => item.id === saved.channelId);
  assert.equal(channel.status, 'idea', '新频道应保持构思状态');
  assert.equal(channel.tasks.length, 0, '仅保存时不能生成 Agent 任务');
  assert.equal(channel.projects.length, 0, '仅保存时不能擅自关联项目');
  assert.equal(act(state, {type:'start', channelId:channel.id}).ok, true, '明确开始后应派工');
  assert.equal(channel.status, 'running', '频道应显示执行中');
  assert.equal(channel.projects.join(','), 'iOS App,Norma API', 'Norma 应关联两个项目');
  assert.equal(channel.tasks.length, 2, 'Norma 应创建两张具名任务卡');
  assert.ok(channel.messages.some(message => message.author === 'Codex' && message.text.includes('已接收')), 'Codex 应在频道回应');
  assert.ok(channel.messages.some(message => message.author === 'Design' && message.text.includes('已接收')), 'Design 应在频道回应');
}

/// [共享上下文]：频道内 @Agent 必有回应，且不会污染别的频道或复制活跃任务。
///
/// 数据构造（含关键数值的推导过程）：初始 3 个频道；在第 1 个频道追加 1 条用户消息和至少 1 条回应。
/// 执行过程：1. @Codex 检查 API；2. 检查回应、任务数、另一个频道及硬约束。
/// 预期结果：Codex 回应，原任务数不变，别的频道消息数不变，生产发布禁令保持有效。
{
  const state = initialState(), channel = state.channels[0], other = state.channels[1];
  const taskCount = channel.tasks.length, otherCount = other.messages.length;
  assert.equal(act(state, {type:'send', channelId:channel.id, text:'@Codex 请继续检查 API 状态'}).ok, true, '@Codex 指令应被接收');
  assert.ok(channel.messages.some(message => message.author === 'Codex' && message.text.includes('收到')), '@Codex 后应有明确回应');
  assert.equal(channel.tasks.length, taskCount, '续聊不应自动复制任务');
  assert.equal(other.messages.length, otherCount, '另一个频道不应混入消息');
  assert.ok(channel.hardRules.includes('禁止发布生产环境'), '硬约束必须保留');
}

/// [收件箱去重]：一个失败若已有待决定的恢复方案，只占用一个注意力事项。
/// 数据构造：初始部署检查频道有 1 次失败和 1 个待决定恢复方案，预期为 1 件待办。
/// 执行过程：1. 生成收件箱；2. 筛出部署检查事项；3. 对比其类型与数量。
/// 预期结果：保留可操作的“需要决定”；不再重复显示同一失败的“执行失败”。
{
  const items = inboxItems(initialState()).filter(item => item.channelId === 'release-check');
  assert.equal(items.length, 1, '同一失败及其恢复决定应合并为一件待办');
  assert.equal(items[0].type, 'decision', '可操作的恢复决定应作为待办入口');
  assert.ok(!items.some(item => item.type === 'failed'), '已有恢复决定时不应重复列出失败卡片');
}

/// [电脑注册与频道项目]：用户先注册电脑，再显式选择频道可用的跨项目范围。
/// 数据构造：初始 2 台电脑提供 3 个项目；注册 1 台后为 3 台，新电脑项目数为 0。
/// 执行过程：1. 检查项目来源；2. 模拟扫码确认注册；3. 给构思频道关联 Release Ops；4. 开始派工。
/// 预期结果：只使用已注册项目；手动范围被派工沿用；未知项目和重复电脑不能被加入。
{
  const state = initialState(), channel = state.channels.find(item => item.id === 'first-connect');
  assert.equal(state.machines.length, 2, '演示初始应有两台已注册电脑');
  assert.ok(state.machines.some(machine => machine.projects.includes('Release Ops')), 'Release Ops 必须来自已注册电脑');
  assert.equal(act(state, {type:'pair-computer'}).ok, true, '扫码确认后应能注册演示电脑');
  assert.equal(state.machines.length, 3, '注册后应恰好多一台电脑');
  assert.equal(state.machines.find(machine => machine.id === 'studio-mini').projects.length, 0, '新电脑不能虚构项目');
  assert.equal(act(state, {type:'pair-computer'}).ok, false, '同一电脑不能重复注册');
  assert.equal(act(state, {type:'toggle-project', channelId:channel.id, project:'不存在的仓库'}).ok, false, '不能关联未注册的项目');
  assert.equal(act(state, {type:'toggle-project', channelId:channel.id, project:'Release Ops'}).ok, true, '用户应能关联已注册项目');
  assert.deepEqual(channel.projects, ['Release Ops'], '频道只应关联用户选中的项目');
  assert.equal(act(state, {type:'start', channelId:channel.id}).ok, true, '配置项目后应可开始派工');
  assert.deepEqual(channel.projects, ['Release Ops'], 'Norma 派工不得覆盖频道手动项目范围');
  assert.ok(channel.tasks.every(task => task.project === 'Release Ops'), '两张任务卡应使用频道配置的项目');
  assert.ok(!channel.projects.includes('iOS App'), '手动只选 Release Ops 时不得自动加入 iOS App');
}

/// [工作简报纠偏]：用户修改目标和禁令后，所有 Agent 以同一频道事实继续工作。
/// 数据构造：初始频道 1 个目标、1 条禁令；另一个频道的目标作为隔离对照。
/// 执行过程：1. 尝试空目标；2. 提交新目标与 2 条禁令；3. 检查频道记录和相邻频道。
/// 预期结果：空目标被拒绝；新事实和同步回执出现；其他频道不受影响。
{
  const state=initialState(),channel=state.channels[0],other=state.channels[1],otherGoal=other.goal;
  assert.equal(act(state,{type:'update-brief',channelId:channel.id,goal:'  ',hardRules:[]}).ok,false,'工作简报不能接受空目标');
  assert.equal(act(state,{type:'update-brief',channelId:channel.id,goal:'先让交接状态清楚',hardRules:['禁止发布生产环境','不得删除用户数据']}).ok,true,'用户应能直接纠正工作简报');
  assert.equal(channel.goal,'先让交接状态清楚','所有参与者应看到更新后的目标');
  assert.deepEqual(channel.hardRules,['禁止发布生产环境','不得删除用户数据'],'两条禁令应进入正式工作简报');
  assert.ok(channel.messages.some(message=>message.author==='Norma'&&message.text.includes('工作简报已更新')),'Norma 应在频道确认已同步给参与 Agent');
  assert.equal(other.goal,otherGoal,'修改一个频道不能污染另一个频道');
}

/// [语音、验收与收口]：通话转写留在原频道，任务经用户验收后方可归档。
///
/// 数据构造（含关键数值的推导过程）：第 1 个频道原有 2 张任务；完成 2 张后形成 1 件待验收成果。
/// 执行过程：1. 开始通话并静音，拒绝转写；2. 取消静音后模拟指令并挂断；3. 模拟完成、验收、归档并重开。
/// 预期结果：静音时无转写，取消静音后的转写留在频道；未验收不能归档，重开后仍保留记录。
{
  const state = initialState(), channel = state.channels[0];
  assert.equal(act(state, {type:'call-start', channelId:channel.id}).ok, true, '当前频道应能开始通话');
  assert.equal(act(state, {type:'call-mute', channelId:channel.id}).ok, true, '通话应能切换到静音');
  assert.equal(act(state, {type:'call-utterance', channelId:channel.id}).ok, false, '静音时不能产生语音指令');
  assert.ok(!channel.messages.some(message => message.kind === 'transcript'), '静音时频道不应出现转写');
  assert.equal(act(state, {type:'call-mute', channelId:channel.id}).ok, true, '通话应能取消静音');
  assert.equal(act(state, {type:'call-utterance', channelId:channel.id}).ok, true, '语音指令应被模拟接收');
  assert.equal(act(state, {type:'call-end', channelId:channel.id}).ok, true, '通话应能结束');
  assert.ok(channel.messages.some(message => message.kind === 'transcript'), '语音转写必须留在原频道');
  assert.equal(act(state, {type:'archive', channelId:channel.id}).ok, false, '执行中的频道不能归档');
  assert.equal(act(state, {type:'complete', channelId:channel.id}).ok, true, '任务应能模拟完成');
  assert.equal(channel.status, 'review', '完成后应进入待验收');
  assert.ok(inboxItems(state).some(item => item.channelId === channel.id && item.type === 'review'), '成果应进入收件箱');
  assert.equal(act(state, {type:'archive', channelId:channel.id}).ok, false, '未验收成果不能归档');
  assert.equal(act(state, {type:'accept', channelId:channel.id}).ok, true, '用户应能验收');
  assert.equal(act(state, {type:'archive', channelId:channel.id}).ok, true, '验收后应能归档');
  assert.equal(channel.status, 'archived', '归档状态应准确');
  assert.equal(act(state, {type:'send', channelId:channel.id, text:'@Codex 继续'}).ok, false, '归档频道不能继续派发文字指令');
  assert.equal(act(state, {type:'call-start', channelId:channel.id}).ok, false, '归档频道不能通过语音继续派工');
  assert.equal(act(state, {type:'reopen', channelId:channel.id}).ok, true, '已归档频道应能重开');
  assert.equal(channel.status, 'idle', '重开后应待下一步指令');
  assert.ok(channel.messages.some(message => message.kind === 'transcript'), '重开不能丢失语音记录');
}

assert.match(html, /variant=A\|B\|C/, '原型应说明三种可分享的视觉方案');
assert.match(html, /prefers-reduced-motion/, '动效应响应减少动态效果偏好');
console.log('PASS · 频道创建、派工、@ 回应、语音、验收与归档');
