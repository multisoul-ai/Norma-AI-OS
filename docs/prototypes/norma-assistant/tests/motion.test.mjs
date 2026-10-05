import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const source = html.match(/<script id="assistant-motion">([\s\S]*?)<\/script>/)?.[1];
assert.ok(source, '原型必须提供可独立验证的连续运动模块');
const sandbox = {};
runInNewContext(source, sandbox);
const { advanceSpring, damp } = sandbox.AssistantMotion;

/// 场景：圆球从静止出发，第一帧没有位置跳变，也不会瞬间获得最大速度。
/// 数据构造：初始位置 0 px、速度 0 px/s，目标 600 px；response=0.4 s 来自 Apple 移动弹簧。
/// 执行过程：1. 以零时间推进。2. 再推进 1/60 s（一帧）。
/// 预期结果：零时间保持原位置和速度；第一帧向目标加速，但不瞬移到目标。
test('弹簧从当前静止位置连续加速', () => {
  const initial = { position: 0, velocity: 0 };
  const zero = advanceSpring(initial, 600, 0);
  const first = advanceSpring(initial, 600, 1 / 60);
  assert.equal(zero.position, 0, '零时间不能改变圆球的可见位置');
  assert.equal(zero.velocity, 0, '零时间不能突然赋予速度');
  assert.ok(first.position > 0, '第一帧应开始朝目标移动');
  assert.ok(first.position < 30, '第一帧不应跨过超过目标距离 5%（600×0.05=30 px）');
  assert.ok(first.velocity > 0, '移动应从静止逐渐建立正向速度');
});

/// 场景：大距离停靠逐步减速收敛，不在终点弹跳。
/// 数据构造：目标距离 600 px；ω=2π/0.4；0.6 s 后残差=600×(1+ω×0.6)×exp(-ω×0.6)≈0.50 px。
/// 执行过程：1. 推进到 0.4 s。2. 从同一起点推进到 0.6 s。
/// 预期结果：0.4 s 已超过 98%；0.6 s 距离目标小于 1 px；始终不越过目标。
test('临界阻尼减速停靠且没有终点弹跳', () => {
  const initial = { position: 0, velocity: 0 };
  const near = advanceSpring(initial, 600, 0.4);
  const settled = advanceSpring(initial, 600, 0.6);
  assert.ok(near.position > 588, '0.4 s 应走完超过 98%（600×0.98=588 px），避免拖沓');
  assert.ok(settled.position > 599, '0.6 s 应进入最后 1 px 的收敛范围');
  assert.ok(settled.position < 600, '临界阻尼不能越过目标产生弹跳');
  assert.ok(settled.velocity < near.velocity, '临近终点必须持续减速');
});

/// 场景：停靠中点击返回，保留当下位置和速度，再平滑转向。
/// 数据构造：从 0 向 600 px 移动 0.12 s；反向目标改为 0 px；一帧=1/60 s。
/// 执行过程：1. 得到飞行中状态。2. 零时间重定向。3. 推进一帧。4. 推进至 0.6 s。
/// 预期结果：重定向不改变位置或截断速度；下一帧先减速；最后回到原点附近。
test('快速反向继承位置与速度', () => {
  const flying = advanceSpring({ position: 0, velocity: 0 }, 600, 0.12);
  const retargeted = advanceSpring(flying, 0, 0);
  const braking = advanceSpring(retargeted, 0, 1 / 60);
  const returned = advanceSpring(retargeted, 0, 0.6);
  assert.equal(retargeted.position, flying.position, '换目标时圆球不能跳到逻辑终点');
  assert.equal(retargeted.velocity, flying.velocity, '换目标时不能把飞行速度重置为零');
  assert.ok(braking.velocity < flying.velocity, '反向后应连续减速，而不是继续加速离开原点');
  assert.ok(braking.position > flying.position, '惯性必须先被消除，不能一帧内硬切方向');
  assert.ok(returned.position < 1, '反向 0.6 s 后应回到原点 1 px 内');
  assert.ok(returned.position > 0, '反向收敛不能越过原点反复弹跳');
});

/// 场景：不同刷新率下移动和能量光效具有相同节奏。
/// 数据构造：60 Hz×10 帧和 120 Hz×20 帧均为 1/6 s；能量平滑时间常数 0.2 s。
/// 执行过程：1. 分别按两种步长累计弹簧。2. 分别累计能量阻尼。
/// 预期结果：位置、速度、能量差均小于 1e-8；能量接近目标但不越界。
test('刷新率不改变移动与能量变化速度', () => {
  let low = { position: 0, velocity: 0 }, high = { position: 0, velocity: 0 };
  let lowEnergy = 0, highEnergy = 0;
  for (let frame = 0; frame < 10; frame++) {
    low = advanceSpring(low, 600, 1 / 60);
    lowEnergy = damp(lowEnergy, 1, 1 / 60, 0.2);
  }
  for (let frame = 0; frame < 20; frame++) {
    high = advanceSpring(high, 600, 1 / 120);
    highEnergy = damp(highEnergy, 1, 1 / 120, 0.2);
  }
  assert.ok(Math.abs(low.position - high.position) < 1e-8, '位置不能依赖显示器帧率');
  assert.ok(Math.abs(low.velocity - high.velocity) < 1e-8, '速度不能依赖显示器帧率');
  assert.ok(Math.abs(lowEnergy - highEnergy) < 1e-8, '光效能量不能依赖显示器帧率');
  assert.ok(lowEnergy > 0.5, '1/6 s 后应已建立超过一半的能量反馈');
  assert.ok(lowEnergy < 1, '能量不能越过目标造成突然闪烁');
});
