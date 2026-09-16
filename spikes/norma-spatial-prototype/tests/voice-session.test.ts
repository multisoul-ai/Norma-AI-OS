import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createVoiceSession } from '../lib/voice-session.ts';
// 场景：连续语音指挥与系统回复交替。数据：假识别器和 TTS 回调；执行半句、定稿、播放、结束与打断；预期不会把半句发送，也不会一边识别一边播报。
test('continuous dialogue pauses recognition for speech and resumes after interruption', () => {
  const sent: string[] = [];
  let starts = 0,
    aborts = 0,
    cancels = 0;
  let complete = () => {};
  const recognition: any = {
    start() {
      starts++;
      this.onstart?.();
    },
    abort() {
      aborts++;
      this.onend?.();
    },
    stop() {
      this.onend?.();
    },
  };
  const voice = createVoiceSession({
    createRecognition: () => recognition,
    send: (t) => sent.push(t),
    notify: () => {},
    changed: () => {},
    speak: (_t, done) => {
      complete = done;
    },
    cancelSpeech: () => {
      cancels++;
    },
    schedule: (fn) => {
      return setTimeout(fn, 0);
    },
    cancelSchedule: (id) => clearTimeout(id as ReturnType<typeof setTimeout>),
  });
  voice.toggle();
  recognition.onresult({
    resultIndex: 0,
    results: [{ isFinal: false, 0: { transcript: '暂停' } }],
  });
  assert.equal(sent.length, 0, '中间识别不触发任务');
  assert.equal(voice.snapshot().transcript, '暂停', '中间识别实时可见');
  recognition.onresult({
    resultIndex: 0,
    results: [{ isFinal: true, 0: { transcript: '暂停 Codex' } }],
  });
  assert.deepEqual(sent, ['暂停 Codex'], '完整一句只发送一次');
  voice.speak('Codex 已暂停');
  assert.equal(voice.snapshot().speaking, true, '连续对话中仍可以播报回复');
  assert.ok(aborts > 0, '播报时停止 ASR，避免识别系统自己的声音');
  assert.equal(voice.snapshot().listening, false, '播放时不显示正在录音');
  complete();
  assert.equal(voice.snapshot().listening, true, '回复结束自动继续聆听');
  voice.speak('现在展示代码改动');
  const stale = complete;
  voice.toggle();
  assert.equal(voice.snapshot().speaking, false, '点击语音可以立即打断播报');
  assert.equal(voice.snapshot().listening, true, '打断后回到聆听');
  const count = starts;
  stale();
  assert.equal(starts, count, '过期的播放回调不会重复启动识别');
  voice.stop();
  assert.equal(voice.snapshot().listening, false, '显式结束关闭识别');
  assert.ok(cancels >= 2, '打断和结束都会清理 TTS');
});
// 场景：浏览器没有语音能力。执行开始；预期返回可显示在 VR 面板中的错误，不让用户困在无反馈状态。
test('unavailable microphone path produces in-scene feedback', () => {
  const voice = createVoiceSession({
    createRecognition: () => null,
    send: () => {},
    notify: () => {},
    changed: () => {},
    speak: () => {},
    cancelSpeech: () => {},
    schedule: () => 0,
    cancelSchedule: () => {},
  });
  voice.toggle();
  assert.ok(
    voice.snapshot().error.includes('演示指令'),
    '错误引导使用空间内的演示指令',
  );
  assert.equal(voice.snapshot().listening, false, '无识别能力时不能显示正在听');
  voice.stop();
});
