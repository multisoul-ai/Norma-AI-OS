import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const sandbox = {};
for (const id of ['assistant-motion', 'assistant-audio', 'assistant-voice']) {
  const source = html.match(new RegExp('<script id="' + id + '">([\\s\\S]*?)<\\/script>'))?.[1];
  if (source) runInNewContext(source, sandbox);
}

/// 场景：文字回应和排队状态不能误触发语音波动。
/// 数据构造：初始 generation=0、status=idle；创建第一条播报请求。
/// 执行过程：1. request 进入 pending。2. start 进入 playing。3. end 回到 idle。
/// 预期结果：排队不等于播放；只有 start 激活波动，end 必须停止。
test('波动由真实播报事件驱动', () => {
  const { transition } = sandbox.AssistantVoice;
  const pending = transition({ generation: 0, status: 'idle' }, 'request');
  assert.equal(pending.status, 'pending', '请求排队时应等待实际音频开始');
  assert.notEqual(pending.status, 'playing', '不能让文字回应冒充实际播放');
  const playing = transition(pending, 'start', pending.generation);
  assert.equal(playing.status, 'playing', '实际开始后必须进入播放状态');
  const ended = transition(playing, 'end', playing.generation);
  assert.equal(ended.status, 'idle', '播放结束必须停止波动目标');
  assert.notEqual(ended.status, 'playing', '不能在声音结束后保持波动');
});

/// 场景：语音暂停与恢复正确控制光场波动。
/// 数据构造：generation=3、status=playing，代表第三条当前播报。
/// 执行过程：1. pause。2. resume。3. error。
/// 预期结果：暂停不继续播放；恢复继续；错误回到 idle。
test('暂停恢复和错误收口播报状态', () => {
  const { transition } = sandbox.AssistantVoice;
  const paused = transition({ generation: 3, status: 'playing' }, 'pause', 3);
  assert.equal(paused.status, 'paused', '暂停应改变波动目标');
  assert.notEqual(paused.status, 'playing', '暂停不能继续保持完整波动');
  const resumed = transition(paused, 'resume', 3);
  assert.equal(resumed.status, 'playing', '恢复应继续波动');
  const failed = transition(resumed, 'error', 3);
  assert.equal(failed.status, 'idle', '错误必须清理播报状态');
  assert.notEqual(failed.status, 'pending', '错误不能让助手一直等待');
});

/// 场景：中断后的迟到回调不能重新激活或结束新一轮语音。
/// 数据构造：旧 generation=4；cancel 增为 5；新 request 增为 6。
/// 执行过程：1. 中断。2. 收到旧 start。3. 新请求开始。4. 收到旧 end。
/// 预期结果：中断保持 idle；旧 start 不激活；旧 end 不结束新语音。
test('取消和新播报隔离迟到事件', () => {
  const { transition } = sandbox.AssistantVoice;
  const cancelled = transition({ generation: 4, status: 'playing' }, 'cancel');
  assert.equal(cancelled.generation, 5, '取消必须让旧回调失效');
  const lateStart = transition(cancelled, 'start', 4);
  assert.equal(lateStart.status, 'idle', '迟到 start 不能恢复已取消语音');
  assert.notEqual(lateStart.status, 'playing', '取消后不能误激活波动');
  const pending = transition(cancelled, 'request');
  const playing = transition(pending, 'start', 6);
  const lateEnd = transition(playing, 'end', 4);
  assert.equal(lateEnd.status, 'playing', '旧语音结束不能停止新语音');
  assert.equal(lateEnd.generation, 6, '当前播报代次必须保留');
  assert.notEqual(lateEnd.status, 'idle', '新语音不能被旧事件重置');
});

const sound = { level: 0.8, pitchHz: 200, centroidHz: 900, low: 0.7, high: 0.05 };

/// 场景：真实声音开始和停止都保持幅度连续。
/// 数据构造：初始 level=0；音频 level=0.8；30 帧=0.5 s；退出响应 0.105 s。
/// 执行过程：1. 零时间输入声音。2. 推进首帧。3. 零时间停止。4. 推进 30 帧静音。
/// 预期结果：首帧幅度在 0..0.25；停止不瞬跳；0.5 s 后残留约 0.8*exp(-0.5/0.105)=0.0068。
test('真实音频幅度平滑进入和退出', () => {
  const { smooth, silence } = sandbox.AssistantAudio;
  const zero = smooth({ time: 2, level: 0 }, sound, 0);
  assert.equal(zero.level, 0, '声学目标不能瞬间放大光场');
  const first = smooth(zero, sound, 1 / 60);
  assert.ok(first.level > 0 && first.level < 0.25, '首帧应平滑响应实际声音');
  const stopped = smooth(first, silence(), 0);
  assert.equal(stopped.level, first.level, '中断必须继承当前幅度');
  assert.notEqual(stopped.time, 0, '中断不能重置形变相位');
  let settled = { time: 4, level: 0.8 };
  for (let i = 0; i < 30; i++) settled = smooth(settled, silence(), 1 / 60);
  assert.ok(settled.level < 0.01, '0.5 s 后波动应完成主要收敛');
  assert.ok(settled.level >= 0, '收敛不能产生负幅度');
});

/// 场景：恒定声学输入没有人为节拍；语音停顿时幅度自然下降。
/// 数据构造：相同 sound 输入 120 帧=2 s；再给 30 帧=0.5 s 静音。
/// 执行过程：1. 记录 1 s 与 2 s 的幅度。2. 输入静音，模拟播放中停顿。
/// 预期结果：恒定输入幅度差小于 0.001；停顿接近 0，而非继续按固定正弦起伏。
test('恒定输入不伪造节奏，播放停顿也会收回', () => {
  const { smooth, silence } = sandbox.AssistantAudio;
  let current = { time: 0, level: 0 }, first;
  for (let i = 0; i < 120; i++) { current = smooth(current, sound, 1 / 60); if (i === 59) first = current.level; }
  assert.ok(Math.abs(current.level - first) < 0.001, '恒定声音不能继续被人为节拍调制');
  assert.ok(current.level > 0.3 && current.level < 0.8, '实际声音的幅度映射应受控');
  for (let i = 0; i < 30; i++) current = smooth(current, silence(), 1 / 60);
  assert.ok(current.level < 0.01, '声音停顿必须降低幅度，即便播放尚未结束');
  assert.notEqual(current.level, first, '停顿不能维持上一段声音的波动');
});

/// 场景：减少动效保留静态声音反馈，后台恢复不补播。
/// 数据构造：time=3、level=0；后台 90 s；最大步长 0.05 s、相位速度上限 0.7+0.55+0.4=1.65。
/// 执行过程：1. reduced=true 输入 sound 与静音。2. 正常模式恢复后台。
/// 预期结果：减少动效时间固定，强度可变；后台相位最多推进 0.05*1.65=0.0825。
test('减少动效和后台恢复保持声音反馈', () => {
  const { smooth, silence } = sandbox.AssistantAudio;
  const reduced = smooth({ time: 3, level: 0 }, sound, 1 / 60, true);
  assert.equal(reduced.time, 3, '减少动效不能播放轮廓变化');
  assert.ok(reduced.level > 0.3 && reduced.level < 0.8, '减少动效仍应显示静态声音反馈');
  const stopped = smooth(reduced, silence(), 1 / 60, true);
  assert.equal(stopped.level, 0, '减少动效的静音状态应恢复亮度');
  assert.notEqual(stopped.time, 0, '静音不能重置相位');
  const resumed = smooth({ time: 3, level: 0 }, sound, 90);
  assert.ok(resumed.time > 3 && resumed.time <= 3.0825, '后台恢复不得补播不可见的运动');
  assert.notEqual(resumed.time, 93, '不能把后台时间灌入形变相位');
});
