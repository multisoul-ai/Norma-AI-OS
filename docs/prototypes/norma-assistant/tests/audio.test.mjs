import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const sandbox = {};
const source = html.match(/<script id="assistant-audio">([\s\S]*?)<\/script>/)?.[1];
if (source) runInNewContext(source, sandbox);
// 12 kHz × 2048/12000 ≈ 171 ms；足以覆盖 100 Hz 声音的约 17 个周期。
const rate = 12000, size = 2048;
const tone = (hz, amplitude) => Float32Array.from({ length: size }, (_, i) => amplitude * Math.sin(2 * Math.PI * hz * i / rate));
const spectrum = hz => {
  const bins = new Float32Array(size / 2).fill(-120);
  // FFT 分辨率 = 12000/2048 ≈ 5.86 Hz；把唯一的有声峰放到最接近目标的位置。
  bins[Math.round(hz / (rate / size))] = -20;
  return bins;
};

/// 场景：动画幅度来自实际 PCM 音量，安静时不会按固定节奏抖动。
/// 数据构造：180 Hz 正弦峰值分别为 0.3 / 0.03，RMS=峰值/√2；另给全零 PCM。
/// 执行过程：1. 分析响声。2. 分析轻声。3. 分析静音。
/// 预期结果：响声 RMS 约 0.212；轻声约 0.0212；响声幅度大于轻声；静音幅度为零。
test('真实音量控制幅度，静音不生成波动', () => {
  const { analyze } = sandbox.AssistantAudio;
  const loud = analyze(tone(180, 0.3), spectrum(180), rate);
  const quiet = analyze(tone(180, 0.03), spectrum(180), rate);
  const silence = analyze(new Float32Array(size), new Float32Array(size / 2).fill(-120), rate);
  assert.ok(Math.abs(loud.rms - 0.3 / Math.SQRT2) < 0.002, '实际 RMS 应匹配 PCM 的能量');
  assert.ok(Math.abs(quiet.rms - 0.03 / Math.SQRT2) < 0.0002, '轻声 RMS 不能被归一化成响声');
  assert.ok(loud.level > quiet.level, '响声必须产生更大的波动目标');
  assert.equal(silence.level, 0, '静音不能生成固定周期起伏');
  assert.equal(silence.pitchHz, 0, '静音不能被识别为音高');
});

/// 场景：低音和高音产生不同且准确的音高控制值。
/// 数据构造：120 Hz 与 360 Hz 正弦，峰值均为 0.2，采样率 12 kHz。
/// 执行过程：1. 估计低音基频。2. 估计高音基频。
/// 预期结果：误差分别小于 3 Hz / 6 Hz；高音不错误折半到 180 Hz。
test('估计真实基频并区分音高', () => {
  const { analyze } = sandbox.AssistantAudio;
  const low = analyze(tone(120, 0.2), spectrum(120), rate);
  const high = analyze(tone(360, 0.2), spectrum(360), rate);
  assert.ok(Math.abs(low.pitchHz - 120) < 3, '低音估计应接近真实 120 Hz');
  assert.ok(Math.abs(high.pitchHz - 360) < 6, '高音估计应接近真实 360 Hz');
  assert.ok(high.pitchHz > low.pitchHz * 2.5, '音高控制必须区分低音与高音');
  assert.ok(Math.abs(high.pitchHz - 180) > 100, '不能把高音误判为半频');
});

/// 场景：频谱重心和高低频能量控制光场的不同层次。
/// 数据构造：同一 180 Hz PCM，频谱峰分别放在 180 Hz 和 3200 Hz。
/// 执行过程：1. 分析低频频谱。2. 分析高频频谱。
/// 预期结果：低频 low>0.95、high<0.01；高频 high>0.95、low<0.01，重心明显更高。
test('低高频能量分别控制边缘细节', () => {
  const { analyze } = sandbox.AssistantAudio;
  const low = analyze(tone(180, 0.2), spectrum(180), rate);
  const high = analyze(tone(180, 0.2), spectrum(3200), rate);
  assert.ok(low.low > 0.95 && low.high < 0.01, '低频峰不能被归入高频层');
  assert.ok(high.high > 0.95 && high.low < 0.01, '高频峰不能被归入低频层');
  assert.ok(high.centroidHz > 3000, '高频重心应接近真实 3200 Hz');
  assert.ok(low.centroidHz < 250, '低频重心不能变成宽带噪声');
});

/// 场景：噪声和直流偏置不能伪造稳定音高。
/// 数据构造：LCG 确定性噪声峰值 0.2；另给每个采样恒定为 0.2 的直流信号。
/// 执行过程：1. 分析非周期噪声。2. 去除直流均值后分析。
/// 预期结果：噪声仍有音量但音高为 0；直流 RMS 与幅度都为 0。
test('噪声不伪造音高，直流不伪造音量', () => {
  const { analyze } = sandbox.AssistantAudio;
  let seed = 42;
  const noise = Float32Array.from({ length: size }, () => { seed = (1664525 * seed + 1013904223) >>> 0; return (seed / 4294967296 - 0.5) * 0.4; });
  const random = analyze(noise, spectrum(3200), rate);
  const dc = analyze(new Float32Array(size).fill(0.2), spectrum(180), rate);
  assert.ok(random.rms > 0.05, '噪声的真实能量应该保留');
  assert.equal(random.pitchHz, 0, '非周期噪声不能锁定一个假音高');
  assert.equal(dc.rms, 0, '直流偏置必须去除');
  assert.equal(dc.level, 0, '直流不能触发光场抖动');
});

/// 场景：音高与频段在相同音量下仍能改变波动幅度，切换时保持连续。
/// 数据构造：level=0.7；分别设 pitch=120 Hz/high=0.1 与 pitch=360 Hz/high=0.7；每帧 1/60 s。
/// 执行过程：1. 零时间输入高音。2. 各自推进 60 帧=1 s。
/// 预期结果：零时间幅度不跳变；高音、高频幅度更大；两者都小于 0.7，避免过度震动。
test('音高和频段映射到受控的连续幅度', () => {
  const { smooth } = sandbox.AssistantAudio;
  const initial = { time: 2, level: 0, pitch: 0.35, bass: 0, treble: 0 };
  const low = { level: 0.7, pitchHz: 120, centroidHz: 300, low: 0.8, high: 0.1 };
  const high = { level: 0.7, pitchHz: 360, centroidHz: 2800, low: 0.1, high: 0.7 };
  const zero = smooth(initial, high, 0);
  assert.equal(zero.level, 0, '新音高不能瞬间放大光场');
  let a = initial, b = initial;
  for (let i = 0; i < 60; i++) { a = smooth(a, low, 1 / 60); b = smooth(b, high, 1 / 60); }
  assert.ok(b.level > a.level + 0.05, '真实高音与高频能量必须影响幅度');
  assert.ok(a.level > 0 && b.level < 0.7, '音频映射不能生成过大的震动');
  assert.ok(b.pitch > a.pitch, '平滑后的音高控制应保持高低关系');
});
