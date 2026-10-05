import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const source = html.match(/<script id="assistant-appearance">([\s\S]*?)<\/script>/)?.[1];
assert.ok(source, '原型必须提供可验证的助手形态选择模块');
const sandbox = {};
runInNewContext(source, sandbox);
const { normalize, read, write } = sandbox.AssistantAppearance;
const key = 'norma.assistant.appearance';

/// 场景：首次打开原型时使用保留的极光，未知方案不能进入渲染路径。
/// 数据构造：没有偏好值；另给旧方案名称 blue-orb。
/// 执行过程：1. 解析空值。2. 解析非法值。
/// 预期结果：两次均回退 aurora；不恢复旧光球。
test('默认使用保留的极光 且拒绝未知方案', () => {
  assert.equal(normalize(null), 'aurora', '首次访问应呈现保留的极光');
  assert.equal(normalize('blue-orb'), 'aurora', '过期方案必须安全回退');
  assert.notEqual(normalize('blue-orb'), 'blue-orb', '旧光球不能进入渲染路径');
});

/// 场景：两种新材质都能保存，且旧纹理球不再可恢复。
/// 数据构造：Map 模拟存储；依次选择 glass、mercury；旧值 prism、silk。
/// 执行过程：1. 保存并恢复水银。2. 切回玻璃。3. 读取旧偏好。
/// 预期结果：每次恢复当前选择；旧外观回退极光；极光保留在首位，同时提供两种新材质。
test('新材质准确恢复且旧纹理球安全迁移', () => {
  const data = new Map();
  const storage = { getItem: name => data.get(name), setItem: (name, value) => data.set(name, value) };
  write(storage, 'mercury');
  assert.equal(read(storage), 'mercury', '水银材质应准确恢复');
  assert.notEqual(read(storage), 'glass', '刷新不能把水银重置为默认');
  write(storage, 'glass');
  assert.equal(read(storage), 'glass', '切回玻璃后应恢复玻璃');
  assert.notEqual(read(storage), 'mercury', '旧水银偏好不能覆盖新选择');
  assert.equal(read({ getItem: () => 'plasma' }), 'aurora', '旧流光应回退保留的极光');
  assert.equal(read({ getItem: () => 'iridescence' }), 'aurora', '旧虹彩应回退保留的极光');
  assert.equal(read({ getItem: () => 'prism' }), 'aurora', '旧棱镜应回退保留的极光');
  assert.equal(read({ getItem: () => 'silk' }), 'aurora', '旧绸光应回退保留的极光');
  assert.equal(sandbox.AssistantAppearance.styles[0], 'aurora', '原来第一款极光必须保留在首位');
  assert.equal(sandbox.AssistantAppearance.styles.length, 3, '选择器应保留极光并提供两种新材质');
  assert.ok(!sandbox.AssistantAppearance.styles.includes('prism'), '旧棱镜不能参与渲染');
  assert.ok(!sandbox.AssistantAppearance.styles.includes('silk'), '旧绸光不能参与渲染');
});

/// 场景：移除星云与波纹后旧偏好安全回退，旧形态不再参与渲染。
/// 数据构造：旧星云标识 orbit、旧波纹标识 badge，分别模拟已保存的浏览器偏好。
/// 执行过程：1. 读取旧星云。2. 读取旧波纹。3. 尝试再次保存这两个旧标识。
/// 预期结果：四次均回退极光；不存在仍可进入渲染的旧标识。
test('已移除的星云与波纹偏好不能恢复', () => {
  assert.equal(read({ getItem: () => 'orbit' }), 'aurora', '旧星云偏好应回退保留的极光');
  assert.equal(read({ getItem: () => 'badge' }), 'aurora', '旧波纹偏好应回退保留的极光');
  assert.notEqual(write(null, 'orbit'), 'orbit', '旧星云不能再次进入渲染路径');
  assert.notEqual(write(null, 'badge'), 'badge', '旧波纹不能再次进入渲染路径');
});

/// 场景：损坏的偏好值不影响首页和全屏内容使用。
/// 数据构造：存储返回 deleted-style；用户又尝试保存相同非法值。
/// 执行过程：1. 读取过期值。2. 保存前规范化。
/// 预期结果：读取和保存均为 aurora；非法值不写入。
test('过期偏好回退为有效形态', () => {
  let saved;
  const storage = { getItem: () => 'deleted-style', setItem: (name, value) => { saved = [name, value]; } };
  assert.equal(read(storage), 'aurora', '过期存储不能导致助手消失');
  write(storage, 'deleted-style');
  assert.equal(saved[1], 'aurora', '保存前必须规范化形态');
  assert.notEqual(saved[1], 'deleted-style', '不能持久化无法渲染的形态');
});

/// 场景：浏览器禁止本地存储时仍能切换形态。
/// 数据构造：getItem/setItem 均抛出访问异常。
/// 执行过程：1. 读取时捕获异常。2. 选择 mercury 并尝试保存。
/// 预期结果：读取使用 aurora；写入返回 mercury，不丢失用户当次选择。
test('存储不可用时仍保留当次选择', () => {
  const blocked = { getItem: () => { throw new Error('blocked'); }, setItem: () => { throw new Error('blocked'); } };
  assert.equal(read(blocked), 'aurora', '禁止存储时应有可用的默认外观');
  assert.equal(write(blocked, 'mercury'), 'mercury', '持久化失败不能阻止本次切换');
  assert.notEqual(write(blocked, 'mercury'), 'aurora', '持久化失败不能把有效选择重置为默认');
});

/// 场景：形态选择只保存外观，不覆盖对话或卡片数据。
/// 数据构造：存储含 conversation=keep；选 mercury。
/// 执行过程：1. 写入方案偏好。2. 检查指定外观键与既有会话键。
/// 预期结果：外观键为 mercury；会话键保持 keep；无额外写入。
test('外观偏好不覆盖已有会话数据', () => {
  const data = new Map([['conversation', 'keep']]);
  const storage = { getItem: name => data.get(name), setItem: (name, value) => data.set(name, value) };
  write(storage, 'mercury');
  assert.equal(data.get(key), 'mercury', '必须使用专用外观偏好键');
  assert.equal(data.get('conversation'), 'keep', '切换形态不能修改会话数据');
  assert.equal(data.size, 2, '写入仅能新增一个外观偏好键');
  assert.notEqual(data.get('conversation'), 'mercury', '外观值不能污染已有会话键');
});
