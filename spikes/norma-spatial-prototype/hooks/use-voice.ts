'use client';
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  createVoiceSession,
  emptyVoice,
  type Recognition,
} from '@/lib/voice-session';
type VoiceWindow = Window & {
  SpeechRecognition?: new () => Recognition;
  webkitSpeechRecognition?: new () => Recognition;
};
export function useVoice(
  send: (text: string) => void,
  notice: (message: string) => void,
) {
  const [state, setState] = useState({ ...emptyVoice });
  const instance = useRef<ReturnType<typeof createVoiceSession> | null>(null);
  const sendRef = useRef(send),
    noticeRef = useRef(notice);
  sendRef.current = send;
  noticeRef.current = notice;
  const get = useCallback(() => {
    if (!instance.current)
      instance.current = createVoiceSession({
        createRecognition: () => {
          const Constructor =
            (window as VoiceWindow).SpeechRecognition ??
            (window as VoiceWindow).webkitSpeechRecognition;
          return Constructor ? new Constructor() : null;
        },
        send: (text) => sendRef.current(text),
        notify: (text) => noticeRef.current(text),
        changed: setState,
        speak: (text, done) => {
          if (!window.speechSynthesis) {
            done();
            return;
          }
          const u = new SpeechSynthesisUtterance(text);
          u.lang = 'zh-CN';
          u.rate = 1.03;
          u.onend = done;
          u.onerror = done;
          window.speechSynthesis.speak(u);
        },
        cancelSpeech: () => window.speechSynthesis?.cancel(),
        schedule: (fn) => setTimeout(fn, 250),
        cancelSchedule: (id) =>
          clearTimeout(id as ReturnType<typeof setTimeout>),
      });
    return instance.current;
  }, []);
  const start = useCallback(() => get().toggle(), [get]);
  const stop = useCallback(() => instance.current?.stop(), []);
  const speak = useCallback((text: string) => get().speak(text), [get]);
  const setReadAloud = useCallback(
    (value: boolean) => get().setReadAloud(value),
    [get],
  );
  useEffect(
    () => () => {
      instance.current?.dispose();
      instance.current = null;
    },
    [],
  );
  return { ...state, start, stop, speak, setReadAloud };
}
