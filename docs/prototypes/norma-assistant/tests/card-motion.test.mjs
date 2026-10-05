import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const source = html.match(/<script id="assistant-motion">([\s\S]*?)<\/script>/)?.[1];
assert.ok(source, '原型必须提供连续运动模块');
const sandbox = {};
runInNewContext(source, sandbox);
const { createCardMotion, advanceCardMotion, cardMotionSettled } = sandbox.AssistantMotion;

/// 场景：新卡片由轻微缩小状态连续展开，两侧的运动方向指向对话区。
/// 数据构造：左右侧 direction=1/-1；起始缩放 0.92；一帧=1/60 s。
/// 执行过程：1. 构造左右卡片。2. 零时间更新。3. 推进第一帧。
/// 预期结果：两侧方向相反；零时间不跳变；首帧缩放和亮度增加，未直接到终点。
test('卡片从两侧连续展开且不瞬间出现', () => {
  const left = createCardMotion(1), right = createCardMotion(-1);
  const zero = advanceCardMotion(left, true, 0);
  const first = advanceCardMotion(left, true, 1 / 60);
  assert.equal(left.scale.position, 0.92, '卡片应从可辨认的尺寸展开，不能从零出现');
  assert.ok(left.x.position > 0, '左侧卡片应从靠近助手的位置进入');
  assert.ok(right.x.position < 0, '右侧卡片应从靠近助手的位置进入');
  assert.equal(zero.scale.position, left.scale.position, '设置目标不能瞬间改变卡片尺寸');
  assert.equal(zero.opacity, 0, '零时间不能突然亮起卡片');
  assert.ok(first.scale.position > 0.92 && first.scale.position < 1, '第一帧应连续展开');
  assert.ok(first.opacity > 0 && first.opacity < 0.5, '第一帧应建立亮度而非直接完全出现');
});

/// 场景：卡片退场途中重新召回，保持位置、尺寸与速度。
/// 数据构造：已显示卡片退场 6×1/60=0.1 s；在当前位置反向进入。
/// 执行过程：1. 从可见状态推进退场。2. 零时间改变目标。3. 推进一帧。
/// 预期结果：反向不重置运动；尺寸先连续减速；亮度重新增加。
test('退场途中召回继承可见形体和速度', () => {
  let leaving = createCardMotion(1, true);
  for (let i = 0; i < 6; i++) leaving = advanceCardMotion(leaving, false, 1 / 60);
  const reversed = advanceCardMotion(leaving, true, 0);
  const first = advanceCardMotion(reversed, true, 1 / 60);
  assert.equal(reversed.scale.position, leaving.scale.position, '召回不能跳回起始尺寸');
  assert.equal(reversed.scale.velocity, leaving.scale.velocity, '召回不能清零已有速度');
  assert.equal(reversed.opacity, leaving.opacity, '召回不能重置透明度');
  assert.ok(first.scale.velocity > leaving.scale.velocity, '尺寸收缩应开始连续减速');
  assert.ok(first.opacity > leaving.opacity, '反向召回应立即恢复亮度');
  assert.notEqual(reversed.scale.position, 0.92, '召回不能重播完整入场');
});

/// 场景：进入和收起均逐渐收敛，结束后不保留不可见的活动卡片。
/// 数据构造：workspace k=225、c=30，ω=15；40×1/60≈0.667 s。
/// 执行过程：1. 推进进入 40 帧。2. 从显示态推进退场 40 帧。
/// 预期结果：进入接近 scale=1、opacity=1；退出接近起始缩放和透明；均完成收口。
test('卡片进出平滑收敛且能完成退场', () => {
  let entering = createCardMotion(-1), leaving = createCardMotion(-1, true);
  for (let i = 0; i < 40; i++) {
    entering = advanceCardMotion(entering, true, 1 / 60);
    leaving = advanceCardMotion(leaving, false, 1 / 60);
  }
  assert.ok(entering.scale.position > 0.999 && entering.scale.position <= 1, '入场应接近原尺寸且不弹过终点');
  assert.ok(entering.opacity > 0.999, '入场不能持续半透明影响阅读');
  assert.ok(leaving.opacity < 0.001, '退场应变得不可见');
  assert.ok(leaving.scale.position < 0.921 && leaving.scale.position >= 0.92, '退场应收回原来的缩放而不越界');
  assert.equal(cardMotionSettled(entering, true), true, '入场应停止无意义的尾帧');
  assert.equal(cardMotionSettled(leaving, false), true, '退场应允许删除实际元素');
  assert.notEqual(cardMotionSettled(createCardMotion(1), true), true, '初始隐藏态不能被误判为已进入');
});

/// 场景：减少动效与后台恢复不会播放大幅缩放或突然追赶。
/// 数据构造：后台间隔 90 s，单帧上限 0.05 s；reduced=true 使用目标静态形体。
/// 执行过程：1. 对比后台恢复与正常 0.05 s。2. 直接进入减少动效状态。
/// 预期结果：后台仅推进一小帧；减少动效采用原尺寸、零位移和完整亮度。
test('减少动效和后台恢复不产生运动跳变', () => {
  const initial = createCardMotion(1);
  const resumed = advanceCardMotion(initial, true, 90);
  const bounded = advanceCardMotion(initial, true, 0.05);
  const reduced = advanceCardMotion(initial, true, 1 / 60, true);
  assert.equal(resumed.scale.position, bounded.scale.position, '90 s 后恢复最多推进 0.05 s');
  assert.ok(resumed.scale.position < 1, '后台恢复不能瞬间追赶到终点');
  assert.equal(reduced.scale.position, 1, '减少动效应采用最终静态尺寸');
  assert.equal(reduced.x.position, 0, '减少动效不能保留横向位移');
  assert.equal(reduced.y.position, 0, '减少动效不能保留纵向位移');
  assert.equal(reduced.opacity, 1, '减少动效应保留清楚的可见反馈');
  assert.equal(reduced.scale.velocity, 0, '减少动效不能积累运动速度');
});
