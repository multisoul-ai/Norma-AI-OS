import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

/**
 * 场景：选好 Agent 直接创建，不再提供布局或位置参数；主题与输入校验仍可用。
 *
 * 数据构造：
 *   /new = 1 个命令词，尚无 Agent 类型。
 *   /new claude = 命令 + 类型，共 2 个词，应返回 1 个直接创建动作。
 *   /new codex right = 3 个词，旧位置参数已移除，不得静默执行。
 * 执行过程：
 *   1. 提取原型中的纯联想函数，在隔离上下文中执行。
 *   2. 分别输入完整命令、前缀、中文、未知命令和否定表达。
 *   3. 检查直接创建动作、废弃位置参数和未知输入。
 * 预期结果：
 *   - 创建候选包含 Codex、Claude，不混入主题操作。
 *   - Claude 直接创建，不出现下一步、auto/right/below 选项。
 *   - 主题返回指定主题；未知参数与否定表达不产生操作。
 */
const source = readFileSync(new URL('./command-dock.html', import.meta.url), 'utf8');
const model = source.split('/* MODEL START */')[1]?.split('/* MODEL END */')[0];
assert.ok(model, '原型必须包含可独立验证的联想函数');
const suggest = runInNewContext(model + '\ngetSuggestions;');
const agents = suggest('/new', '登录流程');
assert.ok(agents.some(x => x.profile === 'codex'), '/new 应包含 Codex');
assert.ok(agents.some(x => x.profile === 'claude'), '/new 应包含 Claude');
assert.ok(!agents.some(x => x.kind === 'theme'), '/new 不应混入主题动作');
assert.equal(agents.find(x => x.profile === 'claude')?.kind, 'create', '选择 Claude 应直接创建，不再进入位置选择');
const claude = suggest('/new claude');
assert.equal(claude.length, 1, '完整 Agent 指令应只保留一个创建动作');
assert.equal(claude[0]?.agentName, 'Claude', '完整指令必须创建指定的 Claude');
assert.equal(claude[0]?.kind, 'create', '完整指令应可直接执行');
assert.ok(!claude[0]?.next, 'Agent 候选不得要求填写下一步参数');
assert.ok(!claude.some(x => x.position), '候选不得再显示 auto/right/below 布局选择');
assert.equal(suggest('/new codex right').length, 0, '已移除的位置参数不能静默执行');
assert.equal(suggest('创建一个 Codex')[0]?.kind, 'create', '中文示例应直接创建');
assert.equal(suggest('在右边创建一个 Codex').length, 0, '已移除的位置请求不得被忽略后静默创建');
assert.equal(suggest('/theme midnight-violet', '登录流程')[0]?.theme, 'midnight-violet', '主题命令必须保留确切主题');
assert.equal(suggest('/new codex nowhere', '登录流程').length, 0, '未知位置不能静默创建');
assert.equal(suggest('/unknown', '登录流程').length, 0, '未知命令不得生成可执行操作');
assert.equal(suggest('不要创建 Codex', '登录流程').length, 0, '否定表达不得误触创建');
assert.ok(suggest('/ne', '登录流程').some(x => x.next === '/new '), '命令前缀应提供补全');
assert.equal(suggest('/terminal', '登录流程')[0]?.profile, 'terminal', '移除终端按钮后必须保留终端创建指令');
assert.equal(suggest('/browser', '登录流程')[0]?.profile, 'browser', '移除浏览器按钮后必须保留浏览器创建指令');
assert.equal(suggest('/preview', '登录流程')[0]?.profile, 'preview', '移除预览按钮后必须保留文件预览指令');
console.log('联想模型检查通过：直接创建、无位置步骤、主题、前缀与未知输入。');
