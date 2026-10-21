import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const source = html.match(/<script id="assistant-ideas">([\s\S]*?)<\/script>/)?.[1];
const sandbox = {};
if (source) runInNewContext(source, sandbox);
const plain = value => JSON.parse(JSON.stringify(value));
const ideas = () => sandbox.AssistantIdeas;

/// 场景：用户在对话里直接让 Norma 记下想法。
/// 数据构造：几种常见说法，以及不该被捕获的普通指令。
/// 执行过程：逐条调用 capture。
/// 预期结果：前缀、后缀、“别忘了”都能提取正文；查看进度、分配类指令返回 null。
test('对话中的“记一下”被提取为想法标题', () => {
  assert.ok(source, '页面应提供独立的想法逻辑');
  assert.equal(ideas().capture('记一下：给设置页加一个深色主题开关'), '给设置页加一个深色主题开关');
  assert.equal(ideas().capture('帮我记下 修复终端卡片标题闪烁'), '修复终端卡片标题闪烁');
  assert.equal(ideas().capture('把抽屉动效再调轻一点，记下来'), '把抽屉动效再调轻一点');
  assert.equal(ideas().capture('别忘了给 README 补截图。'), '给 README 补截图');
  assert.equal(ideas().capture('看看项目进度'), null);
  assert.equal(ideas().capture('把第一条想法交给 Bella，记下来'), null, '带指派语义的话属于派发，不是新想法');
  assert.equal(ideas().capture('记一下'), null, '没有正文不应生成空想法');
});

/// 场景：用口述把想法交给某个智能体或新建智能体。
/// 数据构造：序数、“刚才那个”、中英文名字、新建智能体的几种说法。
/// 执行过程：调用 assignIntent。
/// 预期结果：解析出第几条/最新一条与目标；缺少执行者或不是派发语义时返回 null。
test('口述指派解析想法序号与目标智能体', () => {
  assert.deepEqual(plain(ideas().assignIntent('把第二条想法交给 Bruno')), {ref:{n:2},target:'bruno'});
  assert.deepEqual(plain(ideas().assignIntent('把刚才那个想法派给 Bella')), {ref:{latest:true},target:'bella'});
  assert.deepEqual(plain(ideas().assignIntent('新建一个智能体去做第3条')), {ref:{n:3},target:'new'});
  assert.deepEqual(plain(ideas().assignIntent('让新智能体处理第一个待办')), {ref:{n:1},target:'new'});
  assert.equal(ideas().assignIntent('把第一条想法交给张三'), null);
  assert.equal(ideas().assignIntent('看看项目进度'), null);
  assert.equal(ideas().wantsList('看看想法清单'), true);
  assert.equal(ideas().wantsList('看看项目进度'), false);
});

/// 场景：想法从待办流转到已指派，再被撤回或删除后恢复。
/// 数据构造：依次添加三条想法。
/// 执行过程：指派第二条；按序号与“最新”解析待办；删除后在原位置恢复；重复指派。
/// 预期结果：已指派的想法离开待办且不参与序号；恢复后位置不变；已指派的想法不能被再次指派覆盖。
test('想法的指派、撤回、删除与恢复保持顺序', () => {
  let list = [];
  for (const [index, title] of ['甲','乙','丙'].entries()) list = ideas().add(list, title, {now:1000 + index}).list;
  assert.equal(list.length, 3);
  list = ideas().assign(list, list[1].id, {id:'agent-bruno',name:'Bruno'});
  const { todo, assigned } = ideas().groups(list);
  assert.deepEqual(plain(todo.map(item => item.title)), ['甲','丙']);
  assert.deepEqual(plain(assigned.map(item => item.assignee.name)), ['Bruno']);
  assert.equal(ideas().resolve(list, {n:2}).title, '丙', '序号只数待办');
  assert.equal(ideas().resolve(list, {latest:true}).title, '丙');
  assert.equal(ideas().resolve(list, {n:5}), null);
  const again = ideas().assign(list, list[1].id, {id:'agent-bella',name:'Bella'});
  assert.equal(again[1].assignee.name, 'Bruno', '已有执行者时不被覆盖');
  const removed = list[0];
  const afterRemove = ideas().remove(list, removed.id);
  assert.equal(afterRemove.length, 2);
  assert.deepEqual(plain(ideas().restore(afterRemove, removed, 0).map(item => item.title)), ['甲','乙','丙']);
  assert.equal(ideas().unassign(list, list[1].id)[1].assignee, null);
  assert.equal(ideas().add(list, '   ').idea, null, '空白标题不创建想法');
});

/// 场景：新建智能体命名、相对时间与本机存储。
/// 数据构造：已占用名字；损坏的存储；可写存储。
/// 执行过程：取下一个名字，格式化时间，读写存储。
/// 预期结果：名字避开占用；时间中文化；损坏或不可用存储退回空状态而不抛错。
test('新智能体名字避让、时间文案与存储容错', () => {
  assert.equal(ideas().nextAgentName(['Bella','Bruno']), 'Cleo');
  assert.equal(ideas().nextAgentName(['Cleo','Dana']), 'Eli');
  assert.equal(ideas().age(0, 30 * 1000), '刚刚');
  assert.equal(ideas().age(0, 5 * 60000), '5 分钟前');
  assert.equal(ideas().age(0, 3 * 3600000), '3 小时前');
  const bad = {getItem:() => '{坏的'};
  assert.deepEqual(plain(ideas().read(bad)), {ideas:[],spawned:[]});
  assert.deepEqual(plain(ideas().read(null)), {ideas:[],spawned:[]});
  const box = {};
  const storage = {getItem:key => box[key] ?? null,setItem:(key, value) => {box[key] = value;}};
  ideas().write(storage, {ideas:[{id:'a',title:'x',createdAt:1,assignee:null}],spawned:[]});
  assert.equal(ideas().read(storage).ideas[0].title, 'x');
  assert.doesNotThrow(() => ideas().write({setItem() {throw new Error('quota');}}, {ideas:[],spawned:[]}));
});

/// 场景：想法清单作为运行概览里的一栏接入（不再有独立标签），并提供拖拽以外的等价路径。
/// 数据构造：读取页面源码。
/// 执行过程：检查标签、面板、新建智能体投放区、指派菜单与键盘/减少动态效果分支。
/// 预期结果：只剩对话/运行概览两个标签；想法栏位于当前项目与其他项目之间且每栏有固定高度的滚动内容区；投放区存在；菜单、Esc 取消拖拽、减少动态效果时跳过流星。
test('想法清单入口、拖拽等价路径与减少动态效果均已接入', () => {
  assert.doesNotMatch(html, /id="drawer-tab-ideas"/, '想法不再是独立标签');
  const order = ['run-group-attention', 'run-group-current', 'id="ideas-body"', 'run-group-other'].map((key) => html.indexOf(key));
  assert.ok(order.every((at) => at > 0) && order.every((at, i) => i === 0 || at > order[i - 1]), '栏目顺序：需要你处理、当前项目、想法、其他项目');
  assert.match(html, /\.run-group-body\{flex:1;min-height:0;overflow-y:auto/, '每栏内容区固定高度，内容多了栏内滚动');
  assert.match(html, /data-group="ideas"[\s\S]{0,400}aria-controls="ideas-scroll"/, '想法栏可折叠');
  assert.match(html, /data-clear-assigned[\s\S]{0,40}清空/, '已指派可清空');
  assert.match(html, /data-toggle-assigned/, '已指派可折叠');
  assert.match(html, /id="idea-newzone"/, '应有“新建智能体”投放区');
  assert.match(html, /setAttribute\('role','menu'\)/,'应有指派菜单作为键盘与触屏路径');
  assert.match(html, /keyEvent\.key==='Escape'[\s\S]{0,120}cancelIdeaDrag/, 'Esc 应取消拖拽');
  assert.match(html, /if\(reduced\.matches\)\{land\(\);return;\}/, '减少动态效果时不播放流星，直接落地');
  assert.match(html, /@media\(prefers-reduced-motion:reduce\)\{\.idea-row\.is-new/, '应有减少动态效果样式');
});
