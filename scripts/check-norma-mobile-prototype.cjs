// Run: node scripts/check-norma-mobile-prototype.cjs
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const html = fs.readFileSync(path.join(__dirname, '../docs/prototypes/norma-ios.html'), 'utf8');
assert.match(html, /<html[^>]*data-dark="true"/, '用户要求的深色必须是初始外观，不能等待手动切换');
const script = html.match(/<script id="prototype-model">([\s\S]*?)<\/script>/);
assert.ok(script, '原型必须包含可独立执行的状态逻辑');
const context = vm.createContext({});
vm.runInContext(script[1] + '\nthis.model = { initialState, initialMachines, transition, projectCards, pendingDecisions, swipeDecision, nextSteps };', context);
const { initialState, initialMachines, transition, projectCards, pendingDecisions, swipeDecision, nextSteps } = context.model;
// Parse every inline script so a view-template syntax error fails this check too.
for (const [, source] of html.matchAll(/<script[^>]*>([\s\S]*?)<\/script>/g)) new vm.Script(source);

/// 场景：多台电脑中的同名任务 ID、草稿、权限和项目范围互相隔离。
/// 数据构造：两台 Mac 各有 voice 任务，MacBook 的项目为 Norma OS，Mac mini 的项目为 Website。
/// 执行过程：向 mini 的 voice 发消息 → 只读和离线控制 → 保存草稿 → 撤销 mini → 检查 MacBook。
/// 预期结果：只有 mini 收到消息；跨机器项目无法创建；MacBook 的任务、连接和权限不变。
{
  const [book, mini] = initialMachines();
  assert.equal(book.machineId, 'macbook-pro', '第一台电脑应保留已有演示上下文');
  assert.equal(mini.machineId, 'mac-mini', '第二台电脑必须有独立设备身份');
  assert.equal(transition(mini,{type:'message',id:'voice',text:'先检查首页。'}).ok,true,'mini 上的运行任务可以接收指令');
  assert.equal(mini.tasks.find(t=>t.id==='voice').messages[0],'先检查首页。','指令应进入 mini 的任务');
  assert.equal(book.tasks.find(t=>t.id==='voice').messages.length,0,'同 ID 的 MacBook 任务不能收到 mini 指令');
  assert.equal(transition(mini,{type:'create-card',project:'Norma OS',agent:'Codex'}).ok,false,'mini 不得使用 MacBook 的授权项目创建卡片');
  assert.equal(transition(mini,{type:'create',project:'Norma OS',agent:'Codex',text:'跨机器任务'}).ok,false,'新任务也必须校验当前电脑的项目范围');
  transition(mini,{type:'permission',value:'read'});
  assert.equal(transition(mini,{type:'interrupt',id:'voice'}).ok,false,'mini 的只读权限应阻止控制');
  transition(mini,{type:'connection',online:false});
  transition(mini,{type:'save-draft',text:'mini 草稿',project:'Website',agent:'Codex'});
  mini.messageDrafts.voice='mini 聊天草稿';
  assert.equal(book.messageDrafts.voice,undefined,'不同电脑不能共享聊天草稿');
  assert.equal(book.draft,null,'保存 mini 草稿不能更改 MacBook 草稿');
  assert.equal(mini.draft.text,'mini 草稿','离线电脑必须保留自己的草稿');
  transition(mini,{type:'revoke'});
  assert.equal(mini.paired,false,'撤销必须作用于所选电脑');
  assert.equal(book.paired,true,'撤销 mini 不能解除 MacBook 配对');
  assert.equal(book.online,true,'mini 离线不能影响 MacBook 连接');
  assert.equal(book.permission,'control','mini 只读不能影响 MacBook 权限');
  assert.equal(book.tasks.find(t=>t.id==='voice').status,'running','切换或撤销其他机器不能中断 MacBook 工作');
}

/// 场景：成果验收与继续调整遵守任务完成状态及设备权限。
/// 数据构造：voice 正在运行，workspace 已完成；一条明确反馈要求调整卡片间距。
/// 执行过程：运行中验收 → 只读验收 → 完成后验收 → 重复验收 → 提交反馈。
/// 预期结果：前两次与重复请求被拒绝；成功验收不改写完成事实；反馈送达后等待 Agent 接收。
{
  const reviewState = initialState();
  assert.equal(transition(reviewState, {type:'review-accept',id:'voice'}).ok, false, '运行中的成果不能被验收为完成');
  transition(reviewState, {type:'permission',value:'read'});
  assert.equal(transition(reviewState, {type:'review-accept',id:'workspace'}).ok, false, '只读访问不能提交验收决定');
  transition(reviewState, {type:'permission',value:'control'});
  assert.equal(transition(reviewState, {type:'review-accept',id:'workspace'}).ok, true, '已完成成果应允许用户验收');
  assert.equal(reviewState.tasks.find(t=>t.id==='workspace').reviewed, true, '成功验收必须留下可见回执');
  assert.equal(reviewState.tasks.find(t=>t.id==='workspace').status, 'done', '验收不能伪造新的 Agent 执行状态');
  assert.equal(transition(reviewState, {type:'review-accept',id:'workspace'}).ok, false, '不能重复验收同一份成果');
  assert.equal(transition(reviewState, {type:'review-feedback',id:'workspace',text:'  '}).ok, false, '空白反馈不能重开任务');
  assert.equal(transition(reviewState, {type:'review-feedback',id:'workspace',text:'卡片间距再松一点。'}).ok, true, '明确调整意见应送达 Agent');
  assert.equal(reviewState.tasks.find(t=>t.id==='workspace').status, 'submitted', '送达反馈只能显示等待 Agent 接收');
  assert.equal(reviewState.tasks.find(t=>t.id==='workspace').reviewed, false, '新一轮修改不能沿用旧验收结果');
  assert.equal(reviewState.tasks.find(t=>t.id==='workspace').messages.at(-1), '验收反馈：卡片间距再松一点。', '反馈必须保留在原任务对话里');
  assert.equal(reviewState.tasks.find(t=>t.id==='voice').messages.length, 0, '反馈不能进入其他任务');

  // 再次继续已验收任务会启动新一轮工作，因此旧验收标记必须失效。
  const resumed = initialState();
  transition(resumed,{type:'review-accept',id:'workspace'});
  transition(resumed,{type:'resume',id:'workspace'});
  assert.equal(resumed.tasks.find(t=>t.id==='workspace').status,'running','继续任务应恢复运行状态');
  assert.equal(resumed.tasks.find(t=>t.id==='workspace').reviewed,false,'新一轮工作不能继续显示旧成果已验收');
}

/// 场景：离线草稿不可误报已执行，确认操作不可重复或越权执行。
///
/// 数据构造：
///   initial_tasks = running(2) + approval(3) + done(1) = 6
///   new_task = 1；connected_total = initial_tasks(6) + new_task(1) = 7
///
/// 执行过程：
///   1. 离线提交新任务 → 保留草稿，任务数量不变；重连不自动发送。
///   2. 用户显式重发 → 新建任务；只读用户不能中断或审批。
///   3. 审批处理后不可重复批准；断线不改变桌面任务事实。
///   4. 撤销配对后不能继续控制；已移除的发布功能无剩余状态。
///
/// 预期结果：
///   - 新任务仅在可控连接上创建，离线不会出现“已发送”。
///   - 权限与重复审批检查生效，草稿与历史不会被断线清除。
let s = initialState();
assert.equal(s.tasks.length, 6, '初始应有六个示例任务');
transition(s, { type: 'connection', online: false });
let reply = transition(s, { type: 'create', text: '验证移动端连接', project: 'Norma OS', agent: 'Codex' });
assert.equal(s.tasks.length, 6, '离线不得创建已发送任务');
assert.equal(s.draft.text, '验证移动端连接', '离线输入应保留为草稿');
assert.equal(reply.ok, false, '离线操作不能返回成功');
transition(s, { type: 'connection', online: true });
assert.equal(s.tasks.length, 6, '重连不能自动重放写操作');
reply = transition(s, { type: 'create', ...s.draft });
assert.equal(reply.ok, true, '重连后的显式发送应该成功');
assert.equal(s.tasks.length, 7, '新任务应恰好增加一条');
assert.equal(s.tasks[0].title, '验证移动端连接', '新任务标题应来自输入');
assert.equal(s.draft, null, '发送成功才清除草稿');
transition(s, { type: 'permission', value: 'read' });
assert.equal(transition(s, { type: 'interrupt', id: 'voice' }).ok, false, '只读权限不得中断任务');
assert.equal(s.tasks.find(t => t.id === 'voice').status, 'running', '越权操作不得改变任务状态');
assert.equal(transition(s, { type: 'approve', id: 'login' }).ok, false, '只读权限不得审批');
transition(s, { type: 'permission', value: 'control' });
transition(s, { type: 'approve', id: 'login' });
assert.equal(s.tasks.find(t => t.id === 'login').status, 'running', '批准后应回到执行状态');
assert.equal(transition(s, { type: 'approve', id: 'login' }).ok, false, '已处理审批不能再次生效');
transition(s, { type: 'connection', online: false });
assert.equal(s.tasks.find(t => t.id === 'voice').status, 'running', '手机断线不能篡改桌面任务状态');
transition(s, { type: 'connection', online: true });
assert.equal('release' in s, false, '原型已移除发布页面，不应保留发布演示状态');
transition(s, { type: 'revoke' });
assert.equal(s.paired, false, '撤销配对应移除控制权限');
assert.equal(transition(s, { type: 'message', id: 'voice', text: '继续' }).ok, false, '未配对不能发送指令');
assert.equal(s.tasks.find(t => t.id === 'voice').messages.length, 0, '被拒绝指令不能显示为已发送');

/// 场景：扫描或输入配对码只识别机器，Mac 明确确认后才注册并授予访问权限。
/// 数据构造：482619 为唯一演示码；000000 为无效码；初始撤销配对后 paired=false。
/// 执行过程：无效码 → 有效码但未确认 → 不支持的权限 → Mac 确认只读 → 尝试发指令。
/// 预期结果：前三次均不能连接；确认后设备在线且只读，不能发送控制指令。
s = initialState();
transition(s, {type:'revoke'});
assert.equal(transition(s, {type:'pair', code:'000000', confirmed:true, permission:'read'}).ok, false, '无效码不能注册机器');
assert.equal(s.paired, false, '无效码不能改变未配对状态');
assert.equal(transition(s, {type:'pair', code:'482619', permission:'read'}).ok, false, '识别有效码不能跳过 Mac 确认');
assert.equal(s.paired, false, '等待 Mac 确认时不能显示连接成功');
assert.equal(transition(s, {type:'pair', code:'482619', confirmed:true, permission:'admin'}).ok, false, '未支持的权限不能默认为控制权限');
assert.equal(s.paired, false, '无效权限不能注册机器');
assert.equal(transition(s, {type:'pair', code:'482619', confirmed:true, permission:'read'}).ok, true, 'Mac 确认有效码与只读范围后应连接成功');
assert.equal(s.online, true, '连接成功应显示在线');
assert.equal(s.permission, 'read', '注册机器应保留 Mac 确认的只读权限');
assert.equal(transition(s, {type:'message', id:'voice', text:'执行任务'}).ok, false, '只读注册后仍不能控制 Agent');

/// 场景：项目展示全部 AI 卡片，运行边框严格跟随最新任务与连接状态。
/// 数据构造：Norma OS = 运行(2) + 完成(1) + 空闲(1) = 4 张；Studio Site = 待确认(3)。
/// 执行过程：查询两个项目 → 中断/继续 voice → 离线/重连 → 新建任务。
/// 预期结果：跨项目卡片不混入；完成/空闲/离线不动画；新任务增加且只增加一张卡片。
s = initialState();
let cards = projectCards(s, 'Norma OS');
assert.equal(cards.length, 4, '项目必须包含三张关联任务的卡片和一张空闲卡片');
assert.equal(cards.find(c => c.id === 'voice').animated, true, '正在运行的语音卡片应流动');
assert.equal(cards.find(c => c.id === 'parser').animated, true, '正在运行的终端卡片应流动');
assert.equal(cards.find(c => c.id === 'workspace').animated, false, '已完成卡片不能流动');
assert.equal(cards.find(c => c.id === 'planner').status, 'idle', '没有任务的卡片必须显示为空闲');
assert.equal(cards.find(c => c.id === 'planner').animated, false, '空闲卡片不能流动');
assert.equal(cards.some(c => c.id === 'login'), false, 'Studio Site 的卡片不能混入 Norma OS');
assert.equal(projectCards(s, 'Studio Site').length, 3, '切换项目后只应显示其自身卡片');
assert.equal(projectCards(s, 'Studio Site')[0].animated, false, '待确认卡片不能流动');
transition(s, {type:'interrupt', id:'voice'});
assert.equal(projectCards(s, 'Norma OS').find(c => c.id === 'voice').animated, false, '中断应立即停止对应卡片动画');
transition(s, {type:'resume', id:'voice'});
assert.equal(projectCards(s, 'Norma OS').find(c => c.id === 'voice').animated, true, '继续任务应恢复边框动画');
transition(s, {type:'connection', online:false});
assert.equal(projectCards(s, 'Norma OS').find(c => c.id === 'voice').animated, false, '离线快照不能伪装成实时运行');
assert.equal(projectCards(s, 'Norma OS').find(c => c.id === 'voice').status, 'running', '离线不能篡改任务的最后状态');
transition(s, {type:'connection', online:true});
reply = transition(s, {type:'create', project:'Norma OS', agent:'Codex', text:'新增项目卡片验证'});
cards = projectCards(s, 'Norma OS');
assert.equal(cards.length, 5, '新任务必须在所属项目增加一张卡片');
assert.equal(cards.find(c => c.taskId === reply.id).animated, false, '仅送达的任务不应显示运行动画');
assert.equal(projectCards(s, 'Studio Site').length, 3, '新任务不能增加其他项目的卡片');
console.log('Norma iOS prototype: state, permission and project card checks passed.');

/// 场景：创建卡片不安排任务；显式安排时复用该卡片；删除卡片保留任务和对话。
/// 数据构造：初始 7 张卡片、6 个任务；创建后 8 张/6 个；安排后 8 张/7 个。
/// 执行过程：创建空闲 Claude 卡片 → 校验跨项目安排 → 显式安排 → 重复安排 → 删除运行卡片及空闲卡片。
/// 预期结果：创建时没有任务和动画；安排不额外复制卡片；删除不清除任务、不停止 Agent；只读/离线不能增删。
s = initialState();
// Name limit = 40 characters; 40 + 1 = 41 exercises the rejected upper boundary.
assert.equal(transition(s,{type:'create-card',project:'Norma OS',agent:'Claude',name:'字'.repeat(41)}).ok, false, '超长卡片名称应被拒绝');
assert.equal(transition(s,{type:'create-card',project:'未授权项目',agent:'Claude'}).ok, false, '不能在未授权项目创建卡片');
assert.equal(transition(s,{type:'create-card',project:'Norma OS',agent:'未知 Agent'}).ok, false, '不能创建未支持的 Agent 卡片');
reply = transition(s, {type:'create-card', project:'Norma OS', agent:'Claude', name:'界面助手'});
assert.equal(reply.ok, true, '项目应能独立创建卡片，无需任务描述');
const createdCardId = reply.id;
assert.equal(s.cards.length, 8, '创建应只增加一张卡片：7 + 1 = 8');
assert.equal(s.tasks.length, 6, '创建卡片不能立即安排任务');
assert.equal(s.cards.find(c=>c.id===createdCardId).taskId, null, '新卡片应未绑定任务');
assert.equal(projectCards(s,'Norma OS').find(c=>c.id===createdCardId).status, 'idle', '新卡片应显示空闲');
assert.equal(projectCards(s,'Norma OS').find(c=>c.id===createdCardId).animated, false, '空闲卡片不能显示运行动画');
assert.equal(projectCards(s,'Studio Site').length, 3, '创建不能影响其他项目');
assert.equal(transition(s,{type:'create',cardId:createdCardId,project:'Studio Site',agent:'Claude',text:'检查界面'}).ok, false, '安排任务不能更改卡片所属项目');
reply=transition(s,{type:'create',cardId:createdCardId,project:'Norma OS',agent:'Claude',text:'检查界面'});
assert.equal(reply.ok, true, '显式安排应创建并绑定任务');
assert.equal(s.cards.length, 8, '安排任务应复用原卡片，不能额外增加卡片');
assert.equal(s.tasks.length, 7, '显式安排才增加任务：6 + 1 = 7');
assert.equal(s.cards.find(c=>c.id===createdCardId).taskId, reply.id, '任务必须绑定到用户选中的卡片');
assert.equal(transition(s,{type:'create',cardId:createdCardId,project:'Norma OS',agent:'Claude',text:'重复安排'}).ok, false, '已绑定任务的卡片不能被重复安排覆盖');
assert.equal(s.tasks.some(t=>t.title==='重复安排'), false, '拒绝重复安排后不能产生多余任务');
transition(s,{type:'message',id:'voice',text:'保留这段上下文'});
assert.equal(transition(s,{type:'delete-card',id:'voice'}).ok, true, '运行卡片也可以从项目移除');
assert.equal(s.cards.some(c=>c.id==='voice'), false, '目标运行卡片应被移除');
assert.equal(s.tasks.find(t=>t.id==='voice').status, 'running', '移除卡片不能擅自停止执行');
assert.equal(s.tasks.find(t=>t.id==='voice').messages[0], '保留这段上下文', '移除卡片不能删除对话');
assert.equal(transition(s,{type:'delete-card',id:'planner'}).ok, true, '空闲卡片应能删除');
assert.equal(s.cards.some(c=>c.id==='planner'), false, '删除的空闲卡片不应残留');
assert.equal(s.cards.some(c=>c.id==='parser'), true, '其他卡片应保留');
assert.equal(s.tasks.length, 7, '删除卡片不能删除关联任务');
assert.equal(transition(s,{type:'delete-card',id:'planner'}).ok, false, '重复删除不能误报成功');
transition(s,{type:'permission',value:'read'});
assert.equal(transition(s,{type:'create-card',project:'Norma OS',agent:'Codex'}).ok, false, '只读不能创建卡片');
assert.equal(transition(s,{type:'delete-card',id:'parser'}).ok, false, '只读不能删除卡片');
transition(s,{type:'permission',value:'control'});
transition(s,{type:'connection',online:false});
assert.equal(transition(s,{type:'create-card',project:'Norma OS',agent:'Codex'}).ok, false, '离线不能创建卡片');
assert.equal(transition(s,{type:'delete-card',id:'parser'}).ok, false, '离线不能删除卡片');
assert.equal(s.cards.some(c=>c.id==='parser'), true, '被拒绝的删除不能改变卡片');

/// 场景：卡片任务草稿被保留，删除空闲卡片只解除关联，不丢弃用户输入。
/// 数据构造：初始空闲卡片 planner，归属 Norma OS / Norma；一条未发送目标。
/// 执行过程：保留绑定草稿 → 删除 planner → 从独立任务入口显式发送草稿。
/// 预期结果：删除后目标仍在且解除卡片关联；未发送前任务数不变，显式发送后创建任务。
s=initialState();
transition(s,{type:'save-draft',cardId:'planner',project:'Norma OS',agent:'Norma',text:'整理下一轮工作'});
assert.equal(s.draft.cardId, 'planner', '保留草稿时必须记录原卡片');
transition(s,{type:'delete-card',id:'planner'});
assert.equal(s.draft.text, '整理下一轮工作', '删除空闲卡片不能丢弃已保留的任务草稿');
assert.equal(s.draft.cardId, null, '已删除卡片不能继续绑定任务草稿');
assert.equal(s.tasks.length, 6, '保留草稿和删除卡片不能启动任务');
assert.equal(transition(s,{type:'create',...s.draft}).ok, true, '解除卡片关联的草稿应仍能显式发送');

/// 场景：给另一张卡片安排工作，不能清除没有在本次表单中编辑的已保留草稿。
/// 数据构造：独立任务草稿「原目标」；已有空闲 Norma 卡片 planner。
/// 执行过程：保留独立任务草稿 → 显式给 planner 安排「另一个目标」。
/// 预期结果：新任务可创建，但原目标仍保留供用户继续编辑。
s=initialState();
transition(s,{type:'save-draft',project:'Norma OS',agent:'Codex',text:'原目标'});
assert.equal(transition(s,{type:'create',cardId:'planner',project:'Norma OS',agent:'Norma',text:'另一个目标'}).ok, true, '可以独立给空闲卡片安排任务');
assert.equal(s.draft.text, '原目标', '安排另一张卡片不能清除用户之前保留的草稿');
console.log('Independent card creation, task assignment, deletion and access guards: passed.');

/// 场景：决策卡片左右滑动只处理当前请求，短滑或纵向滚动不会做决定。
/// 数据构造：初始待确认 3 个；卡宽 320px → 门槛 max(88, 320×0.28)=89.6px。
/// 执行过程：80px 短滑 → 回弹；120px 右滑 → 允许；120px 左滑 → 拒绝；处理第三项 → 空队列。
/// 预期结果：待确认 3→2→1→0；重复确认不生效；离线和只读均无决策记录。
s = initialState();
assert.equal(pendingDecisions(s).length, 3, '应有三件待确认事项可连续浏览');
assert.equal(swipeDecision(80, 0, 320), null, '80px 小于89.6px，不能误触确认');
assert.equal(swipeDecision(120, 160, 320), null, '纵向滚动不能触发决定');
assert.equal(swipeDecision(120, 5, 320), 'approve', '明确右滑应允许');
assert.equal(swipeDecision(-120, 5, 320), 'reject', '明确左滑应拒绝');
transition(s, {type:'connection', online:false});
assert.equal(transition(s, {type:'approve', id:'login'}).ok, false, '离线不能确认');
assert.equal(s.decisionLog.length, 0, '离线操作不能记录为已处理');
transition(s, {type:'connection', online:true});
transition(s, {type:'permission', value:'read'});
assert.equal(transition(s, {type:'reject', id:'lockfile'}).ok, false, '只读不能拒绝请求');
assert.equal(pendingDecisions(s).length, 3, '越权动作不能移走卡片');
transition(s, {type:'permission', value:'control'});
transition(s, {type:'approve', id:'login'});
assert.equal(pendingDecisions(s).length, 2, '允许后应展示余下两项');
assert.equal(pendingDecisions(s).some(t=>t.id==='login'), false, '已允许的请求不能再次出现在队列');
assert.equal(transition(s, {type:'reject', id:'login'}).ok, false, '不能对已处理的请求重复决策');
assert.equal(s.decisionLog.length, 1, '重复决策不能新增记录');
transition(s, {type:'reject', id:'lockfile'});
assert.equal(s.tasks.find(t=>t.id==='lockfile').status, 'interrupted', '拒绝锁文件变更应中断对应任务');
assert.equal(s.tasks.find(t=>t.id==='login').status, 'running', '拒绝另一项不能影响已允许任务');
transition(s, {type:'approve', id:'snapshots'});
assert.equal(pendingDecisions(s).length, 0, '处理完三项后必须为空队列');
assert.equal(s.decisionLog.length, 3, '记录必须恰好有三条已处理事项');
console.log('Decision queue, swipe thresholds, permissions and duplicate decisions: passed.');

/// 场景：Norma 梳理下一步，Agent 执行；用户调整方向不等于批准待确认请求。
/// 数据构造：待办 = 总任务(6) - 已完成(1) = 5；默认优先 login，调整为 voice。
/// 执行过程：读待办 → 空方向/无效任务/只读/离线修改被拒 → 有效调整 → 审批同步 → 新建任务。
/// 预期结果：负责人是 Agent；已完成事项不进入待办；方向和顺序同步，审批及执行状态不被越过。
s = initialState();
assert.equal(nextSteps(s).length, 5, '五项未完成工作应进入 Agent 待办');
assert.equal(nextSteps(s)[0].id, 'login', '连接测试是初始优先事项');
assert.equal(nextSteps(s)[0].agent, 'Claude', '执行者必须是 Claude，不能把工作分配给用户');
assert.notEqual(nextSteps(s)[0].nextTitle, nextSteps(s)[0].title, '下一步应描述具体后续动作，不能重复任务标题');
assert.equal(nextSteps(s).some(t=>t.id==='workspace'), false, '已完成的工作不能仍列作待办');
assert.equal(transition(s, {type:'direction', id:'voice', text:' '}).ok, false, '空方向不能覆盖当前目标');
assert.equal(transition(s, {type:'direction', id:'workspace', text:'优先这项'}).ok, false, '已完成任务不能成为下一步');
transition(s, {type:'permission', value:'read'});
assert.equal(transition(s, {type:'direction', id:'voice', text:'先保证语音稳定'}).ok, false, '只读不能修改方向');
transition(s, {type:'permission', value:'control'});
transition(s, {type:'connection', online:false});
assert.equal(transition(s, {type:'direction', id:'voice', text:'先保证语音稳定'}).ok, false, '离线不能发送方向');
assert.equal(s.direction.priorityId, 'login', '发送失败不能改变优先事项');
transition(s, {type:'connection', online:true});
assert.equal(transition(s, {type:'direction', id:'voice', text:'先保证语音稳定，视觉调整稍后再做。'}).ok, true, '可控制时应接收明确方向');
assert.equal(nextSteps(s)[0].id, 'voice', '优先项应同步到待办第一位');
assert.equal(s.direction.text, '先保证语音稳定，视觉调整稍后再做。', '当前方向必须保留用户的原文');
assert.equal(s.tasks.find(t=>t.id==='voice').messages.at(-1), '方向调整：先保证语音稳定，视觉调整稍后再做。', '执行 Agent 应收到方向');
assert.equal(s.tasks.find(t=>t.id==='login').status, 'approval', '调整方向不能批准另一项请求');
assert.equal(s.tasks.find(t=>t.id==='voice').status, 'running', '调整顺序不能伪造完成或中断');
transition(s, {type:'approve', id:'login'});
assert.equal(nextSteps(s).find(t=>t.id==='login').status, 'running', '确认后待办应同步为 Agent 执行中');
transition(s, {type:'reject', id:'lockfile'});
assert.equal(nextSteps(s).find(t=>t.id==='lockfile').status, 'interrupted', '拒绝后的待办应保留已中断状态，不能显示为完成');
reply = transition(s, {type:'create', project:'Norma OS', agent:'Codex', text:'补充自动恢复验证'});
assert.equal(nextSteps(s).find(t=>t.id===reply.id).agent, 'Codex', '新任务应自动进入对应 Agent 的待办');
console.log('Agent next steps, direction changes and decision synchronization: passed.');
