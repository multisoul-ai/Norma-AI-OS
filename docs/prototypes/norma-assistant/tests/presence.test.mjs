import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const source = html.match(/<script id="assistant-motion">([\s\S]*?)<\/script>/)?.[1];
const sandbox = {};
runInNewContext(source, sandbox);
const { advancePresence } = sandbox.AssistantMotion;
assert.equal(typeof advancePresence, 'function', '助手形变必须共享连续的状态时钟');

/// 场景：倾听开始时形变强度逐渐建立，不瞬间放大。
/// 数据构造：初始 energy=0.1；倾听目标 0.8；一帧=1/60 s；状态响应时间为 0.18 s。
/// 执行过程：1. 零时间切换到倾听。2. 推进一帧。3. 推进 0.6 s（36 帧）。
/// 预期结果：零时间不改变当前能量；第一帧小于 0.2；0.6 s 已接近目标且没有越界。
test('倾听形变从当前状态连续建立', () => {
  const initial = { time: 2, energy: 0.1 };
  const zero = advancePresence(initial, 'listening', 0);
  const first = advancePresence(initial, 'listening', 1 / 60);
  let settled = initial;
  for (let i = 0; i < 36; i++) settled = advancePresence(settled, 'listening', 1 / 60);
  assert.equal(zero.energy, 0.1, '零时间不能造成形变能量跳变');
  assert.ok(first.energy > 0.1, '第一帧应响应倾听状态');
  assert.ok(first.energy < 0.2, '第一帧不能直接跳到完整倾听强度');
  assert.ok(settled.energy > 0.77, '36/60=0.6 s 后应接近倾听目标');
  assert.ok(settled.energy < 0.8, '响应不能越过目标产生闪烁');
});

/// 场景：切换形态和回应状态时不重置形变时钟。
/// 数据构造：当前 time=4.25 s、energy=0.5；下一帧=1/60 s，改为 idle。
/// 执行过程：1. 接受新的状态目标。2. 继续当前相位和能量。
/// 预期结果：时钟推进一帧；能量渐减；不会清零或继续朝旧目标增大。
test('回应结束保留形变相位和当前能量', () => {
  const next = advancePresence({ time: 4.25, energy: 0.5 }, 'idle', 1 / 60);
  assert.ok(Math.abs(next.time - (4.25 + 1 / 60)) < 1e-10, '状态变化不能重启形变相位');
  assert.ok(next.energy < 0.5, '返回待命应减弱形变');
  assert.ok(next.energy > 0.4, '返回待命不能一帧内清零造成抽动');
  assert.notEqual(next.time, 0, '状态变化不能让形体突然回到循环起点');
});

/// 场景：减少动态效果时形体静止，但仍能反馈倾听状态。
/// 数据构造：当前 time=2 s、energy=0.1；推进 1/60 s，reduced=true。
/// 执行过程：1. 保持当前相位。2. 直接采用倾听状态强度。
/// 预期结果：时间保持 2；能量变为 0.8；不会播放形变运动。
test('减少动效保留状态反馈并固定相位', () => {
  const next = advancePresence({ time: 2, energy: 0.1 }, 'listening', 1 / 60, true);
  assert.equal(next.time, 2, '减少动效不能推进形变时钟');
  assert.equal(next.energy, 0.8, '减少动效仍应清楚反馈倾听状态');
  assert.notEqual(next.energy, 0.1, '减少动效不能同时丢弃状态反馈');
});

/// 场景：后台停顿后恢复不会跳过整个形变周期。
/// 数据构造：当前 time=3 s；后台停顿 90 s；单次步长上限为 0.05 s（20 Hz 的一帧）。
/// 执行过程：1. 接收 90 s 的过期帧间隔。2. 限制恢复后的单次相位推进。
/// 预期结果：时钟仅增加 0.05 s；不会变为 93 s 或倒退。
test('后台恢复保持可见形体连续', () => {
  const next = advancePresence({ time: 3, energy: 0.1 }, 'idle', 90);
  assert.equal(next.time, 3.05, '后台恢复最多推进 0.05 s，避免形体跳变');
  assert.notEqual(next.time, 93, '不能把后台停顿全部灌入形变时钟');
  assert.ok(next.time > 3, '恢复前台后时钟应继续推进');
});

/// 场景：透明玻璃切换为水银时连续交接，不瞬间替换整个材质。
/// 数据构造：玻璃权重 [0,1,0]，目标水银 index=2；一帧=1/60 s。
/// 执行过程：1. 零时间设目标。2. 推进一帧。3. 再推进 17 帧。
/// 预期结果：零时间保持玻璃；首帧混合；18/60=0.3 s 后水银超过 98%。
test('材质切换连续交接且在三百毫秒内收敛', () => {
  const { advanceAppearance } = sandbox.AssistantMotion;
  const zero = advanceAppearance([0, 1, 0], 2, 0);
  assert.equal(zero[1], 1, '设置目标不能立即移除玻璃');
  assert.equal(zero[2], 0, '零时间不能直接显示水银');
  let weights = advanceAppearance(zero, 2, 1 / 60);
  assert.ok(weights[1] > 0.5, '首帧应保留玻璃主要亮度');
  assert.ok(weights[2] > 0 && weights[2] < 0.5, '水银应连续进入');
  for (let i = 0; i < 17; i++) weights = advanceAppearance(weights, 2, 1 / 60);
  assert.ok(weights[2] > 0.98, '18/60=0.3 s 后水银应完成主要交接');
  assert.ok(weights[1] < 0.02, '旧玻璃不能长期叠在新材质上');
  assert.equal(weights.length, 3, '交接包含保留的极光和两种新材质');
  assert.equal(weights[0], 0, '新材质交接不能混入极光');
});

/// 场景：切换途中反向返回玻璃时继承当前混合。
/// 数据构造：当前权重 [0,0.6,0.4]，目标玻璃 index=1；一帧=1/60 s。
/// 执行过程：1. 零时间反向。2. 推进一帧。
/// 预期结果：起始权重不变；玻璃增加、水银减少；总权重保持 1。
test('快速反向切换保留当前材质混合', () => {
  const { advanceAppearance } = sandbox.AssistantMotion;
  const zero = advanceAppearance([0, 0.6, 0.4], 1, 0);
  assert.equal(zero[1], 0.6, '重定目标必须继承玻璃权重');
  assert.equal(zero[2], 0.4, '重定目标必须继承水银权重');
  assert.notEqual(zero[1], 1, '不能立即跳到玻璃终点');
  const next = advanceAppearance(zero, 1, 1 / 60);
  assert.ok(next[1] > 0.6 && next[1] < 1, '玻璃应从当前值继续进入');
  assert.ok(next[2] > 0 && next[2] < 0.4, '水银应从当前值继续淡出');
  assert.ok(Math.abs(next[1] + next[2] - 1) < 1e-10, '混合亮度预算必须保持 1');
});

/// 场景：减少动效时直接显示水银，不播放混合动画。
/// 数据构造：当前 [0,0.6,0.4]，目标 index=2，reduced=true。
/// 执行过程：接受新材质后立即采用目标权重。
/// 预期结果：水银为 1、玻璃为 0，没有移动过渡。
test('减少动效即时切换新材质', () => {
  const weights = sandbox.AssistantMotion.advanceAppearance([0, 0.6, 0.4], 2, 1 / 60, true);
  assert.equal(weights[2], 1, '减少动效时应直接显示水银');
  assert.equal(weights[1], 0, '减少动效时不能保留玻璃过渡');
  assert.notEqual(weights[1], 0.6, '旧混合不能在减少动效模式下继续播放');
});
