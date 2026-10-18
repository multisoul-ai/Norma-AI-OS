import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import vm from 'node:vm';

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const source = html.match(/<script id="assistant-conversation">([\s\S]*?)<\/script>/)?.[1];
const sandbox = {};
if (source) vm.runInNewContext(source, sandbox);

/// 场景：左侧聊天持续显示用户与 Norma 的多轮对话，右侧内容切换不会清除历史。
/// 数据构造：先放入一条助手欢迎语，再依次追加用户请求和助手回复。
/// 执行过程：调用对话追加函数，并检查原数组及新数组。
/// 预期结果：历史按发送顺序保留，角色与文本正确，原数组不被修改。
test('conversation history keeps both sides of each turn in order', () => {
  assert.ok(source, '页面应提供可独立验证的对话记录逻辑');
  const start = [{ role: 'assistant', text: '你好，我是 Norma。' }];
  const afterUser = sandbox.AssistantConversation.append(start, 'user', '看看项目进度');
  const afterReply = sandbox.AssistantConversation.append(afterUser, 'assistant', '项目进度已在右侧画布显示。');
  assert.deepEqual(JSON.parse(JSON.stringify(afterReply)), [
    { role: 'assistant', text: '你好，我是 Norma。' },
    { role: 'user', text: '看看项目进度' },
    { role: 'assistant', text: '项目进度已在右侧画布显示。' },
  ], '新一轮应追加在欢迎语之后');
  assert.equal(start.length, 1, '追加不应修改旧历史');
  assert.notEqual(afterReply, start, '新历史应是独立数组');
});

/// 场景：用户向上查看较早的对话时，后续助手状态与回复不应抢走滚动位置。
/// 数据构造：视区高 200，内容高 600，分别置于距底部 120 与 20 像素处。
/// 执行过程：检查助手新消息与用户主动发送两种场景的跟随决策。
/// 预期结果：旧消息阅读保持原位；接近底部或主动发送时跟随最新消息。
test('new replies follow only when the conversation is near its end', () => {
  assert.equal(sandbox.AssistantConversation.shouldFollow({scrollTop:280, scrollHeight:600, clientHeight:200}, 'assistant'), false, '距底部 120px 时不应抢走旧消息');
  assert.equal(sandbox.AssistantConversation.shouldFollow({scrollTop:380, scrollHeight:600, clientHeight:200}, 'assistant'), true, '距底部 20px 时应跟随回复');
  assert.equal(sandbox.AssistantConversation.shouldFollow({scrollTop:280, scrollHeight:600, clientHeight:200}, 'user'), true, '主动发送应显示刚发出的消息');
});

/// 场景：左侧对话区移除外观切换条和四个快捷建议后，仍可输入与发送文字。
/// 数据构造：提取原型 HTML 中的左侧 aside，避免右侧卡片按钮影响判断。
/// 执行过程：检查被移除控件和保留的输入表单。
/// 预期结果：两组控件不在左栏 DOM 中，输入框和发送按钮继续存在。
test('conversation sidebar keeps the composer without the removed controls', () => {
  const sidebar = html.match(/<aside class="conversation-panel"[\s\S]*?<\/aside>/)?.[0];
  assert.ok(sidebar, '页面应保留左侧对话区');
  assert.doesNotMatch(sidebar, /class="appearance-control"/, '左侧不应再显示外观切换条');
  assert.doesNotMatch(sidebar, /class="suggestions"/, '左侧不应再显示四个快捷建议');
  assert.match(sidebar, /<form class="composer"/, '左侧仍应保留文字输入表单');
  assert.match(sidebar, /id="send-button"/, '文字指令仍应可以发送');
});
