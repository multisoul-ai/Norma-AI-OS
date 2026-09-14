// Run: node scripts/check-norma-realtime-prototype.cjs
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const html = fs.readFileSync('docs/prototypes/norma-realtime.html', 'utf8');
const source = html.match(/<script id="realtime-model">([\s\S]*?)<\/script>/);
assert.ok(source, '原型必须提供可独立检查的状态逻辑');
for (const [, script] of html.matchAll(/<script[^>]*>([\s\S]*?)<\/script>/g)) new vm.Script(script);
const context = vm.createContext({});
vm.runInContext(source[1] + '\nthis.api={initialRealtime, act};', context);
const {initialRealtime, act} = context.api;

/// 场景：讨论不派工，浏览项目不改变通话目标，重复回执不重复执行。
/// 数据构造：初始任务 3 条；明确派工新增 1 条，最终应为 3 + 1 = 4 条。
/// 执行过程：开聊 → 讨论 → 浏览 Studio Site → 明确派工 → 重复请求 → 挂断。
/// 预期结果：派工仍属 Norma OS；重复请求不新增；挂断不取消任务。
{
  const s = initialRealtime();
  act(s, {type:'start'});
  act(s, {type:'discuss', text:'让指挥台支持语音派工'});
  assert.equal(s.tasks.length, 3, '讨论阶段不能创建任务');
  act(s, {type:'browse', project:'Studio Site'});
  assert.equal(s.project, 'Norma OS', '浏览页面不能切换通话绑定项目');
  assert.equal(act(s, {type:'dispatch', operation:'op-1'}).ok, true, '明确派工应返回已接收');
  assert.equal(s.tasks.length, 4, '派工应新增一条任务');
  assert.equal(s.tasks[0].project, 'Norma OS', '新任务必须进入通话绑定项目');
  assert.notEqual(s.tasks[0].project, 'Studio Site', '不能误派到正在浏览的项目');
  act(s, {type:'dispatch', operation:'op-1'});
  assert.equal(s.tasks.length, 4, '同一回执重试不得重复派工');
  act(s, {type:'hangup'});
  assert.equal(s.tasks[0].status, 'running', '挂断不能取消已派任务');
  assert.equal(s.call, false, '挂断后通话必须结束');
}

/// 场景：Mac 离线仍能讨论，重连保留草稿但不自动执行。
/// 数据构造：初始任务 3 条；离线输入一条想法，显式派工后为 3 + 1 = 4 条。
/// 执行过程：开聊 → Mac 离线 → 讨论与派工 → 重连 → 显式派工。
/// 预期结果：离线派工被拒绝；草稿不丢失；重连不自动新增任务。
{
  const s = initialRealtime();
  act(s, {type:'start'});
  act(s, {type:'connection'});
  act(s, {type:'discuss', text:'保留离线讨论'});
  assert.equal(s.call, true, 'Mac 离线不能中止语音会话');
  assert.equal(act(s, {type:'dispatch', operation:'offline'}).ok, false, '离线不能声称派工成功');
  assert.equal(s.draft, '保留离线讨论', '离线想法必须保留');
  act(s, {type:'connection'});
  assert.equal(s.tasks.length, 3, '重连不能自动提交草稿');
  act(s, {type:'dispatch', operation:'online'});
  assert.equal(s.tasks.length, 4, '明确继续后应派工');
  assert.equal(s.draft, '', '产生接收回执后才清除草稿');
}

/// 场景：界面成果必须经用户查看，退回修改沿用原任务。
/// 数据构造：web、desktop 待验收，voice 运行中，共 3 条任务；一条具体修改意见。
/// 执行过程：越序验收 → 查看 web → 验收 → 重复验收 → 退回 desktop。
/// 预期结果：越序和重复请求失败；查看后才能验收；反馈只更新原任务。
{
  const s = initialRealtime();
  assert.equal(act(s, {type:'accept', id:'voice'}).ok, false, '运行中不能验收');
  assert.equal(act(s, {type:'accept', id:'web'}).ok, false, '未打开成果不能算用户已查看');
  act(s, {type:'inspect', id:'web'});
  assert.equal(act(s, {type:'accept', id:'web'}).ok, true, '查看成果后应能验收');
  assert.equal(s.tasks.find(t=>t.id==='web').status, 'done', 'web 应留下用户验收状态');
  assert.equal(act(s, {type:'accept', id:'web'}).ok, false, '同一成果不能重复验收');
  act(s, {type:'inspect', id:'desktop'});
  act(s, {type:'feedback', id:'desktop', text:'增大卡片间距'});
  assert.equal(s.tasks.find(t=>t.id==='desktop').status, 'running', '反馈必须让原任务继续修改');
  assert.equal(s.tasks.find(t=>t.id==='desktop').seen, false, '新版本不能沿用旧查看状态');
  assert.equal(s.tasks.find(t=>t.id==='desktop').feedback, '增大卡片间距', '调整意见必须保留');
  assert.equal(s.tasks.length, 3, '退回修改不得复制任务');
}
console.log('PASS · 项目绑定、讨论/派工、去重、离线草稿、挂断与验收；全部脚本语法通过');
