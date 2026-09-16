export type RecognitionResultEvent = {
  resultIndex: number;
  results: ArrayLike<{
    isFinal: boolean;
    [index: number]: { transcript: string };
  }>;
};
export type Recognition = {
  lang: string;
  continuous: boolean;
  interimResults: boolean;
  onresult: ((event: RecognitionResultEvent) => void) | null;
  onerror: ((event: { error: string }) => void) | null;
  onend: (() => void) | null;
  onstart: (() => void) | null;
  start(): void;
  stop(): void;
  abort(): void;
};
export type VoiceSnapshot = {
  listening: boolean;
  speaking: boolean;
  transcript: string;
  readAloud: boolean;
  error: string;
};
export const emptyVoice: VoiceSnapshot = {
  listening: false,
  speaking: false,
  transcript: '',
  readAloud: false,
  error: '',
};
type Ports = {
  createRecognition: () => Recognition | null;
  send: (text: string) => void;
  notify: (text: string) => void;
  changed: (value: VoiceSnapshot) => void;
  speak: (text: string, done: () => void) => void;
  cancelSpeech: () => void;
  schedule: (callback: () => void) => unknown;
  cancelSchedule: (id: unknown) => void;
};
export function createVoiceSession(ports: Ports) {
  let state = { ...emptyVoice },
    recognition: Recognition | null = null,
    wanted = false,
    outputting = false,
    epoch = 0,
    restart: unknown = null;
  const update = (change: Partial<VoiceSnapshot>) => {
    state = { ...state, ...change };
    ports.changed({ ...state });
  };
  const clearRestart = () => {
    if (restart !== null) ports.cancelSchedule(restart);
    restart = null;
  };
  const fail = (error: string) => {
    wanted = false;
    clearRestart();
    update({ listening: false, speaking: false, error });
    ports.notify(error);
  };
  const listen = () => {
    if (!wanted || outputting || !recognition) return;
    try {
      recognition.start();
    } catch {
      fail('语音未能连接。可以重试，或使用下方演示指令。');
    }
  };
  const cancelOutput = () => {
    epoch++;
    outputting = false;
    ports.cancelSpeech();
    update({ speaking: false });
  };
  const stop = () => {
    wanted = false;
    clearRestart();
    cancelOutput();
    recognition?.abort();
    update({ listening: false, transcript: '' });
  };
  function toggle() {
    if (outputting) {
      cancelOutput();
      if (wanted) listen();
      return;
    }
    if (wanted) {
      stop();
      return;
    }
    recognition = ports.createRecognition();
    if (!recognition) {
      fail('此浏览器尚不可用语音识别。可用下方演示指令体验连续指挥。');
      return;
    }
    const instance = recognition;
    instance.lang = 'zh-CN';
    instance.continuous = true;
    instance.interimResults = true;
    instance.onstart = () => {
      if (wanted && !outputting) update({ listening: true, error: '' });
    };
    instance.onresult = (event) => {
      if (!wanted || outputting) return;
      let interim = '';
      for (let i = event.resultIndex; i < event.results.length; i++) {
        const r = event.results[i];
        const text = r[0].transcript.trim();
        if (r.isFinal && text) {
          update({ transcript: text });
          ports.send(text);
        } else if (!r.isFinal) interim += text;
      }
      if (interim) update({ transcript: interim });
    };
    instance.onerror = (event) => {
      if (
        event.error === 'no-speech' ||
        event.error === 'aborted' ||
        outputting ||
        !wanted
      )
        return;
      fail(
        event.error === 'not-allowed'
          ? '麦克风未获授权。可以使用下方演示指令。'
          : '语音连接中断，可重试或使用下方演示指令。',
      );
    };
    instance.onend = () => {
      update({ listening: false });
      clearRestart();
      if (wanted && !outputting)
        restart = ports.schedule(() => {
          restart = null;
          listen();
        });
    };
    wanted = true;
    update({ error: '', readAloud: true });
    listen();
  }
  function speak(text: string) {
    if (!state.readAloud || !text) return;
    cancelOutput();
    const current = epoch;
    outputting = true;
    clearRestart();
    recognition?.abort();
    update({ speaking: true, listening: false });
    const done = () => {
      if (current !== epoch) return;
      outputting = false;
      update({ speaking: false });
      if (wanted) listen();
    };
    try {
      ports.speak(text, done);
    } catch {
      done();
    }
  }
  return {
    toggle,
    stop,
    speak,
    snapshot: () => ({ ...state }),
    setReadAloud(value: boolean) {
      update({ readAloud: value });
      if (!value && outputting) {
        cancelOutput();
        if (wanted) listen();
      }
    },
    dispose() {
      stop();
      if (recognition) {
        recognition.onstart = null;
        recognition.onend = null;
        recognition.onerror = null;
        recognition.onresult = null;
      }
    },
  };
}
